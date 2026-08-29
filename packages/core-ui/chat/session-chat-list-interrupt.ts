/*
 * Ordered lists that CommonMark refuses to start.
 *
 * CommonMark lets an ordered list interrupt a paragraph only when it starts
 * with `1.` — "Whole regions not built:" followed directly by "5. …" parses as
 * one paragraph, and every numbered line after it becomes a lazy continuation
 * joined by soft breaks, so the whole list renders as a single run-on
 * paragraph. Agents write exactly that shape whenever they continue a
 * numbering across messages or headings, and the terminal transcript they were
 * mirroring showed a list, so the chat must too.
 *
 * The correction is the source-level twin of the quote fix in
 * session-chat-user-text.ts: insert the blank line CommonMark needs, and only
 * in the exact failing case. A `1.` line needs nothing (it already
 * interrupts), a numbered line under another numbered line needs nothing (the
 * list is already open, and a blank line there would only make it loose), and
 * anything inside a fence is code, not a list.
 */

/** ```` ``` ```` or `~~~`, opening or closing a fence, indented up to three. */
const FENCE_LINE = /^ {0,3}(`{3,}|~{3,})(.*)$/;

/**
 * An ordered-list item line with content: up to three spaces of indent, a
 * CommonMark-sized number, either delimiter, then at least one space and some
 * text. An empty item ("5." alone) is excluded — CommonMark will not let one
 * interrupt a paragraph even when it starts with 1, and this pass keeps to
 * cases where the list reading is unambiguous.
 */
const ORDERED_ITEM_LINE = /^ {0,3}(\d{1,9})[.)][ \t]+\S/;

/**
 * The source a chat body is parsed from: the same text, with a blank line
 * inserted wherever an ordered-list item that CommonMark cannot start sits
 * directly under a line of ordinary text.
 *
 * Returns the input itself when there is nothing to change, so the common body
 * hands the very same string to the parser.
 */
export function sessionChatListInterruptSource(markdown: string): string {
  const lines = markdown.split('\n');
  const output: string[] = [];
  let changed = false;
  let fence: string | null = null;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? '';

    const fenceMatch = FENCE_LINE.exec(line);
    if (fenceMatch) {
      const marker = fenceMatch[1] ?? '';
      if (fence === null) {
        fence = marker[0] ?? null;
      } else if (
        marker[0] === fence &&
        // A closing fence is at least as long as the opening one and carries
        // no info string.
        (fenceMatch[2] ?? '').trim() === ''
      ) {
        fence = null;
      }
      output.push(line);
      continue;
    }
    if (fence !== null) {
      output.push(line);
      continue;
    }

    const previous = lines[index - 1];
    const number = ORDERED_ITEM_LINE.exec(line)?.[1];
    if (
      number !== undefined &&
      number !== '1' &&
      previous !== undefined &&
      previous.trim() !== '' &&
      !ORDERED_ITEM_LINE.test(previous)
    ) {
      output.push('');
      changed = true;
    }
    output.push(line);
  }
  return changed ? output.join('\n') : markdown;
}
