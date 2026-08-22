// @ts-nocheck
export type DiffRun = {
  type: 'equal' | 'insert' | 'delete';
  count: number;
};

export type DiffLcsLimits = {
  // Scalable mapping uses these as exact-LCS chunk limits, not whole-file stop limits.
  maxLines?: number;
  maxCells?: number;
};

type DiffSegment = {
  baseStart: number;
  baseEnd: number;
  currentStart: number;
  currentEnd: number;
};

type AnchorPair = {
  baseIndex: number;
  currentIndex: number;
};

const DEFAULT_EXACT_DIFF_MAX_LINES = 1024;
const DEFAULT_EXACT_DIFF_MAX_CELLS = 1_000_000;
const WINDOW_ANCHOR_RADIUS_MIN = 64;
const WINDOW_ANCHOR_RADIUS_MAX = 512;
const WINDOW_ANCHOR_MAX_CANDIDATES_PER_LINE = 8;

export function normalizeDiffLine(lineText: string): string {
  return lineText.endsWith('\r') ? lineText.slice(0, -1) : lineText;
}

export function splitDiffLines(text: string): string[] {
  return `${text ?? ''}`.split('\n').map(normalizeDiffLine);
}

function hasLaterLineOccurrence(lines: string[], startIndex: number, lineText: string, maxLookahead = 64): boolean {
  const limit = Math.min(lines.length, startIndex + 1 + maxLookahead);
  for (let index = startIndex + 1; index < limit; index += 1) {
    if (lines[index] === lineText) {
      return true;
    }
  }
  return false;
}

function buildLcsMatrix(a: string[], b: string[]): { matrix: Uint32Array; rowSize: number } {
  const n = a.length;
  const m = b.length;
  const rowSize = m + 1;
  const matrix = new Uint32Array((n + 1) * rowSize);

  for (let i = n - 1; i >= 0; i -= 1) {
    const rowIndex = i * rowSize;
    const nextRowIndex = (i + 1) * rowSize;
    for (let j = m - 1; j >= 0; j -= 1) {
      matrix[rowIndex + j] = a[i] === b[j]
        ? matrix[nextRowIndex + j + 1] + 1
        : Math.max(matrix[nextRowIndex + j], matrix[rowIndex + j + 1]);
    }
  }

  return { matrix, rowSize };
}

export function lcsDiffRuns(baseLines: string[], currentLines: string[], limits: DiffLcsLimits = {}): DiffRun[] | null {
  const n = baseLines.length;
  const m = currentLines.length;
  if (!n && !m) {
    return [];
  }

  if (typeof limits.maxLines === 'number' && (n > limits.maxLines || m > limits.maxLines)) {
    return null;
  }
  if (typeof limits.maxCells === 'number' && n * m > limits.maxCells) {
    return null;
  }

  const { matrix, rowSize } = buildLcsMatrix(baseLines, currentLines);
  const runs: DiffRun[] = [];

  const pushRun = (type: DiffRun['type'], count: number) => {
    if (!count) {
      return;
    }
    const prev = runs[runs.length - 1];
    if (prev && prev.type === type) {
      prev.count += count;
      return;
    }
    runs.push({ type, count });
  };

  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    const deleteScore = matrix[(i + 1) * rowSize + j];
    const insertScore = matrix[i * rowSize + (j + 1)];

    if (baseLines[i] === currentLines[j]) {
      const equalScore = matrix[(i + 1) * rowSize + (j + 1)] + 1;
      const optionalEqual = Math.max(deleteScore, insertScore) === equalScore;
      const ambiguousRepeatedLine = optionalEqual && (
        hasLaterLineOccurrence(baseLines, i, baseLines[i]) ||
        hasLaterLineOccurrence(currentLines, j, currentLines[j])
      );

      if (!ambiguousRepeatedLine) {
        pushRun('equal', 1);
        i += 1;
        j += 1;
        continue;
      }
    }

    if (deleteScore > insertScore) {
      pushRun('delete', 1);
      i += 1;
      continue;
    }

    if (insertScore > deleteScore) {
      pushRun('insert', 1);
      j += 1;
      continue;
    }

    if (i + 1 < n && baseLines[i + 1] === currentLines[j]) {
      pushRun('delete', 1);
      i += 1;
    } else {
      pushRun('insert', 1);
      j += 1;
    }
  }

  if (i < n) {
    pushRun('delete', n - i);
  }
  if (j < m) {
    pushRun('insert', m - j);
  }

  return runs;
}

