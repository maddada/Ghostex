// @ts-nocheck
import { RangeSetBuilder, StateEffect, StateField, EditorState, Transaction } from '@codemirror/state';
import { GutterMarker, gutter, EditorView } from '@codemirror/view';
import {
  buildCurrentToBaselineLineMapFromLines,
  lcsDiffRuns,
  normalizeDiffLine,
  splitDiffLines
} from '../gitDiffCore';
import { getLiveGitCollapsedBlockAtLine, getLiveGitCollapsedBlocks } from './liveRenderedBlocks';

const MAX_DIFF_TEXT_CHARS = 1024 * 1024;
const MAX_DIFF_LINES = 1200;
const MAX_DIFF_CELLS = 1_500_000;
const NON_RENDERABLE_GIT_BASELINE_REASONS = new Set(['not-repo', 'ignored']);

export const setGitBaselineEffect = StateEffect.define<any>();

interface BaselineSnapshot {
  available: boolean;
  tracked: boolean;
  baseText: string | null;
  baseLines: string[] | null;
  headOid?: string | null;
  reason?: 'not-file' | 'git-unavailable' | 'not-repo' | 'ignored' | 'too-large' | 'binary' | 'error';
}

export interface MarkerFlags {
  added: boolean;
  modified: boolean;
  eofProxy?: boolean;
  trailingEofProxyOnly?: boolean;
  trailingEofProxySource?: boolean;
  liveBlockStartLine?: number;
  liveBlockEndLine?: number;
}

const emptyBaseline: BaselineSnapshot = Object.freeze({
  available: false,
  tracked: false,
  baseText: null,
  baseLines: null
});

function normalizeBaselineSnapshot(snapshot: any): BaselineSnapshot {
  if (!snapshot || typeof snapshot !== 'object') {
    return emptyBaseline;
  }
  const baseText = typeof snapshot.baseText === 'string' ? snapshot.baseText : null;
  return {
    available: snapshot.available === true,
    tracked: snapshot.tracked === true,
    headOid: typeof snapshot.headOid === 'string' ? snapshot.headOid : snapshot.headOid === null ? null : undefined,
    baseText,
    baseLines: typeof baseText === 'string' ? splitDiffLines(baseText) : null,
    reason: typeof snapshot.reason === 'string' ? snapshot.reason : undefined
  };
}

const gitBaselineField = StateField.define<BaselineSnapshot>({
  create(): BaselineSnapshot {
    return emptyBaseline;
  },
  update(value: BaselineSnapshot, tr: Transaction): BaselineSnapshot {
    for (const effect of tr.effects) {
      if (effect.is(setGitBaselineEffect)) {
        return normalizeBaselineSnapshot(effect.value);
      }
    }
    return value;
  }
});

class GitGutterMarker extends GutterMarker {
  flags: MarkerFlags;
  key: string;

  constructor(flags: MarkerFlags) {
    super();
    this.flags = flags;
    this.key = JSON.stringify(flags);
  }

  eq(other: GitGutterMarker): boolean {
    return other instanceof GitGutterMarker && other.key === this.key;
  }

  toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'meo-git-gutter-marker';
    if (Number.isInteger(this.flags.liveBlockStartLine)) {
      el.dataset.meoLiveBlockStartLine = String(this.flags.liveBlockStartLine);
    }
    if (Number.isInteger(this.flags.liveBlockEndLine)) {
      el.dataset.meoLiveBlockEndLine = String(this.flags.liveBlockEndLine);
    }
    if (this.flags.eofProxy) {
      el.classList.add('is-eof-proxy');
    }

    if (this.flags.added) {
      el.classList.add('is-added');
    }
    if (this.flags.modified) {
      el.classList.add('is-modified');
    }

    if (!this.flags.added && !this.flags.modified) {
      el.classList.add('is-empty');
    }

    const stripe = document.createElement('span');
    stripe.className = 'meo-git-gutter-stripe';
    el.appendChild(stripe);

    return el;
  }
}

class GitGutterSpacerMarker extends GutterMarker {
  toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'meo-git-gutter-marker meo-git-gutter-spacer';
    return el;
  }
}

const markerCache = new Map<string, GitGutterMarker>();
const spacerMarker = new GitGutterSpacerMarker();

