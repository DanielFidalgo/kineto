#!/usr/bin/env node
// Regenerates the playground examples that come from Kineto's own builders.
//
// `themed-scenes` is build_scenes output and `chart` is build_chart output, so
// what a visitor copies is what the tools actually emit. Driven through the
// MCP server over stdio rather than reimplemented here — a second
// implementation is a second thing to drift.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(REPO, "packages/playground/public/examples");
const MCP = process.env.KINETO_MCP ?? join(REPO, "target/release/kineto-mcp");

mkdirSync(OUT, { recursive: true });

function call(name, args) {
  const reqs = [
    { jsonrpc: "2.0", id: 1, method: "initialize", params: {
        protocolVersion: "2024-11-05", capabilities: {},
        clientInfo: { name: "playground-examples", version: "0" } } },
    { jsonrpc: "2.0", method: "notifications/initialized" },
    { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: args } },
  ];
  const out = execFileSync(MCP, {
    input: reqs.map((r) => JSON.stringify(r)).join("\n") + "\n",
    encoding: "utf8",
  });
  for (const line of out.split("\n").filter(Boolean)) {
    const msg = JSON.parse(line);
    if (msg.id !== 2) continue;
    if (msg.result?.isError) throw new Error(JSON.stringify(msg.result.content));
    return msg.result;
  }
  throw new Error(`no response from ${name}`);
}

call("build_scenes", {
  theme: "midnight", width: 960, height: 540,
  out: join(OUT, "themed-scenes.json"),
  scenes: [
    { kind: "title", text: "You write a document.", subtitle: "It compiles to a video." },
    { kind: "points", heading: "Try changing something", items: [
        "edit the JSON on the left",
        "the preview recompiles as you type",
        "then export a real MP4"] },
  ],
});

call("build_chart", {
  kind: "bar", labels: ["Mon", "Tue", "Wed", "Thu", "Fri"],
  series: [{ name: "renders", values: [12, 19, 15, 27, 31] }],
  title: "Charts are elements", subtitle: "paths, rects and text — editable",
  out: join(OUT, "chart.json"), width: 960, height: 540, seconds: 5,
});

process.stdout.write("regenerated themed-scenes.json and chart.json\n");
