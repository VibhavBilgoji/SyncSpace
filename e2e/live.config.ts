import { defineConfig, devices } from "@playwright/test";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

// The smoke test of the deployed site (`pnpm --dir e2e test:live`). It needs the demo password,
// which only ever comes from the environment: DEMO_PASSWORD, or a line DEMO_PASSWORD=... in the
// gitignored `.env.production.local` at the repository root. It is never written anywhere.
const envFile = resolve(__dirname, "..", ".env.production.local");
if (!process.env.DEMO_PASSWORD && existsSync(envFile)) {
  for (const line of readFileSync(envFile, "utf8").split(/\r?\n/)) {
    const match = /^DEMO_PASSWORD=(.*)$/.exec(line.trim());
    if (match) process.env.DEMO_PASSWORD = match[1]!.replace(/^["']|["']$/g, "");
  }
}

export default defineConfig({
  testDir: "./live",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // The API may be asleep on the free tier: allow a minute for the first request.
  timeout: 180_000,
  expect: { timeout: 30_000 },
  reporter: "list",
  use: {
    baseURL: process.env.LIVE_URL ?? "https://syncspace-amber.vercel.app",
    trace: "retain-on-failure",
    viewport: { width: 1700, height: 900 },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1700, height: 900 } } }],
});
