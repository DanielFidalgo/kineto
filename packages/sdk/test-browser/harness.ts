// Exposes the SDK's public surface on `window.kineto` for
// render.spec.ts to drive via `page.evaluate`. This is the whole point of
// the harness: it's the only way a Playwright test (running in Node) can
// call into code executing inside the real browser's WebCodecs/Vite
// module-graph context.
import * as kineto from "../src/index";

declare global {
  interface Window {
    kineto: typeof kineto;
    __kinetoReady: boolean;
  }
}

window.kineto = kineto;
window.__kinetoReady = true;
