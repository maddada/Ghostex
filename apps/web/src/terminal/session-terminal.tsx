import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon, type ISearchOptions } from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { forwardRef, useEffect, useImperativeHandle, useRef, useState, type HTMLAttributes } from 'react';
import { detectghostexHotkeyPlatform } from '@/packages/shared/ghostex-hotkeys';
import type {
  GxserverProjectId,
  GxserverSessionId,
  GxserverTerminalWsExitMessage,
  GxserverTerminalWsReadyMessage,
} from '@/packages/shared/gxserver-protocol';
import { GHOSTTY_DEFAULT_THEME } from './ghostty-default-theme';
import { TerminalWsClient, type TerminalWsClientError, type TerminalVisibility } from './terminal-ws-client';
import './session-terminal.css';

const INITIAL_COLS = 120;
const INITIAL_ROWS = 30;
/*
A hidden client rests at zmx's wide grid so it never clamps the columns of
whichever client is actually displaying the session; the local xterm is
resized to match so its grid equals the one announced to zmx.
*/
const HIDDEN_COLS = 200;
const TERMINAL_FONT_FAMILY =
  '"JetBrainsMono Nerd Font", "JetBrains Mono", "SFMono-Regular", "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", monospace';
const SEARCH_DECORATIONS: NonNullable<ISearchOptions['decorations']> = {
  activeMatchBackground: '#6ca4f8',
  activeMatchBorder: '#ffffff',
  activeMatchColorOverviewRuler: '#6ca4f8',
  matchBackground: '#3b5070',
  matchBorder: '#8b949e',
  matchOverviewRuler: '#3b5070',
};

export interface SessionTerminalHandle {
  clearSearch(): void;
  focus(): void;
  searchNext(term: string, options?: ISearchOptions): boolean;
  searchPrev(term: string, options?: ISearchOptions): boolean;
}

export interface SessionTerminalProps extends Omit<HTMLAttributes<HTMLDivElement>, 'onError' | 'onReady'> {
  authToken: string;
  autoFocus?: boolean;
  baseUrl: string;
  customKeyEventHandler?(event: KeyboardEvent): boolean;
  /**
   * On-screen terminal, on-screen chat, or parked behind another tab.
   * Both non-visible states keep the socket open and pin the local grid.
   * A background browser tab always reports parked.
   */
  visibility?: TerminalVisibility;
  onError?(error: TerminalWsClientError): void;
  onExit?(message: GxserverTerminalWsExitMessage): void;
  onReady?(message: GxserverTerminalWsReadyMessage): void;
  projectId: GxserverProjectId;
  sessionId: GxserverSessionId;
}

function withSearchDecorations(options?: ISearchOptions): ISearchOptions {
  return {
    ...options,
    decorations: options?.decorations ?? SEARCH_DECORATIONS,
  };
}

/*
Ghostty binds Cmd+K to `clear_screen`, and only on macOS, so the web
terminal answers the same chord the same way the desktop app does.
*/
const CLEAR_SCREEN_CHORD_IS_AVAILABLE = detectghostexHotkeyPlatform() === 'mac';

function isClearScreenChord(event: KeyboardEvent): boolean {
  return (
    CLEAR_SCREEN_CHORD_IS_AVAILABLE &&
    event.metaKey &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    event.key.toLowerCase() === 'k'
  );
}

/**
 * Ghostty's `clear_screen`: erase the scrollback, then drop the rows above
 * the cursor so a half-typed command line lifts to the top with its column
 * intact. Returns false on the alternate screen, which Ghostty never clears
 * because an emulator-level clear desynchronizes the running program's idea
 * of where the cursor is; Ghostty leaves the key to the program there.
 */
function clearScreen(terminal: Terminal): boolean {
  const buffer = terminal.buffer.active;
  if (buffer.type === 'alternate') {
    return false;
  }
  // Neither xterm.js nor libghostty exposes an erase that drops rows off
  // the top of the screen, so this writes the sequence the desktop app
  // feeds its own parser. DECSC/DECRC carry the pen across the delete and
  // the pen is reset in between, so the rows DL opens at the bottom are
  // blank in the default background rather than in whatever the program was
  // painting with; DL parks the cursor in column 0, so the trailing CUP puts
  // it back on the content that just moved to the top.
  const rowsAboveCursor = buffer.cursorY;
  terminal.write(
    rowsAboveCursor > 0
      ? `\u001b[3J\u001b7\u001b[m\u001b[H\u001b[${rowsAboveCursor}M\u001b8\u001b[1;${buffer.cursorX + 1}H`
      : '\u001b[3J'
  );
  terminal.clearSelection();
  terminal.scrollToBottom();
  return true;
}

function enableWebgl(terminal: Terminal): (() => void) | undefined {
  let addon: WebglAddon | undefined;
  try {
    addon = new WebglAddon();
    terminal.loadAddon(addon);
    const contextLossSubscription = addon.onContextLoss(() => addon?.dispose());
    return () => contextLossSubscription.dispose();
  } catch {
    addon?.dispose();
    return undefined;
  }
}

