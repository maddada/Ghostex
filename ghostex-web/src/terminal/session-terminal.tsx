import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon, type ISearchOptions } from "@xterm/addon-search";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  type HTMLAttributes,
} from "react";
import type {
  GxserverProjectId,
  GxserverSessionId,
  GxserverTerminalWsExitMessage,
  GxserverTerminalWsReadyMessage,
} from "@/shared/gxserver-protocol";
import { GHOSTTY_DEFAULT_THEME } from "./ghostty-default-theme";
import {
  TerminalWsClient,
  type TerminalWsClientError,
} from "./terminal-ws-client";
import "./session-terminal.css";

const INITIAL_COLS = 120;
const INITIAL_ROWS = 30;
const TERMINAL_FONT_FAMILY =
  '"JetBrainsMono Nerd Font", "JetBrains Mono", "SFMono-Regular", "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", monospace';
const SEARCH_DECORATIONS: NonNullable<ISearchOptions["decorations"]> = {
  activeMatchBackground: "#6ca4f8",
  activeMatchBorder: "#ffffff",
  activeMatchColorOverviewRuler: "#6ca4f8",
  matchBackground: "#3b5070",
  matchBorder: "#8b949e",
  matchOverviewRuler: "#3b5070",
};

export interface SessionTerminalHandle {
  clearSearch(): void;
  focus(): void;
  searchNext(term: string, options?: ISearchOptions): boolean;
  searchPrev(term: string, options?: ISearchOptions): boolean;
}

export interface SessionTerminalProps
  extends Omit<HTMLAttributes<HTMLDivElement>, "onError" | "onReady"> {
  authToken: string;
  autoFocus?: boolean;
  baseUrl: string;
  customKeyEventHandler?(event: KeyboardEvent): boolean;
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

export const SessionTerminal = forwardRef<SessionTerminalHandle, SessionTerminalProps>(
  function SessionTerminal(
    {
      "aria-label": ariaLabel = "Session terminal",
      authToken,
      autoFocus = false,
      baseUrl,
      className,
      customKeyEventHandler,
      onError,
      onExit,
      onReady,
      projectId,
      sessionId,
      ...containerProps
    },
    ref,
  ) {
    const containerRef = useRef<HTMLDivElement>(null);
    const searchAddonRef = useRef<SearchAddon>(null);
    const terminalRef = useRef<Terminal>(null);
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
          return (
            searchAddonRef.current?.findPrevious(term, withSearchDecorations(options)) ?? false
          );
        },
      }),
      [],
    );

    useEffect(() => {
      const container = containerRef.current;
      if (!container) {
        return;
      }
      const terminal = new Terminal({
        allowProposedApi: true,
        cols: INITIAL_COLS,
        cursorBlink: true,
        cursorStyle: "bar",
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
      terminal.attachCustomKeyEventHandler(
        (event) => callbacksRef.current.customKeyEventHandler?.(event) ?? true,
      );
      terminalRef.current = terminal;
      searchAddonRef.current = searchAddon;

      const fit = () => {
        if (container.clientWidth > 0 && container.clientHeight > 0) {
          fitAddon.fit();
        }
      };
      fit();

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
        onReconnect: () => terminal.write("\x1bc"),
        projectId,
        rows: terminal.rows,
        sessionId,
      });
      const dataSubscription = terminal.onData((data) => client.sendInput(data));
      const resizeSubscription = terminal.onResize(({ cols, rows }) => client.resize(cols, rows));
      const resizeObserver = new ResizeObserver(fit);
      resizeObserver.observe(container);
      const initialFitFrame = window.requestAnimationFrame(fit);

      return () => {
        window.cancelAnimationFrame(initialFitFrame);
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

    return (
      <div
        {...containerProps}
        aria-label={ariaLabel}
        className={["session-terminal", className].filter(Boolean).join(" ")}
        ref={containerRef}
      />
    );
  },
);
