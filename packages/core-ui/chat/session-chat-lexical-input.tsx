import { shortcutKeyFromKeyboardEvent } from '@/packages/shared/keyboard-shortcut-key';
import { useLayoutEffect, useRef, useState } from 'react';
import {
  $createRangeSelectionFromDom,
  $getRoot,
  $getSelection,
  $isRangeSelection,
  $setSelection,
  CLEAR_HISTORY_COMMAND,
  createEditor,
  HISTORY_PUSH_TAG,
  REDO_COMMAND,
  SKIP_DOM_SELECTION_TAG,
  UNDO_COMMAND,
} from 'lexical';
import { createEmptyHistoryState, registerHistory } from '@lexical/history';
import { registerPlainText } from '@lexical/plain-text';
import type { SessionChatTheme } from '@/packages/shared/session-chat';
import { Tooltip, TooltipContent } from '../app-tooltip';
import type { SessionChatComposerInputApi, SessionChatComposerKeyEvent } from './session-chat-composer';
import { sessionChatCaretMovement } from './session-chat-caret-navigation';
import { revealSessionChatComposerCaret } from './session-chat-composer-scroll';
import { sessionChatBreaksKillSequence, sessionChatEditingShortcut } from './session-chat-edit-shortcuts';
import { createSessionChatTerminalEditing } from './session-chat-terminal-editing';
import { SESSION_CHAT_REFERENCE_REVEAL_MARKER } from './session-chat-reference-pills';
import {
  $composerLeaves,
  $readComposerSelection,
  $replaceComposerSelection,
  $setComposerSelection,
  $setComposerText,
  SessionChatReferenceNode,
  type ComposerSelection,
} from './session-chat-lexical/model';
import { COMPOSER_EDITOR_COMMANDS, type ComposerEditorControls } from './session-chat-lexical/commands';
import { ComposerEditorPanelView, type ComposerEditorPanel } from './session-chat-lexical/panels';
import './session-chat-lexical/input.css';

const EXTERNAL_VALUE_TAG = 'ghostex-composer-value';

/**
 * CDXC:SessionChat 2026-09-07 WHY:
 * CEF sometimes supplies nonstandard navigation key names. Retain the virtual-key normalization Monaco provided so send, queue, pickers, and history still recognize those keys.
 */
function composerKey(event: KeyboardEvent): string {
  return (
    (
      {
        8: 'Backspace',
        9: 'Tab',
        13: 'Enter',
        27: 'Escape',
        35: 'End',
        36: 'Home',
        37: 'ArrowLeft',
        38: 'ArrowUp',
        39: 'ArrowRight',
        40: 'ArrowDown',
        46: 'Delete',
        112: 'F1',
      } as Record<number, string>
    )[event.keyCode] ?? event.key
  );
}

