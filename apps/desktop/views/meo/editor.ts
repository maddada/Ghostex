// @ts-nocheck
import {
  EditorState,
  Compartment,
  Transaction,
  StateEffect,
  StateField,
  RangeSetBuilder,
  type ChangeSpec,
  type Extension,
} from '@codemirror/state';
import {
  EditorView,
  keymap,
  highlightActiveLine,
  lineNumbers,
  highlightActiveLineGutter,
  scrollPastEnd,
  Decoration,
  type ViewUpdate,
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentMore, indentLess, undo, redo } from '@codemirror/commands';
import { markdown, markdownKeymap, markdownLanguage } from '@codemirror/lang-markdown';
import { indentUnit, syntaxHighlighting, syntaxTree, forceParsing } from '@codemirror/language';
import { vim, Vim } from '@replit/codemirror-vim';
import { highlightStyle } from './theme';
import { shikiCodeHighlight } from './helpers/shikiDecorations';
import { liveModeExtensions } from './liveMode';
import { headingCollapseSharedExtensions, headingCollapseSourceSpacerExtensions } from './helpers/headingCollapse';
import { resolveCodeLanguage, insertCodeBlock, sourceCodeBlockField } from './helpers/codeBlocks';
import { sourceStrikeMarkerField } from './helpers/strikeMarkers';
import { sourceWikiMarkerField } from './helpers/wikiLinks';
import { sourceFileLinkField } from './helpers/sourceRawLinks';
import { sourceUrlBoundaryField } from './helpers/sourceUrlBoundaries';
import { sourceFootnoteMarkerField } from './helpers/sourceFootnotes';
import { getLinkHrefAtPointer, isPrimaryModifierPointerClick } from './helpers/linkNavigation';
import {
  gitDiffGutterBaselineExtensions,
  gitDiffGutterLiveRenderExtensions,
  gitDiffGutterRenderExtensions,
  setGitBaseline as applyGitBaseline,
} from './helpers/gitDiffGutter';
import { gitDiffLineHighlightsField } from './helpers/gitDiffLineHighlights';
import { createGitDiffOverviewRulerController } from './helpers/gitDiffOverviewRuler';
import { createGitBlameHoverController } from './helpers/gitBlameHover';
import { mergeConflictSourceExtensions } from './helpers/mergeConflicts';
import { resolvedSyntaxTree, extractHeadings, extractHeadingSections } from './helpers/markdownSyntax';
import {
  sourceListBorderField,
  sourceListMarkerField,
  listMarkerData,
  handleArrowLeftAtListContentStart,
  handleArrowRightAtListLineStart,
  handleBackspaceAtListContentStart,
  handleEnterAtListContentStart,
  handleEnterOnEmptyListItem,
  handleEnterContinueList,
  handleEnterBeforeNestedList,
  collectOrderedListRenumberChanges,
  indentListByTwoSpaces,
  outdentListByTwoSpaces,
} from './helpers/listMarkers';
import { insertTable, sourceTableHeaderLineField } from './helpers/tables';
import { parseFrontmatter, sourceFrontmatterField } from './helpers/frontmatter';
import { collectLatexMathRanges } from './helpers/math';
import {
  diagnosticDataField,
  diagnosticField,
  setDiagnosticsEffect,
  type EditorDiagnostic,
} from './helpers/diagnostics';

declare module '@codemirror/view' {
  interface EditorView {
    EDIT_CONTEXT?: boolean;
  }
}

type SearchOptions = {
  wholeWord?: boolean;
  caseSensitive?: boolean;
};

type SearchQueryState = {
  text: string;
  wholeWord: boolean;
  caseSensitive: boolean;
};

type SearchMatchRange = {
  start: number;
  end: number;
};

type InlineSelectionRange = {
  from: number;
  to: number;
  anchor: number;
  head: number;
  empty: boolean;
};

type MarkerReplacementContext = {
  contentStart: number;
  oldMarkerLen: number;
  isExistingTask: boolean;
};

const setSearchQueryEffect = StateEffect.define<SearchQueryState>();
const refreshDecorationsEffect = StateEffect.define();
const searchMatchMark = Decoration.mark({ class: 'meo-search-match' });
const activeSearchMatchMark = Decoration.mark({ class: 'meo-search-match meo-search-match-active' });
const tableSearchStateEventName = 'meo-search-state-change';
const existingListMarkerRegex = /^(\s*)([-+*]\s+\[[ xX~\-]\]|[-+*]|\d+[.)])\s+/;
const existingHeadingMarkerRegex = /^(\s*)(#{1,6})\s+/;
const existingTaskMarkerRegex = /^[-+*]\s+\[[ xX~\-]\]/;
const blockquoteLinePrefixRegex = /^[ \t]{0,3}(?:>[ \t]?)+/;
const quotedCodeBlockAncestorNames = new Set(['FencedCode', 'CodeBlock']);

const buildSearchDecorations = (state: EditorState, searchQuery: SearchQueryState) => {
  if (!searchQuery.text) {
    return Decoration.none;
  }

  const builder = new RangeSetBuilder();
  const doc = state.doc;
  const textValue = doc.toString();
  const matches = findSearchMatchRanges(textValue, searchQuery.text, searchQuery);
  const selection = state.selection.main;
  const selectionFrom = Math.min(selection.from, selection.to);
  const selectionTo = Math.max(selection.from, selection.to);
  for (const match of matches) {
    const mark = match.start === selectionFrom && match.end === selectionTo ? activeSearchMatchMark : searchMatchMark;
    builder.add(match.start, match.end, mark);
  }
  return builder.finish();
};

const searchQueryField = StateField.define({
  create() {
    return createSearchQueryState('');
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setSearchQueryEffect)) {
        return effect.value;
      }
    }
    return value;
  },
});

const searchMatchField = StateField.define<any>({
  create() {
    return Decoration.none;
  },
  update(value: any, tr: Transaction) {
    if (tr.docChanged || tr.selection) {
      const searchQuery = tr.state.field(searchQueryField);
      return buildSearchDecorations(tr.state, searchQuery);
    }

    for (const effect of tr.effects) {
      if (effect.is(setSearchQueryEffect)) {
        return buildSearchDecorations(tr.state, effect.value);
      }
    }

    return value;
  },
  provide(field: any) {
    return EditorView.decorations.from(field);
  },
});

