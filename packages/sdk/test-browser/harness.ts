// Exposes the SDK's public surface on `window.zoetrope` for
// render.spec.ts to drive via `page.evaluate`. This is the whole point of
// the harness: it's the only way a Playwright test (running in Node) can
// call into code executing inside the real browser's WebCodecs/Vite
// module-graph context.
import * as zoetrope from "../src/index";

declare global {
  interface Window {
    zoetrope: typeof zoetrope;
    __zoetropeReady: boolean;
  }
}

window.zoetrope = zoetrope;
window.__zoetropeReady = true;
