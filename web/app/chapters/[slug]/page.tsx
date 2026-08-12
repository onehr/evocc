import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { NotebookShell } from "../../components/NotebookShell";
import { chapterBySlug, chaptersFor } from "../../generated/notebook";

type ChapterPageProps = { params: Promise<{ slug: string }> };

export function generateStaticParams() {
  return chaptersFor("en").slice(1).map((chapter) => ({ slug: chapter.slug }));
}

export async function generateMetadata({ params }: ChapterPageProps): Promise<Metadata> {
  const slug = (await params).slug;
  const chapter = chapterBySlug("en", slug);
  if (!chapter) return {};
  return {
    title: chapter.title,
    description: chapter.description,
    alternates: { canonical: `/chapters/${slug}`, languages: { en: `/chapters/${slug}`, "zh-CN": `/zh/chapters/${slug}` } },
  };
}

export default async function ChapterPage({ params }: ChapterPageProps) {
  const chapter = chapterBySlug("en", (await params).slug);
  if (!chapter) notFound();
  return <NotebookShell chapter={chapter} />;
}