function mapEqualLineRun(mapping: Int32Array, baseStart: number, currentStart: number, count: number): void {
  for (let offset = 0; offset < count; offset += 1) {
    mapping[currentStart + 1 + offset] = baseStart + 1 + offset;
  }
}

function applyRunsToMapping(
  mapping: Int32Array,
  runs: DiffRun[],
  baseStart: number,
  currentStart: number
): void {
  let baseLineNo = baseStart + 1;
  let currentLineNo = currentStart + 1;
  const applyPairedModifiedRunMap = (currentCount: number, baseCount: number) => {
    const pairCount = Math.min(currentCount, baseCount);
    for (let offset = 0; offset < pairCount; offset += 1) {
      mapping[currentLineNo + offset] = baseLineNo + offset;
    }
    currentLineNo += currentCount;
    baseLineNo += baseCount;
  };

  for (let runIndex = 0; runIndex < runs.length; runIndex += 1) {
    const run = runs[runIndex];
    if (run.type === 'equal') {
      for (let offset = 0; offset < run.count; offset += 1) {
        mapping[currentLineNo + offset] = baseLineNo + offset;
      }
      baseLineNo += run.count;
      currentLineNo += run.count;
      continue;
    }

    if (run.type === 'insert') {
      const next = runs[runIndex + 1];
      if (next?.type === 'delete') {
        applyPairedModifiedRunMap(run.count, next.count);
        runIndex += 1;
        continue;
      }
      currentLineNo += run.count;
      continue;
    }

    const next = runs[runIndex + 1];
    if (next?.type === 'insert') {
      applyPairedModifiedRunMap(next.count, run.count);
      runIndex += 1;
      continue;
    }
    baseLineNo += run.count;
  }
}

function canUseExactDiffForSegment(baseLen: number, currentLen: number, limits: DiffLcsLimits): boolean {
  const maxLines = typeof limits.maxLines === 'number' ? limits.maxLines : DEFAULT_EXACT_DIFF_MAX_LINES;
  const maxCells = typeof limits.maxCells === 'number' ? limits.maxCells : DEFAULT_EXACT_DIFF_MAX_CELLS;
  if (baseLen > maxLines || currentLen > maxLines) {
    return false;
  }
  if (!baseLen || !currentLen) {
    return true;
  }
  return currentLen <= Math.floor(maxCells / baseLen);
}

function tryApplyExactDiffSegment(
  mapping: Int32Array,
  baseLines: string[],
  currentLines: string[],
  segment: DiffSegment,
  limits: DiffLcsLimits
): boolean {
  const baseLen = segment.baseEnd - segment.baseStart;
  const currentLen = segment.currentEnd - segment.currentStart;
  if (!canUseExactDiffForSegment(baseLen, currentLen, limits)) {
    return false;
  }

  const runs = lcsDiffRuns(
    baseLines.slice(segment.baseStart, segment.baseEnd),
    currentLines.slice(segment.currentStart, segment.currentEnd),
    limits
  );
  if (!runs) {
    return false;
  }

  applyRunsToMapping(mapping, runs, segment.baseStart, segment.currentStart);
  return true;
}

function buildUniqueLinePositionMap(lines: string[], start: number, end: number): Map<string, number> {
  const counts = new Map<string, number>();
  const firstIndex = new Map<string, number>();
  for (let index = start; index < end; index += 1) {
    const lineText = lines[index];
    counts.set(lineText, (counts.get(lineText) ?? 0) + 1);
    if (!firstIndex.has(lineText)) {
      firstIndex.set(lineText, index);
    }
  }

  const uniquePositions = new Map<string, number>();
  for (const [lineText, count] of counts) {
    if (count !== 1) {
      continue;
    }
    const index = firstIndex.get(lineText);
    if (typeof index === 'number') {
      uniquePositions.set(lineText, index);
    }
  }
  return uniquePositions;
}

function longestIncreasingAnchorSubsequence(pairs: AnchorPair[]): AnchorPair[] {
  if (pairs.length <= 1) {
    return pairs;
  }

  const prev = new Array<number>(pairs.length).fill(-1);
  const tails: number[] = [];

  for (let index = 0; index < pairs.length; index += 1) {
    const value = pairs[index].currentIndex;
    let low = 0;
    let high = tails.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (pairs[tails[mid]].currentIndex < value) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    if (low > 0) {
      prev[index] = tails[low - 1];
    }
    tails[low] = index;
  }

  const result = new Array<AnchorPair>(tails.length);
  let cursor = tails[tails.length - 1];
  for (let outIndex = result.length - 1; outIndex >= 0; outIndex -= 1) {
    result[outIndex] = pairs[cursor];
    cursor = prev[cursor];
  }
  return result;
}

