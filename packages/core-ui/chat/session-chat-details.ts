/*
 * `<details>` / `<summary>` in a chat turn, as a real collapsible.
 *
 * Agents reach for a disclosure whenever an answer carries something long they
 * do not want in the reader's face — a failing test's output, a full diff, the
 * env dump behind a one-line conclusion:
 *
 *     <details>
 *     <summary>Show the failing output</summary>
 *
 *     ...body...
 *     </details>
 *
 * react-markdown renders raw HTML as literal text unless `rehype-raw` is in the
 * chain, so until now that printed the tags on screen and left the body
 * permanently expanded.
 *
 * ## Why this is not rehype-raw
 *
 * The obvious fix is `[rehypeRaw, [rehypeSanitize, schema]]`, and it is the
 * wrong one here for two independent reasons.
 *
 * Size: rehype-raw re-parses the document with parse5, which measured at
 * +198 KB minified (+54 KB gzipped) on top of react-markdown + remark-gfm —
 * more than doubling the markdown pipeline. This page ships as one inlined
 * `file://` script in gpui and as a single self-contained ~966 KB `index.html`
 * in the mobile webview, so that is roughly a fifth added to the phone's whole
 * chat page to render one element.
 *
 * Safety: a transcript is untrusted input. It carries text an agent read off a
 * web page, out of a repository, or back from a tool, and the gpui chat page is
 * a privileged CEF document with a host bridge on it. Turning that text into
 * live DOM and then subtracting the dangerous parts with a sanitizer means the
 * blast radius of any mistake in the schema — or of any future edit to it — is
 * script execution inside the host. Nothing here ever becomes markup: the
 * scanner below reads two tag names out of the mdast and builds ordinary nodes,
 * so there is no sanitizer to get wrong.
 *
 * ## What that means for other HTML — deliberately unchanged
 *
 * `<details>` and `<summary>` are the only tags this recognises. Every other
 * tag an agent writes still renders as the literal text it renders as today:
 * `<script>alert(1)</script>` is five words on screen, `<img onerror=...>` is
 * text, `<iframe>` is text. That is not a fallback, it is the contract — the
 * renderer has no raw-HTML mode to opt in to, on any host.
 *
 * ## The one thing that does get re-read as markdown
 *
 * CommonMark ends an HTML block at a blank line, so an agent who writes the
 * body with no blank line after `</summary>` hands us the whole disclosure as
 * one `html` node, body included. That body is prose the agent wrote, not
 * markup, so a stretch of text sitting *inside* a `<details>` is parsed as
 * markdown rather than dumped as text — which is what makes a bare list or
 * fence in that form render at all. Text outside a `<details>` is never
 * touched.
 */

/** The subset of mdast this file reads and writes. */
interface MarkdownAstNode {
  children?: MarkdownAstNode[];
  data?: {
    hName?: string;
    hProperties?: Record<string, unknown>;
  };
  type?: string;
  value?: unknown;
}

/** The markdown parser, taken off the processor this plugin is attached to. */
interface MarkdownParser {
  parse(value: string): MarkdownAstNode;
}

/** Cheap "is it worth scanning this node at all" test. */
const DETAILS_HINT = /<\/?(?:details|summary)\b/i;

/**
 * Every `<details>`, `</details>`, `<summary>` and `</summary>` tag, matched
 * one at a time so the text between them can be kept. Open and close tags are
 * separate tokens rather than one paired match because an agent's summary and
 * its body are often split across mdast nodes by a blank line.
 */
const DETAILS_TOKEN =
  /<details(\s[^>]*?)?\s*>|<\/details\s*>|<summary(?:\s[^>]*?)?\s*>|<\/summary\s*>/gi;

/** `open`, `open=""`, `open="open"` — HTML's three spellings of the same flag. */
const OPEN_ATTRIBUTE = /(?:^|\s)open(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]*))?(?=\s|$)/i;

type DetailsToken =
  | { attributes: string; kind: "details-open" }
  | { kind: "details-close" }
  | { kind: "raw"; text: string }
  | { kind: "summary-close" }
  | { kind: "summary-open" };

/**
 * Splits one `html` node's text into disclosure tags and the text between them.
 * Returns an empty list when the node holds no disclosure tag at all, which is
 * the caller's signal to leave that node exactly as it found it.
 */
function scanDisclosureTags(value: string): DetailsToken[] {
  const tokens: DetailsToken[] = [];
  const pattern = new RegExp(DETAILS_TOKEN.source, "gi");
  let cursor = 0;
  let match = pattern.exec(value);
  while (match !== null) {
    if (match.index > cursor) {
      tokens.push({ kind: "raw", text: value.slice(cursor, match.index) });
    }
    cursor = match.index + match[0].length;
    const tag = match[0].toLowerCase();
    if (tag.startsWith("</details")) {
      tokens.push({ kind: "details-close" });
    } else if (tag.startsWith("</summary")) {
      tokens.push({ kind: "summary-close" });
    } else if (tag.startsWith("<details")) {
      tokens.push({ attributes: match[1] ?? "", kind: "details-open" });
    } else {
      tokens.push({ kind: "summary-open" });
    }
    match = pattern.exec(value);
  }
  if (tokens.length > 0 && cursor < value.length) {
    tokens.push({ kind: "raw", text: value.slice(cursor) });
  }
  return tokens;
}

