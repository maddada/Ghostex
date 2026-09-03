import { useCallback, useEffect, useState } from 'react';

import { DelayedSendModal } from '@/packages/core-ui/delayed-send-modal';
import type { GxserverRpcEndpointPath } from '@/packages/shared/gxserver-protocol';
import { rpcForMachine } from '../connections/connection-registry';
import { parseSidebarSessionId } from '../sidebar-runtime/sidebar-ids';
import type { OpenDelayedActionsModalDetail } from './action-events';

/*
CDXC:DelayedSend 2026-08-19:
Delayed Send is a first-class daemon endpoint: gxserver owns the clock, the
activity watcher, and the eventual Enter, so it no longer accepts
`scheduleDelayedSend`/`cancelDelayedSend` as renderer commands. Close After
Done is still renderer-owned and keeps the renderer-command route.
*/
type DelayedActionRendererCommand = 'toggleCloseAfterDone';

export function DelayedActionsModalHost() {
  const [detail, setDetail] = useState<OpenDelayedActionsModalDetail>();

  useEffect(() => {
    const open = (event: WindowEventMap['ghostex-web:openDelayedActionsModal']) => {
      setDetail(event.detail);
    };
    const close = () => setDetail(undefined);
    window.addEventListener('ghostex-web:openDelayedActionsModal', open);
    window.addEventListener('ghostex-web:closeAppModal', close);
    return () => {
      window.removeEventListener('ghostex-web:openDelayedActionsModal', open);
      window.removeEventListener('ghostex-web:closeAppModal', close);
    };
  }, []);

  const close = useCallback(() => setDetail(undefined), []);

  const request = useCallback(
    (label: string, path: GxserverRpcEndpointPath, params: Record<string, unknown> = {}) => {
      if (!detail) {
        return;
      }
      const target = parseSidebarSessionId(detail.sessionId);
      if (!target) {
        console.warn('[ghostex-web] Ignoring Session Automations for an invalid session id.');
        return;
      }
      void rpcForMachine(target.machineId, path, {
        ...params,
        projectId: target.projectId,
        sessionId: target.sessionId,
      }).catch((error: unknown) => {
        console.error(`[ghostex-web] Session Automations ${label} failed:`, error);
      });
    },
    [detail]
  );

  const dispatch = useCallback(
    (action: DelayedActionRendererCommand, payload: Record<string, unknown> = {}) => {
      if (!detail) {
        return;
      }
      const target = parseSidebarSessionId(detail.sessionId);
      if (!target) {
        console.warn('[ghostex-web] Ignoring Session Automations for an invalid session id.');
        return;
      }
      void rpcForMachine(target.machineId, '/api/dispatchRendererCommand', {
        action,
        payload: {
          ...payload,
          projectId: target.projectId,
          sessionId: target.sessionId,
        },
      }).catch((error: unknown) => {
        console.error(`[ghostex-web] Session Automations ${action} failed:`, error);
      });
    },
    [detail]
  );

  return (
    <DelayedSendModal
      agentIcon={detail?.agentIcon}
      closeAfterDoneActive={detail?.closeAfterDoneActive}
      delayedSendDeadlineAt={detail?.delayedSendDeadlineAt}
      delayedSendRemainingLabel={detail?.delayedSendRemainingLabel}
      isOpen={detail !== undefined}
      onCancel={close}
      onCancelTimer={() => {
        request('cancelDelayedSend', '/api/cancelDelayedSend');
        close();
      }}
      onConfirm={(delayMs, sendWhenAgentStops, sendWhenAllProjectSessionsStop) => {
        /*
        Exactly one trigger reaches the daemon: the modal reports `delayMs` only
        for "After a delay", and the two status triggers are mutually exclusive.
        */
        request('scheduleDelayedSend', '/api/scheduleDelayedSend', {
          ...(delayMs === undefined ? {} : { delayMs }),
          ...(sendWhenAllProjectSessionsStop ? { sendWhenAllProjectSessionsStop: true } : {}),
          ...(sendWhenAgentStops ? { sendWhenAgentStops: true } : {}),
        });
        close();
      }}
      onToggleCloseAfterDone={() => {
        dispatch('toggleCloseAfterDone');
        close();
      }}
      sendWhenAllProjectSessionsStopActive={detail?.sendWhenAllProjectSessionsStopActive}
      sendWhenAgentStopsActive={detail?.sendWhenAgentStopsActive}
      sessionTitle={detail?.title}
      supportsSendWhenAgentStops
      supportsSendWhenAllProjectSessionsStop
    />
  );
}
