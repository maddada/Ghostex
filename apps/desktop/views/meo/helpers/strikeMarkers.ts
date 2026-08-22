// @ts-nocheck
import { StateField, RangeSetBuilder, EditorState } from '@codemirror/state';
import { EditorView, Decoration } from '@codemirror/view';
import { resolvedSyntaxTree } from './markdownSyntax';

interface StrikeRange {
  from: number;
  to: number;
}

interface StrikePair {
  lineNo: number;
  openFrom: number;
  openTo: number;
  closeFrom: number;
  closeTo: number;
  strikeFrom: number;
  strikeTo: number;
}

function isEscaped(text: string, index: number): boolean {
  let backslashes = 0;
  for (let i = index - 1; i >= 0 && text[i] === '\\'; i -= 1) {
    backslashes += 1;
  }
  return backslashes % 2 === 1;
}

function isWhitespace(char: string): boolean {
  return char !== '' && /\s/u.test(char);
}

function isWordChar(char: string): boolean {
  return char !== '' && /[0-9A-Za-z_]/u.test(char);
}

function isBoundaryChar(char: string): boolean {
  return char === '' || isWhitespace(char) || !isWordChar(char);
}

function canOpenSingleTilde(text: string, index: number): boolean {
  const previous = index > 0 ? text[index - 1] : '';
  const next = index + 1 < text.length ? text[index + 1] : '';
  return !isWhitespace(next) && isBoundaryChar(previous);
}

function canCloseSingleTilde(text: string, index: number): boolean {
  const previous = index > 0 ? text[index - 1] : '';
  const next = index + 1 < text.length ? text[index + 1] : '';
  return !isWhitespace(previous) && isBoundaryChar(next);
}

export function collectStrikethroughRanges(tree: any): StrikeRange[] {
  const ranges: StrikeRange[] = [];
  tree.iterate({
    enter(node: any) {
      if (node.name === 'Strikethrough') {
        ranges.push({ from: node.from, to: node.to });
      }
    }
  });
  return ranges;
}

export function collectSingleTildeStrikePairs(state: EditorState, strikeRanges: StrikeRange[] = []): StrikePair[] {
  const pairs: StrikePair[] = [];
  let overlapIndex = 0;

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    const text = line.text;
    if (!text.includes('~')) {
      continue;
    }
    let index = 0;

    while (index < text.length) {
      if (text[index] !== '~' || isEscaped(text, index)) {
        index += 1;
        continue;
      }
      if (text[index - 1] === '~' || text[index + 1] === '~') {
        index += 1;
        continue;
      }
      if (!canOpenSingleTilde(text, index)) {
        index += 1;
        continue;
      }

      let close = -1;
      for (let i = index + 1; i < text.length; i += 1) {
        if (text[i] !== '~' || isEscaped(text, i)) {
          continue;
        }
        if (text[i - 1] === '~' || text[i + 1] === '~') {
          continue;
        }
        if (!canCloseSingleTilde(text, i)) {
          continue;
        }
        close = i;
        break;
      }

      if (close === -1 || close <= index + 1) {
        index += 1;
        continue;
      }

      const strikeFrom = line.from + index + 1;
      const strikeTo = line.from + close;
      while (overlapIndex < strikeRanges.length && strikeRanges[overlapIndex].to <= strikeFrom) {
        overlapIndex += 1;
      }

      let overlaps = false;
      for (let i = overlapIndex; i < strikeRanges.length; i += 1) {
        const range = strikeRanges[i];
        if (range.from >= strikeTo) {
          break;
        }
        if (strikeFrom < range.to && strikeTo > range.from) {
          overlaps = true;
          break;
        }
      }

      if (!overlaps) {
        pairs.push({
          lineNo,
          openFrom: line.from + index,
          openTo: line.from + index + 1,
          closeFrom: line.from + close,
          closeTo: line.from + close + 1,
          strikeFrom,
          strikeTo
        });
      }

      index = close + 1;
    }
  }

  return pairs;
}

const sourceStrikeMarkerDeco = Decoration.mark({ class: 'meo-md-strike-marker' });

function computeSourceStrikeMarkers(state: EditorState): any {
  const ranges = new RangeSetBuilder<any>();
  const tree = resolvedSyntaxTree(state);
  const strikeRanges = collectStrikethroughRanges(tree);
  tree.iterate({
    enter(node: any) {
      if (node.name !== 'StrikethroughMark') {
        return;
      }
      ranges.add(node.from, node.to, sourceStrikeMarkerDeco);
    }
  });

  const pairs = collectSingleTildeStrikePairs(state, strikeRanges);
  for (const pair of pairs) {
    ranges.add(pair.openFrom, pair.openTo, sourceStrikeMarkerDeco);
    ranges.add(pair.closeFrom, pair.closeTo, sourceStrikeMarkerDeco);
  }

  return ranges.finish();
}

export const sourceStrikeMarkerField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return computeSourceStrikeMarkers(state);
    } catch {
      return Decoration.none;
    }
  },
  update(markers: any, transaction: any) {
    if (!transaction.docChanged) {
      return markers;
    }
    try {
      return computeSourceStrikeMarkers(transaction.state);
    } catch {
      return markers;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field)
});