export function createEditor({
  parent,
  text,
  onApplyChanges,
  onOpenLink,
  onSelectionChange,
  onRequestDiagnosticSuggestions = undefined,
  onViewportChange = undefined,
  onRequestGitBlame = undefined,
  onOpenGitRevisionForLine = undefined,
  onOpenGitWorktreeForLine = undefined,
  initialMode = 'source',
  initialTopLine = null,
  initialTopLineOffset = 0,
  initialLineNumbers = true,
  initialGitGutter = true,
  initialVimMode = false,
  initialVimKeybindings = [],
  initialVimLeader = '\\',
  initialDiagnostics = [],
  externalExtensions = [] as Extension[],
}) {
  // VS Code webviews can hit cross-origin window access issues in the EditContext path.
  // Disable it explicitly for stability in embedded Chromium.
  (EditorView as any).EDIT_CONTEXT = false;

  const modeCompartment = new Compartment();
  const gitGutterCompartment = new Compartment();
  const vimCompartment = new Compartment();
  const startMode = initialMode === 'live' ? 'live' : 'source';
  let lineNumbersVisible = initialLineNumbers !== false;
  let gitGutterVisible = initialGitGutter !== false;
  let vimModeEnabled = initialVimMode === true;
  let vimKeybindings = initialVimKeybindings;
  let vimLeader = initialVimLeader;
  let appliedVimKeybindings: Array<{ before: string; mode: string }> = [];
  let currentDiagnostics: EditorDiagnostic[] = Array.isArray(initialDiagnostics) ? initialDiagnostics : [];
  let lastDiagnosticClick: { key: string; from: number; to: number } | null = null;
  let pendingDiagnosticSuggestionRequest: {
    requestId: string;
    key: string;
    from: number;
    to: number;
    anchorX: number;
    anchorY: number;
    anchorBottomY: number;
  } | null = null;

  const expandVimLeader = (keys: string, leaderKey: string) => keys.replace(/<leader>/gi, leaderKey || '\\');
  const clearVimKeybindings = () => {
    for (const { before, mode } of appliedVimKeybindings) {
      try {
        Vim.unmap(before, mode);
      } catch {
        // Ignore stale or unsupported mappings.
      }
    }
    appliedVimKeybindings = [];
  };
  const applyVimKeybindings = (
    bindings: Array<{ before: string; after: string; mode: string; recursive: boolean }>,
    leaderKey: string
  ) => {
    clearVimKeybindings();
    try {
      Vim.setOption('leader', leaderKey);
    } catch {
      // Keep Vim usable if the embedded Vim implementation rejects the option.
    }
    for (const { before, after, mode, recursive } of bindings) {
      const mappedBefore = expandVimLeader(before, leaderKey);
      const mappedAfter = expandVimLeader(after, leaderKey);
      try {
        if (recursive) {
          Vim.map(mappedBefore, mappedAfter, mode);
        } else {
          Vim.noremap(mappedBefore, mappedAfter, mode);
        }
        appliedVimKeybindings.push({ before: mappedBefore, mode });
      } catch {
        // Ignore individual mappings that CodeMirror Vim cannot represent.
      }
    }
  };

  if (initialVimMode === true) {
    applyVimKeybindings(vimKeybindings, vimLeader);
  }
  let applyingExternal = false;
  let capturedPointerId = null;
  let inlineCodeClick = null;
  let checkboxClick = null;
  let frontmatterBoundaryClick = null;
  let view = null;
  let currentMode = startMode;
  let applyingRenumber = false;
  let lastSearchStateSignature = '';
  // External syncs may carry stale selections in their history entries.
  // Preserve the user's current cursor once on the next undo of such a change.
  let pendingExternalUndoSelectionPreserve = false;
  let tableInteractionActive = false;
  let onTableInteraction = null;
  let onTableOpenLink = null;
  let onTableSelectionChange = null;
  let onScroll = null;
  let gitBlameHover = null;
  let gitDiffOverviewRuler = null;
  let sourceLinkHoverPointerActive = false;
  const normalizeTopLineOffset = (value) => {
    if (!Number.isFinite(value)) {
      return 0;
    }
    return Math.max(0, Number(value));
  };
  const vimExtensionsForState = () => (vimModeEnabled ? vim() : []);
  const getLineStartOffset = (docText, targetLineNumber) => {
    const targetLine = Math.max(1, Math.floor(targetLineNumber));
    if (targetLine === 1) {
      return 0;
    }
    let line = 1;
    for (let index = 0; index < docText.length; index += 1) {
      if (docText.charCodeAt(index) !== 10) {
        continue;
      }
      line += 1;
      if (line === targetLine) {
        return index + 1;
      }
    }
    return docText.length;
  };
  const initialCursorPos = (() => {
    if (typeof initialTopLine === 'number' && Number.isFinite(initialTopLine)) {
      return getLineStartOffset(text ?? '', initialTopLine);
    }
    if (!text) {
      return 0;
    }
    const firstLineEnd = text.indexOf('\n');
    return firstLineEnd === -1 ? text.length : firstLineEnd;
  })();
  const targetElementFrom = (target) =>
    target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
  const openLinkIfModifierClick = (event, editorView) => {
    if (!isPrimaryModifierPointerClick(event)) {
      return false;
    }
    const href = getLinkHrefAtPointer(event, editorView);
    if (!href) {
      return false;
    }
    event.preventDefault();
    event.stopPropagation();
    onOpenLink?.(href);
    return true;
  };
  const setSourceLinkHoverCursor = (editorView, active) => {
    if (sourceLinkHoverPointerActive === active) {
      return;
    }
    sourceLinkHoverPointerActive = active;
    const cursor = active ? 'pointer' : '';
    editorView.dom.style.cursor = cursor;
    editorView.contentDOM.style.cursor = cursor;
  };
  const updateSourceLinkHoverCursor = (event, editorView) => {
    if (currentMode !== 'source') {
      setSourceLinkHoverCursor(editorView, false);
      return;
    }
    const target = event.target;
    if (!(target instanceof Node) || !editorView.contentDOM.contains(target)) {
      setSourceLinkHoverCursor(editorView, false);
      return;
    }
    const href = getLinkHrefAtPointer(event, editorView, { exactTextHit: true });
    setSourceLinkHoverCursor(editorView, Boolean(href));
  };
  const isLiveMode = (editorView) => editorView.dom.classList.contains('meo-mode-live');
  const isPlainPrimaryPointerEvent = (event) =>
    event.button === 0 && !event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey;
  const frontmatterBoundaryCursorEnd = (state, pos) => {
    const frontmatter = parseFrontmatter(state);
    if (!frontmatter) {
      return null;
    }
    const line = state.doc.lineAt(pos);
    const openingLineNo = state.doc.lineAt(frontmatter.openingFrom).number;
    const closingLineNo = state.doc.lineAt(frontmatter.closingFrom).number;
    if (line.number !== openingLineNo && line.number !== closingLineNo) {
      return null;
    }
    const lineText = state.doc.sliceString(line.from, line.to);
    const markerStart = lineText.indexOf('---');
    return markerStart >= 0 ? line.from + markerStart + 3 : null;
  };
  const emptyBlockquoteLineCursorEnd = (state, pos) => {
    const line = state.doc.lineAt(pos);
    const lineText = state.doc.sliceString(line.from, line.to);
    const quoteMatch = /^[ \t]{0,3}(?:>[ \t]?)+$/.exec(lineText);
    if (!quoteMatch) {
      return null;
    }

    const probePos = Math.min(line.to, line.from + 1);
    let node = resolvedSyntaxTree(state).resolveInner(probePos, 1);
    while (node) {
      if (node.name === 'Blockquote') {
        return line.from + quoteMatch[0].length;
      }
      node = node.parent;
    }

    return null;
  };
  const trackFrontmatterBoundaryClick = (event, editorView) => {
    frontmatterBoundaryClick = null;
    if (!isLiveMode(editorView) || !isPlainPrimaryPointerEvent(event)) {
      return;
    }
    const clickedPos = editorView.posAtCoords({ x: event.clientX, y: event.clientY });
    if (clickedPos === null) {
      return;
    }
    const cursorEnd = frontmatterBoundaryCursorEnd(editorView.state, clickedPos);
    if (cursorEnd === null) {
      return;
    }
    frontmatterBoundaryClick = {
      pointerId: event.pointerId,
      cursorEnd,
    };
  };

  const setTableInteractionActive = (active) => {
    if (!view || tableInteractionActive === active) {
      return;
    }
    tableInteractionActive = active;
    view.dom.classList.toggle('meo-table-interaction-active', active);
  };
  const tableEntryCellSelector = 'th[data-table-row][data-table-col], td[data-table-row][data-table-col]';
  const tableEntryProbeOffsetsY = [1, 4, 8, 12, 18];
  const focusTableEntryInput = (input) => {
    if (!(input instanceof HTMLTextAreaElement)) {
      return false;
    }
    input.focus({ preventScroll: true });
    const caret = input.value.length;
    input.setSelectionRange(caret, caret);
    input.closest(tableEntryCellSelector)?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
    return true;
  };
  const findTableEntryInput = (wrap, hit, direction) => {
    const cell = hit.closest(tableEntryCellSelector);
    if (cell instanceof HTMLElement && wrap.contains(cell)) {
      const cellInput = cell.querySelector('textarea');
      if (cellInput instanceof HTMLTextAreaElement) {
        return cellInput;
      }
    }

    if (direction === 'down') {
      const first = wrap.querySelector('textarea');
      return first instanceof HTMLTextAreaElement ? first : null;
    }

    const inputs = wrap.querySelectorAll('textarea');
    const last = inputs.length ? inputs[inputs.length - 1] : null;
    return last instanceof HTMLTextAreaElement ? last : null;
  };

  const tryEnterAdjacentTable = (editorView, direction) => {
    if ((direction !== 'down' && direction !== 'up') || currentMode !== 'live' || tableInteractionActive) {
      return false;
    }

    const selection = editorView.state.selection.main;
    if (!selection.empty) {
      return false;
    }

    const caretRect = editorView.coordsAtPos(selection.head);
    if (!caretRect) {
      return false;
    }

    const contentRect = editorView.contentDOM.getBoundingClientRect();
    if (!contentRect || contentRect.width <= 0 || contentRect.height <= 0) {
      return false;
    }

    const clampX = (x) => Math.min(Math.max(x, contentRect.left + 1), contentRect.right - 1);
    const probeXs = [
      clampX(caretRect.left + 1),
      clampX(contentRect.left + Math.min(24, Math.max(8, contentRect.width * 0.05))),
    ];

    for (const offsetY of tableEntryProbeOffsetsY) {
      const y = direction === 'down' ? caretRect.bottom + offsetY : caretRect.top - offsetY;
      if (y < 0 || y >= window.innerHeight) {
        if (direction === 'down' && y >= window.innerHeight) break;
        continue;
      }
      for (const x of probeXs) {
        const hit = document.elementFromPoint(x, y);
        if (!(hit instanceof Element)) {
          continue;
        }
        const wrap = hit.closest('.meo-md-html-table-wrap');
        if (!(wrap instanceof HTMLElement) || !editorView.dom.contains(wrap)) {
          continue;
        }
        const input = findTableEntryInput(wrap, hit, direction);
        if (!focusTableEntryInput(input)) {
          continue;
        }
        return true;
      }
    }

    return false;
  };

  const syncModeClasses = () => {
    if (!view) {
      return;
    }
    const isLiveModeActive = currentMode === 'live';
    view.dom.classList.toggle('meo-mode-live', currentMode === 'live');
    view.dom.classList.toggle('meo-mode-source', currentMode !== 'live');
    // Keep active typography vars explicitly synced to mode so source/live
    // font sizing and line-height don't depend on selector cascade.
    view.dom.style.setProperty(
      '--meo-active-editor-font',
      isLiveModeActive ? 'var(--meo-font-live)' : 'var(--meo-font-source)'
    );
    view.dom.style.setProperty(
      '--meo-active-editor-font-weight',
      isLiveModeActive ? 'var(--meo-font-live-weight)' : 'var(--meo-font-source-weight)'
    );
    view.dom.style.setProperty(
      '--meo-active-editor-font-size',
      isLiveModeActive ? 'var(--meo-font-live-size)' : 'var(--meo-font-source-size)'
    );
    view.dom.style.setProperty(
      '--meo-active-editor-line-height',
      isLiveModeActive ? 'var(--meo-line-height-live)' : 'var(--meo-line-height-source)'
    );
    if (currentMode !== 'source') {
      setSourceLinkHoverCursor(view, false);
    }
  };

  const syncLineNumbersVisibility = () => {
    if (!view) {
      return;
    }
    view.dom.classList.toggle('meo-line-numbers-hidden', !lineNumbersVisible);
  };

  const syncGitGutterVisibility = () => {
    if (!view) {
      return;
    }
    const shouldShowGitGutter = gitGutterVisible;
    view.dom.classList.toggle('meo-git-gutter-hidden', !shouldShowGitGutter);
    if (!shouldShowGitGutter) {
      gitBlameHover?.hide();
    }
    gitDiffOverviewRuler?.refresh();
  };

  const releasePointerCaptureIfHeld = (pointerId) => {
    if (!view || pointerId === null) {
      return;
    }
    if (view.dom.releasePointerCapture && view.dom.hasPointerCapture(pointerId)) {
      view.dom.releasePointerCapture(pointerId);
    }
  };

  const syncSelectionClass = () => {
    if (!view) {
      return;
    }
    const hasSelection = view.state.selection.ranges.some((range) => !range.empty);
    const hasSearchSelection = view.state.selection.ranges.some((range) =>
      isSearchMatchSelection(Math.min(range.from, range.to), Math.max(range.from, range.to))
    );
    view.dom.classList.toggle('has-selection', hasSelection);
    view.dom.classList.toggle('has-search-selection', hasSearchSelection);
  };

  const emitSearchStateChange = () => {
    if (!view) {
      return;
    }

    const searchQuery = view.state.field(searchQueryField);
    const selection = view.state.selection.main;
    const detail = {
      text: searchQuery.text,
      wholeWord: searchQuery.wholeWord,
      caseSensitive: searchQuery.caseSensitive,
      selectionFrom: Math.min(selection.from, selection.to),
      selectionTo: Math.max(selection.from, selection.to),
    };
    (view.dom as any).__meoSearchState = detail;
    const signature = JSON.stringify(detail);
    if (signature === lastSearchStateSignature) {
      return;
    }
    lastSearchStateSignature = signature;
    view.dom.dispatchEvent(new CustomEvent(tableSearchStateEventName, { detail }));
  };

  const getActiveTableInput = () => {
    if (!view) {
      return null;
    }
    const active = document.activeElement;
    if (!(active instanceof HTMLTextAreaElement)) {
      return null;
    }
    if (!view.dom.contains(active)) {
      return null;
    }
    return active.closest('.meo-md-html-table-wrap') ? active : null;
  };

  const getTableInputSourceRange = (input: HTMLTextAreaElement): { from: number; to: number } | null => {
    const from = Number.parseInt(input.dataset.tableCellFrom ?? '', 10);
    const to = Number.parseInt(input.dataset.tableCellTo ?? '', 10);
    if (!Number.isFinite(from) || !Number.isFinite(to) || from < 0 || to < from) {
      return null;
    }
    return { from, to };
  };

  const getTableInputDocumentSelection = (
    input: HTMLTextAreaElement
  ): { from: number; to: number; anchorX: number; anchorY: number; anchorBottomY: number } | null => {
    const sourceRange = getTableInputSourceRange(input);
    if (!sourceRange) {
      return null;
    }
    const rawStart = input.selectionStart ?? 0;
    const rawEnd = input.selectionEnd ?? rawStart;
    if (rawStart === rawEnd) {
      return null;
    }
    const selectionStart = Math.min(rawStart, rawEnd);
    const selectionEnd = Math.max(rawStart, rawEnd);
    const coords = measureTextareaSelectionStart(input, selectionStart);
    const lineHeight = parseFloat(getComputedStyle(input).lineHeight);
    return {
      from: sourceRange.from + selectionStart,
      to: sourceRange.from + selectionEnd,
      anchorX: coords.left,
      anchorY: coords.top,
      anchorBottomY: coords.top + (Number.isFinite(lineHeight) ? lineHeight : 20),
    };
  };

  const commitActiveTableInput = () => {
    if (!view) {
      return false;
    }
    const detail = { committed: false };
    document.dispatchEvent(
      new CustomEvent('meo-commit-table-edits', {
        detail,
      })
    );
    return detail.committed;
  };

  const measureTextareaSelectionStart = (input, index) => {
    const doc = input.ownerDocument;
    const mirror = doc.createElement('div');
    const marker = doc.createElement('span');
    const computed = window.getComputedStyle(input);

    mirror.style.position = 'fixed';
    mirror.style.left = '0';
    mirror.style.top = '0';
    mirror.style.visibility = 'hidden';
    mirror.style.pointerEvents = 'none';
    mirror.style.whiteSpace = 'pre-wrap';
    mirror.style.overflowWrap = 'break-word';
    mirror.style.wordBreak = 'break-word';
    mirror.style.boxSizing = computed.boxSizing;
    mirror.style.width = `${input.getBoundingClientRect().width}px`;
    mirror.style.minHeight = computed.height;
    mirror.style.padding = computed.padding;
    mirror.style.border = computed.border;
    mirror.style.font = computed.font;
    mirror.style.fontFamily = computed.fontFamily;
    mirror.style.fontSize = computed.fontSize;
    mirror.style.fontWeight = computed.fontWeight;
    mirror.style.fontStyle = computed.fontStyle;
    mirror.style.letterSpacing = computed.letterSpacing;
    mirror.style.lineHeight = computed.lineHeight;
    mirror.style.textTransform = computed.textTransform;
    mirror.style.textIndent = computed.textIndent;
    mirror.style.tabSize = computed.tabSize;

    mirror.textContent = input.value.slice(0, index);
    marker.textContent = '\u200b';
    mirror.appendChild(marker);
    doc.body.appendChild(mirror);

    const markerRect = marker.getBoundingClientRect();
    const mirrorRect = mirror.getBoundingClientRect();
    const inputRect = input.getBoundingClientRect();
    const coords = {
      left: inputRect.left + (markerRect.left - mirrorRect.left),
      top: inputRect.top + (markerRect.top - mirrorRect.top),
    };

    mirror.remove();
    return coords;
  };

  const getActiveTableSelectionState = (input) => {
    const selection = getTableInputDocumentSelection(input);
    if (!selection) return null;
    const diagnostic = diagnosticForRange(selection.from, selection.to);
    if (diagnostic) {
      requestDiagnosticSuggestionsFor(diagnostic, selection);
    }
    return {
      visible: true,
      from: selection.from,
      to: selection.to,
      align: diagnostic ? 'start' : undefined,
      anchorX: selection.anchorX,
      anchorY: selection.anchorY,
      anchorBottomY: selection.anchorBottomY,
    };
  };

  const updateActiveTableInput = (input, nextValue, anchor, head = anchor) => {
    input.value = nextValue;
    input.focus({ preventScroll: true });
    input.setSelectionRange(Math.min(anchor, head), Math.max(anchor, head), anchor <= head ? 'forward' : 'backward');
    input.dispatchEvent(new Event('input', { bubbles: true }));
    emitSelectionChange();
    return true;
  };

  const editActiveTableInputWithSelection = (input, transform) => {
    const rawStart = input.selectionStart ?? 0;
    const rawEnd = input.selectionEnd ?? rawStart;
    const start = Math.min(rawStart, rawEnd);
    const end = Math.max(rawStart, rawEnd);
    return transform(input.value, start, end);
  };

  const trimTrailingNewlines = (value, start, end) => {
    let nextEnd = end;
    while (nextEnd > start && value.slice(nextEnd - 1, nextEnd) === '\n') {
      nextEnd -= 1;
    }
    return nextEnd;
  };

  const wrapActiveTableInputSelection = (
    input,
    openMarker,
    closeMarker = openMarker,
    { toggle = true, selectWrapped = true } = {}
  ) => {
    return editActiveTableInputWithSelection(input, (value, start, end) => {
      if (start === end) {
        const insert = `${openMarker}${closeMarker}`;
        const nextValue = value.slice(0, start) + insert + value.slice(end);
        return updateActiveTableInput(input, nextValue, start + openMarker.length);
      }

      const trimmedEnd = trimTrailingNewlines(value, start, end);
      if (toggle) {
        const hasOpenMarker =
          start >= openMarker.length && value.slice(start - openMarker.length, start) === openMarker;
        const hasCloseMarker = value.slice(trimmedEnd, trimmedEnd + closeMarker.length) === closeMarker;
        if (hasOpenMarker && hasCloseMarker) {
          const nextValue =
            value.slice(0, start - openMarker.length) +
            value.slice(start, trimmedEnd) +
            value.slice(trimmedEnd + closeMarker.length);
          return updateActiveTableInput(input, nextValue, start - openMarker.length, trimmedEnd - openMarker.length);
        }
      }

      const nextValue =
        value.slice(0, start) + openMarker + value.slice(start, trimmedEnd) + closeMarker + value.slice(trimmedEnd);
      if (!selectWrapped) {
        const cursor = start + openMarker.length + (trimmedEnd - start) + closeMarker.length;
        return updateActiveTableInput(input, nextValue, cursor);
      }
      return updateActiveTableInput(input, nextValue, start + openMarker.length, trimmedEnd + openMarker.length);
    });
  };

  const insertFormatInActiveTableInput = (input, action) => {
    switch (action) {
      case 'inlineCode':
        return wrapActiveTableInputSelection(input, '`', '`', { toggle: false, selectWrapped: false });
      case 'kbd':
        return wrapActiveTableInputSelection(input, '<kbd>', '</kbd>');
      case 'bold':
        return wrapActiveTableInputSelection(input, '**');
      case 'italic':
        return wrapActiveTableInputSelection(input, '*');
      case 'lineover':
      case 'strike':
        return wrapActiveTableInputSelection(input, '~~');
      case 'link':
        return editActiveTableInputWithSelection(input, (value, start, end) => {
          if (start !== end) {
            const trimmedEnd = trimTrailingNewlines(value, start, end);
            const selectedText = value.slice(start, trimmedEnd);
            const insert = `[${selectedText}]()`;
            const nextValue = value.slice(0, start) + insert + value.slice(trimmedEnd);
            return updateActiveTableInput(input, nextValue, start + insert.length - 1);
          }

          const insert = '[]()';
          const nextValue = value.slice(0, start) + insert + value.slice(end);
          return updateActiveTableInput(input, nextValue, start + 3);
        });
      case 'wikiLink':
        return editActiveTableInputWithSelection(input, (value, start, end) => {
          if (start !== end) {
            const trimmedEnd = trimTrailingNewlines(value, start, end);
            const selectedText = value.slice(start, trimmedEnd);
            const insert = `[[${selectedText}]]`;
            const nextValue = value.slice(0, start) + insert + value.slice(trimmedEnd);
            return updateActiveTableInput(input, nextValue, start + insert.length);
          }

          const insert = '[[]]';
          const nextValue = value.slice(0, start) + insert + value.slice(end);
          return updateActiveTableInput(input, nextValue, start + 2);
        });
      default:
        return false;
    }
  };

  const forEachSelectedLine = (
    state: EditorState,
    callback: (line: { from: number; to: number; number: number }) => void
  ): void => {
    const seen = new Set<number>();
    for (const range of state.selection.ranges) {
      const fromLine = state.doc.lineAt(range.from).number;
      const toPos = Math.max(range.from, range.to - (range.empty ? 0 : 1));
      const toLine = state.doc.lineAt(toPos).number;
      for (let lineNumber = fromLine; lineNumber <= toLine; lineNumber += 1) {
        if (seen.has(lineNumber)) {
          continue;
        }
        seen.add(lineNumber);
        callback(state.doc.line(lineNumber));
      }
    }
  };

  const lineMarkerReplacementContext = (
    state: EditorState,
    line: { from: number; to: number }
  ): MarkerReplacementContext => {
    const lineText = state.doc.sliceString(line.from, line.to);
    const existingMarker = existingListMarkerRegex.exec(lineText);
    const existingHeading = existingHeadingMarkerRegex.exec(lineText);
    const leadingWhitespace = existingMarker?.[1] ?? existingHeading?.[1] ?? /^(\s*)/.exec(lineText)[1];

    const contentStart = line.from + leadingWhitespace.length;
    let oldMarkerLen = 0;
    if (existingMarker) {
      oldMarkerLen = existingMarker[0].length - leadingWhitespace.length;
    } else if (existingHeading) {
      oldMarkerLen = existingHeading[0].length - leadingWhitespace.length;
    }

    const isExistingTask = Boolean(existingMarker && existingTaskMarkerRegex.test(existingMarker[0]));
    return { contentStart, oldMarkerLen, isExistingTask };
  };

  const buildListFormatChangesForSelection = (state: EditorState, insert: string): ChangeSpec[] => {
    const changes: ChangeSpec[] = [];
    forEachSelectedLine(state, (line) => {
      const { contentStart, oldMarkerLen } = lineMarkerReplacementContext(state, line);
      changes.push({ from: contentStart, to: contentStart + oldMarkerLen, insert });
    });
    return changes;
  };

  const dispatchSelectedListFormatChanges = (
    state: EditorState,
    changes: ChangeSpec[],
    shouldRenumberOrdered: boolean
  ): void => {
    if (!changes.length) {
      return;
    }

    if (!shouldRenumberOrdered) {
      view.dispatch({ changes });
      return;
    }

    const withMarkers = state.update({ changes });
    const renumberChanges = collectOrderedListRenumberChanges(withMarkers.state);
    if (!renumberChanges.length) {
      view.dispatch(withMarkers);
      return;
    }

    view.dispatch(state.update({ changes }, { changes: renumberChanges, sequential: true }));
  };

  const isSearchMatchSelection = (from, to) => {
    if (!view || from >= to) {
      return false;
    }

    const searchQuery = view.state.field(searchQueryField);
    if (!searchQuery.text || to - from !== searchQuery.text.length) {
      return false;
    }

    let isMatchSelection = false;
    view.state.field(searchMatchField).between(from, to, (matchFrom, matchTo) => {
      if (matchFrom === from && matchTo === to) {
        isMatchSelection = true;
      }
    });
    return isMatchSelection;
  };

  const resolveNativeSelectionAnchor = (): { anchorX: number; anchorY: number; anchorBottomY: number } | null => {
    if (!view) {
      return null;
    }

    const nativeSelection = window.getSelection();
    if (!nativeSelection || nativeSelection.isCollapsed || nativeSelection.rangeCount === 0) {
      return null;
    }

    let topRect: DOMRect | null = null;
    for (let rangeIndex = 0; rangeIndex < nativeSelection.rangeCount; rangeIndex += 1) {
      const range = nativeSelection.getRangeAt(rangeIndex);
      const ancestor = range.commonAncestorContainer;
      if (!view.dom.contains(ancestor)) {
        continue;
      }

      const rects = range.getClientRects();
      for (let rectIndex = 0; rectIndex < rects.length; rectIndex += 1) {
        const rect = rects.item(rectIndex);
        if (!rect || (rect.width <= 0 && rect.height <= 0)) {
          continue;
        }
        if (!topRect || rect.top < topRect.top || (rect.top === topRect.top && rect.left < topRect.left)) {
          topRect = rect;
        }
      }
    }

    if (!topRect) {
      return null;
    }

    return {
      anchorX: topRect.left,
      anchorY: topRect.top,
      anchorBottomY: topRect.bottom,
    };
  };

  const isDiagnosticSelectionRange = (from: number, to: number): boolean =>
    currentDiagnostics.some((diagnostic) => diagnostic.from === from && diagnostic.to === to);

  const emitSelectionChange = () => {
    if (!view || typeof onSelectionChange !== 'function') {
      return;
    }

    const activeTableInput = getActiveTableInput();
    if (activeTableInput) {
      onSelectionChange(getActiveTableSelectionState(activeTableInput) ?? { visible: false });
      return;
    }

    const selection = view.state.selection.main;
    if (selection.empty) {
      onSelectionChange({ visible: false });
      return;
    }

    const from = Math.min(selection.from, selection.to);
    const to = Math.max(selection.from, selection.to);
    if (isSearchMatchSelection(from, to)) {
      onSelectionChange({ visible: false });
      return;
    }

    if (!isRegularInlineSelection(view.state, from, to)) {
      onSelectionChange({ visible: false });
      return;
    }

    const align = isDiagnosticSelectionRange(from, to) ? 'start' : undefined;
    const nativeAnchor = resolveNativeSelectionAnchor();
    if (nativeAnchor) {
      onSelectionChange({
        visible: true,
        from,
        to,
        align,
        anchorX: nativeAnchor.anchorX,
        anchorY: nativeAnchor.anchorY,
        anchorBottomY: nativeAnchor.anchorBottomY,
      });
      return;
    }

    const fromCoords = view.coordsAtPos(from);
    const toCoords = view.coordsAtPos(to);
    if (!fromCoords || !toCoords) {
      onSelectionChange({ visible: false });
      return;
    }

    const fromCharCoords = view.coordsForChar(from);
    const anchorX = fromCharCoords?.left ?? fromCoords.left;
    const anchorY = fromCharCoords ? Math.min(fromCoords.top, fromCharCoords.top) : fromCoords.top;
    const anchorBottomY = fromCharCoords ? Math.max(fromCoords.bottom, fromCharCoords.bottom) : fromCoords.bottom;

    onSelectionChange({
      visible: true,
      from,
      to,
      align,
      anchorX,
      anchorY,
      anchorBottomY,
    });
  };

  const diagnosticKey = (diagnostic: EditorDiagnostic): string =>
    [diagnostic.from, diagnostic.to, diagnostic.message, diagnostic.source ?? '', diagnostic.code ?? ''].join('\u001f');

  const clearDiagnosticSuggestionState = (): void => {
    lastDiagnosticClick = null;
    pendingDiagnosticSuggestionRequest = null;
  };

  const diagnosticAtPosition = (pos: number): EditorDiagnostic | null => {
    if (!Array.isArray(currentDiagnostics) || currentDiagnostics.length === 0) {
      return null;
    }

    let best: EditorDiagnostic | null = null;
    for (const diagnostic of currentDiagnostics) {
      if (pos < diagnostic.from || pos > diagnostic.to) {
        continue;
      }
      if (!best || diagnostic.to - diagnostic.from < best.to - best.from) {
        best = diagnostic;
      }
    }
    return best;
  };

  function diagnosticForRange(from: number, to: number): EditorDiagnostic | null {
    return currentDiagnostics.find((diagnostic) => diagnostic.from === from && diagnostic.to === to) ?? null;
  }

  const selectedDiagnosticRange = (): EditorDiagnostic | null => {
    if (!view || currentDiagnostics.length === 0) {
      return null;
    }

    const selection = view.state.selection.main;
    if (selection.empty) {
      return null;
    }

    const from = Math.min(selection.from, selection.to);
    const to = Math.max(selection.from, selection.to);
    return diagnosticForRange(from, to);
  };

  const diagnosticFromPointer = (event: MouseEvent | PointerEvent, editorView: EditorView): EditorDiagnostic | null => {
    const pos = editorView.posAtCoords({ x: event.clientX, y: event.clientY });
    if (pos === null) {
      return null;
    }

    const selectedDiagnostic = selectedDiagnosticRange();
    if (selectedDiagnostic && pos >= selectedDiagnostic.from && pos <= selectedDiagnostic.to) {
      return selectedDiagnostic;
    }

    return diagnosticAtPosition(pos);
  };

  const diagnosticMenuAnchor = (
    diagnostic: EditorDiagnostic
  ): { anchorX: number; anchorY: number; anchorBottomY: number } | null => {
    const fromCoords = view.coordsAtPos(diagnostic.from);
    if (!fromCoords) {
      return null;
    }
    const charCoords = view.coordsForChar(diagnostic.from);
    return {
      anchorX: charCoords?.left ?? fromCoords.left,
      anchorY: charCoords ? Math.min(fromCoords.top, charCoords.top) : fromCoords.top,
      anchorBottomY: charCoords ? Math.max(fromCoords.bottom, charCoords.bottom) : fromCoords.bottom,
    };
  };

  const requestDiagnosticSuggestionsFor = (
    diagnostic: EditorDiagnostic,
    anchor: { anchorX: number; anchorY: number; anchorBottomY?: number } | null = diagnosticMenuAnchor(diagnostic)
  ): boolean => {
    if (!anchor || typeof onRequestDiagnosticSuggestions !== 'function') {
      pendingDiagnosticSuggestionRequest = null;
      return false;
    }

    const key = diagnosticKey(diagnostic);
    if (
      pendingDiagnosticSuggestionRequest?.key === key &&
      pendingDiagnosticSuggestionRequest.from === diagnostic.from &&
      pendingDiagnosticSuggestionRequest.to === diagnostic.to
    ) {
      return true;
    }

    const requestId = onRequestDiagnosticSuggestions(diagnostic);
    if (typeof requestId !== 'string' || !requestId) {
      pendingDiagnosticSuggestionRequest = null;
      return false;
    }

    pendingDiagnosticSuggestionRequest = {
      requestId,
      key,
      from: diagnostic.from,
      to: diagnostic.to,
      anchorX: anchor.anchorX,
      anchorY: anchor.anchorY,
      anchorBottomY: anchor.anchorBottomY ?? anchor.anchorY,
    };
    return true;
  };

  const trackDiagnosticClick = (event: PointerEvent, editorView: EditorView): void => {
    const targetElement = targetElementFrom(event.target);
    const diagnostic = diagnosticFromPointer(event, editorView);
    if (!diagnostic || (!targetElement?.closest('.meo-diagnostic') && diagnostic !== selectedDiagnosticRange())) {
      clearDiagnosticSuggestionState();
      return;
    }

    const key = diagnosticKey(diagnostic);
    const isSecondClick =
      lastDiagnosticClick?.key === key &&
      lastDiagnosticClick.from === diagnostic.from &&
      lastDiagnosticClick.to === diagnostic.to;
    const isNativeSecondClick = event.detail >= 2 && diagnostic === selectedDiagnosticRange();
    lastDiagnosticClick = { key, from: diagnostic.from, to: diagnostic.to };

    if ((!isSecondClick && !isNativeSecondClick) || typeof onRequestDiagnosticSuggestions !== 'function') {
      pendingDiagnosticSuggestionRequest = null;
      return;
    }

    requestDiagnosticSuggestionsFor(diagnostic);
  };

  const requestDiagnosticSuggestionsFromPointer = (
    event: MouseEvent | PointerEvent,
    editorView: EditorView
  ): boolean => {
    const targetElement = targetElementFrom(event.target);
    const diagnostic = diagnosticFromPointer(event, editorView);
    if (!diagnostic || (!targetElement?.closest('.meo-diagnostic') && diagnostic !== selectedDiagnosticRange())) {
      return false;
    }

    const anchor = {
      anchorX: event.clientX,
      anchorY: event.clientY,
      anchorBottomY: event.clientY,
    };
    return requestDiagnosticSuggestionsFor(diagnostic, anchor);
  };

  const isHistoryReplayUpdate = (update: ViewUpdate): boolean => {
    return update.transactions.some((transaction) => {
      const userEvent = transaction.annotation(Transaction.userEvent);
      return (
        typeof userEvent === 'string' &&
        (userEvent === 'undo' || userEvent === 'redo' || userEvent.startsWith('undo.') || userEvent.startsWith('redo.'))
      );
    });
  };

  const inlineCodeCaretPosition = (state, position) => {
    let node = syntaxTree(state).resolveInner(position, -1);
    while (node && node.name !== 'InlineCode' && node.name !== 'CodeText') {
      node = node.parent;
    }
    if (!node) {
      return null;
    }

    const text = state.doc.sliceString(node.from, node.to);
    const openTicks = (/^`+/.exec(text) ?? [''])[0].length;
    const closeTicks = (/`+$/.exec(text) ?? [''])[0].length;
    if (!openTicks || !closeTicks) {
      return null;
    }

    const min = node.from + openTicks;
    const max = node.to - closeTicks;
    if (min > max) {
      return null;
    }
    if (position < min) {
      return min;
    }
    if (position > max) {
      return max;
    }
    return null;
  };

  const selectSearchMatch = (from, to, { focusEditor = true } = {}) => {
    view.dispatch({
      selection: { anchor: from, head: to },
      effects: EditorView.scrollIntoView(from, { y: 'center' }),
    });
    if (focusEditor) {
      view.focus();
    }
  };

  const applyRevealSelection = (anchor, head = anchor, { focusEditor = true, align = 'center' } = {}) => {
    const max = view.state.doc.length;
    const nextAnchor = Math.max(0, Math.min(anchor, max));
    const nextHead = Math.max(0, Math.min(head, max));
    const y = align === 'top' ? 'start' : 'center';
    const selection = view.state.selection.main;

    if (selection.anchor === nextAnchor && selection.head === nextHead) {
      view.dispatch({
        effects: EditorView.scrollIntoView(nextAnchor, { y }),
      });
      if (focusEditor) {
        view.focus();
      }
      return;
    }

    view.dispatch({
      selection: { anchor: nextAnchor, head: nextHead },
      effects: EditorView.scrollIntoView(nextAnchor, { y }),
    });
    if (focusEditor) {
      view.focus();
    }
  };

  const TOP_LINE_VISIBILITY_EPSILON = 0.5;
  const SCROLL_RESTORE_EPSILON = 0.5;
  const SCROLL_RESTORE_MAX_ATTEMPTS = 3;

  const getTopLineMetrics = (scrollTopValue = view.scrollDOM.scrollTop) => {
    const scrollTop = Math.max(0, scrollTopValue);
    const lineBlock = view.lineBlockAtHeight(scrollTop);
    const line = view.state.doc.lineAt(lineBlock.from);
    return {
      line,
      lineBlock,
      hiddenTopPixels: scrollTop - lineBlock.top,
    };
  };

  const topVisibleLineAtCurrentScroll = () => {
    const { line, hiddenTopPixels } = getTopLineMetrics();
    if (hiddenTopPixels <= TOP_LINE_VISIBILITY_EPSILON) {
      return line;
    }

    const nextLineNumber = Math.min(view.state.doc.lines, line.number + 1);
    return view.state.doc.line(nextLineNumber);
  };

  const syncCursorToTopVisibleLine = () => {
    const line = topVisibleLineAtCurrentScroll();
    const anchor = line.from;
    const selection = view.state.selection.main;
    if (selection.anchor === anchor && selection.head === anchor) {
      return;
    }
    view.dispatch({
      selection: { anchor },
      annotations: Transaction.addToHistory.of(false),
    });
  };

  const computeTopVisiblePosition = () => {
    const { line, hiddenTopPixels } = getTopLineMetrics();
    return {
      lineNumber: line.number,
      lineOffset: normalizeTopLineOffset(hiddenTopPixels),
    };
  };

  const restoreTopVisibleLine = (lineNumber, lineOffset = 0, { syncCursor = true } = {}) => {
    const targetLineNumber = Math.min(Math.max(1, Math.floor(lineNumber || 1)), view.state.doc.lines);
    const targetOffset = normalizeTopLineOffset(lineOffset);
    let attempts = 0;
    const restoreScroll = () => {
      if (!view || ++attempts > SCROLL_RESTORE_MAX_ATTEMPTS) {
        if (syncCursor && view) {
          syncCursorToTopVisibleLine();
        }
        return;
      }
      const targetLine = view.state.doc.line(targetLineNumber);
      const targetTop = Math.max(0, view.lineBlockAt(targetLine.from).top + targetOffset);
      const currentTop = view.scrollDOM.scrollTop;
      view.scrollDOM.scrollTop = targetTop;
      if (Math.abs(currentTop - targetTop) <= SCROLL_RESTORE_EPSILON) {
        if (syncCursor) {
          syncCursorToTopVisibleLine();
        }
        return;
      }
      requestAnimationFrame(restoreScroll);
    };
    restoreScroll();
  };

  const findMatch = (
    query,
    backward = false,
    { focusEditor = true, ...searchOptions }: SearchOptions & { focusEditor?: boolean } = {}
  ) => {
    if (!query) {
      return { found: false, current: 0, total: 0 };
    }

    const text = view.state.doc.toString();
    const selection = view.state.selection.main;
    const from = Math.min(selection.from, selection.to);
    const to = Math.max(selection.from, selection.to);
    const matches = findSearchMatchRanges(text, query, searchOptions);
    const total = matches.length;

    if (!total) {
      return { found: false, current: 0, total };
    }

    let matchIndex = -1;
    if (backward) {
      for (let index = matches.length - 1; index >= 0; index -= 1) {
        if (matches[index].start < from) {
          matchIndex = index;
          break;
        }
      }
      if (matchIndex < 0) {
        matchIndex = matches.length - 1;
      }
    } else {
      for (let index = 0; index < matches.length; index += 1) {
        if (matches[index].start >= to) {
          matchIndex = index;
          break;
        }
      }
      if (matchIndex < 0) {
        matchIndex = 0;
      }
    }

    const match = matches[matchIndex];
    selectSearchMatch(match.start, match.end, { focusEditor });
    return {
      found: true,
      current: matchIndex + 1,
      total,
    };
  };

  const replaceCurrentMatch = (query, replacement, options: SearchOptions = {}) => {
    if (!query) {
      return { replaced: false, found: false, current: 0, total: 0 };
    }

    const text = view.state.doc.toString();
    const selection = view.state.selection.main;
    const from = Math.min(selection.from, selection.to);
    const to = Math.max(selection.from, selection.to);
    const matches = findSearchMatchRanges(text, query, options);
    const matchIndex = findSelectedSearchMatchIndex(matches, from, to);
    if (matchIndex < 0) {
      return { replaced: false, ...findMatch(query, false, options) };
    }

    view.dispatch({
      changes: { from, to, insert: replacement },
      selection: { anchor: from, head: from + replacement.length },
    });
    const nextMatch = findMatch(query, false, options);
    if (nextMatch.found) {
      return { replaced: true, ...nextMatch };
    }

    const remaining = countMatches(view.state.doc.toString(), query, options);
    return { replaced: true, found: false, current: 0, total: remaining };
  };

  const state = EditorState.create({
    doc: text,
    selection: { anchor: initialCursorPos },
    extensions: [
      EditorState.tabSize.of(4),
      indentUnit.of('  '),
      vimCompartment.of(vimExtensionsForState()),
      keymap.of([
        { key: 'Tab', run: (view) => indentListByTwoSpaces(view) || indentMore(view) },
        { key: 'Shift-Tab', run: (view) => outdentListByTwoSpaces(view) || indentLess(view) },
        { key: 'Backspace', run: deleteBackwardSmart },
        { key: 'ArrowLeft', run: (view) => isLiveMode(view) && handleArrowLeftAtListContentStart(view) },
        { key: 'ArrowRight', run: (view) => isLiveMode(view) && handleArrowRightAtListLineStart(view) },
        {
          key: 'Enter',
          run: (view) =>
            handleEnterContinueQuotedCodeBlock(view) ||
            handleEnterOnEmptyListItem(view) ||
            handleEnterAtListContentStart(view) ||
            handleEnterContinueList(view) ||
            handleEnterBeforeNestedList(view),
        },
        { key: 'Shift-Enter', run: insertTableCellLineBreak },
        { key: 'ArrowUp', run: (view) => tryEnterAdjacentTable(view, 'up') },
        { key: 'ArrowDown', run: (view) => tryEnterAdjacentTable(view, 'down') },
        ...markdownKeymap,
        ...defaultKeymap,
        ...historyKeymap,
      ]),
      history(),
      lineNumbers(),
      ...gitDiffGutterBaselineExtensions(),
      gitGutterCompartment.of(
        startMode === 'live' ? gitDiffGutterLiveRenderExtensions() : gitDiffGutterRenderExtensions()
      ),
      highlightActiveLineGutter(),
      highlightActiveLine(),
      shikiCodeHighlight,
      EditorView.lineWrapping,
      scrollPastEnd(),
      EditorView.domEventHandlers({
        pointerdown(event, view) {
          if (event.button !== 0) {
            frontmatterBoundaryClick = null;
            return false;
          }
          if (openLinkIfModifierClick(event, view)) {
            frontmatterBoundaryClick = null;
            return true;
          }

          const target = event.target;
          const targetElement = targetElementFrom(target);
          if (!(target instanceof Node) || !view.contentDOM.contains(target)) {
            clearDiagnosticSuggestionState();
            return false;
          }

          trackDiagnosticClick(event, view);
          trackFrontmatterBoundaryClick(event, view);

          if (targetElement && targetElement.closest('.meo-mermaid-zoom-controls')) {
            event.preventDefault();
            event.stopPropagation();
            return true;
          }

          if (targetElement && targetElement.closest('.meo-task-checkbox')) {
            checkboxClick = { pointerId: event.pointerId };
            return false;
          }

          // Let interactive HTML table widget controls handle focus/click natively.
          if (targetElement && targetElement.closest('.meo-md-html-table-shell')) {
            inlineCodeClick = null;
            checkboxClick = null;
            return false;
          }

          inlineCodeClick = {
            pointerId: event.pointerId,
            inInlineCode:
              currentMode === 'live' && targetElement && targetElement.closest('.meo-md-inline-code') !== null,
          };

          if (view.dom.setPointerCapture) {
            view.dom.setPointerCapture(event.pointerId);
            capturedPointerId = event.pointerId;
          }
          return false;
        },
        contextmenu(event, view) {
          if (!requestDiagnosticSuggestionsFromPointer(event, view)) {
            return false;
          }
          return false;
        },
        pointerup(event, view) {
          if (checkboxClick?.pointerId === event.pointerId) {
            frontmatterBoundaryClick = null;
            checkboxClick = null;
            return false;
          }

          if (capturedPointerId !== event.pointerId) {
            if (frontmatterBoundaryClick?.pointerId === event.pointerId) {
              frontmatterBoundaryClick = null;
            }
            return false;
          }

          releasePointerCaptureIfHeld(event.pointerId);
          capturedPointerId = null;

          if (inlineCodeClick?.pointerId === event.pointerId && inlineCodeClick.inInlineCode && isLiveMode(view)) {
            const { head, empty } = view.state.selection.main;
            if (empty) {
              const clamped = inlineCodeCaretPosition(view.state, head);
              if (clamped !== null && clamped !== head) {
                view.dispatch({ selection: { anchor: clamped } });
              }
            }
          }

          if (frontmatterBoundaryClick?.pointerId === event.pointerId) {
            if (isLiveMode(view)) {
              const selection = view.state.selection.main;
              if (selection.empty && selection.head !== frontmatterBoundaryClick.cursorEnd) {
                view.dispatch({ selection: { anchor: frontmatterBoundaryClick.cursorEnd } });
              }
            }
            frontmatterBoundaryClick = null;
          }

          if (isLiveMode(view)) {
            const { head, empty } = view.state.selection.main;
            if (empty) {
              const emptyQuoteCursorEnd = emptyBlockquoteLineCursorEnd(view.state, head);
              if (emptyQuoteCursorEnd !== null && head < emptyQuoteCursorEnd) {
                view.dispatch({ selection: { anchor: emptyQuoteCursorEnd } });
                return false;
              }

              const node = resolvedSyntaxTree(view.state).resolveInner(head, -1);
              if (node.name === 'HorizontalRule') {
                const line = view.state.doc.lineAt(head);
                const lineText = view.state.doc.sliceString(line.from, line.to);
                const hrMatch = /^[ \t]*(-{3,}|\*{3,}|_{3,})/.exec(lineText);
                if (hrMatch) {
                  const cursorEnd = line.from + hrMatch[0].length;
                  if (head !== cursorEnd) {
                    view.dispatch({ selection: { anchor: cursorEnd } });
                  }
                }
              }
            }
          }

          inlineCodeClick = null;
          return false;
        },
        pointercancel(event, _view) {
          if (capturedPointerId !== event.pointerId) {
            if (frontmatterBoundaryClick?.pointerId === event.pointerId) {
              frontmatterBoundaryClick = null;
            }
            return false;
          }

          releasePointerCaptureIfHeld(event.pointerId);
          capturedPointerId = null;
          frontmatterBoundaryClick = null;
          inlineCodeClick = null;
          checkboxClick = null;
          return false;
        },
        pointermove(event, view) {
          updateSourceLinkHoverCursor(event, view);
          return false;
        },
        pointerleave(_event, view) {
          setSourceLinkHoverCursor(view, false);
          return false;
        },
      }),
      ...headingCollapseSharedExtensions(),
      modeCompartment.of(startMode === 'live' ? liveModeExtensions() : sourceMode()),
      searchQueryField,
      searchMatchField,
      diagnosticDataField,
      diagnosticField,
      /*
       * CDXC:Docs 2026-06-27-12:40:
       * Manage embeds Meo's live Markdown editor directly, but annotations still belong to Ghostex's project-scoped review sidecar.
       * Let Manage add CodeMirror decorations through a narrow extension hook instead of rewriting Meo's live Markdown renderer.
       */
      ...(Array.isArray(externalExtensions) ? externalExtensions : []),
      EditorView.updateListener.of((update) => {
        syncModeClasses();
        syncLineNumbersVisibility();
        syncGitGutterVisibility();
        emitSearchStateChange();

        if (update.selectionSet) {
          syncSelectionClass();
          emitSelectionChange();
        } else if (update.viewportChanged) {
          emitSelectionChange();
          onViewportChange?.();
        }

        if (update.docChanged) {
          clearDiagnosticSuggestionState();
        }

        if (!update.docChanged || applyingExternal || applyingRenumber) {
          return;
        }

        gitBlameHover?.hide();

        pendingExternalUndoSelectionPreserve = false;

        if (isHistoryReplayUpdate(update)) {
          onApplyChanges(update.state.doc.toString());
          return;
        }

        const renumberChanges = collectOrderedListRenumberChanges(update.state);
        if (renumberChanges.length) {
          applyingRenumber = true;
          view.dispatch({
            changes: renumberChanges,
            annotations: Transaction.addToHistory.of(false),
          });
          applyingRenumber = false;
          onApplyChanges(view.state.doc.toString());
          return;
        }

        onApplyChanges(update.state.doc.toString());
      }),
    ],
  });

  const initialScrollTo = (() => {
    if (typeof initialTopLine !== 'number' || !Number.isFinite(initialTopLine)) {
      return undefined;
    }
    const lineNumber = Math.min(Math.max(1, Math.floor(initialTopLine)), state.doc.lines);
    const line = state.doc.line(lineNumber);
    return EditorView.scrollIntoView(line.from, { y: 'start' });
  })();

  view = new EditorView({
    state,
    parent,
    scrollTo: initialScrollTo,
  });
  if (typeof initialTopLine === 'number' && Number.isFinite(initialTopLine)) {
    restoreTopVisibleLine(initialTopLine, initialTopLineOffset, { syncCursor: true });
  }
  onTableInteraction = (event) => {
    const active = Boolean(event?.detail?.active);
    setTableInteractionActive(active);
  };
  view.dom.addEventListener('meo-table-interaction', onTableInteraction);
  onTableOpenLink = (event) => {
    const href = event?.detail?.href;
    if (typeof href !== 'string' || !href) {
      return;
    }
    onOpenLink?.(href);
  };
  view.dom.addEventListener('meo-open-link', onTableOpenLink);
  onTableSelectionChange = () => {
    emitSelectionChange();
  };
  view.dom.addEventListener('meo-table-selection-change', onTableSelectionChange);
  onScroll = () => {
    emitSelectionChange();
    gitBlameHover?.hide();
    onViewportChange?.();
  };
  view.scrollDOM.addEventListener('scroll', onScroll, { passive: true });
  if (typeof onRequestGitBlame === 'function') {
    gitBlameHover = createGitBlameHoverController({
      view,
      getMode: () => currentMode,
      requestBlame: onRequestGitBlame,
      openRevisionForLine: onOpenGitRevisionForLine,
      openWorktreeForLine: onOpenGitWorktreeForLine,
    });
  }
  gitDiffOverviewRuler = createGitDiffOverviewRulerController({
    view,
    getMode: () => currentMode,
    isGitChangesVisible: () => gitGutterVisible,
  });
  syncModeClasses();
  syncLineNumbersVisibility();
  syncGitGutterVisibility();
  syncSelectionClass();
  view.dispatch({ effects: setDiagnosticsEffect.of(currentDiagnostics) });
  emitSelectionChange();

  return {
    view,
    state: view.state,
    getText() {
      commitActiveTableInput();
      return view.state.doc.toString();
    },
    commitTransientEdits() {
      return commitActiveTableInput();
    },
    selectAll() {
      view.dispatch({
        selection: {
          anchor: 0,
          head: view.state.doc.length,
        },
      });
      return true;
    },
    undo() {
      const shouldPreserveSelection = pendingExternalUndoSelectionPreserve;
      const { anchor, head } = view.state.selection.main;
      const applied = undo(view);
      if (!applied) {
        return false;
      }
      if (!shouldPreserveSelection) {
        return true;
      }

      pendingExternalUndoSelectionPreserve = false;
      const nextAnchor = Math.min(anchor, view.state.doc.length);
      const nextHead = Math.min(head, view.state.doc.length);
      const selection = view.state.selection.main;
      if (selection.anchor === nextAnchor && selection.head === nextHead) {
        return true;
      }

      view.dispatch({
        selection: { anchor: nextAnchor, head: nextHead },
        annotations: Transaction.addToHistory.of(false),
      });
      return true;
    },
    redo() {
      return redo(view);
    },
    findNext(query, options: SearchOptions & { focusEditor?: boolean } = {}) {
      return findMatch(query, false, options);
    },
    findPrevious(query, options: SearchOptions & { focusEditor?: boolean } = {}) {
      return findMatch(query, true, options);
    },
    replaceCurrent(query, replacement, options: SearchOptions = {}) {
      return replaceCurrentMatch(query, replacement, options);
    },
    replaceAll(query, replacement, options: SearchOptions = {}) {
      if (!query) {
        return { replaced: 0, total: 0 };
      }

      const text = view.state.doc.toString();
      const matches = findSearchMatchRanges(text, query, options);
      const replaced = matches.length;
      if (!replaced) {
        return { replaced: 0, total: 0 };
      }

      const nextText = replaceMatchRanges(text, matches, replacement);
      view.dispatch({
        changes: { from: 0, to: text.length, insert: nextText },
        selection: { anchor: 0 },
      });
      return { replaced, total: countMatches(nextText, query, options) };
    },
    countMatches(query, options: SearchOptions = {}) {
      if (!query) {
        return 0;
      }
      return countMatches(view.state.doc.toString(), query, options);
    },
    setSearchQuery(query, options: SearchOptions = {}) {
      const nextQuery = createSearchQueryState(query, options);
      const currentQuery = view.state.field(searchQueryField);
      if (
        currentQuery.text === nextQuery.text &&
        currentQuery.wholeWord === nextQuery.wholeWord &&
        currentQuery.caseSensitive === nextQuery.caseSensitive
      ) {
        return;
      }
      view.dispatch({
        effects: setSearchQueryEffect.of(nextQuery),
      });
    },
    hasFocus() {
      return view.hasFocus;
    },
    focus() {
      const activeTableInput = getActiveTableInput();
      if (activeTableInput) {
        activeTableInput.focus({ preventScroll: true });
        return;
      }
      view.focus();
    },
    destroy() {
      gitBlameHover?.destroy();
      gitBlameHover = null;
      gitDiffOverviewRuler?.destroy();
      gitDiffOverviewRuler = null;
      if (onScroll) {
        view.scrollDOM.removeEventListener('scroll', onScroll);
        onScroll = null;
      }
      if (onTableInteraction) {
        view.dom.removeEventListener('meo-table-interaction', onTableInteraction);
        onTableInteraction = null;
      }
      if (onTableOpenLink) {
        view.dom.removeEventListener('meo-open-link', onTableOpenLink);
        onTableOpenLink = null;
      }
      if (onTableSelectionChange) {
        view.dom.removeEventListener('meo-table-selection-change', onTableSelectionChange);
        onTableSelectionChange = null;
      }
      if (capturedPointerId !== null) {
        releasePointerCaptureIfHeld(capturedPointerId);
        capturedPointerId = null;
      }
      setSourceLinkHoverCursor(view, false);
      view.destroy();
    },
    setText(textValue) {
      gitBlameHover?.hide();
      clearDiagnosticSuggestionState();
      const currentText = view.state.doc.toString();
      const syncChange = findSyncChange(currentText, textValue);
      if (!syncChange) {
        return;
      }

      const { anchor, head } = view.state.selection.main;
      const newLength = textValue.length;
      const mappedAnchor = Math.min(mapPositionThroughChange(anchor, syncChange), newLength);
      const mappedHead = Math.min(mapPositionThroughChange(head, syncChange), newLength);
      applyingExternal = true;
      view.dispatch({
        changes: syncChange,
        selection: { anchor: mappedAnchor, head: mappedHead },
      });
      applyingExternal = false;
      pendingExternalUndoSelectionPreserve = true;
      syncSelectionClass();
      emitSelectionChange();
    },
    setMode(mode) {
      gitBlameHover?.hide();
      clearDiagnosticSuggestionState();
      commitActiveTableInput();
      const nextMode = mode === 'live' ? 'live' : 'source';
      if (nextMode === currentMode) {
        return;
      }

      const topPosition = computeTopVisiblePosition();

      const previousMode = currentMode;
      currentMode = nextMode;
      try {
        view.dispatch({
          effects: [
            modeCompartment.reconfigure(nextMode === 'live' ? liveModeExtensions() : sourceMode()),
            gitGutterCompartment.reconfigure(
              nextMode === 'live' ? gitDiffGutterLiveRenderExtensions() : gitDiffGutterRenderExtensions()
            ),
            vimCompartment.reconfigure(vimExtensionsForState()),
          ],
        });
        forceParsing(view, view.state.doc.length, 500);
      } catch (error) {
        currentMode = previousMode;
        syncModeClasses();
        throw error;
      }
      syncModeClasses();
      syncGitGutterVisibility();

      restoreTopVisibleLine(topPosition.lineNumber, topPosition.lineOffset, { syncCursor: false });
    },
    setLineNumbers(visible) {
      const nextVisible = visible !== false;
      if (nextVisible === lineNumbersVisible) {
        return;
      }
      lineNumbersVisible = nextVisible;
      syncLineNumbersVisibility();
    },
    setGitGutterVisible(visible) {
      const nextVisible = visible !== false;
      if (nextVisible === gitGutterVisible) {
        return;
      }
      gitGutterVisible = nextVisible;
      syncGitGutterVisibility();
    },
    setVimMode(enabled) {
      const nextEnabled = enabled === true;
      if (nextEnabled === vimModeEnabled) {
        return;
      }
      vimModeEnabled = nextEnabled;
      if (vimModeEnabled) {
        applyVimKeybindings(vimKeybindings, vimLeader);
      } else {
        clearVimKeybindings();
      }
      view.dispatch({
        effects: vimCompartment.reconfigure(vimExtensionsForState()),
      });
    },
    setVimKeybindings(
      bindings: Array<{ before: string; after: string; mode: string; recursive: boolean }>,
      leaderKey: string
    ) {
      vimKeybindings = bindings;
      vimLeader = leaderKey;
      if (vimModeEnabled) {
        applyVimKeybindings(vimKeybindings, vimLeader);
      }
    },
    insertFormat(action, level) {
      const activeTableInput = getActiveTableInput();
      if (activeTableInput) {
        return insertFormatInActiveTableInput(activeTableInput, action);
      }

      const { state } = view;
      const selection = state.selection.main;
      let cachedInlineSelection: InlineSelectionRange | null = null;
      const inlineSelection = (): InlineSelectionRange => {
        if (cachedInlineSelection) {
          return cachedInlineSelection;
        }
        cachedInlineSelection =
          currentMode === 'live' ? normalizeLiveInlineSelectionForListContent(state, selection) : selection;
        return cachedInlineSelection;
      };

      let insert = '';
      switch (action) {
        case 'heading':
          insert = `${'#'.repeat(level ?? 1)} `;
          break;
        case 'bulletList':
          insert = '- ';
          break;
        case 'numberedList':
          insert = '1. ';
          break;
        case 'task':
          insert = '- [ ] ';
          break;
        case 'codeBlock':
          return insertCodeBlock(view, selection);
        case 'inlineCode':
          return insertInlineCode(view, inlineSelection());
        case 'kbd':
          return insertKbd(view, inlineSelection());
        case 'bold':
          return insertInlineFence(view, inlineSelection(), '**');
        case 'italic':
          return insertInlineFence(view, inlineSelection(), '*');
        case 'lineover':
        case 'strike':
          return insertInlineFence(view, inlineSelection(), '~~');
        case 'quote':
          return insertQuote(view, selection);
        case 'hr':
          return insertHr(view, selection);
        case 'table':
          return insertTable(view, selection, level?.cols, level?.rows);
        case 'link':
          return insertLink(view, inlineSelection());
        case 'wikiLink':
          return insertWikiLink(view, inlineSelection());
        case 'image':
          return insertImage(view, inlineSelection());
      }

      if (!selection.empty && (action === 'bulletList' || action === 'numberedList')) {
        const changes = buildListFormatChangesForSelection(state, insert);
        dispatchSelectedListFormatChanges(state, changes, action === 'numberedList');
        return;
      }

      const line = state.doc.lineAt(selection.from);
      const { contentStart, oldMarkerLen, isExistingTask } = lineMarkerReplacementContext(state, line);
      if (action === 'task' && isExistingTask) {
        return;
      }

      const newMarkerLen = insert.length;
      const cursorOffset = selection.from - (contentStart + oldMarkerLen);
      const newCursorPos = contentStart + newMarkerLen + Math.max(0, cursorOffset);

      view.dispatch({
        changes: { from: contentStart, to: contentStart + oldMarkerLen, insert },
        selection: { anchor: newCursorPos },
      });
    },
    getHeadings() {
      return extractHeadings(view.state);
    },
    moveHeadingSection(sourceHeadingFrom, targetHeadingFrom, placement) {
      if (placement !== 'before' && placement !== 'after') {
        return false;
      }

      const sections = extractHeadingSections(view.state);
      const source = sections.find((heading) => heading.from === sourceHeadingFrom);
      const target = sections.find((heading) => heading.from === targetHeadingFrom);
      if (!source || !target) {
        return false;
      }

      const insertionPoint = placement === 'before' ? target.sectionFrom : target.sectionTo;
      if (insertionPoint > source.sectionFrom && insertionPoint < source.sectionTo) {
        return false;
      }

      const currentText = view.state.doc.toString();
      const movedText = currentText.slice(source.sectionFrom, source.sectionTo);
      if (!movedText) {
        return false;
      }

      const textWithoutSource = currentText.slice(0, source.sectionFrom) + currentText.slice(source.sectionTo);
      const sourceLength = source.sectionTo - source.sectionFrom;
      const adjustedInsertionPoint =
        insertionPoint >= source.sectionTo ? insertionPoint - sourceLength : insertionPoint;
      const nextText =
        textWithoutSource.slice(0, adjustedInsertionPoint) +
        movedText +
        textWithoutSource.slice(adjustedInsertionPoint);

      if (nextText === currentText) {
        return false;
      }

      const nextAnchor = Math.min(adjustedInsertionPoint, nextText.length);
      view.dispatch({
        changes: { from: 0, to: currentText.length, insert: nextText },
        selection: { anchor: nextAnchor },
        effects: EditorView.scrollIntoView(nextAnchor, { y: 'start' }),
      });
      return true;
    },
    scrollToLine(lineNumber, align = 'center') {
      const line = view.state.doc.line(Math.min(lineNumber, view.state.doc.lines));
      applyRevealSelection(line.from, line.from, { focusEditor: true, align });
    },
    restoreTopLine(lineNumber, lineOffset) {
      restoreTopVisibleLine(lineNumber, lineOffset, { syncCursor: true });
    },
    getTopVisiblePosition() {
      const position = computeTopVisiblePosition();
      return {
        line: position.lineNumber,
        lineOffset: position.lineOffset,
      };
    },
    getTopVisibleLine() {
      return computeTopVisiblePosition().lineNumber;
    },
    revealSelection(anchor, head, options) {
      applyRevealSelection(anchor, head, options);
    },
    refreshSelectionOverlay() {
      emitSelectionChange();
    },
    refreshDecorations() {
      view.dispatch({ effects: refreshDecorationsEffect.of(null) });
    },
    setDiagnostics(diagnostics: EditorDiagnostic[]) {
      currentDiagnostics = Array.isArray(diagnostics) ? diagnostics : [];
      clearDiagnosticSuggestionState();
      view.dispatch({ effects: setDiagnosticsEffect.of(currentDiagnostics) });
    },
    showDiagnosticSuggestions(requestId, payload) {
      if (
        !pendingDiagnosticSuggestionRequest ||
        pendingDiagnosticSuggestionRequest.requestId !== requestId ||
        pendingDiagnosticSuggestionRequest.from !== payload?.from ||
        pendingDiagnosticSuggestionRequest.to !== payload?.to ||
        !Array.isArray(payload?.suggestions) ||
        payload.suggestions.length === 0
      ) {
        return;
      }

      const diagnostic = currentDiagnostics.find(
        (item) =>
          item.from === payload.from &&
          item.to === payload.to &&
          diagnosticKey(item) === pendingDiagnosticSuggestionRequest?.key
      );
      if (!diagnostic) {
        pendingDiagnosticSuggestionRequest = null;
        return;
      }

      onSelectionChange?.({
        visible: true,
        from: payload.from,
        to: payload.to,
        align: 'start',
        anchorX: pendingDiagnosticSuggestionRequest.anchorX,
        anchorY: pendingDiagnosticSuggestionRequest.anchorY,
        anchorBottomY: pendingDiagnosticSuggestionRequest.anchorBottomY,
        diagnosticSuggestions: payload.suggestions.map((text) => ({
          from: payload.from,
          to: payload.to,
          text,
        })),
      });
    },
    applyDiagnosticSuggestion(from: number, to: number, insert: string) {
      const activeTableInput = getActiveTableInput();
      const tableSourceRange = activeTableInput ? getTableInputSourceRange(activeTableInput) : null;
      if (activeTableInput && tableSourceRange && from >= tableSourceRange.from && to <= tableSourceRange.to) {
        const localFrom = from - tableSourceRange.from;
        const localTo = to - tableSourceRange.from;
        clearDiagnosticSuggestionState();
        updateActiveTableInput(
          activeTableInput,
          activeTableInput.value.slice(0, localFrom) + insert + activeTableInput.value.slice(localTo),
          localFrom + insert.length
        );
        return;
      }

      const docLength = view.state.doc.length;
      const safeFrom = Math.max(0, Math.min(Math.floor(from), docLength));
      const safeTo = Math.max(safeFrom, Math.min(Math.floor(to), docLength));
      clearDiagnosticSuggestionState();
      view.dispatch({
        changes: { from: safeFrom, to: safeTo, insert },
        selection: { anchor: safeFrom + insert.length },
      });
      emitSelectionChange();
    },
    refreshLayout() {
      view.requestMeasure();
      if (currentMode === 'live') {
        forceParsing(view, view.state.doc.length, 500);
      }
      emitSelectionChange();
    },
    setGitBaseline(snapshot) {
      applyGitBaseline(view, snapshot);
      gitBlameHover?.hide();
      gitDiffOverviewRuler?.refresh();
    },
    clearGitUiTransientState() {
      gitBlameHover?.hide();
    },
  };
}

