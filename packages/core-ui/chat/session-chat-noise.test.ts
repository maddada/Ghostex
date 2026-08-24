import { describe, expect, test } from 'vitest';
import type { SessionChatMessage } from '../../shared/session-chat';
import { parseSessionChatCommandEnvelope, surfaceSkillInvocationUserTurns } from './session-chat-command-envelope';
import {
  isSessionChatNoiseMessage,
  sessionChatSuppressedTurnLabel,
  sessionChatSuppressedTurnPresentation,
  stripSessionChatNoiseMessages,
} from './session-chat-noise';

function textMsg(id: string, role: SessionChatMessage['role'], text: string): SessionChatMessage {
  return {
    blocks: [{ text, type: 'text' }],
    id,
    role,
    source: 'transcript',
    timestamp: 1,
  };
}

describe('noise filter (§9.1)', () => {
  test('known harness tags are noise', () => {
    expect(isSessionChatNoiseMessage(textMsg('1', 'user', '<system-reminder>stuff'))).toBe(true);
    expect(isSessionChatNoiseMessage(textMsg('2', 'user', '<task-notification> done'))).toBe(true);
    expect(isSessionChatNoiseMessage(textMsg('3', 'system', '<user-prompt-submit-hook>output'))).toBe(true);
  });

  test('unknown custom tags are genuine user turns (no broad kebab match)', () => {
    expect(isSessionChatNoiseMessage(textMsg('1', 'user', '<my-element> is broken'))).toBe(false);
    expect(isSessionChatNoiseMessage(textMsg('2', 'user', '<user_query>hello</user_query>'))).toBe(false);
  });

  test('<channel> only matches its attributed source form', () => {
    expect(isSessionChatNoiseMessage(textMsg('1', 'user', '<channel source="x">hi'))).toBe(true);
    expect(isSessionChatNoiseMessage(textMsg('2', 'user', '<channel><title>RSS</title>'))).toBe(false);
  });

  test('known injected prefixes are noise', () => {
    expect(isSessionChatNoiseMessage(textMsg('1', 'user', '[Request interrupted by user]'))).toBe(true);
    expect(
      isSessionChatNoiseMessage(textMsg('2', 'user', 'This session is being continued from a previous conversation …'))
    ).toBe(true);
  });

  test('assistant rows and tool rows are never noise', () => {
    expect(isSessionChatNoiseMessage(textMsg('1', 'assistant', '<system-reminder>'))).toBe(false);
    const toolResult: SessionChatMessage = {
      blocks: [{ output: '<system-reminder>', type: 'tool-result' }],
      id: '2',
      role: 'user',
      source: 'transcript',
      timestamp: 1,
    };
    expect(isSessionChatNoiseMessage(toolResult)).toBe(false);
  });

  test("local 'Ran /x' markers survive the strip", () => {
    const marker = textMsg('command:1', 'system', 'Ran /clear');
    expect(stripSessionChatNoiseMessages([marker])).toEqual([marker]);
  });

  test('empty text is not noise', () => {
    expect(isSessionChatNoiseMessage(textMsg('1', 'user', '   '))).toBe(false);
  });

  test('special command markers use their meaningful result', () => {
    expect(sessionChatSuppressedTurnPresentation(textMsg('compact-input', 'user', '/compact'))).toBeNull();
    expect(
      sessionChatSuppressedTurnPresentation(
        textMsg(
          'compact-command',
          'user',
          '<command-name>/compact</command-name><command-message>compact</command-message><command-args></command-args>'
        )
      )
    ).toBeNull();
    // Verbatim shape from a real transcript: dim-SGR-wrapped "Compacted".
    expect(
      sessionChatSuppressedTurnLabel(
        textMsg('1', 'user', '<local-command-stdout>[2mCompacted [22m</local-command-stdout>')
      )
    ).toBe('Compaction completed');
    // Lenient: ESC bytes lost in encoding, different wording, trailing period.
    expect(
      sessionChatSuppressedTurnPresentation(
        textMsg('2', 'user', '<local-command-stdout>[2mCompaction complete.[22m</local-command-stdout>')
      )
    ).toMatchObject({ kind: 'status', label: 'Compaction completed' });
    expect(
      sessionChatSuppressedTurnPresentation(
        textMsg(
          'compact-output-with-hint',
          'user',
          '<local-command-stdout>[2mCompacted (ctrl+o to see full summary) [22m</local-command-stdout>'
        )
      )
    ).toMatchObject({ kind: 'status', label: 'Compaction completed' });
    const modelCommand = textMsg(
      'model-command',
      'user',
      '<command-name>/model</command-name><command-message>model</command-message><command-args>opus</command-args>'
    );
    expect(sessionChatSuppressedTurnLabel(modelCommand)).toBeNull();
    const modelOutput = textMsg(
      '3',
      'user',
      '<local-command-stdout>Set model to [1mOpus 5 (1M context) [22m and saved as your default for new sessions</local-command-stdout>'
    );
    expect(sessionChatSuppressedTurnPresentation(modelOutput)).toEqual({
      kind: 'status',
      label: 'Set model to Opus 5 (1M context)',
      text: 'Set model to Opus 5 (1M context) and saved as your default for new sessions',
    });
    const effortCommand = textMsg(
      'effort-command',
      'user',
      '<command-name>/effort</command-name><command-message>effort</command-message><command-args>xhigh</command-args>'
    );
    const effortOutput = textMsg(
      'effort-output',
      'user',
      '<local-command-stdout>Set effort level to xhigh (saved as your default for new sessions): Deeper reasoning than high</local-command-stdout>'
    );
    expect(sessionChatSuppressedTurnLabel(effortCommand)).toBeNull();
    expect(sessionChatSuppressedTurnLabel(effortOutput)).toBeNull();
    // Prose that merely mentions compaction is untouched.
    expect(
      sessionChatSuppressedTurnLabel(
        textMsg('4', 'user', '<local-command-stdout>Compacted 3 files into archive.tar</local-command-stdout>')
      )
    ).toBe('Local command output');
  });
});

describe('command envelope re-surfacing (§9.2)', () => {
  test('parses only leading <command- envelopes', () => {
    expect(
      parseSessionChatCommandEnvelope('<command-name>/spin</command-name><command-args>fast</command-args>')
    ).toEqual({ args: 'fast', name: '/spin' });
    expect(parseSessionChatCommandEnvelope('prose with <command-name>x</command-name>')).toBe(null);
  });

  test('skill envelopes resurface as the short token; catalog commands stay hidden', () => {
    const skillTurn = textMsg(
      'u1',
      'user',
      '<command-name>/plugin:deploy</command-name><command-args>prod</command-args>'
    );
    const catalogTurn = textMsg('u2', 'user', '<command-name>/clear</command-name><command-args></command-args>');
    const out = surfaceSkillInvocationUserTurns([skillTurn, catalogTurn], new Set(['clear']));
    expect(out[0]?.blocks).toEqual([{ text: '/deploy prod', type: 'text' }]);
    expect(out[1]).toBe(catalogTurn);
  });

  test('identity preserved when nothing changes', () => {
    const messages = [textMsg('u1', 'user', 'plain prompt')];
    expect(surfaceSkillInvocationUserTurns(messages, new Set())).toBe(messages);
  });
});
