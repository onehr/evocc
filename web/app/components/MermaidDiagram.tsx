"use client";

import { useEffect, useId, useState } from "react";

export function MermaidDiagram({ source }: { source: string }) {
  const reactId = useId();
  const diagramId = `evocc-diagram-${reactId.replace(/[^a-zA-Z0-9]/g, "")}`;
  const [svg, setSvg] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;

    async function renderDiagram() {
      try {
        const { default: mermaid } = await import("mermaid");
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          theme: "base",
          themeVariables: {
            fontFamily: "var(--font-book-sans)",
            primaryColor: "#f6efe2",
            primaryBorderColor: "#9f7143",
            primaryTextColor: "#28231d",
            secondaryColor: "#e5efe8",
            secondaryBorderColor: "#54725e",
            secondaryTextColor: "#28231d",
            tertiaryColor: "#eef0f4",
            tertiaryBorderColor: "#67758a",
            tertiaryTextColor: "#28231d",
            lineColor: "#766553",
            noteBkgColor: "#fff9ea",
            noteBorderColor: "#b99a59",
            noteTextColor: "#28231d",
          },
          flowchart: { curve: "basis", htmlLabels: true },
          sequence: { mirrorActors: false, useMaxWidth: true },
        });
        const result = await mermaid.render(diagramId, source);
        if (active) setSvg(result.svg);
      } catch (cause) {
        if (active) {
          setError(cause instanceof Error ? cause.message : "The diagram could not be rendered.");
        }
      }
    }

    void renderDiagram();
    return () => {
      active = false;
    };
  }, [diagramId, source]);

  if (error) {
    return (
      <figure className="diagram diagram-error">
        <figcaption>Diagram source</figcaption>
        <pre><code>{source}</code></pre>
      </figure>
    );
  }

  return (
    <figure
      className={`diagram${svg ? " diagram-ready" : ""}`}
      aria-label="Architecture diagram"
    >
      {svg ? (
        <div dangerouslySetInnerHTML={{ __html: svg }} />
      ) : (
        <div className="diagram-loading" role="status">Drawing the system map…</div>
      )}
    </figure>
  );
}
