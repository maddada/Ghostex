import { describe, expect, test } from 'vitest';
import {
  createSidebarAgentButtons,
  normalizeStoredSidebarAgentOrder,
  normalizeStoredSidebarAgents,
  shouldPreferTerminalTitleForAgentIcon,
  supportsTerminalTitleSessionSync,
} from './sidebar-agents';

function sidebarButtons(...args: Parameters<typeof createSidebarAgentButtons>) {
  return createSidebarAgentButtons(...args).map(({ acceptAllMode, ...button }) =>
    acceptAllMode === undefined ? button : { acceptAllMode, ...button }
  );
}

function expectedSidebarButtons<T extends { agentId: string }>(buttons: T[]): T[] {
  return buttons;
}

describe('createSidebarAgentButtons', () => {
  test('should expose the built-in agents by default', () => {
    expect(sidebarButtons([])).toEqual(
      expectedSidebarButtons([
        {
          agentId: 'codex',
          command: 'codex',
          icon: 'codex',
          isDefault: true,
          name: 'Codex',
        },
        {
          agentId: 'claude',
          command: 'claude',
          icon: 'claude',
          isDefault: true,
          name: 'Claude',
        },
        {
          agentId: 'cursor',
          command: 'cursor-agent',
          icon: 'cursor-cli',
          isDefault: true,
          name: 'Cursor CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi Agent',
        },
        {
          agentId: 'opencode',
          command: 'opencode',
          icon: 'opencode',
          isDefault: true,
          name: 'OpenCode',
        },
        {
          agentId: 'gemini',
          command: 'gemini',
          icon: 'gemini',
          isDefault: true,
          name: 'Gemini',
        },
        {
          agentId: 'copilot',
          command: 'copilot',
          icon: 'copilot',
          isDefault: true,
          name: 'Copilot',
        },
        {
          agentId: 'droid',
          command: 'droid',
          icon: 'factory-droid',
          isDefault: true,
          name: 'Factory Droid',
        },
        {
          agentId: 'grok',
          command: 'grok',
          icon: 'grok-build',
          isDefault: true,
          name: 'Grok Build',
        },
        {
          agentId: 'antigravity',
          command: 'agy',
          icon: 'antigravity-cli',
          isDefault: true,
          name: 'Antigravity CLI',
        },
        {
          agentId: 'amp',
          command: 'amp',
          icon: 'amp-cli',
          isDefault: true,
          name: 'Amp CLI',
        },
        {
          agentId: 'hermes-agent',
          command: 'hermes',
          icon: 'hermes-agent',
          isDefault: true,
          name: 'Hermes Agent',
        },
      ])
    );
  });

  test('should apply default command overrides to built-in agents when no stored override exists', () => {
    expect(sidebarButtons([], [], { claude: 'cw', codex: 'x' })).toEqual(
      expectedSidebarButtons([
        {
          agentId: 'codex',
          command: 'x',
          icon: 'codex',
          isDefault: true,
          name: 'Codex',
        },
        {
          agentId: 'claude',
          command: 'cw',
          icon: 'claude',
          isDefault: true,
          name: 'Claude',
        },
        {
          agentId: 'cursor',
          command: 'cursor-agent',
          icon: 'cursor-cli',
          isDefault: true,
          name: 'Cursor CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi Agent',
        },
        {
          agentId: 'opencode',
          command: 'opencode',
          icon: 'opencode',
          isDefault: true,
          name: 'OpenCode',
        },
        {
          agentId: 'gemini',
          command: 'gemini',
          icon: 'gemini',
          isDefault: true,
          name: 'Gemini',
        },
        {
          agentId: 'copilot',
          command: 'copilot',
          icon: 'copilot',
          isDefault: true,
          name: 'Copilot',
        },
        {
          agentId: 'droid',
          command: 'droid',
          icon: 'factory-droid',
          isDefault: true,
          name: 'Factory Droid',
        },
        {
          agentId: 'grok',
          command: 'grok',
          icon: 'grok-build',
          isDefault: true,
          name: 'Grok Build',
        },
        {
          agentId: 'antigravity',
          command: 'agy',
          icon: 'antigravity-cli',
          isDefault: true,
          name: 'Antigravity CLI',
        },
        {
          agentId: 'amp',
          command: 'amp',
          icon: 'amp-cli',
          isDefault: true,
          name: 'Amp CLI',
        },
        {
          agentId: 'hermes-agent',
          command: 'hermes',
          icon: 'hermes-agent',
          isDefault: true,
          name: 'Hermes Agent',
        },
      ])
    );
  });

  test('should merge overrides, rename legacy built-in labels, and append custom agents', () => {
    expect(
      sidebarButtons([
        {
          agentId: 'codex',
          command: 'codex --model gpt-5.4',
          icon: 'codex',
          isDefault: true,
          name: 'Codex CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi',
        },
        {
          agentId: 'aider',
          command: 'aider',
          isDefault: false,
          name: 'Aider',
        },
      ])
    ).toEqual(
      expectedSidebarButtons([
        {
          agentId: 'codex',
          command: 'codex --model gpt-5.4',
          icon: 'codex',
          isDefault: true,
          name: 'Codex',
        },
        {
          agentId: 'claude',
          command: 'claude',
          icon: 'claude',
          isDefault: true,
          name: 'Claude',
        },
        {
          agentId: 'cursor',
          command: 'cursor-agent',
          icon: 'cursor-cli',
          isDefault: true,
          name: 'Cursor CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi Agent',
        },
        {
          agentId: 'opencode',
          command: 'opencode',
          icon: 'opencode',
          isDefault: true,
          name: 'OpenCode',
        },
        {
          agentId: 'gemini',
          command: 'gemini',
          icon: 'gemini',
          isDefault: true,
          name: 'Gemini',
        },
        {
          agentId: 'copilot',
          command: 'copilot',
          icon: 'copilot',
          isDefault: true,
          name: 'Copilot',
        },
        {
          agentId: 'droid',
          command: 'droid',
          icon: 'factory-droid',
          isDefault: true,
          name: 'Factory Droid',
        },
        {
          agentId: 'grok',
          command: 'grok',
          icon: 'grok-build',
          isDefault: true,
          name: 'Grok Build',
        },
        {
          agentId: 'antigravity',
          command: 'agy',
          icon: 'antigravity-cli',
          isDefault: true,
          name: 'Antigravity CLI',
        },
        {
          agentId: 'amp',
          command: 'amp',
          icon: 'amp-cli',
          isDefault: true,
          name: 'Amp CLI',
        },
        {
          agentId: 'hermes-agent',
          command: 'hermes',
          icon: 'hermes-agent',
          isDefault: true,
          name: 'Hermes Agent',
        },
        {
          agentId: 'aider',
          command: 'aider',
          icon: undefined,
          isDefault: false,
          name: 'Aider',
        },
      ])
    );
  });

  test('should hide default agents that are marked hidden', () => {
    expect(
      sidebarButtons([
        {
          agentId: 'codex',
          command: 'codex',
          hidden: true,
          icon: 'codex',
          isDefault: true,
          name: 'Codex',
        },
      ])
    ).toEqual(
      expectedSidebarButtons([
        {
          agentId: 'claude',
          command: 'claude',
          icon: 'claude',
          isDefault: true,
          name: 'Claude',
        },
        {
          agentId: 'cursor',
          command: 'cursor-agent',
          icon: 'cursor-cli',
          isDefault: true,
          name: 'Cursor CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi Agent',
        },
        {
          agentId: 'opencode',
          command: 'opencode',
          icon: 'opencode',
          isDefault: true,
          name: 'OpenCode',
        },
        {
          agentId: 'gemini',
          command: 'gemini',
          icon: 'gemini',
          isDefault: true,
          name: 'Gemini',
        },
        {
          agentId: 'copilot',
          command: 'copilot',
          icon: 'copilot',
          isDefault: true,
          name: 'Copilot',
        },
        {
          agentId: 'droid',
          command: 'droid',
          icon: 'factory-droid',
          isDefault: true,
          name: 'Factory Droid',
        },
        {
          agentId: 'grok',
          command: 'grok',
          icon: 'grok-build',
          isDefault: true,
          name: 'Grok Build',
        },
        {
          agentId: 'antigravity',
          command: 'agy',
          icon: 'antigravity-cli',
          isDefault: true,
          name: 'Antigravity CLI',
        },
        {
          agentId: 'amp',
          command: 'amp',
          icon: 'amp-cli',
          isDefault: true,
          name: 'Amp CLI',
        },
        {
          agentId: 'hermes-agent',
          command: 'hermes',
          icon: 'hermes-agent',
          isDefault: true,
          name: 'Hermes Agent',
        },
      ])
    );
  });

  test('should keep less-common restorable agents hidden until enabled', () => {
    expect(sidebarButtons([]).some((agent) => agent.agentId === 'rovodev')).toBe(false);

    expect(
      sidebarButtons([
        {
          agentId: 'rovodev',
          command: 'acli rovodev run',
          hidden: false,
          icon: 'rovo-dev',
          isDefault: true,
          name: 'Rovo Dev',
        },
      ]).some((agent) => agent.agentId === 'rovodev')
    ).toBe(true);
  });

  test('should keep custom duplicates of default agent types', () => {
    expect(
      sidebarButtons([
        {
          agentId: 'custom-codex-fast',
          command: 'codex --profile fast',
          icon: 'codex',
          isDefault: false,
          name: 'Codex Fast',
        },
      ])
    ).toEqual(
      expectedSidebarButtons([
        {
          agentId: 'codex',
          command: 'codex',
          icon: 'codex',
          isDefault: true,
          name: 'Codex',
        },
        {
          agentId: 'claude',
          command: 'claude',
          icon: 'claude',
          isDefault: true,
          name: 'Claude',
        },
        {
          agentId: 'cursor',
          command: 'cursor-agent',
          icon: 'cursor-cli',
          isDefault: true,
          name: 'Cursor CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi Agent',
        },
        {
          agentId: 'opencode',
          command: 'opencode',
          icon: 'opencode',
          isDefault: true,
          name: 'OpenCode',
        },
        {
          agentId: 'gemini',
          command: 'gemini',
          icon: 'gemini',
          isDefault: true,
          name: 'Gemini',
        },
        {
          agentId: 'copilot',
          command: 'copilot',
          icon: 'copilot',
          isDefault: true,
          name: 'Copilot',
        },
        {
          agentId: 'droid',
          command: 'droid',
          icon: 'factory-droid',
          isDefault: true,
          name: 'Factory Droid',
        },
        {
          agentId: 'grok',
          command: 'grok',
          icon: 'grok-build',
          isDefault: true,
          name: 'Grok Build',
        },
        {
          agentId: 'antigravity',
          command: 'agy',
          icon: 'antigravity-cli',
          isDefault: true,
          name: 'Antigravity CLI',
        },
        {
          agentId: 'amp',
          command: 'amp',
          icon: 'amp-cli',
          isDefault: true,
          name: 'Amp CLI',
        },
        {
          agentId: 'hermes-agent',
          command: 'hermes',
          icon: 'hermes-agent',
          isDefault: true,
          name: 'Hermes Agent',
        },
        {
          agentId: 'custom-codex-fast',
          command: 'codex --profile fast',
          icon: 'codex',
          isDefault: false,
          name: 'Codex Fast',
        },
      ])
    );
  });

  test('should respect stored agent ordering across defaults and custom entries', () => {
    expect(
      sidebarButtons(
        [
          {
            agentId: 'custom-codex-fast',
            command: 'codex --profile fast',
            icon: 'codex',
            isDefault: false,
            name: 'Codex Fast',
          },
        ],
        ['gemini', 'custom-codex-fast', 'claude']
      )
    ).toEqual(
      expectedSidebarButtons([
        {
          agentId: 'gemini',
          command: 'gemini',
          icon: 'gemini',
          isDefault: true,
          name: 'Gemini',
        },
        {
          agentId: 'custom-codex-fast',
          command: 'codex --profile fast',
          icon: 'codex',
          isDefault: false,
          name: 'Codex Fast',
        },
        {
          agentId: 'claude',
          command: 'claude',
          icon: 'claude',
          isDefault: true,
          name: 'Claude',
        },
        {
          agentId: 'codex',
          command: 'codex',
          icon: 'codex',
          isDefault: true,
          name: 'Codex',
        },
        {
          agentId: 'cursor',
          command: 'cursor-agent',
          icon: 'cursor-cli',
          isDefault: true,
          name: 'Cursor CLI',
        },
        {
          agentId: 'pi',
          command: 'pi',
          icon: 'pi',
          isDefault: true,
          name: 'Pi Agent',
        },
        {
          agentId: 'opencode',
          command: 'opencode',
          icon: 'opencode',
          isDefault: true,
          name: 'OpenCode',
        },
        {
          agentId: 'copilot',
          command: 'copilot',
          icon: 'copilot',
          isDefault: true,
          name: 'Copilot',
        },
        {
          agentId: 'droid',
          command: 'droid',
          icon: 'factory-droid',
          isDefault: true,
          name: 'Factory Droid',
        },
        {
          agentId: 'grok',
          command: 'grok',
          icon: 'grok-build',
          isDefault: true,
          name: 'Grok Build',
        },
        {
          agentId: 'antigravity',
          command: 'agy',
          icon: 'antigravity-cli',
          isDefault: true,
          name: 'Antigravity CLI',
        },
        {
          agentId: 'amp',
          command: 'amp',
          icon: 'amp-cli',
          isDefault: true,
          name: 'Amp CLI',
        },
        {
          agentId: 'hermes-agent',
          command: 'hermes',
          icon: 'hermes-agent',
          isDefault: true,
          name: 'Hermes Agent',
        },
      ])
    );
  });
});