function insertTableCellLineBreak(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!isInsideTableCell(state, selection.from) || !isInsideTableCell(state, selection.to)) {
    return false;
  }

  const insert = '<br>';
  view.dispatch({
    changes: { from: selection.from, to: selection.to, insert },
    selection: { anchor: selection.from + insert.length },
  });
  return true;
}

function handleEnterContinueQuotedCodeBlock(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const quotePrefix = getQuotedCodeBlockLinePrefix(state, selection.from);
  if (!quotePrefix) {
    return false;
  }

  const insert = `\n${quotePrefix}`;
  const nextPos = selection.from + insert.length;
  view.dispatch({
    changes: { from: selection.from, to: selection.from, insert },
    selection: { anchor: nextPos },
  });
  return true;
}

function getQuotedCodeBlockLinePrefix(state, position) {
  const line = state.doc.lineAt(position);
  const lineText = state.doc.sliceString(line.from, line.to);
  const match = blockquoteLinePrefixRegex.exec(lineText);
  if (!match) {
    return null;
  }

  return isInsideQuotedCodeBlock(state, position) ? match[0] : null;
}

function isInsideQuotedCodeBlock(state, position) {
  let node = syntaxTree(state).resolveInner(position, -1);
  let insideCodeBlock = false;
  let insideBlockquote = false;

  while (node) {
    if (quotedCodeBlockAncestorNames.has(node.name)) {
      insideCodeBlock = true;
    } else if (node.name === 'Blockquote') {
      insideBlockquote = true;
    }
    if (insideCodeBlock && insideBlockquote) {
      return true;
    }
    node = node.parent;
  }

  return false;
}

