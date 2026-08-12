import Link from "next/link";
import Image from "next/image";
import type { NotebookChapter } from "../generated/notebook";
import { chapters } from "../generated/notebook";
import { MarkdownChapter } from "./MarkdownChapter";
import { ReadingProgress } from "./ReadingProgress";

export function NotebookShell({ chapter }: { chapter: NotebookChapter }) {
  const chapterIndex = chapters.findIndex((entry) => entry.slug === chapter.slug);
  const previous = chapterIndex > 0 ? chapters[chapterIndex - 1] : undefined;
  const next = chapterIndex + 1 < chapters.length ? chapters[chapterIndex + 1] : undefined;

  return (
    <div className="book-shell">
      <ReadingProgress />
      <header className="book-header">
        <Link href="/" className="wordmark" aria-label="Building EvoCC home">
          <span className="wordmark-mark">E</span>
          <span><strong>Building EvoCC</strong><small>A proof-carrying compiler notebook</small></span>
        </Link>
        <div className="header-status" aria-label={`${chapters.length} published chapter`}>
          <span>{chapter.day}</span>
          <span>{chapters.length} chapter published</span>
        </div>
      </header>

      <div className="book-grid">
        <aside className="chapter-rail" aria-label="Book chapters">
          <p className="rail-label">Part I · Ground Truth</p>
          <nav>
            {chapters.map((entry) => (
              <Link
                key={entry.slug}
                href={entry.number === 1 ? "/" : `/chapters/${entry.slug}`}
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
            <span>Next trail</span>
            <strong>From Characters to Preprocessing Tokens</strong>
            <small>In progress</small>
          </div>
        </aside>

        <main className="reading-column">
          <article data-notebook-article>
            <header className="chapter-header">
              <p className="chapter-kicker">Chapter {chapter.number} · {chapter.day}</p>
              <h1>{chapter.title}</h1>
              <p className="chapter-deck">{chapter.description}</p>
              <div className="chapter-meta">
                <span>{chapter.readingMinutes} min read</span>
                <span>{chapter.wordCount.toLocaleString("en-US")} words</span>
                <span>Updated with the verified Day 1–3 baseline</span>
              </div>
              <figure className="chapter-cover">
                <Image
                  src="/evocc-compiler-map.png"
                  width={1822}
                  height={863}
                  priority
                  alt="An engraved compiler landscape: source enters preprocessing machinery, then an AST, an arena, an equality graph, a proof press, and an ARM64 machine."
                />
                <figcaption>The territory ahead: source text becomes a bootable, independently checked result.</figcaption>
              </figure>
            </header>
            <div className="chapter-prose">
              <MarkdownChapter markdown={chapter.body} />
            </div>
          </article>

          <nav className="chapter-pagination" aria-label="Chapter pagination">
            {previous ? (
              <Link href={previous.number === 1 ? "/" : `/chapters/${previous.slug}`}>
                <small>Previous</small><strong>{previous.title}</strong>
              </Link>
            ) : <span />}
            {next ? (
              <Link href={`/chapters/${next.slug}`} className="next">
                <small>Next</small><strong>{next.title}</strong>
              </Link>
            ) : (
              <div className="next next-pending">
                <small>Next chapter</small><strong>Preprocessing Tokens</strong>
              </div>
            )}
          </nav>
        </main>

        <aside className="section-rail" aria-label="On this page">
          <p className="rail-label">On this page</p>
          <nav>
            {chapter.sections.filter((section) => section.level === 2).map((section) => (
              <a key={section.id} href={`#${section.id}`}>{section.title}</a>
            ))}
          </nav>
          <p className="edition-note">
            <span>Chapter baseline</span>
            Day 1 notebook<br /><code>7b2ae32</code>
          </p>
        </aside>
      </div>
    </div>
  );
}
