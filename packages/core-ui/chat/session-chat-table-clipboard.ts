/**
 * Turning a rendered chat table back into text.
 *
 * The table the reader is looking at is the source of truth, not the markdown
 * that produced it: by the time it is on screen the renderer has resolved
 * alignment, and cells may hold nodes that never existed in the source (a
 * file-path chip, a host link button). Reading the DOM is therefore the only
 * way to copy exactly what is shown.
 *
 * Two shapes come out of it, and they want different things. Markdown is a GFM
 * pipe table for pasting back into a prompt or a document, so it keeps the
 * inline markers. CSV is for a spreadsheet, so it keeps the cells' plain text
 * and nothing else. One walker serves both; `plain` is the only difference.
 *
 * Nodes that exist only for presentation are dropped either way: decorative
 * icons, the `aria-hidden` glyphs the chips carry, form controls. A node whose
 * rendered label is an abbreviation of its real text says so with
 * `data-chat-copy-code` — that is how a file-path chip copies the full
 * `path:line:col` the agent wrote rather than the basename it shows.
 */

/** Elements whose text is chrome, never content. */
const SKIPPED_TAGS: ReadonlySet<string> = new Set(['INPUT', 'SCRIPT', 'STYLE', 'TEMPLATE']);

/**
 * Marks a node that stands for a span of inline code whose source text is the
 * attribute's value. Markdown copies it fenced in backticks (it was inline code
 * in the source, and it has to survive a round trip as one); CSV copies the
 * value bare.
 */
export const SESSION_CHAT_COPY_CODE_ATTRIBUTE = 'data-chat-copy-code';

function isSkippedElement(element: Element): boolean {
  return (
    SKIPPED_TAGS.has(element.tagName) || element.localName === 'svg' || element.getAttribute('aria-hidden') === 'true'
  );
}

/**
 * Fences inline code with one more backtick than the longest run inside it, and
 * pads when the text itself starts or ends with a backtick — the same rule
 * CommonMark uses to read it back.
 */
function wrapInlineCode(code: string): string {
  const longestRun = (code.match(/`+/g) ?? []).reduce((longest, run) => Math.max(longest, run.length), 0);
  const fence = '`'.repeat(longestRun + 1);
  const pad = code.startsWith('`') || code.endsWith('`') ? ' ' : '';
  return `${fence}${pad}${code}${pad}${fence}`;
}

/** Hoists surrounding whitespace outside the markers: "` a `" -> " **a** ". */
function wrapInlineMarker(content: string, marker: string): string {
  const match = /^(\s*)([\s\S]*?)(\s*)$/.exec(content);
  const core = match?.[2] ?? '';
  if (core === '') return content;
  return `${match?.[1] ?? ''}${marker}${core}${marker}${match?.[3] ?? ''}`;
}

function serializeAnchor(anchor: Element, plain: boolean): string {
  const label = serializeChildren(anchor, plain).trim();
  const href = anchor.getAttribute('href') ?? '';
  if (plain || label === '' || href === '' || href === label) return label;
  return `[${label}](${href})`;
}

function serializeNode(node: Node, plain: boolean): string {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent ?? '';
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return '';
  const element = node as Element;
  // A node that knows its own source text wins over anything below, which is
  // what lets a chip copy the path instead of its abbreviated label.
  const declaredCode = element.getAttribute(SESSION_CHAT_COPY_CODE_ATTRIBUTE);
  if (declaredCode !== null) {
    return plain ? declaredCode : wrapInlineCode(declaredCode);
  }
  if (isSkippedElement(element)) return '';

  switch (element.tagName) {
    case 'BR':
      return ' ';
    case 'CODE': {
      const code = element.textContent ?? '';
      return plain ? code : wrapInlineCode(code);
    }
    case 'STRONG':
    case 'B':
      return plain ? serializeChildren(element, plain) : wrapInlineMarker(serializeChildren(element, plain), '**');
    case 'EM':
    case 'I':
      return plain ? serializeChildren(element, plain) : wrapInlineMarker(serializeChildren(element, plain), '*');
    case 'DEL':
    case 'S':
      return plain ? serializeChildren(element, plain) : wrapInlineMarker(serializeChildren(element, plain), '~~');
    case 'A':
      return serializeAnchor(element, plain);
    case 'IMG': {
      const alt = element.getAttribute('alt') ?? '';
      const src = element.getAttribute('src') ?? '';
      if (plain) return alt;
      return alt !== '' && src !== '' ? `![${alt}](${src})` : alt;
    }
    default:
      // Includes the host-link and image-link buttons: their label is the
      // cell's text, and dropping it would lose the cell.
      return serializeChildren(element, plain);
  }
}

function serializeChildren(node: Node, plain: boolean): string {
  let out = '';
  for (const child of node.childNodes) {
    out += serializeNode(child, plain);
  }
  return out;
}

/** One cell, on one line: the renderer's own line breaks are not content. */
function cellText(cell: Element, plain: boolean): string {
  return serializeChildren(cell, plain).replace(/\s+/g, ' ').trim();
}

/** A literal pipe would end the cell, so it has to be escaped. */
function markdownCell(cell: Element): string {
  return cellText(cell, false).replaceAll('|', '\\|');
}

function csvCell(cell: Element): string {
  const value = cellText(cell, true);
  return /["\n,]/.test(value) ? `"${value.replaceAll('"', '""')}"` : value;
}

/** Rows of this table only — a table nested inside a cell is not our business. */
function tableRows(table: Element): Element[] {
  return [...table.querySelectorAll(':scope > thead > tr, :scope > tbody > tr, :scope > tfoot > tr, :scope > tr')];
}

function rowCells(row: Element): Element[] {
  return [...row.children].filter((cell) => cell.tagName === 'TH' || cell.tagName === 'TD');
}

/**
 * GFM carries alignment in the delimiter row, and remark-gfm put it on the
 * cells as an inline `text-align`, so it survives the round trip.
 */
function alignmentMarker(cell: Element): string {
  const align = (cell as HTMLElement).style?.textAlign || cell.getAttribute('align') || '';
  if (align === 'center') return ':---:';
  if (align === 'right') return '---:';
  return '---';
}

export function sessionChatTableToMarkdown(table: Element): string {
  const lines: string[] = [];
  let wroteDelimiter = false;
  for (const row of tableRows(table)) {
    const cells = rowCells(row);
    if (cells.length === 0) continue;
    lines.push(`| ${cells.map(markdownCell).join(' | ')} |`);
    if (!wroteDelimiter) {
      lines.push(`| ${cells.map(alignmentMarker).join(' | ')} |`);
      wroteDelimiter = true;
    }
  }
  return lines.join('\n');
}

export function sessionChatTableToCsv(table: Element): string {
  const lines: string[] = [];
  for (const row of tableRows(table)) {
    const cells = rowCells(row);
    if (cells.length === 0) continue;
    lines.push(cells.map(csvCell).join(','));
  }
  return lines.join('\n');
}
