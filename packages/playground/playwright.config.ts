import { defineConfig, devices } from "@playwright/test";

// Chromium-only, matching the other packages: WebCodecs export is what is
// being proven, and Chromium is where it is available on a free runner.
// Port 5201 so this can run beside the sdk (5199) and demo-tape (5200).
export default defineConfig({
  testDir: "./test-browser",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5201",
    trace: "retain-on-failure",
  },
  // DEMO_PREVIEW runs against the built site rather than the dev server. They
  // are different artifacts: a base path or a public-directory file can be
  // right in dev and wrong in dist, and dist is what visitors meet.
  webServer: {
    command: process.env.DEMO_PREVIEW
      ? "npx vite build && npx vite preview --port 5201 --strictPort"
      : "npx vite dev --port 5201 --strictPort",
    url: "http://localhost:5201/index.html",
    cwd: ".",
    reuseExistingServer: !process.env.CI,
    timeout: 90_000,
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
