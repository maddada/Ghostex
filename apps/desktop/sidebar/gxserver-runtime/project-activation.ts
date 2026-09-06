import type { GpuiSidebarRuntime } from './core';
import { createGpuiSidebarSettings } from './helpers/bootstrap';
import {
  createGpuiRemotePresentationGroupId,
  createGpuiRemotePresentationProjectId,
  createGpuiRemotePresentationSessionId,
  parseGpuiRemotePresentationProjectId,
  parseGpuiRemotePresentationSessionId,
} from './helpers/remote-presentation';
import { resolveEffectivePreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import {
  createGxserverPresentationProjectGroupId,
  createGxserverPresentationProjectSessionId,
  createGxserverPresentationSessionsByProjectFromGroups,
} from '@/packages/shared/gxserver-presentation-sidebar-projection';

const storagePrefix = 'ghostex.gpui.project-last-session.v1:';
const pendingReveals = new WeakMap<GpuiSidebarRuntime, string>();
const activations = new WeakMap<GpuiSidebarRuntime, Promise<void>>();
let revealRequestId = Date.now();

/**
 * CDXC:Projects 2026-09-05 DECISION:
 * User: remember each project's last selected agent or terminal across restarts, including sleeping sessions and closed projects.
 * Native tab/pane focus and sidebar focus both write this selection; agent activity timestamps do not represent the user's selection.
 */
export function rememberGpuiProjectSession(runtime: GpuiSidebarRuntime, projectId: string, sessionId: string): void {
  const remote = parseGpuiRemotePresentationSessionId(sessionId);
  const scopedProjectId = remote
    ? createGpuiRemotePresentationProjectId(remote.machineId, remote.projectId)
    : projectId;
  const sidebarSessionId = remote ? sessionId : createGxserverPresentationProjectSessionId(projectId, sessionId);
  try {
    const key = storagePrefix + scopedProjectId;
    if (localStorage.getItem(key) !== sidebarSessionId) {
      localStorage.setItem(key, sidebarSessionId);
    }
  } catch {
    runtime.postSidebarActionToast('warning', 'Could not remember the selected project session.');
  }
  if (pendingReveals.get(runtime) === scopedProjectId) {
    pendingReveals.delete(runtime);
    runtime.messageSource.postMessage({
      requestId: ++revealRequestId,
      sessionId: sidebarSessionId,
      type: 'revealSidebarSession',
    });
  }
}

/**
 * CDXC:Projects 2026-09-05 DECISION:
 * User: opening a project from Quick Access selects and wakes its last agent/terminal, or creates the default agent in Chat mode (a terminal when Terminal is the default), and focuses its input.
 * Refresh before choosing so a restored project cannot look empty merely because its sidebar snapshot has not arrived yet.
 */
export function activateGpuiProject(runtime: GpuiSidebarRuntime, projectId: string): Promise<void> {
  const previous = activations.get(runtime) ?? Promise.resolve();
  const activation = previous
    .then(async () => {
      const remote = parseGpuiRemotePresentationProjectId(projectId);
      if (remote) {
        await runtime.refreshRemotePresentationFromGxserver(remote.machineId);
      } else {
        if (!runtime.client) {
          throw new Error('Local gxserver is unavailable.');
        }
        await runtime.refreshDomainPresentationSnapshotFromClient('patch');
      }
      const presentation = remote ? runtime.remotePresentations.get(remote.machineId) : runtime.presentation;
      const rawProjectId = remote?.projectId ?? projectId;
      if (!presentation?.projects.some((project) => project.projectId === rawProjectId)) {
        throw new Error('The project is unavailable.');
      }
      const sessions = (
        createGxserverPresentationSessionsByProjectFromGroups({ presentation }).get(rawProjectId) ?? []
      ).filter((session) => session.kind === 'agent' || session.kind === 'terminal');
      const sidebarId = (sessionId: string) =>
        remote
          ? createGpuiRemotePresentationSessionId(remote.machineId, remote.projectId, sessionId)
          : createGxserverPresentationProjectSessionId(projectId, sessionId);
      const remembered = localStorage.getItem(storagePrefix + projectId);
      const selected = sessions.find((session) => sidebarId(session.sessionId) === remembered) ?? sessions[0];
      pendingReveals.set(runtime, projectId);
      if (selected) {
        await runtime.focusSession(sidebarId(selected.sessionId), undefined, {
          preferredInterface: selected.kind === 'agent' ? 'chat' : 'terminal',
        });
        return;
      }
      runtime.focusedSessionId = undefined;
      if (remote) {
        runtime.focusRemotePresentationProject(remote);
        runtime.publishRemotePresentationPatch();
      } else {
        runtime.focusProjectId(projectId);
        runtime.publishPresentation('patch');
      }
      const groupId = remote
        ? createGpuiRemotePresentationGroupId(remote.machineId, remote.projectId)
        : createGxserverPresentationProjectGroupId(projectId);
      const agentId = runtime.resolveDefaultPromptAgentId();
      if (
        resolveEffectivePreferredAgentInterface(createGpuiSidebarSettings(runtime.runtimeSettings), agentId) === 'chat'
      ) {
        await runtime.requestAgentSessionLaunch(agentId, groupId);
      } else {
        await runtime.createSession(groupId);
      }
    })
    .catch(() => {
      pendingReveals.delete(runtime);
      runtime.postSidebarActionToast('warning', 'Could not open the project session.');
    });
  activations.set(runtime, activation);
  return activation;
}
