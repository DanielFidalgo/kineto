// The supported platforms, in one place.
//
// The shim maps a running process to a package name; the build script maps a
// rust target to that same package. Those two lists silently disagreeing is a
// real failure mode -- add a platform to the builder alone and npm publishes a
// package nothing resolves; add it to the shim alone and the shim points at a
// package that was never built. Both import this, and a test asserts the shim
// covers exactly these.

export const TARGETS = [
  { rust: "aarch64-apple-darwin", npm: "darwin-arm64", os: "darwin", cpu: "arm64" },
  { rust: "x86_64-apple-darwin", npm: "darwin-x64", os: "darwin", cpu: "x64" },
  { rust: "aarch64-unknown-linux-gnu", npm: "linux-arm64", os: "linux", cpu: "arm64" },
  { rust: "x86_64-unknown-linux-gnu", npm: "linux-x64", os: "linux", cpu: "x64" },
];

/// `process.platform process.arch` -> package name, as the shim needs it.
export const PACKAGES = Object.fromEntries(
  TARGETS.map((t) => [`${t.os} ${t.cpu}`, `@kineto/mcp-${t.npm}`]),
);
