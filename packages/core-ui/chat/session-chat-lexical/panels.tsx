import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { COMPOSER_EDITOR_COMMANDS, type ComposerEditorControls } from './commands';

export type ComposerEditorPanel = 'commands' | 'find' | 'replace' | 'line';

export function ComposerEditorPanelView({
  mode,
  editor,
  onClose,
  onModeChange,
  onToggleWrap,
}: {
  mode: ComposerEditorPanel;
  editor: ComposerEditorControls;
  onClose: () => void;
  onModeChange: (mode: ComposerEditorPanel) => void;
  onToggleWrap: () => void;
}) {
  const [query, setQuery] = useState('');
  const [replacement, setReplacement] = useState('');
  const [selected, setSelected] = useState(0);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [regex, setRegex] = useState(false);
  const value = useSyncExternalStore(editor.subscribe, editor.getValue);
  const inputRef = useRef<HTMLInputElement>(null);
  const zeroMatchRef = useRef<{ query: string; start: number } | null>(null);
  useEffect(() => {
    inputRef.current?.focus();
  }, [mode]);
  const extraCommands = [
    { id: 'find', label: 'Find', run: () => onModeChange('find') },
    { id: 'replace', label: 'Replace', run: () => onModeChange('replace') },
    { id: 'line', label: 'Go to Line/Column', run: () => onModeChange('line') },
    {
      id: 'wrap',
      label: 'Toggle Word Wrap',
      run: () => {
        onToggleWrap();
        onClose();
      },
    },
  ];
  const commands = [
    ...extraCommands,
    ...COMPOSER_EDITOR_COMMANDS.map((command) => ({
      ...command,
      run: () => {
        onClose();
        command.run(editor);
      },
    })),
  ].filter((command) => command.label.toLowerCase().includes(query.toLowerCase()));
  let searchError = '';
  const pattern = regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const getMatches = (text: string): { start: number; end: number }[] => {
    if (!query || (mode !== 'find' && mode !== 'replace')) return [];
    try {
      return [...text.matchAll(new RegExp(pattern, caseSensitive ? 'gm' : 'gmi'))].map((match) => ({
        start: match.index!,
        end: match.index! + match[0].length,
      }));
    } catch {
      searchError = 'Invalid regular expression';
      return [];
    }
  };
  const matches = getMatches(value);
  const find = (backward = false): void => {
    const matches = getMatches(editor.getValue());
    if (!matches.length) return;
    const selection = editor.getSelection();
    const match = backward
      ? ([...matches].reverse().find((item) => item.start < selection.start) ?? matches[matches.length - 1]!)
      : (matches.find(
          (item) =>
            item.start >= selection.end &&
            !(
              item.start === item.end &&
              item.start === selection.start &&
              zeroMatchRef.current?.query === query &&
              zeroMatchRef.current.start === item.start
            )
        ) ?? matches[0]!);
    zeroMatchRef.current = match.start === match.end ? { query, start: match.start } : null;
    editor.setSelection(match.start, match.end);
    inputRef.current?.focus();
  };
  const goToLine = (): void => {
    const [line = 1, column = 1] = query.split(':').map(Number);
    if (!Number.isFinite(line) || !Number.isFinite(column)) return;
    const lines = editor.getValue().split('\n');
    const index = Math.max(0, Math.min(lines.length - 1, Math.trunc(line) - 1));
    const offset = lines.slice(0, index).reduce((sum, value) => sum + value.length + 1, 0);
    editor.setSelection(offset + Math.max(0, Math.min(lines[index]!.length, Math.trunc(column) - 1)));
    onClose();
  };
  const replace = (all: boolean): void => {
    const text = editor.getValue();
    const matches = getMatches(text);
    if (!matches.length) return;
    if (all) {
      editor.setSelection(0, text.length);
      const expression = new RegExp(pattern, caseSensitive ? 'gm' : 'gmi');
      editor.insertText(regex ? text.replace(expression, replacement) : text.replace(expression, () => replacement));
    } else {
      const selection = editor.getSelection();
      const match = matches.find((item) => item.start === selection.start && item.end === selection.end);
      if (match) {
        const expression = new RegExp(pattern, caseSensitive ? 'my' : 'myi');
        expression.lastIndex = match.start;
        const next = regex ? text.replace(expression, replacement) : text.replace(expression, () => replacement);
        editor.insertText(next.slice(match.start, next.length - (text.length - match.end)));
      }
      find();
    }
    inputRef.current?.focus();
  };
  return (
    <div
      className='ghostex-chat-editor-panel'
      data-session-chat-typing-redirect-ignore='true'
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Escape') {
          event.preventDefault();
          onClose();
        } else if (event.key === 'Enter') {
          event.preventDefault();
          if (mode === 'commands') commands[Math.min(selected, commands.length - 1)]?.run();
          else if (mode === 'line') goToLine();
          else find(event.shiftKey);
        } else if (mode === 'commands' && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
          event.preventDefault();
          setSelected((value) =>
            Math.max(0, Math.min(commands.length - 1, value + (event.key === 'ArrowDown' ? 1 : -1)))
          );
        }
      }}
    >
      <div className='ghostex-chat-editor-panel-row'>
        <input
          ref={inputRef}
          aria-label={mode === 'commands' ? 'Editor commands' : mode === 'line' ? 'Go to line and column' : 'Find'}
          placeholder={
            mode === 'commands' ? 'Type the name of an action to run' : mode === 'line' ? 'Line:column' : 'Find'
          }
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setSelected(0);
          }}
        />
        <button type='button' aria-label='Close editor panel' onClick={onClose}>
          ×
        </button>
      </div>
      {mode === 'commands' ? (
        <div role='listbox' aria-label='Editor commands' className='ghostex-chat-editor-command-list'>
          {commands.map((command, index) => (
            <button
              key={command.id}
              type='button'
              role='option'
              aria-selected={selected === index}
              ref={(element) => {
                if (selected === index) element?.scrollIntoView({ block: 'nearest' });
              }}
              onMouseDown={(event) => event.preventDefault()}
              onClick={command.run}
            >
              {command.label}
            </button>
          ))}
        </div>
      ) : mode !== 'line' ? (
        <>
          <div className='ghostex-chat-editor-panel-row'>
            <button type='button' aria-pressed={caseSensitive} onClick={() => setCaseSensitive(!caseSensitive)}>
              Match case
            </button>
            <button type='button' aria-pressed={regex} onClick={() => setRegex(!regex)}>
              Regex
            </button>
            <button type='button' aria-label='Previous match' onClick={() => find(true)}>
              ↑
            </button>
            <button type='button' aria-label='Next match' onClick={() => find()}>
              ↓
            </button>
            <span aria-live='polite'>
              {searchError || (matches.length === 1 ? '1 match' : `${matches.length} matches`)}
            </span>
          </div>
          {mode === 'replace' ? (
            <div className='ghostex-chat-editor-panel-row'>
              <input
                aria-label='Replace with'
                placeholder='Replace'
                value={replacement}
                onChange={(event) => setReplacement(event.target.value)}
              />
              <button type='button' onClick={() => replace(false)}>
                Replace
              </button>
              <button type='button' onClick={() => replace(true)}>
                All
              </button>
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
