#!/usr/bin/env node
// Generates a watchable page per release into the built site.
//
// GitHub serves release assets with `content-disposition: attachment`,
// `content-type: application/octet-stream` and `x-content-type-options:
// nosniff`, so a <video> pointing at one plays nothing and a release page can
// only ever offer a download. Pages serves the same bytes with real content
// types, so the video plays there.
//
// The videos are fetched from the Releases API at build time rather than
// committed: a few hundred KB per release adds up over a hundred releases, and
// a copy in git would be a second source of truth for a file that already has
// one.
//
// Site build tooling, not a Kineto capability — the same category as the vite
// config beside it, which is why it lives here rather than in the CLI.

import { mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const DIST = join(HERE, "..", "dist");
const REPO = process.env.GITHUB_REPOSITORY ?? "DanielFidalgo/kineto";
const API = `https://api.github.com/repos/${REPO}/releases`;

/** How many releases get a page. Older ones stay downloadable on GitHub. */
const KEEP = 6;

const headers = {
  accept: "application/vnd.github+json",
  "user-agent": "kineto-pages",
  ...(process.env.GITHUB_TOKEN ? { authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {}),
};

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
  );
}

const CSS = `
:root{--bg:#0b1116;--surface:#16232f;--line:#22323f;--fg:#f4f7f9;--muted:#8fa3b0;--ok:#4ecdc4}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif}
main{max-width:960px;margin:0 auto;padding:2.5rem 1.5rem 4rem}
h1{font-size:1.9rem;margin:0 0 .3rem;letter-spacing:-.01em}
p.sub{color:var(--muted);margin:0 0 1.6rem}
video{width:100%;border:1px solid var(--line);border-radius:12px;background:#000;display:block}
a{color:var(--ok)}
nav{margin:1.4rem 0 0;display:flex;gap:1rem;flex-wrap:wrap}
ul{list-style:none;padding:0;margin:1.2rem 0 0}
li{border-bottom:1px solid var(--line);padding:.7rem 0}
footer{color:var(--muted);font-size:.85rem;margin-top:2.5rem}
`.trim();

function page({ title, sub, body }) {
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(title)}</title>
<style>${CSS}</style>
</head><body><main>
<h1>${esc(title)}</h1>
<p class="sub">${esc(sub)}</p>
${body}
<footer>Composed from that tag's own commits and rendered by Kineto itself —
<a href="https://github.com/${esc(REPO)}">source</a>.</footer>
</main></body></html>
`;
}

async function main() {
  let releases = [];
  try {
    const res = await fetch(`${API}?per_page=${KEEP}`, { headers });
    if (!res.ok) throw new Error(`releases API: ${res.status}`);
    releases = await res.json();
  } catch (e) {
    // A rate limit or an offline build must not fail the deploy: the
    // playground is the point of the site, and these pages are an extra.
    process.stderr.write(`release pages skipped: ${e.message}\n`);
    return;
  }

  const made = [];
  for (const rel of releases) {
    const video = rel.assets?.find((a) => a.name.endsWith("-release.mp4"));
    if (!video) continue;
    const poster = rel.assets?.find((a) => a.name.endsWith("-poster.png"));

    const dir = join(DIST, "releases", rel.tag_name);
    mkdirSync(dir, { recursive: true });

    for (const asset of [video, poster].filter(Boolean)) {
      const res = await fetch(asset.browser_download_url, { headers, redirect: "follow" });
      if (!res.ok) throw new Error(`${asset.name}: ${res.status}`);
      writeFileSync(join(dir, asset.name), Buffer.from(await res.arrayBuffer()));
    }

    const posterAttr = poster ? ` poster="${esc(poster.name)}"` : "";
    writeFileSync(
      join(dir, "index.html"),
      page({
        title: `Kineto ${rel.tag_name}`,
        sub: "What changed in this release",
        body:
          `<video controls playsinline${posterAttr} src="${esc(video.name)}"></video>\n` +
          `<nav>` +
          `<a href="${esc(rel.html_url)}">Release notes and downloads</a>` +
          `<a href="../../">Try the playground</a>` +
          `</nav>`,
      }),
    );
    made.push({ tag: rel.tag_name, url: rel.html_url });
  }

  if (made.length > 0) {
    writeFileSync(
      join(DIST, "releases", "index.html"),
      page({
        title: "Kineto releases",
        sub: "Each video is composed from that tag's commits and rendered by the version being released.",
        body:
          "<ul>" +
          made
            .map((m) => `<li><a href="./${esc(m.tag)}/">${esc(m.tag)}</a></li>`)
            .join("") +
          "</ul>",
      }),
    );
  }
  process.stdout.write(`release pages: ${made.length}\n`);
}

await main();
