// Shiki engine + highlight cache behind Session Chat's fenced code blocks.
//
// This module owns everything that is NOT React: the lazily-created
// `shiki/core` highlighter, grammar registration, and the bounded LRU of
// already-rendered HTML. The React side (session-chat-markdown.tsx) only calls
// `sessionChatHighlighter()` inside `use()` and reads/writes the cache. The
// language table lives in session-chat-code-languages.ts and the grammar
// chunks in session-chat-code-grammars.ts.
//
// Two deliberate choices:
//
//  1. ONE highlighter core, many grammars. The oniguruma wasm engine and both
//     themes are expensive; creating a fresh core per language would pay that
//     cost repeatedly. Each language instead gets a stable promise that
//     resolves to the shared core once that grammar is registered, which is
//     exactly the shape `use()` needs (stable identity per key).
//
//  2. DUAL-THEME output, not a JS theme signal. Session Chat's theme is a DOM
//     attribute (`data-chat-theme` on `.ghostex-session-chat-scope`, set by
//     session-chat-view.tsx), not React state that reaches the markdown
//     renderer. Shiki's `defaultColor: false` dual-theme mode emits
//     `--shiki-light` / `--shiki-dark` custom properties per token and no
//     inline `color`, so chat.css picks the side that matches the attribute.
//     Theme flips therefore cost zero re-highlighting and one cache entry
//     serves both themes. That is why the theme component of the cache key is
//     the theme *pair*.
//
// Host-neutral by construction: no gpui/web/mobile APIs are referenced here.

import type { HighlighterCore } from "shiki/core";
import { loadSessionChatGrammar } from "./session-chat-code-grammars";
import type { SessionChatCodeLanguage } from "./session-chat-code-languages";
import { createSessionChatHighlighterCore } from "./session-chat-shiki-engine";

export { resolveSessionChatCodeLanguage } from "./session-chat-code-languages";
export { SESSION_CHAT_HIGHLIGHTING_AVAILABLE } from "./session-chat-shiki-engine";
export type { SessionChatCodeLanguage } from "./session-chat-code-languages";

export const SESSION_CHAT_SHIKI_LIGHT_THEME = "github-light-default";
export const SESSION_CHAT_SHIKI_DARK_THEME = "github-dark-default";

/** Identifies the theme pair baked into every cached HTML string. */
const SESSION_CHAT_SHIKI_THEME_KEY = `${SESSION_CHAT_SHIKI_LIGHT_THEME}+${SESSION_CHAT_SHIKI_DARK_THEME}`;

// t3code's caps (apps/web/src/components/ChatMarkdown.tsx).
const MAX_HIGHLIGHT_CACHE_ENTRIES = 500;
const MAX_HIGHLIGHT_CACHE_MEMORY_BYTES = 50 * 1024 * 1024;

let corePromise: Promise<HighlighterCore> | null = null;

function highlighterCore(): Promise<HighlighterCore> {
  corePromise ??= createSessionChatHighlighterCore();
  return corePromise;
}

const highlighterByLanguage = new Map<string, Promise<HighlighterCore>>();

/**
 * Stable-per-language promise resolving to a core with that grammar loaded.
 * The identity must be stable because React's `use()` re-reads it on every
 * render of the suspended subtree.
 *
 * A rejected promise stays cached on purpose: a grammar that cannot be fetched
 * will not start fetching correctly on the next render, and retrying per render
 * would turn one failure into a request storm. The error boundary around the
 * block renders the plain `<pre>` instead.
 */
export function sessionChatHighlighter(
  language: SessionChatCodeLanguage,
): Promise<HighlighterCore> {
  const existing = highlighterByLanguage.get(language);
  if (existing) {
    return existing;
  }
  const ready = (async () => {
    const core = await highlighterCore();
    if (!core.getLoadedLanguages().includes(language)) {
      await core.loadLanguage(
        (await loadSessionChatGrammar(language)) as Parameters<
          HighlighterCore["loadLanguage"]
        >[0],
      );
    }
    return core;
  })();
  ready.catch((error: unknown) => {
    console.error(
      "[session-chat] Shiki grammar failed to load; code block stays plain",
      language,
      error,
    );
  });
  highlighterByLanguage.set(language, ready);
  return ready;
}

/**
 * Renders one fence to dual-theme HTML. Returns `null` when Shiki refuses the
 * input so the caller can keep the plain `<pre>` it already has.
 */
export function highlightSessionChatCode(
  core: HighlighterCore,
  code: string,
  language: SessionChatCodeLanguage,
): string | null {
  try {
    return core.codeToHtml(code, {
      defaultColor: false,
      lang: language,
      themes: {
        dark: SESSION_CHAT_SHIKI_DARK_THEME,
        light: SESSION_CHAT_SHIKI_LIGHT_THEME,
      },
      transformers: [
        {
          pre(node) {
            // Shiki marks its <pre> tabbable. Today's chat code blocks are not
            // tab stops, and gaining one only while a grammar happens to be
            // loaded would make keyboard order depend on network timing.
            delete node.properties.tabindex;
          },
        },
      ],
    });
  } catch (error) {
    console.warn(
      "[session-chat] Shiki highlight failed; code block stays plain",
      language,
      error,
    );
    return null;
  }
}

interface CacheEntry {
  approximateSize: number;
  value: string;
}

/** Insertion-ordered Map used as an LRU, bounded by entry count and bytes. */
class HighlightCache {
  private readonly entries = new Map<string, CacheEntry>();
  private totalSize = 0;

  constructor(
    private readonly maxEntries: number,
    private readonly maxMemoryBytes: number,
  ) {}

  get(key: string): string | null {
    const entry = this.entries.get(key);
    if (!entry) {
      return null;
    }
    // Re-insert so the most recently read key sorts last (least-recent first).
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }

  set(key: string, value: string, approximateSize: number): void {
    if (approximateSize > this.maxMemoryBytes) {
      return;
    }
    const existing = this.entries.get(key);
    if (existing) {
      this.totalSize -= existing.approximateSize;
      this.entries.delete(key);
    }
    while (
      this.entries.size > 0 &&
      (this.entries.size >= this.maxEntries ||
        this.totalSize + approximateSize > this.maxMemoryBytes)
    ) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) {
        break;
      }
      this.totalSize -= this.entries.get(oldest)?.approximateSize ?? 0;
      this.entries.delete(oldest);
    }
    this.entries.set(key, { approximateSize, value });
    this.totalSize += approximateSize;
  }
}

export const sessionChatHighlightCache = new HighlightCache(
  MAX_HIGHLIGHT_CACHE_ENTRIES,
  MAX_HIGHLIGHT_CACHE_MEMORY_BYTES,
);

function fnv1a32(value: string): number {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/** Content hash + length + language + theme pair, per the t3code shape. */
export function sessionChatHighlightCacheKey(
  code: string,
  language: string,
): string {
  return `${fnv1a32(code).toString(36)}:${code.length}:${language}:${SESSION_CHAT_SHIKI_THEME_KEY}`;
}

/** UTF-16 chars are 2 bytes; the DOM copy of the markup costs more again. */
export function estimateSessionChatHighlightSize(
  html: string,
  code: string,
): number {
  return Math.max(html.length * 2, code.length * 3);
}
