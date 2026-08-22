// @ts-nocheck
import { StateField, RangeSetBuilder, EditorState, Transaction } from '@codemirror/state';
import { Decoration, WidgetType, EditorView } from '@codemirror/view';
import { parseFrontmatter, isInsideFrontmatterContent } from './frontmatter';

interface ListMarkerData {
  fromOffset: number;
  leadingWhitespace: string;
  indentLevel: number;
  indentColumns: number;
  markerEndOffset: number;
  toOffset: number;
  contentOffsetColumns: number;
  markerText: string;
  classes: string;
  orderedNumber: string;
  isTask: boolean;
  taskHiddenPrefixColumns: number;
  taskBracketStart?: number;
  taskStatus?: TaskStatus;
}

type TaskStatus = 'todo' | 'inprogress' | 'done' | 'dropped';

interface ListIndentStyle {
  columns: number;
  insert: string;
}

const sourceListMarkerDeco = Decoration.mark({ class: 'meo-md-list-prefix' });
const sourceTaskBracketDeco = Decoration.mark({ class: 'meo-md-task-bracket' });
const taskCompleteDeco = Decoration.mark({ class: 'meo-task-complete' });
const taskDroppedDeco = Decoration.mark({ class: 'meo-task-dropped' });
const listItemRegex = /^(\s*)(?:[-+*]|\d+[.)])(?:\s+|$)/;
const listMarkerRegex = /^(\s*)(?:([-+*])|(\d+)([.)]))(?:\s+(?:\[([ xX~\-])\]\s+)?|$)/;
const TWO_SPACE_INDENT = '  ';
const FOUR_SPACE_INDENT = '    ';
const TAB_INDENT = '\t';
const TWO_SPACE_INDENT_COLUMNS = 2;
const FOUR_SPACE_INDENT_COLUMNS = 4;

const listIndentStyle = {
  twoSpaces: {
    columns: TWO_SPACE_INDENT_COLUMNS,
    insert: TWO_SPACE_INDENT
  },
  fourSpaces: {
    columns: FOUR_SPACE_INDENT_COLUMNS,
    insert: FOUR_SPACE_INDENT
  },
  tabs: {
    columns: FOUR_SPACE_INDENT_COLUMNS,
    insert: TAB_INDENT
  }
};

const defaultListIndentStyle = listIndentStyle.twoSpaces;
const listBorderDecoCache = new Map();
const taskStatusByMarker: Record<string, TaskStatus> = {
  x: 'done',
  '~': 'inprogress',
  '-': 'dropped',
  ' ': 'todo'
};
const taskStatusClassByStatus: Record<TaskStatus, string> = {
  todo: 'is-todo',
  inprogress: 'is-inprogress',
  done: 'is-done',
  dropped: 'is-dropped'
};

function listBorderDecoration(widthColumns = 1) {
  const width = Math.max(1, widthColumns);
  let deco = listBorderDecoCache.get(width);
  if (deco) {
    return deco;
  }

  deco = Decoration.mark({
    class: 'meo-md-list-border',
    attributes: { style: `--meo-list-border-width:${width}ch;` }
  });
  listBorderDecoCache.set(width, deco);
  return deco;
}

function forEachSelectionLine(state, callback) {
  const seen = new Set();
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
}

function lineIsInFrontmatterContent(frontmatter, line) {
  return isInsideFrontmatterContent(frontmatter, line.from);
}

function isListLine(lineText) {
  return listItemRegex.test(lineText);
}

function inferListIndentStyle(listLineTexts) {
  const spaceIndents = [];
  for (const lineText of listLineTexts) {
    const match = listItemRegex.exec(lineText);
    if (!match || !match[1]) {
      continue;
    }

    const leadingWhitespace = match[1];
    if (leadingWhitespace.includes('\t')) {
      return listIndentStyle.tabs;
    }

    if (/^ +$/.test(leadingWhitespace)) {
      spaceIndents.push(leadingWhitespace.length);
    }
  }

  if (!spaceIndents.length) {
    return defaultListIndentStyle;
  }

  const isFourSpaceList = spaceIndents.every((length) => length % FOUR_SPACE_INDENT_COLUMNS === 0);
  return isFourSpaceList ? listIndentStyle.fourSpaces : defaultListIndentStyle;
}

