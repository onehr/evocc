import type { Metadata } from "next";
import { NotebookShell } from "./components/NotebookShell";
import { chapters } from "./generated/notebook";

export const metadata: Metadata = {
  title: chapters[0].title,
  description: chapters[0].description,
};

export default function Home() {
  return <NotebookShell chapter={chapters[0]} />;
}
