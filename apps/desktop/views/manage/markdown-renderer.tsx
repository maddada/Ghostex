import {
  type CSSProperties,
  type ReactNode,
  useCallback,
  useMemo,
  useState,
} from "react";
import {
  IconCheck,
  IconCopy,
} from "@tabler/icons-react";
import { ManageAnnotation, ManageMarkdownBlock } from "./types";
import { manageAnnotationColor, writeTextToClipboard } from "./annotation-store";
import { parseManageMarkdownTableContent } from "./markdown-parser";
import { sanitizeManageBlockHtml, sanitizeManageHref, sanitizeManageImageSrc } from "./html-sanitize";

export function ManageMarkdownBlockRenderer({
  annotations,
  block,
  orderedIndex,
}: {
  annotations: ManageAnnotation[];
  block: ManageMarkdownBlock;
  orderedIndex?: number;
}) {
  switch (block.type) {
    case "heading": {
      const HeadingTag = `h${Math.min(Math.max(block.level ?? 1, 1), 6)}` as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
      return (
        <HeadingTag data-block-id={block.id} data-block-type="heading">
          {renderManageInlineMarkdown(block.content, annotations)}
        </HeadingTag>
      );
    }
    case "blockquote": {
      if (block.alertKind) {
        return (
          <div className="manage-md-alert" data-kind={block.alertKind} data-block-id={block.id}>
            <div className="manage-md-alert-title">{block.alertKind}</div>
            {block.content.split(/\n\n+/u).map((paragraph, index) => (
              <p key={index}>{renderManageInlineMarkdown(paragraph, annotations)}</p>
            ))}
          </div>
        );
      }
      return (
        <blockquote data-block-id={block.id}>
          {block.content.split(/\n\n+/u).map((paragraph, index) => (
            <p key={index}>{renderManageInlineMarkdown(paragraph, annotations)}</p>
          ))}
        </blockquote>
      );
    }
    case "list-item":
      return (
        <div
          className="manage-md-list-item"
          data-block-id={block.id}
          style={{ "--manage-md-list-level": block.level ?? 0 } as CSSProperties}
        >
          <span className="manage-md-list-marker">
            {block.checked !== undefined ? (
              <input checked={block.checked} readOnly tabIndex={-1} type="checkbox" />
            ) : block.ordered ? (
              `${orderedIndex ?? block.orderedStart ?? 1}.`
            ) : (
              "*"
            )}
          </span>
          <span className={block.checked ? "manage-md-list-text is-checked" : "manage-md-list-text"}>
            {renderManageInlineMarkdown(block.content, annotations)}
          </span>
        </div>
      );
    case "code":
      return <ManageMarkdownCodeBlock block={block} />;
    case "table":
      return <ManageMarkdownTable block={block} annotations={annotations} />;
    case "hr":
      return <hr data-block-id={block.id} />;
    case "html":
      return <ManageMarkdownHtmlBlock block={block} />;
    case "directive":
      return (
        <div className="manage-md-directive" data-kind={block.directiveKind ?? "note"} data-block-id={block.id}>
          {block.content.split(/\n\n+/u).map((paragraph, index) => (
            <p key={index}>{renderManageInlineMarkdown(paragraph, annotations)}</p>
          ))}
        </div>
      );
    case "paragraph":
    default:
      return (
        <p data-block-id={block.id}>
          {renderManageInlineMarkdown(block.content, annotations)}
        </p>
      );
  }
}

export function ManageMarkdownCodeBlock({ block }: { block: ManageMarkdownBlock }) {
  const [copied, setCopied] = useState(false);
  const copyCode = useCallback(async () => {
    try {
      await writeTextToClipboard(block.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_600);
    } catch {
      setCopied(false);
    }
  }, [block.content]);
  return (
    <div className="manage-md-code-block" data-block-id={block.id}>
      <button aria-label="Copy code" onClick={() => void copyCode()} type="button">
        {copied ? <IconCheck aria-hidden="true" size={14} /> : <IconCopy aria-hidden="true" size={14} />}
      </button>
      <pre>
        <code className={block.language ? `language-${block.language}` : undefined}>{block.content}</code>
      </pre>
    </div>
  );
}

