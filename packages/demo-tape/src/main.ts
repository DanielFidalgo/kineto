// Task 25: the flagship demo (spec success criterion 1) — plain DOM, no
// framework. Wires the tape adapter (adapter.ts) and the SDK's mount()/
// render() (packages/sdk) into a page: load a tape (the bundled fixture,
// fetched off this same Vite dev server, or a real folder via
// <input webkitdirectory>), preview it live, export it to an MP4 blob
// with a progress bar, and reveal a download link.
import { build, loadEngine, mount, render, TIMEBASE } from "@kineto/sdk";
import type { Player, ZoeDocument } from "@kineto/sdk";
import { parseTape } from "./adapter";

// Stamped after a successful export so export.spec.ts can read the
// wall-clock realtime ratio (spec §6: measured, not asserted as a gate).
declare global {
  interface Window {
    __exportStats?: { videoSeconds: number; exportSeconds: number };
  }
}

const FIXTURE_DIR = "/fixtures/tape-fixture";
const FIXTURE_FILES = ["actions.jsonl", "step-01.jpg", "step-02.jpg", "step-03.jpg"];

function requireEl<T extends Element>(selector: string): T {
  const el = document.querySelector<T>(selector);
  if (el === null) {
    throw new Error(`demo: missing required element '${selector}'`);
  }
  return el;
}

const loadFixtureBtn = requireEl<HTMLButtonElement>("#load-fixture");
const tapeInput = requireEl<HTMLInputElement>("#tape-input");
const canvas = requireEl<HTMLCanvasElement>("#preview");
const exportBtn = requireEl<HTMLButtonElement>("#export");
const progressEl = requireEl<HTMLProgressElement>("#progress");
const downloadLink = requireEl<HTMLAnchorElement>("#download");
const capabilityError = requireEl<HTMLDivElement>("#capability-error");
const statusEl = requireEl<HTMLParagraphElement>("#status");

let currentDoc: ZoeDocument | undefined;
let currentAssets: Map<string, Uint8Array> | undefined;
let player: Player | undefined;

const webCodecsSupported = typeof VideoEncoder !== "undefined";

function showError(message: string): void {
  capabilityError.textContent = message;
  capabilityError.hidden = false;
}

function clearError(): void {
  capabilityError.hidden = true;
  capabilityError.textContent = "";
}

function resetDownload(): void {
  if (downloadLink.href) {
    URL.revokeObjectURL(downloadLink.href);
  }
  downloadLink.removeAttribute("href");
  downloadLink.hidden = true;
}

// Proactive capability check (in addition to catching render()'s own
// error below) so the demo tells the visitor up front, before they load
// anything, rather than only after they click Export.
if (!webCodecsSupported) {
  showError(
    "This browser has no WebCodecs (VideoEncoder) support, so MP4 export is unavailable here. Try a recent Chrome or Edge.",
  );
}

async function fetchFixtureFiles(): Promise<Map<string, Uint8Array>> {
  const files = new Map<string, Uint8Array>();
  for (const name of FIXTURE_FILES) {
    const res = await fetch(`${FIXTURE_DIR}/${name}`);
    if (!res.ok) {
      throw new Error(`demo: failed to fetch fixture file '${name}': ${res.status} ${res.statusText}`);
    }
    files.set(name, new Uint8Array(await res.arrayBuffer()));
  }
  return files;
}

async function filesFromFileList(fileList: FileList): Promise<Map<string, Uint8Array>> {
  const files = new Map<string, Uint8Array>();
  for (const file of Array.from(fileList)) {
    files.set(file.name, new Uint8Array(await file.arrayBuffer()));
  }
  return files;
}

async function loadTape(files: Map<string, Uint8Array>): Promise<void> {
  clearError();
  resetDownload();
  statusEl.textContent = "Loading tape…";

  const { doc, assets } = parseTape(files);
  currentDoc = doc;
  currentAssets = assets;

  player?.dispose();
  player = await mount(canvas, doc, { assets });

  exportBtn.disabled = !webCodecsSupported;
  statusEl.textContent = "Tape loaded — preview ready.";
}

loadFixtureBtn.addEventListener("click", () => {
  loadFixtureBtn.disabled = true;
  fetchFixtureFiles()
    .then(loadTape)
    .catch((err: unknown) => {
      showError(err instanceof Error ? err.message : String(err));
    })
    .finally(() => {
      loadFixtureBtn.disabled = false;
    });
});

tapeInput.addEventListener("change", () => {
  const fileList = tapeInput.files;
  if (fileList === null || fileList.length === 0) return;
  filesFromFileList(fileList)
    .then(loadTape)
    .catch((err: unknown) => {
      showError(err instanceof Error ? err.message : String(err));
    });
});

exportBtn.addEventListener("click", () => {
  if (currentDoc === undefined) {
    showError("Load a tape before exporting.");
    return;
  }

  clearError();
  resetDownload();
  exportBtn.disabled = true;
  progressEl.hidden = false;
  progressEl.value = 0;
  statusEl.textContent = "Exporting…";

  const doc = currentDoc;
  const assets = currentAssets ?? new Map<string, Uint8Array>();
  const fps = doc.defaultFps ?? 30;

  void (async () => {
    // A throwaway engine instance purely to read the compiled video
    // duration up front, so the realtime ratio (spec §6) can be computed
    // once the export finishes. render() loads its own engine internally
    // and this one is disposed immediately — the redundant load is a
    // deliberate simplicity trade-off for a demo page, not a hot path.
    const durationEngine = await loadEngine(build(doc), assets);
    const videoSeconds = durationEngine.durationTicks / TIMEBASE;
    durationEngine.dispose();

    const t0 = performance.now();
    const blob = await render(doc, {
      fps,
      assets,
      onProgress: (done, total) => {
        progressEl.value = total > 0 ? done / total : 0;
      },
    });
    const exportSeconds = (performance.now() - t0) / 1000;

    window.__exportStats = { videoSeconds, exportSeconds };

    const url = URL.createObjectURL(blob);
    downloadLink.href = url;
    downloadLink.hidden = false;
    statusEl.textContent = "Export complete.";
  })()
    .catch((err: unknown) => {
      showError(err instanceof Error ? err.message : String(err));
      statusEl.textContent = "Export failed.";
    })
    .finally(() => {
      exportBtn.disabled = !webCodecsSupported;
      progressEl.hidden = true;
    });
});
