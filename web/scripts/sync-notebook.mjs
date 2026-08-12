import { access, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const notebookDirectory = resolve(scriptDirectory, "../../docs/notebook");
const outputPath = resolve(scriptDirectory, "../app/generated/notebook.ts");
const localeSpecs = [
  { locale: "en", directory: notebookDirectory },
  { locale: "zh", directory: resolve(notebookDirectory, "zh") },
];

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

function parseFrontmatter(source, fileName) {
  const match = source.match(/^---\n([\s\S]*?)\n---\n+/);
  if (!match) throw new Error(`${fileName}: required frontmatter is missing`);

  const metadata = {};
  for (const line of match[1].split("\n")) {
    const separator = line.indexOf(":");
    if (separator === -1) {
      throw new Error(`${fileName}: malformed frontmatter line: ${line}`);
    }
    const key = line.slice(0, separator).trim();
    const value = line.slice(separator + 1).trim();
    if (!key || !value || key in metadata) {
      throw new Error(`${fileName}: invalid or duplicate frontmatter key: ${key}`);
    }
    metadata[key] = value;
  }

  const required = ["title", "chapter", "day", "slug", "description"];
  for (const key of required) {
    if (!metadata[key]) throw new Error(`${fileName}: missing ${key}`);
  }

  const chapter = Number(metadata.chapter);
  if (!Number.isSafeInteger(chapter) || chapter < 1) {
    throw new Error(`${fileName}: chapter must be a positive integer`);
  }
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(metadata.slug)) {
    throw new Error(`${fileName}: slug must be lowercase and hyphenated`);
  }

  return {
    metadata: { ...metadata, chapter },
    markdown: source.slice(match[0].length),
  };
}

function headingId(title) {
  return title
    .toLowerCase()
    .replace(/[`*_]/g, "")
    .replace(/&/g, " and ")
    .replace(/[^a-z0-9\u4e00-\u9fff]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function plainHeading(markdown) {
  return markdown
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[*_]/g, "")
    .trim();
}

function sectionsFor(markdown, fileName) {
  const ids = new Set();
  const sections = [];
  for (const match of markdown.matchAll(/^(##|###)\s+(.+)$/gm)) {
    const title = plainHeading(match[2]);
    const id = headingId(title);
    if (!id || ids.has(id)) {
      throw new Error(`${fileName}: empty or duplicate heading id: ${id}`);
    }
    ids.add(id);
    sections.push({ level: match[1].length, title, id });
  }
  return sections;
}

function stripDocumentTitle(markdown, expectedTitle, fileName) {
  const match = markdown.match(/^#\s+(?:\d+\.\s+)?(.+)\n+/);
  if (!match || plainHeading(match[1]) !== expectedTitle) {
    throw new Error(`${fileName}: H1 must match frontmatter title`);
  }
  return markdown.slice(match[0].length);
}

function measureBody(body, locale) {
  const prose = body.replace(/```[\s\S]*?```/g, " ");
  if (locale === "zh") {
    const hanCount = prose.match(/\p{Script=Han}/gu)?.length ?? 0;
    const latinCount = prose.replace(/\p{Script=Han}/gu, " ").match(/[\p{L}\p{N}_]+/gu)?.length ?? 0;
    return {
      wordCount: hanCount + latinCount,
      readingMinutes: Math.max(1, Math.ceil(hanCount / 400 + latinCount / 220)),
    };
  }

  const wordCount = prose
    .replace(/[^\p{L}\p{N}_]+/gu, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean).length;
  return { wordCount, readingMinutes: Math.max(1, Math.ceil(wordCount / 220)) };
}

if (!(await exists(notebookDirectory))) {
  if (await exists(outputPath)) {
    console.log("Notebook source is outside this deployment checkout; using the validated generated snapshot.");
    process.exit(0);
  }
  throw new Error(`Notebook source is unavailable: ${notebookDirectory}`);
}

const chapters = [];
for (const { locale, directory } of localeSpecs) {
  if (!(await exists(directory))) throw new Error(`Notebook locale directory is unavailable: ${directory}`);
  const fileNames = (await readdir(directory))
    .filter((fileName) => /^\d{2}-[a-z0-9-]+\.md$/.test(fileName))
    .sort();
  if (fileNames.length === 0) throw new Error(`No ${locale} notebook chapters were found`);

  const slugs = new Set();
  const chapterNumbers = new Set();
  for (const fileName of fileNames) {
    const source = await readFile(resolve(directory, fileName), "utf8");
    const qualifiedName = `${locale}/${fileName}`;
    const { metadata, markdown } = parseFrontmatter(source, qualifiedName);
    if (slugs.has(metadata.slug) || chapterNumbers.has(metadata.chapter)) {
      throw new Error(`${qualifiedName}: duplicate chapter number or slug`);
    }
    slugs.add(metadata.slug);
    chapterNumbers.add(metadata.chapter);

    const body = stripDocumentTitle(markdown, metadata.title, qualifiedName);
    const measurement = measureBody(body, locale);
    chapters.push({
      locale,
      slug: metadata.slug,
      number: metadata.chapter,
      day: metadata.day,
      title: metadata.title,
      description: metadata.description,
      body,
      sections: sectionsFor(body, qualifiedName),
      ...measurement,
    });
  }

  const localized = chapters.filter((chapter) => chapter.locale === locale).sort((left, right) => left.number - right.number);
  for (let index = 0; index < localized.length; index += 1) {
    if (localized[index].number !== index + 1) {
      throw new Error(`${locale} chapter numbering must be contiguous from 1; found ${localized[index].number}`);
    }
  }
}

const englishKeys = chapters.filter((chapter) => chapter.locale === "en").map((chapter) => `${chapter.number}:${chapter.slug}`);
const chineseKeys = chapters.filter((chapter) => chapter.locale === "zh").map((chapter) => `${chapter.number}:${chapter.slug}`);
if (JSON.stringify(englishKeys) !== JSON.stringify(chineseKeys)) {
  throw new Error("English and Chinese chapter numbers and slugs must match exactly");
}

chapters.sort((left, right) => left.locale.localeCompare(right.locale) || left.number - right.number);

const generated = `// Generated by scripts/sync-notebook.mjs from docs/notebook/*.md.\n` +
  `// Do not edit this file directly.\n\n` +
  `export type NotebookSection = { level: 2 | 3; title: string; id: string };\n` +
  `export type NotebookLocale = "en" | "zh";\n` +
  `export type NotebookChapter = {\n` +
  `  locale: NotebookLocale; slug: string; number: number; day: string; title: string; description: string;\n` +
  `  body: string; sections: NotebookSection[]; wordCount: number; readingMinutes: number;\n` +
  `};\n\n` +
  `export const chapters: NotebookChapter[] = ${JSON.stringify(chapters, null, 2)};\n\n` +
  `export function chaptersFor(locale: NotebookLocale): NotebookChapter[] {\n` +
  `  return chapters.filter((chapter) => chapter.locale === locale);\n` +
  `}\n\n` +
  `export function chapterBySlug(locale: NotebookLocale, slug: string): NotebookChapter | undefined {\n` +
  `  return chapters.find((chapter) => chapter.locale === locale && chapter.slug === slug);\n` +
  `}\n`;

await mkdir(dirname(outputPath), { recursive: true });
await writeFile(outputPath, generated, "utf8");
console.log(`Synced ${chapters.length} notebook chapter(s).`);
