import { expect, test } from "@playwright/test";
import { ownerWorkspace, signUp } from "./helpers";

// TESTING.md scenario 8 (members and audit), reduced for package D: the members table and a plain
// audit table with an action filter. An admin adds, promotes and removes a member; every change
// is in the audit trail; a non-admin sees the members read-only and no audit trail.
test("an admin manages members and reads the audit trail; a member cannot", async ({ browser }) => {
  const { owner, slug } = await ownerWorkspace(browser);
  const barbara = await signUp(browser, "Barbara");

  // Members screen from the sidebar: the owner is locked.
  await owner.page.getByRole("link", { name: "Settings & Members" }).click();
  await expect(owner.page.getByRole("heading", { name: "Members & access" })).toBeVisible();
  const ownerRow = owner.page.getByRole("row", { name: /ADMIN \(Owner\)/ });
  await expect(ownerRow).toBeVisible();
  await expect(ownerRow.getByRole("button")).toHaveCount(0);

  // An unknown email says so in words; a registered one is added as a Viewer.
  await owner.page.getByRole("button", { name: "+ Invite Member" }).click();
  let dialog = owner.page.getByRole("dialog");
  await dialog.getByLabel("Email").fill("nobody-here@example.com");
  await dialog.getByRole("button", { name: "Add member" }).click();
  await expect(dialog.getByText("No SyncSpace account uses this email")).toBeVisible();
  await dialog.getByLabel("Email").fill(barbara.email);
  await dialog.getByLabel("Role").selectOption("VIEWER");
  await dialog.getByRole("button", { name: "Add member" }).click();
  const barbaraRow = owner.page.getByRole("row", { name: /Barbara/ });
  await expect(barbaraRow).toBeVisible();
  await expect(barbaraRow.getByLabel("Role of Barbara")).toHaveValue("VIEWER");

  // Promote her to Editor: the dialog asks first.
  await barbaraRow.getByLabel("Role of Barbara").selectOption("EDITOR");
  dialog = owner.page.getByRole("dialog");
  await expect(dialog.getByText("Role changes apply to open sessions immediately.")).toBeVisible();
  await dialog.getByRole("button", { name: "Change role" }).click();
  await expect(barbaraRow.getByLabel("Role of Barbara")).toHaveValue("EDITOR");

  // As a member, Barbara sees the table without any controls and no audit trail.
  await barbara.page.goto(`/w/${slug}/settings/members`);
  await expect(barbara.page.getByRole("heading", { name: "Members & access" })).toBeVisible();
  await expect(barbara.page.getByRole("button", { name: "+ Invite Member" })).toHaveCount(0);
  await expect(barbara.page.getByRole("button", { name: /^Remove/ })).toHaveCount(0);
  await expect(barbara.page.getByRole("combobox")).toHaveCount(0);
  await expect(barbara.page.getByRole("link", { name: /Audit Trail/ })).toHaveCount(0);
  await barbara.page.goto(`/w/${slug}/audit`);
  await expect(barbara.page.getByText("Only admins can read the audit trail.")).toBeVisible();

  // The owner reads the trail: both changes are there, newest first, and the filter narrows it.
  await owner.page.getByRole("link", { name: /Audit Trail/ }).click();
  await expect(owner.page.getByRole("heading", { name: "System Audit Trail" })).toBeVisible();
  await expect(owner.page.getByText("IMMUTABLE APPEND-ONLY")).toBeVisible();
  const cells = owner.page.getByRole("cell", { name: /^(MEMBER_ADD|ROLE_CHANGE)$/ });
  await expect(cells).toHaveCount(2);
  await expect(cells.first()).toHaveText("ROLE_CHANGE");
  await expect(owner.page.getByText("from: VIEWER, to: EDITOR")).toBeVisible();
  await owner.page.getByLabel("Action").selectOption("MEMBER_ADD");
  await expect(owner.page.getByRole("cell", { name: "ROLE_CHANGE" })).toHaveCount(0);
  await expect(owner.page.getByRole("cell", { name: "MEMBER_ADD" })).toHaveCount(1);
  await expect(owner.page.getByText("1 event loaded")).toBeVisible();

  // Remove her: the dialog asks first, the row goes, and the trail has the removal.
  await owner.page.getByRole("link", { name: "Settings & Members" }).click();
  await owner.page.getByRole("button", { name: "Remove Barbara" }).click();
  await owner.page.getByRole("dialog").getByRole("button", { name: "Remove member" }).click();
  await expect(owner.page.getByRole("row", { name: /Barbara/ })).toHaveCount(0);
  await owner.page.getByRole("link", { name: /Audit Trail/ }).click();
  // The page keeps its state when you come back, so the earlier filter is still set.
  await expect(owner.page.getByLabel("Action")).toHaveValue("MEMBER_ADD");
  await owner.page.getByLabel("Action").selectOption({ label: "All actions" });
  await expect(owner.page.getByRole("cell", { name: "MEMBER_REMOVE" })).toHaveCount(1);
  await expect(owner.page.getByRole("cell", { name: /^(MEMBER_ADD|ROLE_CHANGE|MEMBER_REMOVE)$/ })).toHaveCount(3);

  // She no longer has access to the workspace.
  await barbara.page.goto(`/w/${slug}/settings/members`);
  await expect(barbara.page.getByRole("heading", { name: "Members & access" })).toHaveCount(0);

  await owner.context.close();
  await barbara.context.close();
});
