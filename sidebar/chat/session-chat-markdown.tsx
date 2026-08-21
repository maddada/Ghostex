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

import {
  IconAlertOctagon,
  IconAlertTriangle,
  IconBulb,
  IconCheck,
  IconCopy,
  IconInfoCircle,
  IconMessageReport,
} from "@tabler/icons-react";
import {
  Children,
  Component,
  createContext,
  isValidElement,
  Suspense,
  use,
  useContext,
  useDeferredValue,
  useEffect,
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
  estimateSessionChatHighlightSize,
  highlightSessionChatCode,
  resolveSessionChatCodeLanguage,
  SESSION_CHAT_HIGHLIGHTING_AVAILABLE,
  sessionChatHighlightCache,
  sessionChatHighlightCacheKey,
  sessionChatHighlighter,
  type SessionChatCodeLanguage,
} from "./session-chat-code-highlight";
import {
  remarkSessionChatGithubAlerts,
  type SessionChatAlertKind,
} from "./session-chat-github-alerts";
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

const REMARK_PLUGINS = [remarkGfm, remarkSessionChatGithubAlerts];

/**
 * GitHub's five alert kinds, with GitHub's own labels and colour families. The
 * colours live in chat.css so both chat themes can carry their own value; this
 * side only picks the label and the icon.
 */
const ALERT_PRESENTATIONS: Record<
  SessionChatAlertKind,
  { Icon: typeof IconInfoCircle; label: string }
> = {
  caution: { Icon: IconAlertOctagon, label: "Caution" },
  important: { Icon: IconMessageReport, label: "Important" },
  note: { Icon: IconInfoCircle, label: "Note" },
  tip: { Icon: IconBulb, label: "Tip" },
  warning: { Icon: IconAlertTriangle, label: "Warning" },
};

function alertPresentation(
  kind: unknown,
): { Icon: typeof IconInfoCircle; kind: SessionChatAlertKind; label: string } | null {
  if (typeof kind !== "string") return null;
  const presentation = ALERT_PRESENTATIONS[kind as SessionChatAlertKind];
  return presentation
    ? { ...presentation, kind: kind as SessionChatAlertKind }
    : null;
}

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

/*
 * Streaming turns append to the same markdown string token by token, so a fence
 * inside the newest turn is a moving target. The message list is the only place
 * that knows this (it owns `isWorking` and the synthetic streaming row), so it
 * hands the answer down through this context rather than every caller of
 * SessionChatMarkdown having to guess.
 */
const SessionChatMarkdownStreamingContext = createContext(false);

/**
 * Renders `fallback` once a descendant throws. Highlighting must never take a
 * message down with it: the fallback here is the exact plain `<pre>` the chat
 * rendered before Shiki existed.
 */
interface CodeHighlightBoundaryProps {
  children: ReactNode;
  fallback: ReactNode;
  resetKey: string;
}

interface CodeHighlightBoundaryState {
  failed: boolean;
  renderedKey: string;
}

class CodeHighlightBoundary extends Component<
  CodeHighlightBoundaryProps,
  CodeHighlightBoundaryState
