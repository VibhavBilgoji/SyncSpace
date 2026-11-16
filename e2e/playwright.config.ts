import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.E2E_BASE_URL ?? "http://localhost:3000";
const mongoUri = process.env.MONGODB_URI ?? "mongodb://localhost:27017";

export default defineConfig({
  testDir: "./tests",
  // Every test creates its own users and workspace, but one API process serves them all.
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
      // The real API on its own database. The two secrets are public test values (32 bytes of
      // 0x01 and 0x02), never used anywhere else.
      command: "cargo run -p server",
      cwd: "..",
      url: "http://127.0.0.1:8080/healthz",
      reuseExistingServer: true,
      timeout: 300_000,
      env: {
        PORT: "8080",
        MONGODB_URI: mongoUri,
        MONGODB_DB: "syncspace_e2e",
        JWT_SECRET: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
        TICKET_SECRET: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=",
        ALLOWED_ORIGINS: baseURL,
        COOKIE_SECURE: "false",
        SNAPSHOT_INTERVAL_SECS: "1",
        RUST_LOG: "warn",
      },
    },
    {
      // REST goes through the web server's rewrite (first-party cookie); the socket connects to
      // the API directly.
      command: "pnpm --dir ../apps/web dev",
      url: baseURL,
      reuseExistingServer: true,
      timeout: 120_000,
      env: {
        API_ORIGIN: "http://localhost:8080",
        NEXT_PUBLIC_WS_URL: "ws://localhost:8080",
      },
    },
  ],
});