export const SessionTerminal = forwardRef<SessionTerminalHandle, SessionTerminalProps>(function SessionTerminal(
  {
    'aria-label': ariaLabel = 'Session terminal',
    authToken,
    autoFocus = false,
    baseUrl,
    className,
    customKeyEventHandler,
    visibility: requestedVisibility = 'visible',
    onError,
    onExit,
    onReady,
    projectId,
    sessionId,
    ...containerProps
  },
  ref
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const searchAddonRef = useRef<SearchAddon>(null);
  const terminalRef = useRef<Terminal>(null);
  const [pageVisible, setPageVisible] = useState(() => document.visibilityState === 'visible');
  useEffect(() => {
    const update = () => setPageVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  }, []);
  const visibility = pageVisible ? requestedVisibility : 'parked';
  const visibilityRef = useRef(visibility);
  visibilityRef.current = visibility;
  const applyVisibilityRef = useRef<(state: TerminalVisibility) => void>(null);
  const callbacksRef = useRef({
    autoFocus,
    customKeyEventHandler,
    onError,
    onExit,
    onReady,
  });
  callbacksRef.current = {
    autoFocus,
    customKeyEventHandler,
    onError,
    onExit,
    onReady,
  };

  useImperativeHandle(
    ref,
    () => ({
      clearSearch() {
        searchAddonRef.current?.clearDecorations();
        terminalRef.current?.clearSelection();
      },
      focus() {
        terminalRef.current?.focus();
      },
      searchNext(term, options) {
        return searchAddonRef.current?.findNext(term, withSearchDecorations(options)) ?? false;
      },
      searchPrev(term, options) {
        return searchAddonRef.current?.findPrevious(term, withSearchDecorations(options)) ?? false;
      },
    }),
    []
  );

  useEffect(() => {
    if (autoFocus) {
      terminalRef.current?.focus();
    }
  }, [autoFocus]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const terminal = new Terminal({
      allowProposedApi: true,
      cols: INITIAL_COLS,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: TERMINAL_FONT_FAMILY,
      fontSize: 13,
      fontWeight: 300,
      letterSpacing: 0,
      lineHeight: 1.2,
      rows: INITIAL_ROWS,
      scrollback: 5_000,
      theme: GHOSTTY_DEFAULT_THEME,
    });
    const fitAddon = new FitAddon();
    const searchAddon = new SearchAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(searchAddon);
    terminal.open(container);
    const disposeWebgl = enableWebgl(terminal);
    terminal.attachCustomKeyEventHandler((event) => {
      if (callbacksRef.current.customKeyEventHandler?.(event) === false) {
        return false;
      }
      if (!isClearScreenChord(event)) {
        return true;
      }
      if (event.type === 'keydown' && !clearScreen(terminal)) {
        // Alternate screen: Ghostty leaves the binding unconsumed.
        return true;
      }
      event.preventDefault();
      event.stopPropagation();
      return false;
    });
    terminalRef.current = terminal;
    searchAddonRef.current = searchAddon;

    const measure = () => {
      if (container.clientWidth > 0 && container.clientHeight > 0) {
        fitAddon.fit();
      }
    };
    // Container-driven fits only apply while displayed; a hidden terminal
    // holds the wide grid it announced to zmx until it is shown again.
    const fit = () => {
      if (visibilityRef.current === 'visible') {
        measure();
      }
    };
    measure();
    if (visibilityRef.current !== 'visible') {
      terminal.resize(HIDDEN_COLS, terminal.rows);
    }

    const client = new TerminalWsClient({
      authToken,
      baseUrl,
      cols: terminal.cols,
      onError: (error) => callbacksRef.current.onError?.(error),
      onExit: (message) => callbacksRef.current.onExit?.(message),
      onOutput: (bytes) => terminal.write(bytes),
      onReady: (message) => {
        callbacksRef.current.onReady?.(message);
        if (callbacksRef.current.autoFocus) {
          terminal.focus();
        }
      },
      onReconnect: () => terminal.write('\x1bc'),
      projectId,
      rows: terminal.rows,
      sessionId,
    });
    const dataSubscription = terminal.onData((data) => client.sendInput(data));
    const resizeSubscription = terminal.onResize(({ cols, rows }) => client.resize(cols, rows));
    const applyVisibility = (state: TerminalVisibility) => {
      if (state !== 'visible') {
        terminal.resize(HIDDEN_COLS, terminal.rows);
      } else {
        measure();
      }
      client.setVisibility(state, { cols: terminal.cols, rows: terminal.rows });
    };
    applyVisibilityRef.current = applyVisibility;
    client.setVisibility(visibilityRef.current, { cols: terminal.cols, rows: terminal.rows });
    const resizeObserver = new ResizeObserver(fit);
    resizeObserver.observe(container);
    const initialFitFrame = window.requestAnimationFrame(fit);

    return () => {
      window.cancelAnimationFrame(initialFitFrame);
      applyVisibilityRef.current = null;
      resizeObserver.disconnect();
      dataSubscription.dispose();
      resizeSubscription.dispose();
      client.close();
      disposeWebgl?.();
      terminalRef.current = null;
      searchAddonRef.current = null;
      terminal.dispose();
    };
  }, [authToken, baseUrl, projectId, sessionId]);

  const mountedVisibilityRef = useRef(visibility);
  useEffect(() => {
    if (mountedVisibilityRef.current === visibility) {
      return;
    }
    mountedVisibilityRef.current = visibility;
    applyVisibilityRef.current?.(visibility);
  }, [visibility]);

  return (
    <div
      {...containerProps}
      aria-label={ariaLabel}
      className={['session-terminal', className].filter(Boolean).join(' ')}
      ref={containerRef}
    />
  );
});
