// @ts-nocheck
import { StateField } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';
import { Decoration, EditorView, WidgetType } from '@codemirror/view';
import { undo, redo } from '@codemirror/commands';
import { ImageWidget } from './images';
import { emojiData } from './emoji';
import { parseKbdTagAt } from './kbd';
import { createLatexMathElement, parseLatexMathAt } from './math';
import { isPrimaryModifierPointerClick } from './linkNavigation';
import { wikiLinkScheme } from './wikiLinks';
import { normalizeSourceHref } from './rawUrls';
import type { EditorDiagnostic } from './diagnostics';

declare global {
  interface HTMLDivElement {
    _meoTableResizeObserver?: ResizeObserver;
  }
}

interface TableData {
  rows: string[][];
  alignments: string[];
  colCount: number;
  startLine?: number;
  endLine?: number;
  indent?: string;
  signature?: string;
  from?: number;
  to?: number;
  headerCells?: string[];
  diagnostics?: TableCellDiagnostics[][][];
  sourceRanges?: TableCellRange[][];
}

interface RowEntry {
  row: HTMLTableRowElement;
  inputs: HTMLTextAreaElement[];
}

interface DomRefs {
  headerInputs: HTMLTextAreaElement[];
  rowInputs: HTMLTextAreaElement[][];
  allRowInputs: HTMLTextAreaElement[][];
  table: HTMLTableElement;
  tbody: HTMLTableSectionElement;
  container: HTMLElement;
  shell: HTMLElement;
  wrap: HTMLElement;
  toolbar: HTMLElement;
  cellGrid: HTMLTableCellElement[][];
  rowEntries: RowEntry[];
  colEls: HTMLElement[];
  sourceBodyRows: HTMLTableRowElement[];
  sourceBodyRowInputs: HTMLTextAreaElement[][];
  sourceBodyCellGrid: HTMLTableCellElement[][];
  sortButtons: HTMLButtonElement[];
  applySortButton: HTMLButtonElement;
  toolbarButtons: {
    insertRowAbove: HTMLButtonElement;
    insertRowBelow: HTMLButtonElement;
    deleteRow: HTMLButtonElement;
    insertColumnLeft: HTMLButtonElement;
    insertColumnRight: HTMLButtonElement;
    deleteColumn: HTMLButtonElement;
  };
}

interface CellCoords {
  row: number;
  col: number;
}

interface SelectionRange {
  fromRow: number;
  toRow: number;
  fromCol: number;
  toCol: number;
}

interface CellMatrix {
  headerCells: string[];
  rows: string[][];
  alignments?: string[];
}

interface TableRange {
  from: number;
  to: number;
}

interface TableToolbarIcon {
  className: string;
  paths: string[];
}

interface TableActionTarget {
  row: number;
  col: number;
}

interface PendingCellFocus {
  row: number;
  col: number;
}

type TableSortDirection = 'asc' | 'desc';

interface TableSortState {
  column: number;
  direction: TableSortDirection;
  order: number[];
}

interface TableCellDiagnostics {
  from: number;
  to: number;
  severity: 0 | 1 | 2 | 3;
  message: string;
  source?: string;
  code?: string;
}

interface TableCellRange {
  from: number;
  to: number;
}

interface TableSearchState {
  text: string;
  wholeWord: boolean;
  caseSensitive: boolean;
  selectionFrom: number;
  selectionTo: number;
}

interface TableSearchMatchRange {
  start: number;
  end: number;
}

const sourceTableHeaderLineDeco = Decoration.line({ class: 'meo-md-source-table-header-line' });
const sourceTableHeaderCellDeco = Decoration.mark({ class: 'meo-md-source-table-header-cell' });
const tableDelimiterRegex = /^\|?\s*[:]?\-+[:]?\s*(\|\s*[:]?\-+[:]?\s*)*\|?$/;
const tableCellSelector = 'th[data-table-row][data-table-col], td[data-table-row][data-table-col]';
const tableControlSelector =
  '.meo-md-html-table-toolbar, .meo-md-html-table-toolbar-btn, .meo-md-html-sort-btn, .meo-md-html-apply-sort-btn';
const minColumnWidthCh = 10;
const maxColumnWidthCh = 40;
const cellHorizontalPaddingCh = 1;
const headerSortButtonPaddingCh = 2.5;
const cellBorderPx = 2;
const tableSortCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
// Icons are inline SVG path data from Tabler Icons (MIT), vendored to avoid a
// broad icon dependency for this table-only toolbar.
const tableToolbarIcons: Record<string, TableToolbarIcon> = {
  rowInsertTop: {
    className: 'icon-tabler-row-insert-top',
    paths: ['M4 18v-4a1 1 0 0 1 1 -1h14a1 1 0 0 1 1 1v4a1 1 0 0 1 -1 1h-14a1 1 0 0 1 -1 -1', 'M12 9v-4', 'M10 7l4 0'],
  },
  rowInsertBottom: {
    className: 'icon-tabler-row-insert-bottom',
    paths: [
      'M20 6v4a1 1 0 0 1 -1 1h-14a1 1 0 0 1 -1 -1v-4a1 1 0 0 1 1 -1h14a1 1 0 0 1 1 1',
      'M12 15l0 4',
      'M14 17l-4 0',
    ],
  },
  columnInsertLeft: {
    className: 'icon-tabler-column-insert-left',
    paths: ['M14 4h4a1 1 0 0 1 1 1v14a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-14a1 1 0 0 1 1 -1', 'M5 12l4 0', 'M7 10l0 4'],
  },
  columnInsertRight: {
    className: 'icon-tabler-column-insert-right',
    paths: ['M6 4h4a1 1 0 0 1 1 1v14a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-14a1 1 0 0 1 1 -1', 'M15 12l4 0', 'M17 10l0 4'],
  },
  rowRemove: {
    className: 'icon-tabler-row-remove',
    paths: [
      'M20 6v4a1 1 0 0 1 -1 1h-14a1 1 0 0 1 -1 -1v-4a1 1 0 0 1 1 -1h14a1 1 0 0 1 1 1',
      'M10 16l4 4',
      'M10 20l4 -4',
    ],
  },
  columnRemove: {
    className: 'icon-tabler-column-remove',
    paths: [
      'M6 4h4a1 1 0 0 1 1 1v14a1 1 0 0 1 -1 1h-4a1 1 0 0 1 -1 -1v-14a1 1 0 0 1 1 -1',
      'M16 10l4 4',
      'M16 14l4 -4',
    ],
  },
  sortNeutral: {
    className: 'icon-tabler-arrows-sort',
    paths: ['M7 7l5 -5l5 5', 'M12 2v20', 'M17 17l-5 5l-5 -5'],
  },
  sortAsc: {
    className: 'icon-tabler-arrow-up',
    paths: ['M12 19v-14', 'M5 12l7 -7l7 7'],
  },
  sortDesc: {
    className: 'icon-tabler-arrow-down',
    paths: ['M12 5v14', 'M19 12l-7 7l-7 -7'],
  },
};

function isTableControlTarget(target) {
  return target instanceof Element && target.closest(tableControlSelector);
}

function isSelectionMenuTarget(target) {
  return target instanceof Element && target.closest('.selection-inline-menu');
}

function targetElementFrom(target) {
  return target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
}

function isPrimaryModifier(event) {
  if (event.altKey) return false;
  return event.metaKey || event.ctrlKey;
}

function isModifierLinkActivationEvent(event) {
  return Boolean(getModifierLinkActivationHref(event));
}

function getModifierLinkActivationHref(event) {
  if (!isPrimaryModifierPointerClick(event)) return '';
  const target = targetElementFrom(event.target);
  if (!target) return '';
  const link = target.closest('[data-meo-link-href]');
  if (!(link instanceof Element)) return '';
  const href = link.getAttribute('data-meo-link-href');
  return href || '';
}

function isUndoShortcut(event) {
  return event.key.toLowerCase() === 'z' && !event.shiftKey;
}

function isRedoShortcut(event) {
  const key = event.key.toLowerCase();
  return (key === 'z' && event.shiftKey) || key === 'y';
}

// Table widget inline preview + pipe-aware row parsing are table-specific and
// live here to keep all HTML-table behavior in one helper module.
const tableInlineSchemeRe = /^[a-z][a-z0-9+.-]*:/i;
const tableInlineRawUrlRe = /^(?:[a-z][a-z0-9+.-]*:\/\/|mailto:|file:|www\.)[^\s<]+/i;
const tableInlineEmojiShortcodeRe = /^:([a-zA-Z0-9_+-]+):/;
const tableInlineEscapableChars = new Set(['\\', '*', '_', '~', '`', '[', ']', '(', ')', '!', '|', '<', '>']);
const tableSearchStateEventName = 'meo-search-state-change';
const tableDiagnosticSeverityClasses = [
  'meo-diagnostic-error',
  'meo-diagnostic-warning',
  'meo-diagnostic-info',
  'meo-diagnostic-hint',
];

function isTableInlineWhitespaceOnly(text) {
  return /^\s+$/.test(text);
}

function isTableInlineEscaped(text, index) {
  let slashCount = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}

function isTableInlineUrlLike(text) {
  return tableInlineRawUrlRe.test(text) || tableInlineSchemeRe.test(text);
}

function tableInlineHrefFromRawUrl(text) {
  return normalizeSourceHref(text);
}

function tableInlineHrefFromWikiTarget(target) {
  const trimmed = (target ?? '').trim();
  if (!trimmed) return '';
  if (tableInlineSchemeRe.test(trimmed)) return trimmed;
  return `${wikiLinkScheme}${encodeURIComponent(trimmed)}`;
}

function tableDiagnosticTitle(diagnostic: TableCellDiagnostics): string {
  const parts = [];
  if (diagnostic.source) parts.push(diagnostic.source);
  if (diagnostic.code) parts.push(diagnostic.code);
  const prefix = parts.length ? `${parts.join(' ')}: ` : '';
  return `${prefix}${diagnostic.message}`;
}

function isTableSearchWordCharacter(value: string): boolean {
  return /[0-9A-Za-z_]/.test(value);
}

function isWholeWordTableSearchRange(text: string, start: number, end: number): boolean {
  const previous = start > 0 ? text.slice(start - 1, start) : '';
  const next = end < text.length ? text.slice(end, end + 1) : '';
  return !isTableSearchWordCharacter(previous) && !isTableSearchWordCharacter(next);
}

function findTableSearchMatchRanges(text: string, searchState: TableSearchState | null): TableSearchMatchRange[] {
  const query = searchState?.text ?? '';
  if (!query) {
    return [];
  }

  const haystack = searchState?.caseSensitive ? text : text.toLocaleLowerCase();
  const needle = searchState?.caseSensitive ? query : query.toLocaleLowerCase();
  const matches: TableSearchMatchRange[] = [];
  let offset = 0;
  while (offset <= text.length) {
    const index = haystack.indexOf(needle, offset);
    if (index < 0) {
      break;
    }

    const end = index + query.length;
    if (!searchState?.wholeWord || isWholeWordTableSearchRange(text, index, end)) {
      matches.push({ start: index, end });
    }
    offset = end;
  }
  return matches;
}

function appendSearchHighlightedText(
  parent: HTMLElement,
  text: string,
  offset: number,
  searchState: TableSearchState | null,
  sourceRange: TableCellRange | null
) {
  const matches = findTableSearchMatchRanges(text, searchState);
  if (matches.length === 0) {
    parent.appendChild(document.createTextNode(text));
    return;
  }

  let cursor = 0;
  for (const match of matches) {
    if (match.start > cursor) {
      parent.appendChild(document.createTextNode(text.slice(cursor, match.start)));
    }

    const span = document.createElement('span');
    const absoluteFrom = (sourceRange?.from ?? 0) + offset + match.start;
    const absoluteTo = (sourceRange?.from ?? 0) + offset + match.end;
    const isActive = Boolean(
      searchState && absoluteFrom === searchState.selectionFrom && absoluteTo === searchState.selectionTo
    );
    span.className = isActive ? 'meo-search-match meo-search-match-active' : 'meo-search-match';
    span.textContent = text.slice(match.start, match.end);
    parent.appendChild(span);
    cursor = match.end;
  }

  if (cursor < text.length) {
    parent.appendChild(document.createTextNode(text.slice(cursor)));
  }
}

