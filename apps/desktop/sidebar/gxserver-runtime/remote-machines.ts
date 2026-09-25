/*
CDXC:RepoStructure 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.

CDXC:RemoteMachines 2026-09-25 WHY:
What is left is this runtime's remote request channel, which only the remote Project Group and
Space edits still use (they wait on the user: CDXC:RemoteMachines 2026-09-21 DECISION in
apps/desktop/src/app/gx_store/remote_project_docs.rs). The runtime's copy of the machines'
presentations, its stale refetch and its reconnect are Rust's.
*/
import type { GpuiSidebarRuntime } from './core';
import type { GpuiSidebarRemoteGxserverResponseEvent } from './types-and-protocol';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import type { GxserverEndpointPath } from '@/packages/shared/gxserver-protocol';

/*
CDXC:RepoStructure 2026-08-22:
The method signatures below are copied verbatim from the original class body.
They exist as a standalone interface — rather than being derived from
`typeof gpuiSidebarRuntimeRemoteMachineMethods` — because deriving them would make
`GpuiSidebarRuntime` depend on the bodies that depend on it, which TypeScript
reports as a circular base type. `gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck`
at the bottom of this file is what keeps the two in step.
*/
export interface GpuiSidebarRuntimeRemoteMachineMethods {
  requestRemoteGxserver<TResult = unknown>(
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
    options?: { timeoutMs?: number }
  ): Promise<TResult>;
  resolveRemoteGxserverRequest(event: GpuiSidebarRemoteGxserverResponseEvent): void;
}

export const gpuiSidebarRuntimeRemoteMachineMethods = {
  requestRemoteGxserver<TResult = unknown>(
    this: GpuiSidebarRuntime,
    remoteMachineId: string,
    path: GxserverEndpointPath,
    params: Record<string, unknown>,
    options: { timeoutMs?: number } = {}
  ): Promise<TResult> {
    const requestId = `remote-${Date.now().toString(36)}-${++this.remoteGxserverRequestSequence}`;
    const timeoutMs = Math.min(Math.max(options.timeoutMs ?? 20_000, 1_000), 130_000);
    return new Promise<TResult>((resolve, reject) => {
      const timeoutId = window.setTimeout(() => {
        this.pendingRemoteGxserverRequests.delete(requestId);
        reject(new Error('Remote gxserver request timed out.'));
      }, timeoutMs + 2_000);
      this.pendingRemoteGxserverRequests.set(requestId, {
        reject,
        resolve: (result) => resolve(result as TResult),
        timeoutId,
      });
      try {
        /*
        CDXC:RemoteMachines 2026-06-24-17:19:
        Response-capable remote sidebar RPCs still carry only a bounded request id plus the allowlisted endpoint params into Rust. Rust owns the live tunnel, token, endpoint allowlist, response sanitization, and presentation refresh; renderer code must not receive tokens, SSH details, command text, URLs, or raw daemon bodies.
        */
        postAppModalHostMessage(
          {
            params,
            path,
            remoteMachineId,
            requestId,
            timeoutMs,
            type: 'gpuiRemoteGxserverSidebarRequest',
          },
          'GPUISidebarRemoteMachines:request'
        );
      } catch (error) {
        window.clearTimeout(timeoutId);
        this.pendingRemoteGxserverRequests.delete(requestId);
        reject(error instanceof Error ? error : new Error('Remote gxserver bridge failed.'));
      }
    });
  },

  resolveRemoteGxserverRequest(this: GpuiSidebarRuntime, event: GpuiSidebarRemoteGxserverResponseEvent): void {
    const pending = this.pendingRemoteGxserverRequests.get(event.requestId);
    if (!pending) {
      return;
    }
    window.clearTimeout(pending.timeoutId);
    this.pendingRemoteGxserverRequests.delete(event.requestId);
    if (event.ok) {
      pending.resolve(event.result);
      return;
    }
    pending.reject(new Error(event.error || 'Remote gxserver request failed.'));
  },
};

const gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck: GpuiSidebarRuntimeRemoteMachineMethods =
  gpuiSidebarRuntimeRemoteMachineMethods;
void gpuiSidebarRuntimeRemoteMachineMethodsShapeCheck;