function deleteTableCellLineBreakBackward(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const pos = selection.from;
  if (!isInsideTableCell(state, pos)) {
    return false;
  }

  const from = Math.max(0, pos - 8);
  const before = state.doc.sliceString(from, pos);
  const match = /<br\s*\/?>$/i.exec(before);
  if (!match) {
    return false;
  }

  const start = pos - match[0].length;
  view.dispatch({
    changes: { from: start, to: pos },
    selection: { anchor: start },
  });
  return true;
}

function deleteBackwardSmart(view) {
  return handleBackspaceAtListContentStart(view) || deleteTableCellLineBreakBackward(view);
}

function isInsideTableCell(state, position) {
  let node = syntaxTree(state).resolveInner(position, -1);
  while (node) {
    if (node.name === 'TableCell') {
      return true;
    }
    if (node.name === 'TableDelimiter' || node.name === 'Table') {
      return false;
    }
    node = node.parent;
  }
  return false;
}

function findSyncChange(previousText, nextText) {
  if (previousText === nextText) {
    return null;
  }

  let from = 0;
  const maxStart = Math.min(previousText.length, nextText.length);
  while (from < maxStart && previousText.charCodeAt(from) === nextText.charCodeAt(from)) {
    from += 1;
  }

  let previousTo = previousText.length;
  let nextTo = nextText.length;
  while (
    previousTo > from &&
    nextTo > from &&
    previousText.charCodeAt(previousTo - 1) === nextText.charCodeAt(nextTo - 1)
  ) {
    previousTo -= 1;
    nextTo -= 1;
  }

  return {
    from,
    to: previousTo,
    insert: nextText.slice(from, nextTo),
  };
}

