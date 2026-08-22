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
    configs: [],
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

function AgentsHubModalStory({ initialTab }: { initialTab: AgentsHubTab }) {
  return (
    <div
      style={{
        background: "#0e0e0e",
        height: "100vh",
        width: "100vw",
      }}
    >
      <AgentsHubModal
        catalog={mockCatalog}
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
