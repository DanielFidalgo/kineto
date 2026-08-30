// The playground: edit a document, watch it compile, export an MP4.
//
// Everything here runs in the tab. `mount` drives the preview off the wasm
// engine; `render` encodes through WebCodecs. No server sees the document.

import { build, loadEngine, mount, render, TIMEBASE } from "@kineto/sdk";
import type { Player, ZoeDocument } from "@kineto/sdk";

const EXAMPLES = [
  { file: "hello.json", label: "Hello — start here" },
  { file: "motion.json", label: "Motion — entrances, paths, gradients" },
  { file: "themed-scenes.json", label: "Themed scenes — from build_scenes" },
  { file: "chart.json", label: "Chart — from build_chart" },
] as const;

// BASE_URL, not a leading slash: deployed under a project path a root-absolute
// fetch would miss.
const EXAMPLE_DIR = `${import.meta.env.BASE_URL}examples`;

// Fonts are deliberately not compiled into the wasm binary (size budget), so
// the JS host supplies their bytes. The SDK only does this automatically for
// an asset whose id is literally "default"; anything else — including the
// `inter` and `mono` ids these examples use — is ours to resolve.
//
// Pointed at the engine's own font files rather than a copy, so the browser
// renders with the same bytes the native build embeds.
const RESERVED: Record<string, URL> = {
  "kineto:inter": new URL(
    "../../../crates/core/assets/fonts/Inter-Regular.ttf",
    import.meta.url,
  ),
  "kineto:jetbrains-mono": new URL(
    "../../../crates/core/assets/fonts/JetBrainsMono-Regular.ttf",
    import.meta.url,
  ),
};

const fontCache = new Map<string, Uint8Array>();