// Map a position through a single replace change so external syncs keep the cursor nearby.
function mapPositionThroughChange(position, change) {
  const insertLength = change.insert.length;
  const deletedLength = change.to - change.from;
  const delta = insertLength - deletedLength;

  if (position <= change.from) {
    return position;
  }

  if (position >= change.to) {
    return position + delta;
  }

  return change.from + insertLength;
}

function createSearchQueryState(query: string | null | undefined, options: SearchOptions = {}): SearchQueryState {
  return {
    text: query ?? '',
    wholeWord: options.wholeWord === true,
    caseSensitive: options.caseSensitive === true,
  };
}

function isWordBoundaryCharacter(value: string): boolean {
  return /[0-9A-Za-z_]/.test(value);
}

function isWholeWordRange(text: string, start: number, end: number): boolean {
  const previous = start > 0 ? text.slice(start - 1, start) : '';
  const next = end < text.length ? text.slice(end, end + 1) : '';
  return !isWordBoundaryCharacter(previous) && !isWordBoundaryCharacter(next);
}

function findSearchMatchRanges(text: string, query: string, options: SearchOptions = {}): SearchMatchRange[] {
  if (!query) {
    return [];
  }

  const haystack = options.caseSensitive ? text : text.toLocaleLowerCase();
  const needle = options.caseSensitive ? query : query.toLocaleLowerCase();
  const matches: SearchMatchRange[] = [];
  let offset = 0;
  while (offset <= text.length) {
    const index = haystack.indexOf(needle, offset);
    if (index < 0) {
      break;
    }

    const end = index + query.length;
    if (!options.wholeWord || isWholeWordRange(text, index, end)) {
      matches.push({ start: index, end });
    }
    offset = end;
  }
  return matches;
}

