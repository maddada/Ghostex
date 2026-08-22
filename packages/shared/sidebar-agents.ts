import {
  normalizeAgentAcceptAllMode,
  type AgentAcceptAllMode,
} from "./sidebar-agent-accept-all";
const DEFAULT_SIDEBAR_AGENT_DEFINITIONS = [
  /**
   * CDXC:SidebarAgents 2026-05-15-15:25:
   * The default model picker should present the first built-in launch engines
   * in the user-facing order Codex, Claude, Cursor CLI, Pi Agent, OpenCode,
   * Gemini, Copilot, Factory Droid, Grok Build, Antigravity CLI, and
   * Amp CLI so the top of the menu matches the expected daily-selection flow.
   */
  {
    agentId: "codex",
    command: "codex",
    icon: "codex",
    name: "Codex",
  },
  {
    agentId: "claude",
    command: "claude",
    icon: "claude",
    name: "Claude",
  },
  /**
   * CDXC:SidebarAgents 2026-05-19-09:10:
   * Cursor CLI is a built-in launch engine directly under Claude in the default
   * agent list. Launch it with `cursor-agent` and reuse the Cursor editor
   * logomark so sidebar cards, configure-agent rows, and native title bars stay
   * visually aligned with the existing Cursor open-target branding.
   */
  {
    agentId: "cursor",
    command: "cursor-agent",
    icon: "cursor-cli",
    name: "Cursor CLI",
  },
  /**
   * CDXC:PiAgent 2026-05-15-15:25:
   * Pi is a first-class default agent so the configure-agent modal, sidebar
   * launch buttons, automatic icon selection, and restore commands can share
   * the same default-agent registry used by Codex. The model picker labels the
   * Pi CLI as Pi Agent to distinguish the launch engine from the shorter
   * executable name while keeping `pi` as the command.
   */
  {
    agentId: "pi",
    command: "pi",
    icon: "pi",
    name: "Pi Agent",
  },
  {
    agentId: "opencode",
    command: "opencode",
    icon: "opencode",
    name: "OpenCode",
  },
  {
    agentId: "gemini",
    command: "gemini",
    icon: "gemini",
    name: "Gemini",
  },
  {
    agentId: "copilot",
    command: "copilot",
    icon: "copilot",
    name: "Copilot",
  },
  /**
   * CDXC:SidebarAgents 2026-05-15-12:41:
   * Factory Droid is a built-in launch engine that belongs at the bottom of
   * the default agent list. Launch it through Factory's `droid` CLI command
   * while keeping the display label explicit enough to distinguish the vendor.
   */
  {
    agentId: "droid",
    command: "droid",
    icon: "factory-droid",
    name: "Factory Droid",
  },
  /**
   * CDXC:SidebarAgents 2026-05-15-12:45:
   * Grok Build is a built-in launch engine at the end of the default list.
   * xAI's current setup docs install Grok Build and launch the interactive CLI
   * with `grok`, so use that executable while keeping the product name visible.
   */
  {
    agentId: "grok",
    command: "grok",
    icon: "grok-build",
    name: "Grok Build",
  },
  /**
   * CDXC:SidebarAgents 2026-05-19-14:40:
   * Antigravity CLI is a built-in launch engine just above Amp at the bottom of
   * the default agent list. Launch it with `agy` and use the Antigravity
   * logomark path from the official icon as a mask-friendly sidebar asset.
   */
  {
    agentId: "antigravity",
    command: "agy",
    icon: "antigravity-cli",
    name: "Antigravity CLI",
  },
  /**
   * CDXC:SidebarAgents 2026-05-19-09:10:
   * Amp CLI is a built-in launch engine at the bottom of the default agent list.
   * Launch it with Sourcegraph's `amp` command and use the official Amp wordmark
   * so the sidebar launcher matches Amp's CLI branding.
   */
  {
    agentId: "amp",
    command: "amp",
    icon: "amp-cli",
    name: "Amp CLI",
  },
  /**
   * CDXC:SidebarAgents 2026-07-02-14:05:
   * Hermes Agent is a built-in launch engine at the bottom of the default
   * agent list. The `hermes` CLI opens the interactive Hermes Agent TUI, and
   * gxserver already owns its hook install, resume shaping, and detection, so
   * the launcher shows it by default like the other first-class engines.
   */
  {
    agentId: "hermes-agent",
    command: "hermes",
    icon: "hermes-agent",
    name: "Hermes Agent",
  },
  /**
   * CDXC:SessionRestore 2026-05-23-00:25:
   * Keep Rovo Dev, CodeBuddy, and Qoder in the restorable-agent
   * surface, but keep these less-common launchers hidden until the user
   * explicitly enables or configures them.
   */
  {
    agentId: "rovodev",
    command: "acli rovodev run",
    hiddenByDefault: true,
    icon: "rovo-dev",
    name: "Rovo Dev",
  },
  {
    agentId: "codebuddy",
    command: "codebuddy",
    hiddenByDefault: true,
    icon: "codebuddy",
    name: "CodeBuddy",
  },
  {
    agentId: "qoder",
    command: "qodercli",
    hiddenByDefault: true,
    icon: "qoder",
    name: "Qoder",
  },
  /*
   * CDXC:AgentHooks 2026-06-11-22:19:
   * Kiro CLI and OMP are hook-supported lower-priority agents. Keep them hidden
   * from the primary launcher by default, but include them in the shared agent
   * registry so Settings and first launch request gxserver hook status for the
   * same provider set that the server can install.
   */
  {
    agentId: "kiro",
    command: "kiro-cli chat --agent ghostex",
    hiddenByDefault: true,
    icon: "kiro",
    name: "Kiro CLI",
  },
  {
    agentId: "omp",
    command: "omp",
    hiddenByDefault: true,
    icon: "omp",
    name: "OMP",
  },
] as const;

