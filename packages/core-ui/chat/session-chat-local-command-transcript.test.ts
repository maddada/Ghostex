import { describe, expect, test } from 'vitest';
import type { SessionChatMessage } from '../../shared/session-chat';
import { parseSessionChatCommandEnvelope } from './session-chat-command-envelope';
import {
  normalizeSessionChatLocalCommandMessages,
  sessionChatLocalCommandTexts,
} from './session-chat-local-command-transcript';
import { isSessionChatCommandTurn, sessionChatSuppressedTurnPresentation } from './session-chat-noise';
import {
  retireSessionChatMarkersCoveredByLocalCommands,
  sessionChatAppCommandsAsMessages,
} from './session-chat-pending';

describe('Codex local-command transcript normalization', () => {
  test('splits one structured execution into command and output rows in order', () => {
    const execution: SessionChatMessage = {
      id: 'execution-1',
      role: 'user',
      blocks: [
        {
          type: 'text',
          text: '<bash-input data-ghostex-escaped="html">printf BANG_TEST</bash-input>',
        },
        {
          type: 'text',
          text: '<bash-stdout data-ghostex-escaped="html">BANG_TEST</bash-stdout>',
        },
      ],
      timestamp: 42,
      source: 'transcript',
      byteOffset: 99,
    };

    expect(normalizeSessionChatLocalCommandMessages([execution])).toEqual([
      {
        ...execution,
        id: 'execution-1:command',
        blocks: [execution.blocks[0]],
      },
      {
        ...execution,
        id: 'execution-1:output',
        blocks: [execution.blocks[1]],
      },
    ]);
  });
});

describe('archived slash commands', () => {
  const textMessage = (id: string, text: string): SessionChatMessage => ({
    id,
    role: 'user',
    blocks: [{ type: 'text', text }],
    timestamp: 1,
    source: 'transcript',
  });

  test('replayed rows read as a slash command and its output', () => {
    const [command, output] = sessionChatLocalCommandTexts(
      '/rename pilot <game> work',
      'Session renamed to: pilot <game> work'
    );

    // The same envelope gxserver replays from its archive, so both halves of
    // the contract are asserted against one presentation.
    expect(sessionChatSuppressedTurnPresentation(textMessage('c', command ?? ''))).toEqual({
      kind: 'inline',
      label: 'Slash command',
      text: '/rename pilot <game> work',
    });
    expect(sessionChatSuppressedTurnPresentation(textMessage('o', output ?? ''))).toEqual({
      kind: 'inline',
      label: 'Local command output',
      text: 'Session renamed to: pilot <game> work',
    });
  });

  test('a command with no output replays as one row', () => {
    expect(sessionChatLocalCommandTexts('/context')).toHaveLength(1);
    expect(sessionChatLocalCommandTexts('/context', '')).toHaveLength(1);
  });

  test('the model command keeps its status pill', () => {
    const [command] = sessionChatLocalCommandTexts('/model opus', 'Set model to Opus 5');
    // Hidden: the output row owns the one user-facing result row.
    expect(sessionChatSuppressedTurnPresentation(textMessage('m', command ?? ''))).toBeNull();
    expect(parseSessionChatCommandEnvelope(command ?? '')).toEqual({ name: '/model', args: 'opus' });
  });

  test('the live acknowledgement renders as those rows and retires against the archive', () => {
    const appCommand = {
      id: 'a1',
      command: '/rename pilot game work',
      output: 'Session renamed to: pilot game work',
      localCommand: true,
      sentAt: '2026-09-10T00:00:00.000Z',
    };

    const live = sessionChatAppCommandsAsMessages([appCommand], []);
    expect(live.map((message) => message.id)).toEqual(['app-command-local:a1', 'app-command-local:a1:output']);
    expect(live.map((message) => (message.blocks[0]?.type === 'text' ? message.blocks[0].text : ''))).toEqual(
      sessionChatLocalCommandTexts(appCommand.command, appCommand.output)
    );

    // Once the archived envelope reaches the messages, the live row IS that
    // row: showing both would render one command twice.
    const archived = textMessage('local-command:1-0', sessionChatLocalCommandTexts(appCommand.command)[0] ?? '');
    expect(sessionChatAppCommandsAsMessages([appCommand], [archived])).toEqual([]);
  });
});

describe('client markers for archived commands', () => {
  const marker = (command: string): SessionChatMessage => ({
    id: `command:${command}`,
    role: 'user',
    blocks: [{ type: 'text', text: command }],
    timestamp: 5,
    source: 'client',
  });

  test('retire once the live local-command row covers them', () => {
    const live = [{ id: 'a1', command: '/context', localCommand: true, sentAt: '2026-09-10T00:00:00.000Z' }];
    expect(
      retireSessionChatMarkersCoveredByLocalCommands([marker('/context'), marker('/help')], live, []).map(
        (message) => message.id
      )
    ).toEqual(['command:/help']);
  });

  test('retire once the archived envelope is replayed after a read', () => {
    const archived: SessionChatMessage = {
      id: 'local-command:1-0',
      role: 'user',
      blocks: [{ type: 'text', text: sessionChatLocalCommandTexts('/rename pilot game work')[0] ?? '' }],
      timestamp: 4,
      source: 'transcript',
    };
    expect(retireSessionChatMarkersCoveredByLocalCommands([marker('/rename pilot game work')], [], [archived])).toEqual(
      []
    );
  });

  test("an agent's own envelope does not retire them", () => {
    // Claude's `/compact` envelope is not ours; its marker retires on the
    // compaction record instead.
    const claudeEnvelope: SessionChatMessage = {
      id: 't1',
      role: 'user',
      blocks: [{ type: 'text', text: '<command-name>/compact</command-name><command-args></command-args>' }],
      timestamp: 4,
      source: 'transcript',
    };
    expect(retireSessionChatMarkersCoveredByLocalCommands([marker('/compact')], [], [claudeEnvelope])).toHaveLength(1);
  });
});

describe('command rows in the turn summary', () => {
  const row = (text: string): SessionChatMessage => ({
    id: text,
    role: 'user',
    blocks: [{ type: 'text', text }],
    timestamp: 1,
    source: 'transcript',
  });

  test('a visible slash command starts its own turn; its output and hidden commands do not', () => {
    const [command, output] = sessionChatLocalCommandTexts('/rename x', 'Session renamed to: x');
    expect(isSessionChatCommandTurn(row(command ?? ''))).toBe(true);
    expect(isSessionChatCommandTurn(row(output ?? ''))).toBe(false);
    // `/model` keeps its status pill and never becomes a turn of its own.
    expect(isSessionChatCommandTurn(row(sessionChatLocalCommandTexts('/model opus')[0] ?? ''))).toBe(false);
    expect(isSessionChatCommandTurn(row('an ordinary prompt'))).toBe(false);
  });
});