function countMatches(text, query, options: SearchOptions = {}) {
  if (!query) {
    return 0;
  }

  return findSearchMatchRanges(text, query, options).length;
}

function findSelectedSearchMatchIndex(matches: SearchMatchRange[], from: number, to: number): number {
  for (let index = 0; index < matches.length; index += 1) {
    if (matches[index].start === from && matches[index].end === to) {
      return index;
    }
  }
  return -1;
}

function replaceMatchRanges(text: string, matches: SearchMatchRange[], replacement: string): string {
  if (!matches.length) {
    return text;
  }

  let nextText = '';
  let offset = 0;
  for (const match of matches) {
    nextText += text.slice(offset, match.start);
    nextText += replacement;
    offset = match.end;
  }
  nextText += text.slice(offset);
  return nextText;
}

function normalizeLiveInlineSelectionForListContent(
  state: EditorState,
  selection: InlineSelectionRange
): InlineSelectionRange {
  if (selection.empty) {
    return selection;
  }

  let from = Math.min(selection.from, selection.to);
  const to = Math.max(selection.from, selection.to);
  if (to <= from) {
    return selection;
  }

  const startLine = state.doc.lineAt(from);
  const endLine = state.doc.lineAt(to - 1);
  if (startLine.number !== endLine.number) {
    return selection;
  }

  const lineText = state.doc.sliceString(startLine.from, startLine.to);
  const marker = listMarkerData(lineText);
  if (!marker) {
    return selection;
  }

  const contentFrom = startLine.from + marker.toOffset;
  if (from >= contentFrom || to <= contentFrom) {
    return selection;
  }

  from = contentFrom;
  if (from >= to) {
    return selection;
  }

  if (selection.anchor <= selection.head) {
    return { from, to, anchor: from, head: to, empty: false };
  }
  return { from, to, anchor: to, head: from, empty: false };
}