export function detectListIndentStylesByLine(state) {
  const stylesByLine = new Map();
  let lineNo = 1;

  while (lineNo <= state.doc.lines) {
    const startLineNo = lineNo;
    const listLineTexts = [];

    while (lineNo <= state.doc.lines) {
      const line = state.doc.line(lineNo);
      const lineText = state.doc.sliceString(line.from, line.to);
      if (!isListLine(lineText)) {
        break;
      }
      listLineTexts.push(lineText);
      lineNo += 1;
    }

    if (!listLineTexts.length) {
      lineNo += 1;
      continue;
    }

    const style = inferListIndentStyle(listLineTexts);
    for (let currentLine = startLineNo; currentLine < lineNo; currentLine += 1) {
      stylesByLine.set(currentLine, style);
    }
  }

  return stylesByLine;
}

function lineIndentStyle(lineNumber, stylesByLine) {
  return stylesByLine?.get(lineNumber) ?? defaultListIndentStyle;
}

export function nextOrderedSequenceNumber(
  orderedCountsByLevel: Array<number | null>,
  level: number,
  orderedNumber: string | null | undefined
): { expected: number | null; isAnchor: boolean } {
  orderedCountsByLevel.length = level + 1;
  if (!orderedNumber) {
    orderedCountsByLevel[level] = null;
    return { expected: null, isAnchor: false };
  }

  const current = orderedCountsByLevel[level];
  if (current === null || current === undefined) {
    const parsed = Number.parseInt(orderedNumber, 10);
    orderedCountsByLevel[level] = parsed;
    return { expected: parsed, isAnchor: true };
  }

  const next = current + 1;
  orderedCountsByLevel[level] = next;
  return { expected: next, isAnchor: false };
}

function indentationColumns(leadingWhitespace, style = defaultListIndentStyle) {
  let columns = 0;
  for (let index = 0; index < leadingWhitespace.length; index += 1) {
    columns += leadingWhitespace[index] === '\t' ? style.columns : 1;
  }
  return columns;
}

function listBorderOffsets(leadingWhitespace, style = defaultListIndentStyle) {
  const offsets = [];
  let columns = 0;
  let levelStart = 0;

  for (let index = 0; index < leadingWhitespace.length; index += 1) {
    if (columns === 0) {
      levelStart = index;
    }
    columns += leadingWhitespace[index] === '\t' ? style.columns : 1;
    if (columns >= style.columns) {
      offsets.push(levelStart);
      columns = 0;
    }
  }

  return offsets;
}

function addListIndentBorders(addRange, lineStart, leadingWhitespace, style = defaultListIndentStyle) {
  for (const offset of listBorderOffsets(leadingWhitespace, style)) {
    const char = leadingWhitespace[offset];
    const borderWidth = char === '\t' ? style.columns : 1;
    addRange(lineStart + offset, lineStart + offset + 1, listBorderDecoration(borderWidth));
  }
}

function listIndentDeleteLength(leadingWhitespace, style = defaultListIndentStyle) {
  if (!leadingWhitespace) {
    return 0;
  }
  return leadingWhitespace.startsWith('\t')
    ? 1
    : Math.min(style.columns, leadingWhitespace.match(/^ +/)?.[0]?.length ?? 0);
}

export function indentListByTwoSpaces(view) {
  const { state } = view;
  const stylesByLine = detectListIndentStylesByLine(state);
  const changes = [];

  forEachSelectionLine(state, (line) => {
    const lineText = state.doc.sliceString(line.from, line.to);
    if (!isListLine(lineText)) {
      return;
    }
    const style = lineIndentStyle(line.number, stylesByLine);
    changes.push({ from: line.from, insert: style.insert });
  });

  if (!changes.length) {
    return false;
  }

  view.dispatch({ changes });
  return true;
}

export function outdentListByTwoSpaces(view) {
  const { state } = view;
  const stylesByLine = detectListIndentStylesByLine(state);
  const changes = [];

  forEachSelectionLine(state, (line) => {
    const lineText = state.doc.sliceString(line.from, line.to);
    const listMatch = listItemRegex.exec(lineText);
    if (!listMatch || !listMatch[1].length) {
      return;
    }

    const leadingWhitespace = listMatch[1];
    const style = lineIndentStyle(line.number, stylesByLine);
    const deleteLength = listIndentDeleteLength(leadingWhitespace, style);
    if (!deleteLength) {
      return;
    }

    changes.push({ from: line.from, to: line.from + deleteLength, insert: '' });
  });

  if (!changes.length) {
    return false;
  }

  view.dispatch({ changes });
  return true;
}

