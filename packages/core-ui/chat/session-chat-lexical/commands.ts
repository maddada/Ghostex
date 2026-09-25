import type { SessionChatComposerInputApi } from '../session-chat-input-api';
import { sessionChatComposerReferences } from '../session-chat-reference-pills';

export interface ComposerEditorControls extends SessionChatComposerInputApi {
  setSelection: (anchor: number, focus?: number) => void;
  subscribe: (listener: () => void) => () => void;
}

export interface ComposerEditorCommand {
  id: string;
  label: string;
  run: (editor: ComposerEditorControls) => void;
}

function selectedLines(editor: ComposerEditorControls): { text: string; start: number; end: number } {
  const value = editor.getValue();
  const selection = editor.getSelection();
  const start = selection.start === 0 ? 0 : value.lastIndexOf('\n', selection.start - 1) + 1;
  const lastOffset =
    selection.end > selection.start && value[selection.end - 1] === '\n' ? selection.end - 1 : selection.end;
  const nextLine = value.indexOf('\n', lastOffset);
  const end = nextLine < 0 ? value.length : nextLine;
  return { text: value.slice(start, end), start, end };
}

function transformLines(editor: ComposerEditorControls, transform: (text: string) => string): void {
  const { text, start, end } = selectedLines(editor);
  const next = transform(text);
  editor.setSelection(start, end);
  editor.insertText(next);
  editor.setSelection(start, start + next.length);
}

function indentLines(editor: ComposerEditorControls, outdent: boolean): void {
  const selection = editor.getSelection();
  const { text, start, end } = selectedLines(editor);
  const edits: { start: number; removed: number; added: number }[] = [];
  let offset = start;
  const next = text
    .split('\n')
    .map((line) => {
      const removed = outdent ? (line.match(/^(?: {1,4}|\t)/)?.[0].length ?? 0) : 0;
      edits.push({ start: offset, removed, added: outdent ? 0 : 4 });
      offset += line.length + 1;
      return outdent ? line.slice(removed) : `    ${line}`;
    })
    .join('\n');
  const mapOffset = (position: number): number =>
    position +
    edits.reduce(
      (delta, edit) => delta + (position < edit.start ? 0 : edit.added - Math.min(edit.removed, position - edit.start)),
      0
    );
  editor.setSelection(start, end);
  editor.insertText(next);
  editor.setSelection(mapOffset(selection.start), mapOffset(selection.end));
}

function transformSelection(editor: ComposerEditorControls, transform: (text: string) => string): void {
  let { start, end } = editor.getSelection();
  const value = editor.getValue();
  if (start === end) {
    start -= value.slice(0, start).match(/[\p{L}\p{N}_]+$/u)?.[0].length ?? 0;
    end += value.slice(end).match(/^[\p{L}\p{N}_]+/u)?.[0].length ?? 0;
    editor.setSelection(start, end);
  }
  const text = value.slice(start, end);
  let offset = 0;
  let next = '';
  for (const reference of sessionChatComposerReferences(text)) {
    next += transform(text.slice(offset, reference.start)) + text.slice(reference.start, reference.end);
    offset = reference.end;
  }
  next += transform(text.slice(offset));
  editor.insertText(next);
  editor.setSelection(start, start + next.length);
}

export function moveComposerLines(editor: ComposerEditorControls, down: boolean, copy = false): void {
  const value = editor.getValue();
  const selection = editor.getSelection();
  const { text, start, end } = selectedLines(editor);
  if (copy) {
    editor.setSelection(down ? end : start);
    editor.insertText(down ? `\n${text}` : `${text}\n`);
    const delta = down ? end + 1 - start : 0;
    editor.setSelection(selection.start + delta, selection.end + delta);
  } else if (down && end < value.length) {
    const nextEnd = value.indexOf('\n', end + 1);
    const boundary = nextEnd < 0 ? value.length : nextEnd;
    const following = value.slice(end + 1, boundary);
    editor.setSelection(start, boundary);
    editor.insertText(`${following}\n${text}`);
    editor.setSelection(selection.start + following.length + 1, selection.end + following.length + 1);
  } else if (!down && start > 0) {
    const previousStart = start < 2 ? 0 : value.lastIndexOf('\n', start - 2) + 1;
    const previous = value.slice(previousStart, start - 1);
    editor.setSelection(previousStart, end);
    editor.insertText(`${text}\n${previous}`);
    editor.setSelection(selection.start + previousStart - start, selection.end + previousStart - start);
  }
}