function insertInlineCode(view, selection) {
  const { state } = view;

  if (!selection.empty) {
    let from = Math.min(selection.from, selection.to);
    let to = Math.max(selection.from, selection.to);
    while (to > from && state.doc.sliceString(to - 1, to) === '\n') {
      to -= 1;
    }
    const selectedText = state.doc.sliceString(from, to);
    const insert = `\`${selectedText}\``;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + insert.length },
    });
    return;
  }

  const insert = '``';
  view.dispatch({
    changes: { from: selection.from, insert },
    selection: { anchor: selection.from + 1 },
  });
}

function toggleInlineWrapper(view, selection, openMarker, closeMarker = openMarker) {
  const { state } = view;

  if (selection.empty) {
    const insert = `${openMarker}${closeMarker}`;
    view.dispatch({
      changes: { from: selection.from, insert },
      selection: { anchor: selection.from + openMarker.length },
    });
    return;
  }

  const from = Math.min(selection.from, selection.to);
  let to = Math.max(selection.from, selection.to);
  while (to > from && state.doc.sliceString(to - 1, to) === '\n') {
    to -= 1;
  }

  const hasOpenMarker =
    from >= openMarker.length && state.doc.sliceString(from - openMarker.length, from) === openMarker;
  const hasCloseMarker = state.doc.sliceString(to, to + closeMarker.length) === closeMarker;

  if (hasOpenMarker && hasCloseMarker) {
    view.dispatch({
      changes: [
        { from: to, to: to + closeMarker.length, insert: '' },
        { from: from - openMarker.length, to: from, insert: '' },
      ],
      selection: {
        anchor: from - openMarker.length,
        head: to - openMarker.length,
      },
    });
    return;
  }

  view.dispatch({
    changes: [
      { from: to, insert: closeMarker },
      { from, insert: openMarker },
    ],
    selection: {
      anchor: from + openMarker.length,
      head: to + openMarker.length,
    },
  });
}

