"use client";

import { useEffect, useState } from "react";

export function ReadingProgress() {
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    function updateProgress() {
      const article = document.querySelector<HTMLElement>("[data-notebook-article]");
      if (!article) return;
      const start = article.offsetTop;
      const distance = Math.max(1, article.offsetHeight - window.innerHeight);
      setProgress(Math.min(1, Math.max(0, (window.scrollY - start) / distance)));
    }
    updateProgress();
    window.addEventListener("scroll", updateProgress, { passive: true });
    window.addEventListener("resize", updateProgress);
    return () => {
      window.removeEventListener("scroll", updateProgress);
      window.removeEventListener("resize", updateProgress);
    };
  }, []);

  return (
    <div className="reading-progress" aria-hidden="true">
      <span style={{ transform: `scaleX(${progress})` }} />
    </div>
  );
}
