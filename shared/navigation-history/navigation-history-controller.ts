/**
 * CDXC:NavigationHistory 2026-08-19:
 * The client half of titlebar Back/Forward. gxserver owns the trail and the
 * cursor; this controller is the transport, the local mirror the titlebar reads
 * without ever blocking on a round trip, and the retry loop for trail stops that
 * no longer exist.
 *
 * Both hosts (gpui's CEF sidebar runtime and the web sidebar runtime) create one
 * of these and hand it two things: how to reach the owning daemon, and how to
 * activate an entry. Everything else — coalescing, debounce, in-flight
 * collapsing, the post-navigation suppression window — lives here so the two
 * apps cannot drift.
 *
 * Performance contract:
 * - `recordVisit` runs on every sidebar publish, which is frequent. An unchanged
 *   active target costs one string compare and returns; nothing is scheduled and
 *   no RPC is sent.
 * - A changed target is debounced, and only one visit is ever in flight; bursts
 *   collapse to the latest target instead of queueing a call per publish.
 * - Titlebars render from `getState()`, a cached object that is only replaced
 *   when the buttons would actually look different.
 */

import {
  createNavigationHistoryUiState,
  EMPTY_NAVIGATION_HISTORY_STATE,
  EMPTY_NAVIGATION_HISTORY_UI_STATE,
  NAVIGATION_HISTORY_NAVIGATE_ENDPOINT,
  NAVIGATION_HISTORY_READ_ENDPOINT,
  NAVIGATION_HISTORY_VISIT_ENDPOINT,
  navigationHistoryEntriesEqual,
  navigationHistoryEntryKey,
  navigationHistoryUiStatesEqual,
  normalizeNavigationHistoryEntry,
  normalizeNavigationHistoryState,
  type NavigationHistoryDirection,
  type NavigationHistoryEntry,
  type NavigationHistoryState,
  type NavigationHistoryUiState,
} from "./navigation-history-contract";

export type NavigationHistoryRpc = (
  path:
    | typeof NAVIGATION_HISTORY_READ_ENDPOINT
    | typeof NAVIGATION_HISTORY_VISIT_ENDPOINT
    | typeof NAVIGATION_HISTORY_NAVIGATE_ENDPOINT,
  params: Record<string, unknown>,
) => Promise<unknown>;

export type NavigationHistoryControllerOptions = {
  /** Per-client trail id, so two Ghostex clients on one daemon stay independent. */
  scopeId: string;
  /**
   * The daemon that owns the trail, or undefined while it is unreachable. Called
   * per request so a reconnect is picked up without re-creating the controller.
   */
  resolveRpc(): NavigationHistoryRpc | undefined;
  /**
   * Focus the entry. Return false when it cannot be resolved anymore (session
   * killed, project removed) — the controller then forgets that stop on the
   * daemon and keeps walking in the same direction.
   */
  activate(entry: NavigationHistoryEntry): boolean | Promise<boolean>;
  /** Called whenever the button state changes. */
  onStateChange?(state: NavigationHistoryUiState): void;
  /** Reported instead of thrown; navigation failures must not break the sidebar. */
  onError?(error: unknown): void;
  visitDebounceMs?: number;
};

const DEFAULT_VISIT_DEBOUNCE_MS = 150;
/**
 * How long a programmatic Back/Forward stays authoritative. The host activates
 * the target asynchronously and may publish intermediate states on the way
 * there; recording those would truncate the forward branch the user just walked
 * away from. Cleared as soon as the target is observed as active.
 */
const NAVIGATION_SETTLE_TIMEOUT_MS = 4_000;
/** Bounded so a trail of dead entries cannot spin. */
const MAX_NAVIGATION_ATTEMPTS = 12;

export class NavigationHistoryController {
  private readonly options: NavigationHistoryControllerOptions;
  private readonly listeners = new Set<() => void>();
  private state: NavigationHistoryState = EMPTY_NAVIGATION_HISTORY_STATE;
  private uiState: NavigationHistoryUiState = EMPTY_NAVIGATION_HISTORY_UI_STATE;