function gitMarker(flags: MarkerFlags): GitGutterMarker {
  const key = JSON.stringify(flags);
  let marker = markerCache.get(key);
  if (!marker) {
    marker = new GitGutterMarker(flags);
    markerCache.set(key, marker);
  }
  return marker;
}

function getDocLines(doc: any): string[] {
  const lines = new Array(doc.lines);
  for (let i = 1; i <= doc.lines; i += 1) {
    const line = doc.line(i);
    lines[i - 1] = normalizeDiffLine(doc.sliceString(line.from, line.to));
  }
  return lines;
}

function isTrailingEofVisualLine(doc: any, lineNo: number): boolean {
  if (!doc || doc.length <= 0 || doc.lines <= 1 || lineNo !== doc.lines) {
    return false;
  }
  const lastLine = doc.line(doc.lines);
  return lastLine.from === lastLine.to;
}

function emptyMarkerFlags(): MarkerFlags {
  return {
    added: false,
    modified: false
  };
}

function coalesceTrailingEofVisualLineFlag(doc: any, lineFlags: (MarkerFlags | undefined)[] | null): (MarkerFlags | undefined)[] | null {
  if (!Array.isArray(lineFlags) || !isTrailingEofVisualLine(doc, doc.lines) || doc.lines < 2) {
    return lineFlags;
  }

  const trailingIndex = doc.lines - 1;
  const previousIndex = trailingIndex - 1;
  const trailingFlags = lineFlags[trailingIndex];
  if (!trailingFlags) {
    return lineFlags;
  }

  const previousFlags = lineFlags[previousIndex] ?? emptyMarkerFlags();
  const previousHadChange = !!(previousFlags.added || previousFlags.modified);
  if (trailingFlags.modified) {
    previousFlags.modified = true;
  }
  if (trailingFlags.added) {
    if (previousFlags.added) {
      previousFlags.added = true;
    } else if (!previousHadChange && !trailingFlags.modified) {
      previousFlags.trailingEofProxyOnly = true;
    } else {
      previousFlags.modified = true;
    }
  }
  previousFlags.trailingEofProxySource = true;
  lineFlags[previousIndex] = previousFlags;
  lineFlags[trailingIndex] = undefined;
  return lineFlags;
}

function canRenderGitDiffBaseline(snapshot: BaselineSnapshot | null): boolean {
  if (!snapshot?.available) {
    return false;
  }
  if (!snapshot.reason) {
    return true;
  }
  return !NON_RENDERABLE_GIT_BASELINE_REASONS.has(snapshot.reason);
}

function getTrailingEofProxyFlags(
  doc: any,
  lineFlags: (MarkerFlags | undefined)[] | null
): MarkerFlags | null {
  if (!isTrailingEofVisualLine(doc, doc.lines) || doc.lines <= 1 || !Array.isArray(lineFlags)) {
    return null;
  }

  const previousFlags = lineFlags[doc.lines - 2];
  if (!previousFlags || (!previousFlags.added && !previousFlags.modified)) {
    if (!previousFlags?.trailingEofProxyOnly) {
      return null;
    }
  }
  if (!previousFlags?.trailingEofProxySource && lineFlags[doc.lines - 1]) {
    return null;
  }

  return {
    added: previousFlags?.trailingEofProxyOnly ? false : !!previousFlags?.added,
    modified: previousFlags?.trailingEofProxyOnly ? true : !!previousFlags?.modified,
    eofProxy: true
  };
}

