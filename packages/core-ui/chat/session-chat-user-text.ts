/*
 * Chat-typed text, rendered the way it was typed.
 *
 * A user turn is not a markdown document. It is what somebody typed into a
 * composer that submits on Enter and inserts a newline on Shift+Enter, so every
 * newline in it is a line the author decided to end. CommonMark reads that text
 * with two rules that are right for documents and wrong for a chat box:
 *
 *   - a single newline inside a paragraph is a space, so a typed line break
 *     disappears; and
 *   - "lazy continuation" lets an unprefixed line continue the block above it,
 *     so `> quoted` followed by `ordinary` swallows the ordinary line into the
 *     quote.
 *
 * Both are corrected here, and only for user-authored text — an agent's answer
 * is real markdown, written by something that knows what a blank line means, so
 * it keeps standard GFM semantics.
 *
 * The stylesheet used to fake the first half of this with `white-space:
 * pre-wrap` on the bubble's paragraphs, list items and quotes. That paints the
 * author's newlines, but it also paints the structural ones react-markdown puts
 * *between* block children — the `"\n"` text nodes that sit around the `<p>` of
 * a loose list item and inside a `<blockquote>` — so a numbered list rendered
 * its marker on one line and the item's text on the next, and a quote stood a
 * couple of blank lines taller than its own text. Real `<br>` nodes cost none
 * of that, because the structural newlines stay collapsible whitespace.
 */

/** The subset of mdast this file reads and writes. */
interface MarkdownAstNode {
  children?: MarkdownAstNode[];
  type?: string;
  value?: unknown;
}

/** A newline plus the spaces or tabs in front of it, which the break replaces. */
const TEXT_NEWLINE = /[\t ]*(?:\r?\n|\r)/g;

/** ```` ``` ```` or `~~~`, opening or closing a fence, indented up to three. */
const FENCE_LINE = /^ {0,3}(`{3,}|~{3,})(.*)$/;

/** A line the author marked as quoted, at any nesting depth. */
const QUOTE_LINE = /^ {0,3}>/;

/**
 * The source a user turn is parsed from: the same text, with a blank line
 * inserted wherever a quote is followed by a line the author did not quote.
 *
 * That blank line is exactly what ends a blockquote in CommonMark, so
 * `> a\nb` becomes `> a\n\nb` and renders as a quote holding `a` and an
 * ordinary paragraph holding `b` — one typed line, one block, which is what
 * somebody who typed `>` on one line and not the next meant.
 *
 * Fenced code is skipped: a `>` inside a fence is content, not a quote, and a
 * blank line inserted into a fence would show up in the code.
 *
 * Returns the input itself when there is nothing to change, so the common turn
 * (no quotes at all) hands the very same string to the parser.
 */
export function sessionChatUserMarkdownSource(markdown: string): string {
  if (!markdown.includes(">")) return markdown;

  const lines = markdown.split("\n");
  const output: string[] = [];
  let fence: string | null = null;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? "";
    output.push(line);

    const fenceMatch = FENCE_LINE.exec(line);
    if (fenceMatch) {
      const marker = fenceMatch[1] ?? "";
      if (fence === null) {
        fence = marker[0] ?? null;
      } else if (
        marker[0] === fence &&
        // A closing fence is at least as long as the opening one and carries
        // no info string.
        (fenceMatch[2] ?? "").trim() === ""
      ) {
        fence = null;
      }
      continue;
    }
    if (fence !== null) continue;

    const next = lines[index + 1];
    if (next === undefined) continue;
    if (!QUOTE_LINE.test(line)) continue;
    // A blank line already ends the quote, and a quoted line continues it.
    if (next.trim() === "" || QUOTE_LINE.test(next)) continue;
    output.push("");
  }
  return output.join("\n");
}

/**
 * Turns every newline the author typed inside a paragraph into a hard break,
 * the way a chat client does. Structural whitespace is untouched: it lives
 * between block nodes, not inside the text of one, so nothing here can see it.
 *
 * Code keeps its own newlines — a fence is a `code` node and a span is
 * `inlineCode`, neither of which holds `text` children — and so does raw HTML.
 */
export function remarkSessionChatHardBreaks() {
  return (tree: MarkdownAstNode): void => {
    const visit = (node: MarkdownAstNode): void => {
      const children = node.children;
      if (!children) return;
      const rebuilt: MarkdownAstNode[] = [];
      let changed = false;
      for (const child of children) {
        if (child.type !== "text" || typeof child.value !== "string") {
          visit(child);
          rebuilt.push(child);
          continue;
        }
        const value = child.value;
        const pattern = new RegExp(TEXT_NEWLINE.source, "g");
        let cursor = 0;
        let match = pattern.exec(value);
        if (match === null) {
          rebuilt.push(child);
          continue;
        }
        changed = true;
        while (match !== null) {
          if (match.index > cursor) {
            rebuilt.push({ type: "text", value: value.slice(cursor, match.index) });
          }
          rebuilt.push({ type: "break" });
          cursor = match.index + match[0].length;
          match = pattern.exec(value);
        }
        if (cursor < value.length) {
          rebuilt.push({ type: "text", value: value.slice(cursor) });
        }
      }
      if (changed) node.children = rebuilt;
    };
    visit(tree);
  };
}
