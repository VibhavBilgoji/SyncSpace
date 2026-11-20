import { expect, test } from "@playwright/test";
import { signUp } from "./helpers";

test("the home page sends a visitor without a session to the login page", async ({
  page,
}) => {
  const response = await page.goto("/");
  await expect(page).toHaveURL(/\/login$/);
  expect(response?.status()).toBe(200);
  await expect(
    page.getByRole("button", { name: /sign in|log in/i }),
  ).toBeVisible();
});

test("the home page sends a signed-in user into the app", async ({
  browser,
}) => {
  const user = await signUp(browser, "visitor");
  await user.page.goto("/");
  await expect(user.page).toHaveURL(/\/(w|onboarding)(\/|$)/);
  await user.context.close();
});
