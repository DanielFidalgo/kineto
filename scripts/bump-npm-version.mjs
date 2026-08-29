#!/usr/bin/env node
// Sets @kineto/mcp's version and its platform pins to the release version.
//
// Called by `just release`. The wrapper pins each platform package exactly, so
// all five numbers move together or npm resolves a package that was never
// published. crates/mcp/tests/manifest.rs is what proves this ran.

import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!version) {
  process.stderr.write("usage: bump-npm-version.mjs <version>\n");
  process.exit(1);
}

const path = new URL("../packages/mcp/package.json", import.meta.url);
const pkg = JSON.parse(readFileSync(path, "utf8"));

pkg.version = version;
for (const name of Object.keys(pkg.optionalDependencies ?? {})) {
  // Only our own platform packages are pinned to the release version; a
  // third-party dependency added later must not be rewritten.
  if (name.startsWith("@kineto/")) pkg.optionalDependencies[name] = version;
}

writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
process.stdout.write(`@kineto/mcp -> ${version}\n`);
