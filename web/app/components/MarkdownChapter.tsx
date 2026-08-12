"use client";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Children, isValidElement } from "react";
import { MermaidDiagram } from "./MermaidDiagram";

function headingId(value: string) {
  return value
    .toLowerCase()
    .replace(/[`*_]/g, "")
    .replace(/&/g, " and ")
    .replace(/[^a-z0-9\u4e00-\u9fff]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function headingText(children: React.ReactNode): string {
  return Array.isArray(children)
    ? children.map(headingText).join("")
    : typeof children === "string" || typeof children === "number"
      ? String(children)
      : "";
}

export function MarkdownChapter({ markdown }: { markdown: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        h2({ children }) {
          const text = headingText(children);
          return <h2 id={headingId(text)}>{children}</h2>;
        },
        h3({ children }) {
          const text = headingText(children);
          return <h3 id={headingId(text)}>{children}</h3>;
        },
        a({ href = "", children }) {
          const external = /^https?:\/\//.test(href);
          const repositoryLocal = href.startsWith("../");
          if (repositoryLocal) {
            return (
              <span className="repository-reference" title={`Repository path: ${href}`}>
                {children}
                <span aria-hidden="true"> ↗</span>
              </span>
            );
          }
          return (
            <a href={href} target={external ? "_blank" : undefined} rel={external ? "noreferrer" : undefined}>
              {children}
            </a>
          );
        },
        code({ className, children, ...properties }) {
          return <code className={className} {...properties}>{children}</code>;
        },
        pre({ children }) {
          const [child] = Children.toArray(children);
          if (
            isValidElement<{ className?: string; children?: React.ReactNode }>(child) &&
            child.props.className === "language-mermaid"
          ) {
            return <MermaidDiagram source={headingText(child.props.children).replace(/\n$/, "")} />;
          }
          return <pre>{children}</pre>;
        },
      }}
    >
      {markdown}
    </ReactMarkdown>
  );
}
