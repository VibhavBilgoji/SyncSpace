import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.E2E_BASE_URL ?? "http://localhost:3000";

export default defineConfig({
  testDir: "./tests",
  // The spike relay holds one shared document, so tests must not run side by side.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: { baseURL, trace: "on-first-retry" },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
  ],
  webServer: [
    {
      // THROWAWAY (task 0.3): the spike relay. Replaced by the real API in Phase 1.
      command: "cargo run -p server --bin spike_relay",
      cwd: "..",
      url: "http://127.0.0.1:8080/healthz",
      reuseExistingServer: true,
      timeout: 300_000,
    },
    {
      command: "pnpm --dir ../apps/web dev",
      url: baseURL,
      reuseExistingServer: true,
      timeout: 120_000,
    },
  ],
});
