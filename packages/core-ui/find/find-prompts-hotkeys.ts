/*
CDXC:AgentHistorySearch 2026-08-20:
One key map for the Find surface, shared by every host so `gx f` muscle memory
carries over. It is a pure resolver — no DOM, no state — so the whole map is
unit-testable and gpui/web/mobile cannot drift apart.

Two keys moved from the terminal picker, and the terminal picker moved with
them: `^t` (agents) and `^r` (projects) are unusable in a browser tab, which
reserves Ctrl+T for a new tab and Ctrl+R for reload and does not let a page take
them back. They are `^g` (a-g-ents) and `^j` (pro-j-ect) in both surfaces now.

Ctrl is the modifier everywhere, including macOS, exactly as in the terminal.
*/

export type FindPromptsMode = 'agentPicker' | 'forkPicker' | 'list' | 'preview' | 'projectPicker';

export interface FindPromptsKeyEvent {
  readonly altKey: boolean;
  readonly ctrlKey: boolean;
  readonly key: string;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
}

export type FindPromptsAction =
  | { type: 'cancelOverlay' }
  | { type: 'close' }
  | { type: 'copyPrompt' }
  | { type: 'deleteWordBackward' }
  | { type: 'deleteWordForward' }
  | { type: 'forkPicker' }
  | { type: 'jumpDay'; delta: -1 | 1 }
  | { type: 'killToEnd' }
  | { type: 'killToStart' }
  | { type: 'move'; delta: -1 | 1 }
  | { type: 'openAgentPicker' }
  | { type: 'openProjectPicker' }
  | { type: 'pickIndex'; index: number }
  | { type: 'resumePrompt' }
  | { type: 'scrollPreview'; delta: -1 | 1 }
  | { type: 'toggleDayGrouping' }
  | { type: 'toggleFavorite' }
  | { type: 'toggleFullscreenPreview' }
  | { type: 'togglePickerSelection' }
  | { type: 'togglePreviewFocus' }
  | { type: 'toggleWrap' }
  | { type: 'viewPrompt' };

export type FindPromptsHintAction = Extract<
  FindPromptsAction['type'],
  | 'copyPrompt'
  | 'forkPicker'
  | 'openAgentPicker'
  | 'openProjectPicker'
  | 'toggleDayGrouping'
  | 'toggleFavorite'
  | 'viewPrompt'
>;

/** The interactive shortcut strip above the results. */
export const FIND_PROMPTS_HINTS: readonly {
  action: FindPromptsHintAction;
  key: string;
  label: string;
}[] = [
  { action: 'toggleDayGrouping', key: '^d', label: 'days' },
  { action: 'openAgentPicker', key: '^g', label: 'agents' },
  { action: 'openProjectPicker', key: '^j', label: 'projects' },
  { action: 'toggleFavorite', key: '^f', label: 'fav' },
  { action: 'viewPrompt', key: '^e', label: 'view' },
  { action: 'copyPrompt', key: '^y', label: 'copy' },
  { action: 'forkPicker', key: '^o', label: 'fork' },
];

/** Digits 1-6 select an agent in the fork and agent-filter overlays. */
function digitIndex(key: string): number | null {
  if (key.length !== 1 || key < '1' || key > '6') {
    return null;
  }
  return Number.parseInt(key, 10) - 1;
}

function isPlainKey(event: FindPromptsKeyEvent): boolean {
  return !event.ctrlKey && !event.metaKey && !event.altKey;
}

export function resolveFindPromptsAction(event: FindPromptsKeyEvent, mode: FindPromptsMode): FindPromptsAction | null {
  if (mode === 'forkPicker') {
    // Any key leaves fork mode; a digit also picks the target agent. Matches
    // the terminal picker, where fork mode is a single keystroke.
    const index = digitIndex(event.key);
    if (index !== null && isPlainKey(event)) {
      return { index, type: 'pickIndex' };
    }
    return { type: 'cancelOverlay' };
  }

  if (mode === 'agentPicker' || mode === 'projectPicker') {
    if (event.key === 'Escape') {
      return { type: 'cancelOverlay' };
    }
    if (event.key === 'Enter') {
      return { type: 'togglePickerSelection' };
    }
    if (event.key === ' ' && isPlainKey(event)) {
      return { type: 'togglePickerSelection' };
    }
    if (event.key === 'ArrowDown' || (event.ctrlKey && event.key === 'n')) {
      return { delta: 1, type: 'move' };
    }
    if (event.key === 'ArrowUp' || (event.ctrlKey && event.key === 'p')) {
      return { delta: -1, type: 'move' };
    }
    if (mode === 'agentPicker') {
      const index = digitIndex(event.key);
      if (index !== null && isPlainKey(event)) {
        return { index, type: 'pickIndex' };
      }
    }
    return null;
  }

  const previewFocused = mode === 'preview';

  if (event.key === 'Escape') {
    return { type: 'close' };
  }
  if (event.ctrlKey && event.key === 'c') {
    return { type: 'close' };
  }
  if (event.key === 'Enter' && !event.shiftKey) {
    return { type: 'resumePrompt' };
  }
  if (event.key === 'Tab') {
    return { type: 'togglePreviewFocus' };
  }

  if (event.ctrlKey && !event.altKey) {
    switch (event.key) {
      case 'd':
        return { type: 'toggleDayGrouping' };
      case 'g':
        return { type: 'openAgentPicker' };
      case 'j':
        return { type: 'openProjectPicker' };
      case 'f':
        // `^f` favorites a result, and toggles the big preview while the
        // preview owns focus — the terminal picker's exact overload.
        return previewFocused ? { type: 'toggleFullscreenPreview' } : { type: 'toggleFavorite' };
      case 'e':
        return { type: 'viewPrompt' };
      case 'y':
        return { type: 'copyPrompt' };
      case 'o':
        return { type: 'forkPicker' };
      case 'k':
        return { type: 'killToEnd' };
      case 'u':
        return { type: 'killToStart' };
      case 'n':
        return { delta: 1, type: 'move' };
      case 'p':
        return { delta: -1, type: 'move' };
      case 'Backspace':
        return { type: 'deleteWordBackward' };
      case 'Delete':
        return { type: 'deleteWordForward' };
      case 'ArrowUp':
        return { delta: -1, type: 'jumpDay' };
      case 'ArrowDown':
        return { delta: 1, type: 'jumpDay' };
      default:
        break;
    }
  }

  if (event.key === 'ArrowDown' && isPlainKey(event)) {
    return { delta: 1, type: 'move' };
  }
  if (event.key === 'ArrowUp' && isPlainKey(event)) {
    return { delta: -1, type: 'move' };
  }
  if (event.key === 'PageDown') {
    return previewFocused ? { delta: 1, type: 'scrollPreview' } : { delta: 1, type: 'jumpDay' };
  }
  if (event.key === 'PageUp') {
    return previewFocused ? { delta: -1, type: 'scrollPreview' } : { delta: -1, type: 'jumpDay' };
  }

  // `W` and `F` are preview controls only. In the list they are ordinary
  // characters and must reach the query input untouched.
  if (previewFocused && isPlainKey(event)) {
    if (event.key === 'w' || event.key === 'W') {
      return { type: 'toggleWrap' };
    }
    if (event.key === 'f' || event.key === 'F') {
      return { type: 'toggleFullscreenPreview' };
    }
  }

  return null;
}
