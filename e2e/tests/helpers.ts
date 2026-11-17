import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test";

/** Registers a fresh user in its own browser context and returns the signed-in page. */
export async function signUp(browser: Browser, label: string) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = `${label}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}@example.com`;
  await page.goto("/register");
  await page.getByLabel("Name").fill(label);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill("correct horse battery");
  await page.getByRole("button", { name: /create account/i }).click();
  await page.waitForURL(/\/(w|onboarding)/);
  return { context, page, email };
}

/** Adds `email` to the workspace through the API, as the signed-in owner. */
export async function addMember(
  context: BrowserContext,
  workspaceId: string,
  email: string,
  role: "EDITOR" | "VIEWER",
) {
  const response = await context.request.post(`/api/v1/workspaces/${workspaceId}/members`, {
    data: { email, role },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
}

/** A new user creates a workspace (through onboarding) and a document; returns both ids and the doc URL. */
export async function ownerWithDocument(browser: Browser, title: string) {
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

  await owner.page.getByRole("button", { name: "Create document" }).click();
  await owner.page.getByLabel("Title").fill(title);
  await owner.page.getByRole("dialog").getByRole("button", { name: "Create document" }).click();
  await owner.page.waitForURL(/\/docs\/[0-9a-f]{24}$/);
  return { owner, slug, workspaceId: workspaceId!, docUrl: owner.page.url() };
}

/** Opens a document and waits until its socket is up. */
export async function openDoc(page: Page, docUrl: string) {
  await page.goto(docUrl);
  await expect(page.getByLabel("paragraph block").first()).toBeVisible();
  await expect(page.getByText(/Active Sockets: \d/)).toBeVisible();
}
