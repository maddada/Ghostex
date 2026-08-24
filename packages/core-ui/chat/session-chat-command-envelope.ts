// Slash-command envelope re-surfacing (upstream chat spec §9.2 port).
// Claude-family harnesses record a slash input's user turn as
// <command-name>/x</command-name><command-args>…</command-args> — hidden by
// the noise filter (correctly, for CATALOG commands, since a local "Ran /x"
// marker is shown instead). But a skill invocation IS the user's chat turn,
// so dropping its envelope would make the assistant appear to answer an
// empty conversation.

import type { SessionChatMessage } from '../../shared/session-chat';

const COMMAND_NAME = /<command-name>([\s\S]*?)<\/command-name>/;
const COMMAND_ARGS = /<command-args>([\s\S]*?)<\/command-args>/;

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
  const name = COMMAND_NAME.exec(trimmed)?.[1]?.trim();
  if (!name) {
    return null;
  }
  return { args: COMMAND_ARGS.exec(trimmed)?.[1]?.trim() ?? '', name };
}

export function surfaceSkillInvocationUserTurns(
  messages: readonly SessionChatMessage[],
  catalogCommandNames: ReadonlySet<string>
): readonly SessionChatMessage[] {
  let changed = false;
  const out: SessionChatMessage[] = [];
  for (const message of messages) {
    if (message.role !== 'user' || !message.blocks.every((block) => block.type === 'text')) {
      out.push(message);
      continue;
    }
    const envelope = parseSessionChatCommandEnvelope(
      message.blocks.map((block) => (block.type === 'text' ? block.text : '')).join('\n')
    );
    if (!envelope || catalogCommandNames.has(envelope.name.replace(/^\//, ''))) {
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