function appendTablePlainText(
  parent: HTMLElement,
  text: string,
  offset: number,
  diagnostics: TableCellDiagnostics[],
  searchState: TableSearchState | null,
  sourceRange: TableCellRange | null
) {
  if (!text) return;
  if (!Array.isArray(diagnostics) || diagnostics.length === 0) {
    appendSearchHighlightedText(parent, text, offset, searchState, sourceRange);
    return;
  }

  appendDiagnosticText(parent, text, offset, diagnostics, searchState, sourceRange);
}

function appendDiagnosticText(
  parent: HTMLElement,
  text: string,
  offset: number,
  diagnostics: TableCellDiagnostics[],
  searchState: TableSearchState | null,
  sourceRange: TableCellRange | null
) {
  if (!text) return;
  if (!Array.isArray(diagnostics) || diagnostics.length === 0) {
    appendSearchHighlightedText(parent, text, offset, searchState, sourceRange);
    return;
  }

  const from = offset;
  const to = offset + text.length;
  const relevant = diagnostics
    .filter((diagnostic) => diagnostic.from < to && diagnostic.to > from)
    .sort((left, right) => (left.from === right.from ? left.to - right.to : left.from - right.from));
  if (relevant.length === 0) {
    appendSearchHighlightedText(parent, text, offset, searchState, sourceRange);
    return;
  }

  let cursor = from;
  for (const diagnostic of relevant) {
    const diagnosticFrom = Math.max(from, diagnostic.from);
    const diagnosticTo = Math.min(to, diagnostic.to);
    if (diagnosticTo <= diagnosticFrom || diagnosticFrom < cursor) {
      continue;
    }
    if (diagnosticFrom > cursor) {
      appendSearchHighlightedText(
        parent,
        text.slice(cursor - from, diagnosticFrom - from),
        cursor,
        searchState,
        sourceRange
      );
    }
    const span = document.createElement('span');
    const severityClass = tableDiagnosticSeverityClasses[diagnostic.severity] ?? tableDiagnosticSeverityClasses[0];
    span.className = `meo-diagnostic ${severityClass}`;
    span.title = tableDiagnosticTitle(diagnostic);
    appendSearchHighlightedText(
      span,
      text.slice(diagnosticFrom - from, diagnosticTo - from),
      diagnosticFrom,
      searchState,
      sourceRange
    );
    parent.appendChild(span);
    cursor = diagnosticTo;
  }
  if (cursor < to) {
    appendSearchHighlightedText(parent, text.slice(cursor - from), cursor, searchState, sourceRange);
  }
}

function decodeTableInlineEscapes(text) {
  let result = '';
  for (let i = 0; i < text.length; i += 1) {
    if (text[i] === '\\' && i + 1 < text.length && tableInlineEscapableChars.has(text[i + 1])) {
      result += text[i + 1];
      i += 1;
      continue;
    }
    result += text[i];
  }
  return result;
}

function findTableInlineMatchingBackticks(text, index, tickCount) {
  const marker = '`'.repeat(tickCount);
  for (let i = index; i <= text.length - tickCount; i += 1) {
    if (text.startsWith(marker, i)) return i;
  }
  return -1;
}

function parseTableInlineCodeSpan(text, index) {
  if (text[index] !== '`') return null;
  let tickCount = 1;
  while (text[index + tickCount] === '`') tickCount += 1;
  const close = findTableInlineMatchingBackticks(text, index + tickCount, tickCount);
  if (close < 0) return null;
  return {
    content: text.slice(index + tickCount, close),
    nextIndex: close + tickCount,
  };
}

function consumeTableInlineAngleSection(text, index) {
  if (text[index] !== '<' || isTableInlineEscaped(text, index)) return null;
  const close = text.indexOf('>', index + 1);
  if (close < 0) return null;
  return {
    content: text.slice(index + 1, close),
    nextIndex: close + 1,
  };
}

function consumeTableInlineBracketContent(text, index) {
  if (text[index] !== '[' || isTableInlineEscaped(text, index)) return null;
  let depth = 1;
  for (let i = index + 1; i < text.length;) {
    if (text[i] === '\\' && i + 1 < text.length) {
      i += 2;
      continue;
    }
    const code = parseTableInlineCodeSpan(text, i);
    if (code) {
      i = code.nextIndex;
      continue;
    }
    if (text[i] === '[' && !isTableInlineEscaped(text, i)) {
      depth += 1;
      i += 1;
      continue;
    }
    if (text[i] === ']' && !isTableInlineEscaped(text, i)) {
      depth -= 1;
      if (depth === 0) {
        return {
          content: text.slice(index + 1, i),
          nextIndex: i + 1,
        };
      }
      i += 1;
      continue;
    }
    i += 1;
  }
  return null;
}

function consumeTableInlineParenContent(text, index) {
  if (text[index] !== '(' || isTableInlineEscaped(text, index)) return null;
  let depth = 1;
  for (let i = index + 1; i < text.length;) {
    if (text[i] === '\\' && i + 1 < text.length) {
      i += 2;
      continue;
    }
    const code = parseTableInlineCodeSpan(text, i);
    if (code) {
      i = code.nextIndex;
      continue;
    }
    const angle = consumeTableInlineAngleSection(text, i);
    if (angle) {
      i = angle.nextIndex;
      continue;
    }
    if (text[i] === '(' && !isTableInlineEscaped(text, i)) {
      depth += 1;
      i += 1;
      continue;
    }
    if (text[i] === ')' && !isTableInlineEscaped(text, i)) {
      depth -= 1;
      if (depth === 0) {
        return {
          content: text.slice(index + 1, i),
          nextIndex: i + 1,
        };
      }
      i += 1;
      continue;
    }
    i += 1;
  }
  return null;
}

function parseTableInlineMarkdownLink(text, index, { image = false } = {}) {
  const start = image ? index + 1 : index;
  if (image) {
    if (!(text[index] === '!' && text[index + 1] === '[') || isTableInlineEscaped(text, index)) return null;
  } else if (text[index] !== '[' || isTableInlineEscaped(text, index)) {
    return null;
  }
  if (!image && text.startsWith('[[', index)) return null;

  const label = consumeTableInlineBracketContent(text, start);
  if (!label || text[label.nextIndex] !== '(') return null;
  const destination = consumeTableInlineParenContent(text, label.nextIndex);
  if (!destination) return null;

  let url = normalizeSourceHref(destination.content.trim());
  if (url.startsWith('<') && url.endsWith('>') && url.length >= 2) {
    url = url.slice(1, -1).trim();
  }

  return {
    label: label.content,
    url,
    nextIndex: destination.nextIndex,
  };
}

function parseTableInlineWikiLink(text, index) {
  if (!text.startsWith('[[', index) || isTableInlineEscaped(text, index)) return null;
  for (let i = index + 2; i < text.length - 1; i += 1) {
    if (text[i] === '\\') {
      i += 1;
      continue;
    }
    if (text[i] === ']' && text[i + 1] === ']' && !isTableInlineEscaped(text, i)) {
      const content = text.slice(index + 2, i);
      const pipeIndex = content.indexOf('|');
      const rawTarget = pipeIndex >= 0 ? content.slice(0, pipeIndex).trim() : content.trim();
      const rawAlias = pipeIndex >= 0 ? content.slice(pipeIndex + 1).trim() : '';
      return {
        target: rawTarget,
        visibleText: rawAlias || rawTarget,
        nextIndex: i + 2,
      };
    }
  }
  return null;
}

function findTableInlineClosingMarker(text, startIndex, marker, { singleTilde = false } = {}) {
  const markerLen = marker.length;
  for (let i = startIndex; i <= text.length - markerLen; i += 1) {
    if (!text.startsWith(marker, i)) continue;
    if (isTableInlineEscaped(text, i)) continue;
    if (singleTilde && (text[i - 1] === '~' || text[i + 1] === '~')) continue;
    return i;
  }
  return -1;
}

function parseTableInlineDelimitedSpan(text, index) {
  const strongMarker = text.startsWith('**', index) ? '**' : text.startsWith('__', index) ? '__' : null;
  if (strongMarker) {
    const start = index + 2;
    const close = findTableInlineClosingMarker(text, start, strongMarker);
    if (close > start) {
      const content = text.slice(start, close);
      if (!isTableInlineWhitespaceOnly(content)) {
        return { kind: 'strong', content, nextIndex: close + 2 };
      }
    }
  }

  if (text.startsWith('~~', index)) {
    const start = index + 2;
    const close = findTableInlineClosingMarker(text, start, '~~');
    if (close > start) {
      const content = text.slice(start, close);
      if (!isTableInlineWhitespaceOnly(content)) {
        return { kind: 'strike', content, nextIndex: close + 2 };
      }
    }
  }

  const emMarker = text[index] === '*' || text[index] === '_' ? text[index] : null;
  if (emMarker && text[index + 1] !== emMarker) {
    const start = index + 1;
    const close = findTableInlineClosingMarker(text, start, emMarker);
    if (close > start) {
      const content = text.slice(start, close);
      if (!isTableInlineWhitespaceOnly(content)) {
        return { kind: 'em', content, nextIndex: close + 1 };
      }
    }
  }

  if (text[index] === '~' && text[index + 1] !== '~' && text[index - 1] !== '~') {
    const start = index + 1;
    const close = findTableInlineClosingMarker(text, start, '~', { singleTilde: true });
    if (close > start) {
      const content = text.slice(start, close);
      if (!isTableInlineWhitespaceOnly(content)) {
        return { kind: 'strike', content, nextIndex: close + 1 };
      }
    }
  }

  return null;
}

function trimTableInlineRawUrl(raw, precedingChar) {
  let end = raw.length;
  while (end > 0 && /[.,!?;:]/.test(raw[end - 1])) end -= 1;
  while (end > 0 && raw[end - 1] === ')') {
    const body = raw.slice(0, end);
    const opens = (body.match(/\(/g) ?? []).length;
    const closes = (body.match(/\)/g) ?? []).length;
    if (closes <= opens) break;
    end -= 1;
  }
  let trimmed = raw.slice(0, end);
  while (trimmed.length > 0) {
    const last = trimmed[trimmed.length - 1];
    if (last !== '"' && last !== "'" && last !== '`') {
      break;
    }
    const withoutTrailing = trimmed.slice(0, -1);
    if (normalizeSourceHref(withoutTrailing) === normalizeSourceHref(trimmed)) {
      trimmed = withoutTrailing;
      continue;
    }
    break;
  }
  if (precedingChar === '"' || precedingChar === "'" || precedingChar === '`') {
    while (trimmed.length > 0 && trimmed[trimmed.length - 1] === precedingChar) {
      trimmed = trimmed.slice(0, -1);
    }
  }
  return trimmed;
}

function parseTableInlineAutolink(text, index) {
  const angle = consumeTableInlineAngleSection(text, index);
  if (!angle) return null;
  const inner = angle.content.trim();
  if (!inner || /\s/.test(inner)) return null;
  const looksLikeEmail = /.+@.+\..+/.test(inner);
  if (!isTableInlineUrlLike(inner) && !looksLikeEmail) return null;
  const href =
    looksLikeEmail && !tableInlineSchemeRe.test(inner) ? `mailto:${inner}` : tableInlineHrefFromRawUrl(inner);
  return { label: inner, href, nextIndex: angle.nextIndex };
}

function parseTableInlineRawUrl(text, index) {
  if (isTableInlineEscaped(text, index)) return null;
  if (index > 0 && /[A-Za-z0-9]/.test(text[index - 1])) return null;
  const match = tableInlineRawUrlRe.exec(text.slice(index));
  if (!match) return null;
  const trimmed = trimTableInlineRawUrl(match[0], text[index - 1]);
  if (!trimmed) return null;
  return {
    label: trimmed,
    href: tableInlineHrefFromRawUrl(trimmed),
    nextIndex: index + trimmed.length,
  };
}

function parseTableInlineEmojiShortcode(text, index) {
  if (text[index] !== ':' || isTableInlineEscaped(text, index)) return null;
  const match = tableInlineEmojiShortcodeRe.exec(text.slice(index));
  if (!match) return null;
  const emoji = emojiData[match[1]];
  if (!emoji) return null;
  return {
    emoji,
    nextIndex: index + match[0].length,
  };
}

function appendTableInlinePreviewLink(parent, label, href) {
  const el = document.createElement('span');
  el.className = 'meo-md-link';
  if (href) el.setAttribute('data-meo-link-href', href);
  appendTableInlinePreviewNodes(el, label, { disableLinkParsers: true });
  parent.appendChild(el);
}

