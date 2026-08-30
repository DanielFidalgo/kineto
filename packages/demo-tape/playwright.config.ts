import { defineConfig, devices } from "@playwright/test";

// Chromium-only, free-tier browser test for Task 25's flagship in-browser
// tape exporter demo (spec success criterion 1: a tape exports to a
// captioned, crossfaded MP4 in the browser, zero server involvement).
// Mirrors packages/sdk/playwright.config.ts on a different port (5200) so
// both packages' test:browser can run — including side by side in CI —
// without colliding on the sdk's 5199.
export default defineConfig({
  testDir: "./test-browser",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5200",
    trace: "retain-on-failure",
  },
  // DEMO_PREVIEW runs the suite against the *built* site rather than the dev
  // server. They are not the same artifact: a base path, an asset URL or a
  // public-directory file can be right in dev and wrong in dist, and the
  // deployed demo is the one users meet.
  webServer: {
    command: process.env.DEMO_PREVIEW
      ? "npx vite build && npx vite preview --port 5200 --strictPort"
      : "npx vite dev --port 5200 --strictPort",
    url: "http://localhost:5200/index.html",
    cwd: ".",
    reuseExistingServer: !process.env.CI,
    timeout: 90_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
