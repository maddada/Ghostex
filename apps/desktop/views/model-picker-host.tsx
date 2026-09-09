import { createRoot } from 'react-dom/client';
import { useEffect, useRef, useState } from 'react';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import {
  SessionChatModelPicker,
  type ModelPickerRequest,
  type ModelPickerProvider,
  type ModelPickerSelection,
} from '@/packages/core-ui/chat/session-chat-model-picker';
import {
  createModelPickerRequest,
  modelPickerProvider,
} from '@/packages/core-ui/chat/session-chat-model-picker-request';
import { currentAgentModelCatalog, useAgentModelCatalog } from '@/packages/shared/agent-model-catalog-store';
import {
  ghostexHotkeyTextFromKeyboardEvent,
  getghostexHotkeyActionIdForKey,
  normalizeghostexHotkeySettings,
} from '@/packages/shared/ghostex-hotkeys';
import type { GxserverReadSessionChatResult } from '@/packages/shared/session-chat';
import type { GxserverSelectSessionChatModelResult } from '@/packages/shared/gxserver-protocol';
import './model-picker-host.css';

interface PickerOpen {
  type: 'open';
  modal: 'modelPicker';
  provider: ModelPickerProvider;
  projectId: string;
  sessionId: string;
  hotkeys: unknown;
  connection: { baseUrl: string; authToken: string; protocolVersion: number };
}

function close() {
  postAppModalHostMessage({ type: 'close' }, 'ModelPicker:close');
}

async function request<T>(context: PickerOpen, endpoint: string, params: object, signal?: AbortSignal): Promise<T> {
  const { baseUrl, authToken, protocolVersion } = context.connection;
  const response = await fetch(`${baseUrl}${endpoint}`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${authToken}`,
      'content-type': 'application/json',
      'x-gxserver-protocol-version': String(protocolVersion),
    },
    body: JSON.stringify({
      protocolVersion,
      params: { projectId: context.projectId, sessionId: context.sessionId, ...params },
    }),
    signal: signal ?? AbortSignal.timeout(15000),
  });
  const envelope = (await response.json()) as { ok: boolean; result: T; error?: { message?: string } };
  if (!response.ok || !envelope.ok) throw new Error(envelope.error?.message ?? 'Could not reach the session.');
  return envelope.result;
}

function cacheKey(context: PickerOpen) {
  return `ghostex.terminal-model-picker.${JSON.stringify([context.connection.baseUrl, context.projectId, context.sessionId])}`;
}
function readSelection(context: PickerOpen): Partial<ModelPickerSelection> {
  try {
    const value = JSON.parse(localStorage.getItem(cacheKey(context)) ?? 'null');
    return value && typeof value.model === 'string' && typeof value.effort === 'string' ? value : {};
  } catch {
    return {};
  }
}
function remember(context: PickerOpen, value: Partial<ModelPickerSelection>) {
  try {
    localStorage.setItem(cacheKey(context), JSON.stringify(value));
  } catch {
    /* Cache is optional. */
  }
}

/**
 * CDXC:AppModal 2026-09-08 WHY:
 * The terminal picker uses the Settings native window lifecycle, but a dedicated small entry keeps Settings, transcript parsing and editor bundles out of the opening path.
 * The bundled/cached model catalog paints immediately; refreshing detected values must not move the cursor after the user starts choosing.
 */
function ModelPickerHost() {
  useAgentModelCatalog();
  const [context, setContext] = useState<PickerOpen>();
  const [picker, setPicker] = useState<ModelPickerRequest>();
  const [cancelRequested, setCancelRequested] = useState(false);
  const [error, setError] = useState<string>();
  const interacted = useRef(false);
  const current = useRef(context);
  current.current = context;

  useEffect(() => {
    const receive = (event: Event) => {
      const message = (event as CustomEvent<PickerOpen>).detail;
      if (message?.type !== 'open' || message.modal !== 'modelPicker') return;
      if (!modelPickerProvider(message.provider)) return;
      interacted.current = false;
      setCancelRequested(false);
      setError(undefined);
      const selected = readSelection(message);
      setContext(message);
      setPicker(
        createModelPickerRequest(currentAgentModelCatalog(), message.provider, selected.model, selected.effort)
      );
    };
    const markInteraction = () => {
      interacted.current = true;
    };
    const keydown = (event: KeyboardEvent) => {
      if (event.repeat || event.isComposing || !current.current) return;
      const chord = ghostexHotkeyTextFromKeyboardEvent(event);
      if (
        !chord ||
        getghostexHotkeyActionIdForKey(normalizeghostexHotkeySettings(current.current.hotkeys), chord) !==
          'openModelPicker'
      )
        return;
      event.preventDefault();
      event.stopImmediatePropagation();
      setCancelRequested(true);
    };
    window.addEventListener('ghostex-app-modal-host-message', receive);
    window.addEventListener('keydown', keydown, true);
    window.addEventListener('keydown', markInteraction, true);
    window.addEventListener('pointerdown', markInteraction, true);
    window.addEventListener('wheel', markInteraction, true);
    postAppModalHostMessage(
      { type: 'ready', nativeWindowHostId: window.__ghostex_APP_MODAL_HOST_ID__ },
      'ModelPicker:ready'
    );
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', receive);
      window.removeEventListener('keydown', keydown, true);
      window.removeEventListener('keydown', markInteraction, true);
      window.removeEventListener('pointerdown', markInteraction, true);
      window.removeEventListener('wheel', markInteraction, true);
    };
  }, []);

  useEffect(() => {
    if (!context) return;
    postAppModalHostMessage({ type: 'presented', modal: 'modelPicker' }, 'ModelPicker:presented');
    const abort = new AbortController();
    void request<GxserverReadSessionChatResult>(context, '/api/readSessionChat', { limit: 1 }, abort.signal)
      .then((result) => {
        const selection = {
          model: result.pendingModelSelection?.model || result.selectedOptions?.model?.value,
          effort: result.pendingModelSelection?.effort || result.selectedOptions?.effort?.value,
        };
        if (abort.signal.aborted || !selection.model) return;
        remember(context, selection);
        if (!interacted.current)
          setPicker(
            createModelPickerRequest(currentAgentModelCatalog(), context.provider, selection.model, selection.effort)
          );
      })
      .catch(() => {
        /* Detection is not required to open or choose a model. */
      });
    return () => abort.abort();
  }, [context]);

  const save = async (selection: ModelPickerSelection) => {
    if (!context) return;
    interacted.current = true;
    setError(undefined);
    try {
      const result = await request<GxserverSelectSessionChatModelResult>(context, '/api/selectSessionChatModel', {
        ...selection,
        defer: true,
      });
      if (!result.queued || !result.pendingModelSelection)
        throw new Error('The server has not accepted the selection.');
      remember(context, selection);
    } catch (error) {
      if (current.current === context) {
        setError(error instanceof Error ? error.message : 'Could not save the selection.');
        setCancelRequested(false);
      }
      throw error;
    }
  };

  return picker ? (
    <SessionChatModelPicker
      key={picker.requestId}
      request={picker}
      container={document.getElementById('root')!}
      cancelRequested={cancelRequested}
      onCommit={save}
      onSave={close}
      onClose={close}
      // CDXC:SessionChat 2026-09-08 DECISION:
      // User: do not show "Saving selection" at the top of the picker.
      notice={
        error ? (
          <p className='model-picker-host-status' role='alert'>
            {error} Press Enter to retry or Escape to cancel.
          </p>
        ) : undefined
      }
    />
  ) : null;
}

createRoot(document.getElementById('root')!).render(<ModelPickerHost />);
