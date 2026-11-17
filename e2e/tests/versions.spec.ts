import { expect, test } from "@playwright/test";
import { addMember, openDoc, ownerWithDocument, signUp } from "./helpers";

// TESTING.md scenario 6: save a named version, edit, restore it; the other context sees the
// restored content live; the history shows the RESTORE card; restore that card to undo.
test("save a named version, restore it, and undo the restore", async ({ browser }) => {
  const { owner, workspaceId, docUrl } = await ownerWithDocument(browser, "Versioned notes");
  const editor = await signUp(browser, "editor");
  await addMember(owner.context, workspaceId, editor.email, "EDITOR");
  await openDoc(owner.page, docUrl);
  await openDoc(editor.page, docUrl);

  const ownerBlock = owner.page.getByLabel("paragraph block").first();
  const editorBlock = editor.page.getByLabel("paragraph block").first();

  // The first text, seen by both.
  await ownerBlock.click();
  await ownerBlock.pressSequentially("Version one text");
  await expect(editorBlock).toHaveValue("Version one text");

  // The owner opens the history (the editor does too, to watch it change) and saves a named version.
  await owner.page.getByRole("button", { name: "Version History" }).click();
  await editor.page.getByRole("button", { name: "Version History" }).click();
  await expect(owner.page.getByText("No versions yet")).toBeVisible();
  await owner.page.getByRole("button", { name: "Create Named Version" }).first().click();
  await owner.page.getByLabel("Label").fill("First draft");
  await owner.page.getByRole("button", { name: "Save version" }).click();
  const firstCard = owner.page.getByRole("listitem", { name: "Version 1" });
  await expect(firstCard.getByText("v1 · MANUAL")).toBeVisible();
  await expect(firstCard.getByText("First draft")).toBeVisible();
  // The other context's open panel gets the new card without reloading.
  await expect(editor.page.getByRole("listitem", { name: "Version 1" })).toBeVisible();

  // The text changes for everyone.
  await ownerBlock.fill("Version two text");
  await expect(editorBlock).toHaveValue("Version two text");

  // Preview shows the archive read-only; the live text is untouched.
  await firstCard.getByRole("button", { name: "Preview" }).click();
  await expect(owner.page.getByText("Viewing Version 1 · Read-only archive.")).toBeVisible();
  await expect(ownerBlock).toHaveValue("Version one text");
  await expect(ownerBlock).toHaveAttribute("readonly", "");
  await expect(editorBlock).toHaveValue("Version two text");

  // Restore it: the dialog says a copy is saved first, and the other context sees it live.
  await firstCard.getByRole("button", { name: "Restore" }).click();
  const dialog = owner.page.getByRole("dialog");
  await expect(dialog.getByText("Restore Version 1?")).toBeVisible();
  await expect(dialog.getByText(/A copy of the current content is saved first/)).toBeVisible();
  await dialog.getByRole("button", { name: "Confirm & Restore" }).click();
  await expect(owner.page.getByText("Restored Version 1")).toBeVisible();
  await expect(owner.page.getByText(/Read-only archive/)).toBeHidden();
  await expect(ownerBlock).toHaveValue("Version one text");
  await expect(editorBlock).toHaveValue("Version one text");
  await expect(editor.page.getByText("Version 1 restored")).toBeVisible();

  // The RESTORE card is first in both histories.
  const restoreCard = owner.page.getByRole("listitem", { name: "Version 2" });
  await expect(restoreCard.getByText("v2 · RESTORE")).toBeVisible();
  await expect(restoreCard.getByText("Restored from Version 1")).toBeVisible();
  await expect(editor.page.getByRole("listitem", { name: "Version 2" })).toBeVisible();

  // Undo: restoring the RESTORE card brings back the text from before the restore.
  await restoreCard.getByRole("button", { name: "Restore" }).click();
  await owner.page
    .getByRole("dialog")
    .getByRole("button", { name: "Confirm & Restore" })
    .click();
  await expect(ownerBlock).toHaveValue("Version two text");
  await expect(editorBlock).toHaveValue("Version two text");
  await expect(owner.page.getByRole("listitem", { name: "Version 3" }).getByText("v3 · RESTORE")).toBeVisible();

  await owner.context.close();
  await editor.context.close();
});