function collectNestedListHoistChanges(state, parentLine, parentMarker, stylesByLine) {
  if (parentLine.number >= state.doc.lines) {
    return [];
  }

  const parentIndentColumns = parentMarker.indentColumns ?? 0;
  const changes = [];
  let foundNestedDescendants = false;

  for (let lineNo = parentLine.number + 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    const lineText = state.doc.sliceString(line.from, line.to);

    if (!lineText.trim()) {
      continue;
    }

    const style = lineIndentStyle(lineNo, stylesByLine);
    const marker = listMarkerData(lineText, null, style);
    if (!marker) {
      if (!foundNestedDescendants) {
        return [];
      }
      break;
    }

    const indentColumns = marker.indentColumns ?? 0;
    if (indentColumns <= parentIndentColumns) {
      if (!foundNestedDescendants) {
        return [];
      }
      break;
    }

    foundNestedDescendants = true;
    const deleteLength = listIndentDeleteLength(marker.leadingWhitespace, style);
    if (!deleteLength) {
      continue;
    }

    changes.push({ from: line.from, to: line.from + deleteLength, insert: '' });
  }

  return changes;
}

function parseListMarkerParts(lineText) {
  const match = listMarkerRegex.exec(lineText);
  if (!match) {
    return null;
  }

  return {
    leadingWhitespace: match[1],
    bullet: match[2],
    orderedNumber: match[3],
    orderedSuffix: match[4],
    hasTask: match[5] !== undefined
  };
}

function taskStatusFromMarker(markerChar: string): TaskStatus {
  return taskStatusByMarker[markerChar.toLowerCase()] ?? 'todo';
}

function taskStatusCssClass(status: TaskStatus): string {
  return taskStatusClassByStatus[status];
}

function buildListMarkerText(parts, orderedNumber = parts?.orderedNumber) {
  if (!parts) {
    return null;
  }

  if (parts.bullet) {
    return parts.hasTask
      ? `${parts.leadingWhitespace}${parts.bullet} [ ] `
      : `${parts.leadingWhitespace}${parts.bullet} `;
  }

  if (!orderedNumber || !parts.orderedSuffix) {
    return null;
  }

  return parts.hasTask
    ? `${parts.leadingWhitespace}${orderedNumber}${parts.orderedSuffix} [ ] `
    : `${parts.leadingWhitespace}${orderedNumber}${parts.orderedSuffix} `;
}

export function listMarkerData(lineText: string, orderedDisplayIndex: string | null = null, style: ListIndentStyle = defaultListIndentStyle): ListMarkerData | null {
  const match = listMarkerRegex.exec(lineText);
  if (!match) {
    return null;
  }

  const indent = match[1].length;
  const leadingWhitespace = match[1];
  const orderedNumber = match[3];
  const orderedSuffix = match[4];
  const taskMarker = match[5];

  let markerText = '•';
  let classes = 'meo-md-list-marker-bullet';

  if (orderedNumber && orderedSuffix) {
    markerText = `${orderedDisplayIndex ?? orderedNumber}${orderedSuffix}`;
    classes = 'meo-md-list-marker-ordered';
  }

  const markerCharLength = match[2]?.length ?? (orderedNumber?.length ?? 0) + (orderedSuffix?.length ?? 0);
  const markerEndOffset = indent + markerCharLength;
  const indentColumns = indentationColumns(leadingWhitespace, style);
  const contentOffsetColumns = indentColumns + (match[0].length - indent);

  const result: ListMarkerData = {
    fromOffset: indent,
    leadingWhitespace,
    indentLevel: Math.floor(indentColumns / style.columns),
    indentColumns,
    markerEndOffset,
    toOffset: match[0].length,
    contentOffsetColumns,
    markerText,
    classes,
    orderedNumber,
    isTask: false,
    taskHiddenPrefixColumns: 0
  };

  if (taskMarker !== undefined) {
    const hiddenTaskPrefixLength = Math.max(0, (match[0].length - indent) - 1);
    result.taskBracketStart = markerEndOffset + 1;
    result.taskStatus = taskStatusFromMarker(taskMarker);
    result.isTask = true;
    result.taskHiddenPrefixColumns = hiddenTaskPrefixLength;
  }

  return result;
}

class ListMarkerWidget extends WidgetType {
  text: string;
  classes: string;

  constructor(text: string, classes: string) {
    super();
    this.text = text;
    this.classes = classes;
  }

  eq(other: WidgetType): boolean {
    return other instanceof ListMarkerWidget && other.text === this.text && other.classes === this.classes;
  }

