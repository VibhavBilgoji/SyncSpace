import { expect, test, type Page } from "@playwright/test";
import { addMember, ownerWorkspace, signUp } from "./helpers";

test.use({ viewport: { width: 1700, height: 900 } }); // all five columns on screen, so pointer targets are visible

// Drag and drop on the board (package D, last item): by pointer and by keyboard, with the arrow
// buttons and the status dropdown (covered in board.spec.ts) as the fallback.
const column = (page: Page, label: string) => page.getByRole("region", { name: label });
const cards = (page: Page, label: string) =>
  column(page, label).getByRole("button", { name: /^Open task / });
const titles = async (page: Page, label: string) =>
  (await cards(page, label).allTextContents()).map((t) => t.replace(/\s+/g, " "));

/** A real pointer drag: press on the card, move in steps (dnd-kit starts after a few pixels), release. */
async function pointerDrag(page: Page, from: ReturnType<Page["locator"]>, to: ReturnType<Page["locator"]>) {
  const a = (await from.boundingBox())!;
  const b = (await to.boundingBox())!;
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 12, a.y + a.height / 2 + 12, { steps: 4 });
  await page.mouse.move(b.x + b.width / 2, b.y + 24, { steps: 25 });
  await page.mouse.up();
  // dnd-kit animates the card into its place for about 250 ms and ignores a new drag until then.
  await page.waitForTimeout(400);
}

async function boardWithTasks(browser: import("@playwright/test").Browser) {
  const { owner, slug, workspaceId } = await ownerWorkspace(browser);
  const api = owner.context.request;
  const board = (await (
    await api.post(`/api/v1/workspaces/${workspaceId}/boards`, { data: { name: "Drag board" } })
  ).json()) as { id: string };
  for (const [title, status] of [
    ["Alpha", "TODO"],
    ["Beta", "TODO"],
    ["Gamma", "TODO"],
    ["Delta", "DONE"],
  ]) {
    const response = await api.post(`/api/v1/boards/${board.id}/tasks`, { data: { title, status } });
    expect(response.ok(), await response.text()).toBeTruthy();
  }
  const serverOrder = async (status: string) => {
    const list = (await (await api.get(`/api/v1/boards/${board.id}/tasks`)).json()) as Array<{
      title: string;
      status: string;
    }>;
    return list.filter((t) => t.status === status).map((t) => t.title);
  };
  return { owner, workspaceId, boardUrl: `/w/${slug}/boards/${board.id}`, serverOrder };
}

test("a card is dragged with the pointer to another column and to another place", async ({ browser }) => {
  const { owner, workspaceId, boardUrl, serverOrder } = await boardWithTasks(browser);
  const colleague = await signUp(browser, "colleague");
  await addMember(owner.context, workspaceId, colleague.email, "EDITOR");
  await owner.page.goto(boardUrl);
  await colleague.page.goto(boardUrl);
  await expect(cards(owner.page, "To do")).toHaveCount(3);
  await expect(cards(colleague.page, "To do")).toHaveCount(3);

  // Inside a column: Gamma is dropped on Alpha, so it goes above it.
  await pointerDrag(owner.page, owner.page.getByRole("button", { name: "Open task Gamma" }), owner.page.getByRole("button", { name: "Open task Alpha" }));
  await expect.poll(() => serverOrder("TODO")).toEqual(["Gamma", "Alpha", "Beta"]);
  const names = ["Gamma", "Alpha", "Beta"];
  await expect
    .poll(async () => (await titles(colleague.page, "To do")).map((t) => names.find((n) => t.includes(n))))
    .toEqual(names);

  // Into an empty column: Alpha is dropped on In progress.
  await pointerDrag(owner.page, owner.page.getByRole("button", { name: "Open task Alpha" }), owner.page.locator('[data-column-drop="IN_PROGRESS"]'));
  await expect(cards(owner.page, "In progress")).toHaveCount(1);
  await expect.poll(() => serverOrder("IN_PROGRESS")).toEqual(["Alpha"]);
  // Live in the other browser, without a reload.
  await expect(cards(colleague.page, "In progress")).toHaveCount(1);
  await expect(cards(colleague.page, "To do")).toHaveCount(2);

  // To another column, onto a card: Beta is dropped on Delta, so it goes above it.
  await pointerDrag(owner.page, owner.page.getByRole("button", { name: "Open task Beta" }), owner.page.getByRole("button", { name: "Open task Delta" }));
  await expect.poll(() => serverOrder("DONE")).toEqual(["Beta", "Delta"]);
  await expect(cards(colleague.page, "Done")).toHaveCount(2);

  // A plain click still opens the card (the drag needs a few pixels of movement).
  await owner.page.getByRole("button", { name: "Open task Beta" }).click();
  await expect(owner.page.getByRole("dialog").getByLabel("Title")).toHaveValue("Beta");
  await owner.page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();

  await owner.context.close();
  await colleague.context.close();
});