function appendTableInlinePreviewImage(parent, altText, url) {
  if (!url) {
    parent.appendChild(document.createTextNode(`![${altText}]()`));
    return;
  }
  const dom = new ImageWidget(url, decodeTableInlineEscapes(altText), '').toDOM();
  if (dom instanceof HTMLElement) {
    dom.setAttribute('data-meo-link-href', url);
  }
  parent.appendChild(dom);
}

function appendTableInlinePreviewNodes(
  parent: HTMLElement,
  text: string,
  options: {
    baseOffset?: number;
    diagnostics?: TableCellDiagnostics[];
    disableLinkParsers?: boolean;
    searchState?: TableSearchState | null;
    sourceRange?: TableCellRange | null;
  } = {}
) {
  const {
    baseOffset = 0,
    diagnostics = [],
    disableLinkParsers = false,
    searchState = null,
    sourceRange = null,
  } = options;
  let buffer = '';
  let bufferStart = 0;
  const flushBuffer = () => {
    if (!buffer) return;
    appendTablePlainText(parent, buffer, baseOffset + bufferStart, diagnostics, searchState, sourceRange);
    buffer = '';
  };
  const appendToBuffer = (value: string, index: number) => {
    if (!buffer) {
      bufferStart = index;
    }
    buffer += value;
  };

  for (let i = 0; i < text.length;) {
    if (text[i] === '\\' && i + 1 < text.length && tableInlineEscapableChars.has(text[i + 1])) {
      appendToBuffer(text[i + 1], i);
      i += 2;
      continue;
    }

    const code = parseTableInlineCodeSpan(text, i);
    if (code) {
      flushBuffer();
      const el = document.createElement('code');
      el.className = 'meo-md-inline-code';
      el.textContent = decodeTableInlineEscapes(code.content);
      parent.appendChild(el);
      i = code.nextIndex;
      continue;
    }

    const kbd = text[i] === '<' && !isTableInlineEscaped(text, i) ? parseKbdTagAt(text, i) : null;
    if (kbd) {
      const keyText = decodeTableInlineEscapes(kbd.content).trim();
      if (!keyText) {
        appendToBuffer(text.slice(i, kbd.nextIndex), i);
      } else {
        flushBuffer();
        const el = document.createElement('kbd');
        el.className = 'meo-md-kbd';
        el.textContent = keyText;
        parent.appendChild(el);
      }
      i = kbd.nextIndex;
      continue;
    }

    const math = parseLatexMathAt(text, i);
    if (math) {
      const mathElement = createLatexMathElement(math.content, math.mode);
      if (mathElement) {
        flushBuffer();
        parent.appendChild(mathElement);
      } else {
        appendToBuffer(text.slice(math.from, math.to), math.from);
      }
      i = math.to;
      continue;
    }

    const image = parseTableInlineMarkdownLink(text, i, { image: true });
    if (image) {
      flushBuffer();
      appendTableInlinePreviewImage(parent, image.label, decodeTableInlineEscapes(image.url));
      i = image.nextIndex;
      continue;
    }

    if (!disableLinkParsers) {
      const wiki = parseTableInlineWikiLink(text, i);
      if (wiki) {
        flushBuffer();
        appendTableInlinePreviewLink(parent, wiki.visibleText, tableInlineHrefFromWikiTarget(wiki.target));
        i = wiki.nextIndex;
        continue;
      }

      const link = parseTableInlineMarkdownLink(text, i);
      if (link) {
        flushBuffer();
        if (link.url) {
          appendTableInlinePreviewLink(parent, link.label, decodeTableInlineEscapes(link.url));
        } else {
          appendTableInlinePreviewNodes(parent, link.label, {
            ...options,
            baseOffset: baseOffset + i + 1,
          });
        }
        i = link.nextIndex;
        continue;
      }

      const autolink = parseTableInlineAutolink(text, i);
      if (autolink) {
        flushBuffer();
        appendTableInlinePreviewLink(parent, autolink.label, autolink.href);
        i = autolink.nextIndex;
        continue;
      }
    }

    const span = parseTableInlineDelimitedSpan(text, i);
    if (span) {
      flushBuffer();
      if (span.kind === 'em') {
        const el = document.createElement('em');
        appendTableInlinePreviewNodes(el, span.content, {
          ...options,
          baseOffset: baseOffset + i + 1,
        });
        parent.appendChild(el);
      } else if (span.kind === 'strong') {
        const el = document.createElement('strong');
        el.className = 'meo-md-strong';
        appendTableInlinePreviewNodes(el, span.content, {
          ...options,
          baseOffset: baseOffset + i + 2,
        });
        parent.appendChild(el);
      } else if (span.kind === 'strike') {
        const el = document.createElement('span');
        el.className = 'meo-md-strike';
        appendTableInlinePreviewNodes(el, span.content, {
          ...options,
          baseOffset: baseOffset + i + (text.startsWith('~~', i) ? 2 : 1),
        });
        parent.appendChild(el);
      }
      i = span.nextIndex;
      continue;
    }

    if (!disableLinkParsers) {
      const rawUrl = parseTableInlineRawUrl(text, i);
      if (rawUrl) {
        flushBuffer();
        appendTableInlinePreviewLink(parent, rawUrl.label, rawUrl.href);
        i = rawUrl.nextIndex;
        continue;
      }
    }

    const emoji = parseTableInlineEmojiShortcode(text, i);
    if (emoji) {
      flushBuffer();
      const el = document.createElement('span');
      el.className = 'meo-md-emoji';
      el.textContent = emoji.emoji;
      parent.appendChild(el);
      i = emoji.nextIndex;
      continue;
    }

    appendToBuffer(text[i], i);
    i += 1;
  }

  flushBuffer();
}

function renderTableCellInlinePreview(
  previewEl,
  value,
  diagnostics: TableCellDiagnostics[] = [],
  searchState: TableSearchState | null = null,
  sourceRange: TableCellRange | null = null
) {
  if (!(previewEl instanceof HTMLElement)) return;
  previewEl.replaceChildren();
  appendTableInlinePreviewNodes(previewEl, value ?? '', { diagnostics, searchState, sourceRange });
}

function consumeTableInlineProtectedSpan(text, index, endIndex) {
  const code = parseTableInlineCodeSpan(text, index);
  if (code && code.nextIndex <= endIndex) return code.nextIndex;

  const kbd = text[index] === '<' && !isTableInlineEscaped(text, index) ? parseKbdTagAt(text, index) : null;
  if (kbd && kbd.nextIndex <= endIndex) return kbd.nextIndex;

  const wiki = parseTableInlineWikiLink(text, index);
  if (wiki && wiki.nextIndex <= endIndex) return wiki.nextIndex;

  const image = parseTableInlineMarkdownLink(text, index, { image: true });
  if (image && image.nextIndex <= endIndex) return image.nextIndex;

  const link = parseTableInlineMarkdownLink(text, index);
  if (link && link.nextIndex <= endIndex) return link.nextIndex;

  const angle = consumeTableInlineAngleSection(text, index);
  if (angle && angle.nextIndex <= endIndex) return angle.nextIndex;

  if (text[index] === '\\' && index + 1 < endIndex) return index + 2;
  return null;
}

function findTableRowSeparatorPipes(text, startIndex, endIndex) {
  const pipes = [];
  for (let i = startIndex; i < endIndex;) {
    const protectedNext = consumeTableInlineProtectedSpan(text, i, endIndex);
    if (protectedNext && protectedNext > i) {
      i = protectedNext;
      continue;
    }
    if (text[i] === '|' && !isTableInlineEscaped(text, i)) {
      pipes.push(i);
    }
    i += 1;
  }
  return pipes;
}

function parseTableRowCells(lineText, lineFrom = 0) {
  const leadingWhitespaceLen = /^(\s*)/.exec(lineText)?.[1].length ?? 0;
  let contentStart = leadingWhitespaceLen;
  let contentEnd = lineText.length;
  while (contentStart < contentEnd && /\s/.test(lineText[contentStart])) contentStart += 1;
  while (contentEnd > contentStart && /\s/.test(lineText[contentEnd - 1])) contentEnd -= 1;

  let innerStart = contentStart;
  let innerEnd = contentEnd;
  if (innerStart < innerEnd && lineText[innerStart] === '|') innerStart += 1;
  if (innerEnd > innerStart && lineText[innerEnd - 1] === '|') innerEnd -= 1;

  const allSeparatorPipes = findTableRowSeparatorPipes(lineText, 0, lineText.length);
  const innerPipes = allSeparatorPipes.filter((index) => index >= innerStart && index < innerEnd);

  const cells = [];
  if (innerStart < innerEnd || innerPipes.length > 0) {
    let cursor = innerStart;
    for (const pipeIndex of innerPipes) {
      cells.push(lineText.slice(cursor, pipeIndex).trim());
      cursor = pipeIndex + 1;
    }
    cells.push(lineText.slice(cursor, innerEnd).trim());
  }

  const segments = [];
  for (let i = 0; i + 1 < allSeparatorPipes.length; i += 1) {
    const rawFrom = allSeparatorPipes[i] + 1;
    const rawTo = allSeparatorPipes[i + 1];
    let from = rawFrom;
    let to = rawTo;
    if (from < to && lineText[from] === ' ') from += 1;
    if (to > from && lineText[to - 1] === ' ') to -= 1;
    if (to <= from) {
      segments.push({ from: lineFrom + rawFrom, to: lineFrom + rawTo, cellIndex: i, empty: true });
      continue;
    }
    segments.push({ from: lineFrom + from, to: lineFrom + to, cellIndex: i, empty: false });
  }

  const hasExplicitSingleCell = cells.length === 1 && cells[0] === '' && allSeparatorPipes.length >= 2;

  return {
    cells: cells.length === 1 && cells[0] === '' && !hasExplicitSingleCell ? [] : cells,
    pipes: allSeparatorPipes,
    segments,
  };
}

function normalizeRow(cells, colCount) {
  const result = cells.slice(0, colCount);
  while (result.length < colCount) result.push('');
  return result;
}

function isValidTableRange(from, to, docLength) {
  return Number.isInteger(from) && Number.isInteger(to) && from >= 0 && from < to && to <= docLength;
}

function parseDelimiterAlignments(lineText) {
  const alignments = [];
  const parts = lineText.split('|').filter((part) => part.trim());
  for (const part of parts) {
    const value = part.trim();
    const left = value.startsWith(':');
    const right = value.endsWith(':');
    alignments.push(left && right ? 'center' : left ? 'left' : right ? 'right' : null);
  }
  return alignments;
}

function delimiterCellForAlignment(alignment) {
  if (alignment === 'left') return ':---';
  if (alignment === 'right') return '---:';
  if (alignment === 'center') return ':---:';
  return '---';
}

function serializeTableMarkdown(indent, headerCells, alignments, rows) {
  const colCount = headerCells.length;
  const normalizedAlignments = normalizeRow(alignments, colCount).map((value) => value ?? null);
  const normalizedRows = rows.map((row) => normalizeRow(row, colCount));
  const header = `| ${headerCells.join(' | ')} |`;
  const delimiter = `| ${normalizedAlignments.map(delimiterCellForAlignment).join(' | ')} |`;
  const dataRows = normalizedRows.map((row) => `| ${row.join(' | ')} |`);
  return [header, delimiter, ...dataRows].map((line) => `${indent}${line}`).join('\n');
}

function longestUnbreakableSegmentLength(value) {
  return Math.max(
    0,
    ...String(value ?? '')
      .split(/\s+/)
      .map((part) => part.length)
  );
}

function preferredCellWidthCh(value, extraCh = 0) {
  return Math.max(
    minColumnWidthCh,
    Math.min(
      maxColumnWidthCh,
      Math.max(String(value ?? '').length, longestUnbreakableSegmentLength(value) + extraCh) + 2
    )
  );
}

function computePreferredColumnCharWidths(headerCells, rows, colCount) {
  const widths = new Array(colCount).fill(minColumnWidthCh);
  const update = (value, col, extraCh = 0) => {
    widths[col] = Math.max(widths[col], preferredCellWidthCh(value, extraCh));
  };

  for (let col = 0; col < colCount; col++) {
    update(headerCells[col] ?? '', col, headerSortButtonPaddingCh);
  }
  for (const row of rows) {
    for (let col = 0; col < colCount; col++) {
      update(row[col] ?? '', col);
    }
  }
  return widths;
}

function computePreferredColumnCharWidthsFromInputs(headerInputs, rowInputs, colCount) {
  const headerCells = normalizeRow(
    headerInputs.map((input) => input.value.trim()),
    colCount
  );
  const rows = rowInputs.map((inputs) =>
    normalizeRow(
      inputs.map((input) => input.value.trim()),
      colCount
    )
  );
  return computePreferredColumnCharWidths(headerCells, rows, colCount);
}

