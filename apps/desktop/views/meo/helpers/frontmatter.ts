// @ts-nocheck
import { StateField, RangeSetBuilder, EditorState } from '@codemirror/state';
import { Decoration, EditorView } from '@codemirror/view';

const thematicBreakRe = /^[ \t]{0,3}(?:([-*_])(?:[ \t]*\1){2,})[ \t]*$/;
const frontmatterCache = new WeakMap<object, FrontmatterInfo | null>();

interface FrontmatterInfo {
  openingFrom: number;
  openingTo: number;
  contentFrom: number;
  contentTo: number;
  closingFrom: number;
  closingTo: number;
  from: number;
  to: number;
}

interface YamlFieldOffsets {
  keyFromOffset: number;
  keyToOffset: number;
  valueFromOffset: number | null;
}

interface YamlArrayValue {
  fromOffset: number;
  toOffset: number;
  items: Array<{ fromOffset: number; toOffset: number; text: string }>;
}

interface LineInfo {
  from: number;
  to: number;
  text: string;
  number: number;
}

interface YamlFieldRange {
  line: LineInfo;
  keyFrom: number;
  keyTo: number;
  valueFrom: number | null;
  valueTo: number;
}

function isFrontmatterDelimiterLine(lineText: string): boolean {
  return lineText.trim() === '---';
}

export function isThematicBreakLine(lineText: string): boolean {
  const first = lineText.trimStart()[0];
  if (first !== '-' && first !== '*' && first !== '_') {
    return false;
  }
  return thematicBreakRe.test(lineText);
}

export function isInsideFrontmatter(frontmatter: FrontmatterInfo | null, pos: number): boolean {
  return Boolean(frontmatter && pos >= frontmatter.from && pos < frontmatter.to);
}

export function isInsideFrontmatterContent(frontmatter: FrontmatterInfo | null, pos: number): boolean {
  return Boolean(frontmatter && pos >= frontmatter.contentFrom && pos < frontmatter.contentTo);
}

export function parseFrontmatter(state: EditorState): FrontmatterInfo | null {
  const { doc } = state;
  const cached = frontmatterCache.get(doc);
  if (cached !== undefined) {
    return cached;
  }

  let parsed: FrontmatterInfo | null = null;
  if (doc.lines >= 2) {
    const openingLine = doc.line(1);
    if (isFrontmatterDelimiterLine(openingLine.text)) {
      const openingOffset = openingLine.text.indexOf('---');
      for (let lineNo = 2; lineNo <= doc.lines; lineNo += 1) {
        const closingLine = doc.line(lineNo);
        if (!isFrontmatterDelimiterLine(closingLine.text)) {
          continue;
        }
        const closingOffset = closingLine.text.indexOf('---');
        parsed = {
          openingFrom: openingLine.from + openingOffset,
          openingTo: openingLine.from + openingOffset + 3,
          contentFrom: doc.line(2).from,
          contentTo: closingLine.from,
          closingFrom: closingLine.from + closingOffset,
          closingTo: closingLine.from + closingOffset + 3,
          from: openingLine.from,
          to: closingLine.from + closingOffset + 3,
        };
        break;
      }
    }
  }

  frontmatterCache.set(doc, parsed);
  return parsed;
}

export const sourceFrontmatterField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return buildSourceFrontmatterDecorations(state);
    } catch {
      return Decoration.none;
    }
  },
  update(value: any, tr: any) {
    if (!tr.docChanged) {
      return value;
    }
    try {
      return buildSourceFrontmatterDecorations(tr.state);
    } catch {
      return value;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field),
});

const sourceFrontmatterContentLineDeco = Decoration.line({
  class: 'meo-md-frontmatter-line meo-md-frontmatter-content',
});
const sourceFrontmatterDelimiterLineDeco = Decoration.line({ class: 'meo-md-frontmatter-delimiter-line' });
const sourceFrontmatterKeyDeco = Decoration.mark({ class: 'meo-md-frontmatter-key' });
const sourceFrontmatterValueDeco = Decoration.mark({ class: 'meo-md-frontmatter-value' });

export function yamlFrontmatterFieldOffsets(lineText: string): YamlFieldOffsets | null {
  let offset = 0;
  while (offset < lineText.length && (lineText[offset] === ' ' || lineText[offset] === '\t')) {
    offset += 1;
  }

  if (lineText[offset] === '-' && /\s/.test(lineText[offset + 1] ?? '')) {
    offset += 1;
    while (offset < lineText.length && (lineText[offset] === ' ' || lineText[offset] === '\t')) {
      offset += 1;
    }
  }

  if (offset >= lineText.length || lineText[offset] === '#') {
    return null;
  }

  const colonOffset = lineText.indexOf(':', offset);
  if (colonOffset < 0) {
    return null;
  }

  let keyEndOffset = colonOffset;
  while (keyEndOffset > offset && (lineText[keyEndOffset - 1] === ' ' || lineText[keyEndOffset - 1] === '\t')) {
    keyEndOffset -= 1;
  }
  if (keyEndOffset <= offset) {
    return null;
  }

  let valueStartOffset = colonOffset + 1;
  while (
    valueStartOffset < lineText.length &&
    (lineText[valueStartOffset] === ' ' || lineText[valueStartOffset] === '\t')
  ) {
    valueStartOffset += 1;
  }

  return {
    keyFromOffset: offset,
    keyToOffset: colonOffset + 1,
    valueFromOffset: valueStartOffset < lineText.length ? valueStartOffset : null,
  };
}

