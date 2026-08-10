import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, waitFor, within } from "storybook/test";
import { SettingsModal } from "./settings-modal";
import { DEFAULT_ghostex_SETTINGS, type ghostexSettings } from "../shared/ghostex-settings";
import { DEFAULT_SIDEBAR_AGENTS } from "../shared/sidebar-agents";
import type {
  SidebarAgentHookStatusMessage,
  SidebarGhostexCliStatusMessage,
  SidebarProjectSettingsItem,
} from "../shared/session-grid-contract";

const modalSettings: ghostexSettings = {
  ...DEFAULT_ghostex_SETTINGS,
  agentManagerZoomPercent: 95,
  completionBellEnabled: true,
  showCloseButtonOnSessionCards: true,
  terminalFontSize: 16,
  terminalFontWeight: 400,
  terminalLineHeight: 1.35,
};

const storyProjects: SidebarProjectSettingsItem[] = [
  {
    beadsDirectory: "",
    beadsDisplayKey: "ZMX",
    name: "Ghostex",
    path: "/Users/you/dev/ghostex",
    projectId: "project-ghostex",
    worktreeCommand: "bun install",
  },
  {
    beadsDirectory: "/Users/you/dev/infra/.beads",
    beadsDisplayKey: "INF",
    name: "Infra Control Plane",
    path: "/Users/you/dev/platform/infra-control-plane",
    projectId: "project-infra",
    worktreeCommand: "pnpm install",
  },
  {
    beadsDirectory: "",
    beadsDisplayKey: "WEB",
    name: "Customer Web",
    path: "/Users/you/dev/products/customer-web-application",
    projectId: "project-web",
    worktreeCommand: "",
  },
  {
    beadsDirectory: "",
    beadsDisplayKey: "OPS",
    name: "Operations Dashboard",
    path: "/Users/you/dev/internal/tools/operations-dashboard",
    projectId: "project-ops",
    worktreeCommand: "bun run setup",
  },
];

function SettingsModalStory({
  cuaPermissionsGranted,
  initialSettings = modalSettings,
  initialTab = "settings",
  projects,
}: {
  cuaPermissionsGranted?: boolean;
  initialSettings?: ghostexSettings;
  initialTab?: "settings" | "integrations" | "projects" | "agents" | "actions" | "openTargets" | "hotkeys";
  projects?: SidebarProjectSettingsItem[];
}) {
  const [settings, setSettings] = useState<ghostexSettings>(initialSettings);
  const [agentHookStatus, setAgentHookStatus] = useState<SidebarAgentHookStatusMessage>({
    agents: DEFAULT_SIDEBAR_AGENTS.map((agent, index) => ({
      agentId: agent.agentId,
      cliCommand: agent.command.split(" ")[0] ?? agent.command,
      cliInstalled: index < 10,
      detail: index < 4 ? "Hook config is installed." : "Hook config is not installed.",
      hookInstalled: index < 4,
      paths: [`~/.ghostex/mock-hooks/${agent.agentId}.json`],
      status: index < 4 ? "installed" : index < 10 ? "missing" : "cliMissing",
    })),
    generatedAt: "2026-05-27T04:17:00.000Z",
    hookStateDirectory: "~/.ghostexterm",
    notifyHookPath: "~/.ghostexterm/notify-agent-status.js",
    type: "agentHookStatus",
  });
  const [ghostexCliStatus, setGhostexCliStatus] = useState<SidebarGhostexCliStatusMessage>({
    agentOrchestrationSkillInstalled: false,
    browserSkillInstalled: false,
    computerUseSkillInstalled: false,
    embeddedBrowserSkillInstalled: false,
    cuaAppInstalled: false,
    cuaDriverAccessibilityPermissionGranted: cuaPermissionsGranted,
    cuaDriverInstalled: cuaPermissionsGranted !== undefined,
    cuaDriverScreenRecordingPermissionGranted: cuaPermissionsGranted,
    detail: "Ghostex CLI is installed automatically with the app. Use ghostex for the full command. Ghostex Browser Use and Ghostex Computer Use are not installed yet.",
    generateTitleSkillInstalled: false,
    generatedAt: "2026-05-27T04:17:00.000Z",
    ghostexPath: "/opt/homebrew/bin/ghostex",
    gxBlockedByExistingCommand: false,
    gxUsable: false,
    installed: true,
    moveCodexSessionSkillInstalled: false,
    type: "ghostexCliStatus",
  });

  return (
    <div
      style={{
        background: "#0e0e0e",
        height: "100vh",
        width: "100vw",
      }}
    >
      <SettingsModal
        agentHookStatus={agentHookStatus}
        ghostexCliStatus={ghostexCliStatus}
        initialTab={initialTab}
        isOpen
        onChange={setSettings}
        onClose={() => undefined}
        onInstallAgentHooks={() =>
          setAgentHookStatus({
            ...agentHookStatus,
            agents: agentHookStatus.agents.map((agent) =>
              agent.cliInstalled
                ? { ...agent, detail: "Hook config is installed.", hookInstalled: true, status: "installed" }
                : agent,
            ),
          })
        }
        onInstallBrowserControl={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            embeddedBrowserSkillInstalled: true,
            embeddedBrowserSkillPath:
              "/Users/madda/agents/skills/ghostex-embedded-browser-use/SKILL.md",
          })
        }
        onInstallBrowserUseSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            browserSkillInstalled: true,
            browserSkillPath: "/Users/madda/agents/skills/ghostex-browser-use/SKILL.md",
          })
        }
        onInstallComputerUseSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            computerUseSkillInstalled: true,
            computerUseSkillPath: "/Users/madda/agents/skills/ghostex-computer-use/SKILL.md",
          })
        }
        onInstallAgentOrchestrationSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            agentOrchestrationSkillInstalled: true,
            agentOrchestrationSkillPath:
              "/Users/madda/agents/skills/ghostex-agent-orchestration/SKILL.md",
          })
        }
        onInstallGenerateTitleSkill={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            generateTitleSkillInstalled: true,
            generateTitleSkillPath: "/Users/madda/agents/skills/ghostex-auto-rename-session/SKILL.md",
          })
        }
        onInstallCuaDriver={() =>
          setGhostexCliStatus({
            ...ghostexCliStatus,
            computerUseSkillInstalled: true,
            computerUseSkillPath: "/Users/madda/agents/skills/ghostex-computer-use/SKILL.md",
            cuaAppInstalled: true,
            cuaDriverAccessibilityPermissionGranted: true,
            cuaDriverInstalled: true,
            cuaDriverPath: "/Users/madda/.local/bin/cua-driver",
            cuaDriverScreenRecordingPermissionGranted: true,
          })
        }
        onInstallGhostexCli={() => setGhostexCliStatus({ ...ghostexCliStatus, installed: true })}
        onOpenAccessibilityPreferences={() => undefined}
        onOpenScreenRecordingPreferences={() => undefined}
        onRequestAgentHookStatus={() => undefined}
        onRequestGhostexCliStatus={() => undefined}
        projects={projects}
        settings={settings}
        theme={settings.sidebarTheme === "light-orange" ? "light-orange" : "dark-blue"}
      />
    </div>
  );
}

