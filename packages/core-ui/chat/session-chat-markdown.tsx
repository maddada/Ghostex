// Markdown body for chat bubbles: react-markdown + remark-gfm (per the
// client-integration map both are in the root package.json for this purpose).
//
// Link handling is three-way (session-chat-links.ts classifies the href):
// image destinations stay as thumbnails that open in the centered viewer,
// machine-path links use the same typed pills as the composer and invoke the
// host's Docs/Code route, and web URLs go to the host's browser (gpui: its own
// Browser view, Shift+click for the OS browser; web/phone: a normal
// target="_blank" anchor).
//
// Inline code gets the same treatment without a markdown link around it: a span
// that is unmistakably a file reference (session-chat-file-paths.ts decides,
// conservatively) becomes a chip that opens the file. Everything else — and
// every span on a host with no editor — stays the grey inline-code span it has
// always been.

import {
  IconAlertOctagon,
  IconAlertTriangle,
  IconArrowsMaximize,
  IconArrowsMinimize,
  IconBulb,
  IconCheck,
  IconChevronRight,
  IconCopy,
  IconExternalLink,
  IconInfoCircle,
  IconLink,
  IconMessageReport,
  IconTextWrap,
} from '@tabler/icons-react';
import {
  Children,
  Component,
  createContext,
  isValidElement,
  Suspense,
  use,
  useCallback,
  useContext,
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type CSSProperties,
  type ReactNode,
} from 'react';
import ReactMarkdown, { type Components, type ExtraProps } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Button } from '../../components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu';
import { AppTooltip } from '../app-tooltip';
import {
  remarkSessionChatCodeMeta,
  sessionChatFenceMeta,
  sessionChatFenceTitle,
  sessionChatFenceTitleIcon,
} from './session-chat-code-fence-meta';
import {
  estimateSessionChatHighlightSize,
  highlightSessionChatCode,
  resolveSessionChatCodeLanguage,
  SESSION_CHAT_HIGHLIGHTING_AVAILABLE,
  sessionChatHighlightCache,
  sessionChatHighlightCacheKey,
  sessionChatHighlighter,
  type SessionChatCodeLanguage,
} from './session-chat-code-highlight';
import { readSessionChatCodeWrapDefault, writeSessionChatCodeWrapDefault } from './session-chat-code-wrap';
import { remarkSessionChatDetails } from './session-chat-details';
import {
  remarkSessionChatBareFilePaths,
  remarkSessionChatInlineCode,
  resolveSessionChatFenceTitleFilePath,
  resolveSessionChatInlineCodeFilePath,
  SESSION_CHAT_FILE_PATH_ATTRIBUTE,
  sessionChatFilePathChipLabel,
  sessionChatFilePathIcon,
  sessionChatFilePathTitle,
  sessionChatFilePositionSuffix,
  type SessionChatFilePathRef,
  type SessionChatFilePosition,
} from './session-chat-file-paths';
import { remarkSessionChatGithubAlerts, type SessionChatAlertKind } from './session-chat-github-alerts';
import {
  isSessionChatImageHref,
  SessionChatImageReference,
  SessionChatInlineImage,
  sessionChatImageTargetForHref,
  useSessionChatImageViewer,
  type SessionChatImageViewerApi,
} from './session-chat-image-viewer';
import { remarkSessionChatImageReferences } from './session-chat-image-reference-markdown';
import {
  classifySessionChatLinkHref,
  SESSION_CHAT_WEB_URL_ATTRIBUTE,
  sessionChatFilePositionFromHref,
  useSessionChatHostLinks,
  type SessionChatHostLinks,
} from './session-chat-links';
import { sessionChatReferenceKind, type SessionChatReferenceKind } from './session-chat-reference-pills';
import {
  SESSION_CHAT_COPY_CODE_ATTRIBUTE,
  sessionChatTableToCsv,
  sessionChatTableToMarkdown,
} from './session-chat-table-clipboard';
import { sessionChatListInterruptSource } from './session-chat-list-interrupt';
import { remarkSessionChatHardBreaks, sessionChatUserMarkdownSource } from './session-chat-user-text';

