// @ts-nocheck
import { RangeSetBuilder, StateField, EditorState } from '@codemirror/state';
import { Decoration, EditorView, WidgetType } from '@codemirror/view';

const lineDecos = {
  currentHeader: Decoration.line({ class: 'meo-merge-line meo-merge-current-header' }),
  current: Decoration.line({ class: 'meo-merge-line meo-merge-current' }),
  baseHeader: Decoration.line({ class: 'meo-merge-line meo-merge-base-header' }),
  base: Decoration.line({ class: 'meo-merge-line meo-merge-base' }),
  separator: Decoration.line({ class: 'meo-merge-line meo-merge-separator' }),
  incomingHeader: Decoration.line({ class: 'meo-merge-line meo-merge-incoming-header' }),
  incoming: Decoration.line({ class: 'meo-merge-line meo-merge-incoming' }),
};

interface MergeConflict {
  id: number;
  startLineNo: number;
  baseMarkerLineNo: number | null;
  separatorLineNo: number;
  endLineNo: number;
  blockFrom: number;
  blockTo: number;
  currentLabel: string;
  incomingLabel: string;
  currentText: string;
  incomingText: string;
}

interface MergeConflictState {
  conflicts: MergeConflict[];
  decorations: any;
}

function lineText(state: EditorState, lineNo: number): string {
  const line = state.doc.line(lineNo);
  return state.doc.sliceString(line.from, line.to);
}

function lineEndWithBreak(doc: any, lineNo: number): number {
  if (lineNo < doc.lines) {
    return doc.line(lineNo + 1).from;
  }
  return doc.line(lineNo).to;
}

function markerLabel(text: string, marker: string): string {
  return text.slice(marker.length).trim();
}

export function parseMergeConflicts(state: EditorState): MergeConflict[] {
  const conflicts: MergeConflict[] = [];
  const { doc } = state;
  let lineNo = 1;

  while (lineNo <= doc.lines) {
    const startText = lineText(state, lineNo);
    if (!startText.startsWith('<<<<<<<')) {
      lineNo += 1;
      continue;
    }

    const startLineNo = lineNo;
    const currentLabel = markerLabel(startText, '<<<<<<<');
    let baseMarkerLineNo: number | null = null;
    let separatorLineNo: number | null = null;
    let endLineNo: number | null = null;

    let scanLineNo = startLineNo + 1;
    while (scanLineNo <= doc.lines) {
      const text = lineText(state, scanLineNo);
      if (separatorLineNo === null && baseMarkerLineNo === null && text.startsWith('|||||||')) {
        baseMarkerLineNo = scanLineNo;
        scanLineNo += 1;
        continue;
      }
      if (separatorLineNo === null && text.startsWith('=======')) {
        separatorLineNo = scanLineNo;
        scanLineNo += 1;
        break;
      }
      scanLineNo += 1;
    }

    if (separatorLineNo === null) {
      lineNo = startLineNo + 1;
      continue;
    }

    while (scanLineNo <= doc.lines) {
      const text = lineText(state, scanLineNo);
      if (text.startsWith('>>>>>>>')) {
        endLineNo = scanLineNo;
        break;
      }
      scanLineNo += 1;
    }

    if (endLineNo === null) {
      lineNo = startLineNo + 1;
      continue;
    }

    const startLine = doc.line(startLineNo);
    const separatorLine = doc.line(separatorLineNo);
    const endLine = doc.line(endLineNo);
    const baseMarkerLine = baseMarkerLineNo ? doc.line(baseMarkerLineNo) : null;

    const currentStart = lineEndWithBreak(doc, startLineNo);
    const currentEnd = baseMarkerLine ? baseMarkerLine.from : separatorLine.from;
    const baseStart = baseMarkerLineNo ? lineEndWithBreak(doc, baseMarkerLineNo) : null;
    const baseEnd = baseMarkerLineNo ? separatorLine.from : null;
    const incomingStart = lineEndWithBreak(doc, separatorLineNo);
    const incomingEnd = endLine.from;

    conflicts.push({
      id: conflicts.length,
      startLineNo,
      baseMarkerLineNo,
      separatorLineNo,
      endLineNo,
      blockFrom: startLine.from,
      blockTo: lineEndWithBreak(doc, endLineNo),
      currentLabel,
      incomingLabel: markerLabel(lineText(state, endLineNo), '>>>>>>>'),
      currentText: doc.sliceString(currentStart, currentEnd),
      incomingText: doc.sliceString(incomingStart, incomingEnd),
    });

    lineNo = endLineNo + 1;
  }

  return conflicts;
}

class MergeConflictActionsWidget extends WidgetType {
  conflictId: number;
  currentLabel: string;
  incomingLabel: string;

  constructor(conflictId: number, currentLabel: string, incomingLabel: string) {
    super();
    this.conflictId = conflictId;
    this.currentLabel = currentLabel;
    this.incomingLabel = incomingLabel;
  }

  eq(other: MergeConflictActionsWidget): boolean {
    return (
      other instanceof MergeConflictActionsWidget &&
      other.conflictId === this.conflictId &&
      other.currentLabel === this.currentLabel &&
      other.incomingLabel === this.incomingLabel
    );
  }