  toDOM(): HTMLElement {
    const marker = document.createElement('span');
    marker.className = `meo-md-list-marker ${this.classes}`;
    marker.style.color = 'var(--meo-color-base02)';
    marker.textContent = this.text;
    return marker;
  }
}

class CheckboxWidget extends WidgetType {
  status: TaskStatus;
  bracketStart: number;

  constructor(status: TaskStatus, bracketStart: number) {
    super();
    this.status = status;
    this.bracketStart = bracketStart;
  }

  eq(other: WidgetType): boolean {
    return other instanceof CheckboxWidget && other.status === this.status && other.bracketStart === this.bracketStart;
  }

  toDOM(view: EditorView): HTMLElement {
    const isDone = this.status === 'done';
    const checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.className = `meo-task-checkbox ${taskStatusCssClass(this.status)}`;
    checkbox.checked = isDone;
    checkbox.setAttribute('aria-label', isDone ? 'Mark task as incomplete' : 'Mark task as complete');

    checkbox.addEventListener('mousedown', (e) => {
      e.preventDefault();
    });

    checkbox.addEventListener('change', () => {
      const newChar = checkbox.checked ? 'x' : ' ';
      view.dispatch({
        changes: { from: this.bracketStart + 1, to: this.bracketStart + 2, insert: newChar }
      });
    });

    return checkbox;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

export function addListMarkerDecoration(
  builder,
  state,
  from,
  orderedDisplayIndex = null,
  style = defaultListIndentStyle,
  options = null
) {
  const line = state.doc.lineAt(from);
  const lineText = state.doc.sliceString(line.from, line.to);
  const marker = listMarkerData(lineText, orderedDisplayIndex, style);
  if (!marker) {
    return;
  }

  const indentEnd = line.from + marker.fromOffset;
  const markerEnd = line.from + marker.markerEndOffset;
  const markerTo = line.from + marker.toOffset;

  if (options?.useSourceStyleLiteral) {
    if (markerTo > indentEnd) {
      builder.push(sourceListMarkerDeco.range(indentEnd, markerTo));
    }
    return;
  }

  if (marker.taskBracketStart !== undefined) {
    const bracketStart = line.from + marker.taskBracketStart;
    const fullEnd = markerTo - 1;
    const taskStatus = marker.taskStatus ?? 'todo';
    builder.push(
      Decoration.replace({
        widget: new CheckboxWidget(taskStatus, bracketStart),
        inclusive: false
      }).range(indentEnd, fullEnd)
    );

    if (taskStatus === 'done' || taskStatus === 'dropped') {
      const textStart = line.from + marker.toOffset;
      if (textStart < line.to) {
        builder.push((taskStatus === 'done' ? taskCompleteDeco : taskDroppedDeco).range(textStart, line.to));
      }
    }
  } else if (markerEnd > indentEnd) {
    builder.push(
      Decoration.replace({
        widget: new ListMarkerWidget(marker.markerText, marker.classes),
        inclusive: false
      }).range(indentEnd, markerEnd)
    );
  }

}

export function continuedListMarker(lineText) {
  const parts = parseListMarkerParts(lineText);
  if (!parts) {
    return null;
  }
  const marker = listMarkerData(lineText);
  if (!marker) {
    return null;
  }

  const hasContent = lineText.slice(marker.toOffset).trim().length > 0;
  if (!hasContent) {
    return null;
  }

  if (!parts.orderedNumber) {
    return buildListMarkerText(parts);
  }

  const nextNumber = Number.parseInt(parts.orderedNumber, 10) + 1;
  if (!Number.isFinite(nextNumber)) {
    return null;
  }
  return buildListMarkerText(parts, String(nextNumber));
}

function sameLevelListMarker(lineText) {
  return buildListMarkerText(parseListMarkerParts(lineText));
}

export function handleEnterContinueList(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const position = selection.head;
  const line = state.doc.lineAt(position);
  if (position !== line.to) {
    return false;
  }

  const lineText = state.doc.sliceString(line.from, line.to);
  const marker = continuedListMarker(lineText);
  if (!marker) {
    return false;
  }

  const insert = `\n${marker}`;
  view.dispatch({
    changes: { from: position, insert },
    selection: { anchor: position + insert.length }
  });
  return true;
}

export function handleEnterOnEmptyListItem(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const position = selection.head;
  const line = state.doc.lineAt(position);
  const lineText = state.doc.sliceString(line.from, line.to);
  const marker = listMarkerData(lineText);
  if (!marker) {
    return false;
  }

  const contentStart = line.from + marker.toOffset;
  if (position !== contentStart && position !== line.to) {
    return false;
  }

  if (lineText.slice(marker.toOffset).trim().length > 0) {
    return false;
  }

  view.dispatch({
    changes: { from: line.from, to: contentStart, insert: '' },
    selection: { anchor: line.from }
  });
  return true;
}

export function handleBackspaceAtListContentStart(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const line = state.doc.lineAt(selection.head);
  const lineText = state.doc.sliceString(line.from, line.to);
  const stylesByLine = detectListIndentStylesByLine(state);
  const style = lineIndentStyle(line.number, stylesByLine);
  const marker = listMarkerData(lineText, null, style);
  if (!marker) {
    return false;
  }

  const contentStart = line.from + marker.toOffset;
  const markerVisualEnd = line.from + marker.markerEndOffset;
  const content = lineText.slice(marker.toOffset);
  const isAtContentStart = selection.head === contentStart;
  const isAtEmptyItemMarkerEnd =
    !content.trim() &&
    selection.head >= markerVisualEnd &&
    selection.head <= contentStart;

  if (!isAtContentStart && !isAtEmptyItemMarkerEnd) {
    return false;
  }

  const deletingEmptyLine = !content.trim();
  const nextLine = line.number < state.doc.lines ? state.doc.line(line.number + 1) : null;
  const hoistChanges = collectNestedListHoistChanges(state, line, marker, stylesByLine);

  const changes = [
    deletingEmptyLine
      ? {
          from: line.from,
          to: line.number < state.doc.lines ? state.doc.line(line.number + 1).from : line.to,
          insert: ''
        }
      : { from: line.from, to: contentStart, insert: '' },
    ...hoistChanges
  ];

  let selectionAnchor = line.from;
  if (deletingEmptyLine && nextLine) {
    const nextLineText = state.doc.sliceString(nextLine.from, nextLine.to);
    const nextStyle = lineIndentStyle(nextLine.number, stylesByLine);
    const nextMarker = listMarkerData(nextLineText, null, nextStyle);
    if (nextMarker) {
      const nextLineHoist = hoistChanges.find((change) => change.from === nextLine.from);
      const hoistDeleteLength = nextLineHoist ? nextLineHoist.to - nextLineHoist.from : 0;
      selectionAnchor = line.from + Math.max(0, nextMarker.toOffset - hoistDeleteLength);
    }
  }

  view.dispatch({
    changes,
    selection: { anchor: selectionAnchor }
  });
  return true;
}

function collapsedSingleCursorListContext(state) {
  if (state.selection.ranges.length !== 1) {
    return null;
  }

  const selection = state.selection.main;
  if (!selection.empty) {
    return null;
  }

  const line = state.doc.lineAt(selection.head);
  const frontmatter = parseFrontmatter(state);
  if (lineIsInFrontmatterContent(frontmatter, line)) {
    return null;
  }

  const lineText = state.doc.sliceString(line.from, line.to);
  const marker = listMarkerData(lineText);
  if (!marker) {
    return null;
  }

  return {
    selection,
    line,
    marker,
    contentStart: line.from + marker.toOffset
  };
}

export function handleArrowLeftAtListContentStart(view) {
  const context = collapsedSingleCursorListContext(view.state);
  if (!context || context.selection.head !== context.contentStart) {
    return false;
  }

  view.dispatch({ selection: { anchor: context.line.from } });
  return true;
}

export function handleArrowRightAtListLineStart(view) {
  const context = collapsedSingleCursorListContext(view.state);
  if (!context || context.selection.head !== context.line.from) {
    return false;
  }

  view.dispatch({ selection: { anchor: context.contentStart } });
  return true;
}

export function handleEnterAtListContentStart(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const position = selection.head;
  const line = state.doc.lineAt(position);
  const lineText = state.doc.sliceString(line.from, line.to);
  const marker = listMarkerData(lineText);
  if (!marker) {
    return false;
  }

  const contentStart = line.from + marker.toOffset;
  if (position !== contentStart) {
    return false;
  }

  const content = lineText.slice(marker.toOffset).trim();
  if (!content) {
    return false;
  }

  const sameMarker = sameLevelListMarker(lineText);
  if (!sameMarker) {
    return false;
  }

  const insert = `${sameMarker}\n`;
  view.dispatch({
    changes: { from: line.from, insert },
    selection: { anchor: line.from + sameMarker.length }
  });
  return true;
}

export function handleEnterBeforeNestedList(view) {
  const { state } = view;
  const selection = state.selection.main;
  if (!selection.empty) {
    return false;
  }

  const position = selection.head;
  const line = state.doc.lineAt(position);
  if (position !== line.to || line.number >= state.doc.lines) {
    return false;
  }

  const currentText = state.doc.sliceString(line.from, line.to);
  const nextLine = state.doc.line(line.number + 1);
  const nextText = state.doc.sliceString(nextLine.from, nextLine.to);

  if (!/^[ \t]+(?:[-+*]|\d+[.)])\s+/.test(nextText)) {
    return false;
  }

  const marker = continuedListMarker(currentText);
  if (!marker) {
    return false;
  }

  const insert = `\n${marker}`;
  view.dispatch({
    changes: { from: position, insert },
    selection: { anchor: position + insert.length }
  });
  return true;
}

export function collectOrderedListRenumberChanges(state) {
  const changes = [];
  const stylesByLine = detectListIndentStylesByLine(state);
  const orderedCountsByLevel: Array<number | null> = [];

  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    const lineText = state.doc.sliceString(line.from, line.to);
    const style = lineIndentStyle(lineNo, stylesByLine);
    const marker = listMarkerData(lineText, null, style);

    if (!marker) {
      orderedCountsByLevel.length = 0;
      continue;
    }

    const level = marker.indentLevel;
    const { expected, isAnchor } = nextOrderedSequenceNumber(
      orderedCountsByLevel,
      level,
      marker.orderedNumber
    );
    if (expected === null || isAnchor) {
      continue;
    }
    const expectedText = String(expected);
    if (marker.orderedNumber !== expectedText) {
      const from = line.from + marker.leadingWhitespace.length;
      changes.push({
        from,
        to: from + marker.orderedNumber.length,
        insert: expectedText
      });
    }
  }

