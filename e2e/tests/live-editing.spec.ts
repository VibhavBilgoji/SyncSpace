import { expect, test, type Page } from "@playwright/test";
import { addMember, signUp } from "./helpers";

test("two browsers edit one document live and a viewer cannot edit", async ({ browser }) => {
  const owner = await signUp(browser, "owner");
  await owner.page.waitForURL(/\/onboarding/);
  const slug = `e2e-${Date.now().toString(36)}`;
  await owner.page.getByLabel("Workspace name").fill("E2E Team");
  await owner.page.getByLabel("Slug").fill(slug);
  await owner.page.getByRole("button", { name: "Create workspace" }).click();
  await owner.page.waitForURL(`**/w/${slug}/docs`);

  const workspaces = await owner.context.request.get("/api/v1/workspaces");
  const workspaceId = ((await workspaces.json()) as Array<{ id: string; slug: string }>).find(
    (w) => w.slug === slug,
  )?.id;
  expect(workspaceId).toBeTruthy();

  const editor = await signUp(browser, "editor");
  const viewer = await signUp(browser, "viewer");
  await addMember(owner.context, workspaceId!, editor.email, "EDITOR");
  await addMember(owner.context, workspaceId!, viewer.email, "VIEWER");

  // The owner creates a document and lands in the editor.
  await owner.page.getByRole("button", { name: "Create document" }).click();
  await owner.page.getByLabel("Title").fill("Live notes");
  await owner.page.getByRole("dialog").getByRole("button", { name: "Create document" }).click();
  await owner.page.waitForURL(/\/docs\/[0-9a-f]{24}$/);
  const docUrl = owner.page.url();

  const openDoc = async (page: Page) => {
    await page.goto(docUrl);
    await expect(page.getByLabel("paragraph block").first()).toBeVisible();
    await expect(page.getByText(/Active Sockets: \d/)).toBeVisible();
  };
  await openDoc(editor.page);
  await openDoc(viewer.page);

  // The owner types; the editor and the viewer see it without reloading.
  const ownerBlock = owner.page.getByLabel("paragraph block").first();
  await ownerBlock.click();
  await ownerBlock.pressSequentially("Hello from the owner");
  await expect(editor.page.getByLabel("paragraph block").first()).toHaveValue(
    "Hello from the owner",
  );
  await expect(viewer.page.getByLabel("paragraph block").first()).toHaveValue(
    "Hello from the owner",
  );

  // The editor answers in a second block, made with Enter; the owner sees both lines.
  const editorBlock = editor.page.getByLabel("paragraph block").first();
  await editorBlock.click();
  await editorBlock.press("End");
  await editorBlock.press("Enter");
  await editor.page.getByLabel("paragraph block").nth(1).pressSequentially("and the editor");
  await expect(owner.page.getByLabel("paragraph block").nth(1)).toHaveValue("and the editor");

  // The slash menu changes a block's type for everyone.
  const second = editor.page.getByLabel("paragraph block").nth(1);
  await second.fill("");
  await second.pressSequentially("/head");
  await expect(editor.page.getByRole("listbox", { name: "Block types" })).toBeVisible();
  await second.press("Enter");
  await expect(owner.page.getByLabel("heading 1 block")).toBeVisible();

  // The viewer's page is read-only, and a forced edit never reaches anyone.
  await expect(viewer.page.getByText(/VIEWER · Read-only access/)).toBeVisible();
  await expect(viewer.page.getByLabel("paragraph block").first()).toHaveAttribute("readonly", "");
  await viewer.page.getByLabel("paragraph block").first().click({ force: true });
  await viewer.page.keyboard.type("forged");
  await expect(owner.page.getByLabel("paragraph block").first()).toHaveValue(
    "Hello from the owner",
  );

  // The server saved it: a fresh page load shows the same text, and the revision moved.
  await expect(owner.page.getByText(/Revision: rev_[1-9]/)).toBeVisible({ timeout: 10_000 });
  await openDoc(viewer.page);
  await expect(viewer.page.getByLabel("paragraph block").first()).toHaveValue(
    "Hello from the owner",
  );

  await owner.context.close();
  await editor.context.close();
  await viewer.context.close();
});
