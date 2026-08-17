import {
  IconChevronDown,
  IconChevronUp,
  IconSearch,
  IconX,
} from "@tabler/icons-react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, RefObject } from "react";
import { Button } from "../../components/ui/button";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
  InputGroupText,
} from "../../components/ui/input-group";

const MATCH_HIGHLIGHT_NAME = "ghostex-chat-search-match";
const ACTIVE_HIGHLIGHT_NAME = "ghostex-chat-search-active";

interface HighlightRegistry {
  delete(name: string): void;
  set(name: string, highlight: unknown): void;
}

type HighlightConstructor = new (...ranges: Range[]) => unknown;

function highlightApi(): {
  HighlightClass: HighlightConstructor;
  registry: HighlightRegistry;
} | null {
  const HighlightClass = (window as typeof window & { Highlight?: HighlightConstructor })
    .Highlight;
  const registry = (window.CSS as
    | (typeof CSS & { highlights?: HighlightRegistry })
    | undefined)?.highlights;
  return HighlightClass && registry ? { HighlightClass, registry } : null;
}

function clearHighlights(): void {
  const api = highlightApi();
  api?.registry.delete(MATCH_HIGHLIGHT_NAME);
  api?.registry.delete(ACTIVE_HIGHLIGHT_NAME);
}

function transcriptMatches(root: HTMLElement, query: string): Range[] {
  const content = root.querySelector<HTMLElement>(
    '[data-slot="message-scroller-content"]',
  );
  const needle = query.trim();
  if (!content || !needle) {
    return [];
  }

  const textNodes: Array<{ end: number; node: Text; start: number }> = [];
  let transcriptText = "";
  const matches: Range[] = [];
  const walker = document.createTreeWalker(content, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (
        !node.textContent?.trim() ||
        parent?.closest(
          'button, input, script, style, textarea, [aria-hidden="true"], [hidden], [data-session-chat-search-ignore="true"]',
        )
      ) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });

  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node.textContent ?? "";
    const start = transcriptText.length;
    transcriptText += text;
    textNodes.push({ end: transcriptText.length, node: node as Text, start });
  }

  const escapedNeedle = needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(escapedNeedle, "giu");
  for (const match of transcriptText.matchAll(pattern)) {
    const matchStart = match.index;
    const matchEnd = matchStart + match[0].length;
    const first = textNodes.find(({ end }) => end > matchStart);
    const last = textNodes.find(({ end }) => end >= matchEnd);
    if (!first || !last) {
      continue;
    }
    const range = document.createRange();
    range.setStart(first.node, matchStart - first.start);
    range.setEnd(last.node, matchEnd - last.start);
    matches.push(range);
  }
  return matches;
}

function centerMatch(root: HTMLElement, range: Range): void {
  const viewport = root.querySelector<HTMLElement>(
    '[data-slot="message-scroller-viewport"]',
  );
  const parent = range.startContainer.parentElement;
  const target = parent?.closest<HTMLElement>('[data-slot="message-scroller-item"]') ?? parent;
  if (!viewport || !target) {
    return;
  }

  const viewportRect = viewport.getBoundingClientRect();
  const targetRect = target.getBoundingClientRect();
  viewport.scrollTo({
    behavior: "smooth",
    top:
      viewport.scrollTop +
      targetRect.top -
      viewportRect.top -
      viewport.clientHeight / 2 +
      targetRect.height / 2,
  });
}