  private lastVisitKey: string | undefined;
  private lastVisitEntry: NavigationHistoryEntry | undefined;
  private pendingVisit: NavigationHistoryEntry | undefined;
  private pendingVisitReplacesCurrent = false;
  private visitTimeoutId: ReturnType<typeof setTimeout> | undefined;
  private visitInFlight = false;
  private visitPromise: Promise<void> | undefined;

  private navigating = false;
  private queuedDirection: NavigationHistoryDirection | undefined;
  private settleKey: string | undefined;
  private settleProjectId: string | undefined;
  private settleDeadline = 0;
  private disposed = false;

  constructor(options: NavigationHistoryControllerOptions) {
    this.options = options;
  }

  getState = (): NavigationHistoryUiState => this.uiState;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  dispose(): void {
    this.disposed = true;
    this.clearVisitTimeout();
    this.listeners.clear();
  }

  /** Pull the trail state after a (re)connect, so the buttons start correct. */
  async refresh(): Promise<void> {
    const rpc = this.options.resolveRpc();
    if (!rpc) {
      return;
    }
    try {
      const response = await rpc(NAVIGATION_HISTORY_READ_ENDPOINT, {
        scopeId: this.options.scopeId,
      });
      this.applyState(normalizeNavigationHistoryState(response));
    } catch (error) {
      this.options.onError?.(error);
    }
  }

  /**
   * The host's active project/session, called on every publish. `undefined`
   * (no project, gxserver down) leaves the trail untouched rather than pushing
   * an empty stop the user can never navigate back to.
   */
  recordVisit(entry: NavigationHistoryEntry | undefined): void {
    if (this.disposed || !entry) {
      return;
    }
    const key = navigationHistoryEntryKey(entry);

    if (this.settleKey !== undefined) {
      if (key === this.settleKey) {
        // The Back/Forward target became active. The daemon's cursor is already
        // there, so adopt it as the last recorded stop without a round trip.
        this.clearSettleWindow();
        this.lastVisitKey = key;
        this.lastVisitEntry = entry;
        return;
      }
      if (entry.projectId === this.settleProjectId) {
        /*
         * Landing REFINED the target: Back went to a project and the host then
         * focused a session inside it. Recording that as a new stop would
         * truncate the forward branch the user just walked away from, so it
         * updates the stop in place instead.
         */
        this.clearSettleWindow();
        this.lastVisitKey = key;
        this.lastVisitEntry = entry;
        this.pendingVisit = entry;
        this.pendingVisitReplacesCurrent = true;
        this.scheduleVisit();
        return;
      }
      if (Date.now() < this.settleDeadline) {
        return;
      }
      this.clearSettleWindow();
    }

    if (key === this.lastVisitKey && navigationHistoryEntriesEqual(entry, this.lastVisitEntry)) {
      return;
    }
    this.lastVisitKey = key;
    this.lastVisitEntry = entry;
    this.pendingVisit = entry;
    this.scheduleVisit();
  }

  async navigate(direction: NavigationHistoryDirection): Promise<void> {
    if (this.disposed) {
      return;
    }
    if (this.navigating) {
      // One queued click is plenty: it covers double-clicking Back while the
      // first hop is still activating, without letting a held key pile up.
      this.queuedDirection = direction;
      return;
    }
    this.navigating = true;
    this.publishUiState();
    try {
      await this.runNavigation(direction);
    } finally {
      this.navigating = false;
      this.publishUiState();
      const queued = this.queuedDirection;
      this.queuedDirection = undefined;
      if (queued) {
        void this.navigate(queued);
      }
    }
  }