  return changes;
}

function computeSourceListBorders(state) {
  const stylesByLine = detectListIndentStylesByLine(state);
  const frontmatter = parseFrontmatter(state);
  const ranges = new RangeSetBuilder();
  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    if (lineIsInFrontmatterContent(frontmatter, line)) {
      continue;
    }
    const lineText = state.doc.sliceString(line.from, line.to);
    const style = lineIndentStyle(lineNo, stylesByLine);
    const marker = listMarkerData(lineText, null, style);
    if (!marker || marker.fromOffset === 0) {
      continue;
    }
    addListIndentBorders(
      (from, to, deco) => ranges.add(from, to, deco),
      line.from,
      marker.leadingWhitespace,
      style
    );
  }
  return ranges.finish();
}

function computeSourceListMarkers(state) {
  const stylesByLine = detectListIndentStylesByLine(state);
  const ranges = new RangeSetBuilder();
  for (let lineNo = 1; lineNo <= state.doc.lines; lineNo += 1) {
    const line = state.doc.line(lineNo);
    const lineText = state.doc.sliceString(line.from, line.to);
    const style = lineIndentStyle(lineNo, stylesByLine);
    const marker = listMarkerData(lineText, null, style);
    if (!marker) {
      continue;
    }

    const markerFrom = line.from + marker.fromOffset;
    const markerTo = line.from + (marker.isTask ? marker.markerEndOffset : marker.toOffset);
    if (markerTo > markerFrom) {
      ranges.add(markerFrom, markerTo, sourceListMarkerDeco);
    }
    if (marker.taskBracketStart !== undefined) {
      const openBracket = line.from + marker.taskBracketStart;
      const closeBracket = openBracket + 2;
      ranges.add(openBracket, openBracket + 1, sourceTaskBracketDeco);
      ranges.add(closeBracket, closeBracket + 1, sourceTaskBracketDeco);
    }
  }
  return ranges.finish();
}

export const sourceListBorderField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return computeSourceListBorders(state);
    } catch {
      return Decoration.none;
    }
  },
  update(borders: any, transaction: Transaction) {
    if (!transaction.docChanged) {
      return borders;
    }
    try {
      return computeSourceListBorders(transaction.state);
    } catch {
      return borders;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field)
});

export const sourceListMarkerField = StateField.define<any>({
  create(state: EditorState) {
    try {
      return computeSourceListMarkers(state);
    } catch {
      return Decoration.none;
    }
  },
  update(markers: any, transaction: Transaction) {
    if (!transaction.docChanged) {
      return markers;
    }
    try {
      return computeSourceListMarkers(transaction.state);
    } catch {
      return markers;
    }
  },
  provide: (field: any) => EditorView.decorations.from(field)
});
