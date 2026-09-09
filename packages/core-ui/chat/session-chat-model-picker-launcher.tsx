import { useEffect, useRef, useState, type RefObject } from 'react';
import { useSidebarStore } from '@/packages/core-ui/sidebar-store';
import {
  normalizeghostexHotkeySettings,
  ghostexHotkeyTextFromKeyboardEvent,
  getghostexHotkeyActionIdForKey,
} from '@/packages/shared/ghostex-hotkeys';
import { useAgentModelCatalog } from '@/packages/shared/agent-model-catalog-store';
import { createModelPickerRequest, modelPickerProvider } from './session-chat-model-picker-request';
import type { SessionChatSessionOptionPillsProps } from './session-chat-option-pills';
import type { SessionChatOptionDispatchReceipt } from './session-chat-option-state';
import type { SessionChatSelectionOptions } from '@/packages/shared/session-chat';
import {
  SessionChatModelPicker,
  type ModelPickerRequest,
  type ModelPickerSelection,
} from './session-chat-model-picker';

const deliveries = new Map<string, Promise<unknown>>();

export interface ModelPickerActions {
  open: () => void;
  select: (selection: ModelPickerSelection) => void;
  selectOptions: (options: SessionChatSelectionOptions) => void;
}
interface OutboxSelection extends ModelPickerSelection {
  id: string;
  options?: SessionChatSelectionOptions;
}

function readOutbox(key: string): OutboxSelection | null {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? 'null');
    return value && typeof value.id === 'string' && typeof value.model === 'string' && typeof value.effort === 'string'
      ? value
      : null;
  } catch {
    return null;
  }
}

/**
 * CDXC:SessionChat 2026-09-05 DECISION:
 * User: Option+P and model selection always work, including while the agent is working; an undeliverable selection waits for the next opportunity.
 * User: do not show the model/effort/queued status sentence in the chat box.
 * Opening uses the focused chat pane, not composer focus or current-value detection. The local outbox covers disconnects until gxserver accepts the durable intent.
 * SEE-ALSO: server/src/session_chat_model_selection.rs owns delivery, coalescing and retries after the client closes.
 */
