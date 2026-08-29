#!/usr/bin/env node
// Refuses to publish from anywhere but this repository's CI.
//
// Cargo takes its credential from an environment variable, so pointing it at
// the wrong account takes deliberate effort. npm does not: it reads
// `~/.npmrc`, which is global to the machine and shared by every project. A
// single `npm publish` in this directory would use whichever account happens
// to be logged in, and publishing to the wrong account cannot be undone --
// npm allows unpublishing only within 72 hours, and the name stays taken.
//
// So the safeguard cannot be "remember not to". This runs as `prepublishOnly`
// in every publishable package here and fails closed.
//
// Deliberately dependency-free and self-contained: the release build copies
// this file verbatim into each generated platform package, where no repo
// layout and no node_modules exist.

const isActions = process.env.GITHUB_ACTIONS === "true";

if (!isActions) {
  process.stderr.write(
    "\n" +
      "  Refusing to publish.\n\n" +
      "  Kineto publishes only from GitHub Actions, against a token scoped to\n" +
      "  this repository. Publishing from a workstation would use whatever\n" +
      "  account ~/.npmrc is logged into, and npm only allows unpublishing\n" +
      "  within 72 hours -- the name stays taken either way.\n\n" +
      "  To release: tag a version and let the release workflow run.\n" +
      "      just release <version> && git push && git push --tags\n\n",
  );
  process.exit(1);
}

// Belt and braces: a registry override in a config file would redirect the
// publish somewhere else entirely, and npm would report success.
const registry = process.env.npm_config_registry;
if (registry && !registry.startsWith("https://registry.npmjs.org")) {
  process.stderr.write(
    `\n  Refusing to publish: registry is ${registry}, not registry.npmjs.org.\n\n`,
  );
  process.exit(1);
}
