/**
 * CDXC:Git 2026-07-29:
 * Switching projects in the sidebar used to re-run the whole Git fan-out
 * (~10 gxserver RPCs, each spawning a subprocess, one of them a networked
 * `gh pr view`) every single time, because the runtime only remembered the
 * *last* project it refreshed. Switching A -> B -> A therefore paid the full
 * cost again and starved the terminal-attach RPCs sharing the same daemon.
 *
 * This module owns the pure caching policy for that fan-out: a bounded,
 * time-to-live keyed memo with least-recently-used eviction. It is deliberately
 * clock-free — every entry point takes `nowMs` — so the policy can be unit
 * tested without fake timers and so the runtime keeps a single source of time.
 */

/**
 * How long a computed local Git state stays publishable without re-probing.
 * Local `git` probes are cheap-ish but not free (one subprocess each), and a
 * working tree does not meaningfully change while the user is bouncing between
 * projects. 45s is short enough that a returning user sees near-live counters
 * and long enough that rapid project switching costs zero RPCs.
 */
export const SIDEBAR_GIT_STATE_MEMO_TTL_MS = 45 * 1000;

/**
 * How long GitHub CLI results (`gh --version`, `gh pr view`) stay publishable.
 * `gh pr view` is a network round trip with a 120s server-side timeout, and
 * pull-request state changes on a human timescale, so it gets a much longer
 * lease than the local working-tree probes.
 */
export const SIDEBAR_GIT_HUB_MEMO_TTL_MS = 5 * 60 * 1000;

/**
 * Upper bound on remembered projects. A long-lived sidebar session can visit
 * many projects; keeping the newest 50 bounds memory without ever evicting the
 * handful of projects a user actually switches between.
 */
export const SIDEBAR_GIT_MEMO_MAX_ENTRIES = 50;

type SidebarGitMemoEntry<Value> = {
  storedAtMs: number;
  value: Value;
};

/**
 * Bounded key/value memo with a fixed TTL and least-recently-used eviction.
 *
 * Recency is tracked by re-inserting on every read and write (a `Map` iterates
 * in insertion order), so the oldest *touched* entry is evicted first. Reading
 * an entry never extends its TTL: freshness is decided purely by when the value
 * was stored.
 */
export class SidebarGitTtlMemo<Value> {
  private readonly entries = new Map<string, SidebarGitMemoEntry<Value>>();
  private readonly maxEntries: number;
  private readonly ttlMs: number;

  constructor(options: { maxEntries?: number; ttlMs: number }) {
    this.maxEntries = Math.max(1, Math.trunc(options.maxEntries ?? SIDEBAR_GIT_MEMO_MAX_ENTRIES));
    this.ttlMs = Math.max(0, Math.trunc(options.ttlMs));
  }

  get size(): number {
    return this.entries.size;
  }

  /**
   * Fresh value for `key`, or `undefined` when the key is unknown or expired.
   * An expired entry is dropped so it cannot be revived by a later `peek`.
   */
  get(key: string, nowMs: number): Value | undefined {
    const entry = this.entries.get(key);
    if (!entry) {
      return undefined;
    }
    if (!this.isFresh(entry, nowMs)) {
      this.entries.delete(key);
      return undefined;
    }
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }

  /**
   * Last stored value for `key` regardless of age, without touching recency.
   *
   * This exists for the stale-while-revalidate GitHub path: showing the
   * previously observed pull-request badge while a fresh probe is in flight is
   * strictly better than blanking it, and it still issues zero RPCs on the
   * switch-critical path.
   */
  peek(key: string): Value | undefined {
    return this.entries.get(key)?.value;
  }

  /** True when `key` holds a value that is still within its TTL. */
  isFreshKey(key: string, nowMs: number): boolean {
    const entry = this.entries.get(key);
    return entry !== undefined && this.isFresh(entry, nowMs);
  }

  set(key: string, value: Value, nowMs: number): void {
    this.entries.delete(key);
    this.entries.set(key, { storedAtMs: nowMs, value });
    while (this.entries.size > this.maxEntries) {
      const oldestKey = this.entries.keys().next();
      if (oldestKey.done === true) {
        return;
      }
      this.entries.delete(oldestKey.value);
    }
  }

  delete(key: string): void {
    this.entries.delete(key);
  }

  clear(): void {
    this.entries.clear();
  }

  private isFresh(entry: SidebarGitMemoEntry<Value>, nowMs: number): boolean {
    return nowMs - entry.storedAtMs < this.ttlMs;
  }
}