export function SessionChatModelPickerLauncher(
  props: Pick<SessionChatSessionOptionPillsProps, 'controller' | 'onQueueModel' | 'pendingModelSelection'> & {
    actionsRef: RefObject<ModelPickerActions | null>;
  }
) {
  const catalog = useAgentModelCatalog();
  const storageKey = `ghostex.model-selection-outbox.${props.controller.sessionKey ?? ''}`;
  const [outbox, setOutbox] = useState<OutboxSelection | null>(() => readOutbox(storageKey));
  const [request, setRequest] = useState<ModelPickerRequest | null>(null);
  const [cancelRequested, setCancelRequested] = useState(false);
  const [container, setContainer] = useState<HTMLElement | null>(null);
  const anchor = useRef<HTMLSpanElement>(null);
  const requestRef = useRef<ModelPickerRequest | null>(null);
  const latest = useRef(props);
  const latestOutbox = useRef(outbox);
  const openedSession = useRef(props.controller.sessionKey);
  const receipt = useRef<{ id: string; value: SessionChatOptionDispatchReceipt } | null>(null);
  latest.current = props;
  latestOutbox.current = outbox;
  const desired = outbox ?? props.pendingModelSelection;

  useEffect(() => {
    setOutbox(readOutbox(storageKey));
    requestRef.current = null;
    setRequest(null);
    receipt.current = null;
  }, [storageKey]);

  useEffect(() => {
    if (desired) {
      if (receipt.current?.id !== desired.id) {
        // The next intent can cover different controls; release fields it no longer owns.
        receipt.current?.value.complete();
        receipt.current = {
          id: desired.id,
          value: props.controller.beginDispatch({
            ...(desired.model ? { model: desired.model } : {}),
            ...(desired.effort ? { effort: desired.effort } : {}),
            ...desired.options,
          }),
        };
      }
    } else if (receipt.current) {
      receipt.current.value.complete();
      receipt.current = null;
    }
  }, [desired, props.controller]);

  useEffect(() => {
    if (!outbox || !props.onQueueModel) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const deliver = async () => {
      try {
        const previous = deliveries.get(storageKey);
        const operation = (async () => {
          await previous?.catch(() => undefined);
          if (cancelled || latestOutbox.current?.id !== outbox.id) return;
          return props.onQueueModel!(outbox);
        })();
        deliveries.set(storageKey, operation);
        let accepted;
        try {
          accepted = await operation;
        } finally {
          if (deliveries.get(storageKey) === operation) deliveries.delete(storageKey);
        }
        if (!accepted) return;
        if (readOutbox(storageKey)?.id === outbox.id) localStorage.removeItem(storageKey);
        if (cancelled || latestOutbox.current?.id !== outbox.id) return;
        setOutbox(null);
      } catch {
        if (cancelled) return;
        timer = setTimeout(() => void deliver(), 5000);
      }
    };
    void deliver();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [outbox, props.onQueueModel, storageKey]);

  useEffect(() => {
    const open = () => {
      const current = latest.current;
      const provider = modelPickerProvider(current.controller.catalog?.modelIcon);
      if (requestRef.current || !provider) return;
      const pane = anchor.current?.closest<HTMLElement>('.ghostex-session-chat-scope');
      if (!pane) return;
      const desired = latestOutbox.current ?? current.pendingModelSelection;
      const next = createModelPickerRequest(
        catalog,
        provider,
        desired?.model || current.controller.state.model?.value,
        desired?.effort || current.controller.state.effort?.value
      );
      if (!next) return;
      delete document.documentElement.dataset.ghostexModelPickerRequested;
      openedSession.current = current.controller.sessionKey;
      requestRef.current = next;
      setCancelRequested(false);
      setContainer(pane);
      setRequest(next);
    };
    const toggle = () => {
      if (requestRef.current) {
        delete document.documentElement.dataset.ghostexModelPickerRequested;
        setCancelRequested(true);
      } else {
        open();
      }
    };
    const keydown = (event: KeyboardEvent) => {
      if (event.repeat || event.isComposing) return;
      const chord = ghostexHotkeyTextFromKeyboardEvent(event);
      const hotkeys = normalizeghostexHotkeySettings(useSidebarStore.getState().hud.settings?.hotkeys);
      if (!chord || getghostexHotkeyActionIdForKey(hotkeys, chord) !== 'openModelPicker') return;
      if (!requestRef.current) {
        const pane = anchor.current?.closest<HTMLElement>('.ghostex-session-chat-scope');
        if (!pane?.getClientRects().length || pane.closest('[aria-hidden="true"]')) return;
        const focusedPane = document.querySelector('.workspace-pane--focused');
        if (focusedPane && !focusedPane.contains(pane)) return;
        const inputPane = document.activeElement?.closest('.ghostex-session-chat-scope');
        if (!focusedPane && inputPane && inputPane !== pane) return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
      toggle();
    };
    const persist = (selection: ModelPickerSelection, options?: SessionChatSelectionOptions) => {
      const key = `ghostex.model-selection-outbox.${latest.current.controller.sessionKey ?? ''}`;
      const next = { ...selection, options: { ...latestOutbox.current?.options, ...options }, id: crypto.randomUUID() };
      try {
        localStorage.setItem(key, JSON.stringify(next));
      } catch {
        // Keep the in-memory intent until the connection accepts it.
      }
      latestOutbox.current = next;
      setOutbox(next);
    };
    props.actionsRef.current = {
      open,
      select: (selection) => persist(selection),
      selectOptions: (options) =>
        persist({ model: latestOutbox.current?.model ?? '', effort: latestOutbox.current?.effort ?? '' }, options),
    };
    window.addEventListener('keydown', keydown, true);
    window.addEventListener('ghostex-open-model-picker', toggle);
    if (document.documentElement.dataset.ghostexModelPickerRequested === 'true') open();
    return () => {
      props.actionsRef.current = null;
      window.removeEventListener('keydown', keydown, true);
      window.removeEventListener('ghostex-open-model-picker', toggle);
    };
  }, [catalog, props.actionsRef, props.controller.catalog?.modelIcon]);

  const save = (selection: ModelPickerSelection) => {
    requestRef.current = null;
    setRequest(null);
    if (openedSession.current !== props.controller.sessionKey) return;
    if (desired?.model === selection.model && desired.effort === selection.effort) return;
    if (
      !desired &&
      selection.model === props.controller.state.model?.value &&
      (selection.effort === (props.controller.state.effort?.value ?? '') ||
        request?.models.find((entry) => entry.value === selection.model)?.efforts.length === 0)
    )
      return;
    const next = { ...selection, options: latestOutbox.current?.options, id: crypto.randomUUID() };
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch {
      // Keep the in-memory intent until the connection accepts it.
    }
    latestOutbox.current = next;
    setOutbox(next);
  };
  return (
    <>
      <span ref={anchor} className='model-picker-launcher-anchor' />
      {request && container && (
        <SessionChatModelPicker
          key={request.requestId}
          request={request}
          container={container}
          cancelRequested={cancelRequested}
          onSave={save}
          onClose={() => {
            requestRef.current = null;
            setRequest(null);
          }}
        />
      )}
    </>
  );
}
