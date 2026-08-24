import { describe, expect, test } from 'vitest';
import { summarizeSessionChatCommandInput } from './session-chat-tool-summary';

describe('command tool previews', () => {
  test('uses at most the first three command lines in one row', () => {
    expect(
      summarizeSessionChatCommandInput({
        command: 'git status\ngit diff\nbun run typecheck\nfourth line',
      })
    ).toBe('git status git diff bun run typecheck');
  });

  test('accepts Codex freeform exec input', () => {
    expect(
      summarizeSessionChatCommandInput(
        'const result = await tools.exec_command({"cmd":"rg foo\\nbun test\\ngit diff\\nfourth"});'
      )
    ).toBe('rg foo bun test git diff');
  });
});
