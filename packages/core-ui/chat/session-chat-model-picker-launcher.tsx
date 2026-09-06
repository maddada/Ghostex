import { useEffect, useRef, useState, type RefObject } from 'react';
import { useSidebarStore } from '@/packages/core-ui/sidebar-store';
import {
  normalizeghostexHotkeySettings,
  ghostexHotkeyTextFromKeyboardEvent,
  getghostexHotkeyActionIdForKey,
} from '@/packages/shared/ghostex-hotkeys';
import { useAgentModelCatalog } from '@/packages/shared/agent-model-catalog-store';
import { agentModelCatalogEffortLabel } from '@/packages/shared/agent-model-catalog';
import type { SessionChatSessionOptionPillsProps } from './session-chat-option-pills';
import type { SessionChatOptionDispatchReceipt } from './session-chat-option-state';
import {
  SessionChatModelPicker,
  type ModelPickerRequest,
  type ModelPickerSelection,
} from './session-chat-model-picker';

const deliveries = new Map<string, Promise<unknown>>();

const SHORT_MODEL_LABELS: Record<string, string> = {
  'gpt-6-astra': 'Astra',
  'gpt-5.6-sol': 'Sol',
  'gpt-5.6-terra': 'Terra',
  'gpt-5.6-luna': 'Luna',
  fable: 'Fable',
  'opus[1m]': 'Opus (1m)',
  opus: 'Opus',
  sonnet: 'Sonnet',
  haiku: 'Haiku',
};
export interface ModelPickerActions {
  open: () => void;
  select: (selection: ModelPickerSelection) => void;
}
interface OutboxSelection extends ModelPickerSelection {
  id: string;
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
        receipt.current = {
          id: desired.id,
          value: props.controller.beginDispatch({
            model: desired.model,
            ...(desired.effort ? { effort: desired.effort } : {}),
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
      const provider = current.controller.catalog?.modelIcon;
      if (requestRef.current || (provider !== 'codex' && provider !== 'claude')) return;
      const pane = anchor.current?.closest<HTMLElement>('.ghostex-session-chat-scope');
      if (!pane) return;
      const agent = catalog.agents[provider];
      const models = agent.models
        .filter((model) => !model.group)
        .map((model) => ({
          value: model.value,
          label: SHORT_MODEL_LABELS[model.value] ?? model.label,
          version: provider === 'codex' ? model.label.replace(/\s+(Astra|Sol|Terra|Luna)$/, '') : undefined,
          efforts: model.efforts.map((value) => ({ value, label: agentModelCatalogEffortLabel(catalog, value) })),
          defaultEffort: model.defaultEffort ?? agent.defaultEffort,
        }));
      const desired = latestOutbox.current ?? current.pendingModelSelection;
      const selectedModel = desired?.model ?? current.controller.state.model?.value;
      // Detection may not have arrived yet. The catalog default is a starting cursor, not a claim about the running agent.
      const model =
        models.find((entry) => entry.value === selectedModel) ??
        models.find((entry) => entry.value === agent.models.find((model) => model.default)?.value) ??
        models[0];
      if (!model) return;
      const selectedEffort = desired?.effort ?? current.controller.state.effort?.value;
      const effort =
        model.efforts.find((entry) => entry.value === selectedEffort)?.value ??
        model.efforts.find((entry) => entry.value === model.defaultEffort)?.value ??
        model.efforts[0]?.value ??
        '';
      const efforts = agent.efforts.map((value) => ({ value, label: agentModelCatalogEffortLabel(catalog, value) }));
      const next: ModelPickerRequest = {
        requestId: crypto.randomUUID(),
        provider,
        models,
        efforts,
        model: model.value,
        effort,
      };
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
    props.actionsRef.current = {
      open,
      select: (selection) => {
        const key = `ghostex.model-selection-outbox.${latest.current.controller.sessionKey ?? ''}`;
        const next = { ...selection, id: crypto.randomUUID() };
        try {
          localStorage.setItem(key, JSON.stringify(next));
        } catch {
          // Keep the in-memory intent until the connection accepts it.
        }
        setOutbox(next);
      },
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
    const next = { ...selection, id: crypto.randomUUID() };
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch {
      // Keep the in-memory intent until the connection accepts it.
    }
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