/** One `<details>` being built: its flag, its summary so far, its body so far. */
interface DetailsFrame {
  body: MarkdownAstNode[];
  open: boolean;
  summary: MarkdownAstNode[];
  summaryOpen: boolean;
  summarySeen: boolean;
}

/**
 * The node the renderer sees. Custom types on purpose: mdast-util-to-hast's
 * unknown handler still applies `data.hName`/`data.hProperties`, so these
 * become `<details>`, `<summary>` and the body wrapper without borrowing a
 * standard node type that the other remark plugins in the chain inspect.
 */
function detailsNode(frame: DetailsFrame): MarkdownAstNode {
  // A summary that is a single paragraph is unwrapped, so `<summary>` holds
  // phrasing content the way the HTML spec expects rather than a stray block.
  const summary =
    frame.summary.length === 1 && frame.summary[0]?.type === "paragraph"
      ? (frame.summary[0]?.children ?? [])
      : frame.summary;
  return {
    children: [
      {
        // An agent who opens a disclosure and never names it still gets a
        // clickable row; a blank summary would be an invisible control.
        children: summary.length > 0 ? summary : [{ type: "text", value: "Details" }],
        data: { hName: "summary" },
        type: "sessionChatDetailsSummary",
      },
      {
        children: frame.body,
        data: {
          hName: "div",
          hProperties: { className: ["ghostex-chat-markdown-details-body"] },
        },
        type: "sessionChatDetailsBody",
      },
    ],
    data: {
      hName: "details",
      // `open` is the only attribute carried across; everything else an agent
      // wrote on the tag is dropped rather than reproduced.
      hProperties: frame.open ? { open: true } : {},
    },
    type: "sessionChatDetails",
  };
}

/**
 * Rewrites one parent's children, turning disclosure tags into `details` nodes
 * and leaving everything else — including every other scrap of HTML — alone.
 */
function foldDisclosures(
  children: MarkdownAstNode[],
  parse: (value: string) => MarkdownAstNode[],
): MarkdownAstNode[] {
  const hasTag = children.some(
    (child) =>
      child.type === "html" &&
      typeof child.value === "string" &&
      DETAILS_HINT.test(child.value),
  );
  if (!hasTag) return children;

  const output: MarkdownAstNode[] = [];
  const stack: DetailsFrame[] = [];

  const emit = (node: MarkdownAstNode): void => {
    const frame = stack[stack.length - 1];
    if (!frame) {
      output.push(node);
      return;
    }
    (frame.summaryOpen ? frame.summary : frame.body).push(node);
  };

  const emitRaw = (text: string): void => {
    if (text.trim() === "") return;
    if (stack.length === 0) {
      // Outside a disclosure this is still whatever the agent typed, and it
      // keeps rendering as the literal text it renders as today.
      output.push({ type: "html", value: text });
      return;
    }
    for (const node of parse(text)) emit(node);
  };

  for (const child of children) {
    if (child.type !== "html" || typeof child.value !== "string") {
      emit(child);
      continue;
    }
    const tokens = scanDisclosureTags(child.value);
    if (tokens.length === 0) {
      // No disclosure tag in this node: hand it through untouched, so its HTML
      // reaches the renderer as text exactly as before.
      emit(child);
      continue;
    }
    for (const token of tokens) {
      switch (token.kind) {
        case "details-open":
          stack.push({
            body: [],
            open: OPEN_ATTRIBUTE.test(token.attributes),
            summary: [],
            summaryOpen: false,
            summarySeen: false,
          });
          break;
        case "details-close": {
          const frame = stack.pop();
          if (!frame) {
            // A close with nothing open is a typo, not a disclosure.
            output.push({ type: "html", value: "</details>" });
            break;
          }
          emit(detailsNode(frame));
          break;
        }
        case "summary-open": {
          const frame = stack[stack.length - 1];
          if (!frame || frame.summarySeen) {
            emitRaw("<summary>");
            break;
          }
          frame.summaryOpen = true;
          frame.summarySeen = true;
          break;
        }
        case "summary-close": {
          const frame = stack[stack.length - 1];
          if (!frame?.summaryOpen) {
            emitRaw("</summary>");
            break;
          }
          frame.summaryOpen = false;
          break;
        }
        case "raw":
          emitRaw(token.text);
          break;
      }
    }
  }

  // A turn that is still streaming has written `<details>` but not yet its
  // closing tag. Closing the open frames here renders the disclosure the agent
  // is in the middle of writing instead of showing its opening tag as text
  // until the last token lands.
  while (stack.length > 0) {
    const frame = stack.pop();
    if (frame) emit(detailsNode(frame));
  }

  return output;
}

export function remarkSessionChatDetails(this: MarkdownParser) {
  const parse = (value: string): MarkdownAstNode[] =>
    this.parse(value).children ?? [];
  return (tree: MarkdownAstNode): void => {
    const visit = (node: MarkdownAstNode): void => {
      if (!node.children) return;
      // Depth first: a disclosure nested in a list item is folded before the
      // level above it is rebuilt, so the rebuilt level keeps the folded child.
      node.children.forEach(visit);
      node.children = foldDisclosures(node.children, parse);
    };
    visit(tree);
  };
}
