import type { SessionChatBlock, SessionChatMessage, SessionChatTextBlock } from '../../shared/session-chat';

/*
CDXC:SessionChat 2026-09-10 WHY:
The escaped-markup contract, one place for both halves of it. gxserver marks a
harness marker whose payload it escaped (Codex's `!` commands, and the slash
commands it archives and replays in session_chat_local_command.rs), because the
reader strips markup out of these rows to find their text and would otherwise
eat a command or an output that contains `<…>`. The attribute string has to
match gxserver's `ESCAPED_MARKUP_ATTRIBUTE` byte for byte.
*/
export const SESSION_CHAT_ESCAPED_MARKUP_ATTRIBUTE = 'data-ghostex-escaped="html"';

const CODEX_LOCAL_COMMAND_INPUT = `<bash-input ${SESSION_CHAT_ESCAPED_MARKUP_ATTRIBUTE}>`;
const CODEX_LOCAL_COMMAND_OUTPUT = `<bash-stdout ${SESSION_CHAT_ESCAPED_MARKUP_ATTRIBUTE}>`;

/** The escaped marker gxserver writes, for a row this client synthesizes. */
export function sessionChatEscapedMarker(tag: string, body: string): string {
  const escaped = body.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  return `<${tag} ${SESSION_CHAT_ESCAPED_MARKUP_ATTRIBUTE}>${escaped}</${tag}>`;
}

export function decodeSessionChatEscapedMarkup(text: string): string {
  return text.replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&');
}

/**
 * The two rows one slash command renders as: the command, and its output when
 * the CLI printed any. Identical text to the rows gxserver replays from its
 * archive, so the live row and the archived one are the same row twice.
 */
export function sessionChatLocalCommandTexts(command: string, output?: string): string[] {
  const trimmed = command.trim();
  const separator = trimmed.search(/\s/);
  const name = separator < 0 ? trimmed : trimmed.slice(0, separator);
  const args = separator < 0 ? '' : trimmed.slice(separator).trim();
  // The space between the two markers is load-bearing: readers strip the
  // markup to get the text, and without it the name and its arguments would
  // become one word.
  const texts = [
    args.length > 0
      ? `${sessionChatEscapedMarker('command-name', name ?? command)} ${sessionChatEscapedMarker('command-args', args)}`
      : sessionChatEscapedMarker('command-name', name ?? command),
  ];
  if (output !== undefined && output.length > 0) {
    texts.push(sessionChatEscapedMarker('local-command-stdout', output));
  }
  return texts;
}

function isTextBlock(block: SessionChatBlock | undefined): block is SessionChatTextBlock {
  return block?.type === 'text';
}

export function normalizeSessionChatLocalCommandMessages(
  messages: readonly SessionChatMessage[]
): SessionChatMessage[] {
  const normalized: SessionChatMessage[] = [];
  for (const message of messages) {
    const command = message.blocks[0];
    const output = message.blocks[1];
    if (
      message.role !== 'user' ||
      message.source !== 'transcript' ||
      message.blocks.length !== 2 ||
      !isTextBlock(command) ||
      !isTextBlock(output) ||
      !command.text.startsWith(CODEX_LOCAL_COMMAND_INPUT) ||
      !output.text.startsWith(CODEX_LOCAL_COMMAND_OUTPUT)
    ) {
      normalized.push(message);
      continue;
    }
    normalized.push(
      { ...message, id: `${message.id}:command`, blocks: [command] },
      { ...message, id: `${message.id}:output`, blocks: [output] }
    );
  }
  return normalized;
}
