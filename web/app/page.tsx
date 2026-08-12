import type { Metadata } from "next";
import { NotebookShell } from "./components/NotebookShell";
import { chaptersFor } from "./generated/notebook";

const chapter = chaptersFor("en")[0];

export const metadata: Metadata = {
  title: chapter.title,
  description: chapter.description,
  alternates: { canonical: "/", languages: { en: "/", "zh-CN": "/zh" } },
};

export default function Home() {
  return <NotebookShell chapter={chapter} />;
}