function parseTableSortNumber(value) {
  const normalized = value.replace(/,/g, '').replace(/%$/, '').trim();
  if (!/^[+-]?(?:\d+|\d*\.\d+)(?:e[+-]?\d+)?$/i.test(normalized)) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseTableSortDate(value) {
  const normalized = value.trim();
  if (!/^\d{4}-\d{2}-\d{2}(?:[T\s]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?$/.test(normalized)) {
    return null;
  }
  const parsed = Date.parse(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

function compareTableSortValues(leftValue, rightValue, direction: TableSortDirection) {
  const left = `${leftValue ?? ''}`.trim();
  const right = `${rightValue ?? ''}`.trim();
  const leftEmpty = left.length === 0;
  const rightEmpty = right.length === 0;
  if (leftEmpty || rightEmpty) {
    if (leftEmpty && rightEmpty) return 0;
    return leftEmpty ? 1 : -1;
  }

  const leftNumber = parseTableSortNumber(left);
  const rightNumber = parseTableSortNumber(right);
  let comparison = 0;
  if (leftNumber !== null && rightNumber !== null) {
    comparison = leftNumber - rightNumber;
  } else {
    const leftDate = parseTableSortDate(left);
    const rightDate = parseTableSortDate(right);
    comparison =
      leftDate !== null && rightDate !== null ? leftDate - rightDate : tableSortCollator.compare(left, right);
  }

  if (comparison === 0) return 0;
  return direction === 'desc' ? -comparison : comparison;
}

function parseTableLine(lineNo, from, to, text) {
  const { cells, pipes, segments } = parseTableRowCells(text, from);
  return { lineNo, from, to, text, cells, pipes, segments };
}

function isTableContentLine(lineText) {
  return lineText.includes('|');
}

function buildTableData(state, tableNode) {
  const startLine = state.doc.lineAt(tableNode.from);
  const endLine = state.doc.lineAt(Math.max(tableNode.to - 1, tableNode.from));
  return buildTableDataForLineRange(state, startLine.number, endLine.number);
}

function buildTableDataForLineRange(state, startLineNo, endLineNo) {
  const startLine = state.doc.line(startLineNo);
  const endLine = state.doc.line(endLineNo);
  const lines = [];
  let delimiterIdx = -1;

  for (let lineNo = startLine.number; lineNo <= endLine.number; lineNo++) {
    const line = state.doc.line(lineNo);
    const text = state.doc.sliceString(line.from, line.to);
    if (delimiterIdx === -1 && isTableDelimiterLine(text)) {
      delimiterIdx = lines.length;
    }
    lines.push(parseTableLine(lineNo, line.from, line.to, text));
  }

  const headerLine = delimiterIdx > 0 ? lines[delimiterIdx - 1] : null;
  let lastTableLineIdx = delimiterIdx;
  if (delimiterIdx >= 0) {
    for (let idx = delimiterIdx + 1; idx < lines.length; idx += 1) {
      if (!isTableContentLine(lines[idx].text)) {
        break;
      }
      lastTableLineIdx = idx;
    }
  }

  const dataLines = delimiterIdx >= 0 ? lines.slice(delimiterIdx + 1, lastTableLineIdx + 1) : [];
  const alignments = delimiterIdx >= 0 ? parseDelimiterAlignments(lines[delimiterIdx].text) : [];
  const colCount = headerLine ? headerLine.cells.length : dataLines[0] ? dataLines[0].cells.length : 0;
  const tableFrom = headerLine ? headerLine.from : startLine.from;
  const tableTo = delimiterIdx >= 0 && lines[lastTableLineIdx] ? lines[lastTableLineIdx].to : endLine.to;
  const effectiveStartLine = headerLine ? headerLine.lineNo : startLine.number;
  const effectiveEndLine =
    delimiterIdx >= 0 && lines[lastTableLineIdx] ? lines[lastTableLineIdx].lineNo : endLine.number;

  return {
    from: tableFrom,
    to: tableTo,
    lines,
    delimiterIdx,
    headerLine,
    dataLines,
    alignments,
    colCount,
    startLine: effectiveStartLine,
    endLine: effectiveEndLine,
  };
}

class HtmlTableWidget extends WidgetType {
  tableData: TableData;
  view: EditorView | null;
  layoutFrame: number;
  pendingResizeRows: boolean;
  lastAppliedWidths: number[];
  domRefs: DomRefs | null;
  chPx: number;
  cleanupFns: (() => void)[];
  selectionAnchor: CellCoords | null;
  selectionRange: SelectionRange | null;
  selectionPointerId: number | null;
  isDraggingSelection: boolean;
  hasPendingCellEdits: boolean;
  sortState: TableSortState | null;
  activeTarget: TableActionTarget;
  searchState: TableSearchState | null;

  constructor(tableData: TableData) {
    super();
    this.tableData = tableData;
    this.view = null;
    this.layoutFrame = 0;
    this.pendingResizeRows = false;
    this.lastAppliedWidths = [];
    this.domRefs = null;
    this.chPx = 0;
    this.cleanupFns = [];
    this.selectionAnchor = null;
    this.selectionRange = null;
    this.selectionPointerId = null;
    this.isDraggingSelection = false;
    this.hasPendingCellEdits = false;
    this.sortState = null;
    this.activeTarget = { row: this.tableData.rows.length > 0 ? 1 : 0, col: 0 };
    this.searchState = null;
  }

  eq(other: WidgetType): boolean {
    return (
      other instanceof HtmlTableWidget &&
      other.tableData.signature === this.tableData.signature &&
      other.tableData.indent === this.tableData.indent
    );
  }

  getEditorView(dom?: HTMLElement): EditorView | null {
    if (this.view) return this.view;
    if (!dom) return null;
    return EditorView.findFromDOM(dom);
  }

  resolveCurrentTableRange(view: EditorView, dom: HTMLElement): TableRange | null {
    let pos = 0;
    try {
      pos = view.posAtDOM(dom, 0);
    } catch {
      pos = -1;
    }

    if (pos >= 0) {
      let node = syntaxTree(view.state).resolveInner(pos, 1);
      while (node) {
        if (node.name === 'Table') {
          if (this.tableData) {
            this.tableData.from = node.from;
            this.tableData.to = node.to;
          }
          return { from: node.from, to: node.to };
        }
        node = node.parent;
      }
    }

    const tableFrom = this.tableData?.from;
    const tableTo = this.tableData?.to;
    if (isValidTableRange(tableFrom, tableTo, view.state.doc.length)) {
      return { from: tableFrom, to: tableTo };
    }

    return null;
  }

  readCellMatrix(): CellMatrix {
    if (!this.domRefs) return { headerCells: [], rows: [], alignments: [] };
    const { headerInputs, rowInputs } = this.domRefs;
    const headerCells = normalizeRow(
      headerInputs.map((input) => input.value.trim()),
      this.tableData.colCount
    );

    const rows = rowInputs.map((inputs) =>
      normalizeRow(
        inputs.map((input) => input.value.trim()),
        this.tableData.colCount
      )
    );

    return { headerCells, rows, alignments: this.tableData.alignments };
  }

  sourceRowOrder() {
    return this.tableData.rows.map((_row, index) => index);
  }

  sortedRowOrder(column, direction: TableSortDirection) {
    return this.sourceRowOrder().sort((leftIndex, rightIndex) => {
      const leftRow = this.tableData.rows[leftIndex] ?? [];
      const rightRow = this.tableData.rows[rightIndex] ?? [];
      const comparison = compareTableSortValues(leftRow[column], rightRow[column], direction);
      return comparison || leftIndex - rightIndex;
    });
  }

  updateBodyRowDatasets(rowInputs, cellGrid) {
    for (let row = 0; row < rowInputs.length; row += 1) {
      const tableRow = row + 1;
      for (let col = 0; col < rowInputs[row].length; col += 1) {
        rowInputs[row][col].dataset.tableRow = String(tableRow);
        rowInputs[row][col].dataset.tableCol = String(col);
      }
      for (let col = 0; col < cellGrid[row].length; col += 1) {
        cellGrid[row][col].dataset.tableRow = String(tableRow);
        cellGrid[row][col].dataset.tableCol = String(col);
      }
    }
  }

  setVisualRowOrder(order) {
    if (!this.domRefs) return;
    const { tbody, sourceBodyRows, sourceBodyRowInputs, sourceBodyCellGrid, headerInputs, cellGrid, rowEntries } =
      this.domRefs;
    const normalizedOrder = order.filter((index) => sourceBodyRows[index]);
    const nextRows = normalizedOrder.map((index) => sourceBodyRows[index]);
    const nextRowInputs = normalizedOrder.map((index) => sourceBodyRowInputs[index]);
    const nextCellGrid = normalizedOrder.map((index) => sourceBodyCellGrid[index]);

    for (const row of nextRows) {
      tbody.appendChild(row);
    }

    this.updateBodyRowDatasets(nextRowInputs, nextCellGrid);
    this.domRefs.rowInputs = nextRowInputs;
    this.domRefs.allRowInputs = [headerInputs, ...nextRowInputs];
    this.domRefs.cellGrid = [cellGrid[0], ...nextCellGrid];
    this.domRefs.rowEntries = [rowEntries[0], ...nextRows.map((row, index) => ({ row, inputs: nextRowInputs[index] }))];
    this.clearSelection();
    this.updateActionTargetStyles();
    this.scheduleLayout({ resizeRows: true });
  }

  updateSortControls() {
    if (!this.domRefs) return;
    const { sortButtons, applySortButton } = this.domRefs;
    for (let col = 0; col < sortButtons.length; col += 1) {
      const button = sortButtons[col];
      const active = this.sortState?.column === col;
      button.classList.toggle('is-active', active);
      button.dataset.sortDirection = active ? this.sortState.direction : '';
      this.setSortButtonIcon(button, active ? this.sortState.direction : null);
      button.title = active
        ? `Sorted ${this.sortState.direction === 'desc' ? 'descending' : 'ascending'}; click to toggle`
        : 'Sort column descending';
      button.setAttribute('aria-label', button.title);
      button.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
    applySortButton.hidden = !this.sortState;
  }

  setSortButtonIcon(button: HTMLButtonElement, direction: TableSortDirection | null) {
    const icon =
      direction === 'asc'
        ? tableToolbarIcons.sortAsc
        : direction === 'desc'
          ? tableToolbarIcons.sortDesc
          : tableToolbarIcons.sortNeutral;
    const svg = this.createToolbarIcon(icon);
    svg.setAttribute('width', '14');
    svg.setAttribute('height', '14');
    svg.classList.add('meo-md-html-sort-icon');
    button.replaceChildren(svg);
  }

  clearVisualSort() {
    if (!this.sortState) return;
    this.sortState = null;
    this.setVisualRowOrder(this.sourceRowOrder());
    this.updateSortControls();
  }

  activeBodyRowIndex() {
    if (this.tableData.rows.length === 0) return null;
    if (this.activeTarget.row <= 0) return null;
    const visualIndex = this.activeTarget.row - 1;
    if (this.sortState?.order) {
      const sourceIndex = this.sortState.order[visualIndex];
      return Number.isInteger(sourceIndex) ? sourceIndex : null;
    }
    return visualIndex >= 0 && visualIndex < this.tableData.rows.length ? visualIndex : null;
  }

  activeColumnIndex() {
    const colCount = this.tableData.colCount;
    if (colCount <= 0) return null;
    return Math.min(Math.max(this.activeTarget.col, 0), colCount - 1);
  }

  updateToolbarState() {
    if (!this.domRefs) return;
    const { toolbarButtons } = this.domRefs;
    const activeBodyRow = this.activeBodyRowIndex();
    const activeColumn = this.activeColumnIndex();
    const hasRowTarget = activeBodyRow !== null;
    const hasColumnTarget = activeColumn !== null;

    toolbarButtons.insertRowAbove.disabled = this.tableData.colCount === 0;
    toolbarButtons.insertRowBelow.disabled = this.tableData.colCount === 0;
    toolbarButtons.deleteRow.disabled = !hasRowTarget || this.tableData.rows.length <= 1;
    toolbarButtons.insertColumnLeft.disabled = !hasColumnTarget;
    toolbarButtons.insertColumnRight.disabled = !hasColumnTarget;
    toolbarButtons.deleteColumn.disabled = !hasColumnTarget || this.tableData.colCount <= 1;
  }

  updateActionTargetStyles() {
    this.updateToolbarState();
  }

  setActionTarget(target) {
    const row = Math.min(Math.max(target.row ?? 0, 0), this.tableData.rows.length);
    const col = Math.min(Math.max(target.col ?? 0, 0), Math.max(0, this.tableData.colCount - 1));
    this.activeTarget = { row, col };
    this.updateActionTargetStyles();
  }

  insertRowAboveTarget(container) {
    const rowIndex = this.activeBodyRowIndex();
    if (rowIndex === null) {
      this.addRowAfter(container, -1);
      return;
    }
    this.addRowBefore(container, rowIndex);
  }

  insertRowBelowTarget(container) {
    const rowIndex = this.activeBodyRowIndex();
    this.addRowAfter(container, rowIndex ?? -1);
  }

  deleteTargetRow(container) {
    const rowIndex = this.activeBodyRowIndex();
    if (rowIndex === null) return;
    this.removeRowAt(container, rowIndex);
  }

  insertColumnLeftTarget(container) {
    const colIndex = this.activeColumnIndex();
    if (colIndex === null) return;
    this.addColumnBefore(container, colIndex);
  }

  insertColumnRightTarget(container) {
    const colIndex = this.activeColumnIndex();
    if (colIndex === null) return;
    this.addColumnAfter(container, colIndex);
  }

  deleteTargetColumn(container) {
    const colIndex = this.activeColumnIndex();
    if (colIndex === null) return;
    this.removeColumnAt(container, colIndex);
  }

  sortByColumn(container, column) {
    if (!this.domRefs || this.tableData.rows.length <= 1) return;
    if (this.hasPendingCellEdits) {
      this.commit(container);
      if (!this.domRefs) return;
    }
    const direction: TableSortDirection =
      this.sortState?.column === column && this.sortState.direction === 'desc' ? 'asc' : 'desc';
    const order = this.sortedRowOrder(column, direction);
    this.sortState = { column, direction, order };
    this.setVisualRowOrder(order);
    this.updateSortControls();
    this.setTableInteractionActive(container, true);
  }

  applyCurrentSort(container) {
    if (!this.sortState) return;
    this.commitMatrix(this.readCellMatrix(), container);
    this.sortState = null;
    this.updateSortControls();
  }

  parseCellCoords(rowText, colText) {
    const row = Number.parseInt(rowText ?? '', 10);
    const col = Number.parseInt(colText ?? '', 10);
    if (Number.isNaN(row) || Number.isNaN(col)) return null;
    return { row, col };
  }

  findCellElement(node) {
    if (!this.domRefs || !(node instanceof Element)) return null;
    const cell = node.closest(tableCellSelector);
    if (!cell || !this.domRefs.table.contains(cell)) return null;
    return cell;
  }

  coordsFromCell(cell) {
    return this.parseCellCoords(cell.dataset.tableRow, cell.dataset.tableCol);
  }

  focusTableInput(input, caret = null) {
    if (!(input instanceof HTMLTextAreaElement)) return false;
    this.setCellEditingState(input, true);
    input.focus({ preventScroll: true });
    const nextCaret = Math.min(Math.max(caret ?? input.value.length, 0), input.value.length);
    input.setSelectionRange(nextCaret, nextCaret);
    input.closest(tableCellSelector)?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
    const container = input.closest('.meo-md-html-table-wrap');
    if (container instanceof HTMLElement) {
      this.emitTableSelectionChange(container);
    }
    return true;
  }

  focusCellInput(cell, { updateSelection = false } = {}) {
    const input = cell.querySelector('textarea');
    if (!this.focusTableInput(input)) return false;
    if (!updateSelection) return true;
    const coords = this.coordsFromCell(cell);
    if (coords) {
      this.setSingleCellSelection(coords);
    }
    return true;
  }

  focusCellInputAt(row, col, caret = null) {
    const input = this.domRefs?.allRowInputs?.[row]?.[col];
    return this.focusTableInput(input, caret);
  }

  moveVerticalOutOfTable(container, direction, preferredColumn = 0) {
    const view = this.getEditorView(container);
    if (!view) return false;

    const range = this.resolveCurrentTableRange(view, container);
    if (!range) return false;

    const firstLine = view.state.doc.lineAt(range.from);
    const lastLine = view.state.doc.lineAt(Math.max(range.to - 1, range.from));
    const lineStep = direction === 'up' ? -1 : direction === 'down' ? 1 : 0;
    if (!lineStep) return false;
    const anchorLineNo = lineStep < 0 ? firstLine.number : lastLine.number;
    const targetLineNo = anchorLineNo + lineStep;
    if (targetLineNo < 1 || targetLineNo > view.state.doc.lines) return false;

    const targetLine = view.state.doc.line(targetLineNo);
    const targetPos = Math.min(targetLine.from + Math.max(preferredColumn, 0), targetLine.to);

    this.commit(container);
    this.exitTableInteraction(container);
    view.dispatch({
      selection: { anchor: targetPos },
      effects: EditorView.scrollIntoView(targetPos, { y: 'nearest' }),
    });
    view.focus();
    return true;
  }

  normalizeSelectionRange(a, b) {
    return {
      fromRow: Math.min(a.row, b.row),
      toRow: Math.max(a.row, b.row),
      fromCol: Math.min(a.col, b.col),
      toCol: Math.max(a.col, b.col),
    };
  }

  isCellSelected(row, col, range) {
    if (!range) return false;
    return row >= range.fromRow && row <= range.toRow && col >= range.fromCol && col <= range.toCol;
  }

  applySelection(range) {
    if (!this.domRefs) return;
    this.selectionRange = range;
    const showSelectionStyle = Boolean(range && (range.fromRow !== range.toRow || range.fromCol !== range.toCol));
    const { cellGrid } = this.domRefs;
    for (let row = 0; row < cellGrid.length; row++) {
      const cells = cellGrid[row];
      for (let col = 0; col < cells.length; col++) {
        const cell = cells[col];
        const selected = this.isCellSelected(row, col, range);
        const styledSelected = selected && showSelectionStyle;
        const isTopEdge = styledSelected && row === range.fromRow;
        const isRightEdge = styledSelected && col === range.toCol;
        const isBottomEdge = styledSelected && row === range.toRow;
        const isLeftEdge = styledSelected && col === range.fromCol;
        cell.classList.toggle('meo-md-html-table-cell-selected', styledSelected);
        cell.classList.toggle('meo-md-html-table-cell-selected-top', isTopEdge);
        cell.classList.toggle('meo-md-html-table-cell-selected-right', isRightEdge);
        cell.classList.toggle('meo-md-html-table-cell-selected-bottom', isBottomEdge);
        cell.classList.toggle('meo-md-html-table-cell-selected-left', isLeftEdge);
      }
    }
  }

  setSingleCellSelection(coords) {
    this.selectionAnchor = coords;
    this.setActionTarget(coords);
    this.applySelection(this.normalizeSelectionRange(coords, coords));
  }

  clearSelection() {
    this.selectionAnchor = null;
    this.applySelection(null);
  }

  exitTableInteraction(container) {
    this.setTableInteractionActive(container, false);
    this.clearSelection();
  }

  setTableInteractionActive(container, active) {
    const view = this.getEditorView(container);
    if (!view) return;
    view.dom.dispatchEvent(new CustomEvent('meo-table-interaction', { detail: { active } }));
  }

  emitTableSelectionChange(container) {
    const view = this.getEditorView(container);
    if (!view) return;
    view.dom.dispatchEvent(new CustomEvent('meo-table-selection-change'));
  }

  hasFocusedTableInput(container) {
    const view = this.getEditorView(container);
    if (!view) return false;
    const active = document.activeElement;
    if (!(active instanceof Element)) return false;
    if (!view.dom.contains(active)) return false;
    return active.closest('.meo-md-html-table-wrap') !== null;
  }

  selectedCellCount() {
    if (!this.selectionRange) return 0;
    const rowCount = this.selectionRange.toRow - this.selectionRange.fromRow + 1;
    const colCount = this.selectionRange.toCol - this.selectionRange.fromCol + 1;
    return rowCount * colCount;
  }

  selectedTextAsTsv() {
    if (!this.selectionRange || !this.domRefs) return '';
    const lines = [];
    for (let row = this.selectionRange.fromRow; row <= this.selectionRange.toRow; row++) {
      const values = [];
      for (let col = this.selectionRange.fromCol; col <= this.selectionRange.toCol; col++) {
        values.push(this.domRefs.allRowInputs[row][col].value.trim());
      }
      lines.push(values.join('\t'));
    }
    return lines.join('\n');
  }

  handleHistoryShortcut(event, table) {
    if (!isPrimaryModifier(event) || (!isUndoShortcut(event) && !isRedoShortcut(event))) {
      return false;
    }
    event.preventDefault();
    event.stopPropagation();
    const wrap = this.domRefs?.wrap ?? table;
    const view = this.getEditorView(wrap);
    if (!view) return true;
    const { scrollTop, scrollLeft } = view.scrollDOM;
    this.commit(wrap);
    if (isUndoShortcut(event)) undo(view);
    else redo(view);
    requestAnimationFrame(() => {
      view.scrollDOM.scrollTop = scrollTop;
      view.scrollDOM.scrollLeft = scrollLeft;
    });
    return true;
  }

  wireTableSelection(table) {
    const getWrap = () => this.domRefs?.wrap ?? table;
    const getContainer = () => this.domRefs?.container ?? getWrap();

    const onWrapPointerDown = (event) => {
      if (!(event.target instanceof Node)) return;
      const container = getContainer();
      if (!container.contains(event.target)) return;
      if (isModifierLinkActivationEvent(event)) return;
      this.setTableInteractionActive(getWrap(), true);
    };

    const onPointerDown = (event) => {
      if (event.button !== 0) return;
      const modifierHref = getModifierLinkActivationHref(event);
      if (modifierHref) {
        event.preventDefault();
        event.stopPropagation();
        table.dispatchEvent(
          new CustomEvent('meo-open-link', {
            bubbles: true,
            detail: { href: modifierHref },
          })
        );
        return;
      }
      if (isTableControlTarget(event.target)) return;
      const cell = this.findCellElement(event.target);
      if (!cell) return;
      const current = this.coordsFromCell(cell);
      if (!current) return;
      if (event.target instanceof HTMLTextAreaElement) {
        this.selectionAnchor = current;
        this.setActionTarget(current);
        this.applySelection(this.normalizeSelectionRange(current, current));
        return;
      }
      const anchor = current;
      this.selectionAnchor = anchor;
      this.applySelection(this.normalizeSelectionRange(anchor, current));
      this.selectionPointerId = event.pointerId;
      this.isDraggingSelection = true;
      table.setPointerCapture?.(event.pointerId);

      if (!(event.target instanceof HTMLTextAreaElement)) {
        event.preventDefault();
        this.focusCellInput(cell);
      }
    };

    const onPointerMove = (event) => {
      if (!this.isDraggingSelection || this.selectionPointerId !== event.pointerId) return;
      const el = document.elementFromPoint(event.clientX, event.clientY);
      const cell = this.findCellElement(el);
      if (!cell || !this.selectionAnchor) return;
      const current = this.coordsFromCell(cell);
      if (!current) return;
      this.applySelection(this.normalizeSelectionRange(this.selectionAnchor, current));
    };

    const endPointerSelection = (event) => {
      if (this.selectionPointerId !== event.pointerId) return;
      this.isDraggingSelection = false;
      this.selectionPointerId = null;
      if (table.hasPointerCapture?.(event.pointerId)) {
        table.releasePointerCapture?.(event.pointerId);
      }
    };

    const onCopy = (event) => {
      if (this.selectedCellCount() <= 1) return;
      const text = this.selectedTextAsTsv();
      if (!text) return;
      event.preventDefault();
      event.clipboardData?.setData('text/plain', text);
    };

    const onKeyDown = (event) => {
      if (this.handleHistoryShortcut(event, table)) {
        return;
      }

      if (this.selectedCellCount() <= 1) return;
      if (event.key !== 'Backspace' && event.key !== 'Delete') return;
      if (!this.selectionRange || !this.domRefs) return;
      event.preventDefault();
      for (let row = this.selectionRange.fromRow; row <= this.selectionRange.toRow; row++) {
        for (let col = this.selectionRange.fromCol; col <= this.selectionRange.toCol; col++) {
          const input = this.domRefs.allRowInputs[row][col];
          if (input.value !== '') {
            input.value = '';
            this.refreshCellPreviewFromInput(input);
            this.hasPendingCellEdits = true;
          }
        }
      }
      this.scheduleLayout({ resizeRows: true });
    };

    const onFocusOut = (event) => {
      const nextTarget = event.relatedTarget;
      const wrap = this.domRefs?.wrap ?? table;
      const container = this.domRefs?.container ?? wrap;
      if (nextTarget instanceof Node && container.contains(nextTarget)) return;
      this.commit(wrap);
      this.exitTableInteraction(wrap);
    };

    const onDocumentPointerDown = (event) => {
      if (!(event.target instanceof Node)) return;
      const wrap = getWrap();
      const container = getContainer();
      if (isSelectionMenuTarget(event.target)) {
        return;
      }
      if (container.contains(event.target) && isModifierLinkActivationEvent(event)) return;
      if (!container.contains(event.target)) {
        this.setTableInteractionActive(wrap, false);
      }
      if (isTableControlTarget(event.target)) {
        return;
      }
      const active = document.activeElement;
      if (!(active instanceof HTMLElement) || !table.contains(active)) return;
      if (active === event.target || active.contains(event.target)) return;

      const targetCell = this.findCellElement(event.target);
      if (targetCell) {
        const targetInput = targetCell.querySelector('textarea');
        if (targetInput !== active) {
          event.preventDefault();
          this.focusCellInput(targetCell, { updateSelection: true });
        }
        return;
      }

      // Defer blur/selection cleanup until after the current click event finishes.
      // `focusout` handles commit when a table input actually loses focus.
      setTimeout(() => {
        if (this.selectedCellCount() > 1) {
          this.exitTableInteraction(wrap);
          return;
        }

        const currentActive = document.activeElement;
        if (currentActive instanceof HTMLElement && table.contains(currentActive)) {
          currentActive.blur();
        } else {
          this.exitTableInteraction(wrap);
        }
      }, 0);
    };

    table.addEventListener('pointerdown', onPointerDown);
    getContainer().addEventListener('pointerdown', onWrapPointerDown, true);
    table.addEventListener('pointermove', onPointerMove);
    table.addEventListener('pointerup', endPointerSelection);
    table.addEventListener('pointercancel', endPointerSelection);
    table.addEventListener('copy', onCopy);
    table.addEventListener('keydown', onKeyDown, true);
    table.addEventListener('focusout', onFocusOut);
    document.addEventListener('pointerdown', onDocumentPointerDown, true);
    const onCommitTableEdits = (event) => {
      const hadPendingEdits = this.hasPendingCellEdits;
      this.commit(getWrap());
      if (event instanceof CustomEvent && event.detail && typeof event.detail === 'object') {
        event.detail.committed = Boolean(event.detail.committed || hadPendingEdits);
      }
    };
    document.addEventListener('meo-commit-table-edits', onCommitTableEdits);
    this.cleanupFns.push(() => {
      table.removeEventListener('pointerdown', onPointerDown);
      getContainer().removeEventListener('pointerdown', onWrapPointerDown, true);
      table.removeEventListener('pointermove', onPointerMove);
      table.removeEventListener('pointerup', endPointerSelection);
      table.removeEventListener('pointercancel', endPointerSelection);
      table.removeEventListener('copy', onCopy);
      table.removeEventListener('keydown', onKeyDown, true);
      table.removeEventListener('focusout', onFocusOut);
      document.removeEventListener('pointerdown', onDocumentPointerDown, true);
      document.removeEventListener('meo-commit-table-edits', onCommitTableEdits);
    });
  }

  commit(dom) {
    if (!this.hasPendingCellEdits) return;
    this.commitMatrix(this.readCellMatrix(), dom);
  }

  scheduleFocusCellAfterCommit(view: EditorView, tableStartLine: number, focusTarget: PendingCellFocus) {
    const focusCell = () => {
      const input = view.dom.querySelector(
        `.meo-md-html-table-shell[data-meo-rendered-block-start-line="${tableStartLine}"] textarea[data-table-row="${focusTarget.row}"][data-table-col="${focusTarget.col}"]`
      );
      if (!(input instanceof HTMLTextAreaElement)) return false;
      input.focus({ preventScroll: true });
      input.setSelectionRange(0, 0);
      input.closest(tableCellSelector)?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
      return true;
    };

    requestAnimationFrame(() => {
      if (!focusCell()) {
        setTimeout(focusCell, 0);
      }
    });
  }

  commitMatrix(matrix, dom, focusTarget: PendingCellFocus | null = null) {
    const view = this.getEditorView(dom);
    if (!view) return;

    const { headerCells, rows, alignments = this.tableData.alignments } = matrix;
    if (!headerCells.length) return;
    const range = this.resolveCurrentTableRange(view, dom);
    if (!range) return;
    const tableStartLine = view.state.doc.lineAt(range.from).number;
    const markdown = serializeTableMarkdown(this.tableData.indent, headerCells, alignments, rows);
    const current = view.state.doc.sliceString(range.from, range.to);
    if (current === markdown) {
      this.hasPendingCellEdits = false;
      if (focusTarget) {
        this.focusCellInputAt(focusTarget.row, focusTarget.col, 0);
      }
      return;
    }

    view.dispatch({ changes: { from: range.from, to: range.to, insert: markdown } });
    this.hasPendingCellEdits = false;
    if (focusTarget) {
      this.scheduleFocusCellAfterCommit(view, tableStartLine, focusTarget);
    }
  }

  addRowAfter(dom, rowIndex) {
    this.clearVisualSort();
    const matrix = this.readCellMatrix();
    if (!matrix.headerCells.length) return;
    const insertAt = Math.min(Math.max(rowIndex + 1, 0), matrix.rows.length);
    matrix.rows.splice(insertAt, 0, new Array(matrix.headerCells.length).fill(''));
    this.commitMatrix(matrix, dom, { row: insertAt + 1, col: this.activeColumnIndex() ?? 0 });
  }

  addRowBefore(dom, rowIndex) {
    this.clearVisualSort();
    const matrix = this.readCellMatrix();
    if (!matrix.headerCells.length) return;
    const insertAt = Math.min(Math.max(rowIndex, 0), matrix.rows.length);
    matrix.rows.splice(insertAt, 0, new Array(matrix.headerCells.length).fill(''));
    this.commitMatrix(matrix, dom, { row: insertAt + 1, col: this.activeColumnIndex() ?? 0 });
  }

  removeRowAt(dom, rowIndex) {
    this.clearVisualSort();
    const matrix = this.readCellMatrix();
    if (matrix.rows.length <= 1) return;
    if (rowIndex < 0 || rowIndex >= matrix.rows.length) return;
    matrix.rows.splice(rowIndex, 1);
    const focusRow = Math.min(rowIndex, matrix.rows.length - 1) + 1;
    this.commitMatrix(matrix, dom, { row: focusRow, col: this.activeColumnIndex() ?? 0 });
  }

  addColumnAfter(dom, colIndex) {
    this.clearVisualSort();
    const matrix = this.readCellMatrix();
    if (!matrix.headerCells.length) return;
    const insertAt = Math.min(Math.max(colIndex + 1, 0), matrix.headerCells.length);
    matrix.headerCells.splice(insertAt, 0, '');
    matrix.rows = matrix.rows.map((row) => {
      const next = row.slice();
      next.splice(insertAt, 0, '');
      return next;
    });
    const alignments = normalizeRow(this.tableData.alignments, matrix.headerCells.length - 1).map(
      (value) => value ?? null
    );
    alignments.splice(insertAt, 0, null);
    matrix.alignments = alignments;
    this.commitMatrix(matrix, dom, { row: this.activeTarget.row, col: insertAt });
  }

  addColumnBefore(dom, colIndex) {
    this.clearVisualSort();
    const matrix = this.readCellMatrix();
    if (!matrix.headerCells.length) return;
    const insertAt = Math.min(Math.max(colIndex, 0), matrix.headerCells.length);
    matrix.headerCells.splice(insertAt, 0, '');
    matrix.rows = matrix.rows.map((row) => {
      const next = row.slice();
      next.splice(insertAt, 0, '');
      return next;
    });
    const alignments = normalizeRow(this.tableData.alignments, matrix.headerCells.length - 1).map(
      (value) => value ?? null
    );
    alignments.splice(insertAt, 0, null);
    matrix.alignments = alignments;
    this.commitMatrix(matrix, dom, { row: this.activeTarget.row, col: insertAt });
  }

  removeColumnAt(dom, colIndex) {
    this.clearVisualSort();
    const matrix = this.readCellMatrix();
    if (matrix.headerCells.length <= 1) return;
    if (colIndex < 0 || colIndex >= matrix.headerCells.length) return;
    matrix.headerCells.splice(colIndex, 1);
    matrix.rows = matrix.rows.map((row) => {
      const next = row.slice();
      next.splice(colIndex, 1);
      return next;
    });
    const alignments = normalizeRow(this.tableData.alignments, matrix.headerCells.length + 1).map(
      (value) => value ?? null
    );
    alignments.splice(colIndex, 1);
    matrix.alignments = alignments;
    const focusCol = Math.min(colIndex, matrix.headerCells.length - 1);
    this.commitMatrix(matrix, dom, { row: this.activeTarget.row, col: focusCol });
  }

  cellDiagnostics(rowIndex, colIndex): TableCellDiagnostics[] {
    return this.tableData.diagnostics?.[rowIndex]?.[colIndex] ?? [];
  }

  wireInput(input, rowEl, rowInputs, container, rowIndex, colIndex, preview) {
    const refreshPreview = () => {
      this.renderCellPreview(
        preview,
        input.value,
        this.cellDiagnostics(rowIndex, colIndex),
        this.cellSourceRange(rowIndex, colIndex)
      );
    };
    const notifySelectionChange = () => {
      this.emitTableSelectionChange(container);
    };
    const getCollapsedCaretLineInfo = () => {
      const start = input.selectionStart ?? 0;
      const end = input.selectionEnd ?? start;
      if (start !== end) return null;
      const value = input.value ?? '';
      const prevNl = value.lastIndexOf('\n', Math.max(0, start - 1));
      const nextNl = value.indexOf('\n', start);
      const lineStart = prevNl + 1;
      return {
        column: start - lineStart,
        isFirstLine: lineStart === 0,
        isLastLine: nextNl < 0,
      };
    };
    const onArrowVertical = (event, direction) => {
      if (event.defaultPrevented) return false;
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return false;
      if (direction !== 'up' && direction !== 'down') return false;

      const caretInfo = getCollapsedCaretLineInfo();
      if (!caretInfo) return false;

      const atBoundary = direction === 'up' ? caretInfo.isFirstLine : caretInfo.isLastLine;
      if (!atBoundary) return false;

      const nextRow = direction === 'up' ? rowIndex - 1 : rowIndex + 1;
      event.preventDefault();
      event.stopPropagation();

      const nextInput = this.domRefs?.allRowInputs?.[nextRow]?.[colIndex];
      if (nextInput instanceof HTMLTextAreaElement) {
        const nextCaret = Math.min(caretInfo.column, nextInput.value.length);
        return this.focusTableInput(nextInput, nextCaret);
      }

      return this.moveVerticalOutOfTable(container, direction, caretInfo.column);
    };

    input.addEventListener('input', () => {
      this.hasPendingCellEdits = true;
      // The preview layer is hidden while editing. Rebuilding it on each keystroke
      // recreates inline image DOM and resets image load opacity, which causes flicker.
      this.resizeRow(rowEl, rowInputs);
      this.scheduleLayout();
      notifySelectionChange();
    });
    input.addEventListener('select', notifySelectionChange);
    input.addEventListener('keyup', notifySelectionChange);
    input.addEventListener('pointerup', notifySelectionChange);
    input.addEventListener('keydown', (event) => {
      const direction = event.key === 'ArrowUp' ? 'up' : event.key === 'ArrowDown' ? 'down' : null;
      if (direction) onArrowVertical(event, direction);
    });
    input.addEventListener('focus', () => {
      this.clearVisualSort();
      this.setCellEditingState(input, true);
      this.setTableInteractionActive(container, true);
      this.setSingleCellSelection({ row: rowIndex, col: colIndex });
      notifySelectionChange();
    });
    input.addEventListener('blur', (event) => {
      refreshPreview();
      this.setCellEditingState(input, false);
      notifySelectionChange();
      const nextTarget = event.relatedTarget;
      if (nextTarget instanceof Node && container.contains(nextTarget)) return;
      this.commit(container);
      requestAnimationFrame(() => {
        if (this.hasFocusedTableInput(container)) return;
        this.setTableInteractionActive(container, false);
      });
    });
  }

  resizeRow(row, rowInputs = null) {
    if (!row) return;
    const textareas = rowInputs ?? Array.from(row.querySelectorAll('textarea'));
    if (textareas.length === 0) return;

    let maxHeight = 0;
    for (const textarea of textareas) {
      textarea.style.height = 'auto';
      maxHeight = Math.max(maxHeight, textarea.scrollHeight);
    }

    for (const textarea of textareas) {
      textarea.style.height = `${maxHeight}px`;
    }
  }

  resizeAllRows() {
    if (!this.domRefs) return;
    for (const entry of this.domRefs.rowEntries) {
      this.resizeRow(entry.row, entry.inputs);
    }
  }

  measureChPx(container) {
    if (this.chPx > 0) return this.chPx;
    const probe = document.createElement('span');
    probe.textContent = '0';
    probe.style.position = 'absolute';
    probe.style.visibility = 'hidden';
    probe.style.width = '1ch';
    container.appendChild(probe);
    this.chPx = probe.getBoundingClientRect().width || 8;
    probe.remove();
    return this.chPx;
  }

  fitColumnWidths() {
    if (!this.domRefs) return false;
    const { shell, wrap, table, colEls, headerInputs, rowInputs } = this.domRefs;
    const chPx = this.measureChPx(wrap);
    const cellChromePx = cellHorizontalPaddingCh * chPx + cellBorderPx;
    const minPx = minColumnWidthCh * chPx + cellChromePx;
    const maxPx = maxColumnWidthCh * chPx + cellChromePx;
    const livePreferredCh = computePreferredColumnCharWidthsFromInputs(
      headerInputs,
      rowInputs,
      this.tableData.colCount
    );
    const preferredPx = livePreferredCh.map((ch) => Math.max(minPx, Math.min(maxPx, ch * chPx + cellChromePx)));

    const nextAppliedWidths = new Array(colEls.length);
    let changed = colEls.length !== this.lastAppliedWidths.length;
    let targetTotal = 0;
    for (let i = 0; i < colEls.length; i++) {
      const widthPx = Math.max(1, Math.round(preferredPx[i] ?? minPx));
      nextAppliedWidths[i] = widthPx;
      targetTotal += widthPx;
      if (!changed && this.lastAppliedWidths[i] !== widthPx) changed = true;
      colEls[i].style.width = `${widthPx}px`;
      colEls[i].style.minWidth = `${widthPx}px`;
      colEls[i].style.maxWidth = 'none';
    }
    this.lastAppliedWidths = nextAppliedWidths;
    table.style.width = `${targetTotal}px`;
    table.style.minWidth = `${targetTotal}px`;
    table.style.maxWidth = 'none';
    this.updateApplySortAnchor(shell, wrap, targetTotal);
    return changed;
  }

  updateApplySortAnchor(shell: HTMLElement, wrap: HTMLElement, tableWidth: number) {
    const visibleWidth = Math.max(0, Math.floor(wrap.clientWidth));
    const visibleTableRightEdge = Math.max(0, tableWidth - wrap.scrollLeft);
    const anchor = Math.min(visibleWidth, visibleTableRightEdge);
    shell.style.setProperty('--meo-html-table-right-edge', `${anchor}px`);
  }

  recalcLayout() {
    const widthsChanged = this.fitColumnWidths();
    if (this.pendingResizeRows || widthsChanged) {
      this.resizeAllRows();
    }
  }

  scheduleLayout({ resizeRows = false } = {}) {
    if (resizeRows) this.pendingResizeRows = true;
    if (this.layoutFrame) return;
    this.layoutFrame = requestAnimationFrame(() => {
      this.layoutFrame = 0;
      this.recalcLayout();
      this.pendingResizeRows = false;
    });
  }

  renderCellPreview(
    preview,
    value,
    diagnostics: TableCellDiagnostics[] = [],
    sourceRange: TableCellRange | null = null
  ) {
    if (!(preview instanceof HTMLElement)) return;
    renderTableCellInlinePreview(preview, value ?? '', diagnostics, this.searchState, sourceRange);
  }

  refreshCellPreviewFromInput(input) {
    if (!(input instanceof HTMLTextAreaElement)) return;
    const preview = input.parentElement?.querySelector('.meo-md-html-table-cell-preview');
    const coords = this.parseCellCoords(input.dataset.tableRow, input.dataset.tableCol);
    this.renderCellPreview(
      preview,
      input.value,
      coords ? this.cellDiagnostics(coords.row, coords.col) : [],
      coords ? this.cellSourceRange(coords.row, coords.col) : null
    );
  }

  setCellEditingState(input, isEditing) {
    const content = input?.parentElement;
    if (!(content instanceof HTMLElement)) return;
    content.classList.toggle('is-editing', isEditing);
  }

  refreshAllCellPreviews() {
    if (!this.domRefs) return;
    for (let row = 0; row < this.domRefs.allRowInputs.length; row += 1) {
      const inputs = this.domRefs.allRowInputs[row];
      for (let col = 0; col < inputs.length; col += 1) {
        this.refreshCellPreviewFromInput(inputs[col]);
      }
    }
    this.scheduleLayout({ resizeRows: true });
  }

  setSearchState(searchState: TableSearchState | null) {
    this.searchState = searchState?.text ? searchState : null;
    this.refreshAllCellPreviews();
  }

  createCellPreview(value, diagnostics: TableCellDiagnostics[] = [], sourceRange: TableCellRange | null = null) {
    const preview = document.createElement('div');
    preview.className = 'meo-md-html-table-cell-preview';
    preview.setAttribute('aria-hidden', 'true');
    this.renderCellPreview(preview, value, diagnostics, sourceRange);
    return preview;
  }

  cellSourceRange(rowIndex, colIndex): TableCellRange | null {
    return this.tableData.sourceRanges?.[rowIndex]?.[colIndex] ?? null;
  }

  createCellInput(value, rowIndex, colIndex) {
    const input = document.createElement('textarea');
    input.rows = 1;
    input.spellcheck = true;
    input.value = value;
    input.dataset.tableRow = String(rowIndex);
    input.dataset.tableCol = String(colIndex);
    const sourceRange = this.cellSourceRange(rowIndex, colIndex);
    if (sourceRange) {
      input.dataset.tableCellFrom = String(sourceRange.from);
      input.dataset.tableCellTo = String(sourceRange.to);
    }
    return input;
  }

  createCellEditor(value, rowEl, rowInputs, container, rowIndex, colIndex) {
    const content = document.createElement('div');
    content.className = 'meo-md-html-table-cell-content';
    const preview = this.createCellPreview(
      value,
      this.cellDiagnostics(rowIndex, colIndex),
      this.cellSourceRange(rowIndex, colIndex)
    );
    const input = this.createCellInput(value, rowIndex, colIndex);
    this.wireInput(input, rowEl, rowInputs, container, rowIndex, colIndex, preview);
    content.append(preview, input);
    return { content, input };
  }

  createToolbarIcon(icon: TableToolbarIcon) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', '18');
    svg.setAttribute('height', '18');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '2');
    svg.setAttribute('stroke-linecap', 'round');
    svg.setAttribute('stroke-linejoin', 'round');
    svg.setAttribute('aria-hidden', 'true');
    svg.setAttribute('class', `meo-md-html-table-toolbar-icon ${icon.className}`);

    for (const d of icon.paths) {
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path.setAttribute('d', d);
      svg.appendChild(path);
    }

    return svg;
  }

  createToolbarButton(label, icon: TableToolbarIcon, onClick) {
    const button = document.createElement('button');
    button.type = 'button';
    button.tabIndex = -1;
    button.className = 'meo-md-html-table-toolbar-btn';
    button.title = label;
    button.setAttribute('aria-label', label);
    button.appendChild(this.createToolbarIcon(icon));
    button.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      onClick();
    });
    button.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
    });
    return button;
  }

  createTableToolbar(container) {
    const toolbar = document.createElement('div');
    toolbar.className = 'meo-md-html-table-toolbar';
    toolbar.setAttribute('aria-label', 'Table actions');

    const insertRowAbove = this.createToolbarButton('Insert row above', tableToolbarIcons.rowInsertTop, () => {
      this.insertRowAboveTarget(container);
    });
    const insertRowBelow = this.createToolbarButton('Insert row below', tableToolbarIcons.rowInsertBottom, () => {
      this.insertRowBelowTarget(container);
    });
    const deleteRow = this.createToolbarButton('Delete row', tableToolbarIcons.rowRemove, () => {
      this.deleteTargetRow(container);
    });
    const insertColumnLeft = this.createToolbarButton('Insert column left', tableToolbarIcons.columnInsertLeft, () => {
      this.insertColumnLeftTarget(container);
    });
    const insertColumnRight = this.createToolbarButton(
      'Insert column right',
      tableToolbarIcons.columnInsertRight,
      () => {
        this.insertColumnRightTarget(container);
      }
    );
    const deleteColumn = this.createToolbarButton('Delete column', tableToolbarIcons.columnRemove, () => {
      this.deleteTargetColumn(container);
    });
    deleteRow.classList.add('meo-md-html-table-toolbar-delete-btn');
    deleteColumn.classList.add('meo-md-html-table-toolbar-delete-btn');

    toolbar.append(insertRowAbove, insertRowBelow, insertColumnLeft, insertColumnRight, deleteRow, deleteColumn);

    return {
      toolbar,
      buttons: {
        insertRowAbove,
        insertRowBelow,
        deleteRow,
        insertColumnLeft,
        insertColumnRight,
        deleteColumn,
      },
    };
  }

  toDOM(view: EditorView) {
    this.view = view;
    const existingSearchState = (view.dom as any).__meoSearchState;
    if (existingSearchState && typeof existingSearchState === 'object') {
      this.searchState = existingSearchState.text ? existingSearchState : null;
    }
    const shell = document.createElement('div');
    shell.className = 'meo-md-html-table-shell';
    const wrap = document.createElement('div');
    wrap.className = 'meo-md-html-table-wrap';
    if (Number.isFinite(this.tableData.startLine)) {
      shell.dataset.meoRenderedBlockStartLine = String(this.tableData.startLine);
    }
    if (Number.isFinite(this.tableData.endLine)) {
      shell.dataset.meoRenderedBlockEndLine = String(this.tableData.endLine);
    }
    shell.dataset.meoRenderedBlockKind = 'table';
    const { toolbar, buttons: toolbarButtons } = this.createTableToolbar(wrap);

    const applySortButton = document.createElement('button');
    applySortButton.type = 'button';
    applySortButton.tabIndex = -1;
    applySortButton.className = 'meo-md-html-apply-sort-btn';
    applySortButton.textContent = 'Apply Sort';
    applySortButton.title = 'Apply current sort to markdown';
    applySortButton.hidden = true;
    applySortButton.addEventListener('pointerdown', (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      this.applyCurrentSort(wrap);
    });
    applySortButton.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
    });

    const table = document.createElement('table');
    table.className = 'meo-md-html-table';
    const colgroup = document.createElement('colgroup');
    const colEls = [];
    for (let col = 0; col < this.tableData.colCount; col++) {
      const colEl = document.createElement('col');
      colEls.push(colEl);
      colgroup.appendChild(colEl);
    }
    table.appendChild(colgroup);
    const rowEntries = [];
    const headerInputs = [];
    const cellGrid = [];
    const allRowInputs = [];
    const sortButtons = [];

    const thead = document.createElement('thead');
    const headerRow = document.createElement('tr');
    rowEntries.push({ row: headerRow, inputs: headerInputs });
    const headerCells = [];
    for (let col = 0; col < this.tableData.colCount; col++) {
      const th = document.createElement('th');
      th.dataset.tableRow = '0';
      th.dataset.tableCol = String(col);
      const { content, input } = this.createCellEditor(
        this.tableData.headerCells[col] ?? '',
        headerRow,
        headerInputs,
        wrap,
        0,
        col
      );
      headerInputs.push(input);
      headerCells.push(th);
      th.appendChild(content);

      const sortBtn = document.createElement('button');
      sortBtn.type = 'button';
      sortBtn.tabIndex = -1;
      sortBtn.className = 'meo-md-html-sort-btn';
      this.setSortButtonIcon(sortBtn, null);
      sortBtn.title = 'Sort column descending';
      sortBtn.setAttribute('aria-label', 'Sort column descending');
      sortBtn.setAttribute('aria-pressed', 'false');
      sortBtn.addEventListener('pointerdown', (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.stopPropagation();
        this.sortByColumn(wrap, col);
      });
      sortBtn.addEventListener('click', (event) => {
        event.preventDefault();
        event.stopPropagation();
      });
      sortButtons.push(sortBtn);
      th.appendChild(sortBtn);
      headerRow.appendChild(th);
    }
    cellGrid.push(headerCells);
    allRowInputs.push(headerInputs);
    thead.appendChild(headerRow);
    table.appendChild(thead);

    const tbody = document.createElement('tbody');
    const bodyRowInputs = [];
    const sourceBodyRows = [];
    const sourceBodyCellGrid = [];
    for (let rowIdx = 0; rowIdx < this.tableData.rows.length; rowIdx++) {
      const tr = document.createElement('tr');
      const inputs = [];
      rowEntries.push({ row: tr, inputs });
      const bodyCells = [];
      const tableRowIndex = rowIdx + 1;
      for (let col = 0; col < this.tableData.colCount; col++) {
        const td = document.createElement('td');
        td.dataset.tableRow = String(tableRowIndex);
        td.dataset.tableCol = String(col);
        const { content, input } = this.createCellEditor(
          this.tableData.rows[rowIdx][col] ?? '',
          tr,
          inputs,
          wrap,
          tableRowIndex,
          col
        );
        inputs.push(input);
        bodyCells.push(td);
        td.appendChild(content);

        tr.appendChild(td);
      }
      cellGrid.push(bodyCells);
      allRowInputs.push(inputs);
      bodyRowInputs.push(inputs);
      sourceBodyRows.push(tr);
      sourceBodyCellGrid.push(bodyCells);
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);

    wrap.append(table);
    shell.append(toolbar, wrap, applySortButton);
    this.domRefs = {
      shell,
      wrap,
      table,
      tbody,
      container: shell,
      toolbar,
      colEls,
      rowEntries,
      headerInputs,
      rowInputs: bodyRowInputs,
      allRowInputs,
      cellGrid,
      sourceBodyRows,
      sourceBodyRowInputs: bodyRowInputs,
      sourceBodyCellGrid,
      sortButtons,
      applySortButton,
      toolbarButtons,
    };
    this.updateActionTargetStyles();
    this.wireTableSelection(table);
    this.pendingResizeRows = true;
    this.scheduleLayout({ resizeRows: true });

    if (typeof ResizeObserver !== 'undefined') {
      const observer = new ResizeObserver(() => {
        this.chPx = 0;
        this.scheduleLayout({ resizeRows: true });
      });
      observer.observe(wrap);
      const resizeTargets = [view?.contentDOM, view?.scrollDOM, view?.dom, shell.parentElement];
      for (const target of resizeTargets) {
        if (target && target !== wrap) observer.observe(target);
      }
      wrap._meoTableResizeObserver = observer;
    }
    const onTableScroll = () => {
      const tableWidth = this.lastAppliedWidths.reduce((sum, width) => sum + width, 0);
      this.updateApplySortAnchor(shell, wrap, tableWidth);
    };
    const onSearchStateChange = (event) => {
      const detail = event instanceof CustomEvent ? event.detail : null;
      this.setSearchState(detail && typeof detail === 'object' ? detail : null);
    };
    wrap.addEventListener('scroll', onTableScroll);
    view.dom.addEventListener(tableSearchStateEventName, onSearchStateChange);
    this.cleanupFns.push(() => {
      wrap.removeEventListener('scroll', onTableScroll);
      view.dom.removeEventListener(tableSearchStateEventName, onSearchStateChange);
    });
    return shell;
  }

  ignoreEvent() {
    return true;
  }

  destroy(dom) {
    for (const cleanup of this.cleanupFns) cleanup();
    this.cleanupFns = [];
    dom?._meoTableResizeObserver?.disconnect();
    if (this.layoutFrame) {
      cancelAnimationFrame(this.layoutFrame);
      this.layoutFrame = 0;
    }
    this.domRefs = null;
    this.view = null;
    this.selectionAnchor = null;
    this.selectionRange = null;
    this.selectionPointerId = null;
    this.isDraggingSelection = false;
    this.hasPendingCellEdits = false;
    this.sortState = null;
  }
}