/*
 * Order matters in one place: remarkSessionChatDetails runs before the three
 * plugins under it, because it can hand them nodes that did not exist when it
 * started — the body of a `<details>` an agent wrote without a blank line is
 * parsed inside that plugin, and its fences, quotes and inline code have to
 * reach the fence-meta, alert and inline-code passes like any other node.
 */
const REMARK_PLUGINS = [
  remarkGfm,
  remarkSessionChatImageReferences,
  remarkSessionChatDetails,
  remarkSessionChatCodeMeta,
  remarkSessionChatGithubAlerts,
  remarkSessionChatInlineCode,
];

/*
 * The same chain for text somebody typed into the composer, with the two
 * chat-text corrections from session-chat-user-text.ts on the end: every typed
 * newline becomes a real hard break. It runs last so it also reaches the nodes
 * remarkSessionChatDetails parsed into existence.
 */
const CHAT_TEXT_REMARK_PLUGINS = [...REMARK_PLUGINS, remarkSessionChatBareFilePaths, remarkSessionChatHardBreaks];

/**
 * GitHub's five alert kinds, with GitHub's own labels and colour families. The
 * colours live in chat.css so both chat themes can carry their own value; this
 * side only picks the label and the icon.
 */
const ALERT_PRESENTATIONS: Record<SessionChatAlertKind, { Icon: typeof IconInfoCircle; label: string }> = {
  caution: { Icon: IconAlertOctagon, label: 'Caution' },
  important: { Icon: IconMessageReport, label: 'Important' },
  note: { Icon: IconInfoCircle, label: 'Note' },
  tip: { Icon: IconBulb, label: 'Tip' },
  warning: { Icon: IconAlertTriangle, label: 'Warning' },
};

function alertPresentation(
  kind: unknown
): { Icon: typeof IconInfoCircle; kind: SessionChatAlertKind; label: string } | null {
  if (typeof kind !== 'string') return null;
  const presentation = ALERT_PRESENTATIONS[kind as SessionChatAlertKind];
  return presentation ? { ...presentation, kind: kind as SessionChatAlertKind } : null;
}

function nodeText(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map(nodeText).join('');
  }
  if (isValidElement<{ children?: ReactNode }>(node)) {
    return nodeText(node.props.children);
  }
  return '';
}

/**
 * Widens the marker gutter for an ordered list whose last marker needs more
 * room than `.ghostex-chat-markdown ol`'s 1.25rem, which holds two digits.
 * Markers are painted outside the padding box, so a three-digit one on an
 * ordinary list would sit left of the message's text edge, on top of the agent
 * rail. Only the list that needs it pays: `undefined` leaves the stylesheet's
 * default in place, so the common short list keeps the tight indent.
 */