function buildLineFlagsFromRuns(runs: any[] | null, currentLineCount: number): (MarkerFlags | undefined)[] {
  const lineFlags: (MarkerFlags | undefined)[] = new Array(currentLineCount);
  if (!runs) {
    return lineFlags;
  }

  let currentLineNo = 1;

  for (let i = 0; i < runs.length; i += 1) {
    const run = runs[i];
    if (run.type === 'equal') {
      currentLineNo += run.count;
      continue;
    }

    if (run.type === 'insert') {
      const next = runs[i + 1];
      if (next?.type === 'delete') {
        const pairCount = Math.min(run.count, next.count);
        for (let offset = 0; offset < pairCount; offset += 1) {
          const index = currentLineNo - 1 + offset;
          if (index < 0 || index >= currentLineCount) {
            continue;
          }
          const flags = lineFlags[index] ?? (lineFlags[index] = emptyMarkerFlags());
          flags.modified = true;
        }
        for (let offset = pairCount; offset < run.count; offset += 1) {
          const index = currentLineNo - 1 + offset;
          if (index < 0 || index >= currentLineCount) {
            continue;
          }
          const flags = lineFlags[index] ?? (lineFlags[index] = emptyMarkerFlags());
          flags.added = true;
        }
        currentLineNo += run.count;
        i += 1;
        continue;
      }

      for (let offset = 0; offset < run.count; offset += 1) {
        const index = currentLineNo - 1 + offset;
        if (index < 0 || index >= currentLineCount) {
          continue;
        }
        const flags = lineFlags[index] ?? (lineFlags[index] = emptyMarkerFlags());
        flags.added = true;
      }
      currentLineNo += run.count;
      continue;
    }

    if (run.type === 'delete') {
      const next = runs[i + 1];
      if (next?.type === 'insert') {
        const pairCount = Math.min(run.count, next.count);
        for (let offset = 0; offset < pairCount; offset += 1) {
          const index = currentLineNo - 1 + offset;
          if (index < 0 || index >= currentLineCount) {
            continue;
          }
          const flags = lineFlags[index] ?? (lineFlags[index] = emptyMarkerFlags());
          flags.modified = true;
        }
        for (let offset = pairCount; offset < next.count; offset += 1) {
          const index = currentLineNo - 1 + offset;
          if (index < 0 || index >= currentLineCount) {
            continue;
          }
          const flags = lineFlags[index] ?? (lineFlags[index] = emptyMarkerFlags());
          flags.added = true;
        }
        currentLineNo += next.count;
        i += 1;
        continue;
      }
    }
  }

  return lineFlags;
}

function buildLineFlagsFromMapping(baseLines: string[], currentLines: string[], mapping: Record<number, number> | null): (MarkerFlags | undefined)[] {
  const lineFlags: (MarkerFlags | undefined)[] = new Array(currentLines.length);
  if (!mapping) {
    return lineFlags;
  }

  for (let lineNo = 1; lineNo <= currentLines.length; lineNo += 1) {
    const mappedBaseLineNo = mapping[lineNo] ?? 0;
    if (mappedBaseLineNo <= 0) {
      lineFlags[lineNo - 1] = { ...emptyMarkerFlags(), added: true };
      continue;
    }

    const baseText = baseLines[mappedBaseLineNo - 1];
    const currentText = currentLines[lineNo - 1];
    if (baseText === currentText) {
      continue;
    }
    lineFlags[lineNo - 1] = { ...emptyMarkerFlags(), modified: true };
  }

  return lineFlags;
}

function buildDiffLineFlags(state: EditorState, baseline: BaselineSnapshot | null): (MarkerFlags | undefined)[] | null {
  if (!canRenderGitDiffBaseline(baseline)) {
    return null;
  }

  if (typeof baseline.baseText !== 'string') {
    if (!baseline.tracked || baseline.headOid === null) {
      const lineFlags: (MarkerFlags | undefined)[] = new Array(state.doc.lines);
      const textLength = state.doc.length;
      if (!textLength && state.doc.lines === 1 && state.doc.sliceString(0, state.doc.length) === '') {
        return lineFlags;
      }
      for (let i = 0; i < state.doc.lines; i += 1) {
        lineFlags[i] = { ...emptyMarkerFlags(), added: true };
      }
      return lineFlags;
    }
    return null;
  }

  if (state.doc.length > MAX_DIFF_TEXT_CHARS || baseline.baseText.length > MAX_DIFF_TEXT_CHARS) {
    return null;
  }

  const baseLines = Array.isArray(baseline.baseLines) ? baseline.baseLines : splitDiffLines(baseline.baseText);
  const currentLines = getDocLines(state.doc);
  const mapping = buildCurrentToBaselineLineMapFromLines(baseLines, currentLines, {
    maxLines: MAX_DIFF_LINES,
    maxCells: MAX_DIFF_CELLS
  });
  if (mapping) {
    return buildLineFlagsFromMapping(baseLines, currentLines, mapping);
  }
  const runs = lcsDiffRuns(baseLines, currentLines, {
    maxLines: MAX_DIFF_LINES,
    maxCells: MAX_DIFF_CELLS
  });
  if (!runs) {
    return null;
  }

  return buildLineFlagsFromRuns(runs, currentLines.length);
}

