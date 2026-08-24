import type { Meta, StoryObj } from "@storybook/react-vite";
import { AgentsHubModal } from "./agents-hub-modal";
import type { WebviewApi } from "./webview-api";
import type { AgentsHubCatalogMessage, AgentsHubTab } from "../shared/session-grid-contract";

const mockVscode: WebviewApi = {
  postMessage: () => undefined,
};

const mockCatalog: AgentsHubCatalogMessage = {
  generatedAt: "2026-05-15T11:41:00.000Z",
  groupsByTab: {
    configs: [
      {
        description: "MCP servers and CLI config owned by the Codex profile.",
        files: [
          {
            content: '{\n  "mcpServers": {}\n}\n',
            id: "codex-config",
            language: "json",
            name: "config.toml",
            path: "/Users/madda/.codex/config.toml",
          },
          {
            content: '{\n  "servers": []\n}\n',
            id: "codex-mcp",
            language: "json",
            name: "mcp.json",
            path: "/Users/madda/.codex/mcp.json",
          },
        ],
        id: "config-codex",
        name: "Codex configuration",
        path: "/Users/madda/.codex",
        profiles: [
          {
            agentIcon: "codex",
            filePath: "/Users/madda/.codex/config.toml",
            label: "Codex main",
            profilePath: "/Users/madda/.codex",
          },
          {
            agentIcon: "claude",
            filePath: "/Users/madda/.claude-profiles/work/settings.json",
            label: "Claude Code work",
            profilePath: "/Users/madda/.claude-profiles/work",
          },
        ],
      },
      {
        description: "Claude Code settings for every installed profile.",
        files: [
          {
            content: '{\n  "permissions": {}\n}\n',
            id: "claude-settings",
            language: "json",
            name: "settings.json",
            path: "/Users/madda/.claude/settings.json",
          },
        ],
        id: "config-claude",
        name: "Claude Code settings",
        path: "/Users/madda/.claude",
        profiles: [
          {
            agentIcon: "claude",
            filePath: "/Users/madda/.claude/settings.json",
            label: "Claude Code main",
            profilePath: "/Users/madda/.claude",
          },
        ],
      },
    ],
    hooks: [],
    mds: [
      {
        description: "CLAUDE.md files owned by Claude profiles.",
        files: [
          {
            content: "# Claude Code work\n\nProject instructions.",
            id: "claude-code-work",
            language: "markdown",
            name: "work/CLAUDE.md",
            path: "/Users/madda/.claude-profiles/work/CLAUDE.md",
          },
        ],
        id: "md-claude-profiles",
        name: "Claude profile instructions",
        path: "/Users/madda/.claude-profiles",
        profiles: [
          {
            agentIcon: "claude",
            filePath: "/Users/madda/.claude-profiles/work/CLAUDE.md",
            label: "Claude Code work",
            profilePath: "/Users/madda/.claude-profiles/work",
            targetPath: "/Users/madda/.agents/main.md",
          },
        ],
      },
    ],
    skills: [
      {
        description: "Shared skill installed under ~/agents/skills.",
        files: [
          {
            content: "---\nname: tooltip-cleanup\n---\n\nSkill instructions.",
            id: "tooltip-cleanup-skill",
            language: "markdown",
            name: "SKILL.md",
            path: "/Users/madda/agents/skills/tooltip-cleanup/SKILL.md",
          },
        ],
        id: "skill-shared-tooltip-cleanup",
        name: "tooltip-cleanup",
        path: "/Users/madda/agents/skills/tooltip-cleanup",
        profiles: [
          {
            agentIcon: "codex",
            filePath: "/Users/madda/.codex/AGENTS.md",
            label: "Codex main",
            profilePath: "/Users/madda/.codex",
            targetPath: "/Users/madda/.agents/main.md",
          },
        ],
      },
    ],
  },
  type: "agentsHubCatalog",
};

const emptyCatalog: AgentsHubCatalogMessage = {
  generatedAt: "2026-08-24T09:00:00.000Z",
  groupsByTab: { configs: [], hooks: [], mds: [], skills: [] },
  type: "agentsHubCatalog",
};

function AgentsHubModalStory({
  catalog = mockCatalog,
  initialTab,
}: {
  catalog?: AgentsHubCatalogMessage;
  initialTab: AgentsHubTab;
}) {
  return (
    <div
      style={{
        background: "#0e0e0e",
        height: "100vh",
        width: "100vw",
      }}
    >
      <AgentsHubModal
        catalog={catalog}
        initialTab={initialTab}
        isOpen
        onClose={() => undefined}
        vscode={mockVscode}
      />
    </div>
  );
}

const meta = {
  title: "Sidebar/Agents Hub Modal",
  parameters: {
    layout: "fullscreen",
  },
  render: () => (
    /**
     * CDXC:AgentsHub 2026-05-13-08:08:
     * The default story opens the Skills tab because left-card tree clipping was reported there and needs a stable visual regression target.
     */
    <AgentsHubModalStory initialTab="skills" />
  ),
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const Skills: Story = {};

export const ProfileTooltip: Story = {
  render: () => (
    /**
     * CDXC:AgentsHub 2026-05-15-15:49:
     * The tooltip story opens the MDs tab with a linked Claude work profile so profile tooltip spacing, path wrapping, and target-arrow layout can be inspected against the real profile-link content shape.
     */
    <AgentsHubModalStory initialTab="mds" />
  ),
};

export const ConfigsWithSelectedFile: Story = {
  render: () => (
    /*
     * CDXC:AgentsHubRedesign 2026-08-24:
     * The Configs tab auto-expands its groups, so this story is the review
     * target for the redesigned raised group cards, the nested file tree, the
     * accent-highlighted selected file row, and the profile chips.
     */
    <AgentsHubModalStory initialTab="configs" />
  ),
};

export const EmptyCatalog: Story = {
  render: () => (
    /*
     * CDXC:AgentsHubRedesign 2026-08-24:
     * Empty-state chrome (list pane and editor frame with no selection) has to
     * read as the same quiet panel surface as the populated modal.
     */
    <AgentsHubModalStory catalog={emptyCatalog} initialTab="configs" />
  ),
};