export function parseSimpleYamlFlowArrayValue(lineText: string, valueFromOffset: number | null): YamlArrayValue | null {
  if (
    valueFromOffset === null ||
    valueFromOffset < 0 ||
    valueFromOffset >= lineText.length ||
    lineText[valueFromOffset] !== '['
  ) {
    return null;
  }

  let arrayToOffset = lineText.length;
  while (
    arrayToOffset > valueFromOffset &&
    (lineText[arrayToOffset - 1] === ' ' || lineText[arrayToOffset - 1] === '\t')
  ) {
    arrayToOffset -= 1;
  }

  if (arrayToOffset <= valueFromOffset + 1 || lineText[arrayToOffset - 1] !== ']') {
    return null;
  }

  const innerFromOffset = valueFromOffset + 1;
  const innerToOffset = arrayToOffset - 1;
  if (innerToOffset <= innerFromOffset) {
    return null;
  }

  for (let i = innerFromOffset; i < innerToOffset; i += 1) {
    const ch = lineText[i];
    if (ch === '"' || ch === "'" || ch === '[' || ch === ']' || ch === '{' || ch === '}') {
      return null;
    }
  }

  const items: Array<{ fromOffset: number; toOffset: number; text: string }> = [];
  let partFromOffset = innerFromOffset;
  for (let i = innerFromOffset; i <= innerToOffset; i += 1) {
    const atEnd = i === innerToOffset;
    if (!atEnd && lineText[i] !== ',') {
      continue;
    }

    let itemFromOffset = partFromOffset;
    let itemToOffset = i;
    while (itemFromOffset < itemToOffset && (lineText[itemFromOffset] === ' ' || lineText[itemFromOffset] === '\t')) {
      itemFromOffset += 1;
    }
    while (
      itemToOffset > itemFromOffset &&
      (lineText[itemToOffset - 1] === ' ' || lineText[itemToOffset - 1] === '\t')
    ) {
      itemToOffset -= 1;
    }

    if (itemFromOffset >= itemToOffset) {
      return null;
    }

    items.push({
      fromOffset: itemFromOffset,
      toOffset: itemToOffset,
      text: lineText.slice(itemFromOffset, itemToOffset),
    });

    partFromOffset = i + 1;
  }

  if (!items.length) {
    return null;
  }

  return {
    fromOffset: valueFromOffset,
    toOffset: arrayToOffset,
    items,
  };
}

function frontmatterContentLineRange(
  state: EditorState,
  frontmatter: FrontmatterInfo | null
): { startLineNo: number; endLineNo: number } | null {
  if (!frontmatter || frontmatter.contentTo <= frontmatter.contentFrom) {
    return null;
  }

  return {
    startLineNo: state.doc.lineAt(frontmatter.contentFrom).number,
    endLineNo: state.doc.lineAt(frontmatter.contentTo - 1).number,
  };
}

export function forEachFrontmatterContentLine(
  state: EditorState,
  frontmatter: FrontmatterInfo | null,
  callback: (line: LineInfo) => void
): void {
  const range = frontmatterContentLineRange(state, frontmatter);
  if (!range) {
    return;
  }

  for (let lineNo = range.startLineNo; lineNo <= range.endLineNo; lineNo += 1) {
    callback(state.doc.line(lineNo));
  }
}

export function forEachYamlFrontmatterField(
  state: EditorState,
  frontmatter: FrontmatterInfo | null,
  callback: (range: YamlFieldRange) => void
): void {
  forEachFrontmatterContentLine(state, frontmatter, (line) => {
    const offsets = yamlFrontmatterFieldOffsets(line.text);
    if (!offsets) {
      return;
    }

    callback({
      line,
      keyFrom: line.from + offsets.keyFromOffset,
      keyTo: line.from + offsets.keyToOffset,
      valueFrom: offsets.valueFromOffset === null ? null : line.from + offsets.valueFromOffset,
      valueTo: line.to,
    });
  });
}

function buildSourceFrontmatterDecorations(state: EditorState): any {
  const builder = new RangeSetBuilder<any>();
  const frontmatter = parseFrontmatter(state);
  if (!frontmatter) {
    return builder.finish();
  }

  const openingLine = state.doc.lineAt(frontmatter.openingFrom);
  builder.add(openingLine.from, openingLine.from, sourceFrontmatterDelimiterLineDeco);

  forEachFrontmatterContentLine(state, frontmatter, (line) => {
    builder.add(line.from, line.from, sourceFrontmatterContentLineDeco);

    const offsets = yamlFrontmatterFieldOffsets(line.text);
    if (!offsets) {
      return;
    }

    builder.add(line.from + offsets.keyFromOffset, line.from + offsets.keyToOffset, sourceFrontmatterKeyDeco);

    if (offsets.valueFromOffset !== null) {
      const valueFrom = line.from + offsets.valueFromOffset;
      if (valueFrom < line.to) {
        builder.add(valueFrom, line.to, sourceFrontmatterValueDeco);
      }
    }
  });

  const closingLine = state.doc.lineAt(frontmatter.closingFrom);
  builder.add(closingLine.from, closingLine.from, sourceFrontmatterDelimiterLineDeco);

  return builder.finish();
}