function buildCoalescedDiffLineFlags(state: EditorState, baseline: BaselineSnapshot | null): (MarkerFlags | undefined)[] | null {
  return coalesceTrailingEofVisualLineFlag(state.doc, buildDiffLineFlags(state, baseline));
}

function buildGitGutterMarkersFromLineFlags(state: EditorState, lineFlags: (MarkerFlags | undefined)[] | null): any {
  const builder = new RangeSetBuilder<any>();
  if (!lineFlags) {
    return builder.finish();
  }
  const trailingEofProxyFlags = getTrailingEofProxyFlags(state.doc, lineFlags);

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    if (isTrailingEofVisualLine(state.doc, lineNo)) {
      if (trailingEofProxyFlags) {
        const line = state.doc.line(lineNo);
        builder.add(line.from, line.from, gitMarker(trailingEofProxyFlags));
      }
      continue;
    }
    const flags = lineFlags[lineNo - 1];
    if (!flags) {
      continue;
    }
    if (flags.trailingEofProxyOnly) {
      continue;
    }
    const line = state.doc.line(lineNo);
    builder.add(line.from, line.from, gitMarker(flags));
  }

  return builder.finish();
}

function buildLiveGitGutterMarkersFromLineFlags(state: EditorState, lineFlags: (MarkerFlags | undefined)[] | null): any {
  const builder = new RangeSetBuilder<any>();
  if (!lineFlags) {
    return builder.finish();
  }

  const collapsedBlocks = getLiveGitCollapsedBlocks(state, lineFlags);
  let collapsedBlockIndex = 0;
  let activeCollapsedBlock = collapsedBlocks[collapsedBlockIndex] ?? null;
  let activeCollapsedFlags = activeCollapsedBlock ? liveCollapsedBlockMarkerFlags(activeCollapsedBlock) : null;

  const trailingEofProxyFlags = getTrailingEofProxyFlags(state.doc, lineFlags);

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);

    while (activeCollapsedBlock && lineNo > activeCollapsedBlock.endLine) {
      collapsedBlockIndex += 1;
      activeCollapsedBlock = collapsedBlocks[collapsedBlockIndex] ?? null;
      activeCollapsedFlags = activeCollapsedBlock ? liveCollapsedBlockMarkerFlags(activeCollapsedBlock) : null;
    }

    if (activeCollapsedBlock && lineNo >= activeCollapsedBlock.startLine && activeCollapsedFlags) {
      builder.add(line.from, line.from, gitMarker(activeCollapsedFlags));
      continue;
    }

    if (isTrailingEofVisualLine(state.doc, lineNo)) {
      if (trailingEofProxyFlags) {
        builder.add(line.from, line.from, gitMarker(trailingEofProxyFlags));
      }
      continue;
    }

    const flags = lineFlags[lineNo - 1];
    if (!flags || flags.trailingEofProxyOnly) {
      continue;
    }
    builder.add(line.from, line.from, gitMarker(flags));
  }

  return builder.finish();
}

function liveCollapsedBlockMarkerFlags(block: { startLine: number; endLine: number; aggregateChangeKind: 'added' | 'modified' }): MarkerFlags {
  return block.aggregateChangeKind === 'modified'
    ? {
        ...emptyMarkerFlags(),
        modified: true,
        liveBlockStartLine: block.startLine,
        liveBlockEndLine: block.endLine
      }
    : {
        ...emptyMarkerFlags(),
        added: true,
        liveBlockStartLine: block.startLine,
        liveBlockEndLine: block.endLine
      };
}

function liveCollapsedBlockMarkerAtPos(
  state: EditorState,
  lineFlags: (MarkerFlags | undefined)[] | null,
  pos: number
): GitGutterMarker | null {
  if (!Array.isArray(lineFlags)) {
    return null;
  }
  const lineNo = state.doc.lineAt(Math.max(0, Math.min(pos, state.doc.length))).number;
  const block = getLiveGitCollapsedBlockAtLine(state, lineFlags, lineNo);
  return block ? gitMarker(liveCollapsedBlockMarkerFlags(block)) : null;
}