export function isTableDelimiterLine(lineText) {
  return tableDelimiterRegex.test(lineText);
}

export function parseTableInfo(state, tableNode) {
  const data = buildTableData(state, tableNode);
  const { from, to, lines, delimiterIdx, headerLine, dataLines, alignments, colCount, startLine, endLine } = data;

  const parseRow = (line) => ({
    from: line.from,
    to: line.to,
    lineNo: line.lineNo,
    lineFrom: line.from,
    lineTo: line.to,
    cells: line.cells.map((content, index) => ({
      from: line.segments[index]?.from ?? line.from,
      to: line.segments[index]?.to ?? line.from,
      content,
    })),
  });

  return {
    from,
    to,
    startLine,
    endLine,
    headerRow: headerLine ? parseRow(headerLine) : null,
    delimiterRow:
      delimiterIdx >= 0
        ? {
            from: lines[delimiterIdx].from,
            to: lines[delimiterIdx].to,
            lineNo: lines[delimiterIdx].lineNo,
            lineFrom: lines[delimiterIdx].from,
            lineTo: lines[delimiterIdx].to,
            alignments,
          }
        : null,
    rows: dataLines.map(parseRow),
    columnCount: colCount,
  };
}

export function addTableDecorations(builder, state, tableNode, diagnostics: EditorDiagnostic[] = []) {
  const data = buildTableData(state, tableNode);
  addTableWidgetDecoration(builder, data, diagnostics);
}