export function SessionChatLexicalInput({
  initialValue,
  onCaretChange,
  onChange,
  onKeyDown,
  onPasteData,
  placeholder,
  fillHeight,
  collapsed = false,
  registerApi,
  theme,
}: {
  initialValue: string;
  onCaretChange: (caret: number) => void;
  onChange: (value: string, caret: number) => void;
  onKeyDown: (event: SessionChatComposerKeyEvent) => void;
  onPasteData: (data: DataTransfer) => boolean;
  placeholder: string;
  fillHeight: boolean;
  collapsed?: boolean;
  registerApi: (api: SessionChatComposerInputApi | null) => void;
  theme: SessionChatTheme;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const caretRef = useRef<HTMLDivElement>(null);
  const controlsRef = useRef<ComposerEditorControls | null>(null);
  const selectionRef = useRef<ComposerSelection>({
    anchor: initialValue.length,
    focus: initialValue.length,
    start: initialValue.length,
    end: initialValue.length,
  });
  const valueRef = useRef(initialValue.replace(/\r\n?/g, '\n'));
  const [panel, setPanel] = useState<ComposerEditorPanel | null>(null);
  const panelRef = useRef(panel);
  panelRef.current = panel;
  const [wrap, setWrap] = useState(true);
  const [referenceTooltip, setReferenceTooltip] = useState<{ anchor: HTMLElement; content: string } | null>(null);
  const callbacksRef = useRef({ onCaretChange, onChange, onKeyDown, onPasteData, registerApi });
  callbacksRef.current = { onCaretChange, onChange, onKeyDown, onPasteData, registerApi };
  const updateVisualsRef = useRef<(() => void) | null>(null);

  useLayoutEffect(() => {
    const root = rootRef.current;
    const container = containerRef.current;
    if (!root || !container) return;
    const editor = createEditor({
      namespace: 'ghostex-chat-composer',
      nodes: [SessionChatReferenceNode],
      onError: (error) => {
        throw error;
      },
    });
    editor.setRootElement(root);
    editor.update(
      () => {
        $setComposerText(valueRef.current);
        $setComposerSelection(selectionRef.current.anchor, selectionRef.current.focus);
      },
      { discrete: true, tag: SKIP_DOM_SELECTION_TAG }
    );
    const unregisterPlainText = registerPlainText(editor);
    const history = createEmptyHistoryState();
    history.current = { editor, editorState: editor.getEditorState() };
    const unregisterHistory = registerHistory(editor, history, 1000);
    let disposed = false;
    let visualFrame = 0;
    let revealSelection = false;
    let scrollbarFadeTimeout: number | undefined;

    const readSelection = (): ComposerSelection =>
      editor.getEditorState().read(() => {
        selectionRef.current = $readComposerSelection(selectionRef.current);
        return selectionRef.current;
      });
    const updateVisuals = (): void => {
      visualFrame = 0;
      if (disposed) return;
      const shouldReveal = revealSelection;
      revealSelection = false;
      root.dataset.empty = valueRef.current === '' ? 'true' : 'false';
      const current = readSelection();
      editor.getEditorState().read(() => {
        for (const { node, start, end } of $composerLeaves()) {
          if (node instanceof SessionChatReferenceNode) {
            const pill = editor.getElementByKey(node.getKey());
            if (pill) pill.dataset.selected = String(current.start < end && current.end > start);
          }
        }
      });
      const caret = caretRef.current;
      if (!caret) return;
      caret.style.display = 'none';
      // The saved caret is visual-only: the input and the pane keep their normal input ownership.
      let rect: DOMRect | undefined;
      const domSelection = document.getSelection();
      if (domSelection?.rangeCount && domSelection.focusNode && root.contains(domSelection.focusNode)) {
        const range = document.createRange();
        range.setStart(domSelection.focusNode, domSelection.focusOffset);
        range.collapse(true);
        rect = [...range.getClientRects()].find((value) => value.height > 0);
      }
      if (!rect) {
        editor.getEditorState().read(() => {
          for (const { node, start, end } of $composerLeaves()) {
            if (current.focus < start || current.focus > end) continue;
            const element = editor.getElementByKey(node.getKey());
            if (!element) continue;
            if (node instanceof SessionChatReferenceNode) {
              const box = element.getBoundingClientRect();
              rect = new DOMRect(current.focus === start ? box.left : box.right, box.top, 0, box.height);
            } else if (element.firstChild instanceof Text) {
              const range = document.createRange();
              range.setStart(element.firstChild, Math.min(current.focus - start, element.firstChild.length));
              range.collapse(true);
              rect = [...range.getClientRects()].find((value) => value.height > 0);
            } else if (element instanceof HTMLBRElement && current.focus === start) {
              const box = element.getBoundingClientRect();
              if (box.height > 0) rect = box;
            }
            if (rect) break;
          }
        });
      }
      const box = root.getBoundingClientRect();
      if (!rect && current.focus === valueRef.current.length) {
        const lastLine = root.lastElementChild?.lastElementChild;
        if (lastLine instanceof HTMLBRElement) rect = lastLine.getBoundingClientRect();
      }
      if (rect && shouldReveal && document.activeElement === root) {
        rect = revealSessionChatComposerCaret(root, rect);
      }
      if (current.start !== current.end) return;
      if (!rect || rect.top < box.top || rect.bottom > box.bottom + 1) return;
      const parent = container.getBoundingClientRect();
      caret.style.cssText = `left:${rect.left - parent.left}px;top:${rect.top - parent.top}px;height:${rect.height}px`;
    };
    const scheduleVisuals = (): void => {
      if (!visualFrame) visualFrame = requestAnimationFrame(updateVisuals);
    };
    /**
     * CDXC:SessionChat 2026-09-08 WHY:
     * Selection.modify bypasses native arrow-key scrolling, and Lexical skips its scroll when the DOM selection already matches or is extended.
     * Reveal the moving focus edge after explicit navigation, scrolling only the editor so manual scrolling and the surrounding chat stay put.
     */
    const scheduleSelectionReveal = (): void => {
      revealSelection = true;
      scheduleVisuals();
    };
    updateVisualsRef.current = scheduleVisuals;

    const focus = (): void => {
      if (document.activeElement === root) return;
      const saved = selectionRef.current;
      root.focus({ preventScroll: true });
      editor.update(() => $setComposerSelection(saved.anchor, saved.focus), { discrete: true });
    };
    const setSelection = (anchor: number, head = anchor): void => {
      focus();
      editor.update(() => $setComposerSelection(anchor, head), {
        discrete: true,
        ...(document.activeElement !== root ? { tag: SKIP_DOM_SELECTION_TAG } : {}),
      });
      readSelection();
      scheduleSelectionReveal();
    };
    const insertText = (text: string): boolean => {
      terminalEditing.breakSequence();
      focus();
      editor.update(
        () => {
          $setComposerSelection(selectionRef.current.anchor, selectionRef.current.focus);
          $replaceComposerSelection(text);
        },
        { discrete: true, tag: HISTORY_PUSH_TAG }
      );
      return true;
    };
    const valueListeners = new Set<() => void>();

    const controls: ComposerEditorControls = {
      focus,
      getValue: () => editor.getEditorState().read(() => $getRoot().getTextContent()),
      getSelection: () => {
        const { start, end } = readSelection();
        return { start, end };
      },
      setSelection,
      subscribe: (listener) => {
        valueListeners.add(listener);
        return () => {
          valueListeners.delete(listener);
        };
      },
      insertText,
      insertSavedPrompt: insertText,
      applyValue: (next, caret) => {
        terminalEditing.breakSequence();
        const normalized = next.replace(/\r\n?/g, '\n');
        const changed = controls.getValue() !== normalized;
        editor.update(
          () => {
            if (changed) $setComposerText(normalized);
            $setComposerSelection(Math.max(0, Math.min(caret, normalized.length)));
          },
          {
            discrete: true,
            tag: [
              EXTERNAL_VALUE_TAG,
              HISTORY_PUSH_TAG,
              ...(document.activeElement !== root ? [SKIP_DOM_SELECTION_TAG] : []),
            ],
          }
        );
        if (changed && normalized === '') {
          editor.dispatchCommand(CLEAR_HISTORY_COMMAND, undefined);
          history.current = { editor, editorState: editor.getEditorState() };
        }
        valueRef.current = normalized;
        readSelection();
        scheduleVisuals();
      },
      selectAll: () => {
        terminalEditing.breakSequence();
        focus();
        setSelection(0, controls.getValue().length);
      },
      editText: (command) => {
        if (panelRef.current) return;
        focus();
        if (terminalEditing.run(command)) return;
        if (command === 'undo' || command === 'redo') {
          editor.dispatchCommand(command === 'undo' ? UNDO_COMMAND : REDO_COMMAND, undefined);
          return;
        }
        editor.update(
          () => {
            $setComposerSelection(selectionRef.current.anchor, selectionRef.current.focus);
            const selection = $getSelection();
            if (!$isRangeSelection(selection)) return;
            if (!selection.isCollapsed()) selection.removeText();
            else if (command.startsWith('deleteWord')) selection.deleteWord(command.endsWith('Left'));
            else selection.deleteLine(command.endsWith('Left'));
          },
          { discrete: true, tag: HISTORY_PUSH_TAG }
        );
      },
      navigateCaret: ({ direction, select, unit }) => {
        if (panelRef.current) return;
        terminalEditing.breakSequence();
        focus();
        const current = readSelection();
        const backward = direction === 'left' || direction === 'up';
        let target: number | undefined;
        if (unit === 'document') target = backward ? 0 : controls.getValue().length;
        else if (unit === 'paragraph') {
          const text = controls.getValue();
          const boundaries = [0, ...[...text.matchAll(/\n[\t ]*\n/g)].map((match) => match.index! + 1), text.length];
          target = backward
            ? (boundaries.filter((offset) => offset < current.focus).at(-1) ?? 0)
            : (boundaries.find((offset) => offset > current.focus) ?? text.length);
        } else if (unit === 'character' && (direction === 'left' || direction === 'right')) {
          if (!select && current.start !== current.end) target = backward ? current.start : current.end;
          else
            editor.getEditorState().read(() => {
              const reference = $composerLeaves().find(
                ({ node, start, end }) =>
                  node instanceof SessionChatReferenceNode &&
                  (backward ? end === current.focus : start === current.focus)
              );
              if (reference) target = backward ? reference.start : reference.end;
            });
        }
        if (target !== undefined) {
          setSelection(select ? current.anchor : target, target);
          return;
        }
        const domSelection = document.getSelection();
        if (!domSelection) return;
        domSelection.modify(
          select ? 'extend' : 'move',
          backward ? 'backward' : 'forward',
          {
            character: 'character',
            word: 'word',
            line: 'line',
            lineBoundary: 'lineboundary',
            paragraph: 'paragraphboundary',
            document: 'documentboundary',
          }[unit]
        );
        editor.update(
          () => {
            $setSelection($createRangeSelectionFromDom(domSelection, editor));
          },
          { discrete: true }
        );
        readSelection();
        scheduleSelectionReveal();
      },
    };
    const terminalEditing = createSessionChatTerminalEditing({
      getValue: controls.getValue,
      getSelection: readSelection,
      setSelection,
      insertText,
    });
    controlsRef.current = controls;
    const unregisterUpdates = editor.registerUpdateListener(({ editorState, tags }) => {
      editorState.read(() => {
        const value = $getRoot().getTextContent();
        selectionRef.current = $readComposerSelection(selectionRef.current);
        const changed = value !== valueRef.current;
        valueRef.current = value;
        if (changed) for (const listener of valueListeners) listener();
        if (changed && !tags.has(SKIP_DOM_SELECTION_TAG)) scheduleSelectionReveal();
        if (changed && !tags.has(EXTERNAL_VALUE_TAG)) callbacksRef.current.onChange(value, selectionRef.current.focus);
        callbacksRef.current.onCaretChange(selectionRef.current.focus);
      });
      setReferenceTooltip(null);
      scheduleVisuals();
    });

    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.isComposing || event.keyCode === 229 || editor.isComposing()) return;
      const key = composerKey(event);
      const shortcutKey = shortcutKeyFromKeyboardEvent(event);
      const mac = /Mac|iPhone|iPad/.test(navigator.platform);
      const primary = mac ? event.metaKey : event.ctrlKey;
      const adapted: SessionChatComposerKeyEvent = {
        altKey: event.altKey,
        code: event.code,
        ctrlKey: event.ctrlKey,
        metaKey: event.metaKey,
        shiftKey: event.shiftKey,
        isComposing: false,
        key,
        keyCode: event.keyCode,
        preventDefault: () => {
          event.preventDefault();
          event.stopPropagation();
        },
      };
      if (sessionChatBreaksKillSequence(adapted)) terminalEditing.breakSequence();
      let nextPanel: ComposerEditorPanel | undefined;
      if (key === 'F1' || (primary && event.shiftKey && shortcutKey === 'p')) nextPanel = 'commands';
      else if (primary && shortcutKey === 'f' && !event.altKey) nextPanel = 'find';
      else if ((primary && event.altKey && shortcutKey === 'f') || (!mac && primary && shortcutKey === 'h'))
        nextPanel = 'replace';
      else if (event.ctrlKey && shortcutKey === 'g') nextPanel = 'line';
      if (nextPanel) {
        event.preventDefault();
        event.stopPropagation();
        panelRef.current = nextPanel;
        setPanel(nextPanel);
        return;
      }
      callbacksRef.current.onKeyDown(adapted);
      if (event.defaultPrevented) return;
      if (event.altKey && !event.metaKey && !event.ctrlKey && shortcutKey === 'z') {
        event.preventDefault();
        event.stopPropagation();
        setWrap((value) => !value);
        return;
      }
      if (key === 'Tab' && !event.metaKey && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        event.stopPropagation();
        if (event.shiftKey) COMPOSER_EDITOR_COMMANDS.find((item) => item.id === 'outdent')?.run(controls);
        else controls.insertText('    ');
        return;
      }
      if (key === 'Enter') {
        event.preventDefault();
        event.stopPropagation();
        controls.insertText('\n');
        return;
      }
      const editing = sessionChatEditingShortcut(adapted);
      if (editing && !['copy', 'cut', 'paste'].includes(editing)) {
        event.preventDefault();
        event.stopPropagation();
        if (editing === 'selectAll') controls.selectAll();
        else controls.editText(editing as Parameters<SessionChatComposerInputApi['editText']>[0]);
        return;
      }
      const movement = sessionChatCaretMovement(adapted);
      if (event.altKey && !event.ctrlKey && !event.metaKey && (key === 'ArrowUp' || key === 'ArrowDown')) {
        event.preventDefault();
        event.stopPropagation();
        const command = `${event.shiftKey ? 'copy' : 'move'}Lines${key === 'ArrowUp' ? 'Up' : 'Down'}`;
        COMPOSER_EDITOR_COMMANDS.find((item) => item.id === command)?.run(controls);
        return;
      }
      if (movement) {
        event.preventDefault();
        event.stopPropagation();
        controls.navigateCaret(movement);
        return;
      }
      if (key === 'Home' || key === 'End') {
        event.preventDefault();
        event.stopPropagation();
        controls.navigateCaret({
          direction: key === 'Home' ? (primary ? 'up' : 'left') : primary ? 'down' : 'right',
          select: event.shiftKey,
          unit: primary ? 'document' : 'lineBoundary',
        });
      }
      const command =
        primary && !event.altKey
          ? shortcutKey === ']'
            ? 'indent'
            : shortcutKey === '['
              ? 'outdent'
              : shortcutKey === 'l'
                ? 'expandLineSelection'
                : event.shiftKey && shortcutKey === 'k'
                  ? 'deleteLines'
                  : undefined
          : undefined;
      if (command) {
        event.preventDefault();
        event.stopPropagation();
        COMPOSER_EDITOR_COMMANDS.find((item) => item.id === command)?.run(controls);
      }
    };

    const clipboard = (event: ClipboardEvent): void => {
      terminalEditing.breakSequence();
      if (!event.clipboardData) return;
      if (event.type === 'paste') {
        const handled = callbacksRef.current.onPasteData(event.clipboardData);
        const text = event.clipboardData.getData('text/plain');
        if (!handled && text !== '') insertText(text);
      } else {
        const { start, end } = controls.getSelection();
        if (start !== end) event.clipboardData.setData('text/plain', controls.getValue().slice(start, end));
        if (event.type === 'cut' && start !== end) insertText('');
      }
      event.preventDefault();
      event.stopPropagation();
    };
    const doubleClick = (event: MouseEvent): void => {
      if (event.detail !== 2 || !(event.target instanceof Element)) return;
      const pill = event.target.closest<HTMLElement>('[data-ghostex-reference-key]');
      if (!pill || !root.contains(pill)) return;
      event.preventDefault();
      event.stopPropagation();
      let reference: { source: string; start: number; end: number } | undefined;
      editor.getEditorState().read(() => {
        const found = $composerLeaves().find(({ node }) => node.getKey() === pill.dataset.ghostexReferenceKey);
        if (found) reference = { source: found.node.getTextContent(), start: found.start, end: found.end };
      });
      if (!reference) return;
      const { source, start, end } = reference;
      const labelEnd = source.indexOf('](');
      if (labelEnd < 0) return;
      setSelection(start, end);
      insertText(`${source.slice(0, labelEnd)}${SESSION_CHAT_REFERENCE_REVEAL_MARKER}${source.slice(labelEnd)}`);
      setSelection(start + labelEnd + 1);
    };
    const handleScroll = (): void => {
      scheduleVisuals();
      root.dataset.scrolling = 'true';
      window.clearTimeout(scrollbarFadeTimeout);
      scrollbarFadeTimeout = window.setTimeout(() => {
        delete root.dataset.scrolling;
      }, 900);
    };
    root.addEventListener('keydown', handleKeyDown, true);
    root.addEventListener('input', scheduleSelectionReveal);
    root.addEventListener('paste', clipboard, true);
    root.addEventListener('copy', clipboard, true);
    root.addEventListener('cut', clipboard, true);
    root.addEventListener('mousedown', doubleClick, true);
    root.addEventListener('pointerdown', terminalEditing.breakSequence);
    root.addEventListener('scroll', handleScroll);
    root.addEventListener('blur', scheduleVisuals);
    const resize = new ResizeObserver(scheduleVisuals);
    resize.observe(root);
    window.addEventListener('ghostex-session-chat-font-family-changed', scheduleVisuals);
    callbacksRef.current.registerApi(controls);
    scheduleVisuals();
    return () => {
      disposed = true;
      cancelAnimationFrame(visualFrame);
      window.clearTimeout(scrollbarFadeTimeout);
      delete root.dataset.scrolling;
      callbacksRef.current.registerApi(null);
      unregisterUpdates();
      unregisterHistory();
      unregisterPlainText();
      root.removeEventListener('keydown', handleKeyDown, true);
      root.removeEventListener('input', scheduleSelectionReveal);
      root.removeEventListener('paste', clipboard, true);
      root.removeEventListener('copy', clipboard, true);
      root.removeEventListener('cut', clipboard, true);
      root.removeEventListener('mousedown', doubleClick, true);
      root.removeEventListener('pointerdown', terminalEditing.breakSequence);
      root.removeEventListener('scroll', handleScroll);
      root.removeEventListener('blur', scheduleVisuals);
      resize.disconnect();
      window.removeEventListener('ghostex-session-chat-font-family-changed', scheduleVisuals);
      editor.setRootElement(null);
      controlsRef.current = null;
      updateVisualsRef.current = null;
    };
  }, []);

  useLayoutEffect(() => {
    updateVisualsRef.current?.();
  }, [collapsed, fillHeight, wrap, panel]);
  const closePanel = (): void => {
    panelRef.current = null;
    setPanel(null);
    controlsRef.current?.focus();
  };
  return (
    <div
      ref={containerRef}
      className='ghostex-chat-composer-lexical w-full min-w-0 flex-1'
      data-collapsed={collapsed}
      data-fill-height={fillHeight}
      data-word-wrap={wrap && !collapsed}
      data-editor-theme={theme}
      data-editor-panel-open={panel !== null}
      data-session-chat-typing-redirect-ignore='true'
      onPointerLeave={() => setReferenceTooltip(null)}
      onPointerOver={(event) => {
        const pill =
          event.target instanceof Element ? event.target.closest<HTMLElement>('[data-ghostex-reference-path]') : null;
        if (pill?.dataset.ghostexReferencePath)
          setReferenceTooltip({ anchor: pill, content: pill.dataset.ghostexReferencePath });
      }}
      onPointerOut={(event) => {
        const next = event.relatedTarget;
        if (referenceTooltip && (!(next instanceof Node) || !referenceTooltip.anchor.contains(next)))
          setReferenceTooltip(null);
      }}
    >
      {panel && controlsRef.current ? (
        <ComposerEditorPanelView
          key={panel}
          mode={panel}
          editor={controlsRef.current}
          onClose={closePanel}
          onModeChange={(mode) => {
            panelRef.current = mode;
            setPanel(mode);
          }}
          onToggleWrap={() => setWrap((value) => !value)}
        />
      ) : null}
      <div
        ref={rootRef}
        className='ghostex-chat-composer-lexical-content scroll-fade-y'
        role='textbox'
        aria-label='Message'
        aria-multiline='true'
        aria-placeholder={placeholder}
        contentEditable
        spellCheck={false}
        autoCapitalize='off'
        autoCorrect='off'
        data-placeholder={placeholder}
        data-empty={initialValue === ''}
        suppressContentEditableWarning
      />
      <div ref={caretRef} className='ghostex-chat-composer-saved-caret' aria-hidden='true' />
      {referenceTooltip ? (
        <Tooltip open>
          <TooltipContent anchor={referenceTooltip.anchor} side='top'>
            {referenceTooltip.content}
          </TooltipContent>
        </Tooltip>
      ) : null}
    </div>
  );
}
