import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { NotebookShell } from "../../components/NotebookShell";
import { chapterBySlug, chapters } from "../../generated/notebook";

type ChapterPageProps = { params: Promise<{ slug: string }> };

export function generateStaticParams() {
  return chapters.slice(1).map((chapter) => ({ slug: chapter.slug }));
}

export async function generateMetadata({ params }: ChapterPageProps): Promise<Metadata> {
  const chapter = chapterBySlug((await params).slug);
  if (!chapter) return {};
  return { title: chapter.title, description: chapter.description };
}

export default async function ChapterPage({ params }: ChapterPageProps) {
  const chapter = chapterBySlug((await params).slug);
  if (!chapter) notFound();
  return <NotebookShell chapter={chapter} />;
}
