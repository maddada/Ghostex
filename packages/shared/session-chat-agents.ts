import { normalizeContentThemeSetting, type ContentThemeSetting } from './appearance';

export interface SessionChatSelectionOptions {
  mode?: string;
  fastMode?: 'on' | 'off';
}

export interface SessionChatPendingModelSelection {
  id: string;
  model: string;
  effort: string;
  /** `failed` is terminal: the row survives only to carry `errorMessage` to the chat. */
  state: 'queued' | 'applying' | 'failed';
  options?: SessionChatSelectionOptions;
  /** Omitted means `'default'`: an older client's choice still changes the agent's saved default. */
  scope?: SessionChatModelSelectionScope;
  errorMessage?: string;
}

/**
 * CDXC:SessionChat 2026-09-27 DECISION:
 * User: a chat model pick can apply to this session alone instead of changing the agent's saved default (2026-09-18), and Codex uses its own session choice too (2026-09-27).
 * `'session'` works for Claude (its `/model` list answers `s` with "for this session only"), Codex 0.157 or newer (`s session` in its reasoning lists, answered "for this conversation") and OpenCode. This supersedes Claude-only session picks.
 * SEE-ALSO: server/src/session_chat_codex_picker.rs drives both scopes; server/src/session_chat_model_selection.rs
 * carries this through the durable queue.
 */
export type SessionChatModelSelectionScope = 'session' | 'default';

/**
 * CDXC:SessionChat 2026-10-21 SEE-ALSO:
 * A copy of `SESSION_CHAT_AGENTS` in packages/gx-protocol/src/session_chat_agents.rs, the table every Rust layer reads; the phone cannot link that crate, so `cargo xtask generate-mobile-chat-agents` turns this set into its list. Change both together.
 */
export const SESSION_CHAT_SUPPORTED_AGENTS = new Set([
  'antigravity',
  'antigravity-cli',
  'agy',
  'claude',
  'openclaude',
  'codex',
  'copilot',
  'cursor',
  'empryo',
  'opencode',
  'grok',
  'grok-build',
  'hermes',
  'hermes-agent',
  'pi',
  'omp',
  'zcode',
  'freebuff',
  'dsh',
  'dsh-tui',
]);

export type SessionChatTranscriptAgent =
  | 'antigravity'
  | 'claude'
  | 'codex'
  | 'copilot'
  | 'cursor'
  | 'empryo'
  | 'grok'
  | 'hermes'
  | 'opencode'
  | 'pi'
  | 'zcode'
  | 'freebuff'
  | 'dsh';

export function resolveSessionChatTranscriptAgent(
  agentId: string | null | undefined,
  agentIcon?: string | null
): SessionChatTranscriptAgent | null {
  const candidates = [agentId, agentIcon];
  for (const candidate of candidates) {
    const normalized = candidate?.trim().toLowerCase();
    if (
      normalized === 'antigravity' ||
      normalized === 'antigravity-cli' ||
      normalized === 'antigravity cli' ||
      normalized === 'agy'
    ) {
      return 'antigravity';
    }
    if (normalized === 'claude' || normalized === 'openclaude') return 'claude';
    if (normalized === 'codex') return 'codex';
    if (normalized === 'copilot' || normalized === 'github copilot' || normalized === 'github-copilot')
      return 'copilot';
    if (
      normalized === 'cursor' ||
      normalized === 'cursor-agent' ||
      normalized === 'cursor-cli' ||
      normalized === 'cursor cli'
    )
      return 'cursor';
    if (normalized === 'empryo') return 'empryo';
    if (normalized === 'grok' || normalized === 'grok-build') return 'grok';
    if (normalized === 'hermes' || normalized === 'hermes-agent' || normalized === 'hermes agent') return 'hermes';
    if (normalized === 'opencode') return 'opencode';
    if (normalized === 'pi' || normalized === 'omp') return 'pi';
    if (normalized === 'zcode' || normalized === 'zcode-cli') return 'zcode';
    if (normalized === 'freebuff') return 'freebuff';
    if (
      normalized === 'dsh' ||
      normalized === 'dsh-tui' ||
      normalized === 'deepseek' ||
      normalized === 'deepseek harness'
    )
      return 'dsh';
  }
  return null;
}

export type SessionChatDisplayAgent = SessionChatTranscriptAgent | 'omp';

/**
 * Resolve the agent identity shown by chat UI without conflating it with the
 * transcript parser family. OMP transcripts use Pi's format, but OMP remains
 * its own product name and logo everywhere the session is presented.
 */
export function resolveSessionChatDisplayAgent(
  agentId: string | null | undefined,
  agentIcon?: string | null
): SessionChatDisplayAgent | null {
  const candidates = [agentId, agentIcon];
  for (const candidate of candidates) {
    if (candidate?.trim().toLowerCase() === 'omp') {
      return 'omp';
    }
  }
  return resolveSessionChatTranscriptAgent(agentId, agentIcon);
}

/**
 * Sidebar artwork id for a chat agent label. Read-state labels are transcript
 * family ids, and two of those differ from the sidebar agent id that owns the
 * brand artwork (`hermes` → `hermes-agent`, `grok` → `grok-build`); the rest
 * match their sidebar id as-is.
 */
export function sessionChatAgentIconId(agentLabel: string | null | undefined): string | null {
  const display = resolveSessionChatDisplayAgent(agentLabel);
  if (display === 'antigravity') return 'antigravity-cli';
  if (display === 'hermes') return 'hermes-agent';
  if (display === 'grok') return 'grok-build';
  return display;
}

export type SessionChatSource = 'transcript' | 'hook' | 'client';

/** Visual palette for the shared chat surface, independent of app chrome. */
export type SessionChatTheme = 'light' | 'dark';

export type SessionChatThemeSetting = ContentThemeSetting;

export function normalizeSessionChatTheme(value: unknown): SessionChatThemeSetting {
  return normalizeContentThemeSetting(value);
}

// Higher wins when the same message id/turn arrives from two sources.
export const SESSION_CHAT_SOURCE_PRIORITY: Record<SessionChatSource, number> = {
  transcript: 3,
  hook: 2,
  client: 1,
};
