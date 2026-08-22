/*
 * Tiny localStorage-backed useState for control-panel UI preferences (which
 * sections are expanded, event-log filters, …). Deliberately separate from the
 * simulated persistence the engine owns (`ghostex.onboardingSandbox.*`): these
 * keys are inspector chrome only and must never influence the simulation.
 */
import { useCallback, useState } from "react";

const CONTROLS_STORAGE_PREFIX = "ghostex.onboardingSandbox.controls.";

function readPersisted<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(CONTROLS_STORAGE_PREFIX + key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function usePersistedState<T>(key: string, fallback: T): [T, (next: T) => void] {
  const [value, setValue] = useState<T>(() => readPersisted(key, fallback));
  const persist = useCallback(
    (next: T) => {
      setValue(next);
      try {
        window.localStorage.setItem(CONTROLS_STORAGE_PREFIX + key, JSON.stringify(next));
      } catch {
        /* private mode / quota — inspector prefs are disposable */
      }
    },
    [key],
  );
  return [value, persist];
}
