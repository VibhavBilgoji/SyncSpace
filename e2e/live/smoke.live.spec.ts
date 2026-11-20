import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";

// Smoke test of the deployed site with the seeded demo data (`cargo run -p server --bin seed`).
// It changes things only to put them back (text it typed, a task it moved), and it collects every
// browser console error: CORS and WebSocket errors fail the run.
const PASSWORD = process.env.DEMO_PASSWORD ?? "";
const USERS = {
  admin: { email: "vibhav@syncspace.demo", name: "Vibhav Bilgoji", role: "ADMIN" },
  editor: { email: "sparsh@syncspace.demo", name: "Sparsh Pai Bir", role: "EDITOR" },
  viewer: { email: "suyash@syncspace.demo", name: "Suyash Berde", role: "VIEWER" },
} as const;
type Who = keyof typeof USERS;

const consoleErrors: string[] = [];

function watch(page: Page, who: string) {
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    const text = message.text();
    // A 401 from /auth/me before login is the normal "not signed in" answer.
    if (/401/.test(text) && /auth\/me/.test(text)) return;
    consoleErrors.push(`[${who}] ${text}`);
  });
  page.on("pageerror", (error) => consoleErrors.push(`[${who}] pageerror: ${error.message}`));
}

async function signIn(browser: Browser, who: Who) {
  const context = await browser.newContext();
  const page = await context.newPage();
  watch(page, who);
  await page.goto("/login");
  await page.getByLabel("Email").fill(USERS[who].email);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL(/\/w(\/|$)/);
  return { context, page };
}

async function inWorkspace(browser: Browser, who: Who) {
  const session = await signIn(browser, who);
  await session.page.getByRole("link", { name: /Engineering Team/ }).first().click();
  await session.page.waitForURL(/\/w\/engineering-team\/docs/);
  return session;
}

async function openDocument(page: Page, title: string) {
  await page.getByRole("link", { name: new RegExp(title) }).first().click();
  await expect(page.getByText(/Active Sockets: \d/)).toBeVisible();
}

const column = (page: Page, label: string) => page.getByRole("region", { name: label });

test.beforeAll(() => {
  expect(PASSWORD, "DEMO_PASSWORD must be set (environment or .env.production.local)").not.toBe("");
});

test.afterEach(() => {
  const bad = consoleErrors.filter((line) => /cors|access-control|websocket|ws:\/\/|wss:\/\//i.test(line));
  expect(bad, `browser console errors:\n${bad.join("\n")}`).toEqual([]);
});

test.afterAll(() => {
  // Anything else the console said is reported, not failed: the demo must not be blocked by noise.
  if (consoleErrors.length > 0) console.log(`console errors seen (${consoleErrors.length}):\n${consoleErrors.join("\n")}`);
});

test("each seeded user signs in and sees the workspace with the right role", async ({ browser }) => {
  for (const who of ["admin", "editor", "viewer"] as const) {
    const { context, page } = await signIn(browser, who);
    const card = page.getByRole("link", { name: /Engineering Team/ }).first();
    await expect(card).toBeVisible();
    await expect(card).toContainText(USERS[who].role);
    await context.close();
  }
});

test("a wrong password is refused", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  watch(page, "anonymous");
  await page.goto("/login");
  await page.getByLabel("Email").fill(USERS.admin.email);
  await page.getByLabel("Password").fill("not the password");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/login/);
  await expect(page.getByRole("alert").first()).toBeVisible();
  await context.close();
});

test("live editing between two browsers, and the viewer cannot edit", async ({ browser }) => {
  const admin = await inWorkspace(browser, "admin");
  const editor = await inWorkspace(browser, "editor");
  const viewer = await inWorkspace(browser, "viewer");
  for (const s of [admin, editor, viewer]) await openDocument(s.page, "Product Requirements");

  const blockOf = (page: Page) => page.getByLabel("paragraph block").last();
  const original = await blockOf(admin.page).inputValue();
  const marker = ` live-${Date.now().toString(36)}`;

  // The admin types; the editor and the viewer see it without a reload.
  await blockOf(admin.page).click();
  await blockOf(admin.page).press("End");
  await blockOf(admin.page).pressSequentially(marker);
  await expect(blockOf(editor.page)).toHaveValue(original + marker);
  await expect(blockOf(viewer.page)).toHaveValue(original + marker);

  // The editor types back: the admin sees it.
  await blockOf(editor.page).click();
  await blockOf(editor.page).press("End");
  await blockOf(editor.page).pressSequentially("!");
  await expect(blockOf(admin.page)).toHaveValue(original + marker + "!");

  // The viewer's page is read-only, and forced typing never reaches anyone.
  await expect(viewer.page.getByText(/VIEWER · Read-only access/)).toBeVisible();
  await expect(blockOf(viewer.page)).toHaveAttribute("readonly", "");
  await blockOf(viewer.page).click({ force: true });
  await viewer.page.keyboard.type("forged");
  await expect(blockOf(admin.page)).toHaveValue(original + marker + "!");

  // The server saved it ("Saved to MongoDB" with a revision) and put it back for the next run.
  await expect(admin.page.getByText(/Revision: rev_[1-9]\d* · Saved to MongoDB/)).toBeVisible({ timeout: 30_000 });
  await blockOf(admin.page).fill(original);
  await expect(blockOf(editor.page)).toHaveValue(original);
  for (const s of [admin, editor, viewer]) await s.context.close();
});

