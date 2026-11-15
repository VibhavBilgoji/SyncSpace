import { expect, test } from "@playwright/test";

test("home page loads", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle(/SyncSpace/);
  await expect(page.getByRole("heading", { name: "SyncSpace" })).toBeVisible();
});