function findPatienceAnchors(
  baseLines: string[],
  currentLines: string[],
  segment: DiffSegment
): AnchorPair[] {
  const uniqueBase = buildUniqueLinePositionMap(baseLines, segment.baseStart, segment.baseEnd);
  const uniqueCurrent = buildUniqueLinePositionMap(currentLines, segment.currentStart, segment.currentEnd);
  if (!uniqueBase.size || !uniqueCurrent.size) {
    return [];
  }

  const pairs: AnchorPair[] = [];
  for (let baseIndex = segment.baseStart; baseIndex < segment.baseEnd; baseIndex += 1) {
    const lineText = baseLines[baseIndex];
    const uniqueBaseIndex = uniqueBase.get(lineText);
    if (uniqueBaseIndex !== baseIndex) {
      continue;
    }
    const currentIndex = uniqueCurrent.get(lineText);
    if (typeof currentIndex === 'number') {
      pairs.push({ baseIndex, currentIndex });
    }
  }

  return longestIncreasingAnchorSubsequence(pairs);
}

function buildSparseLinePositionIndex(
  lines: string[],
  start: number,
  end: number,
  maxCandidatesPerLine: number
): Map<string, number[] | null> {
  const indexByLine = new Map<string, number[] | null>();
  for (let lineIndex = start; lineIndex < end; lineIndex += 1) {
    const lineText = lines[lineIndex];
    if (!indexByLine.has(lineText)) {
      indexByLine.set(lineText, [lineIndex]);
      continue;
    }
    const entry = indexByLine.get(lineText);
    if (!entry) {
      continue;
    }
    entry.push(lineIndex);
    if (entry.length > maxCandidatesPerLine) {
      indexByLine.set(lineText, null);
    }
  }
  return indexByLine;
}

function findWindowAnchors(
  baseLines: string[],
  currentLines: string[],
  segment: DiffSegment
): AnchorPair[] {
  const baseLen = segment.baseEnd - segment.baseStart;
  const currentLen = segment.currentEnd - segment.currentStart;
  if (!baseLen || !currentLen) {
    return [];
  }

  const expectedScale = baseLen / currentLen;
  const radius = Math.max(
    WINDOW_ANCHOR_RADIUS_MIN,
    Math.min(WINDOW_ANCHOR_RADIUS_MAX, Math.floor(Math.max(baseLen, currentLen) / 8))
  );
  const baseIndexByLine = buildSparseLinePositionIndex(
    baseLines,
    segment.baseStart,
    segment.baseEnd,
    WINDOW_ANCHOR_MAX_CANDIDATES_PER_LINE
  );

  const anchors: AnchorPair[] = [];
  let lastBaseIndex = segment.baseStart - 1;
  for (let currentIndex = segment.currentStart; currentIndex < segment.currentEnd; currentIndex += 1) {
    const positions = baseIndexByLine.get(currentLines[currentIndex]);
    if (!positions) {
      continue;
    }

    const expectedBaseIndex = segment.baseStart + Math.round((currentIndex - segment.currentStart) * expectedScale);
    let chosenBaseIndex = -1;
    let bestDistance = Number.POSITIVE_INFINITY;
    for (let posIndex = 0; posIndex < positions.length; posIndex += 1) {
      const baseIndex = positions[posIndex];
      if (baseIndex <= lastBaseIndex) {
        continue;
      }
      const distance = Math.abs(baseIndex - expectedBaseIndex);
      if (distance > radius) {
        continue;
      }
      if (distance < bestDistance) {
        bestDistance = distance;
        chosenBaseIndex = baseIndex;
      }
    }

    if (chosenBaseIndex < 0) {
      continue;
    }

    anchors.push({ baseIndex: chosenBaseIndex, currentIndex });
    lastBaseIndex = chosenBaseIndex;
  }

  return longestIncreasingAnchorSubsequence(anchors);
}

function applyHeuristicPairedSegment(
  mapping: Int32Array,
  segment: DiffSegment
): void {
  const pairCount = Math.min(segment.baseEnd - segment.baseStart, segment.currentEnd - segment.currentStart);
  if (pairCount <= 0) {
    return;
  }
  mapEqualLineRun(mapping, segment.baseStart, segment.currentStart, pairCount);
}

