import { expect, test, type Browser, type Page } from "@playwright/test";
import { signUp } from "./helpers";

// Sign in with GitHub (ADR-015) and the Repositories page, against a fake GitHub
// (tools/mock-github.mjs): the browser really goes to "GitHub", comes back to the callback on the
// web domain, and the API signs the person in. The fake chooses who signs in and which public
// repositories exist.
const GITHUB = "http://127.0.0.1:4010";

interface Scenario {
  id: number;
  login: string;
  name?: string | null;
  emails?: Array<{ email: string; primary: boolean; verified: boolean }>;
  repos?: unknown[];
  deny?: boolean;
}

async function pretendToBe(scenario: Scenario) {
  const response = await fetch(`${GITHUB}/__scenario`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(scenario),
  });
  expect(response.ok).toBeTruthy();
}

const unique = () => Date.now() * 1000 + Math.floor(Math.random() * 1000);

function repo(n: number, extra: Record<string, unknown> = {}) {
  return {
    id: n,
    name: `repo-${n}`,
    html_url: `https://github.com/someone/repo-${n}`,
    description: `Description of repo ${n}`,
    language: n % 2 ? "Rust" : "TypeScript",
    stargazers_count: n * 3,
    forks_count: n,
    updated_at: "2026-11-20T10:00:00Z",
    fork: false,
    archived: false,
    ...extra,
  };
}

/** The GitHub error banner (Next also has an invisible route announcer with the alert role). */
const githubError = (page: Page) =>
  page.getByRole("alert").filter({ hasText: "GitHub sign-in failed" });

async function me(page: Page) {
  const response = await page.context().request.get("/api/v1/auth/me");
  expect(response.ok()).toBeTruthy();
  return (await response.json()) as { id: string; email: string; name: string; github_login: string | null };
}

async function createWorkspace(page: Page) {
  await page.waitForURL(/\/onboarding/);
  const slug = `gh-${Date.now().toString(36)}`;
  await page.getByLabel("Workspace name").fill("GitHub Team");
  await page.getByLabel("Slug").fill(slug);
  await page.getByRole("button", { name: "Create workspace" }).click();
  await page.waitForURL(`**/w/${slug}/docs`);
  return slug;
}

async function freshVisitor(browser: Browser) {
  const context = await browser.newContext();
  return { context, page: await context.newPage() };
}

test("a new person signs in with GitHub and sees their public repositories", async ({
  browser,
}) => {
  const login = `octo-${Date.now().toString(36)}`;
  const email = `${login}@example.com`;
  await pretendToBe({
    id: unique(),
    login,
    name: "Octo Person",
    emails: [
      { email: "unverified@example.com", primary: false, verified: false },
      { email, primary: true, verified: true },
    ],
    repos: [
      repo(1),
      repo(2, { fork: true }),
      repo(3, { name: "special-tool", description: "Does one thing", archived: true }),
      repo(4, { html_url: "javascript:alert(1)", name: "unsafe-link" }),
    ],
  });

  const { context, page } = await freshVisitor(browser);
  await page.goto("/login");
  await page.getByRole("link", { name: "Continue with GitHub" }).click();

  // The new account has no workspace yet, so the app asks for one.
  await page.waitForURL(/\/onboarding/);
  const profile = await me(page);
  expect(profile).toMatchObject({ email, name: "Octo Person", github_login: login });
  const slug = await createWorkspace(page);

  await page.getByRole("link", { name: "Repositories" }).click();
  await expect(page).toHaveURL(`/w/${slug}/repos`);
  await expect(page.getByRole("heading", { name: "Repositories" })).toBeVisible();
  await expect(page.getByText(`@${login}`)).toBeVisible();
  const list = page.getByRole("list", { name: "Repositories" });
  // The entry with a `javascript:` link is not listed.
  await expect(list.getByRole("listitem")).toHaveCount(3);
  await expect(page.getByText("3 found")).toBeVisible();
  const first = list.getByRole("listitem").first();
  await expect(first.getByRole("link", { name: "repo-1" })).toHaveAttribute(
    "href",
    "https://github.com/someone/repo-1",
  );
  await expect(first.getByRole("link", { name: "repo-1" })).toHaveAttribute("target", "_blank");
  await expect(first).toContainText("Description of repo 1");
  await expect(first).toContainText("Rust");
  await expect(list.getByRole("listitem").nth(1)).toContainText("Fork");
  await expect(list.getByRole("listitem").nth(2)).toContainText("Archived");

  await page.getByLabel("Filter").fill("special");
  await expect(list.getByRole("listitem")).toHaveCount(1);
  await expect(list).toContainText("special-tool");
  await page.getByLabel("Filter").fill("nothing like this");
  await expect(page.getByText(/No repository matches/)).toBeVisible();

  // The search box finds the page too.
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox").fill("repos");
  await expect(page.getByRole("option", { name: "Repositories" })).toBeVisible();

  await context.close();
});

