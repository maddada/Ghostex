import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ComponentType,
  type JSX,
} from 'react';

/**
 * CDXC:AppModal 2026-09-21 WHY:
 * Every app modal shares one page, so a modal imported eagerly is code every other modal (Settings above all) has to parse before its first paint; first-run onboarding alone was about 1 MB of it.
 * A modal wrapped here is its own on-demand file: it stays out of the page until it is first opened, then stays mounted so its own close handling keeps working.
 * The host measures compact dialogs and reports `presented` in the commit that opens them, which is before an on-demand modal exists, so it waits on `useLazyModalsSettled` instead.
 */
const loadingModals = new Set<symbol>();
const settledListeners = new Set<() => void>();

function setModalLoading(id: symbol, isLoading: boolean): void {
  if (isLoading === loadingModals.has(id)) {
    return;
  }
  if (isLoading) {
    loadingModals.add(id);
  } else {
    loadingModals.delete(id);
  }
  for (const listener of settledListeners) {
    listener();
  }
}

function subscribeLazyModalsSettled(listener: () => void): () => void {
  settledListeners.add(listener);
  return () => settledListeners.delete(listener);
}

export function areLazyModalsSettled(): boolean {
  return loadingModals.size === 0;
}

/** Re-renders the host when an opened on-demand modal finishes loading; read `areLazyModalsSettled` inside effects. */
export function useLazyModalsSettled(): boolean {
  return useSyncExternalStore(subscribeLazyModalsSettled, areLazyModalsSettled);
}

/**
 * Dialog portals mount their content in a second synchronous pass after their first commit. A modal that was always mounted had done that long before it opened; an on-demand modal first mounts while opening, so report it one pass later, when the dialog the host measures is in the document.
 */
function MountedSignal({ onMounted }: { onMounted: () => void }) {
  const [hasCommitted, setHasCommitted] = useState(false);
  useLayoutEffect(() => {
    if (hasCommitted) {
      onMounted();
    } else {
      setHasCommitted(true);
    }
  }, [hasCommitted, onMounted]);
  return null;
}

/** An on-demand modal; `preload` fetches its code ahead of the first open, which the warm spare window does for Settings. */
export type LazyModalComponent<Props> = ComponentType<Props> & { preload: () => void };

function createLazyModal<Props extends object>(
  load: () => Promise<ComponentType<Props>>,
  isWanted: (props: Props) => boolean
): LazyModalComponent<Props> {
  /*
   * CDXC:AppModal 2026-09-21 WHY:
   * Not React.lazy with Suspense: React holds content that resolves behind a Suspense fallback for about 300 ms so fallbacks do not flicker, which made every on-demand modal open 300 ms late although its files load in about 15 ms.
   */
  let loadedComponent: ComponentType<Props> | undefined;
  let loadError: unknown;
  let loading: Promise<void> | undefined;
  const ensureLoading = (): Promise<void> => {
    loading ??= load().then(
      (component) => {
        loadedComponent = component;
      },
      (error: unknown) => {
        loadError = error ?? new Error('Could not load this dialog.');
      }
    );
    return loading;
  };
  function LazyModal(props: Props) {
    const idRef = useRef(Symbol('lazy-modal'));
    const [wasWanted, setWasWanted] = useState(() => isWanted(props));
    const [isMounted, setIsMounted] = useState(false);
    const [, setLoadRevision] = useState(0);
    const markMounted = useCallback(() => setIsMounted(true), []);
    if (!wasWanted && isWanted(props)) {
      setWasWanted(true);
    }
    useLayoutEffect(() => {
      const id = idRef.current;
      setModalLoading(id, wasWanted && !isMounted);
      return () => setModalLoading(id, false);
    }, [isMounted, wasWanted]);
    useLayoutEffect(() => {
      if (!wasWanted || loadedComponent) {
        return;
      }
      let isCurrent = true;
      void ensureLoading().then(() => {
        if (isCurrent) {
          setLoadRevision((revision) => revision + 1);
        }
      });
      return () => {
        isCurrent = false;
      };
    }, [wasWanted]);
    if (loadError) {
      throw loadError;
    }
    const LoadedComponent = loadedComponent;
    if (!wasWanted || !LoadedComponent) {
      return null;
    }
    return (
      <>
        <LoadedComponent {...(props as Props & JSX.IntrinsicAttributes)} />
        <MountedSignal onMounted={markMounted} />
      </>
    );
  }
  return Object.assign(LazyModal, { preload: () => void ensureLoading() });
}

/** For modals that are always rendered and driven by `isOpen`. */
export function lazyModal<Props extends { isOpen: boolean }>(
  load: () => Promise<ComponentType<Props>>
): LazyModalComponent<Props> {
  return createLazyModal(load, (props) => props.isOpen);
}

/** For modals the host renders only while they are open. */
export function lazyRenderedModal<Props extends object>(
  load: () => Promise<ComponentType<Props>>
): LazyModalComponent<Props> {
  return createLazyModal(load, () => true);
}
