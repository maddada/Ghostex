import { useMemo, useSyncExternalStore } from 'react';
import {
  applySessionChatDetectedOptions,
  readStoredSessionChatOptions,
  reconcileSessionChatOptionsFromCommand,
  seedSessionChatOptionState,
  SESSION_CHAT_DISPATCH_GRACE_MS,
  writeStoredSessionChatOptions,
  type SessionChatDetectedOptionInput,
  type SessionChatOptionState,
  type SessionChatSessionOptionCatalog,
} from './session-chat-session-options';

export interface SessionChatOptionDispatchReceipt {
  complete: () => void;
  rollback: () => void;
}

interface PendingOptionChange {
  values: Readonly<Record<string, string>>;
  previous: SessionChatOptionState;
  startedAt: number;
  deliveredAt?: number;
}

/**
 * CDXC:AgentScreenDetection 2026-09-05 DECISION:
 * User: model, effort, Fast and Plan selections update optimistically, then reconcile with the agent; this supersedes waiting for footer detection before showing Fast or Plan.
 * Each change owns only its fields, so an old failure cannot roll back a newer selection or another session.
 */
export function createSessionChatOptionState(
  catalog: SessionChatSessionOptionCatalog | null,
  storageKey: string | undefined,
  onUnconfirmed: () => void
) {
  let state: SessionChatOptionState = catalog
    ? seedSessionChatOptionState(catalog, readStoredSessionChatOptions(storageKey))
    : {};
  let detectedState: SessionChatOptionState = {};
  const pending = new Map<string, PendingOptionChange>();
  for (const [id, value] of Object.entries(state)) {
    const dispatchedAt = Date.parse(value.dispatchedAt ?? '');
    if (value.source === 'dispatched' && Number.isFinite(dispatchedAt)) {
      pending.set(id, {
        values: { [id]: value.value },
        previous: {},
        startedAt: dispatchedAt,
        deliveredAt: dispatchedAt,
      });
    }
  }
  const listeners = new Set<() => void>();
  let expiryTimer: ReturnType<typeof setTimeout> | undefined;

  const publish = (next: SessionChatOptionState): void => {
    if (state === next) return;
    state = next;
    writeStoredSessionChatOptions(storageKey, next);
    for (const listener of listeners) listener();
  };

  const scheduleExpiry = (): void => {
    clearTimeout(expiryTimer);
    expiryTimer = undefined;
    if (listeners.size === 0) return;
    const deadlines = [...pending.values()].flatMap((change) =>
      change.deliveredAt === undefined ? [] : [change.deliveredAt + SESSION_CHAT_DISPATCH_GRACE_MS]
    );
    if (deadlines.length === 0) return;
    expiryTimer = setTimeout(
      () => {
        const next = { ...state };
        let expired = false;
        for (const [id, change] of pending) {
          if (change.deliveredAt === undefined || Date.now() < change.deliveredAt + SESSION_CHAT_DISPATCH_GRACE_MS) {
            continue;
          }
          pending.delete(id);
          const detected = detectedState[id];
          // An unchanged footer is still fresh evidence after a rejected CLI toggle.
          if (detected?.detectedAt && Date.parse(detected.detectedAt) >= change.startedAt) {
            next[id] = detected;
          } else {
            delete next[id];
          }
          expired = true;
        }
        if (expired) {
          publish(next);
          onUnconfirmed();
        }
        scheduleExpiry();
      },
      Math.max(0, Math.min(...deadlines) - Date.now())
    );
  };

  const beginDispatch = (values: Readonly<Record<string, string>>): SessionChatOptionDispatchReceipt => {
    const change: PendingOptionChange = { values, previous: state, startedAt: Date.now() };
    let next = state;
    for (const [id, value] of Object.entries(values)) {
      pending.set(id, change);
      next = {
        ...next,
        [id]: { value, source: 'dispatched', dispatchedAt: new Date(change.startedAt).toISOString() },
      };
    }
    publish(next);
    scheduleExpiry();
    return {
      complete: () => {
        change.deliveredAt = Date.now();
        const next = { ...state };
        for (const id of Object.keys(values)) {
          if (pending.get(id) === change && next[id]?.source === 'dispatched') {
            next[id] = { ...next[id], dispatchedAt: new Date(change.deliveredAt).toISOString() };
          }
        }
        publish(next);
        scheduleExpiry();
      },
      rollback: () => {
        const next = { ...state };
        for (const id of Object.keys(values)) {
          if (pending.get(id) !== change) continue;
          pending.delete(id);
          const previous = detectedState[id] ?? change.previous[id];
          if (previous) next[id] = previous;
          else delete next[id];
        }
        publish(next);
        scheduleExpiry();
      },
    };
  };

  return {
    getSnapshot: () => state,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      scheduleExpiry();
      return () => {
        listeners.delete(listener);
        scheduleExpiry();
      };
    },
    beginDispatch,
    recordDispatched: (id: string, value: string): void => {
      beginDispatch({ [id]: value }).complete();
    },
    reconcileTypedCommand: (text: string): void => {
      if (!catalog) return;
      const next = reconcileSessionChatOptionsFromCommand(catalog, state, text);
      const changed = Object.fromEntries(
        Object.entries(next)
          .filter(([id, entry]) => entry !== state[id])
          .map(([id, entry]) => [id, entry.value])
      );
      if (Object.keys(changed).length > 0) beginDispatch(changed).complete();
    },
    applyDetected: (detected: SessionChatDetectedOptionInput | null | undefined): void => {
      if (!catalog || !detected) return;
      detectedState = applySessionChatDetectedOptions(catalog, detectedState, detected);
      const next = { ...applySessionChatDetectedOptions(catalog, state, detected) };
      for (const [id, change] of pending) {
        const actual = detectedState[id];
        const fresh = actual?.detectedAt !== undefined && Date.parse(actual.detectedAt) >= change.startedAt;
        if (fresh && actual.value === change.values[id]) {
          next[id] = actual;
          pending.delete(id);
        } else if (state[id]) {
          next[id] = state[id];
        }
      }
      publish(next);
      scheduleExpiry();
    },
  };
}

export function useSessionChatOptionState(
  catalog: SessionChatSessionOptionCatalog | null,
  storageKey: string | undefined,
  onUnconfirmed: () => void
) {
  const store = useMemo(
    () => createSessionChatOptionState(catalog, storageKey, onUnconfirmed),
    [catalog, storageKey, onUnconfirmed]
  );
  const state = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  return { ...store, state };
}