test("signing in with GitHub again is the same account, even after the email changed on GitHub", async ({
  browser,
}) => {
  const login = `again-${Date.now().toString(36)}`;
  const id = unique();
  await pretendToBe({
    id,
    login,
    emails: [{ email: `${login}@example.com`, primary: true, verified: true }],
  });
  const first = await freshVisitor(browser);
  await first.page.goto("/login");
  await first.page.getByRole("link", { name: "Continue with GitHub" }).click();
  await first.page.waitForURL(/\/onboarding/);
  const before = await me(first.page);

  await pretendToBe({
    id,
    login: `${login}-renamed`,
    emails: [{ email: `changed-${login}@example.com`, primary: true, verified: true }],
  });
  const second = await freshVisitor(browser);
  await second.page.goto("/login");
  await second.page.getByRole("link", { name: "Continue with GitHub" }).click();
  await second.page.waitForURL(/\/onboarding/);
  const after = await me(second.page);
  expect(after.id).toBe(before.id);
  expect(after.email).toBe(before.email);
  expect(after.github_login).toBe(`${login}-renamed`);
  await first.context.close();
  await second.context.close();
});

test("GitHub links to the existing account that has the same verified email", async ({
  browser,
}) => {
  const existing = await signUp(browser, "linker");
  const before = await me(existing.page);
  const login = `linked-${Date.now().toString(36)}`;
  await pretendToBe({
    id: unique(),
    login,
    emails: [{ email: existing.email.toUpperCase(), primary: true, verified: true }],
  });

  const { context, page } = await freshVisitor(browser);
  await page.goto("/login");
  await page.getByRole("link", { name: "Continue with GitHub" }).click();
  await page.waitForURL(/\/(w|onboarding)/);
  const after = await me(page);
  expect(after.id).toBe(before.id);
  expect(after.github_login).toBe(login);

  // And the password still works for that account.
  const password = await freshVisitor(browser);
  await password.page.goto("/login");
  await password.page.getByLabel("Email").fill(existing.email);
  await password.page.getByLabel("Password").fill("correct horse battery");
  await password.page.getByRole("button", { name: "Sign in" }).click();
  await password.page.waitForURL(/\/(w|onboarding)/);
  expect((await me(password.page)).id).toBe(before.id);
  for (const c of [existing.context, context, password.context]) await c.close();
});

test("a signed-in person connects GitHub from the Repositories page and comes back to it", async ({
  browser,
}) => {
  const person = await signUp(browser, "connector");
  const slug = await createWorkspace(person.page);
  const login = `connect-${Date.now().toString(36)}`;
  await pretendToBe({
    id: unique(),
    login,
    // A different address than the account's: linking while signed in does not need a match.
    emails: [{ email: `other-${login}@example.com`, primary: true, verified: true }],
    repos: [repo(1), repo(2)],
  });

  await person.page.goto(`/w/${slug}/repos`);
  await expect(person.page.getByText("Connect your GitHub account")).toBeVisible();
  await person.page.getByRole("link", { name: "Connect GitHub" }).click();

  await expect(person.page).toHaveURL(`/w/${slug}/repos`);
  await expect(person.page.getByText(`@${login}`)).toBeVisible();
  await expect(
    person.page.getByRole("list", { name: "Repositories" }).getByRole("listitem"),
  ).toHaveCount(2);
  const after = await me(person.page);
  expect(after.github_login).toBe(login);
  expect(after.email).toBe(person.email);
  await person.context.close();
});

test("declining on GitHub, or having no verified email, says so on the sign-in page", async ({
  browser,
}) => {
  await pretendToBe({ id: unique(), login: "decliner", deny: true });
  const declined = await freshVisitor(browser);
  await declined.page.goto("/login");
  await declined.page.getByRole("link", { name: "Continue with GitHub" }).click();
  await expect(declined.page).toHaveURL(/\/login\?github_error=denied/);
  await expect(githubError(declined.page)).toContainText("GitHub sign-in was cancelled.");
  // The password form is still there.
  await expect(declined.page.getByLabel("Email")).toBeVisible();
  const afterDecline = await declined.context.request.get("/api/v1/auth/me");
  expect(afterDecline.status()).toBe(401);

  await pretendToBe({
    id: unique(),
    login: "no-email",
    emails: [{ email: "unverified@example.com", primary: true, verified: false }],
  });
  const noEmail = await freshVisitor(browser);
  await noEmail.page.goto("/login");
  await noEmail.page.getByRole("link", { name: "Continue with GitHub" }).click();
  await expect(noEmail.page).toHaveURL(/github_error=no_email/);
  await expect(githubError(noEmail.page)).toContainText("no verified email address");
  expect((await noEmail.context.request.get("/api/v1/auth/me")).status()).toBe(401);
  await declined.context.close();
  await noEmail.context.close();
});

test("a callback that GitHub did not start is refused", async ({ browser }) => {
  const { context, page } = await freshVisitor(browser);
  await page.goto("/api/v1/auth/github/callback?code=forged&state=forged");
  await expect(page).toHaveURL(/\/login\?github_error=state/);
  await expect(githubError(page)).toContainText("expired or was not started");
  expect((await context.request.get("/api/v1/auth/me")).status()).toBe(401);
  await context.close();
});