export const gitDiffLineFlagsField = StateField.define<(MarkerFlags | undefined)[] | null>({
  create(state: EditorState): (MarkerFlags | undefined)[] | null {
    return buildCoalescedDiffLineFlags(state, state.field(gitBaselineField));
  },
  update(value: (MarkerFlags | undefined)[] | null, tr: Transaction): (MarkerFlags | undefined)[] | null {
    let baselineChanged = false;
    for (const effect of tr.effects) {
      if (effect.is(setGitBaselineEffect)) {
        baselineChanged = true;
        break;
      }
    }
    if (!tr.docChanged && !baselineChanged) {
      return value;
    }
    const baseline = tr.state.field(gitBaselineField);
    return buildCoalescedDiffLineFlags(tr.state, baseline);
  }
});

const gitDiffGutterField = StateField.define<any>({
  create(state: EditorState): any {
    return buildGitGutterMarkersFromLineFlags(state, state.field(gitDiffLineFlagsField));
  },
  update(value: any, tr: Transaction): any {
    let baselineChanged = false;
    for (const effect of tr.effects) {
      if (effect.is(setGitBaselineEffect)) {
        baselineChanged = true;
        break;
      }
    }
    if (!tr.docChanged && !baselineChanged) {
      return value;
    }
    return buildGitGutterMarkersFromLineFlags(tr.state, tr.state.field(gitDiffLineFlagsField));
  }
});

const gitDiffGutterExtension = gutter({
  class: 'meo-git-gutter',
  renderEmptyElements: true,
  initialSpacer() {
    return spacerMarker;
  },
  markers(view: EditorView) {
    return (
      view.state.field(gitDiffGutterField, false) ??
      buildGitGutterMarkersFromLineFlags(view.state, view.state.field(gitDiffLineFlagsField, false))
    );
  }
});

const gitDiffGutterLiveExtension = gutter({
  class: 'meo-git-gutter',
  renderEmptyElements: true,
  initialSpacer() {
    return spacerMarker;
  },
  markers(view: EditorView) {
    return buildLiveGitGutterMarkersFromLineFlags(view.state, view.state.field(gitDiffLineFlagsField, false));
  },
  widgetMarker(view: EditorView, _widget: any, block: any) {
    return liveCollapsedBlockMarkerAtPos(view.state, view.state.field(gitDiffLineFlagsField, false), block.from);
  }
});

export function gitDiffGutterBaselineExtensions(): any[] {
  return [gitBaselineField, gitDiffLineFlagsField];
}

export function gitDiffGutterRenderExtensions(): any[] {
  return [gitDiffGutterField, gitDiffGutterExtension];
}

export function gitDiffGutterLiveRenderExtensions(): any[] {
  return [gitDiffGutterLiveExtension];
}

interface DiffSegment {
  fromLine: number;
  toLine: number;
  added: boolean;
  modified: boolean;
}

export function getGitDiffOverviewSegments(state: EditorState): DiffSegment[] {
  const lineFlags = state.field(gitDiffLineFlagsField, false);
  if (!Array.isArray(lineFlags) || !lineFlags.length) {
    return [];
  }

  const segments: DiffSegment[] = [];
  let active: DiffSegment | null = null;

  const flush = () => {
    if (!active) {
      return;
    }
    segments.push(active);
    active = null;
  };

  for (let lineNo = 1; lineNo <= lineFlags.length; lineNo += 1) {
    const flags = lineFlags[lineNo - 1];
    const added = !!flags?.added;
    const modified = !!flags?.modified || !!flags?.trailingEofProxyOnly;
    if (!added && !modified) {
      flush();
      continue;
    }

    if (
      active &&
      active.toLine + 1 === lineNo &&
      active.added === added &&
      active.modified === modified
    ) {
      active.toLine = lineNo;
      continue;
    }

    flush();
    active = { fromLine: lineNo, toLine: lineNo, added, modified };
  }

  flush();
  return segments;
}

export function setGitBaseline(view: EditorView, snapshot: any): void {
  view.dispatch({
    effects: setGitBaselineEffect.of(snapshot)
  });
}