export function addTableDecorationsForLineRange(
  builder,
  state,
  startLineNo,
  endLineNo,
  diagnostics: EditorDiagnostic[] = []
) {
  const data = buildTableDataForLineRange(state, startLineNo, endLineNo);
  addTableWidgetDecoration(builder, data, diagnostics);
}

function collectCellDiagnostics(
  diagnostics: EditorDiagnostic[],
  segment: { from: number; to: number } | undefined
): TableCellDiagnostics[] {
  if (!segment || !Array.isArray(diagnostics) || diagnostics.length === 0) {
    return [];
  }
  return diagnostics
    .filter((diagnostic) => diagnostic.from < segment.to && diagnostic.to > segment.from)
    .map((diagnostic) => ({
      from: Math.max(0, diagnostic.from - segment.from),
      to: Math.max(0, Math.min(diagnostic.to, segment.to) - segment.from),
      severity: diagnostic.severity,
      message: diagnostic.message,
      source: diagnostic.source,
      code: diagnostic.code,
    }))
    .filter((diagnostic) => diagnostic.to > diagnostic.from);
}

function collectTableDiagnostics(data, diagnostics: EditorDiagnostic[]): TableCellDiagnostics[][][] {
  const rows = [];
  const { headerLine, dataLines, colCount } = data;
  if (!headerLine || colCount <= 0) {
    return rows;
  }

  const collectRow = (line) =>
    Array.from({ length: colCount }, (_value, index) => collectCellDiagnostics(diagnostics, line.segments[index]));

  rows.push(collectRow(headerLine));
  for (const line of dataLines) {
    rows.push(collectRow(line));
  }
  return rows;
}

