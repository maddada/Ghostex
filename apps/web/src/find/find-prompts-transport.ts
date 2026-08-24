/*
CDXC:AgentHistorySearch 2026-08-20:
ghostex-web FindPromptsTransport. Prompt history lives on the machine that ran
the agent, so every Find RPC is scoped to one machine's gxserver connection —
the same rule the Session Chat transport follows.
*/

import type {
  ReadAgentPromptTextParams,
  ReadAgentPromptTextResult,
  ResolveAgentPromptLaunchParams,
  ResolveAgentPromptLaunchResult,
  SearchAgentPromptsParams,
  SearchAgentPromptsResult,
  ToggleAgentPromptFavoriteParams,
  ToggleAgentPromptFavoriteResult,
} from '@/packages/shared/agent-prompt-search';
import type { FindPromptsTransport } from '@/packages/core-ui/find/find-prompts-transport';
import { rpcForMachine } from '../connections/connection-registry';

export interface WebFindPromptsHostActions {
  /** Brings an already-open session tab to the front. */
  focusSession(params: { projectId: string; sessionId: string }): void;
  /** Closes the app-level Search by Prompt modal. */
  close(): void;
}

export function createFindPromptsTransport(machineId: string, host: WebFindPromptsHostActions): FindPromptsTransport {
  return {
    close() {
      host.close();
    },
    async copyText(text) {
      await navigator.clipboard.writeText(text);
    },
    async focusSession(params) {
      host.focusSession(params);
    },
    /*
     * Web has no session factory of its own for an arbitrary command in an
     * arbitrary folder — the daemon-side project may not even be registered —
     * so `launchSession` is deliberately absent. The view then shows the exact
     * resolved command instead of a button that would quietly do nothing.
     */
    readText(params: ReadAgentPromptTextParams) {
      return rpcForMachine<ReadAgentPromptTextResult>(machineId, '/api/readAgentPromptText', { ...params });
    },
    resolveLaunch(params: ResolveAgentPromptLaunchParams) {
      return rpcForMachine<ResolveAgentPromptLaunchResult>(machineId, '/api/resolveAgentPromptLaunch', { ...params });
    },
    search(params: SearchAgentPromptsParams) {
      return rpcForMachine<SearchAgentPromptsResult>(machineId, '/api/searchAgentPrompts', {
        ...params,
      });
    },
    toggleFavorite(params: ToggleAgentPromptFavoriteParams) {
      return rpcForMachine<ToggleAgentPromptFavoriteResult>(machineId, '/api/toggleAgentPromptFavorite', { ...params });
    },
  };
}
