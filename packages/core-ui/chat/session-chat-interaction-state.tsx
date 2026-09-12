import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type SetStateAction,
} from 'react';

export interface SessionChatScrollSnapshot {
  top: number;
  anchorId: string | null;
  anchorOffset: number;
  followBottom: boolean;
  streamOnScreen: boolean;
  streamHoldReleased: boolean;
  readerScrolledInHold: boolean;
  fileNavigationActive: boolean;
  interactedMessageIds: string[];
}

interface InteractionState {
  disclosures: Record<string, { open: boolean; defaultOpen: boolean }>;
  scroll?: SessionChatScrollSnapshot;
  cursor?: { fingerprint: string; anchor: number; focus: number };
}

const STORAGE_PREFIX = 'ghostex.session-chat.interactions.v1:';
const MAX_SESSIONS = 32;
const states = new Map<string, InteractionState>();
const sessionKeys = new WeakMap<InteractionState, string>();
const savedValues = new Map<string, string | null>();

/** CDXC:SessionChat 2026-09-12 DECISION:
 * User: switching threads restores the reading position, expanded tools and composer cursor.
 * Keep this small interaction cache separate from transcript and durable draft storage so releasing a chat page does not release the reader's place.
 */
export function sessionChatInteractionState(sessionKey?: string): InteractionState {
  if (!sessionKey) return { disclosures: {} };
  let state = states.get(sessionKey);
  try {
    const saved = localStorage.getItem(STORAGE_PREFIX + sessionKey);
    if (!state || saved !== savedValues.get(sessionKey)) {
      const parsed = JSON.parse(saved ?? 'null');
      if (parsed?.state?.disclosures && typeof parsed.state.disclosures === 'object') state = parsed.state;
      savedValues.set(sessionKey, saved);
    }
  } catch {
    /* Unavailable browser storage starts a fresh in-page cache. */
  }
  state ??= { disclosures: {} };
  sessionKeys.set(state, sessionKey);
  states.delete(sessionKey);
  states.set(sessionKey, state);
  while (states.size > MAX_SESSIONS) {
    const oldest = states.keys().next().value!;
    states.delete(oldest);
    savedValues.delete(oldest);
  }
  return state;
}

export function persistSessionChatInteractions(state: InteractionState): void {
  const sessionKey = sessionKeys.get(state);
  if (!sessionKey) return;
  try {
    const storageKey = STORAGE_PREFIX + sessionKey;
    const isNew = localStorage.getItem(storageKey) === null;
    const saved = JSON.stringify({ updatedAt: Date.now(), state });
    localStorage.setItem(storageKey, saved);
    savedValues.set(sessionKey, saved);
    if (isNew) {
      const entries: { key: string; updatedAt: number }[] = [];
      for (let index = 0; index < localStorage.length; index++) {
        const key = localStorage.key(index);
        if (key?.startsWith(STORAGE_PREFIX))
          entries.push({ key, updatedAt: JSON.parse(localStorage.getItem(key) ?? '{}').updatedAt ?? 0 });
      }
      entries.sort((left, right) => right.updatedAt - left.updatedAt);
      for (const entry of entries.slice(MAX_SESSIONS)) localStorage.removeItem(entry.key);
    }
  } catch {
    /* The in-page cache still owns the active interactions. */
  }
}

const StateContext = createContext<InteractionState | null>(null);
const ScopeContext = createContext('');

export function SessionChatInteractionProvider({ state, children }: { state: InteractionState; children: ReactNode }) {
  return <StateContext value={state}>{children}</StateContext>;
}

export function SessionChatInteractionScope({ id, children }: { id: string; children: ReactNode }) {
  return <ScopeContext value={id}>{children}</ScopeContext>;
}

export function useSessionChatDisclosureState(name: string, defaultOpen: boolean) {
  const state = useContext(StateContext);
  const scope = useContext(ScopeContext);
  const key = useMemo(() => JSON.stringify([scope, name]), [name, scope]);
  const stored = state?.disclosures[key];
  const [open, setOpen] = useState(stored?.defaultOpen === defaultOpen ? stored.open : defaultOpen);
  const previousDefault = useRef(defaultOpen);
  const openRef = useRef(open);
  const update = useCallback(
    (next: SetStateAction<boolean>) => {
      const value = typeof next === 'function' ? next(openRef.current) : next;
      openRef.current = value;
      if (state) {
        state.disclosures[key] = { open: value, defaultOpen };
        const keys = Object.keys(state.disclosures);
        if (keys.length > 2000) delete state.disclosures[keys[0]!];
      }
      setOpen(value);
    },
    [defaultOpen, key, state]
  );
  useEffect(() => {
    if (previousDefault.current !== defaultOpen) {
      previousDefault.current = defaultOpen;
      update(defaultOpen);
    }
  }, [defaultOpen, update]);
  return [open, update] as const;
}

function draftFingerprint(value: string): string {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index++) hash = Math.imul(hash ^ value.charCodeAt(index), 16777619);
  return `${value.length}:${hash >>> 0}`;
}

export function readSessionChatCursor(sessionKey: string | undefined, value: string) {
  const cursor = sessionChatInteractionState(sessionKey).cursor;
  return cursor?.fingerprint === draftFingerprint(value) ? cursor : null;
}

export function saveSessionChatCursor(
  sessionKey: string | undefined,
  value: string,
  selection: { anchor: number; focus: number }
): void {
  if (!sessionKey) return;
  const state = sessionChatInteractionState(sessionKey);
  state.cursor = { fingerprint: draftFingerprint(value), anchor: selection.anchor, focus: selection.focus };
  persistSessionChatInteractions(state);
}
