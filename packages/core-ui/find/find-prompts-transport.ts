/*
CDXC:AgentHistorySearch 2026-08-20:
Find surface transport contract. Hosts (gpui CEF, ghostex-web, the mobile
WebView) inject an implementation so the shared Find components never talk to
gxserver directly — the same split the Session Chat surface uses.

`focusSession` and `launchSession` are host capabilities, not server ones:
gxserver resolves *what* opening a result means, and the host performs it with
the session machinery it already owns.
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
  FindPromptLaunchPlan,
} from '../../shared/agent-prompt-search';

export interface FindPromptsTransport {
  search(params: SearchAgentPromptsParams): Promise<SearchAgentPromptsResult>;
  readText(params: ReadAgentPromptTextParams): Promise<ReadAgentPromptTextResult>;
  toggleFavorite(params: ToggleAgentPromptFavoriteParams): Promise<ToggleAgentPromptFavoriteResult>;
  resolveLaunch(params: ResolveAgentPromptLaunchParams): Promise<ResolveAgentPromptLaunchResult>;
  /**
   * Brings an already-open Ghostex session to the front. Hosts without a
   * workspace to focus (a standalone page) omit it, and the view says the
   * conversation is already open instead of pretending it moved.
   */
  focusSession?(params: { projectId: string; sessionId: string }): Promise<void>;
  /**
   * Opens a new Ghostex session running the resolved command. Hosts without a
   * session-creation path omit it; the view then offers the command for copying
   * rather than a dead button.
   */
  launchSession?(plan: FindPromptLaunchPlan): Promise<void>;
  /** Copies text to the clipboard (`^y`). */
  copyText?(text: string): Promise<void>;
  /** Closes the Find surface (`Esc` / `^c`). */
  close?(): void;
}