  private async runNavigation(direction: NavigationHistoryDirection): Promise<void> {
    const rpc = this.options.resolveRpc();
    if (!rpc) {
      return;
    }
    // A queued visit would race the cursor move: flush it first so the daemon
    // walks back from where the user actually is.
    await this.flushPendingVisit();

    const forgetKeys: string[] = [];
    for (let attempt = 0; attempt < MAX_NAVIGATION_ATTEMPTS; attempt += 1) {
      let response: unknown;
      try {
        response = await rpc(NAVIGATION_HISTORY_NAVIGATE_ENDPOINT, {
          direction,
          scopeId: this.options.scopeId,
          ...(forgetKeys.length > 0 ? { forgetKeys } : {}),
        });
      } catch (error) {
        this.options.onError?.(error);
        return;
      }
      this.applyState(normalizeNavigationHistoryState(response));
      const target = normalizeNavigationHistoryEntry(
        (response as { target?: unknown } | undefined)?.target,
      );
      if (!target) {
        return;
      }
      const targetKey = navigationHistoryEntryKey(target);
      let activated = false;
      try {
        activated = await this.options.activate(target);
      } catch (error) {
        this.options.onError?.(error);
        activated = false;
      }
      if (activated) {
        this.settleKey = targetKey;
        this.settleProjectId = target.projectId;
        this.settleDeadline = Date.now() + NAVIGATION_SETTLE_TIMEOUT_MS;
        this.lastVisitKey = targetKey;
        this.lastVisitEntry = target;
        this.pendingVisit = undefined;
        this.pendingVisitReplacesCurrent = false;
        this.clearVisitTimeout();
        return;
      }
      forgetKeys.push(targetKey);
    }
  }

  private scheduleVisit(): void {
    if (this.visitTimeoutId !== undefined) {
      return;
    }
    this.visitTimeoutId = setTimeout(() => {
      this.visitTimeoutId = undefined;
      void this.flushPendingVisit();
    }, this.options.visitDebounceMs ?? DEFAULT_VISIT_DEBOUNCE_MS);
  }

  private clearSettleWindow(): void {
    this.settleKey = undefined;
    this.settleProjectId = undefined;
    this.settleDeadline = 0;
  }

  private clearVisitTimeout(): void {
    if (this.visitTimeoutId !== undefined) {
      clearTimeout(this.visitTimeoutId);
      this.visitTimeoutId = undefined;
    }
  }

  /**
   * Drain the pending visit, and resolve only once nothing is in flight. A
   * navigation awaits this: a visit that landed AFTER the cursor moved would
   * push the pre-navigation target back onto the trail and truncate the branch
   * the user just walked into.
   */
  private flushPendingVisit(): Promise<void> {
    this.clearVisitTimeout();
    if (this.visitInFlight) {
      return this.visitPromise ?? Promise.resolve();
    }
    if (!this.pendingVisit) {
      return Promise.resolve();
    }
    const rpc = this.options.resolveRpc();
    if (!rpc) {
      // Nothing to send to yet. Keep the entry pending and drop the memo of it
      // so the next publish retries once the daemon is back.
      this.lastVisitKey = undefined;
      this.lastVisitEntry = undefined;
      return Promise.resolve();
    }
    const entry = this.pendingVisit;
    const replaceCurrent = this.pendingVisitReplacesCurrent;
    this.pendingVisit = undefined;
    this.pendingVisitReplacesCurrent = false;
    this.visitInFlight = true;
    const promise = this.sendVisit(rpc, entry, replaceCurrent);
    this.visitPromise = promise;
    return promise;
  }

  private async sendVisit(
    rpc: NavigationHistoryRpc,
    entry: NavigationHistoryEntry,
    replaceCurrent: boolean,
  ): Promise<void> {
    try {
      const response = await rpc(NAVIGATION_HISTORY_VISIT_ENDPOINT, {
        entry,
        scopeId: this.options.scopeId,
        ...(replaceCurrent ? { replaceCurrent: true } : {}),
      });
      this.applyState(normalizeNavigationHistoryState(response));
    } catch (error) {
      this.lastVisitKey = undefined;
      this.lastVisitEntry = undefined;
      this.options.onError?.(error);
    } finally {
      this.visitInFlight = false;
      this.visitPromise = undefined;
    }
    if (this.pendingVisit) {
      await this.flushPendingVisit();
    }
  }

  private applyState(state: NavigationHistoryState): void {
    this.state = state;
    this.publishUiState();
  }

  private publishUiState(): void {
    const next = createNavigationHistoryUiState(this.state, this.navigating);
    if (navigationHistoryUiStatesEqual(next, this.uiState)) {
      return;
    }
    this.uiState = next;
    this.options.onStateChange?.(next);
    for (const listener of this.listeners) {
      listener();
    }
  }
}
