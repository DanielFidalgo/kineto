#!/usr/bin/env node
// Locates the platform binary npm installed and hands control to it.
//
// `stdio: "inherit"` is not a detail here. Kineto's MCP server speaks JSON-RPC
// over stdin/stdout, so the child must receive the real file descriptors --
// piping through Node would put a buffering layer inside the protocol, and any
// stray write from this script would corrupt the stream. Nothing here may
// print to stdout, ever.
//
// spawnSync rather than spawn: it blocks until the child exits and forwards
// the descriptors directly, so there is no parent-side event loop to keep
// alive and no chance of the wrapper outliving the server.

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname } from "node:path";

// npm installs exactly one of these, chosen by the `os`/`cpu` fields the
// build script writes into each platform package.
import { PACKAGES } from "../targets.mjs";

const require = createRequire(import.meta.url);

const key = `${process.platform} ${process.arch}`;
const pkg = PACKAGES[key];

if (!pkg) {
  process.stderr.write(
    `kineto: no prebuilt binary for ${key}.\n` +
      `Supported: ${Object.keys(PACKAGES).join(", ")}.\n` +
      `Build from source instead: cargo install kineto\n`,
  );
  process.exit(1);
}

// Node resolves a module's realpath, so under any symlinked layout --
// `npm link`, npm workspaces, pnpm's default store -- `import.meta.url` points
// at the package's true location rather than the tree it was installed into,
// and resolution from there misses its own siblings. A registry install is not
// symlinked and hits the first branch; the fallbacks are what keep the linked
// cases working. argv[1] before cwd: under `npx` the cache directory holds the
// dependency, and the user's cwd is unrelated.
function resolveBinary(spec) {
  const bases = [null, dirname(process.argv[1] ?? "."), process.cwd()];
  for (const base of bases) {
    try {
      return base === null ? require.resolve(spec) : require.resolve(spec, { paths: [base] });
    } catch {
      // Try the next base; the caller reports failure once all are exhausted.
    }
  }
  return null;
}

const binary = resolveBinary(`${pkg}/bin/kineto-mcp`);
if (!binary) {
  // Optional dependencies fail silently by design, so this is a normal
  // outcome of --no-optional, a partially populated cache, or an install that
  // raced. Naming the package is what makes it fixable.
  process.stderr.write(
    `kineto: ${pkg} is not installed.\n` +
      `It is an optional dependency selected by platform; installing with\n` +
      `--no-optional or --omit=optional skips it. Try: npm install ${pkg}\n`,
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  process.stderr.write(`kineto: failed to run ${binary}: ${result.error.message}\n`);
  process.exit(1);
}

// A signalled child has a null status. Reporting 0 there would tell the caller
// the server exited cleanly when it was killed.
process.exit(result.status === null ? 1 : result.status);