test("a card is dragged with the keyboard, and Escape puts it back", async ({ browser }) => {
  const { owner, boardUrl, serverOrder } = await boardWithTasks(browser);
  await owner.page.goto(boardUrl);
  await expect(cards(owner.page, "To do")).toHaveCount(3);
  const handle = owner.page.getByRole("button", { name: "Drag Alpha" });

  // Lift with space, move right with the arrow, drop with space: it leaves To do.
  await handle.focus();
  await owner.page.keyboard.press("Space");
  await owner.page.waitForTimeout(150);
  await owner.page.keyboard.press("ArrowRight");
  await owner.page.waitForTimeout(150);
  await owner.page.keyboard.press("Space");
  await owner.page.waitForTimeout(150);
  await expect(cards(owner.page, "To do")).toHaveCount(2);
  await expect.poll(async () => (await serverOrder("TODO")).includes("Alpha")).toBe(false);
  await expect(owner.page.getByRole("button", { name: "Open task Alpha" })).toBeVisible();

  // Escape cancels a lift: nothing changes, on the page or on the server.
  const before = await serverOrder("TODO");
  const beta = owner.page.getByRole("button", { name: "Drag Beta" });
  await beta.focus();
  await owner.page.keyboard.press("Space");
  await owner.page.waitForTimeout(150);
  await owner.page.keyboard.press("ArrowRight");
  await owner.page.waitForTimeout(150);
  await owner.page.keyboard.press("Escape");
  await owner.page.waitForTimeout(150);
  await expect(cards(owner.page, "To do")).toHaveCount(2);
  expect(await serverOrder("TODO")).toEqual(before);

  // Down moves a card below the next one in its column.
  await beta.focus();
  await owner.page.keyboard.press("Space");
  await owner.page.waitForTimeout(150);
  await owner.page.keyboard.press("ArrowDown");
  await owner.page.waitForTimeout(150);
  await owner.page.keyboard.press("Space");
  await owner.page.waitForTimeout(150);
  await expect.poll(() => serverOrder("TODO")).toEqual(["Gamma", "Beta"]);

  await owner.context.close();
});

test("a viewer has no way to drag, and the buttons stay the way to move a card for everyone else", async ({ browser }) => {
  const { owner, workspaceId, boardUrl } = await boardWithTasks(browser);
  const viewer = await signUp(browser, "viewer");
  await addMember(owner.context, workspaceId, viewer.email, "VIEWER");
  await viewer.page.goto(boardUrl);
  await expect(cards(viewer.page, "To do")).toHaveCount(3);
  await expect(viewer.page.getByRole("button", { name: /^Drag / })).toHaveCount(0);
  // Dragging with the pointer does nothing for a viewer.
  await pointerDrag(viewer.page, viewer.page.getByRole("button", { name: "Open task Alpha" }), viewer.page.locator("section[aria-label='Done']"));
  await expect(cards(viewer.page, "To do")).toHaveCount(3);

  await owner.page.goto(boardUrl);
  await owner.page.getByRole("button", { name: "Move Alpha to In progress" }).click();
  await expect(cards(owner.page, "In progress")).toHaveCount(1);
  await expect(cards(viewer.page, "In progress")).toHaveCount(1);

  await owner.context.close();
  await viewer.context.close();
});
