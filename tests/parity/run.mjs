// Byte-parity gate (Task 16, spec §1/§6), wasm half. Renders every
// `(corpus doc, tick)` pair through the wasm engine and diffs its sha256
// against `target/parity/native-hashes.json`, written by the native half
// (`cargo run -p zoetrope-core --bin dump-parity --features parity`).
//
// MUST be run from the repo root: every path below (the wasm-pack output
// under crates/wasm/pkg, target/parity/native-hashes.json, and the asset
// files under assets/ and testdata/assets/) is relative to `cwd`, not to
// this file's own location.
//
//   1. cargo run -p zoetrope-core --bin dump-parity --features parity
//   2. wasm-pack build crates/wasm --target web --release -- --features corpus
//   3. node tests/parity/run.mjs

import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import initWasm, {
  WasmEngine,
  corpus_names,
  corpus_doc_json,
  corpus_ticks,
  corpus_asset_srcs,
} from "../../crates/wasm/pkg/zoetrope_wasm.js";

await initWasm({
  module_or_path: await readFile(
    new URL("../../crates/wasm/pkg/zoetrope_wasm_bg.wasm", import.meta.url),
  ),
});

const native = JSON.parse(
  await readFile("target/parity/native-hashes.json", "utf8"),
);

// Reserved font srcs (crates/core/src/assets.rs::resolve_reserved_src)
// resolve to the same bundled font files native's `bundled-fonts` feature
// `include_bytes!`s; everything else is a corpus fixture under
// testdata/assets/.
const RESERVED = {
  "zoetrope:inter": "assets/fonts/Inter-Regular.ttf",
  "zoetrope:jetbrains-mono": "assets/fonts/JetBrainsMono-Regular.ttf",
};

let fail = 0;
let checked = 0;

for (const name of corpus_names()) {
  const docJson = corpus_doc_json(name);
  const eng = new WasmEngine(docJson);
  for (const pair of corpus_asset_srcs(docJson)) {
    const [id, src] = pair.split("\t");
    const path = RESERVED[src] ?? `testdata/assets/${src}`;
    eng.add_asset(id, await readFile(path));
  }
  eng.ready();

  for (const t of corpus_ticks(name)) {
    eng.render(t);
    const hash = createHash("sha256").update(eng.frame_copy()).digest("hex");
    const key = `${name}@${t}`;
    checked++;
    if (native[key] !== hash) {
      console.error(`PARITY FAIL ${key}: native=${native[key]} wasm=${hash}`);
      fail++;
    }
  }
}

console.log(`parity: ${checked - fail}/${checked} identical`);
process.exit(fail ? 1 : 0);