function collectTableSourceRanges(data): TableCellRange[][] {
  const rows = [];
  const { headerLine, dataLines, colCount } = data;
  if (!headerLine || colCount <= 0) {
    return rows;
  }

  const collectRow = (line) =>
    Array.from({ length: colCount }, (_value, index) => {
      const segment = line.segments[index];
      return segment ? { from: segment.from, to: segment.to } : { from: line.from, to: line.from };
    });

  rows.push(collectRow(headerLine));
  for (const line of dataLines) {
    rows.push(collectRow(line));
  }
  return rows;
}

function addTableWidgetDecoration(builder, data, diagnostics: EditorDiagnostic[] = []) {
  const { from, to, headerLine, dataLines, alignments, colCount, startLine, endLine } = data;
  if (colCount === 0 || !headerLine) return;

  const indent = /^(\s*)/.exec(headerLine.text)?.[1] ?? '';
  const normalizedAlignments = normalizeRow(alignments, colCount).map((value) => value ?? null);
  const headerCells = normalizeRow(headerLine.cells, colCount);
  const rows = dataLines.map((line) => normalizeRow(line.cells, colCount));
  const signature = JSON.stringify({
    colCount,
    headerCells,
    rows,
    normalizedAlignments,
    diagnostics: collectTableDiagnostics(data, diagnostics),
  });

  builder.push(
    Decoration.replace({
      block: true,
      widget: new HtmlTableWidget({
        from,
        to,
        indent,
        colCount,
        alignments: normalizedAlignments,
        headerCells,
        rows,
        signature,
        startLine,
        endLine,
        diagnostics: collectTableDiagnostics(data, diagnostics),
        sourceRanges: collectTableSourceRanges(data),
      }),
    }).range(from, to)
  );
}

