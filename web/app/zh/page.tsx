import type { Metadata } from "next";
import { NotebookShell } from "../components/NotebookShell";
import { chaptersFor } from "../generated/notebook";

const chapter = chaptersFor("zh")[0];

export const metadata: Metadata = {
  title: chapter.title,
  description: chapter.description,
  alternates: { canonical: "/zh", languages: { en: "/", "zh-CN": "/zh" } },
};

export default function ChineseHome() {
  return <NotebookShell chapter={chapter} />;
}
