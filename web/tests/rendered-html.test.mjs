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
  assert.match(html, /aria-label=""/);
  assert.match(html, /href="\/zh"/);
  assert.match(html, /data-sketch="egraph-equivalence"/);
  assert.match(html, /data-sketch="compiler-territory"/);
  assert.match(html, /data-sketch="semantic-and-evidence"/);
  assert.match(html, /data-sketch="three-arm64-worlds"/);
  assert.match(html, /data-sketch="nonce-bound-boot"/);
  assert.equal(html.match(/data-sketch=/g)?.length, 5);
  assert.match(html, /The nonce is a causal join/);
  assert.doesNotMatch(html, /mermaid|Drawing the system map/);
  assert.doesNotMatch(html, /<pre><figure/);
  assert.doesNotMatch(html, /codex-preview|react-loading-skeleton|Your site is taking shape/);
});

test("server-renders the complete localized Chinese chapter", async () => {
  const response = await render("/zh");
  assert.equal(response.status, 200);
  const html = await response.text();

  assert.match(html, /<title> Token · Building EvoCC<\/title>/i);
  assert.match(html, /lang="zh-CN"/);
  assert.match(html, / EvoCC /);
  assert.match(html, / golden truth/);
  assert.match(html, /, /);
  assert.match(html, //);
  assert.match(html, /aria-label="Switch to English"/);
  assert.match(html, /href="\/"/);
  assert.match(html, /data-sketch="nonce-bound-boot"/);
  assert.match(html, /nonce /);
  assert.match(html, /: PASS/);
  assert.doesNotMatch(html, /Drawing the system map|mermaid/);
});

test("generated bilingual content is an exact projection of both chapter sources", async () => {
  const [source, chineseSource, generated] = await Promise.all([
    readFile(new URL("../../docs/notebook/01-before-the-first-token.md", import.meta.url), "utf8"),
    readFile(new URL("../../docs/notebook/zh/01-before-the-first-token.md", import.meta.url), "utf8"),
    readFile(new URL("../app/generated/notebook.ts", import.meta.url), "utf8"),
  ]);

  assert.match(source, /^---\n/);
  assert.match(source, /slug: before-the-first-token/);
  assert.match(chineseSource, /title: Token /);
  assert.match(generated, /"slug": "before-the-first-token"/);
  assert.equal(generated.match(/"slug": "before-the-first-token"/g)?.length, 2);
  assert.match(generated, /"locale": "en"/);
  assert.match(generated, /"locale": "zh"/);
  assert.match(generated, /"title": "Before the First Token"/);
  assert.match(generated, /"title": " Token "/);
  assert.match(generated, /A compiler that cannot identify its inputs cannot explain its output\./);
  assert.match(generated, /The artifacts left by Day 1/);
  assert.doesNotMatch(generated, /codex-preview|SkeletonPreview/);
});