export function ManageMarkdownTable({
  annotations,
  block,
}: {
  annotations: ManageAnnotation[];
  block: ManageMarkdownBlock;
}) {
  const { headers, rows } = parseManageMarkdownTableContent(block.content);
  return (
    <div className="manage-md-table-wrap" data-block-id={block.id}>
      <table>
        <thead>
          <tr>
            {headers.map((header, index) => (
              <th key={index}>{renderManageInlineMarkdown(header, annotations)}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {row.map((cell, cellIndex) => (
                <td key={cellIndex}>{renderManageInlineMarkdown(cell, annotations)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function ManageMarkdownHtmlBlock({ block }: { block: ManageMarkdownBlock }) {
  const sanitized = useMemo(() => sanitizeManageBlockHtml(block.content), [block.content]);
  return (
    <div
      className="manage-md-html-block"
      data-block-id={block.id}
      data-block-type="html"
      dangerouslySetInnerHTML={{ __html: sanitized }}
    />
  );
}

export function renderManageInlineMarkdown(text: string, annotations: ManageAnnotation[]): ReactNode {
  return renderManageAnnotatedInline(
    text,
    annotations.filter((annotation) => annotation.scope === "selection" && Boolean(annotation.quote)),
  );
}

export function renderManageAnnotatedInline(text: string, annotations: ManageAnnotation[]): ReactNode {
  const annotation = annotations.find((candidate) => text.includes(candidate.quote));
  if (!annotation) {
    return renderManageInlineTokens(text);
  }
  const index = text.indexOf(annotation.quote);
  const before = text.slice(0, index);
  const match = text.slice(index, index + annotation.quote.length);
  const after = text.slice(index + annotation.quote.length);
  const remaining = annotations.filter((candidate) => candidate.id !== annotation.id);
  return (
    <>
      {renderManageAnnotatedInline(before, remaining)}
      <mark
        className={`annotation-highlight manage-annotation-highlight ${annotation.type === "redline" ? "deletion" : "comment"}`}
        data-label-id={annotation.labelId}
        data-type={annotation.type}
        style={{ "--manage-annotation-color": manageAnnotationColor(annotation) } as CSSProperties}
      >
        {renderManageInlineTokens(match)}
      </mark>
      {renderManageAnnotatedInline(after, remaining)}
    </>
  );
}

export function renderManageInlineTokens(text: string): ReactNode {
  const nodes: ReactNode[] = [];
  let index = 0;
  while (index < text.length) {
    if (text.startsWith("`", index)) {
      const end = text.indexOf("`", index + 1);
      if (end > index) {
        nodes.push(
          <code className="manage-md-inline-code" key={`code-${index}`}>
            {text.slice(index + 1, end)}
          </code>,
        );
        index = end + 1;
        continue;
      }
    }
    if (text.startsWith("![", index)) {
      const image = parseManageMarkdownImageToken(text, index);
      if (image) {
        nodes.push(image.node);
        index = image.nextIndex;
        continue;
      }
    }
    if (text.startsWith("[", index)) {
      const link = parseManageMarkdownLinkToken(text, index);
      if (link) {
        nodes.push(link.node);
        index = link.nextIndex;
        continue;
      }
    }
    const strongMarker = text.startsWith("**", index) ? "**" : text.startsWith("__", index) ? "__" : "";
    if (strongMarker) {
      const end = text.indexOf(strongMarker, index + 2);
      if (end > index + 2) {
        nodes.push(<strong key={`strong-${index}`}>{renderManageInlineTokens(text.slice(index + 2, end))}</strong>);
        index = end + 2;
        continue;
      }
    }
    if (text.startsWith("~~", index)) {
      const end = text.indexOf("~~", index + 2);
      if (end > index + 2) {
        nodes.push(<del key={`del-${index}`}>{renderManageInlineTokens(text.slice(index + 2, end))}</del>);
        index = end + 2;
        continue;
      }
    }
    const emphasisMarker = text[index] === "*" || text[index] === "_" ? text[index] : "";
    if (emphasisMarker && !text.startsWith(`${emphasisMarker}${emphasisMarker}`, index)) {
      const end = text.indexOf(emphasisMarker, index + 1);
      if (end > index + 1) {
        nodes.push(<em key={`em-${index}`}>{renderManageInlineTokens(text.slice(index + 1, end))}</em>);
        index = end + 1;
        continue;
      }
    }

    const nextSpecial = findNextManageInlineSpecial(text, index + 1);
    nodes.push(...renderManagePlainInlineText(text.slice(index, nextSpecial), `text-${index}`));
    index = nextSpecial;
  }
  return nodes;
}

export function parseManageMarkdownLinkToken(text: string, index: number): { nextIndex: number; node: ReactNode } | undefined {
  const labelEnd = text.indexOf("]", index + 1);
  if (labelEnd <= index + 1 || text[labelEnd + 1] !== "(") {
    return undefined;
  }
  const hrefEnd = text.indexOf(")", labelEnd + 2);
  if (hrefEnd <= labelEnd + 2) {
    return undefined;
  }
  const href = sanitizeManageHref(text.slice(labelEnd + 2, hrefEnd).trim());
  const label = text.slice(index + 1, labelEnd);
  if (!href) {
    return {
      nextIndex: hrefEnd + 1,
      node: <span key={`link-${index}`}>{renderManageInlineTokens(label)}</span>,
    };
  }
  return {
    nextIndex: hrefEnd + 1,
    node: (
      <a href={href} key={`link-${index}`} rel="noreferrer" target={href.startsWith("#") ? undefined : "_blank"}>
        {renderManageInlineTokens(label)}
      </a>
    ),
  };
}

export function parseManageMarkdownImageToken(text: string, index: number): { nextIndex: number; node: ReactNode } | undefined {
  const altEnd = text.indexOf("]", index + 2);
  if (altEnd <= index + 2 || text[altEnd + 1] !== "(") {
    return undefined;
  }
  const srcEnd = text.indexOf(")", altEnd + 2);
  if (srcEnd <= altEnd + 2) {
    return undefined;
  }
  const alt = text.slice(index + 2, altEnd);
  const src = sanitizeManageImageSrc(text.slice(altEnd + 2, srcEnd).trim());
  return {
    nextIndex: srcEnd + 1,
    node: src ? <img alt={alt} className="manage-md-inline-image" key={`image-${index}`} src={src} /> : alt,
  };
}

export function renderManagePlainInlineText(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const urlPattern = /(https?:\/\/[^\s<)]+)/giu;
  let lastIndex = 0;
  for (const match of text.matchAll(urlPattern)) {
    const url = match[0];
    const index = match.index ?? 0;
    if (index > lastIndex) {
      nodes.push(text.slice(lastIndex, index));
    }
    nodes.push(
      <a href={url} key={`${keyPrefix}-url-${index}`} rel="noreferrer" target="_blank">
        {url}
      </a>,
    );
    lastIndex = index + url.length;
  }
  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

export function findNextManageInlineSpecial(text: string, start: number): number {
  const candidates = ["`", "![", "[", "**", "__", "~~", "*", "_"]
    .map((marker) => text.indexOf(marker, start))
    .filter((candidate) => candidate >= 0);
  return candidates.length > 0 ? Math.min(...candidates) : text.length;
}
