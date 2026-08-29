#!/usr/bin/env node
// Builds the four kineto-mcp-<platform>-<arch> packages from release archives.
//
// Deliberately repackages the *same* tarballs the release publishes rather
// than compiling again. A second build could differ from the one users
// download and verify by checksum, and nothing would catch it.
//
//   node build-platforms.mjs --version 0.1.1 --from dist --out dist/npm
//
// `--from` holds kineto-v<version>-<target>.tar.gz, as produced by the build
// matrix in .github/workflows/release.yml.

import { execFileSync } from "node:child_process";
import { mkdirSync, copyFileSync, writeFileSync, rmSync, existsSync, chmodSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, "..", "..");

import { TARGETS } from "./targets.mjs";

function arg(name, required = true) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1 || !process.argv[i + 1]) {
    if (!required) return undefined;
    throw new Error(`missing --${name}`);
  }
  return process.argv[i + 1];
}

const version = arg("version");
const from = arg("from");
const out = arg("out");

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

for (const t of TARGETS) {
  const stem = `kineto-v${version}-${t.rust}`;
  const archive = join(from, `${stem}.tar.gz`);
  if (!existsSync(archive)) {
    throw new Error(`missing archive ${archive} -- the build matrix did not produce ${t.rust}`);
  }

  const scratch = join(out, `.extract-${t.npm}`);
  mkdirSync(scratch, { recursive: true });
  execFileSync("tar", ["xzf", archive, "-C", scratch]);

  const name = `kineto-mcp-${t.npm}`;
  const dir = join(out, `mcp-${t.npm}`);
  mkdirSync(join(dir, "bin"), { recursive: true });

  const binary = join(scratch, stem, "kineto-mcp");
  if (!existsSync(binary)) throw new Error(`no kineto-mcp inside ${archive}`);
  copyFileSync(binary, join(dir, "bin", "kineto-mcp"));
  // tar preserves the mode, but copyFileSync does not guarantee it and an
  // unexecutable binary fails only at run time, long after publishing.
  chmodSync(join(dir, "bin", "kineto-mcp"), 0o755);

  for (const f of ["LICENSE-MIT", "LICENSE-APACHE"]) {
    copyFileSync(join(scratch, stem, f), join(dir, f));
  }
  // Self-contained: a generated package has no repo around it.
  copyFileSync(join(REPO, "scripts", "guard-publish.mjs"), join(dir, "guard-publish.mjs"));

  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify(
      {
        name,
        version,
        description: `Kineto MCP server binary for ${t.os} ${t.cpu}.`,
        license: "MIT OR Apache-2.0",
        // Provenance attests which repo and workflow built a package, and npm
        // requires `repository` to match the publishing workflow to issue it.
        repository: {
          type: "git",
          url: "git+https://github.com/DanielFidalgo/kineto.git",
          directory: "packages/mcp",
        },
        homepage: "https://github.com/DanielFidalgo/kineto",
        // npm reads these to decide whether to install this package at all.
        os: [t.os],
        cpu: [t.cpu],
        files: ["bin/kineto-mcp", "LICENSE-MIT", "LICENSE-APACHE"],
        scripts: { prepublishOnly: "node guard-publish.mjs" },
        publishConfig: { registry: "https://registry.npmjs.org/" },
      },
      null,
      2,
    ) + "\n",
  );

  writeFileSync(
    join(dir, "README.md"),
    `# ${name}\n\n` +
      `The \`kineto-mcp\` binary for ${t.os} ${t.cpu}, built from \`${t.rust}\`.\n\n` +
      `You do not install this directly. It is an optional dependency of\n` +
      `[\`kineto-mcp\`](https://www.npmjs.com/package/kineto-mcp), which npm\n` +
      `selects by platform.\n`,
  );

  rmSync(scratch, { recursive: true, force: true });
  process.stdout.write(`built ${name}\n`);
}
