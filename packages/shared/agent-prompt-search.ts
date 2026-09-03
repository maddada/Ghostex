/*
CDXC:PromptSearch 2026-08-20:
Wire contract for the Find surface — the GUI for `gx f`. gxserver keeps one warm
prompt index (the same zehn scanner, matcher, and favorites file the terminal
picker uses) and answers these four RPCs from it.

Rows address prompts by `key` — a stable 16-digit hex identity derived from the
agent and the prompt text, which is exactly what the favorites file hashes. A
rebuild reorders records, and a user can sit on a result for minutes before
acting on it, so keying by identity means the action still lands on the prompt
they were looking at rather than on whatever moved into that slot.
*/

export const FIND_PROMPT_AGENTS = ['claude', 'codex', 'pi', 'opencode', 'cursor', 'grok'] as const;

export type FindPromptAgent = (typeof FIND_PROMPT_AGENTS)[number];

export function isFindPromptAgent(value: unknown): value is FindPromptAgent {
  return typeof value === 'string' && (FIND_PROMPT_AGENTS as readonly string[]).includes(value);
}

export interface FindPromptUsage {
  cacheRead: number;
  cacheWrite: number;
  contextWindow: number;
  cost: number;
  input: number;
  output: number;
  ratePercent: number;
  total: number;
}

export interface FindPromptMeta {
  model: string;
  plan: string;
  provider: string;
  thinking: string;
  usage: FindPromptUsage;
}

export interface FindPromptRow {
  agent: FindPromptAgent;
  /** Brand color for the agent, so every host paints the same palette. */
  agentColor: string;
  /** Day bucket used for `^d` grouping; a very negative number means unknown. */
  dayKey: number;
  favorite: boolean;
  /** Byte offsets in `text` that matched the query. */
  highlights: readonly number[];
  /** Position in the server's current ranking; presentation only. */
  index: number;
  /** Stable identity of this prompt; pass it to every follow-up call. */
  key: string;
  meta: FindPromptMeta;
  project: string;
  projectName: string;
  score: number;
  sessionId: string;
  /** Possibly truncated; `readAgentPromptText` returns the rest. */
  text: string;
  textLength: number;
  title: string;
  truncated: boolean;
  /** Session last-active unix seconds; 0 when unknown. */
  ts: number;
}

export interface FindPromptProjectFacet {
  name: string;
  path: string;
}

export interface FindPromptAgentFacet {
  agent: FindPromptAgent;
  color: string;
  present: boolean;
}

export interface SearchAgentPromptsParams {
  agents?: readonly FindPromptAgent[];
  groupByDay?: boolean;
  includeFacets?: boolean;
  limit?: number;
  offset?: number;
  project?: string;
  query?: string;
  /** Forces an index rebuild before answering. */
  refresh?: boolean;
  textLimit?: number;
}

export interface SearchAgentPromptsResult {
  agents?: readonly FindPromptAgentFacet[];
  indexEpoch: number;
  /** Unix seconds when the index was built. */
  indexedAt: number;
  /** Records that passed the filters and matched the query. */
  matched: number;
  offset: number;
  /** Present when an opencode database exists but could not be read. */
  opencodeError?: string;
  projects?: readonly FindPromptProjectFacet[];
  rows: readonly FindPromptRow[];
  /** Every record in the index, before filtering. */
  total: number;
}

export interface ReadAgentPromptTextParams {
  key: string;
}

export interface ReadAgentPromptTextResult {
  key: string;
  text: string;
}

export interface ToggleAgentPromptFavoriteParams {
  /** Omit to flip the current state. */
  favorite?: boolean;
  key: string;
}

export interface ToggleAgentPromptFavoriteResult {
  favorite: boolean;
  key: string;
}

export interface ResolveAgentPromptLaunchParams {
  /** `"resume"` re-enters the recorded session; `"fork"` starts a fresh one. */
  action: 'fork' | 'resume';
  /** Adds the agent's permission-bypass flags where one exists. */
  acceptAll?: boolean;
  /** Target agent for `"fork"`; defaults to the prompt's own agent. */
  forkAgent?: FindPromptAgent;
  key: string;
}

/** A live Ghostex session already owns this agent conversation — focus it. */
export interface FindPromptFocusPlan {
  key: string;
  mode: 'focus';
  projectId: string;
  sessionId: string;
}

/** Nothing owns it — run `command` in `cwd` as a new session. */
export interface FindPromptLaunchPlan {
  agent: FindPromptAgent;
  command: readonly string[];
  /** `command` quoted into one POSIX line, for hosts that can only type text. */
  commandLine: string;
  cwd: string;
  /** False when the recorded project directory is gone. */
  cwdExists: boolean;
  key: string;
  mode: 'launch';
  title: string;
}

export type ResolveAgentPromptLaunchResult = FindPromptFocusPlan | FindPromptLaunchPlan;

/** Day bucket for a unix-second timestamp; mirrors the server's `day_key`. */
export const FIND_PROMPT_SECONDS_PER_DAY = 86_400;

export function findPromptDayKey(ts: number): number {
  if (!Number.isFinite(ts) || ts <= 0) {
    return Number.NEGATIVE_INFINITY;
  }
  return Math.floor(ts / FIND_PROMPT_SECONDS_PER_DAY);
}
