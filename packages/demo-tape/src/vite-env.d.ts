// Vite's ambient types, for `import.meta.env.BASE_URL` in main.ts.
//
// Without this, `tsc --noEmit` fails on ImportMeta having no `env` — which
// `vite build` does not catch, because it never type-checks. The two disagree,
// and only CI runs the one that is strict.
/// <reference types="vite/client" />