function insertKbd(view, selection) {
  return toggleInlineWrapper(view, selection, '<kbd>', '</kbd>');
}

function insertInlineFence(view, selection, marker) {
  return toggleInlineWrapper(view, selection, marker);
}

function insertQuote(view, selection) {
  const { state } = view;
  const line = state.doc.lineAt(selection.from);
  const lineText = state.doc.sliceString(line.from, line.to);

  const existingQuote = /^(\s*)(>\s*)/.exec(lineText);
  if (existingQuote) {
    return;
  }

  const leadingWhitespace = /^(\s*)/.exec(lineText)[1];
  const insert = '> ';
  const contentStart = line.from + leadingWhitespace.length;
  const cursorOffset = selection.from - contentStart;

  view.dispatch({
    changes: { from: contentStart, insert },
    selection: { anchor: contentStart + insert.length + cursorOffset },
  });
}

function insertHr(view, selection) {
  const { state } = view;
  const line = state.doc.lineAt(selection.from);
  const lineText = state.doc.sliceString(line.from, line.to);
  const trimmed = lineText.trim();

  if (!trimmed) {
    const insert = '---';
    const cursorPos = line.from + insert.length;
    view.dispatch({
      changes: { from: line.from, to: line.to, insert },
      selection: { anchor: cursorPos },
    });
  } else {
    const insert = '\n---';
    const cursorPos = line.to + insert.length;
    view.dispatch({
      changes: { from: line.to, insert },
      selection: { anchor: cursorPos },
    });
  }
}

function insertLink(view, selection) {
  const { state } = view;

  if (!selection.empty) {
    let from = Math.min(selection.from, selection.to);
    let to = Math.max(selection.from, selection.to);
    while (to > from && state.doc.sliceString(to - 1, to) === '\n') {
      to -= 1;
    }
    const selectedText = state.doc.sliceString(from, to);
    const insert = `[${selectedText}]()`;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + insert.length - 1 },
    });
    return;
  }

  const insert = '[]()';
  view.dispatch({
    changes: { from: selection.from, insert },
    selection: { anchor: selection.from + 3 },
  });
}

function insertImage(view, selection) {
  const { state } = view;

  if (!selection.empty) {
    let from = Math.min(selection.from, selection.to);
    let to = Math.max(selection.from, selection.to);
    while (to > from && state.doc.sliceString(to - 1, to) === '\n') {
      to -= 1;
    }
    const selectedText = state.doc.sliceString(from, to);
    const insert = `![${selectedText}]()`;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + insert.length - 1 },
    });
    return;
  }

  const insert = '![]()';
  view.dispatch({
    changes: { from: selection.from, insert },
    selection: { anchor: selection.from + 4 },
  });
}

function insertWikiLink(view, selection) {
  const { state } = view;

  if (!selection.empty) {
    let from = Math.min(selection.from, selection.to);
    let to = Math.max(selection.from, selection.to);
    while (to > from && state.doc.sliceString(to - 1, to) === '\n') {
      to -= 1;
    }
    const selectedText = state.doc.sliceString(from, to);
    const insert = `[[${selectedText}]]`;
    view.dispatch({
      changes: { from, to, insert },
      selection: { anchor: from + insert.length },
    });
    return;
  }

  const insert = '[[]]';
  view.dispatch({
    changes: { from: selection.from, insert },
    selection: { anchor: selection.from + 2 },
  });
}

function sourceMode() {
  return [
    markdown({
      base: markdownLanguage,
      addKeymap: false,
      codeLanguages: resolveCodeLanguage,
      extensions: [{ remove: ['SetextHeading'] }],
    }),
    syntaxHighlighting(highlightStyle),
    sourceCodeBlockField,
    sourceListBorderField,
    sourceListMarkerField,
    sourceStrikeMarkerField,
    sourceWikiMarkerField,
    sourceFileLinkField,
    sourceUrlBoundaryField,
    sourceFootnoteMarkerField,
    sourceTableHeaderLineField,
    sourceFrontmatterField,
    gitDiffLineHighlightsField,
    ...headingCollapseSourceSpacerExtensions(),
    ...mergeConflictSourceExtensions(),
  ];
}

const blockedInlineSelectionAncestors = new Set([
  'FencedCode',
  'CodeBlock',
  'CodeText',
  'InlineCode',
  'URL',
  'Autolink',
  'HTMLBlock',
  'HTMLTag',
  'TableDelimiter',
]);

const latexSelectionBlockCache = new WeakMap<object, Array<{ from: number; to: number }>>();

function hasBlockedInlineAncestor(state, position) {
  let node = syntaxTree(state).resolveInner(position, 1);
  while (node) {
    if (blockedInlineSelectionAncestors.has(node.name)) {
      return true;
    }
    node = node.parent;
  }
  return false;
}

function getLatexSelectionBlockRanges(state) {
  const docKey = state.doc as unknown as object;
  const cached = latexSelectionBlockCache.get(docKey);
  if (cached) {
    return cached;
  }

  const text = state.doc.toString();
  if (text.indexOf('$') === -1) {
    latexSelectionBlockCache.set(docKey, []);
    return [];
  }

  const ranges = collectLatexMathRanges(text).map((range) => ({ from: range.from, to: range.to }));
  latexSelectionBlockCache.set(docKey, ranges);
  return ranges;
}

function overlapsLatexMathSelection(state, from, to) {
  if (to <= from) {
    return false;
  }
  const ranges = getLatexSelectionBlockRanges(state);
  for (const range of ranges) {
    if (range.from < to && range.to > from) {
      return true;
    }
  }
  return false;
}

function isRegularInlineSelection(state, from, to) {
  if (to <= from) {
    return false;
  }
  const text = state.doc.sliceString(from, to);
  const trimmedText = text.trim();
  if (!trimmedText) {
    return false;
  }
  if (trimmedText.includes('\n')) {
    return false;
  }
  if (hasBlockedInlineAncestor(state, from) || hasBlockedInlineAncestor(state, to - 1)) {
    return false;
  }
  if (overlapsLatexMathSelection(state, from, to)) {
    return false;
  }
  return true;
}