export const DEFAULT_SIDEBAR_AGENTS = DEFAULT_SIDEBAR_AGENT_DEFINITIONS;

export type DefaultSidebarAgent = (typeof DEFAULT_SIDEBAR_AGENT_DEFINITIONS)[number];
export type DefaultSidebarAgentId = DefaultSidebarAgent["agentId"];
export type SidebarAgentIcon = "browser" | DefaultSidebarAgent["icon"];
export type DefaultSidebarAgentCommandOverrides = Partial<
  Record<DefaultSidebarAgentId, string | null>
>;

export type SidebarAgentButton = {
  acceptAllMode?: AgentAcceptAllMode;
  agentId: string;
  command?: string;
  icon?: SidebarAgentIcon;
  isDefault: boolean;
  name: string;
};

export type StoredSidebarAgent = {
  acceptAllMode?: AgentAcceptAllMode;
  agentId: string;
  command: string;
  hidden?: boolean;
  icon?: SidebarAgentIcon;
  isDefault: boolean;
  name: string;
};

/**
 * CDXC:SessionHistoryTitleSource 2026-07-29:
 * Empty-title Generate Name asks gxserver to summarize the session's recent
 * transcript user messages. Only agents whose local transcript format gxserver
 * can read support this; other agents keep requiring pasted text in the
 * rename modal.
 */
export function sidebarAgentIconSupportsSessionHistoryTitleGeneration(
  icon: SidebarAgentIcon | string | undefined,
): boolean {
  return icon === "claude" || icon === "codex" || icon === "cursor-cli";
}

export function createSidebarAgentSelectItems(
  agents: readonly Pick<SidebarAgentButton, "agentId" | "name">[],
): Array<{ label: string; value: string }> {
  /*
   * CDXC:PromptAgents 2026-05-30-07:46:
   * Collapsed macOS agent selects must show the configured friendly title, not
   * the persisted agent id value. Base UI needs root-level items because the
   * popup options are not mounted when the trigger renders its initial value.
   */
  return agents.map((agent) => ({
    label: agent.name,
    value: agent.agentId,
  }));
}

export function createDefaultSidebarAgentButtons(
  commandOverrides: DefaultSidebarAgentCommandOverrides = {},
): SidebarAgentButton[] {
  return DEFAULT_SIDEBAR_AGENTS.flatMap((agent) =>
    "hiddenByDefault" in agent && agent.hiddenByDefault === true
      ? []
      : [
          {
            agentId: agent.agentId,
            command: commandOverrides[agent.agentId] ?? agent.command,
            icon: agent.icon,
            isDefault: true,
            name: agent.name,
          },
        ],
  );
}

