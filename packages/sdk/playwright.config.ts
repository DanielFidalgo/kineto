import { defineConfig, devices } from "@playwright/test";

// Chromium-only, free-tier browser test for Task 19's render() (spec
// §4.3). The harness page (test-browser/harness.html) imports the SDK
// straight from source through Vite's dev server — see vite.config.ts for
// why server.fs.allow is widened to the repo root.
export default defineConfig({
  testDir: "./test-browser",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5199",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npx vite dev --port 5199 --strictPort",
    // Vite has no index.html at the served root (the harness lives at
    // test-browser/harness.html), so point the readiness check straight
    // at that page — a bare "/" 404s and Playwright treats 4xx as "not
    // ready yet" rather than "server is up".
    url: "http://localhost:5199/test-browser/harness.html",
    cwd: ".",
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
