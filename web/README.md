# Building EvoCC

The web edition of the EvoCC implementation notebook. It presents the material
in `../docs/notebook/` as a small, book-like site with chapter navigation,
reading progress, syntax-aware code blocks, and original hand-drawn SVG diagrams.

The paired English and Chinese Markdown notebooks are the source of truth. Do not edit
`app/generated/notebook.ts` by hand; it is a checked-in deployment snapshot
generated from the chapter frontmatter and body.

## Local development

```bash
npm install
npm run dev
```

Open <http://localhost:3000>. When port 3000 is occupied, vinext prints the
fallback port it selected.

## Verification

```bash
npm run sync:notebook
npm test
npm run lint
```

`sync:notebook` fails closed on missing metadata, duplicate slugs, or gaps in
chapter numbering. `npm test` rebuilds the site and checks that the rendered
page and generated snapshot still match the Markdown source.

## Adding a chapter

Add matching Markdown files to `../docs/notebook/` and `../docs/notebook/zh/`
with this frontmatter:

```yaml
---
title: Chapter title
chapter: 2
day: Day 2
slug: preprocessing-tokens
description: One sentence used for the chapter deck and page metadata.
---
```

Then run the verification commands above. Navigation and the on-page table of
contents are derived automatically. The build rejects missing translations or
any mismatch in chapter number and slug.

Chapter diagrams use a small set of named `evocc-sketch` blocks. Their
accessible, server-rendered drawings live in `app/components/SketchDiagram.tsx`;
the notebook never ships a generic diagramming runtime to readers.
