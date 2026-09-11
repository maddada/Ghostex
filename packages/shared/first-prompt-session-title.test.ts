import { describe, expect, test } from 'vitest';
import {
  explainFirstPromptAutoRenameDecision,
  getCurrentTitleForFirstPromptAutoRename,
  resolveFirstPromptAutoRenameStrategy,
} from './first-prompt-session-title';

describe('first-prompt auto naming current title selection', () => {
  test('should claim generic titles but preserve meaningful terminal-synced titles', () => {
    /**
     * CDXC:SessionTitles 2026-05-08-16:23
     * Codex can publish a good terminal title before the first-prompt hook is
     * polled. That title must count as the current title so the pending prompt
     * does not generate and send a redundant `/rename <generated title>`.
     */
    expect(
      getCurrentTitleForFirstPromptAutoRename({
        agentName: 'codex',
        pendingPrompt: 'adjust agent icon opacity',
        persistedTitle: undefined,
        sessionTitle: 'Codex Session',
        terminalTitle: 'Codex Session',
      })
    ).toBeUndefined();

    expect(
      getCurrentTitleForFirstPromptAutoRename({
        agentName: 'codex',
        pendingPrompt: 'adjust agent icon opacity',
        persistedTitle: undefined,
        sessionTitle: 'Session Icon Opacity States',
        terminalTitle: 'Session Icon Opacity States',
      })
    ).toBe('Session Icon Opacity States');

    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'codex',
        currentTitle: 'Session Icon Opacity States',
        prompt: 'adjust agent icon opacity',
      })
    ).toMatchObject({
      reason: 'unsupportedAgent',
      shouldAutoName: false,
    });
  });
});

describe('Pi first-prompt auto naming', () => {
  test("should generate a title and send Pi's name command strategy", () => {
    expect(resolveFirstPromptAutoRenameStrategy('pi')).toBe('generateTitleAndName');
    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'pi',
        currentTitle: 'π - ghostex',
        prompt: 'Implement first-class Pi restore support',
      })
    ).toMatchObject({
      reason: 'eligible',
      shouldAutoName: true,
      strategy: 'generateTitleAndName',
    });
  });
});

describe('Cursor first-prompt auto naming', () => {
  test('should not auto generate titles because Cursor names sessions itself', () => {
    /**
     * CDXC:SessionTitles 2026-05-30-05:44:
     * Cursor Agent owns its session titles by default, so Ghostex must not run
     * first-prompt title generation or send `/rename` for Cursor sessions.
     */
    expect(resolveFirstPromptAutoRenameStrategy('cursor')).toBeUndefined();
    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'cursor',
        currentTitle: 'Cursor Agent',
        prompt: 'Implement title overlay cancellation',
      })
    ).toMatchObject({
      reason: 'unsupportedAgent',
      shouldAutoName: false,
    });
  });
});

describe('Claude first-prompt auto naming', () => {
  test('should not auto generate titles because Claude names sessions itself', () => {
    expect(resolveFirstPromptAutoRenameStrategy('claude')).toBeUndefined();
    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'claude',
        currentTitle: 'Claude Code',
        prompt: 'Implement title overlay cancellation',
      })
    ).toMatchObject({
      reason: 'unsupportedAgent',
      shouldAutoName: false,
    });
  });
});

describe('first-prompt slash command mentions', () => {
  test('should allow title generation when slash commands are not at the start of a line', () => {
    /**
     * CDXC:SessionTitles 2026-05-30-05:18:
     * Forked Pi sessions must still generate a first-prompt title when the
     * user's natural-language request mentions `/rename` as product behavior.
     * Only short slash-command invocations at the start of a line should
     * suppress auto-title; long prompts should be renamed from their text.
     */
    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'pi',
        currentTitle: 'Terminal Session',
        prompt: 'For all GitHub related terminals, the /rename command should write the action starting with "Git: ".',
      })
    ).toMatchObject({
      reason: 'eligible',
      shouldAutoName: true,
      strategy: 'generateTitleAndName',
    });

    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'pi',
        currentTitle: 'Terminal Session',
        prompt: 'Please run /compact before continuing',
      })
    ).toMatchObject({
      reason: 'eligible',
      shouldAutoName: true,
      strategy: 'generateTitleAndName',
    });
  });

  test('should skip only short slash command prompts at the start of a line', () => {
    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'pi',
        currentTitle: 'Terminal Session',
        prompt: '/compact',
      })
    ).toMatchObject({
      reason: 'slashCommand',
      shouldAutoName: false,
      strategy: 'generateTitleAndName',
    });

    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'pi',
        currentTitle: 'Terminal Session',
        prompt: 'Prep context\n  /compact',
      })
    ).toMatchObject({
      reason: 'slashCommand',
      shouldAutoName: false,
      strategy: 'generateTitleAndName',
    });

    expect(
      explainFirstPromptAutoRenameDecision({
        agentName: 'pi',
        currentTitle: 'Terminal Session',
        prompt:
          '/rename Git: update project board action labels so all spawned GitHub terminals keep the action prefix',
      })
    ).toMatchObject({
      reason: 'eligible',
      shouldAutoName: true,
      strategy: 'generateTitleAndName',
    });
  });
});
