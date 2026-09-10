// Slash-command envelope re-surfacing (upstream chat spec §9.2 port).
// Claude-family harnesses record a slash input's user turn as
// <command-name>/x</command-name><command-args>…</command-args> — hidden by
// the noise filter (correctly, for most CATALOG commands, since a local command
// marker is shown instead). But a skill invocation IS the user's chat turn,
// and `/compact`'s marker retires when compaction finishes, so those durable
// transcript records must be converted back into readable user turns.

import type { SessionChatMessage } from '../../shared/session-chat';
import {
  decodeSessionChatEscapedMarkup,
  SESSION_CHAT_ESCAPED_MARKUP_ATTRIBUTE,
} from './session-chat-local-command-transcript';

// The optional attribute list is gxserver's replayed envelope (see
// session-chat-local-command-transcript.ts): same tags, marked escaped.
const COMMAND_NAME = /<command-name(\s[^>]*)?>([\s\S]*?)<\/command-name>/;
const COMMAND_ARGS = /<command-args(\s[^>]*)?>([\s\S]*?)<\/command-args>/;

export interface SessionChatCommandEnvelope {
  name: string;
  args: string;
}

export function parseSessionChatCommandEnvelope(text: string): SessionChatCommandEnvelope | null {
  const trimmed = text.trimStart();
  if (!trimmed.toLowerCase().startsWith('<command-')) {
    // Ordinary prompts, XML pastes.
    return null;
  }
  const nameMatch = COMMAND_NAME.exec(trimmed);
  const name = nameMatch?.[2]?.trim();
  if (!name) {
    return null;
  }
  const decode = (value: string, attributes?: string): string =>
    attributes?.includes(SESSION_CHAT_ESCAPED_MARKUP_ATTRIBUTE) ? decodeSessionChatEscapedMarkup(value) : value;
  const argsMatch = COMMAND_ARGS.exec(trimmed);
  return { args: decode(argsMatch?.[2]?.trim() ?? '', argsMatch?.[1]), name: decode(name, nameMatch?.[1]) };
}

export function surfaceSkillInvocationUserTurns(
  messages: readonly SessionChatMessage[],
  catalogCommandNames: ReadonlySet<string>
): readonly SessionChatMessage[] {
  let changed = false;
  const out: SessionChatMessage[] = [];
  for (const message of messages) {
    if (
      message.id.startsWith('local-command:') ||
      message.role !== 'user' ||
      !message.blocks.every((block) => block.type === 'text')
    ) {
      out.push(message);
      continue;
    }
    const envelope = parseSessionChatCommandEnvelope(
      message.blocks.map((block) => (block.type === 'text' ? block.text : '')).join('\n')
    );
    const catalogName = envelope?.name.replace(/^\//, '').toLowerCase();
    if (!envelope || (catalogName !== 'compact' && catalogCommandNames.has(catalogName ?? ''))) {
      out.push(message);
      continue;
    }
    // The harness canonicalizes a plugin skill to `/plugin:name`, but the
    // user typed the SHORT name.
    const shortName = envelope.name.replace(/^\//, '').split(':').at(-1) ?? '';
    const token = `/${shortName}`;
    out.push({
      ...message,
      blocks: [
        {
          text: envelope.args ? `${token} ${envelope.args}` : token,
          type: 'text',
        },
      ],
    });
    changed = true;
  }
  // Identity preserved when nothing changed.
  return changed ? out : messages;
}