function pushAnchoredGaps(
  stack: DiffSegment[],
  mapping: Int32Array,
  segment: DiffSegment,
  anchors: AnchorPair[]
): void {
  const gaps: DiffSegment[] = [];
  let baseCursor = segment.baseStart;
  let currentCursor = segment.currentStart;
  let anchorIndex = 0;

  while (anchorIndex < anchors.length) {
    const runStart = anchors[anchorIndex];
    let runEndIndex = anchorIndex;
    while (
      runEndIndex + 1 < anchors.length &&
      anchors[runEndIndex + 1].baseIndex === anchors[runEndIndex].baseIndex + 1 &&
      anchors[runEndIndex + 1].currentIndex === anchors[runEndIndex].currentIndex + 1
    ) {
      runEndIndex += 1;
    }

    const runCount = runEndIndex - anchorIndex + 1;
    if (runStart.baseIndex > baseCursor || runStart.currentIndex > currentCursor) {
      gaps.push({
        baseStart: baseCursor,
        baseEnd: runStart.baseIndex,
        currentStart: currentCursor,
        currentEnd: runStart.currentIndex
      });
    }

    mapEqualLineRun(mapping, runStart.baseIndex, runStart.currentIndex, runCount);
    baseCursor = runStart.baseIndex + runCount;
    currentCursor = runStart.currentIndex + runCount;
    anchorIndex = runEndIndex + 1;
  }

  if (baseCursor < segment.baseEnd || currentCursor < segment.currentEnd) {
    gaps.push({
      baseStart: baseCursor,
      baseEnd: segment.baseEnd,
      currentStart: currentCursor,
      currentEnd: segment.currentEnd
    });
  }

  for (let gapIndex = gaps.length - 1; gapIndex >= 0; gapIndex -= 1) {
    const gap = gaps[gapIndex];
    if (gap.baseStart === gap.baseEnd && gap.currentStart === gap.currentEnd) {
      continue;
    }
    stack.push(gap);
  }
}

function buildCurrentToBaselineLineMapScalableFromLines(
  baseLines: string[],
  currentLines: string[],
  limits: DiffLcsLimits = {}
): Int32Array {
  const mapping = new Int32Array(currentLines.length + 1);
  const stack: DiffSegment[] = [{
    baseStart: 0,
    baseEnd: baseLines.length,
    currentStart: 0,
    currentEnd: currentLines.length
  }];

  while (stack.length > 0) {
    const rawSegment = stack.pop();
    if (!rawSegment) {
      continue;
    }

    let { baseStart, baseEnd, currentStart, currentEnd } = rawSegment;

    while (
      baseStart < baseEnd &&
      currentStart < currentEnd &&
      baseLines[baseStart] === currentLines[currentStart]
    ) {
      mapping[currentStart + 1] = baseStart + 1;
      baseStart += 1;
      currentStart += 1;
    }

    while (
      baseStart < baseEnd &&
      currentStart < currentEnd &&
      baseLines[baseEnd - 1] === currentLines[currentEnd - 1]
    ) {
      mapping[currentEnd] = baseEnd;
      baseEnd -= 1;
      currentEnd -= 1;
    }

    if (baseStart >= baseEnd || currentStart >= currentEnd) {
      continue;
    }

    const segment: DiffSegment = { baseStart, baseEnd, currentStart, currentEnd };
    if (tryApplyExactDiffSegment(mapping, baseLines, currentLines, segment, limits)) {
      continue;
    }

    const patienceAnchors = findPatienceAnchors(baseLines, currentLines, segment);
    if (patienceAnchors.length > 0) {
      pushAnchoredGaps(stack, mapping, segment, patienceAnchors);
      continue;
    }

    const windowAnchors = findWindowAnchors(baseLines, currentLines, segment);
    if (windowAnchors.length > 0) {
      pushAnchoredGaps(stack, mapping, segment, windowAnchors);
      continue;
    }

    // Last-resort stable mapping for large ambiguous regions: pair by position and
    // leave unmatched inserted lines at 0. This avoids all-or-nothing null results.
    applyHeuristicPairedSegment(mapping, segment);
  }

  return mapping;
}

export function buildCurrentToBaselineLineMapFromLines(
  baseLines: string[],
  currentLines: string[],
  limits: DiffLcsLimits = {}
): Int32Array | null {
  return buildCurrentToBaselineLineMapScalableFromLines(baseLines, currentLines, limits);
}

export function buildCurrentToBaselineLineMap(
  baseText: string,
  currentText: string,
  limits: DiffLcsLimits = {}
): Int32Array | null {
  const baseLines = splitDiffLines(baseText);
  const currentLines = splitDiffLines(currentText);
  return buildCurrentToBaselineLineMapFromLines(baseLines, currentLines, limits);
}