  toDOM(): HTMLElement {
    const wrap = document.createElement('div');
    wrap.className = 'meo-merge-actions';
    wrap.setAttribute('contenteditable', 'false');

    const buttons = [
      { action: 'current', label: 'Accept Current' },
      { action: 'incoming', label: 'Accept Incoming' },
      { action: 'both', label: 'Accept Both' },
    ];

    for (const item of buttons) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'meo-merge-action-btn';
      button.dataset.conflictId = `${this.conflictId}`;
      button.dataset.action = item.action;
      button.textContent = item.label;
      wrap.appendChild(button);
    }

    if (this.currentLabel || this.incomingLabel) {
      const labels = document.createElement('span');
      labels.className = 'meo-merge-action-labels';
      const parts: string[] = [];
      if (this.currentLabel) {
        parts.push(`Current: ${this.currentLabel},`);
      }
      if (this.incomingLabel) {
        parts.push(`Incoming: ${this.incomingLabel}`);
      }
      labels.textContent = parts.join('  ');
      wrap.appendChild(labels);
    }

    return wrap;
  }

  ignoreEvent(): boolean {
    return false;
  }
}

function addLineDeco(builder: RangeSetBuilder<any>, state: EditorState, lineNo: number, deco: any) {
  const line = state.doc.line(lineNo);
  builder.add(line.from, line.from, deco);
}

function buildConflictDecorations(state: EditorState, conflicts: MergeConflict[]): any {
  const builder = new RangeSetBuilder<any>();

  for (const conflict of conflicts) {
    const startLine = state.doc.line(conflict.startLineNo);
    builder.add(
      startLine.from,
      startLine.from,
      Decoration.widget({
        widget: new MergeConflictActionsWidget(conflict.id, conflict.currentLabel, conflict.incomingLabel),
        side: -1,
        block: true,
      })
    );

    addLineDeco(builder, state, conflict.startLineNo, lineDecos.currentHeader);

    const currentContentEndLine = (conflict.baseMarkerLineNo ?? conflict.separatorLineNo) - 1;
    for (let lineNo = conflict.startLineNo + 1; lineNo <= currentContentEndLine; lineNo += 1) {
      addLineDeco(builder, state, lineNo, lineDecos.current);
    }

    if (conflict.baseMarkerLineNo !== null) {
      addLineDeco(builder, state, conflict.baseMarkerLineNo, lineDecos.baseHeader);
      for (let lineNo = conflict.baseMarkerLineNo + 1; lineNo <= conflict.separatorLineNo - 1; lineNo += 1) {
        addLineDeco(builder, state, lineNo, lineDecos.base);
      }
    }

    addLineDeco(builder, state, conflict.separatorLineNo, lineDecos.separator);
    for (let lineNo = conflict.separatorLineNo + 1; lineNo <= conflict.endLineNo - 1; lineNo += 1) {
      addLineDeco(builder, state, lineNo, lineDecos.incoming);
    }
    addLineDeco(builder, state, conflict.endLineNo, lineDecos.incomingHeader);
  }

  return builder.finish();
}

function buildMergeConflictState(state: EditorState): MergeConflictState {
  const conflicts = parseMergeConflicts(state);
  return {
    conflicts,
    decorations: buildConflictDecorations(state, conflicts),
  };
}

const mergeConflictField = StateField.define<MergeConflictState>({
  create(state: EditorState) {
    return buildMergeConflictState(state);
  },
  update(value: MergeConflictState, tr: any): MergeConflictState {
    if (!tr.docChanged) {
      return value;
    }
    return buildMergeConflictState(tr.state);
  },
  provide(field: any) {
    return EditorView.decorations.from(field, (value: MergeConflictState) => value.decorations);
  },
});

function detectDocEol(state: EditorState): string {
  const text = state.doc.toString();
  return text.includes('\r\n') ? '\r\n' : '\n';
}

function joinConflictBoth(state: EditorState, currentText: string, incomingText: string): string {
  if (!currentText || !incomingText) {
    return currentText + incomingText;
  }
  if (/\r?\n$/.test(currentText) || /^[\r\n]/.test(incomingText)) {
    return currentText + incomingText;
  }
  return `${currentText}${detectDocEol(state)}${incomingText}`;
}

function applyConflictResolution(view: EditorView, conflict: MergeConflict, action: string): boolean {
  let insert = '';
  if (action === 'current') {
    insert = conflict.currentText;
  } else if (action === 'incoming') {
    insert = conflict.incomingText;
  } else if (action === 'both') {
    insert = joinConflictBoth(view.state, conflict.currentText, conflict.incomingText);
  } else {
    return false;
  }

  view.dispatch({
    changes: {
      from: conflict.blockFrom,
      to: conflict.blockTo,
      insert,
    },
    selection: {
      anchor: conflict.blockFrom,
    },
  });
  return true;
}

const mergeConflictDomHandlers = EditorView.domEventHandlers({
  mousedown(event: MouseEvent, view: EditorView) {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest('.meo-merge-action-btn');
    if (!(button instanceof HTMLButtonElement)) {
      return false;
    }

    const conflictId = Number.parseInt(button.dataset.conflictId ?? '', 10);
    const action = button.dataset.action ?? '';
    const stateValue = view.state.field(mergeConflictField, false);
    const conflict = stateValue?.conflicts?.[conflictId];
    if (!conflict) {
      return true;
    }

    event.preventDefault();
    event.stopPropagation();
    const applied = applyConflictResolution(view, conflict, action);
    if (applied) {
      view.focus();
    }
    return true;
  },
});

export function mergeConflictSourceExtensions(): any[] {
  return [mergeConflictField, mergeConflictDomHandlers];
}
