/*
 * GitHub's blockquote alerts: a quote whose first line is `[!NOTE]` — or TIP,
 * IMPORTANT, WARNING, CAUTION — renders as a titled callout. They are GitHub's
 * own extension rather than GFM, so remark-gfm leaves the marker sitting in the
 * quote as literal text. Agents write them constantly, so this lifts the marker
 * off the mdast onto the blockquote as a `data-alert` attribute for the
 * renderer to style, and drops the marker line itself.
 *
 * Only a marker alone on the quote's first line counts, which is GitHub's own
 * rule: `> [!NOTE] aside` stays an ordinary quote.
 */

interface MarkdownAstNode {
  children?: MarkdownAstNode[];
  data?: {
    hProperties?: Record<string, unknown>;
  };
  type?: string;
  value?: unknown;
}

export const SESSION_CHAT_ALERT_KINDS = ['note', 'tip', 'important', 'warning', 'caution'] as const;

export type SessionChatAlertKind = (typeof SESSION_CHAT_ALERT_KINDS)[number];

const ALERT_MARKER = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\](?:\r?\n|$)/i;

function readAlertMarker(node: MarkdownAstNode): void {
  if (node.type !== 'blockquote') return;
  const paragraph = node.children?.[0];
  const text = paragraph?.children?.[0];
  if (paragraph?.type !== 'paragraph' || text?.type !== 'text' || typeof text.value !== 'string') {
    return;
  }
  const match = ALERT_MARKER.exec(text.value);
  if (!match?.[1]) return;

  const remainder = text.value.slice(match[0].length);
  // Whether the marker's own line ended inside this text node. Without it an
  // empty remainder is ambiguous: `[!NOTE]\n**bold**` parses as
  // [text "[!NOTE]\n", strong] — next-line content as a sibling — while
  // `[!NOTE]*aside*` parses the same way minus the newline.
  const markerEndsItsLine = match[0].endsWith('\n');
  if (remainder.length > 0) {
    // The quote continues on the next line inside this same text node: keep the
    // paragraph, shed the marker line.
    text.value = remainder;
  } else if (markerEndsItsLine || paragraph.children?.length === 1) {
    // The marker was the whole text node — a marker-only paragraph, or a marker
    // line whose next line starts as a sibling inline. Drop the node, and the
    // paragraph with it if that empties it.
    paragraph.children?.shift();
    if (paragraph.children?.length === 0) {
      node.children?.shift();
    }
  } else {
    // Something else shares the marker's line — `[!NOTE]*aside*` — which GitHub
    // does not read as an alert.
    return;
  }

  node.data = {
    ...node.data,
    hProperties: {
      ...node.data?.hProperties,
      dataAlert: match[1].toLowerCase(),
    },
  };
}

export function remarkSessionChatGithubAlerts() {
  return (tree: MarkdownAstNode) => {
    const visit = (node: MarkdownAstNode) => {
      node.children?.forEach(visit);
      readAlertMarker(node);
    };
    visit(tree);
  };
}
