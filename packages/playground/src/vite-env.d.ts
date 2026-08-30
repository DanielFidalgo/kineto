// Vite's ambient types, for `import.meta.env.BASE_URL`.
//
// `vite build` never type-checks, so without this the build succeeds and
// `tsc --noEmit` fails. That combination once reached CI.
/// <reference types="vite/client" />