> {
  override state: CodeHighlightBoundaryState = {
    failed: false,
    renderedKey: this.props.resetKey,
  };

  static getDerivedStateFromError(): Pick<CodeHighlightBoundaryState, "failed"> {
    return { failed: true };
  }

  static getDerivedStateFromProps(
    props: CodeHighlightBoundaryProps,
    state: CodeHighlightBoundaryState,
  ): CodeHighlightBoundaryState | null {
    // New content in the same slot deserves a fresh attempt; one bad fence must
    // not leave that position permanently unhighlighted.
    return state.renderedKey === props.resetKey
      ? null
      : { failed: false, renderedKey: props.resetKey };
  }

  override render(): ReactNode {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

function ShikiCodeHtml({ html }: { html: string }) {
  // Shiki output is generated from the fence's own text by a tokenizer that
  // escapes it; nothing user-authored reaches the DOM as markup.
  return (
    <div
      className="ghostex-chat-markdown-shiki"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

function UncachedShikiCode({
  cacheKey,
  code,
  fallback,
  language,
}: {
  cacheKey: string | null;
  code: string;
  fallback: ReactNode;
  language: SessionChatCodeLanguage;
}) {
  // Suspends until this language's grammar chunk has been fetched and
  // registered; the Suspense fallback above is the plain block, so first paint
  // is never blocked on Shiki.
  const core = use(sessionChatHighlighter(language));
  const html = useMemo(
    () => highlightSessionChatCode(core, code, language),
    [code, core, language],
  );

  useEffect(() => {
    if (html === null || cacheKey === null) {
      return;
    }
    sessionChatHighlightCache.set(
      cacheKey,
      html,
      estimateSessionChatHighlightSize(html, code),
    );
  }, [cacheKey, code, html]);

  return html === null ? <>{fallback}</> : <ShikiCodeHtml html={html} />;
}

function ShikiCodeBody({
  code,
  fallback,
  language,
}: {
  code: string;
  fallback: ReactNode;
  language: SessionChatCodeLanguage;
}) {
  const isStreaming = useContext(SessionChatMarkdownStreamingContext);

  /*
   * Two separate guards, because they solve two separate problems.
   *
   * `isStreaming` disables the cache: a half-written fence is not a document
   * anyone will scroll back to, and storing every intermediate prefix would
   * churn the 500-entry LRU out from under the finished blocks that need it.
   *
   * `useDeferredValue` is what stops the per-chunk tokenize. React renders the
   * fence with the previous value first and retries the new one at low
   * priority; when the next chunk lands before that retry finishes, the retry
   * is thrown away and restarted, so a fast stream tokenizes zero times and a
   * stream that pauses (or ends) tokenizes once. While the deferred value is
   * behind, the plain block is shown rather than stale highlighted text —
   * showing yesterday's tokens in a live code block would be worse than showing
   * today's without colour.
   */
  const deferredCode = useDeferredValue(code);
  const cacheKey = isStreaming
    ? null
    : sessionChatHighlightCacheKey(code, language);
  const cached = cacheKey === null ? null : sessionChatHighlightCache.get(cacheKey);
  if (cached !== null) {
    return <ShikiCodeHtml html={cached} />;
  }
  if (deferredCode !== code) {
    return <>{fallback}</>;
  }

  return (
    <CodeHighlightBoundary fallback={fallback} resetKey={`${language}:${code.length}`}>
      <Suspense fallback={fallback}>
        <UncachedShikiCode
          cacheKey={cacheKey}
          code={code}
          fallback={fallback}
          language={language}
        />
      </Suspense>
    </CodeHighlightBoundary>
  );
}

function MarkdownCodeBlock({ children }: ComponentProps<"pre">) {
  const [copied, setCopied] = useState(false);
  const codeNode = Children.toArray(children)[0];
  const className = isValidElement<{ className?: string }>(codeNode)
    ? codeNode.props.className
    : undefined;
  const fenceInfo = className?.match(/language-([^\s]+)/)?.[1];
  const language = fenceInfo ?? "code";
  // The raw fence text, trailing newline included, is what Shiki must see: the
  // plain <pre> renders that newline as a final empty line, so dropping it
  // would make the block jump a row shorter the moment highlighting lands.
  const source = nodeText(children);
  const text = source.replace(/\n$/, "");
  // Hosts that cannot load the highlighter at all (the mobile webview has no
  // origin to load anything from) build the flag as false, so no fence even
  // starts a load that cannot succeed.
  const shikiLanguage = SESSION_CHAT_HIGHLIGHTING_AVAILABLE
    ? resolveSessionChatCodeLanguage(fenceInfo)
    : null;
  // Unlabelled and unsupported fences are a normal outcome, not a failure:
  // they stay exactly as they render today.
  const plainBlock = <pre>{children}</pre>;

  return (
    <div className="ghostex-chat-markdown-codeblock">
      <div className="ghostex-chat-markdown-codeblock-header">
        <span>{language}</span>
        <Button
          aria-label="Copy code"
          onClick={() => {
            // Always the fence's own source, never the highlighted markup.
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
      {shikiLanguage === null ? (
        plainBlock
      ) : (
        <ShikiCodeBody
          code={source}
          fallback={plainBlock}
          language={shikiLanguage}
        />
      )}
    </div>
  );
}

function markdownComponents(
  viewer: SessionChatImageViewerApi | null,
  hostLinks: SessionChatHostLinks | null,
): Components {
  return {
    pre: MarkdownCodeBlock,
    blockquote: ({ children, node: _node, ...props }) => {
      const alert = alertPresentation(
        (props as Record<string, unknown>)["data-alert"],
      );
      if (!alert) {
        return <blockquote {...props}>{children}</blockquote>;
      }
      // Deliberately not a <blockquote>: quotes are muted, and an alert's body
      // is ordinary text sitting under a coloured title — only the border and
      // the title carry the colour.
      return (
        <div
          className="ghostex-chat-markdown-alert"
          data-alert={alert.kind}
          role="note"
        >
          <div className="ghostex-chat-markdown-alert-title">
            <alert.Icon aria-hidden size={15} stroke={2} />
            {alert.label}
          </div>
          {children}
        </div>
      );
    },
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

export function SessionChatMarkdown({
  isStreaming = false,
  markdown,
}: {
  /**
   * True while this body is still being appended to by a working agent. Only
   * syntax highlighting reads it (see ShikiCodeBody); the markdown itself
   * renders identically either way.
   */
  isStreaming?: boolean;
  markdown: string;
}) {
  const viewer = useSessionChatImageViewer();
  const hostLinks = useSessionChatHostLinks();
  const components = useMemo(
    () => markdownComponents(viewer, hostLinks),
    [hostLinks, viewer],
  );
  return (
    <SessionChatMarkdownStreamingContext value={isStreaming}>
      <div className="ghostex-chat-markdown">
        <ReactMarkdown components={components} remarkPlugins={REMARK_PLUGINS}>
          {markdown}
        </ReactMarkdown>
      </div>
    </SessionChatMarkdownStreamingContext>
  );
}
