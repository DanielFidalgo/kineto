import { configDefaults, defineConfig } from "vitest/config";

// Keep vitest (Node-side unit tests, `npm test`) scoped to test/ — the
// Task 25 browser suite in test-browser/ uses Playwright's own `test`
// (see playwright.config.ts / `npm run test:browser`) and must not be
// picked up by vitest's default *.spec.ts glob.
export default defineConfig({
  test: {
    exclude: [...configDefaults.exclude, "test-browser/**"],
  },
});