const meta = {
  title: "Sidebar/Settings Modal",
  parameters: {
    layout: "fullscreen",
  },
  render: () => <SettingsModalStory />,
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const DarkGray: Story = {
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        sidebarTheme: "plain",
      }}
    />
  ),
};

/*
 * CDXC:SidebarV2 2026-07-29:
 * Sidebar version is the first General setting. This story selects the Inbox
 * sidebar so the nested Group by project row is visible for review.
 */
export const SidebarVersionInbox: Story = {
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        sidebarV2Layout: "byProject",
        sidebarVersion: "v2",
      }}
    />
  ),
  play: async ({ canvasElement, step }) => {
    const body = within(canvasElement.ownerDocument.body);

    /*
     * CDXC:SidebarV2Lifecycle 2026-07-29:
     * Auto-settle is nested under the Inbox sidebar because it drives a shelf
     * only that sidebar has. This pins the nesting: the row must appear with
     * V2 selected and disappear when the user goes back to Classic, so the
     * classic sidebar never advertises a shelf that does not exist.
     */
    await step("show the auto-settle window while the Inbox sidebar is selected", async () => {
      await body.findByText("Auto-settle inactive sessions");
    });

    await step("hide it again when the classic sidebar is selected", async () => {
      await userEvent.click(await body.findByRole("button", { name: "Classic" }));
      await waitFor(() => {
        expect(body.queryByText("Auto-settle inactive sessions")).toBeNull();
      });
    });
  },
};

export const AccessibilityOff: Story = {
  render: () => <SettingsModalStory cuaPermissionsGranted={false} />,
};

export const Integrations: Story = {
  render: () => <SettingsModalStory cuaPermissionsGranted={false} initialTab="integrations" />,
};

export const Projects: Story = {
  render: () => <SettingsModalStory initialTab="projects" projects={storyProjects} />,
};

export const LightOrange: Story = {
  render: () => (
    <SettingsModalStory
      initialSettings={{
        ...modalSettings,
        sidebarTheme: "light-orange",
      }}
    />
  ),
};

export const NarrowModal: Story = {
  parameters: {
    viewport: {
      defaultViewport: "narrowSettings",
      viewports: {
        narrowSettings: {
          name: "Narrow settings modal",
          styles: {
            height: "900px",
            width: "520px",
          },
        },
      },
    },
  },
  render: () => <SettingsModalStory />,
};