describe('shouldPreferTerminalTitleForAgentIcon', () => {
  test('should prefer terminal titles for OpenCode', () => {
    expect(shouldPreferTerminalTitleForAgentIcon('opencode')).toBe(true);
    expect(shouldPreferTerminalTitleForAgentIcon('pi')).toBe(true);
    expect(shouldPreferTerminalTitleForAgentIcon('cursor-cli')).toBe(true);
    expect(shouldPreferTerminalTitleForAgentIcon('antigravity-cli')).toBe(true);
  });
});

describe('supportsTerminalTitleSessionSync', () => {
  test('should allow Cursor CLI terminal titles to sync like Codex', () => {
    expect(supportsTerminalTitleSessionSync('cursor')).toBe(true);
    expect(supportsTerminalTitleSessionSync('Cursor CLI')).toBe(true);
    expect(supportsTerminalTitleSessionSync('cursor-agent')).toBe(true);
    expect(supportsTerminalTitleSessionSync('agy')).toBe(true);
    expect(supportsTerminalTitleSessionSync('Antigravity CLI')).toBe(true);
    expect(supportsTerminalTitleSessionSync('codex')).toBe(true);
    expect(supportsTerminalTitleSessionSync('amp')).toBe(false);
  });
});

describe('normalizeStoredSidebarAgents', () => {
  test('should trim valid entries and ignore invalid ones', () => {
    expect(
      normalizeStoredSidebarAgents([
        {
          agentId: ' codex ',
          command: ' codex ',
          hidden: true,
          icon: 'codex',
          isDefault: true,
          name: ' Codex ',
        },
        {
          agentId: 'broken',
          command: '',
          isDefault: false,
          name: 'Broken',
        },
      ])
    ).toEqual([
      {
        agentId: 'codex',
        command: 'codex',
        hidden: true,
        icon: 'codex',
        isDefault: true,
        name: 'Codex',
      },
    ]);
  });
});

describe('normalizeStoredSidebarAgentOrder', () => {
  test('should trim, dedupe, and ignore invalid order entries', () => {
    expect(normalizeStoredSidebarAgentOrder([' codex ', 'gemini', 'codex', 123, '', ' gemini '])).toEqual([
      'codex',
      'gemini',
    ]);
  });
});