async function fontBytes(src: string): Promise<Uint8Array | undefined> {
  const url = RESERVED[src];
  if (!url) return undefined;
  const cached = fontCache.get(src);
  if (cached) return cached;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`could not load the bundled font for ${src}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  fontCache.set(src, bytes);
  return bytes;
}

/** Bytes for every reserved font the document names, keyed by asset id. */
async function assetsFor(doc: ZoeDocument): Promise<Map<string, Uint8Array>> {
  const out = new Map<string, Uint8Array>();
  for (const [id, asset] of Object.entries(doc.assets ?? {})) {
    if (asset.type !== "font") continue;
    const bytes = await fontBytes(asset.src);
    if (bytes) out.set(id, bytes);
  }
  return out;
}

function el<T extends HTMLElement>(sel: string): T {
  const found = document.querySelector<T>(sel);
  if (!found) throw new Error(`missing ${sel}`);
  return found;
}

const presetSel = el<HTMLSelectElement>("#preset");
const editor = el<HTMLTextAreaElement>("#doc");
const canvas = el<HTMLCanvasElement>("#preview");
const scrub = el<HTMLInputElement>("#scrub");
const playBtn = el<HTMLButtonElement>("#play");
const exportBtn = el<HTMLButtonElement>("#export");
const download = el<HTMLAnchorElement>("#download");
const progress = el<HTMLProgressElement>("#progress");
const statusEl = el<HTMLParagraphElement>("#status");
const errorEl = el<HTMLParagraphElement>("#error");
const capability = el<HTMLDivElement>("#capability");
const tidyBtn = el<HTMLButtonElement>("#format");

let player: Player | undefined;
let currentDoc: ZoeDocument | undefined;
let durationTicks = 0;
let playing = false;

const webCodecs = typeof VideoEncoder !== "undefined";
if (!webCodecs) {
  capability.textContent =
    "This browser has no WebCodecs, so MP4 export is unavailable. The preview " +
    "still works. For export, try a recent Chrome or Edge — or run `cargo install kineto` locally.";
  capability.hidden = false;
  exportBtn.disabled = true;
}

function showError(message: string): void {
  errorEl.textContent = message;
  errorEl.hidden = false;
}
function clearError(): void {
  errorEl.hidden = true;
  errorEl.textContent = "";
}

/** Total document length, so the scrubber maps to real ticks. */
function totalTicks(doc: ZoeDocument): number {
  return doc.scenes.reduce((sum, s) => sum + s.duration, 0);
}

async function apply(source: string): Promise<void> {
  let doc: ZoeDocument;
  try {
    doc = JSON.parse(source) as ZoeDocument;
  } catch (e) {
    // A syntax error while typing is the normal case, not a failure — keep the
    // last good preview on screen rather than blanking it.
    showError(`JSON: ${(e as Error).message}`);
    return;
  }

  try {
    player?.dispose();
    player = undefined;
    playing = false;
    playBtn.textContent = "Play";

    // `build` canonicalises and validates; it throws with the same message the
    // CLI would print, which is the useful part.
    const assets = await assetsFor(doc);
    player = await mount(canvas, doc, { assets });
    currentDoc = doc;
    durationTicks = totalTicks(doc);
    canvas.width = doc.size.w;
    canvas.height = doc.size.h;
    scrub.value = "0";
    player.seek(0);
    clearError();
    statusEl.textContent = `${doc.scenes.length} scene${
      doc.scenes.length === 1 ? "" : "s"
    }, ${(durationTicks / TIMEBASE).toFixed(1)}s, ${doc.size.w}x${doc.size.h}`;
  } catch (e) {
    showError((e as Error).message);
  }
}

let debounce: number | undefined;
editor.addEventListener("input", () => {
  window.clearTimeout(debounce);
  // Long enough that a burst of typing recompiles once, short enough to feel
  // like the preview is following you.
  debounce = window.setTimeout(() => void apply(editor.value), 400);
});

scrub.addEventListener("input", () => {
  if (!player || durationTicks === 0) return;
  const frac = Number(scrub.value) / Number(scrub.max);
  player.seek(Math.round(frac * durationTicks));
});

playBtn.addEventListener("click", () => {
  if (!player) return;
  playing = !playing;
  playBtn.textContent = playing ? "Pause" : "Play";
  if (playing) player.play();
  else player.pause();
});

tidyBtn.addEventListener("click", () => {
  try {
    editor.value = JSON.stringify(JSON.parse(editor.value), null, 2);
    void apply(editor.value);
  } catch (e) {
    showError(`JSON: ${(e as Error).message}`);
  }
});

async function loadExample(file: string): Promise<void> {
  const res = await fetch(`${EXAMPLE_DIR}/${file}`);
  if (!res.ok) throw new Error(`could not load ${file}: ${res.status}`);
  editor.value = (await res.text()).trimEnd();
  await apply(editor.value);
}

exportBtn.addEventListener("click", async () => {
  if (!currentDoc) return;
  exportBtn.disabled = true;
  download.hidden = true;
  progress.hidden = false;
  progress.value = 0;
  statusEl.textContent = "Encoding…";
  const started = performance.now();

  try {
    const assets = await assetsFor(currentDoc);
    // Frame count comes from the engine rather than being recomputed here, so
    // the progress bar cannot disagree with what is actually encoded.
    const engine = await loadEngine(build(currentDoc), assets);
    const fps = currentDoc.defaultFps ?? 30;
    const blob = await render(currentDoc, {
      fps,
      assets,
      onProgress: (done, total) => {
        progress.value = total > 0 ? done / total : 0;
      },
    });
    engine.dispose();

    download.href = URL.createObjectURL(blob);
    download.hidden = false;
    const seconds = (performance.now() - started) / 1000;
    const videoSeconds = durationTicks / TIMEBASE;
    statusEl.textContent =
      `${(blob.size / 1024).toFixed(0)} KB · ` +
      `${videoSeconds.toFixed(1)}s of video in ${seconds.toFixed(1)}s ` +
      `(${(videoSeconds / seconds).toFixed(2)}x realtime)`;
  } catch (e) {
    showError(`Export failed: ${(e as Error).message}`);
    statusEl.textContent = "";
  } finally {
    progress.hidden = true;
    exportBtn.disabled = !webCodecs;
  }
});

for (const ex of EXAMPLES) {
  const opt = document.createElement("option");
  opt.value = ex.file;
  opt.textContent = ex.label;
  presetSel.append(opt);
}
presetSel.addEventListener("change", () => {
  void loadExample(presetSel.value);
});

void loadExample(EXAMPLES[0].file).catch((e: unknown) => {
  showError((e as Error).message);
});
