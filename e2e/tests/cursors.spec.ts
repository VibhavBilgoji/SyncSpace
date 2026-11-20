import { expect, test, type Locator, type Page } from "@playwright/test";
import { addMember, ownerWorkspace, signUp } from "./helpers";

// FR6: everyone in a document sees the others' carets and selections, live, with their names.
// The positions are checked in pixels, because that is the part a component test cannot see.
test("people see each other's carets, selections and names, and a viewer is seen too", async ({
  browser,
}) => {
  test.setTimeout(90_000);
  const { owner, workspaceId } = await ownerWorkspace(browser);
  const editor = await signUp(browser, "editor");
  const viewer = await signUp(browser, "viewer");
  await addMember(owner.context, workspaceId, editor.email, "EDITOR");
  await addMember(owner.context, workspaceId, viewer.email, "VIEWER");

  await owner.page.getByRole("button", { name: "Create document" }).click();
  await owner.page.getByLabel("Title").fill("Cursor notes");
  await owner.page.getByRole("dialog").getByRole("button", { name: "Create document" }).click();
  await owner.page.waitForURL(/\/docs\/[0-9a-f]{24}$/);
  const docUrl = owner.page.url();

  const text = "The engine uses Conflict-Free Replicated Data Types to ensure convergence.";
  const block = (page: Page) => page.getByLabel("paragraph block").first();
  const open = async (page: Page) => {
    await page.goto(docUrl);
    await expect(block(page)).toBeVisible();
    await expect(page.getByText(/Active Sockets: \d/)).toBeVisible();
  };
  await block(owner.page).click();
  await block(owner.page).pressSequentially(text);
  await owner.page.keyboard.press("Home");
  await open(editor.page);
  await open(viewer.page);
  await expect(block(editor.page)).toHaveValue(text);

  const cursorOf = (page: Page, name: string) => page.locator(`[data-remote-cursor="${name}"]`);
  const caretX = async (cursor: Locator) => {
    const box = await cursor.locator("[data-cursor-caret]").boundingBox();
    expect(box).not.toBeNull();
    return box!.x;
  };

  // At the start of the line the caret is at the left edge of the text, and the name is shown.
  await owner.page.keyboard.press("ArrowRight");
  await owner.page.keyboard.press("ArrowLeft");
  await expect(cursorOf(editor.page, "owner").locator("[data-cursor-caret]")).toBeVisible();
  await expect(cursorOf(editor.page, "owner")).toContainText("owner");
  await expect(cursorOf(viewer.page, "owner").locator("[data-cursor-caret]")).toBeVisible();
  const textLeft = (await block(editor.page).boundingBox())!.x;
  await expect.poll(async () => Math.abs((await caretX(cursorOf(editor.page, "owner"))) - textLeft)).toBeLessThan(4);

  // Moving the caret to the right moves the drawn caret with it, in steps of the text.
  const start = await caretX(cursorOf(editor.page, "owner"));
  for (let i = 0; i < 10; i += 1) await owner.page.keyboard.press("ArrowRight");
  await expect.poll(() => caretX(cursorOf(editor.page, "owner"))).toBeGreaterThan(start + 40);
  const ten = await caretX(cursorOf(editor.page, "owner"));
  for (let i = 0; i < 10; i += 1) await owner.page.keyboard.press("ArrowRight");
  await expect.poll(() => caretX(cursorOf(editor.page, "owner"))).toBeGreaterThan(ten + 40);

  // A selection is drawn as a highlight as wide as the selected text, and a caret at its end.
  await owner.page.keyboard.press("Home");
  await owner.page.keyboard.down("Shift");
  for (let i = 0; i < 15; i += 1) await owner.page.keyboard.press("ArrowRight");
  await owner.page.keyboard.up("Shift");
  const highlight = cursorOf(editor.page, "owner").locator("[data-cursor-selection]").first();
  await expect(highlight).toBeVisible();
  await expect
    .poll(async () => (await highlight.boundingBox())?.width ?? 0)
    .toBeGreaterThan(60);
  const wide = (await highlight.boundingBox())!;
  expect(wide.x).toBeGreaterThanOrEqual(textLeft - 2);
  expect(await caretX(cursorOf(editor.page, "owner"))).toBeGreaterThan(wide.x + wide.width - 4);

  // The editor and the viewer are seen by the owner, each with their own name.
  await block(editor.page).click({ position: { x: 150, y: 8 } });
  await block(viewer.page).click({ position: { x: 60, y: 8 } });
  await expect(cursorOf(owner.page, "editor").locator("[data-cursor-caret]")).toBeVisible();
  await expect(cursorOf(owner.page, "viewer").locator("[data-cursor-caret]")).toBeVisible();

  // A caret that moves because of someone else's typing stays on the same word.
  await block(owner.page).click();
  await owner.page.keyboard.press("Home");
  await owner.page.keyboard.type("Hey! ");
  await expect(block(editor.page)).toHaveValue("Hey! " + text);

  // Leaving the text field takes the caret away for everyone else.
  await owner.page.getByRole("heading", { level: 1 }).click();
  await expect(cursorOf(editor.page, "owner")).toHaveCount(0);
  await expect(cursorOf(viewer.page, "owner")).toHaveCount(0);

  for (const user of [owner, editor, viewer]) await user.context.close();
});
