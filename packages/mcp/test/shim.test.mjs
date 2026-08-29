// Tests for the npm wrapper.
//
// The shim sits between `npx @kineto/mcp` and a binary, on the stdio channel
// an MCP client speaks JSON-RPC over. Its failure modes are quiet ones: a
// platform that resolves to nothing, or a stray byte on stdout that corrupts
// the protocol for a client that never sees an error message.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { TARGETS, PACKAGES } from "../targets.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SHIM = join(HERE, "..", "bin", "kineto-mcp.mjs");

test("the platforms npm ships are exactly the ones the release builds", () => {
  // The invariant that can actually break. PACKAGES is derived from TARGETS,
  // so comparing those two proves nothing -- the real risk is TARGETS drifting
  // from the release matrix. Add a target there and npm silently never ships
  // it; a user on that platform gets "no prebuilt binary" for something the
  // project does build.
  const workflow = join(HERE, "..", "..", "..", ".github", "workflows", "release.yml");
  if (!existsSync(workflow)) {
    // Tests ship inside the published package, where the repo is not around.
    return;
  }
  const matrix = [...readFileSync(workflow, "utf8").matchAll(/^\s*- target: (\S+)$/gm)].map(
    (m) => m[1],
  );
  assert.ok(matrix.length > 0, "parsed no targets from release.yml");

  assert.deepEqual(
    [...TARGETS.map((t) => t.rust)].sort(),
    [...matrix].sort(),
    "targets.mjs and the release build matrix disagree",
  );
  assert.equal(Object.keys(PACKAGES).length, TARGETS.length);
});

test("a missing platform package fails loudly, and only on stderr", () => {
  // Run from the repo, where no @kineto/mcp-* package is installed -- the
  // same state a user reaches with `--omit=optional`.
  const r = spawnSync(process.execPath, [SHIM], { encoding: "utf8", input: "" });

  assert.notEqual(r.status, 0, "exited 0 without a binary to run");

  // The critical property. A client reads stdout as JSON-RPC; a diagnostic
  // written there is not a bad message, it is a corrupt stream.
  assert.equal(r.stdout, "", `wrote to stdout: ${JSON.stringify(r.stdout)}`);

  // Naming the package is what makes the failure actionable.
  assert.match(r.stderr, /@kineto\/mcp-/);
  assert.match(r.stderr, /optional/i);
});
