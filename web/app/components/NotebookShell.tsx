import Link from "next/link";
import Image from "next/image";
import type { NotebookChapter } from "../generated/notebook";
import { chaptersFor } from "../generated/notebook";
import { MarkdownChapter } from "./MarkdownChapter";
import { ReadingProgress } from "./ReadingProgress";

export function NotebookShell({ chapter }: { chapter: NotebookChapter }) {
  const isChinese = chapter.locale === "zh";
  const chapters = chaptersFor(chapter.locale);
  const chapterIndex = chapters.findIndex((entry) => entry.slug === chapter.slug);
  const previous = chapterIndex > 0 ? chapters[chapterIndex - 1] : undefined;
  const next = chapterIndex + 1 < chapters.length ? chapters[chapterIndex + 1] : undefined;
  const chapterHref = (entry: NotebookChapter, locale = chapter.locale) => {
    const prefix = locale === "zh" ? "/zh" : "";
    return entry.number === 1 ? prefix || "/" : `${prefix}/chapters/${entry.slug}`;
  };
  const copy = isChinese ? {
    subtitle: "",
    published: ` ${chapters.length} `,
    part: " · ",
    nextTrail: "",
    nextTitle: " Token",
    inProgress: "",
    chapter: "",
    chapterSuffix: "",
    minutes: "",
    units: "",
    updated: " 1–3 ",
    coverAlt: ": , AST, Arena, , , ARM64 . ",
    coverCaption: ": , . ",
    previous: "",
    next: "",
    nextPending: " Token",
    page: "",
    baseline: "",
    baselineValue: " 1 ",
  } : {
    subtitle: "A proof-carrying compiler notebook",
    published: `${chapters.length} chapter published`,
    part: "Part I · Ground Truth",
    nextTrail: "Next trail",
    nextTitle: "From Characters to Preprocessing Tokens",
    inProgress: "In progress",
    chapter: "Chapter",
    chapterSuffix: "",
    minutes: "min read",
    units: "words",
    updated: "Updated with the verified Day 1–3 baseline",
    coverAlt: "An engraved compiler landscape: source enters preprocessing machinery, then an AST, an arena, an equality graph, a proof press, and an ARM64 machine.",
    coverCaption: "The territory ahead: source text becomes a bootable, independently checked result.",
    previous: "Previous",
    next: "Next",
    nextPending: "Preprocessing Tokens",
    page: "On this page",
    baseline: "Chapter baseline",
    baselineValue: "Day 1 notebook",
  };
  const alternateHref = chapterHref(chapter, isChinese ? "en" : "zh");

  return (
    <div className={isChinese ? "book-shell locale-zh" : "book-shell"} lang={isChinese ? "zh-CN" : "en"}>
      <ReadingProgress />
      <header className="book-header">
        <Link href={isChinese ? "/zh" : "/"} className="wordmark" aria-label={isChinese ? "Building EvoCC " : "Building EvoCC home"}>
          <span className="wordmark-mark">E</span>
          <span><strong>Building EvoCC</strong><small>{copy.subtitle}</small></span>
        </Link>
        <div className="header-actions">
          <div className="header-status" aria-label={copy.published}>
            <span>{chapter.day}</span>
            <span>{copy.published}</span>
          </div>
          <Link className="language-switch" href={alternateHref} aria-label={isChinese ? "Switch to English" : ""}>
            <span className={!isChinese ? "active" : ""}>EN</span>
            <i aria-hidden="true">/</i>
            <span className={isChinese ? "active" : ""}>CH</span>
          </Link>
        </div>
      </header>

      <div className="book-grid">
        <aside className="chapter-rail" aria-label="Book chapters">
          <p className="rail-label">{copy.part}</p>
          <nav>
            {chapters.map((entry) => (
              <Link
                key={entry.slug}
                href={chapterHref(entry)}
                className={entry.slug === chapter.slug ? "chapter-link current" : "chapter-link"}
                aria-current={entry.slug === chapter.slug ? "page" : undefined}
              >
                <span>{String(entry.number).padStart(2, "0")}</span>
                <strong>{entry.title}</strong>
                <small>{entry.day}</small>
              </Link>
            ))}
          </nav>
          <div className="coming-next">
            <span>{copy.nextTrail}</span>
            <strong>{copy.nextTitle}</strong>
            <small>{copy.inProgress}</small>
          </div>
        </aside>

        <main className="reading-column">
          <article data-notebook-article>
            <header className="chapter-header">
              <p className="chapter-kicker">{copy.chapter} {chapter.number}{copy.chapterSuffix} · {chapter.day}</p>
              <h1>{chapter.title}</h1>
              <p className="chapter-deck">{chapter.description}</p>
              <div className="chapter-meta">
                <span>{chapter.readingMinutes} {copy.minutes}</span>
                <span>{chapter.wordCount.toLocaleString(isChinese ? "zh-CN" : "en-US")} {copy.units}</span>
                <span>{copy.updated}</span>
              </div>
              <figure className="chapter-cover">
                <Image
                  src="/evocc-compiler-map.png"
                  width={1822}
                  height={863}
                  priority
                  alt={copy.coverAlt}
                />
                <figcaption>{copy.coverCaption}</figcaption>
              </figure>
            </header>
            <div className="chapter-prose">
              <MarkdownChapter markdown={chapter.body} locale={chapter.locale} />
            </div>
          </article>

          <nav className="chapter-pagination" aria-label="Chapter pagination">
            {previous ? (
              <Link href={chapterHref(previous)}>
                <small>{copy.previous}</small><strong>{previous.title}</strong>
              </Link>
            ) : <span />}
            {next ? (
              <Link href={chapterHref(next)} className="next">
                <small>{copy.next}</small><strong>{next.title}</strong>
              </Link>
            ) : (
              <div className="next next-pending">
                <small>{copy.next}</small><strong>{copy.nextPending}</strong>
              </div>
            )}
          </nav>
        </main>

        <aside className="section-rail" aria-label={copy.page}>
          <p className="rail-label">{copy.page}</p>
          <nav>
            {chapter.sections.filter((section) => section.level === 2).map((section) => (
              <a key={section.id} href={`#${section.id}`}>{section.title}</a>
            ))}
          </nav>
          <p className="edition-note">
            <span>{copy.baseline}</span>
            {copy.baselineValue}<br /><code>7b2ae32</code>
          </p>
        </aside>
      </div>
    </div>
  );
}
