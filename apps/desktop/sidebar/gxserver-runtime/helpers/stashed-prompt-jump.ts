/*
CDXC:SavedPrompts 2026-08-24:
Payload normalizer for the Saved Prompts "Go to session" jump. Same shape rule
as every other first-party GPUI bridge payload: a fixed key set, an exact
type/version match, and bounded strings — anything else is dropped rather than
routed. The ids are RAW gxserver ids, so combined presentation ids
(`combined-session:…`) are rejected here instead of being half-resolved later.
*/
import {
  GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_TYPE,
  GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_VERSION,
} from '../constants';
import { normalizeNonEmptyString } from './records';

export type GpuiStashedPromptSessionJumpRequest = {
  agentSessionId?: string;
  projectId?: string;
  sessionId?: string;
};

/**
 * Project and session ids are gxserver's own ids, which never contain `:`.
 * Rejecting the separator here is what keeps a combined presentation id from
 * being routed as if it were a raw one. `agentSessionId` is a provider-owned
 * conversation id and is only trimmed, because its format belongs to the agent
 * CLI, not to Ghostex.
 */
function normalizeGpuiStashedPromptJumpGxserverId(value: unknown): string | undefined {
  const id = normalizeNonEmptyString(value)?.trim();
  if (!id || id.includes(':')) {
    return undefined;
  }
  return id;
}

/**
 * The id triple on its own, without the bridge envelope. The sidebar's own
 * `jumpToStashedPromptSession` contract message arrives already typed and goes
 * straight through here; the Rust bridge payload passes the envelope check
 * first.
 */
export function normalizeGpuiStashedPromptSessionJumpRequest(
  value: unknown
): GpuiStashedPromptSessionJumpRequest | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  const agentSessionId = normalizeNonEmptyString(record.agentSessionId)?.trim();
  const projectId = normalizeGpuiStashedPromptJumpGxserverId(record.projectId);
  const sessionId = normalizeGpuiStashedPromptJumpGxserverId(record.sessionId);
  if (!agentSessionId && !(projectId && sessionId)) {
    return undefined;
  }
  return {
    ...(agentSessionId ? { agentSessionId } : {}),
    ...(projectId ? { projectId } : {}),
    ...(sessionId ? { sessionId } : {}),
  };
}

export function normalizeGpuiStashedPromptSessionJump(value: unknown): GpuiStashedPromptSessionJumpRequest | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }
  const record = value as Record<string, unknown>;
  if (
    Object.keys(record).some((key) => !['agentSessionId', 'projectId', 'sessionId', 'type', 'version'].includes(key))
  ) {
    return undefined;
  }
  if (
    record.type !== GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_STASHED_PROMPT_SESSION_JUMP_MESSAGE_VERSION
  ) {
    return undefined;
  }
  return normalizeGpuiStashedPromptSessionJumpRequest(record);
}
