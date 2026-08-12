import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

async function render(pathname = "/") {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request(`http://localhost${pathname}`, { headers: { accept: "text/html" } }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );
}

test("server-renders the first notebook chapter from the Markdown snapshot", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<title>Before the First Token · Building EvoCC<\/title>/i);
  assert.match(html, /Building EvoCC/);
  assert.match(html, /Before the First Token/);
  assert.match(html, /evocc-compiler-map\.png/);
  assert.match(html, /The territory ahead/);
  assert.match(html, /The compiler we are building/);
  assert.match(html, /What counts as golden truth\?/);
  assert.match(html, /Challenges/);
  assert.match(html, /On this page/);
  assert.match(html, /aria-label="Architecture diagram"/);
  assert.match(html, /Drawing the system map/);
  assert.doesNotMatch(html, /<pre><figure/);
  assert.doesNotMatch(html, /codex-preview|react-loading-skeleton|Your site is taking shape/);
});

test("generated content is an exact projection of the chapter source", async () => {
  const [source, generated] = await Promise.all([
    readFile(new URL("../../docs/notebook/01-before-the-first-token.md", import.meta.url), "utf8"),
    readFile(new URL("../app/generated/notebook.ts", import.meta.url), "utf8"),
  ]);

  assert.match(source, /^---\n/);
  assert.match(source, /slug: before-the-first-token/);
  assert.match(generated, /"slug": "before-the-first-token"/);
  assert.match(generated, /"title": "Before the First Token"/);
  assert.match(generated, /A compiler that cannot identify its inputs cannot explain its output\./);
  assert.match(generated, /The artifacts left by Day 1/);
  assert.doesNotMatch(generated, /codex-preview|SkeletonPreview/);
});