function buildSourceTableHeaderDecorations(state) {
  const ranges = [];
  const tree = syntaxTree(state);
  const parsedTableRanges = [];
  const decoratedHeaderLines = new Set();

  tree.iterate({
    enter(node) {
      if (node.name !== 'Table') return;

      const data = buildTableData(state, node);
      if (!data.headerLine) return;
      parsedTableRanges.push({ from: data.from, to: data.to });

      addSourceHeaderLineDecorations(ranges, data.headerLine);
      decoratedHeaderLines.add(data.headerLine.lineNo);
    },
  });

  for (let lineNo = 2; lineNo <= state.doc.lines; lineNo += 1) {
    const delimiterLine = state.doc.line(lineNo);
    const delimiterText = state.doc.sliceString(delimiterLine.from, delimiterLine.to);
    if (!isTableDelimiterLine(delimiterText)) continue;

    const headerLineNo = lineNo - 1;
    if (decoratedHeaderLines.has(headerLineNo)) continue;
    const headerLine = state.doc.line(headerLineNo);
    const headerText = state.doc.sliceString(headerLine.from, headerLine.to);
    if (!isTableContentLine(headerText)) continue;
    if (overlapsParsedTableRange(headerLine.from, delimiterLine.to, parsedTableRanges)) continue;
    if (isPositionInsideCodeBlock(tree, headerLine.from)) continue;

    const parsedHeaderLine = parseTableLine(headerLineNo, headerLine.from, headerLine.to, headerText);
    addSourceHeaderLineDecorations(ranges, parsedHeaderLine);
    decoratedHeaderLines.add(headerLineNo);
  }

  return Decoration.set(ranges, true);
}

function addSourceHeaderLineDecorations(ranges, line) {
  ranges.push(sourceTableHeaderLineDeco.range(line.from));
  for (const seg of line.segments) {
    ranges.push(sourceTableHeaderCellDeco.range(seg.from, seg.to));
  }
}

function overlapsParsedTableRange(from, to, ranges) {
  return ranges.some((range) => from < range.to && to > range.from);
}

function isPositionInsideCodeBlock(tree, pos) {
  let node = tree.resolveInner(pos, 1);
  while (node) {
    if (node.name === 'FencedCode' || node.name === 'CodeBlock') return true;
    node = node.parent;
  }
  return false;
}

export const sourceTableHeaderLineField = StateField.define({
  create(state) {
    try {
      return buildSourceTableHeaderDecorations(state);
    } catch {
      return Decoration.none;
    }
  },
  update(decorations, transaction) {
    if (!transaction.docChanged) {
      return decorations;
    }
    try {
      return buildSourceTableHeaderDecorations(transaction.state);
    } catch {
      return decorations;
    }
  },
  provide: (field) => EditorView.decorations.from(field),
});

export function insertTable(view, selection, cols = 3, rows = 2) {
  const line = view.state.doc.lineAt(selection.from);
  const lineText = view.state.doc.sliceString(line.from, line.to);
  const leadingWhitespace = /^(\s*)/.exec(lineText)?.[1] ?? '';

  const headerCells = Array.from({ length: cols }, () => '  ').join('|');
  const separatorCells = Array.from({ length: cols }, () => ' --- ').join('|');
  const bodyRows = Array.from({ length: rows }, () => {
    const cells = Array.from({ length: cols }, () => '  ').join('|');
    return `${leadingWhitespace}|${cells}|`;
  }).join('\n');

  const table = `${leadingWhitespace}|${headerCells}|\n${leadingWhitespace}|${separatorCells}|\n${bodyRows}`;

  view.dispatch({
    changes: { from: line.from, to: line.to, insert: table },
    selection: { anchor: line.from + leadingWhitespace.length + 2 },
  });
}
