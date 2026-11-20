import { test, type Browser, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

// Takes the README screenshots from the live site: `SCREENSHOTS=1 pnpm --dir e2e exec playwright
// test -c live.config.ts screenshots`. Skipped otherwise. Read-only: it changes nothing.
test.skip(!process.env.SCREENSHOTS, "set SCREENSHOTS=1 to take screenshots");
const OUT = resolve(__dirname, "..", "..", "docs", "screenshots");
const PASSWORD = process.env.DEMO_PASSWORD ?? "";

async function enter(browser: Browser, email: string) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  await page.goto("/login");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL(/\/w(\/|$)/);
  await page.getByRole("link", { name: /Engineering Team/ }).first().click();
  await page.waitForURL(/\/w\/engineering-team\/docs/);
  return { context, page };
}
const shot = (page: Page, name: string) => page.screenshot({ path: resolve(OUT, name) });

test("README screenshots", async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const admin = await enter(browser, "vibhav@syncspace.demo");
  const editor = await enter(browser, "sparsh@syncspace.demo");
  await page6(admin.page);

  for (const s of [admin, editor]) {
    await s.page.getByRole("link", { name: /Product Requirements/ }).first().click();
  }
  for (const s of [admin, editor]) await s.page.getByText(/Active Sockets: [2-9]/).waitFor();
  await admin.page.waitForTimeout(1500);
  await shot(admin.page, "01-live-editing.png");

  await admin.page.getByRole("button", { name: "Version History" }).click();
  await admin.page.getByRole("listitem", { name: /^Version \d+$/ }).first().waitFor();
  await admin.page.waitForTimeout(500);
  await shot(admin.page, "02-version-history.png");

  await admin.page.getByRole("link", { name: "All boards" }).click();
  await admin.page.locator('a[href*="/boards/"]').filter({ hasText: "Sprint 12" }).first().click();
  await admin.page.getByRole("heading", { name: "Sprint 12" }).waitFor();
  await admin.page.setViewportSize({ width: 1700, height: 900 });
  await admin.page.getByText(/Connected/).waitFor();
  await admin.page.waitForTimeout(800);
  await shot(admin.page, "03-kanban-board.png");
  await admin.page.setViewportSize({ width: 1440, height: 900 });

  await admin.page.getByRole("link", { name: "Settings & Members" }).click();
  await admin.page.getByRole("heading", { name: "Members & access" }).waitFor();
  await shot(admin.page, "04-members.png");

  await admin.page.getByRole("link", { name: /Audit Trail/ }).click();
  await admin.page.getByRole("table").waitFor();
  await shot(admin.page, "05-audit-trail.png");

  await admin.context.close();
  await editor.context.close();
});

async function page6(page: Page) {
  await page.getByRole("link", { name: /Architecture Overview/ }).first().waitFor();
  await page.waitForTimeout(500);
  await shot(page, "06-documents.png");
}
