// Markdown body for chat bubbles: react-markdown + remark-gfm (per the
// client-integration map both are in the root package.json for this purpose).
//
// Link handling is three-way (session-chat-links.ts classifies the href):
// image destinations open in the chat's centered image overlay, web URLs go to
// the host's browser (gpui: its own Browser view, Shift+click for the OS
// browser; web/phone: a normal target="_blank" anchor), and machine paths go to
// the host's editor surfaces. A path link on a host with no editor surface
// renders as inert text — following it would only navigate the page away from
// the chat.

import { IconCheck, IconCopy } from "@tabler/icons-react";
import {
  Children,
  isValidElement,
  useMemo,
  useState,
  type ComponentProps,
  type ReactNode,
} from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "../../components/ui/button";
import { AppTooltip } from "../app-tooltip";
import {
  isSessionChatImageHref,
  SessionChatInlineImage,
  sessionChatImageTargetForHref,
  useSessionChatImageViewer,
  type SessionChatImageViewerApi,
} from "./session-chat-image-viewer";
import {
  classifySessionChatLinkHref,
  useSessionChatHostLinks,
  type SessionChatHostLinks,
} from "./session-chat-links";

const REMARK_PLUGINS = [remarkGfm];

function nodeText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map(nodeText).join("");
  }
  if (isValidElement<{ children?: ReactNode }>(node)) {
    return nodeText(node.props.children);
  }
  return "";
}

function MarkdownCodeBlock({ children }: ComponentProps<"pre">) {
  const [copied, setCopied] = useState(false);
  const codeNode = Children.toArray(children)[0];
  const className = isValidElement<{ className?: string }>(codeNode)
    ? codeNode.props.className
    : undefined;
  const language = className?.match(/language-([^\s]+)/)?.[1] ?? "code";
  const text = nodeText(children).replace(/\n$/, "");

  return (
    <div className="ghostex-chat-markdown-codeblock">
      <div className="ghostex-chat-markdown-codeblock-header">
        <span>{language}</span>
        <Button
          aria-label="Copy code"
          onClick={() => {
            void navigator.clipboard.writeText(text).then(() => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            });
          }}
          size="icon-xs"
          variant="ghost"
        >
          {copied ? (
            <IconCheck aria-hidden="true" data-icon="inline-start" stroke={1.9} />
          ) : (
            <IconCopy aria-hidden="true" data-icon="inline-start" stroke={1.9} />
          )}
        </Button>
      </div>
      <pre>{children}</pre>
    </div>
  );
}

function markdownComponents(
  viewer: SessionChatImageViewerApi | null,
  hostLinks: SessionChatHostLinks | null,
): Components {
  return {
    pre: MarkdownCodeBlock,
    a: ({ children, href }) => {
      if (typeof href !== "string" || href === "") {
        return <>{children}</>;
      }
      if (viewer && isSessionChatImageHref(href)) {
        const target = sessionChatImageTargetForHref(href);
        if (viewer.canOpen(target)) {
          return (
            <button
              className="ghostex-chat-image-link"
              onClick={() => viewer.open(target)}
              type="button"
            >
              {children}
            </button>
          );
        }
      }
      const target = classifySessionChatLinkHref(href);
      if (target.kind === "url") {
        const openUrl = hostLinks?.openUrl;
        if (openUrl) {
          return (
            <AppTooltip content={target.url}>
              <a
                // Kept an anchor so the URL shows in the status bar and the
                // context menu still offers Copy Link; the host owns the open.
                href={target.url}
                onClick={(event) => {
                  event.preventDefault();
                  openUrl(target.url, { external: event.shiftKey });
                }}
              >
                {children}
              </a>
            </AppTooltip>
          );
        }
        return (
          <a href={target.url} rel="noreferrer" target="_blank">
            {children}
          </a>
        );
      }
      if (target.kind === "file" && hostLinks?.openFile) {
        const openFile = hostLinks.openFile;
        return (
          <AppTooltip content={target.path}>
            <button
              className="ghostex-chat-file-link"
              onClick={() => openFile(target.path)}
              type="button"
            >
              {children}
            </button>
          </AppTooltip>
        );
      }
      return (
        <AppTooltip content={target.kind === "file" ? target.path : undefined}>
          <span>{children}</span>
        </AppTooltip>
      );
    },
    img: ({ alt, src }) => {
      if (viewer && typeof src === "string" && src !== "") {
        const target = {
          ...sessionChatImageTargetForHref(src),
          ...(alt ? { alt } : {}),
        };
        if (viewer.canOpen(target)) {
          // An image the agent wrote as an image renders as one; the named
          // button stays as the stand-in when its bytes cannot be read.
          return (
            <SessionChatInlineImage
              fallback={
                <button
                  className="ghostex-chat-image-link"
                  onClick={() => viewer.open(target)}
                  type="button"
                >
                  {alt || "Image"}
                </button>
              }
              target={target}
            />
          );
        }
      }
      return <img alt={alt ?? ""} src={src ?? ""} />;
    },
  };
}

export function SessionChatMarkdown({ markdown }: { markdown: string }) {
  const viewer = useSessionChatImageViewer();
  const hostLinks = useSessionChatHostLinks();
  const components = useMemo(
    () => markdownComponents(viewer, hostLinks),
    [hostLinks, viewer],
  );
  return (
    <div className="ghostex-chat-markdown">
      <ReactMarkdown components={components} remarkPlugins={REMARK_PLUGINS}>
        {markdown}
      </ReactMarkdown>
    </div>
  );
}