function sessionChatOrderedListGutter(itemCount: number, start: number | undefined): CSSProperties | undefined {
  const first = typeof start === 'number' && Number.isFinite(start) ? start : 1;
  const digits = String(Math.abs(first + Math.max(itemCount - 1, 0))).length;
  // One extra character for the "." and the space after it.
  return digits > 2 ? ({ '--chat-list-gutter': `${digits + 1}ch` } as CSSProperties) : undefined;
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

class CodeHighlightBoundary extends Component<CodeHighlightBoundaryProps, CodeHighlightBoundaryState> {
  override state: CodeHighlightBoundaryState = {
    failed: false,
    renderedKey: this.props.resetKey,
  };

  static getDerivedStateFromError(): Pick<CodeHighlightBoundaryState, 'failed'> {
    return { failed: true };
  }

  static getDerivedStateFromProps(
    props: CodeHighlightBoundaryProps,
    state: CodeHighlightBoundaryState
  ): CodeHighlightBoundaryState | null {
    // New content in the same slot deserves a fresh attempt; one bad fence must
    // not leave that position permanently unhighlighted.
    return state.renderedKey === props.resetKey ? null : { failed: false, renderedKey: props.resetKey };
  }

  override render(): ReactNode {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}

function ShikiCodeHtml({ html }: { html: string }) {
  // Shiki output is generated from the fence's own text by a tokenizer that
  // escapes it; nothing user-authored reaches the DOM as markup.
  return <div className='ghostex-chat-markdown-shiki' dangerouslySetInnerHTML={{ __html: html }} />;
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
  const html = useMemo(() => highlightSessionChatCode(core, code, language), [code, core, language]);

  useEffect(() => {
    if (html === null || cacheKey === null) {
      return;
    }
    sessionChatHighlightCache.set(cacheKey, html, estimateSessionChatHighlightSize(html, code));
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
  const cacheKey = isStreaming ? null : sessionChatHighlightCacheKey(code, language);
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
        <UncachedShikiCode cacheKey={cacheKey} code={code} fallback={fallback} language={language} />
      </Suspense>
    </CodeHighlightBoundary>
  );
}

/**
 * The left of a fence's header. A fence that names a file shows that name with
 * the same glyph an inline-code path chip would carry; a fence that names only
 * a language keeps showing the language, which is every fence agents write
 * without meta.
 *
 * A named path is split where the chips split one — directories, then filename
 * — for the same reason: in a ~385px transcript column a deep path cannot fit
 * beside the header's actions, and the part that may go is the middle of the
 * path, never the filename the header exists to name.
 */
function MarkdownCodeBlockTitle({
  language,
  reference,
  title,
}: {
  language: string;
  reference: SessionChatFilePathRef | null;
  title: string | null;
}) {
  if (title === null) {
    return <span className='ghostex-chat-markdown-codeblock-language'>{language}</span>;
  }
  const Icon = sessionChatFenceTitleIcon(title);
  // Split on the title as the agent wrote it, so the `:line` a fence quoted
  // stays attached to the filename rather than becoming a separate box.
  const separator = reference === null ? -1 : Math.max(title.lastIndexOf('/'), title.lastIndexOf('\\'));
  return (
    <span className='ghostex-chat-markdown-codeblock-title'>
      <Icon aria-hidden='true' className='ghostex-chat-markdown-codeblock-title-icon' size={13} stroke={1.8} />
      {separator < 0 ? null : (
        <span className='ghostex-chat-markdown-codeblock-title-parent'>{title.slice(0, separator)}</span>
      )}
      <span className='ghostex-chat-markdown-codeblock-title-name'>
        {separator < 0 ? title : title.slice(separator)}
      </span>
    </span>
  );
}

/**
 * The header's open-in-editor action, for a fence that names a file.
 *
 * It goes through the host's editor exactly the way an inline path chip does —
 * the path is handed over as the agent wrote it, relative or not, because the
 * host is the side that knows the project root — and it carries the `:line`
 * the fence quoted, when it quoted one.
 *
 * The label is the whole path rather than "Open file": the header truncates a
 * deep path to fit a ~385px column, and this is where the elided directories
 * come back.
 */
function OpenFenceFileButton({
  openFile,
  reference,
}: {
  openFile: NonNullable<SessionChatHostLinks['openFile']>;
  reference: SessionChatFilePathRef;
}) {
  const label = `Open ${sessionChatFilePathTitle(reference)}`;
  return (
    <AppTooltip content={label}>
      <Button
        aria-label={label}
        onClick={() => openFile(reference.path, reference.position)}
        size='icon-xs'
        variant='ghost'
      >
        <IconExternalLink aria-hidden='true' data-icon='inline-start' stroke={1.9} />
      </Button>
    </AppTooltip>
  );
}

function MarkdownCodeBlock({ children, node }: ComponentProps<'pre'> & ExtraProps) {
  const [copied, setCopied] = useState(false);
  /*
   * Wrapping is the block's own state, seeded from the last choice made
   * anywhere in the chat (session-chat-code-wrap.ts): toggling one block never
   * reflows the blocks the reader is already looking at, and the blocks that
   * mount next follow the preference.
   */
  const [wrapped, setWrapped] = useState(readSessionChatCodeWrapDefault);
  const codeNode = Children.toArray(children)[0];
  const className = isValidElement<{ className?: string }>(codeNode) ? codeNode.props.className : undefined;
  const fenceInfo = className?.match(/language-([^\s]+)/)?.[1];
  const language = fenceInfo ?? 'code';
  // The raw fence text, trailing newline included, is what Shiki must see: the
  // plain <pre> renders that newline as a final empty line, so dropping it
  // would make the block jump a row shorter the moment highlighting lands.
  const source = nodeText(children);
  const text = source.replace(/\n$/, '');
  // Hosts that cannot load the highlighter at all (the mobile webview has no
  // origin to load anything from) build the flag as false, so no fence even
  // starts a load that cannot succeed.
  const shikiLanguage = SESSION_CHAT_HIGHLIGHTING_AVAILABLE ? resolveSessionChatCodeLanguage(fenceInfo) : null;
  // Unlabelled and unsupported fences are a normal outcome, not a failure:
  // they stay exactly as they render today.
  const plainBlock = <pre>{children}</pre>;
  const title = sessionChatFenceTitle(sessionChatFenceMeta(node));
  /*
   * A fence that names a file can open it, on the same terms an inline path
   * chip does: only when the name really is a path, and only on a host that
   * owns an editor. The web app and the phone leave openFile undefined, and
   * there the header keeps the two actions it has always had rather than
   * growing a third that would do nothing.
   */
  const openFile = useSessionChatHostLinks()?.openFile;
  const titleReference = title === null ? null : resolveSessionChatFenceTitleFilePath(title);
  const wrapLabel = wrapped ? 'Stop wrapping lines' : 'Wrap lines';
  const copyLabel = copied ? 'Copied' : 'Copy code';

  return (
    // data-wrap drives the wrapping rule in chat.css, which is written against
    // any <pre> inside this block — the plain one and the <pre class="shiki">
    // Shiki swaps in for it alike.
    <div className='ghostex-chat-markdown-codeblock' data-wrap={wrapped ? 'true' : 'false'}>
      <div className='ghostex-chat-markdown-codeblock-header'>
        <MarkdownCodeBlockTitle language={language} reference={titleReference} title={title} />
        <span aria-label='Code block actions' className='ghostex-chat-markdown-codeblock-actions' role='toolbar'>
          <AppTooltip content={wrapLabel}>
            <Button
              aria-label={wrapLabel}
              aria-pressed={wrapped}
              onClick={() => {
                setWrapped(!wrapped);
                writeSessionChatCodeWrapDefault(!wrapped);
              }}
              size='icon-xs'
              variant='ghost'
            >
              <IconTextWrap aria-hidden='true' data-icon='inline-start' stroke={1.9} />
            </Button>
          </AppTooltip>
          {openFile && titleReference ? <OpenFenceFileButton openFile={openFile} reference={titleReference} /> : null}
          <AppTooltip content={copyLabel}>
            <Button
              aria-label={copyLabel}
              onClick={() => {
                // Always the fence's own source, never the highlighted markup.
                void navigator.clipboard.writeText(text).then(() => {
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1200);
                });
              }}
              size='icon-xs'
              variant='ghost'
            >
              {copied ? (
                <IconCheck aria-hidden='true' data-icon='inline-start' stroke={1.9} />
              ) : (
                <IconCopy aria-hidden='true' data-icon='inline-start' stroke={1.9} />
              )}
            </Button>
          </AppTooltip>
        </span>
      </div>
      {shikiLanguage === null ? (
        plainBlock
      ) : (
        <ShikiCodeBody code={source} fallback={plainBlock} language={shikiLanguage} />
      )}
    </div>
  );
}

/**
 * An inline-code file reference, as a chip that opens the file. The label is
 * the whole path the agent wrote plus the coordinates they quoted it with.
 *
 * A transcript column is ~385px, so a deep path will not always fit, and the
 * chip is built to lose the right part when it does not: the parent is the
 * only shrinkable box in the row, so it ellipsizes from its tail while the
 * filename and the coordinates stay whole. One line, always — a chip that
 * wrapped would stop reading as a word in a sentence and start reading as a
 * broken box. The tooltip is what gives the elided directories back.
 */
function FileChip({
  openFile,
  reference,
}: {
  openFile: NonNullable<SessionChatHostLinks['openFile']>;
  reference: SessionChatFilePathRef;
}) {
  const { name, parent } = sessionChatFilePathChipLabel(reference);
  const title = sessionChatFilePathTitle(reference);
  const Icon = sessionChatFilePathIcon(reference.basename);
  return (
    <AppTooltip content={title}>
      <button
        className='ghostex-chat-markdown-file-chip'
        onClick={() => openFile(reference.path, reference.position)}
        type='button'
        // The chip can be truncated on screen, so it declares the inline code
        // it stands for: copying a table cell that holds one yields the whole
        // path the agent wrote, however much of it was visible.
        {...{ [SESSION_CHAT_COPY_CODE_ATTRIBUTE]: title }}
        {...{ [SESSION_CHAT_FILE_PATH_ATTRIBUTE]: reference.path }}
      >
        <Icon aria-hidden='true' className='ghostex-chat-markdown-file-chip-icon' size={13} stroke={1.8} />
        {parent === '' ? null : <span className='ghostex-chat-markdown-file-chip-parent'>{parent}</span>}
        <span className='ghostex-chat-markdown-file-chip-name'>{name}</span>
      </button>
    </AppTooltip>
  );
}

function pinMarkdownTableColumnWidths(table: HTMLTableElement): void {
  const columnWidths: number[] = [];
  for (const row of table.rows) {
    [...row.cells].forEach((cell, column) => {
      columnWidths[column] = Math.max(columnWidths[column] ?? 0, cell.getBoundingClientRect().width);
    });
  }
  // The header row is where a column's width is decided, so pinning it
  // there holds every row below it in place.
  [...(table.tHead?.rows[0]?.cells ?? [])].forEach((cell, column) => {
    const width = columnWidths[column] ?? cell.getBoundingClientRect().width;
    cell.style.minWidth = `${Math.round(width)}px`;
  });
}

/**
 * A markdown table, with the chrome that makes an agent-written one readable.
 *
 * Three things happen here that a bare `<table>` cannot do.
 *
 * The table keeps real table layout and the horizontal scrolling moves to a
 * wrapper around it. `display: block` on the table itself buys the same
 * overflow, but it throws away the column algorithm with it, so every column
 * sizes itself independently of the rows below and the result is ragged.
 *
 * Cells wrap by default when anything would clip, and the toggle recaps them
 * so long cells ellipsize. Expanding measures the widest cell in each column
 * and pins that width on the header row: expanding otherwise re-runs the
 * column algorithm against wrapped text and every column jumps somewhere else,
 * which loses the reader's place.
 *
 * Copy offers markdown and CSV, serialized from the rendered table rather than
 * the source, so what lands on the clipboard is what is on screen — chips
 * included (session-chat-table-clipboard.ts).
 */
function MarkdownTable({ children, node: _node, ...props }: ComponentProps<'table'> & ExtraProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const tableRef = useRef<HTMLTableElement | null>(null);
  const userCollapsedRef = useRef(false);
  const [expanded, setExpanded] = useState(false);
  /*
   * Whether this table has anything the toggle can change. A 2x2 table of
   * short cells has nothing clipped and nothing off-screen, so it stays
   * collapsed with no toggle rather than a control that would visibly do
   * nothing. Overflowing tables start expanded (wrapped) instead.
   */
  const [expandable, setExpandable] = useState(false);
  const [copied, setCopied] = useState(false);

  useLayoutEffect(() => {
    // Only measured while collapsed: once expanded, the cap is gone and nothing
    // is clipped, so re-measuring would delete the control that got us here.
    if (expanded) return undefined;
    const scroller = scrollRef.current;
    const table = tableRef.current;
    if (!scroller || !table) return undefined;
    const measure = (): void => {
      const clipped = [...table.querySelectorAll('.ghostex-chat-markdown-table-cell')].some(
        (cell) => cell.scrollWidth > cell.clientWidth + 1
      );
      const needsToggle = clipped || scroller.scrollWidth > scroller.clientWidth + 1;
      setExpandable(needsToggle);
      if (needsToggle && !userCollapsedRef.current) {
        pinMarkdownTableColumnWidths(table);
        setExpanded(true);
      }
    };
    measure();
    // Both, and for different reasons: the scroller changes size when the pane
    // does, and the table changes size when a streaming turn appends a row.
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    observer.observe(table);
    return () => observer.disconnect();
  }, [expanded]);

  const toggleExpanded = useCallback((): void => {
    const table = tableRef.current;
    if (!table) return;
    if (!expanded) {
      pinMarkdownTableColumnWidths(table);
    } else {
      userCollapsedRef.current = true;
    }
    setExpanded(!expanded);
  }, [expanded]);

  const copyTable = useCallback((format: 'csv' | 'markdown'): void => {
    const table = tableRef.current;
    if (!table) return;
    const text = format === 'csv' ? sessionChatTableToCsv(table) : sessionChatTableToMarkdown(table);
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  }, []);

  const expandLabel = expanded ? 'Collapse cells' : 'Expand cells';
  const copyLabel = copied ? 'Copied' : 'Copy table';

  return (
    // data-expanded drives the cell cap in chat.css; the wrapper, not the
    // table, is what the actions sit under and what the scroller lives in.
    <div className='ghostex-chat-markdown-table' data-expanded={expanded ? 'true' : 'false'}>
      <div className='ghostex-chat-markdown-table-scroll' ref={scrollRef}>
        <table {...props} ref={tableRef}>
          {children}
        </table>
      </div>
      <span aria-label='Table actions' className='ghostex-chat-markdown-table-actions' role='toolbar'>
        {expandable || expanded ? (
          <AppTooltip content={expandLabel}>
            <Button
              aria-label={expandLabel}
              aria-pressed={expanded}
              onClick={toggleExpanded}
              size='icon-xs'
              variant='ghost'
            >
              {expanded ? (
                <IconArrowsMinimize aria-hidden='true' stroke={1.9} />
              ) : (
                <IconArrowsMaximize aria-hidden='true' stroke={1.9} />
              )}
            </Button>
          </AppTooltip>
        ) : null}
        <DropdownMenu>
          <AppTooltip content={copyLabel}>
            <DropdownMenuTrigger render={<Button aria-label={copyLabel} size='icon-xs' variant='ghost' />}>
              {copied ? <IconCheck aria-hidden='true' stroke={1.9} /> : <IconCopy aria-hidden='true' stroke={1.9} />}
            </DropdownMenuTrigger>
          </AppTooltip>
          <DropdownMenuContent align='end' className='w-auto min-w-36' side='top' style={{ borderRadius: 8 }}>
            <DropdownMenuItem onClick={() => copyTable('markdown')}>Copy as MD</DropdownMenuItem>
            <DropdownMenuItem onClick={() => copyTable('csv')}>Copy as CSV</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </span>
    </div>
  );
}

/**
 * One table cell. The truncation lives on this inner block rather than on the
 * `<td>` itself because `text-overflow` is not applied to a `display:
 * table-cell` box — the text would clip with no ellipsis, which reads as a
 * rendering bug rather than as "there is more here".
 */
function MarkdownTableCell({
  children,
  node: _node,
  ...props
}: ComponentProps<'td'> & ExtraProps & { as: 'td' | 'th' }) {
  const { as, ...cellProps } = props;
  const Cell = as;
  return (
    <Cell {...cellProps}>
      <span className='ghostex-chat-markdown-table-cell'>{children}</span>
    </Cell>
  );
}

/**
 * A `<details>` an agent wrote (session-chat-details.ts folded the tags out of
 * the mdast; nothing user-authored reached the DOM as markup).
 *
 * The element is the real one rather than a button-plus-region pair, so the
 * disclosure semantics, the Enter/Space handling and the expanded state the
 * screen reader announces all come from the platform. Only the open flag is
 * held in React: a working turn re-renders this subtree on every token, and an
 * uncontrolled `<details>` would be at the mercy of whether reconciliation
 * happens to touch the attribute the reader just toggled.
 */
function MarkdownDetails({ children, node: _node, open, ...props }: ComponentProps<'details'> & ExtraProps) {
  const [isOpen, setIsOpen] = useState(open === true);
  return (
    <details
      {...props}
      className='ghostex-chat-markdown-details'
      onToggle={(event) => setIsOpen(event.currentTarget.open)}
      open={isOpen}
    >
      {children}
    </details>
  );
}

/** The disclosure's own row: the chevron the rest of the chat uses, then the label. */
function MarkdownSummary({ children, node: _node, ...props }: ComponentProps<'summary'> & ExtraProps) {
  return (
    <summary {...props} className='ghostex-chat-markdown-details-summary'>
      <IconChevronRight aria-hidden='true' className='ghostex-chat-markdown-details-chevron' size={14} stroke={2} />
      <span className='ghostex-chat-markdown-details-summary-text'>{children}</span>
    </summary>
  );
}

/** Rendered Markdown counterpart of the Monaco reference decoration. */
function MarkdownReferencePill({
  kind,
  label,
  openFile,
  path,
  position,
}: {
  kind: SessionChatReferenceKind;
  label: string;
  openFile: SessionChatHostLinks['openFile'];
  path: string;
  position?: SessionChatFilePosition;
}) {
  const positionSuffix = sessionChatFilePositionSuffix(position);
  const displayLabel = positionSuffix !== '' && !label.endsWith(positionSuffix) ? `${label}${positionSuffix}` : label;
  const action = openFile ? `Open ${displayLabel}` : `Copy path for ${displayLabel}`;
  const title = `${path}${positionSuffix}`;
  return (
    <AppTooltip content={title}>
      <button
        aria-label={`${action}, ${kind}`}
        className={`ghostex-chat-reference-pill ghostex-chat-reference-pill--${kind}`}
        onClick={() => {
          if (openFile) {
            openFile(path, position);
          } else {
            void navigator.clipboard.writeText(path);
          }
        }}
        type='button'
        {...{ [SESSION_CHAT_FILE_PATH_ATTRIBUTE]: path }}
      >
        {displayLabel}
      </button>
    </AppTooltip>
  );
}

function markdownComponents(
  viewer: SessionChatImageViewerApi | null,
  hostLinks: SessionChatHostLinks | null
): Components {
  return {
    details: MarkdownDetails,
    pre: MarkdownCodeBlock,
    summary: MarkdownSummary,
    table: MarkdownTable,
    td: (props) => <MarkdownTableCell {...props} as='td' />,
    th: (props) => <MarkdownTableCell {...props} as='th' />,
    ol: ({ children, node, start, style, ...props }) => {
      const items = node?.children.filter((child) => child.type === 'element' && child.tagName === 'li').length ?? 0;
      const gutter = sessionChatOrderedListGutter(items, start);
      return (
        <ol {...props} start={start} style={gutter ? { ...style, ...gutter } : style}>
          {children}
        </ol>
      );
    },
    code: ({ children, className, node, ...props }) => {
      const plain = (
        <code {...props} className={className}>
          {children}
        </code>
      );
      /*
       * Only inline code can become a chip, and only on a host that has an
       * editor to open it in. A fence's <code> arrives here too but is never
       * tagged by the remark plugin, so the Shiki path is untouched; and on the
       * web app or the phone, where openFile is absent, a path stays exactly
       * the grey span it is today rather than a chip that would do nothing.
       */
      const openFile = hostLinks?.openFile;
      if (!openFile || node?.properties?.dataInlineCode == null) {
        return plain;
      }
      const reference = resolveSessionChatInlineCodeFilePath(nodeText(children));
      if (!reference) {
        return plain;
      }
      return <FileChip openFile={openFile} reference={reference} />;
    },
    blockquote: ({ children, node: _node, ...props }) => {
      const alert = alertPresentation((props as Record<string, unknown>)['data-alert']);
      if (!alert) {
        return <blockquote {...props}>{children}</blockquote>;
      }
      // Deliberately not a <blockquote>: quotes are muted, and an alert's body
      // is ordinary text sitting under a coloured title — only the border and
      // the title carry the colour.
      return (
        <div className='ghostex-chat-markdown-alert' data-alert={alert.kind} role='note'>
          <div className='ghostex-chat-markdown-alert-title'>
            <alert.Icon aria-hidden size={15} stroke={2} />
            {alert.label}
          </div>
          {children}
        </div>
      );
    },
    a: ({ children, href, node, ...props }) => {
      if (typeof href !== 'string' || href === '') {
        return <>{children}</>;
      }
      /*
       * A GFM footnote reference and the ↩ that comes back from its definition
       * are the one kind of chat link that points inside the message itself, so
       * they are settled before the host routing below: that classifies a "#…"
       * href as nothing the host can open and renders inert text, which cost
       * the jump and the data attributes the chip styling hangs off.
       *
       * The jump is done here rather than by letting the anchor follow its
       * fragment, for two reasons. Fragment navigation would push a history
       * entry and rewrite the URL of the page the whole app is running in; and
       * the ids remark mints are per-message ("user-content-fn-1"), so in a
       * transcript of many footnoted answers document.getElementById would
       * land on the first message's footnote. Resolving inside the message
       * that was clicked is the only reading of it that is always right.
       */
      const properties = node?.properties;
      if (properties?.dataFootnoteRef != null || properties?.dataFootnoteBackref != null) {
        const footnoteId = href.startsWith('#') ? href.slice(1) : null;
        return (
          // Spread rather than picked apart: remark puts the data attribute the
          // chip styling matches on, the id the ↩ comes back to, and the
          // aria-label that names it, all on this one anchor.
          <a
            {...props}
            href={href}
            onClick={(event) => {
              event.preventDefault();
              if (!footnoteId) {
                return;
              }
              const message = event.currentTarget.closest('.ghostex-chat-markdown');
              message?.querySelector(`#${CSS.escape(footnoteId)}`)?.scrollIntoView({ block: 'center' });
            }}
          >
            {children}
          </a>
        );
      }
      if (isSessionChatImageHref(href)) {
        return (
          <SessionChatImageReference
            className='mx-0.5 align-middle'
            label={nodeText(children).trim() || 'Image'}
            target={sessionChatImageTargetForHref(href)}
          />
        );
      }
      const target = classifySessionChatLinkHref(href);
      if (target.kind === 'file') {
        const label = nodeText(children).trim() || target.path;
        const position = sessionChatFilePositionFromHref(href);
        return (
          <MarkdownReferencePill
            kind={sessionChatReferenceKind(label, target.path)}
            label={label}
            openFile={hostLinks?.openFile}
            path={target.path}
            position={position}
          />
        );
      }
      if (target.kind === 'url') {
        const openUrl = hostLinks?.openUrl;
        if (openUrl) {
          return (
            <AppTooltip content={target.url}>
              <a
                // Kept an anchor so the URL shows in the status bar; the host
                // owns ordinary clicks and both explicit context-menu opens.
                href={target.url}
                {...{ [SESSION_CHAT_WEB_URL_ATTRIBUTE]: target.url }}
                onClick={(event) => {
                  event.preventDefault();
                  openUrl(target.url, { external: event.shiftKey });
                }}
              >
                <IconLink aria-hidden='true' className='ghostex-chat-markdown-link-icon' size={13} stroke={1.8} />
                {children}
              </a>
            </AppTooltip>
          );
        }
        return (
          <a href={target.url} rel='noreferrer' target='_blank'>
            <IconLink aria-hidden='true' className='ghostex-chat-markdown-link-icon' size={13} stroke={1.8} />
            {children}
          </a>
        );
      }
      return <span>{children}</span>;
    },
    img: ({ alt, src }) => {
      if (viewer && typeof src === 'string' && src !== '') {
        const target = {
          ...sessionChatImageTargetForHref(src),
          ...(alt ? { alt } : {}),
        };
        if (viewer.canOpen(target)) {
          // An image the agent wrote as an image renders as one; the named
          // button stays as the stand-in when its bytes cannot be read.
          return <SessionChatInlineImage fallback={<span>{alt || 'Image'}</span>} target={target} />;
        }
      }
      return <img alt={alt ?? ''} src={src ?? ''} />;
    },
  };
}

export function SessionChatMarkdown({
  chatText = false,
  isStreaming = false,
  markdown,
}: {
  /**
   * True when this body is text somebody typed into a composer rather than
   * markdown an agent authored. Typed newlines then render as line breaks and
   * a quote ends where the author stopped typing `>`, which is what those
   * keystrokes meant in a chat box (session-chat-user-text.ts). Everything
   * else about the render — GFM, fences, links, chips — is identical.
   */
  chatText?: boolean;
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
  const components = useMemo(() => markdownComponents(viewer, hostLinks), [hostLinks, viewer]);
  // Both authors hit the same CommonMark rule: an ordered list that does not
  // start at 1 cannot interrupt a paragraph, so "heading line:\n5. item" would
  // render as one run-on paragraph (session-chat-list-interrupt.ts).
  const listSafe = sessionChatListInterruptSource(markdown);
  const source = chatText ? sessionChatUserMarkdownSource(listSafe) : listSafe;
  return (
    <SessionChatMarkdownStreamingContext value={isStreaming}>
      <div className='ghostex-chat-markdown'>
        <ReactMarkdown components={components} remarkPlugins={chatText ? CHAT_TEXT_REMARK_PLUGINS : REMARK_PLUGINS}>
          {source}
        </ReactMarkdown>
      </div>
    </SessionChatMarkdownStreamingContext>
  );
}