test("save a version, edit, restore it: the other browser follows live", async ({ browser }) => {
  const admin = await inWorkspace(browser, "admin");
  const editor = await inWorkspace(browser, "editor");
  for (const s of [admin, editor]) await openDocument(s.page, "Sprint 12 Planning");
  const blockOf = (page: Page) => page.getByLabel("paragraph block").last();
  const original = await blockOf(admin.page).inputValue();

  await admin.page.getByRole("button", { name: "Version History" }).click();
  await editor.page.getByRole("button", { name: "Version History" }).click();
  await expect(admin.page.getByRole("listitem", { name: /^Version \d+$/ }).first()).toBeVisible();

  // Save a named version; the editor's open panel shows it without a reload.
  const label = `Smoke ${Date.now().toString(36)}`;
  await admin.page.getByRole("button", { name: "Create Named Version" }).click();
  await admin.page.getByRole("dialog").getByLabel("Label").fill(label);
  await admin.page.getByRole("dialog").getByRole("button", { name: "Save version" }).click();
  await expect(admin.page.getByText(label)).toBeVisible();
  await expect(editor.page.getByText(label)).toBeVisible();

  // Edit, then restore the version: the text goes back, for both.
  await blockOf(admin.page).click();
  await blockOf(admin.page).press("End");
  await blockOf(admin.page).pressSequentially(" EDITED");
  await expect(blockOf(editor.page)).toHaveValue(original + " EDITED");
  const card = admin.page.getByRole("listitem", { name: /^Version \d+$/ }).filter({ hasText: label });
  await card.getByRole("button", { name: "Restore" }).click();
  await admin.page.getByRole("dialog").getByRole("button", { name: "Confirm & Restore" }).click();
  await expect(blockOf(admin.page)).toHaveValue(original);
  await expect(blockOf(editor.page)).toHaveValue(original);
  await expect(admin.page.getByText(/· RESTORE/).first()).toBeVisible();
  for (const s of [admin, editor]) await s.context.close();
});

test("a Kanban move shows up in a second browser without a reload", async ({ browser }) => {
  const editor = await inWorkspace(browser, "editor");
  const admin = await inWorkspace(browser, "admin");
  for (const s of [editor, admin]) {
    await s.page.getByRole("link", { name: "All boards" }).click();
    await s.page.locator('a[href*="/boards/"]').filter({ hasText: "Sprint 12" }).first().click();
    await expect(s.page.getByRole("heading", { name: "Sprint 12" })).toBeVisible();
  }
  const card = (page: Page, label: string) =>
    column(page, label).getByRole("button", { name: "Open task Plan the load test" });
  await expect(card(editor.page, "Backlog")).toBeVisible();
  await expect(card(admin.page, "Backlog")).toBeVisible();

  await editor.page.getByRole("button", { name: "Move Plan the load test to To do" }).click();
  await expect(card(admin.page, "To do")).toBeVisible();
  await expect(card(admin.page, "Backlog")).toHaveCount(0);

  // Put it back.
  await editor.page.getByRole("button", { name: "Move Plan the load test to Backlog" }).click();
  await expect(card(admin.page, "Backlog")).toBeVisible();
  for (const s of [editor, admin]) await s.context.close();
});

test("the members screen shows the three people, and read-only for a non-admin", async ({ browser }) => {
  const admin = await inWorkspace(browser, "admin");
  await admin.page.getByRole("link", { name: "Settings & Members" }).click();
  await expect(admin.page.getByRole("heading", { name: "Members & access" })).toBeVisible();
  for (const who of ["admin", "editor", "viewer"] as const) {
    await expect(admin.page.getByRole("row", { name: new RegExp(USERS[who].name) })).toBeVisible();
  }
  await expect(admin.page.getByRole("row", { name: /ADMIN \(Owner\)/ })).toBeVisible();
  await expect(admin.page.getByRole("button", { name: "+ Invite Member" })).toBeVisible();

  const viewer = await inWorkspace(browser, "viewer");
  await viewer.page.getByRole("link", { name: "Settings & Members" }).click();
  await expect(viewer.page.getByRole("heading", { name: "Members & access" })).toBeVisible();
  await expect(viewer.page.getByRole("button", { name: "+ Invite Member" })).toHaveCount(0);
  await expect(viewer.page.getByRole("combobox")).toHaveCount(0);
  await admin.context.close();
  await viewer.context.close();
});

test("the audit trail is a table for the admin and refused for others", async ({ browser }) => {
  const admin = await inWorkspace(browser, "admin");
  await admin.page.getByRole("link", { name: /Audit Trail/ }).click();
  await expect(admin.page.getByRole("heading", { name: "System Audit Trail" })).toBeVisible();
  await expect(admin.page.getByText("IMMUTABLE APPEND-ONLY")).toBeVisible();
  await expect(admin.page.getByRole("table")).toBeVisible();
  expect(await admin.page.getByRole("row").count()).toBeGreaterThan(5);
  await expect(admin.page.getByRole("cell", { name: "VERSION_SAVE" }).first()).toBeVisible();
  await admin.page.getByLabel("Action").selectOption("VERSION_SAVE");
  await expect(admin.page.getByRole("cell", { name: "TASK_MOVE" })).toHaveCount(0);
  await expect(admin.page.getByRole("cell", { name: "VERSION_SAVE" }).first()).toBeVisible();

  const editor = await inWorkspace(browser, "editor");
  await expect(editor.page.getByRole("link", { name: /Audit Trail/ })).toHaveCount(0);
  await editor.page.goto("/w/engineering-team/audit");
  await expect(editor.page.getByText("Only admins can read the audit trail.")).toBeVisible();
  await admin.context.close();
  await editor.context.close();
});

test("the console had no CORS or WebSocket errors in any window", () => {
  const bad = consoleErrors.filter((line) => /cors|access-control|websocket|ws:\/\/|wss:\/\//i.test(line));
  expect(bad).toEqual([]);
});

export type { BrowserContext };