export const COMPOSER_EDITOR_COMMANDS: readonly ComposerEditorCommand[] = [
  { id: 'undo', label: 'Undo', run: (editor) => editor.editText('undo') },
  { id: 'redo', label: 'Redo', run: (editor) => editor.editText('redo') },
  { id: 'selectAll', label: 'Select All', run: (editor) => editor.selectAll() },
  {
    id: 'expandLineSelection',
    label: 'Expand Line Selection',
    run: (editor) => {
      const { start, end } = selectedLines(editor);
      const value = editor.getValue();
      const selection = editor.getSelection();
      let nextEnd = Math.min(end + 1, value.length);
      if (selection.start === start && selection.end === nextEnd && nextEnd < value.length) {
        const followingEnd = value.indexOf('\n', nextEnd);
        nextEnd = followingEnd < 0 ? value.length : followingEnd + 1;
      }
      editor.setSelection(start, nextEnd);
    },
  },
  {
    id: 'deleteLines',
    label: 'Delete Line',
    run: (editor) => {
      const { start, end } = selectedLines(editor);
      const length = editor.getValue().length;
      editor.setSelection(end === length && start > 0 ? start - 1 : start, Math.min(end + 1, length));
      editor.insertText('');
    },
  },
  { id: 'moveLinesUp', label: 'Move Line Up', run: (editor) => moveComposerLines(editor, false) },
  { id: 'moveLinesDown', label: 'Move Line Down', run: (editor) => moveComposerLines(editor, true) },
  { id: 'copyLinesUp', label: 'Copy Line Up', run: (editor) => moveComposerLines(editor, false, true) },
  { id: 'copyLinesDown', label: 'Copy Line Down', run: (editor) => moveComposerLines(editor, true, true) },
  {
    id: 'indent',
    label: 'Indent Line',
    run: (editor) => indentLines(editor, false),
  },
  {
    id: 'outdent',
    label: 'Outdent Line',
    run: (editor) => indentLines(editor, true),
  },
  {
    id: 'joinLines',
    label: 'Join Lines',
    run: (editor) => {
      const selection = editor.getSelection();
      const { end } = selectedLines(editor);
      if (selection.start === selection.end && end < editor.getValue().length)
        editor.setSelection(selection.start, end + 2);
      transformLines(editor, (text) => text.replace(/\n\s*/g, ' '));
    },
  },
  {
    id: 'sortAscending',
    label: 'Sort Lines Ascending',
    run: (editor) => transformLines(editor, (text) => text.split('\n').sort().join('\n')),
  },
  {
    id: 'sortDescending',
    label: 'Sort Lines Descending',
    run: (editor) => transformLines(editor, (text) => text.split('\n').sort().reverse().join('\n')),
  },
  {
    id: 'deleteDuplicateLines',
    label: 'Delete Duplicate Lines',
    run: (editor) => transformLines(editor, (text) => [...new Set(text.split('\n'))].join('\n')),
  },
  {
    id: 'trimTrailingWhitespace',
    label: 'Trim Trailing Whitespace',
    run: (editor) => {
      const value = editor.getValue();
      editor.setSelection(0, value.length);
      editor.insertText(value.replace(/[\t ]+$/gm, ''));
    },
  },
  {
    id: 'uppercase',
    label: 'Transform to Uppercase',
    run: (editor) => transformSelection(editor, (text) => text.toUpperCase()),
  },
  {
    id: 'lowercase',
    label: 'Transform to Lowercase',
    run: (editor) => transformSelection(editor, (text) => text.toLowerCase()),
  },
  {
    id: 'titlecase',
    label: 'Transform to Title Case',
    run: (editor) =>
      transformSelection(editor, (text) =>
        text.replace(/\b\w+/g, (word) => word[0]!.toUpperCase() + word.slice(1).toLowerCase())
      ),
  },
];