export function SessionChatSearch({
  rootRef,
  searchRevision,
  showButton = false,
}: {
  rootRef: RefObject<HTMLDivElement | null>;
  searchRevision: unknown;
  showButton?: boolean;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<readonly Range[]>([]);
  const [activeIndex, setActiveIndex] = useState(0);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setMatches([]);
    setActiveIndex(0);
    clearHighlights();
  }, []);

  useEffect(() => {
    const handleShortcut = (event: globalThis.KeyboardEvent): void => {
      if (
        event.key.toLocaleLowerCase() !== "f" ||
        !event.metaKey ||
        event.ctrlKey ||
        event.altKey
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      setOpen(true);
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      });
    };
    window.addEventListener("keydown", handleShortcut, true);
    return () => window.removeEventListener("keydown", handleShortcut, true);
  }, []);

  useEffect(() => {
    if (!open) {
      return;
    }
    const frame = requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
    return () => cancelAnimationFrame(frame);
  }, [open]);

  useEffect(() => {
    if (!open) {
      clearHighlights();
      return;
    }
    const root = rootRef.current;
    const nextMatches = root ? transcriptMatches(root, query) : [];
    setMatches(nextMatches);
    setActiveIndex(0);
  }, [open, query, rootRef, searchRevision]);

  useEffect(() => {
    clearHighlights();
    if (!open || matches.length === 0) {
      return;
    }
    const api = highlightApi();
    if (!api) {
      return;
    }
    api.registry.set(MATCH_HIGHLIGHT_NAME, new api.HighlightClass(...matches));
    const activeMatch = matches[activeIndex];
    if (activeMatch) {
      api.registry.set(ACTIVE_HIGHLIGHT_NAME, new api.HighlightClass(activeMatch));
    }
    return clearHighlights;
  }, [activeIndex, matches, open]);

  useEffect(() => {
    const root = rootRef.current;
    const activeMatch = matches[activeIndex];
    if (open && root && activeMatch) {
      centerMatch(root, activeMatch);
    }
  }, [activeIndex, matches, open, rootRef]);

  useEffect(() => clearHighlights, []);

  const move = useCallback(
    (offset: number): void => {
      if (matches.length === 0) {
        return;
      }
      setActiveIndex((current) => (current + offset + matches.length) % matches.length);
    },
    [matches.length],
  );

  const handleInputKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLInputElement>): void => {
      if (event.key === "Enter") {
        event.preventDefault();
        move(event.shiftKey ? -1 : 1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        move(-1);
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        move(1);
      } else if (event.key === "Escape") {
        event.preventDefault();
        close();
      }
    },
    [close, move],
  );

  if (!open) {
    return showButton ? (
      <Button
        aria-label="Search conversation"
        className="absolute right-3 top-3 z-30 shadow-sm"
        onClick={() => setOpen(true)}
        size="icon"
        type="button"
        variant="secondary"
      >
        <IconSearch aria-hidden="true" data-icon="inline-start" />
      </Button>
    ) : null;
  }

  const resultLabel = query.trim()
    ? matches.length > 0
      ? `${activeIndex + 1} of ${matches.length}`
      : "No results"
    : "";

  if (!showButton) {
    const terminalResultLabel = query.trim()
      ? matches.length > 0
        ? `${activeIndex + 1}/${matches.length}`
        : "N/A"
      : "";
    return (
      <div
        className="ghostex-chat-terminal-search-row"
        data-session-chat-search-ignore="true"
        data-session-chat-typing-redirect-ignore="true"
        role="search"
      >
        <div className="ghostex-chat-terminal-search-bar">
          <input
            aria-label="Search conversation"
            autoFocus
            className="ghostex-chat-terminal-search-input"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleInputKeyDown}
            placeholder="Search"
            ref={inputRef}
            spellCheck={false}
            value={query}
          />
          {terminalResultLabel ? (
            <span
              aria-live="polite"
              className="ghostex-chat-terminal-search-count"
            >
              {terminalResultLabel}
            </span>
          ) : null}
          <div className="ghostex-chat-terminal-search-actions">
            <button
              aria-label="Previous result"
              className="ghostex-chat-terminal-search-button"
              onClick={() => move(-1)}
              onMouseDown={(event) => event.preventDefault()}
              type="button"
            >
              ↑
            </button>
            <button
              aria-label="Next result"
              className="ghostex-chat-terminal-search-button"
              onClick={() => move(1)}
              onMouseDown={(event) => event.preventDefault()}
              type="button"
            >
              ↓
            </button>
            <button
              aria-label="Close search"
              className="ghostex-chat-terminal-search-button"
              onClick={close}
              onMouseDown={(event) => event.preventDefault()}
              type="button"
            >
              ✕
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className="absolute inset-x-3 top-3 z-30 rounded-lg border border-border bg-popover p-1 shadow-lg"
      data-session-chat-typing-redirect-ignore="true"
      data-session-chat-search-ignore="true"
      role="search"
    >
      <InputGroup className="rounded-lg bg-background">
        <InputGroupInput
          aria-label="Search conversation"
          autoFocus
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleInputKeyDown}
          placeholder="Search conversation"
          ref={inputRef}
          value={query}
        />
        <InputGroupAddon align="inline-end" className="gap-0.5 pr-1.5">
          <InputGroupText
            aria-live="polite"
            className="min-w-14 justify-end whitespace-nowrap text-xs tabular-nums"
          >
            {resultLabel}
          </InputGroupText>
          <InputGroupButton
            aria-label="Previous result"
            disabled={matches.length === 0}
            onClick={() => move(-1)}
            size="icon-sm"
          >
            <IconChevronUp aria-hidden="true" data-icon="inline-start" />
          </InputGroupButton>
          <InputGroupButton
            aria-label="Next result"
            disabled={matches.length === 0}
            onClick={() => move(1)}
            size="icon-sm"
          >
            <IconChevronDown aria-hidden="true" data-icon="inline-start" />
          </InputGroupButton>
          <InputGroupButton aria-label="Close search" onClick={close} size="icon-sm">
            <IconX aria-hidden="true" data-icon="inline-start" />
          </InputGroupButton>
        </InputGroupAddon>
      </InputGroup>
    </div>
  );
}