export function createSidebarAgentButtons(
  storedAgents: readonly StoredSidebarAgent[],
  storedOrder: readonly string[] = [],
  commandOverrides: DefaultSidebarAgentCommandOverrides = {},
): SidebarAgentButton[] {
  const enabledStoredAgents = storedAgents.filter(
    (agent) => !agent.isDefault || isDefaultSidebarAgentId(agent.agentId),
  );
  const storedAgentById = new Map(enabledStoredAgents.map((agent) => [agent.agentId, agent]));
  const defaultButtons = DEFAULT_SIDEBAR_AGENTS.flatMap((agent) => {
    const storedAgent = storedAgentById.get(agent.agentId);
    if (!storedAgent && "hiddenByDefault" in agent && agent.hiddenByDefault === true) {
      return [];
    }
    if (storedAgent?.hidden === true) {
      return [];
    }

    if (!storedAgent) {
      return [
        {
          acceptAllMode: undefined,
          agentId: agent.agentId,
          command: commandOverrides[agent.agentId] ?? agent.command,
          icon: agent.icon,
          isDefault: true,
          name: agent.name,
        },
      ];
    }

    return [
      {
        acceptAllMode: storedAgent.acceptAllMode,
        agentId: storedAgent.agentId,
        command: storedAgent.command,
        icon: storedAgent.icon ?? agent.icon,
        isDefault: true,
        name: getDefaultSidebarAgentName(agent.agentId, storedAgent.name),
      },
    ];
  });

  const customButtons = enabledStoredAgents
    .filter((agent) => !isDefaultSidebarAgentId(agent.agentId) && agent.hidden !== true)
    .map((agent) => ({
      acceptAllMode: agent.acceptAllMode,
      agentId: agent.agentId,
      command: agent.command,
      icon: agent.icon,
      isDefault: false,
      name: agent.name,
    }));

  return orderSidebarAgentButtons([...defaultButtons, ...customButtons], storedOrder);
}

export function isDefaultSidebarAgentId(agentId: string): boolean {
  return DEFAULT_SIDEBAR_AGENTS.some((agent) => agent.agentId === agentId);
}

export function getDefaultSidebarAgentById(
  agentId: string | undefined,
): DefaultSidebarAgent | undefined {
  const normalizedAgentId = agentId?.trim().toLowerCase();
  return DEFAULT_SIDEBAR_AGENTS.find((agent) => agent.agentId === normalizedAgentId);
}

export function getDefaultSidebarAgentByIcon(
  icon: SidebarAgentIcon | undefined,
): DefaultSidebarAgent | undefined {
  if (!icon || icon === "browser") {
    return undefined;
  }

  return DEFAULT_SIDEBAR_AGENTS.find((agent) => agent.icon === icon);
}

export function getSidebarAgentIconById(agentId: string | undefined): SidebarAgentIcon | undefined {
  return getDefaultSidebarAgentById(agentId)?.icon;
}

export function getSidebarAgentNameByIcon(icon: SidebarAgentIcon | undefined): string | undefined {
  if (icon === "browser") {
    return "Browser";
  }

  return DEFAULT_SIDEBAR_AGENTS.find((agent) => agent.icon === icon)?.name;
}

/**
 * CDXC:CursorCLI 2026-05-19-15:35:
 * Cursor CLI auto-generates conversation titles in the terminal. Include Cursor
 * alongside the other agents whose live terminal titles can drive sidebar cards
 * and be persisted through native terminal-title sync.
 */
const TERMINAL_TITLE_SESSION_SYNC_AGENT_IDS = new Set<DefaultSidebarAgentId>([
  "antigravity",
  "claude",
  "codex",
  "copilot",
  "cursor",
  "gemini",
  "hermes-agent",
  "opencode",
  "pi",
  "qoder",
  "rovodev",
]);

export function supportsTerminalTitleSessionSync(agentName: string | undefined): boolean {
  const normalizedAgentName = agentName?.trim().toLowerCase();
  if (!normalizedAgentName) {
    return false;
  }

  if (TERMINAL_TITLE_SESSION_SYNC_AGENT_IDS.has(normalizedAgentName as DefaultSidebarAgentId)) {
    return true;
  }

  if (
    normalizedAgentName === "claude code" ||
    normalizedAgentName === "codex cli" ||
    normalizedAgentName === "agy" ||
    normalizedAgentName === "antigravity cli" ||
    normalizedAgentName === "cursor agent" ||
    normalizedAgentName === "cursor cli" ||
    normalizedAgentName === "cursor-agent" ||
    normalizedAgentName === "github copilot" ||
    normalizedAgentName === "hermes" ||
    normalizedAgentName === "hermes agent" ||
    normalizedAgentName === "open code" ||
    normalizedAgentName === "qodercli" ||
    normalizedAgentName === "rovo" ||
    normalizedAgentName === "rovo dev" ||
    normalizedAgentName === "π"
  ) {
    return true;
  }

  const defaultAgent = DEFAULT_SIDEBAR_AGENTS.find(
    (agent) =>
      agent.agentId === normalizedAgentName ||
      agent.name.trim().toLowerCase() === normalizedAgentName,
  );
  return defaultAgent
    ? TERMINAL_TITLE_SESSION_SYNC_AGENT_IDS.has(defaultAgent.agentId)
    : false;
}

