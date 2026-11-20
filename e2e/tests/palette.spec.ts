import { expect, test } from "@playwright/test";
import { ownerWithDocument } from "./helpers";

// FR18: the search field and Ctrl/Cmd + K open the command palette; typing filters documents and
// pages; Enter or a click jumps there.
test("search finds a document and a page, with the mouse field and with Ctrl+K", async ({
  browser,
}) => {
  const { owner, slug, docUrl } = await ownerWithDocument(browser, "Quarterly plan");
  const page = owner.page;

  // From another page, through the search field in the header.
  await page.getByRole("link", { name: "Settings & Members" }).click();
  await expect(page.getByRole("heading", { name: "Members & access" })).toBeVisible();
  await page.getByRole("button", { name: "Search or jump to" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  const input = page.getByRole("combobox");
  await expect(input).toBeFocused();
  await input.fill("quart");
  await expect(page.getByRole("option")).toHaveCount(1);
  await input.press("Enter");
  await expect(page).toHaveURL(docUrl);
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // The keyboard shortcut, a page of the workspace, then Esc closes without leaving.
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("combobox").fill("audit");
  await page.getByRole("option", { name: /Audit Trail/ }).click();
  await expect(page).toHaveURL(`/w/${slug}/audit`);
  await page.keyboard.press("Control+k");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page).toHaveURL(`/w/${slug}/audit`);
  await owner.context.close();
});