export function shouldPreferTerminalTitleForAgentIcon(icon: SidebarAgentIcon | undefined): boolean {
  return (
    icon === "antigravity-cli" ||
    icon === "claude" ||
    icon === "codex" ||
    icon === "cursor-cli" ||
    icon === "hermes-agent" ||
    icon === "opencode" ||
    icon === "pi" ||
    icon === "qoder" ||
    icon === "rovo-dev"
  );
}

export function normalizeStoredSidebarAgents(candidate: unknown): StoredSidebarAgent[] {
  if (!Array.isArray(candidate)) {
    return [];
  }

  const normalizedAgents: StoredSidebarAgent[] = [];
  const seenAgentIds = new Set<string>();

  for (const item of candidate) {
    if (!item || typeof item !== "object") {
      continue;
    }

    const partialItem = item as Partial<StoredSidebarAgent>;
    const agentId = partialItem.agentId?.trim();
    const name = partialItem.name?.trim();
    const command = partialItem.command?.trim();
    const icon = isSidebarAgentIcon(partialItem.icon) ? partialItem.icon : undefined;
    const acceptAllMode = normalizeAgentAcceptAllMode(partialItem.acceptAllMode);
    const isDefault =
      partialItem.isDefault === true || (agentId ? isDefaultSidebarAgentId(agentId) : false);
    const hidden = partialItem.hidden === true;

    if (!agentId || !name || !command || seenAgentIds.has(agentId)) {
      continue;
    }

    normalizedAgents.push({
      acceptAllMode,
      agentId,
      command,
      hidden,
      icon,
      isDefault,
      name,
    });
    seenAgentIds.add(agentId);
  }

  return normalizedAgents;
}

export function normalizeStoredSidebarAgentOrder(candidate: unknown): string[] {
  if (!Array.isArray(candidate)) {
    return [];
  }

  const normalizedOrder: string[] = [];
  const seenAgentIds = new Set<string>();

  for (const item of candidate) {
    if (typeof item !== "string") {
      continue;
    }

    const agentId = item.trim();
    if (!agentId || seenAgentIds.has(agentId)) {
      continue;
    }

    normalizedOrder.push(agentId);
    seenAgentIds.add(agentId);
  }

  return normalizedOrder;
}

function isSidebarAgentIcon(candidate: unknown): candidate is SidebarAgentIcon {
  if (candidate === "browser") {
    return true;
  }

  return (
    typeof candidate === "string" &&
    DEFAULT_SIDEBAR_AGENTS.some((agent) => agent.icon === candidate)
  );
}

function getDefaultSidebarAgentName(agentId: string, storedName: string): string {
  const defaultName = DEFAULT_SIDEBAR_AGENTS.find((agent) => agent.agentId === agentId)?.name;
  if (!defaultName) {
    return storedName;
  }

  const normalizedStoredName = storedName.trim().toLowerCase();
  if (
    (agentId === "codex" && normalizedStoredName === "codex cli") ||
    (agentId === "claude" && normalizedStoredName === "claude code") ||
    (agentId === "cursor" && normalizedStoredName === "cursor") ||
    (agentId === "pi" && normalizedStoredName === "pi")
  ) {
    return defaultName;
  }

  return storedName;
}

function orderSidebarAgentButtons(
  buttons: readonly SidebarAgentButton[],
  storedOrder: readonly string[],
): SidebarAgentButton[] {
  const buttonById = new Map(buttons.map((button) => [button.agentId, button] as const));
  const orderedButtons: SidebarAgentButton[] = [];

  for (const agentId of normalizeStoredSidebarAgentOrder(storedOrder)) {
    const button = buttonById.get(agentId);
    if (button) {
      orderedButtons.push(button);
    }
  }

  for (const button of buttons) {
    if (!orderedButtons.some((candidate) => candidate.agentId === button.agentId)) {
      orderedButtons.push(button);
    }
  }

  return orderedButtons;
}
