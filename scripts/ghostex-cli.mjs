#!/usr/bin/env node
import { execFile, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import { appendFile, cp, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import http from "node:http";
import net from "node:net";
import { homedir, tmpdir } from "node:os";
import path from "node:path";
import { emitKeypressEvents } from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const DEFAULT_PORT = 58743;
// CDXC:GxserverBootstrap 2026-05-30-15:39: gxserver owns 58744 in the hard cutover, so the ghostex-dev native bridge must use 58742 for legacy bridge automation commands instead of competing with the daemon API.
const DEV_PORT = 58742;
const GXSERVER_PRODUCT = "gxserver";
const GXSERVER_PROTOCOL_VERSION = 1;
const GXSERVER_PROTOCOL_HEADER = "x-gxserver-protocol-version";
const GXSERVER_LOCAL_API_HOST = "127.0.0.1";
const GXSERVER_LOCAL_API_PORT = 58744;
const GXSERVER_SSH_FORWARD_DEFAULT_PORT = GXSERVER_LOCAL_API_PORT + 1;
const GXSERVER_SSH_FORWARD_PORT_SCAN_LIMIT = 25;
const GXSERVER_SSH_TUNNEL_READY_TIMEOUT_MS = 8_000;
const GXSERVER_SSH_COMMAND_TIMEOUT_MS = 12_000;
const GXSERVER_SSH_TUNNEL_IDLE_KILL_MS = 500;
const GXSERVER_RENAME_COMMAND_SUBMIT_DELAY_MS = 1_000;
const activeGxserverSshTunnels = new Map();
let gxserverSshTunnelExitHooksInstalled = false;
/**
 * CDXC:DevAppFlavor 2026-05-11-12:10
 * CLI-side logs, selector caches, and bridge metadata must follow the app
 * variant. Legacy ghostex-dev bundle invocations use ~/.ghostex-dev so CLI
 * commands issued through that old app do not touch the installed app's data.
 *
 * CDXC:LocalStartSingleApp 2026-06-09-09:27
 * Local dev start and build entry points were removed so agents stop launching
 * the alternate app by mistake; keep only the legacy variant read here for old
 * bundles that already exist.
 *
 * CDXC:CliBranding 2026-05-12-07:35
 * Public CLI commands are `ghostex` and the shorter `gx` alias. Internal
 * GHOSTEX_* environment names and ~/.ghostex storage remain unchanged because they
 * are implementation state, not user-facing command names.
 *
 * CDXC:CliBranding 2026-05-26-15:11
 * The short command is now `gx` instead of `gtx`; setup should expose the new
 * binary only when the user's Homebrew prefix does not already contain another
 * `gx`.
 */
const GHOSTEX_HOME =
  process.env.GHOSTEX_HOME?.trim() ||
  path.join(homedir(), process.env.GHOSTEX_APP_VARIANT === "dev" ? ".ghostex-dev" : ".ghostex");
const LOG_DIR = path.join(GHOSTEX_HOME, "logs");
const CLI_DIR = path.join(GHOSTEX_HOME, "cli");
const BRIDGE_TOKEN_PATH = path.join(CLI_DIR, "bridge-token");
const GXSERVER_ROOT = path.join(homedir(), ".ghostex", "gxserver");
const GXSERVER_AUTH_TOKEN_PATH = path.join(GXSERVER_ROOT, "auth", "token");
const GXSERVER_IDENTITY_PATH = path.join(GXSERVER_ROOT, "identity.json");
const GXSERVER_LOG_PATH = path.join(GXSERVER_ROOT, "logs", "gxserver.jsonl");
const GXSERVER_STATE_DB_PATH = path.join(GXSERVER_ROOT, "state.db");
const GXSERVER_CONNECTIONS_PATH = path.join(homedir(), ".ghostex", "clients", "connections.json");
const SESSION_ALIAS_CACHE_PATH = path.join(CLI_DIR, "session-aliases.json");
const SHARED_SETTINGS_PATH = path.join(GHOSTEX_HOME, "state", "native-sidebar-settings.json");
const GHOSTEX_AGENT_SKILL_INSTALL_ROOT = path.join(homedir(), "agents", "skills");
const GHOSTEX_BROWSER_SKILL_NAME = "ghostex-browser-use";
const GHOSTEX_BROWSER_LEGACY_SKILL_NAMES = ["ghostex-browser-devtools-mcp"];
const GHOSTEX_COMPUTER_USE_SKILL_NAME = "ghostex-computer-use";
const GHOSTEX_AGENT_ORCHESTRATION_SKILL_NAME = "ghostex-agent-orchestration";
const GHOSTEX_GENERATE_TITLE_SKILL_NAME = "ghostex-generate-title";
const GHOSTEX_MANAGE_BEADS_SKILL_NAME = "ghostex-manage-beads";
const GHOSTEX_GENERATE_TITLE_LEGACY_SKILL_NAMES = ["madda-generate-title"];
const QUICK_TERMINALS_PROJECT_NAME = "Quick Terminals";
const RESET_ANSI = "\x1b[0m";
const PICKER_TITLE = "Attach to Ghostex Session";
const PICKER_TITLE_STYLE = "\x1b[1m\x1b[38;2;255;255;255m";
const PROJECT_HEADER_STYLE = "\x1b[1m\x1b[38;2;130;183;255m";
const SELECTED_SESSION_STYLE = "\x1b[1m\x1b[38;2;255;255;255m";
const AGENT_PICKER_INDICATORS = new Map([
  ["amp", { color: "#ffffff", label: "AMP" }],
  ["amp-cli", { color: "#ffffff", label: "AMP" }],
  ["antigravity", { color: "#749bff", label: "AGY" }],
  ["antigravity-cli", { color: "#749bff", label: "AGY" }],
  ["claude", { color: "#d97757", label: "CLD" }],
  ["claude-code", { color: "#d97757", label: "CLD" }],
  ["codex", { color: "#a991ff", label: "CDX" }],
  ["codex-cli", { color: "#a991ff", label: "CDX" }],
  ["copilot", { color: "#ffffff", label: "PLT" }],
  ["cursor", { color: "#749bff", label: "CRS" }],
  ["cursor-cli", { color: "#749bff", label: "CRS" }],
  ["droid", { color: "#ff7a1a", label: "DRD" }],
  ["factory-droid", { color: "#ff7a1a", label: "DRD" }],
  ["gemini", { color: "#8b9aff", label: "GEM" }],
  ["grok", { color: "#ffffff", label: "GRK" }],
  ["grok-build", { color: "#ffffff", label: "GRK" }],
  ["opencode", { color: "#6d96c0", label: "OPN" }],
  ["open-code", { color: "#6d96c0", label: "OPN" }],
  ["pi", { color: "#c8ff62", label: "PIA" }],
  ["t3", { color: "#ff6af3", label: "T3C" }],
  ["t3-code", { color: "#ff6af3", label: "T3C" }],
  ["work-codex", { color: "#a991ff", label: "CDX" }],
]);
const DEFAULT_PICKER_AGENT_INDICATOR = { color: "#9ca3af", label: "UNK" };

const COMMANDS = new Map([
  ["sessions", sessionsCommand],
  ["s", sessionsCommand],
  ["list-sessions", sessionsCommand],
  ["ls", sessionsCommand],
  ["find", zehnSearchCommand],
  ["f", zehnSearchCommand],
  ["android-check", androidCheckCommand],
  ["attach", attachSessionCommand],
  ["a", attachSessionCommand],
  ["resume", attachSessionCommand],
  ["r", attachSessionCommand],
  ["kill", sessionActionCommand("closeSession", "killed")],
  ["k", sessionActionCommand("closeSession", "killed")],
  ["sleep", sessionActionCommand("sleepSession", "slept", { sleeping: true })],
  ["wake", sessionActionCommand("sleepSession", "woke", { sleeping: false })],
  ["focus", focusSmartSessionCommand],
  ["floating-editor", floatingEditorCommand],
  ["fe", floatingEditorCommand],
  ["floating-monaco-editor", floatingMonacoEditorCommand],
  ["fme", floatingMonacoEditorCommand],
  ["prompt-editor", promptEditorCommand],
  ["state", bridgeAction("state")],
  ["dump-state", bridgeAction("dumpState")],
  ["open", bridgeAction("openPaths", parseOpenPaths, { failOnNotOk: true })],
  ["o", bridgeAction("openPaths", parseOpenPaths, { failOnNotOk: true })],
  ["edit", bridgeAction("openPaths", parseEditPaths, { failOnNotOk: true })],
  ["e", bridgeAction("openPaths", parseEditPaths, { failOnNotOk: true })],
  ["terminal", bridgeAction("createQuickTerminal", parseQuickTerminal, { failOnNotOk: true })],
  ["t", bridgeAction("createQuickTerminal", parseQuickTerminal, { failOnNotOk: true })],
  ["create-session", bridgeAction("createSession", parseCreateSession, { failOnNotOk: true })],
  ["create-agent", bridgeAction("createAgentSession", parseAgent)],
  ["run-agent", bridgeAction("runAgent", parseAgent)],
  ["run-command", bridgeAction("runCommand", parseCommandButton)],
  ["click-button", bridgeAction("clickButton", parseClickButton)],
  ["save-agent", bridgeAction("saveAgent", parseSaveAgent, { failOnNotOk: true })],
  ["automation-state", bridgeAction("automationState", parseAutomationProject, { failOnNotOk: true })],
  ["automation-save", bridgeAction("automationSave", parseAutomationSave, { failOnNotOk: true })],
  ["automation-run-now", bridgeAction("automationRunNow", parseAutomationId, { failOnNotOk: true })],
  ["automation-set-enabled", bridgeAction("automationSetEnabled", parseAutomationEnabled, { failOnNotOk: true })],
  ["automation-archive-run", bridgeAction("automationArchiveRun", parseAutomationRun, { failOnNotOk: true })],
  ["automation-mark-run-read", bridgeAction("automationMarkRunRead", parseAutomationRun, { failOnNotOk: true })],
  ["focus-session", bridgeAction("focusSession", parseSessionSelector)],
  ["acknowledge-session-attention", bridgeAction("acknowledgeSessionAttention", parseSessionSelector)],
  ["ack-session-attention", bridgeAction("acknowledgeSessionAttention", parseSessionSelector)],
  ["focus-group", bridgeAction("focusGroup", parseGroup)],
  ["switch-project", bridgeAction("switchProject", parseProject)],
  ["move-project", bridgeAction("moveProject", parseProjectMove, { failOnNotOk: true })],
  ["add-project", bridgeAction("addProject", parseProjectPath)],
  ["remove-project", bridgeAction("removeProject", parseProject, { failOnNotOk: true })],
  ["close-session", bridgeAction("closeSession", parseSessionSelector)],
  ["restart-session", bridgeAction("restartSession", parseSessionSelector)],
  ["fork-session", forkSessionCommand],
  ["reload-session", bridgeAction("fullReloadSession", parseSessionSelector)],
  ["rename-session", bridgeAction("renameSession", parseRename, { failOnNotOk: true })],
  ["sleep-session", bridgeAction("sleepSession", parseSessionBoolean("sleeping"))],
  ["favorite-session", bridgeAction("favoriteSession", parseSessionBoolean("favorite"))],
  ["pin-session", bridgeAction("pinSession", parseSessionBoolean("pinned"))],
  ["send-text", resolvedSessionBridgeAction("sendText", parseSendText)],
  ["send-enter", resolvedSessionBridgeAction("sendEnter", parseSessionSelector)],
  ["send-key", resolvedSessionBridgeAction("sendKey", parseSendKey)],
  ["send-message", sendMessageCommand],
  ["message", sendMessageCommand],
  ["msg", sendMessageCommand],
  ["read-text", readSessionTextCommand],
  ["read-messages", readSessionTextCommand],
  ["read-thread", readSessionTextCommand],
  ["rename-command", resolvedSessionBridgeAction("renameCommand", parseRename)],
  ["set-visible-count", bridgeAction("setVisibleCount", parseVisibleCount)],
  ["set-view-mode", bridgeAction("setViewMode", parseViewMode)],
  ["open-browser", bridgeAction("openBrowser", parseUrl)],
  ["open-browser-pane", bridgeAction("openBrowserPane")],
  ["browser", browserCommand],
  ["browser-devtools-mcp", browserDevToolsMcpCommand],
  ["browser-mcp", browserDevToolsMcpCommand],
  ["bd", beadsCommand],
  ["beads", beadsCommand],
  ["server", serverCommand],
  ["install-browser-skill", installBrowserSkillCommand],
  ["install-browser-mcp-skill", installBrowserSkillCommand],
  ["computer-use", computerUseCommand],
  ["install-computer-use-skill", installComputerUseSkillCommand],
  ["agent-orchestration", agentOrchestrationCommand],
  ["install-agent-orchestration-skill", installAgentOrchestrationSkillCommand],
  ["generate-title", generateTitleCommand],
  ["install-generate-title-skill", installGenerateTitleSkillCommand],
  ["manage-beads", manageBeadsCommand],
  ["install-manage-beads-skill", installManageBeadsSkillCommand],
  ["toggle-sidebar", bridgeAction("toggleSidebarCollapsed")],
  ["move-sidebar", bridgeAction("moveSidebar")],
  ["assert-card", bridgeAction("assertSidebarCard", parseAssertCard, { assertOk: true })],
  ["wait-for", bridgeAction("waitFor", parseWaitFor, { assertOk: true })],
  ["screenshot", screenshotCommand],
  ["logs", logsCommand],
  ["bundle", bundleCommand],
  ["help", helpCommand],
  ["h", helpCommand],
]);

if (isDirectCliEntryPoint()) {
  main().catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    if (cliArgsWantJson(process.argv.slice(2))) {
      printJson({ error: message, ok: false });
    } else {
      console.error(message);
    }
    process.exitCode = 1;
  });
}

function cliArgsWantJson(args) {
  return args.some((arg) => arg === "--json" || arg === "-json" || arg.startsWith("--json="));
}

function isDirectCliEntryPoint() {
  const entryPath = process.argv[1];
  /**
   * CDXC:CliEntrypoint 2026-05-18-01:17:
   * Android reaches the Mac CLI through the installed `ghostex` wrapper, which
   * can execute a Homebrew symlink to this repo's `ghostex-cli.mjs`. Resolve
   * both paths before deciding whether to run `main()`, otherwise the CLI exits
   * zero with no JSON and Android reports "Ghostex CLI did not return JSON."
   */
  if (!entryPath) return false;
  try {
    return realpathSync(entryPath) === realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return import.meta.url === pathToFileURL(entryPath).href;
  }
}

async function main() {
  const argv = process.argv.slice(2);
  /**
   * CDXC:GhostexTui 2026-05-24-19:18:
   * Running bare `ghostex` or `gx` should open the full terminal TUI. Keep
   * `gx a <session>` and the attach aliases outside this path so direct
   * single-session attaches remain fast and script-compatible.
   */
  if (argv.length === 0) {
    await ghostexTuiCommand([]);
    return;
  }
  const [commandName, ...args] = argv;
  if (commandName === "-h" || commandName === "--help") {
    helpCommand();
    return;
  }
  const command = COMMANDS.get(commandName);
  if (!command) {
    if (isExistingBarePathArgument(commandName)) {
      /**
       * CDXC:OSIntegration 2026-05-27-18:06:
       * Ghostex should behave like VS Code for CLI path opens: `ghostex ./file`
       * and `ghostex ./folder` open existing filesystem targets, while a bare
       * unknown word that is not a path remains an explicit CLI typo.
       */
      await bridgeAction("openPaths", parseOpenPaths, { failOnNotOk: true })(argv);
      return;
    }
    throw new Error(`Unknown command: ${commandName}\n\n${usage()}`);
  }
  if (
    !["agent-orchestration", "bd", "beads", "browser", "computer-use", "f", "find", "generate-title", "manage-beads", "server"].includes(commandName) &&
    (args.includes("-h") || args.includes("--help"))
  ) {
    helpCommand();
    return;
  }
  await command(args);
}

function bridgeAction(action, parser = () => ({}), options = {}) {
  return async (args) => {
    const { flags, rest } = parseArgs(args);
    const payload = parser(rest, flags);
    const bridgeFlags =
      payload && typeof payload === "object" && payload.wait === true && flags.timeout === undefined
        ? { ...flags, timeout: 0 }
        : flags;
    const result = await sendSidebarCliCommand(action, payload, bridgeFlags);
    /**
     * CDXC:AndroidRemoteSessions 2026-05-17-14:24:
     * Android remote actions use SSH exit status to decide whether to show
     * recovery UI. Android-facing bridge commands such as rename-session must
     * convert `{ ok: false }` bridge replies into a nonzero CLI exit.
     */
    if ((options.assertOk || options.failOnNotOk) && isFailedCliResult(result)) {
      printJson(result);
      process.exitCode = 1;
      return;
    }
    printJson(result);
  };
}

function resolvedSessionBridgeAction(action, parser = () => ({}), options = {}) {
  return async (args) => {
    const { flags, rest } = parseArgs(args);
    const payload = parser(rest, flags);
    const selector = sessionSelectorFromArgs(rest, flags);
    const resolvedSession = selector ? await resolveCliSessionSelector(selector, flags) : undefined;
    /**
     * CDXC:CliSessionSelectors 2026-06-04-03:20:
     * gxserver session ids are project-scoped. Selector-backed bridge actions
     * must carry the resolved projectId with the sessionId so remote and mobile
     * clients reconnect through the same S/P/G zmx route instead of addressing
     * a bare G id.
     */
    const resolvedPayload = resolvedSession
      ? { ...payload, projectId: payload.projectId ?? resolvedSession.projectId, sessionId: resolvedSession.sessionId }
      : payload;
    const result = await sendSidebarCliCommand(action, resolvedPayload, flags);
    if ((options.assertOk || options.failOnNotOk) && isFailedCliResult(result)) {
      printJson(result);
      process.exitCode = 1;
      return;
    }
    printJson(result);
  };
}

async function browserDevToolsMcpCommand(args) {
  await new Promise((resolve) => setImmediate(resolve));
  const { flags } = parseArgs(args);
  await runBrowserDevToolsMcpServer({
    port: normalizePositiveInteger(flags.port ?? process.env.GHOSTEX_CEF_REMOTE_DEBUGGING_PORT),
    target: stringFlag(flags.target ?? flags.page ?? flags.pageId),
    timeoutMs: normalizePositiveInteger(flags.timeout ?? process.env.GHOSTEX_BROWSER_MCP_TIMEOUT_MS) ?? 10_000,
  });
}

async function browserCommand(args) {
  const [subcommand = "help", ...rest] = args;
  /**
   * CDXC:BrowserAgentControl 2026-05-27-01:59:
   * Agents should discover embedded CEF control through `gx browser --help`.
   * Keep browser MCP, skill install, pane opening, and browser visibility under
   * the `browser` namespace so "browser" is the durable keyword for this control
   * surface instead of a scattered set of top-level command names.
   */
  if (rest.includes("-h") || rest.includes("--help")) {
    console.log(browserUsage());
    return;
  }
  switch (subcommand) {
    case "help":
    case "-h":
    case "--help":
      console.log(browserUsage());
      return;
    case "mcp":
    case "devtools-mcp":
    case "browser-devtools-mcp":
      await browserDevToolsMcpCommand(rest);
      return;
    case "install-skill":
    case "install-browser-skill":
    case "install-mcp-skill":
      await installBrowserSkillCommand(rest);
      return;
    case "open":
      await bridgeAction("openBrowserPane", parseBrowserOpen)(rest);
      return;
    case "open-pane":
    case "pane":
      await bridgeAction("openBrowserPane", parseBrowserOpen)(rest);
      return;
    default:
      throw new Error(`Unknown browser command: ${subcommand}\n\n${browserUsage()}`);
  }
}

async function serverCommand(args) {
  const [subcommand = ""] = args;
  /*
   * CDXC:GxserverCli 2026-06-02-18:36:
   * Users should operate the background daemon through the public `gx`/`ghostex`
   * CLI instead of learning a second top-level command. Keep `gx server ...` as
   * a thin launcher over the existing gxserver CLI so daemon lifecycle behavior,
   * Node checks, protocol reuse, and control-plane stop semantics remain owned
   * by gxserver.
   */
  if (subcommand === "help" || subcommand === "-h" || subcommand === "--help") {
    console.log(serverUsage());
    return;
  }
  await runGxserverCliCommand(args);
}

async function runGxserverCliCommand(args) {
  const launch = resolveGxserverCliLaunch();
  await runInteractiveProcess(launch.command, [...launch.args, ...args], {
    cwd: launch.cwd,
    env: launch.env,
  });
}

async function installBrowserSkillCommand(args) {
  /**
   * CDXC:BrowserAgentControl 2026-05-27-06:58:
   * Agents should invoke Ghostex embedded browser control as
   * `$ghostex-browser-use`, not the implementation-shaped
   * `$ghostex-browser-devtools-mcp`. Installing the renamed skill also removes
   * the legacy shared install so Codex does not discover both names.
   */
  await installGhostexAgentSkill({
    args,
    command: "ghostex browser mcp",
    envVars: ["GHOSTEX_BROWSER_USE_SKILL_SOURCE", "GHOSTEX_BROWSER_SKILL_SOURCE"],
    legacySkillNames: GHOSTEX_BROWSER_LEGACY_SKILL_NAMES,
    skillName: GHOSTEX_BROWSER_SKILL_NAME,
  });
}

async function computerUseCommand(args) {
  const [subcommand = "help", ...rest] = args;
  if (rest.includes("-h") || rest.includes("--help")) {
    console.log(computerUseUsage());
    return;
  }
  switch (subcommand) {
    case "help":
    case "-h":
    case "--help":
      console.log(computerUseUsage());
      return;
    case "install-skill":
      await installComputerUseSkillCommand(rest);
      return;
    default:
      throw new Error(`Unknown computer-use command: ${subcommand}\n\n${computerUseUsage()}`);
  }
}

async function installComputerUseSkillCommand(args) {
  /**
   * CDXC:ComputerAgentControl 2026-05-27-06:58:
   * Desktop Control setup must install an agent-facing `$ghostex-computer-use`
   * skill in addition to Cua Driver so users can request computer use through a
   * Ghostex-named wrapper instead of remembering the lower-level `$cua-driver`
   * skill name.
   */
  await installGhostexAgentSkill({
    args,
    command: "cua-driver",
    envVars: ["GHOSTEX_COMPUTER_USE_SKILL_SOURCE"],
    skillName: GHOSTEX_COMPUTER_USE_SKILL_NAME,
  });
}

async function agentOrchestrationCommand(args) {
  const [subcommand = "help", ...rest] = args;
  if (rest.includes("-h") || rest.includes("--help")) {
    console.log(agentOrchestrationUsage());
    return;
  }
  switch (subcommand) {
    case "help":
    case "-h":
    case "--help":
      console.log(agentOrchestrationUsage());
      return;
    case "install-skill":
      await installAgentOrchestrationSkillCommand(rest);
      return;
    default:
      throw new Error(`Unknown agent-orchestration command: ${subcommand}\n\n${agentOrchestrationUsage()}`);
  }
}

async function installAgentOrchestrationSkillCommand(args) {
  /**
   * CDXC:AgentOrchestration 2026-05-27-07:15:
   * Agents need a Ghostex-native orchestration skill that teaches the CLI
   * workflow for creating panes, sending messages to other agent sessions,
   * checking status, and reading terminal output through `ghostex read-text`
   * instead of reaching for raw zmx commands directly.
   */
  await installGhostexAgentSkill({
    args,
    command: "ghostex --help",
    envVars: ["GHOSTEX_AGENT_ORCHESTRATION_SKILL_SOURCE"],
    skillName: GHOSTEX_AGENT_ORCHESTRATION_SKILL_NAME,
  });
}

async function generateTitleCommand(args) {
  const [subcommand = "help", ...rest] = args;
  if (rest.includes("-h") || rest.includes("--help")) {
    console.log(generateTitleUsage());
    return;
  }
  switch (subcommand) {
    case "help":
    case "-h":
    case "--help":
      console.log(generateTitleUsage());
      return;
    case "install-skill":
      await installGenerateTitleSkillCommand(rest);
      return;
    default:
      throw new Error(`Unknown generate-title command: ${subcommand}\n\n${generateTitleUsage()}`);
  }
}

async function installGenerateTitleSkillCommand(args) {
  /**
   * CDXC:GenerateTitleSkill 2026-05-27-07:28:
   * The old `$madda-generate-title` behavior is now a Ghostex skill. Agents must
   * produce titles shorter than 47 characters, then submit `/rename <title>` in
   * their own Ghostex session.
   *
   * CDXC:GenerateTitleSkill 2026-06-09-17:49:
   * Generated title sessions should not leave `/rename <title>` staged in the
   * agent CLI. Use `rename-command` so Ghostex stages the command and sends
   * Enter through the supported session input path.
   *
   * CDXC:GenerateTitleSkill 2026-06-12-04:10:
   * zmx-backed terminals can expose the exact gxserver global ref or provider
   * session name without GHOSTEX_SESSION_ID. The installed skill must prefer
   * stable self-session selectors and never fall back to title/alias guessing.
   *
   * CDXC:GenerateTitleSkill 2026-06-13-01:55:
   * `rename-command` is implemented by the gxserver CLI dispatcher after the
   * bridge cutover, so installed skill wording should describe the command
   * contract instead of promising a macOS-only native bridge route.
   */
  await installGhostexAgentSkill({
    args,
    command: "ghostex rename-command --session-id \"${GHOSTEX_GLOBAL_SESSION_REF:-${GHOSTEX_SESSION_ID:-${ZMX_SESSION:-}}}\" --title \"<title>\"",
    envVars: ["GHOSTEX_GENERATE_TITLE_SKILL_SOURCE"],
    legacySkillNames: GHOSTEX_GENERATE_TITLE_LEGACY_SKILL_NAMES,
    skillName: GHOSTEX_GENERATE_TITLE_SKILL_NAME,
  });
}

async function manageBeadsCommand(args) {
  const [subcommand = "help", ...rest] = args;
  if (rest.includes("-h") || rest.includes("--help")) {
    console.log(manageBeadsUsage());
    return;
  }
  switch (subcommand) {
    case "help":
    case "-h":
    case "--help":
      console.log(manageBeadsUsage());
      return;
    case "install-skill":
      await installManageBeadsSkillCommand(rest);
      return;
    default:
      throw new Error(`Unknown manage-beads command: ${subcommand}\n\n${manageBeadsUsage()}`);
  }
}

async function installManageBeadsSkillCommand(args) {
  /**
   * CDXC:ProjectBoardBeads 2026-06-04-03:32:
   * Agents need a bundled `$ghostex-manage-beads` skill that teaches the `bd`
   * project-board workflow and the session-association pattern for review
   * beads. Installing it through the Ghostex CLI keeps the skill version tied
   * to the app bundle instead of relying on a local checkout.
   *
   * CDXC:ProjectBoardBeads 2026-06-10-09:31:
   * Agent-facing bead workflows must go through `gx bd` so installed agents use
   * Ghostex's pinned bundled Beads binary instead of whichever shell `bd`
   * happens to be first on PATH.
   */
  await installGhostexAgentSkill({
    args,
    command: "gx bd --help",
    envVars: ["GHOSTEX_MANAGE_BEADS_SKILL_SOURCE"],
    skillName: GHOSTEX_MANAGE_BEADS_SKILL_NAME,
  });
}

async function beadsCommand(args) {
  const launch = resolveBundledBeadsLaunch();
  await runInteractiveProcess(launch.command, [...launch.args, ...args], {
    cwd: process.cwd(),
    env: launch.env,
  });
}

async function installGhostexAgentSkill({ args, command, envVars, legacySkillNames = [], skillName }) {
  const { flags } = parseArgs(args);
  const sourceDir = resolveGhostexAgentSkillSourceDir(skillName, envVars);
  const defaultTargetDir = ghostexAgentSkillInstallDir(skillName);
  const targetDir = path.resolve(stringFlag(flags.targetDir ?? flags.target) ?? defaultTargetDir);
  const installsDefaultTarget = targetDir === defaultTargetDir;
  /**
   * CDXC:BrowserAgentControl 2026-05-26-22:17:
   * First-launch CLI setup should install the agent skill, not only the
   * `ghostex` executable. The CLI owns this copy step because Homebrew installs
   * the bundled app resources and agents discover user skills under
   * ~/agents/skills.
   */
  await mkdir(path.dirname(targetDir), { recursive: true });
  await cp(sourceDir, targetDir, { force: true, recursive: true });

  const removedLegacySkillDirs = [];
  if (installsDefaultTarget) {
    for (const legacySkillName of legacySkillNames) {
      const legacyDir = ghostexAgentSkillInstallDir(legacySkillName);
      await rm(legacyDir, { force: true, recursive: true });
      removedLegacySkillDirs.push(legacyDir);
    }
  }

  const result = {
    command,
    ok: true,
    removedLegacySkillDirs,
    skill: skillName,
    sourceDir,
    targetDir,
  };
  if (flags.json) {
    printJson(result);
    return;
  }
  console.log(`Installed ${skillName} to ${targetDir}`);
  if (removedLegacySkillDirs.length > 0) {
    console.log(`Removed legacy skill installs: ${removedLegacySkillDirs.join(", ")}`);
  }
  console.log(`Configure agents to run: ${command}`);
}

/**
 * CDXC:BrowserAgentControl 2026-05-26-15:40:
 * Agents need Chrome-DevTools-style control over Ghostex's embedded CEF panes, including console visibility. CEF already exposes a loopback CDP endpoint, so the CLI hosts a small MCP-compatible stdio server that talks directly to CDP instead of adding a parallel browser automation stack or WebKit-style JavaScript fallback path.
 */
async function runBrowserDevToolsMcpServer(options = {}) {
  const state = {
    captures: new Map(),
    clients: new Map(),
    options,
    refMaps: new Map(),
    selectedPageId: options.target ?? null,
  };
  const transport = new McpStdioTransport(async (message) => {
    const response = await handleBrowserMcpMessage(message, state);
    if (response) {
      transport.send(response);
    }
  });
  transport.start();
}

async function handleBrowserMcpMessage(message, state) {
  if (!message || typeof message !== "object") return null;
  const { id, method, params } = message;
  if (method === "notifications/initialized" || id === undefined || id === null) {
    return null;
  }
  try {
    if (method === "initialize") {
      return {
        id,
        jsonrpc: "2.0",
        result: {
          capabilities: { tools: {} },
          protocolVersion: params?.protocolVersion ?? "2024-11-05",
          serverInfo: { name: "ghostex-browser-devtools", version: "1.0.0" },
        },
      };
    }
    if (method === "tools/list") {
      return { id, jsonrpc: "2.0", result: { tools: browserMcpTools() } };
    }
    if (method === "tools/call") {
      const result = await callBrowserMcpTool(params?.name, params?.arguments ?? {}, state);
      return { id, jsonrpc: "2.0", result };
    }
    return {
      error: { code: -32601, message: `Unknown MCP method: ${method}` },
      id,
      jsonrpc: "2.0",
    };
  } catch (error) {
    return {
      error: { code: -32000, message: error instanceof Error ? error.message : String(error) },
      id,
      jsonrpc: "2.0",
    };
  }
}

function browserMcpTools() {
  const pageSelectorProperties = {
    pageId: { description: "CDP target id. Defaults to the selected page, then the first Ghostex CEF page.", type: "string" },
    titleContains: { description: "Select a page whose title contains this text.", type: "string" },
    urlContains: { description: "Select a page whose URL contains this text.", type: "string" },
  };
  return [
    {
      name: "ghostex_list_pages",
      description: "List embedded Ghostex CEF pages available over the local DevTools endpoint.",
      inputSchema: { type: "object", properties: {} },
    },
    {
      name: "ghostex_select_page",
      description: "Select the embedded page used by subsequent Ghostex browser tools.",
      inputSchema: { type: "object", properties: { ...pageSelectorProperties, index: { type: "number" } } },
    },
    {
      name: "ghostex_navigate",
      description: "Navigate a Ghostex embedded browser page.",
      inputSchema: { type: "object", required: ["url"], properties: { ...pageSelectorProperties, url: { type: "string" } } },
    },
    {
      name: "ghostex_evaluate",
      description: "Evaluate JavaScript in the selected embedded browser page.",
      inputSchema: {
        type: "object",
        required: ["script"],
        properties: { ...pageSelectorProperties, awaitPromise: { type: "boolean" }, script: { type: "string" } },
      },
    },
    {
      name: "ghostex_console_logs",
      description: "Read captured console, exception, and browser log entries for the selected embedded page.",
      inputSchema: {
        type: "object",
        properties: { ...pageSelectorProperties, clear: { type: "boolean" }, limit: { type: "number" } },
      },
    },
    {
      name: "ghostex_snapshot",
      description: "Return an agent-friendly snapshot of visible interactive elements and assign @e refs.",
      inputSchema: { type: "object", properties: { ...pageSelectorProperties, limit: { type: "number" } } },
    },
    {
      name: "ghostex_click",
      description: "Click an element by @e ref from ghostex_snapshot or a CSS selector.",
      inputSchema: {
        type: "object",
        properties: { ...pageSelectorProperties, ref: { type: "string" }, selector: { type: "string" } },
      },
    },
    {
      name: "ghostex_fill",
      description: "Fill an input, textarea, select, or contenteditable element by @e ref or CSS selector.",
      inputSchema: {
        type: "object",
        required: ["text"],
        properties: { ...pageSelectorProperties, ref: { type: "string" }, selector: { type: "string" }, text: { type: "string" } },
      },
    },
    {
      name: "ghostex_press_key",
      description: "Send a keyboard key to the selected embedded browser page.",
      inputSchema: { type: "object", required: ["key"], properties: { ...pageSelectorProperties, key: { type: "string" } } },
    },
    {
      name: "ghostex_screenshot",
      description: "Capture the selected embedded browser viewport as a PNG image.",
      inputSchema: { type: "object", properties: { ...pageSelectorProperties } },
    },
  ];
}

async function callBrowserMcpTool(name, args, state) {
  switch (name) {
    case "ghostex_list_pages":
      return textToolResult(await browserMcpListPages(state));
    case "ghostex_select_page":
      return textToolResult(await browserMcpSelectPage(args, state));
    case "ghostex_navigate":
      return textToolResult(await browserMcpNavigate(args, state));
    case "ghostex_evaluate":
      return textToolResult(await browserMcpEvaluate(args, state));
    case "ghostex_console_logs":
      return textToolResult(await browserMcpConsoleLogs(args, state));
    case "ghostex_snapshot":
      return textToolResult(await browserMcpSnapshot(args, state));
    case "ghostex_click":
      return textToolResult(await browserMcpClick(args, state));
    case "ghostex_fill":
      return textToolResult(await browserMcpFill(args, state));
    case "ghostex_press_key":
      return textToolResult(await browserMcpPressKey(args, state));
    case "ghostex_screenshot":
      return imageToolResult(await browserMcpScreenshot(args, state));
    default:
      throw new Error(`Unknown Ghostex browser MCP tool: ${name}`);
  }
}

function textToolResult(value) {
  return { content: [{ type: "text", text: JSON.stringify(value, null, 2) }] };
}

function imageToolResult(value) {
  return {
    content: [
      { type: "text", text: JSON.stringify({ page: value.page, size: value.size }, null, 2) },
      { type: "image", data: value.data, mimeType: "image/png" },
    ],
  };
}

async function browserMcpListPages(state) {
  const discovery = await discoverGhostexCdpPages(state.options);
  return {
    port: discovery.port,
    selectedPageId: state.selectedPageId,
    pages: discovery.pages.map((page, index) => ({
      index,
      id: page.id,
      title: page.title ?? "",
      type: page.type ?? "",
      url: page.url ?? "",
      selected: page.id === state.selectedPageId,
    })),
  };
}

async function browserMcpSelectPage(args, state) {
  const { page } = await resolveGhostexCdpPage(args, state);
  state.selectedPageId = page.id;
  return { selected: { id: page.id, title: page.title ?? "", url: page.url ?? "" } };
}

async function browserMcpNavigate(args, state) {
  const url = stringFlag(args.url);
  if (!url) throw new Error("ghostex_navigate requires url");
  const { client, page } = await getGhostexCdpClient(args, state);
  await client.call("Page.enable");
  const result = await client.call("Page.navigate", { url: normalizeBrowserNavigationUrl(url) });
  return { frameId: result.frameId, page: cdpPageSummary(page) };
}

async function browserMcpEvaluate(args, state) {
  const script = stringFlag(args.script);
  if (!script) throw new Error("ghostex_evaluate requires script");
  const { client, page } = await getGhostexCdpClient(args, state);
  const result = await client.call("Runtime.evaluate", {
    awaitPromise: args.awaitPromise !== false,
    expression: script,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    return { exception: result.exceptionDetails, ok: false, page: cdpPageSummary(page) };
  }
  return {
    ok: true,
    page: cdpPageSummary(page),
    result: normalizeRemoteObject(result.result),
  };
}

async function browserMcpConsoleLogs(args, state) {
  const { client, page } = await getGhostexCdpClient(args, state);
  await client.ensureCaptureEnabled();
  const key = page.id;
  const entries = state.captures.get(key) ?? [];
  const limit = normalizePositiveInteger(args.limit) ?? 200;
  const selected = entries.slice(Math.max(0, entries.length - limit));
  if (args.clear === true) {
    state.captures.set(key, []);
  }
  return {
    entries: selected,
    page: cdpPageSummary(page),
    total: entries.length,
  };
}

async function browserMcpSnapshot(args, state) {
  const { client, page } = await getGhostexCdpClient(args, state);
  const limit = normalizePositiveInteger(args.limit) ?? 120;
  const snapshot = await evaluateFunction(client, ghostexSnapshotScript, [limit]);
  const refMap = new Map();
  for (const element of snapshot.elements ?? []) {
    if (element.ref && element.selector) {
      refMap.set(element.ref, element.selector);
    }
  }
  state.refMaps.set(page.id, refMap);
  return {
    page: cdpPageSummary(page),
    snapshot,
  };
}

async function browserMcpClick(args, state) {
  const { client, page } = await getGhostexCdpClient(args, state);
  const selector = resolveBrowserElementSelector(args, state, page.id);
  const result = await evaluateFunction(client, ghostexClickScript, [selector]);
  return { clicked: result, page: cdpPageSummary(page) };
}

async function browserMcpFill(args, state) {
  const text = stringFlag(args.text);
  if (text == null) throw new Error("ghostex_fill requires text");
  const { client, page } = await getGhostexCdpClient(args, state);
  const selector = resolveBrowserElementSelector(args, state, page.id);
  const result = await evaluateFunction(client, ghostexFillScript, [selector, text]);
  return { filled: result, page: cdpPageSummary(page) };
}

async function browserMcpPressKey(args, state) {
  const key = stringFlag(args.key);
  if (!key) throw new Error("ghostex_press_key requires key");
  const { client, page } = await getGhostexCdpClient(args, state);
  const event = keyEventForBrowserMcp(key);
  await client.call("Input.dispatchKeyEvent", { ...event, type: "keyDown" });
  await client.call("Input.dispatchKeyEvent", { ...event, type: "keyUp" });
  return { key, page: cdpPageSummary(page), pressed: true };
}

async function browserMcpScreenshot(args, state) {
  const { client, page } = await getGhostexCdpClient(args, state);
  await client.call("Page.enable");
  const result = await client.call("Page.captureScreenshot", { format: "png", fromSurface: true });
  return {
    data: result.data,
    page: cdpPageSummary(page),
    size: { encoding: "base64", mimeType: "image/png" },
  };
}

async function getGhostexCdpClient(args, state) {
  const { page } = await resolveGhostexCdpPage(args, state);
  state.selectedPageId = page.id;
  let client = state.clients.get(page.id);
  if (!client || client.isClosed) {
    client = await GhostexCdpClient.connect(page, {
      onEvent: (event) => recordGhostexCdpEvent(page.id, event, state),
      timeoutMs: state.options.timeoutMs,
    });
    state.clients.set(page.id, client);
  }
  return { client, page };
}

async function resolveGhostexCdpPage(args, state) {
  const discovery = await discoverGhostexCdpPages(state.options);
  if (discovery.pages.length === 0) {
    throw new Error(`No Ghostex CEF pages found on 127.0.0.1:${discovery.port}`);
  }
  const pageId = stringFlag(args.pageId ?? args.page ?? args.target ?? state.selectedPageId);
  let page = pageId ? discovery.pages.find((candidate) => candidate.id === pageId) : null;
  if (!page && typeof args.index === "number") {
    page = discovery.pages[args.index] ?? null;
  }
  const titleContains = stringFlag(args.titleContains);
  const urlContains = stringFlag(args.urlContains);
  if (!page && titleContains) {
    page = discovery.pages.find((candidate) => String(candidate.title ?? "").includes(titleContains)) ?? null;
  }
  if (!page && urlContains) {
    page = discovery.pages.find((candidate) => String(candidate.url ?? "").includes(urlContains)) ?? null;
  }
  if (!page) {
    page = discovery.pages[0];
  }
  if (!page?.webSocketDebuggerUrl) {
    throw new Error(`Ghostex CEF page ${page?.id ?? "unknown"} does not expose a DevTools WebSocket URL`);
  }
  return { page, port: discovery.port };
}

async function discoverGhostexCdpPages(options = {}) {
  const explicitPort = normalizePositiveInteger(options.port);
  const ports = explicitPort
    ? [explicitPort]
    : uniqueNumbers([
        normalizePositiveInteger(process.env.GHOSTEX_CEF_REMOTE_DEBUGGING_PORT),
        9333,
        9334,
        9335,
        9336,
        9337,
        9338,
        9339,
        9340,
        9341,
        9342,
        9343,
      ]);
  let lastError = null;
  for (const port of ports) {
    try {
      const targets = await httpJson(`http://127.0.0.1:${port}/json`, 450);
      const pages = Array.isArray(targets)
        ? targets.filter((target) => target?.type === "page" && !String(target.url ?? "").startsWith("devtools://"))
        : [];
      return { pages, port };
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(
    `Could not reach Ghostex CEF DevTools on ports ${ports.join(", ")}${lastError ? `: ${lastError.message}` : ""}`,
  );
}

function recordGhostexCdpEvent(pageId, event, state) {
  if (!event?.method) return;
  const entries = state.captures.get(pageId) ?? [];
  const pushEntry = (entry) => {
    entries.push({ timestamp: new Date().toISOString(), ...entry });
    if (entries.length > 1000) {
      entries.splice(0, entries.length - 1000);
    }
    state.captures.set(pageId, entries);
  };
  if (event.method === "Runtime.consoleAPICalled") {
    pushEntry({
      args: (event.params?.args ?? []).map(normalizeRemoteObject),
      level: event.params?.type ?? "log",
      source: "console",
      stackTrace: event.params?.stackTrace ?? null,
      text: (event.params?.args ?? []).map(remoteObjectText).join(" "),
    });
  } else if (event.method === "Runtime.exceptionThrown") {
    pushEntry({
      exception: event.params?.exceptionDetails ?? null,
      level: "error",
      source: "exception",
      text: event.params?.exceptionDetails?.text ?? "JavaScript exception",
    });
  } else if (event.method === "Log.entryAdded") {
    pushEntry({
      level: event.params?.entry?.level ?? "info",
      source: event.params?.entry?.source ?? "browser",
      text: event.params?.entry?.text ?? "",
      url: event.params?.entry?.url ?? null,
    });
  }
}

async function evaluateFunction(client, fn, args) {
  const expression = `(${fn.toString()})(...${JSON.stringify(args)})`;
  const result = await client.call("Runtime.evaluate", {
    awaitPromise: true,
    expression,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text ?? "Browser evaluation failed");
  }
  return result.result?.value;
}

function ghostexSnapshotScript(limit) {
  const selectors = [
    "a[href]",
    "button",
    "input",
    "textarea",
    "select",
    "[role='button']",
    "[role='link']",
    "[role='textbox']",
    "[contenteditable='true']",
    "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  const cssPath = (element) => {
    if (!element || element.nodeType !== 1) return "";
    const parts = [];
    let cursor = element;
    while (cursor && cursor.nodeType === 1 && cursor !== document.documentElement) {
      let part = cursor.nodeName.toLowerCase();
      if (cursor.id) {
        part += `#${CSS.escape(cursor.id)}`;
        parts.unshift(part);
        break;
      }
      const parent = cursor.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter((child) => child.nodeName === cursor.nodeName);
        if (siblings.length > 1) {
          part += `:nth-of-type(${siblings.indexOf(cursor) + 1})`;
        }
      }
      parts.unshift(part);
      cursor = parent;
    }
    return parts.join(" > ");
  };
  const labelFor = (element) => {
    const aria = element.getAttribute("aria-label");
    if (aria) return aria.trim();
    if (element.id) {
      const label = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      if (label?.innerText) return label.innerText.trim();
    }
    return (element.innerText || element.value || element.placeholder || element.title || "").trim().replace(/\s+/g, " ");
  };
  const elements = [];
  for (const element of Array.from(document.querySelectorAll(selectors))) {
    const rect = element.getBoundingClientRect();
    const style = window.getComputedStyle(element);
    if (rect.width <= 0 || rect.height <= 0 || style.visibility === "hidden" || style.display === "none") {
      continue;
    }
    elements.push({
      bounds: { height: Math.round(rect.height), width: Math.round(rect.width), x: Math.round(rect.x), y: Math.round(rect.y) },
      disabled: Boolean(element.disabled || element.getAttribute("aria-disabled") === "true"),
      label: labelFor(element).slice(0, 240),
      placeholder: element.getAttribute("placeholder") || "",
      ref: `@e${elements.length + 1}`,
      role: element.getAttribute("role") || element.nodeName.toLowerCase(),
      selector: cssPath(element),
      tag: element.nodeName.toLowerCase(),
      type: element.getAttribute("type") || "",
      value: "value" in element ? String(element.value ?? "").slice(0, 240) : "",
    });
    if (elements.length >= limit) break;
  }
  return { elements, title: document.title, url: location.href };
}

function ghostexClickScript(selector) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`Element not found: ${selector}`);
  element.scrollIntoView({ block: "center", inline: "center" });
  element.focus?.();
  element.click();
  const rect = element.getBoundingClientRect();
  return { bounds: { height: rect.height, width: rect.width, x: rect.x, y: rect.y }, selector };
}

function ghostexFillScript(selector, text) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`Element not found: ${selector}`);
  element.scrollIntoView({ block: "center", inline: "center" });
  element.focus?.();
  if (element.isContentEditable) {
    element.textContent = text;
  } else if (element.tagName === "SELECT") {
    element.value = text;
  } else if ("value" in element) {
    element.value = text;
  } else {
    throw new Error(`Element cannot be filled: ${selector}`);
  }
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
  return { selector, value: text };
}

function resolveBrowserElementSelector(args, state, pageId) {
  const selector = stringFlag(args.selector);
  if (selector) return selector;
  const ref = stringFlag(args.ref ?? args.element);
  if (!ref) throw new Error("Expected selector or ref");
  const mapped = state.refMaps.get(pageId)?.get(ref);
  if (!mapped) {
    throw new Error(`Unknown element ref ${ref}. Run ghostex_snapshot again for fresh refs.`);
  }
  return mapped;
}

function normalizeRemoteObject(value) {
  if (!value) return null;
  if (Object.hasOwn(value, "value")) return value.value;
  if (value.unserializableValue) return value.unserializableValue;
  return value.description ?? value.type ?? null;
}

function remoteObjectText(value) {
  const normalized = normalizeRemoteObject(value);
  return typeof normalized === "string" ? normalized : JSON.stringify(normalized);
}

function keyEventForBrowserMcp(key) {
  const special = {
    ArrowDown: { code: "ArrowDown", key: "ArrowDown", windowsVirtualKeyCode: 40 },
    ArrowLeft: { code: "ArrowLeft", key: "ArrowLeft", windowsVirtualKeyCode: 37 },
    ArrowRight: { code: "ArrowRight", key: "ArrowRight", windowsVirtualKeyCode: 39 },
    ArrowUp: { code: "ArrowUp", key: "ArrowUp", windowsVirtualKeyCode: 38 },
    Backspace: { code: "Backspace", key: "Backspace", windowsVirtualKeyCode: 8 },
    Delete: { code: "Delete", key: "Delete", windowsVirtualKeyCode: 46 },
    Enter: { code: "Enter", key: "Enter", windowsVirtualKeyCode: 13 },
    Escape: { code: "Escape", key: "Escape", windowsVirtualKeyCode: 27 },
    Tab: { code: "Tab", key: "Tab", windowsVirtualKeyCode: 9 },
  };
  if (special[key]) return special[key];
  const text = key.length === 1 ? key : "";
  return { code: text ? `Key${key.toUpperCase()}` : key, key, text, windowsVirtualKeyCode: text ? key.toUpperCase().charCodeAt(0) : 0 };
}

function cdpPageSummary(page) {
  return { id: page.id, title: page.title ?? "", url: page.url ?? "" };
}

function normalizeBrowserNavigationUrl(value) {
  if (/^[a-z][a-z0-9+.-]*:/i.test(value)) return value;
  return `https://${value}`;
}

function stringFlag(value) {
  if (typeof value !== "string") return value == null ? null : String(value);
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function normalizePositiveInteger(value) {
  const number = Number(value);
  return Number.isInteger(number) && number > 0 ? number : null;
}

function uniqueNumbers(values) {
  return [...new Set(values.filter((value) => Number.isInteger(value) && value > 0))];
}

function httpJson(url, timeoutMs) {
  return new Promise((resolve, reject) => {
    const request = http.get(url, { timeout: timeoutMs }, (response) => {
      let body = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => {
        body += chunk;
      });
      response.on("end", () => {
        if (response.statusCode < 200 || response.statusCode >= 300) {
          reject(new Error(`HTTP ${response.statusCode}`));
          return;
        }
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(error);
        }
      });
    });
    request.on("timeout", () => request.destroy(new Error("request timed out")));
    request.on("error", reject);
  });
}

class GhostexCdpClient {
  constructor(socket, options = {}) {
    this.nextId = 1;
    this.options = options;
    this.pending = new Map();
    this.socket = socket;
    this.captureEnabled = false;
    this.isClosed = false;
    socket.onMessage = (message) => this.handleMessage(message);
    socket.onClose = () => {
      this.isClosed = true;
      for (const { reject, timeout } of this.pending.values()) {
        clearTimeout(timeout);
        reject(new Error("CDP connection closed"));
      }
      this.pending.clear();
    };
  }

  static async connect(page, options = {}) {
    const socket = await SimpleWebSocket.connect(page.webSocketDebuggerUrl);
    return new GhostexCdpClient(socket, options);
  }

  async ensureCaptureEnabled() {
    if (this.captureEnabled) return;
    await this.call("Runtime.enable");
    await this.call("Log.enable");
    await this.call("Page.enable");
    this.captureEnabled = true;
  }

  call(method, params = {}) {
    if (this.isClosed) {
      return Promise.reject(new Error("CDP connection is closed"));
    }
    const id = this.nextId++;
    const timeoutMs = this.options.timeoutMs ?? 10_000;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Timed out waiting for CDP method ${method}`));
      }, timeoutMs);
      this.pending.set(id, { reject, resolve, timeout });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  handleMessage(message) {
    const parsed = parseJson(message);
    if (!parsed) return;
    if (parsed.id != null) {
      const pending = this.pending.get(parsed.id);
      if (!pending) return;
      clearTimeout(pending.timeout);
      this.pending.delete(parsed.id);
      if (parsed.error) {
        pending.reject(new Error(parsed.error.message ?? JSON.stringify(parsed.error)));
      } else {
        pending.resolve(parsed.result ?? {});
      }
      return;
    }
    this.options.onEvent?.(parsed);
  }
}

class SimpleWebSocket {
  constructor(socket) {
    this.buffer = Buffer.alloc(0);
    this.handshakeComplete = false;
    this.isClosed = false;
    this.onClose = null;
    this.onMessage = null;
    this.socket = socket;
    socket.on("data", (chunk) => this.read(chunk));
    socket.on("close", () => {
      this.isClosed = true;
      this.onClose?.();
    });
    socket.on("error", () => {
      this.isClosed = true;
      this.onClose?.();
    });
  }

  static connect(rawUrl) {
    const url = new URL(rawUrl);
    const port = Number(url.port || 80);
    return new Promise((resolve, reject) => {
      const socket = net.createConnection({ host: url.hostname, port }, () => {
        const key = randomBytes(16).toString("base64");
        const request = [
          `GET ${url.pathname}${url.search} HTTP/1.1`,
          `Host: ${url.host}`,
          "Upgrade: websocket",
          "Connection: Upgrade",
          `Sec-WebSocket-Key: ${key}`,
          "Sec-WebSocket-Version: 13",
          "\r\n",
        ].join("\r\n");
        socket.write(request);
      });
      const ws = new SimpleWebSocket(socket);
      const timeout = setTimeout(() => {
        socket.destroy();
        reject(new Error("Timed out opening CDP WebSocket"));
      }, 5_000);
      ws.onHandshake = (headers) => {
        clearTimeout(timeout);
        const accept = headers["sec-websocket-accept"];
        if (!accept) {
          reject(new Error("Invalid WebSocket handshake"));
          socket.destroy();
          return;
        }
        resolve(ws);
      };
      socket.on("error", reject);
    });
  }

  send(text) {
    if (this.isClosed) throw new Error("WebSocket is closed");
    const payload = Buffer.from(text, "utf8");
    const headerLength = payload.length < 126 ? 2 : payload.length < 65536 ? 4 : 10;
    const frame = Buffer.alloc(headerLength + 4 + payload.length);
    frame[0] = 0x81;
    if (payload.length < 126) {
      frame[1] = 0x80 | payload.length;
    } else if (payload.length < 65536) {
      frame[1] = 0x80 | 126;
      frame.writeUInt16BE(payload.length, 2);
    } else {
      frame[1] = 0x80 | 127;
      frame.writeBigUInt64BE(BigInt(payload.length), 2);
    }
    const maskOffset = headerLength;
    const mask = randomBytes(4);
    mask.copy(frame, maskOffset);
    for (let index = 0; index < payload.length; index += 1) {
      frame[maskOffset + 4 + index] = payload[index] ^ mask[index % 4];
    }
    this.socket.write(frame);
  }

  read(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    if (!this.handshakeComplete) {
      const headerEnd = this.buffer.indexOf("\r\n\r\n");
      if (headerEnd < 0) return;
      const headerText = this.buffer.slice(0, headerEnd).toString("utf8");
      this.buffer = this.buffer.slice(headerEnd + 4);
      const lines = headerText.split("\r\n");
      if (!/^HTTP\/1\.1 101/.test(lines[0])) {
        this.socket.destroy();
        return;
      }
      const headers = {};
      for (const line of lines.slice(1)) {
        const separator = line.indexOf(":");
        if (separator > 0) {
          headers[line.slice(0, separator).trim().toLowerCase()] = line.slice(separator + 1).trim();
        }
      }
      this.handshakeComplete = true;
      this.onHandshake?.(headers);
    }
    while (this.buffer.length >= 2) {
      const first = this.buffer[0];
      const second = this.buffer[1];
      const opcode = first & 0x0f;
      const masked = (second & 0x80) !== 0;
      let length = second & 0x7f;
      let offset = 2;
      if (length === 126) {
        if (this.buffer.length < 4) return;
        length = this.buffer.readUInt16BE(2);
        offset = 4;
      } else if (length === 127) {
        if (this.buffer.length < 10) return;
        length = Number(this.buffer.readBigUInt64BE(2));
        offset = 10;
      }
      const maskLength = masked ? 4 : 0;
      if (this.buffer.length < offset + maskLength + length) return;
      const mask = masked ? this.buffer.slice(offset, offset + 4) : null;
      offset += maskLength;
      let payload = this.buffer.slice(offset, offset + length);
      this.buffer = this.buffer.slice(offset + length);
      if (mask) {
        payload = Buffer.from(payload.map((byte, index) => byte ^ mask[index % 4]));
      }
      if (opcode === 0x1) {
        this.onMessage?.(payload.toString("utf8"));
      } else if (opcode === 0x8) {
        this.socket.end();
      } else if (opcode === 0x9) {
        this.socket.write(Buffer.from([0x8a, 0x00]));
      }
    }
  }
}

class McpStdioTransport {
  constructor(onMessage) {
    this.buffer = Buffer.alloc(0);
    this.onMessage = onMessage;
  }

  start() {
    process.stdin.on("data", (chunk) => this.read(chunk));
    process.stdin.resume();
  }

  send(message) {
    const body = Buffer.from(JSON.stringify(message), "utf8");
    process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
    process.stdout.write(body);
  }

  read(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    while (true) {
      const headerEnd = this.buffer.indexOf("\r\n\r\n");
      if (headerEnd < 0) return;
      const header = this.buffer.slice(0, headerEnd).toString("utf8");
      const lengthMatch = /^Content-Length:\s*(\d+)/im.exec(header);
      if (!lengthMatch) {
        this.buffer = Buffer.alloc(0);
        return;
      }
      const length = Number(lengthMatch[1]);
      const bodyStart = headerEnd + 4;
      if (this.buffer.length < bodyStart + length) return;
      const body = this.buffer.slice(bodyStart, bodyStart + length).toString("utf8");
      this.buffer = this.buffer.slice(bodyStart + length);
      const message = parseJson(body);
      if (message) {
        void this.onMessage(message);
      }
    }
  }
}

async function sendSidebarCliCommand(action, payload, flags = {}) {
  return sendGxserverCliAction(action, payload, flags);
}

class GxserverCliConnectionError extends Error {
  constructor(message, options = {}) {
    super(message);
    this.name = "GxserverCliConnectionError";
    this.cause = options.cause;
  }
}

class GxserverCliUnsupportedError extends Error {
  constructor(action) {
    super(
      `Ghostex CLI action "${action}" requires a gxserver API endpoint that is not available in this cutover. Update Ghostex/gxserver when that endpoint lands; the macOS app bridge is no longer a fallback.`,
    );
    this.name = "GxserverCliUnsupportedError";
  }
}

class GxserverCliRpcError extends Error {
  constructor(response) {
    const message =
      response?.error === "notImplemented"
        ? `${response?.message || "gxserver endpoint is not implemented."} Update Ghostex/gxserver when that endpoint lands; the macOS app bridge is no longer a fallback.`
        : response?.message || response?.error || "gxserver request failed.";
    super(message);
    this.name = "GxserverCliRpcError";
    this.response = response;
  }
}

async function sendGxserverCliAction(action, payload = {}, flags = {}) {
  /**
   * CDXC:GxserverCliCutover 2026-05-30-15:15:
   * gx/ghostex remains the Node user CLI, but hard-cutover commands must talk
   * to gxserver instead of the macOS app bridge. Renderer-only commands still
   * enter through a gxserver API endpoint so auth, protocol, remote access, and
   * unsupported-action failures stay daemon-owned.
   */
  switch (action) {
    case "listSessions":
      return fetchGxserverSessionList(flags);
    case "state":
    case "dumpState":
      return fetchGxserverState(flags);
    case "createQuickTerminal":
      return createGxserverQuickTerminal(payload, flags);
    case "createSession":
      return createGxserverSession(payload, flags);
    case "createAgentSession":
    case "runAgent":
      return createGxserverAgentSession(payload, flags);
    case "addProject":
      return callGxserverRpc("/api/addProjectPath", payload, flags);
    case "removeProject":
      return callGxserverRpc("/api/removeProject", payload, flags);
    case "closeSession":
      return callGxserverRpc("/api/killSession", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "sleepSession":
      return callGxserverRpc(
        payload?.sleeping === false ? "/api/wakeSession" : "/api/sleepSession",
        await withResolvedGxserverSessionParams(payload, flags),
        flags,
      );
    case "forkSession":
      return callGxserverRpc("/api/forkSession", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "renameSession":
      return callGxserverRpc("/api/updateSession", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "favoriteSession":
      return callGxserverRpc(
        "/api/updateSession",
        await withResolvedGxserverSessionParams({ ...payload, isFavorite: payload?.favorite }, flags),
        flags,
      );
    case "pinSession":
      return callGxserverRpc(
        "/api/updateSession",
        await withResolvedGxserverSessionParams({ ...payload, isPinned: payload?.pinned }, flags),
        flags,
      );
    case "acknowledgeSessionAttention":
      return callGxserverRpc(
        "/api/updateAgentActivity",
        await withResolvedGxserverSessionParams({ ...payload, event: "acknowledge" }, flags),
        flags,
      );
    case "focusSession":
      return callGxserverRpc("/api/focusSession", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "readSessionText":
      return callGxserverRpc("/api/readSessionText", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "sendText":
      return callGxserverRpc("/api/sendSessionText", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "sendEnter":
      return callGxserverRpc("/api/sendSessionEnter", await withResolvedGxserverSessionParams(payload, flags), flags);
    case "sendKey":
      return sendGxserverSessionKey(payload, flags);
    case "renameCommand":
      return sendGxserverRenameCommand(payload, flags);
    case "sendMessage":
      return callGxserverRpc("/api/sendSessionMessage", payload, flags);
    case "assertSidebarCard":
    case "automationArchiveRun":
    case "automationMarkRunRead":
    case "automationRunNow":
    case "automationSave":
    case "automationSetEnabled":
    case "automationState":
    case "clickButton":
    case "focusGroup":
    case "fullReloadSession":
    case "moveProject":
    case "moveSidebar":
    case "openBrowser":
    case "openBrowserPane":
    case "openPaths":
    case "restartSession":
    case "runCommand":
    case "saveAgent":
    case "setViewMode":
    case "setVisibleCount":
    case "switchProject":
    case "toggleSidebarCollapsed":
    case "waitFor":
      return dispatchGxserverRendererCommand(action, payload, flags);
    default:
      throw new GxserverCliUnsupportedError(action);
  }
}

function dispatchGxserverRendererCommand(action, payload = {}, flags = {}) {
  /*
   * CDXC:GxserverRendererCommands 2026-06-13-02:24:
   * CLI commands that still need visible macOS workspace state route through
   * gxserver's renderer-command endpoint. Do not reconnect the old native CLI
   * bridge; gxserver owns the command contract and macOS is only the executor.
   */
  return callGxserverRpc("/api/dispatchRendererCommand", { action, payload }, flags);
}

async function sendGxserverSessionKey(payload = {}, flags = {}) {
  const text = terminalTextForCliKey(payload.key);
  if (!text) {
    throw new Error(`Unsupported key: ${String(payload.key)}`);
  }
  return callGxserverRpc(
    "/api/sendSessionText",
    await withResolvedGxserverSessionParams({ ...payload, text }, flags),
    flags,
  );
}

async function sendGxserverRenameCommand(payload = {}, flags = {}) {
  const title = String(payload.title ?? "").trim();
  if (!title) {
    throw new Error("rename-command requires --title or a positional title.");
  }
  const params = await withResolvedGxserverSessionParams(payload, flags);
  /*
   * CDXC:GenerateTitleSkill 2026-06-13-01:55:
   * Agent-generated session titles call `ghostex rename-command` from inside the
   * current pane. The gxserver CLI cutover must keep that exposed command working
   * by staging `/rename <title>` through the session text endpoint, then submitting
   * Enter as a second interaction so the Agent CLI receives the same command shape
   * as the old native bridge path.
   */
  const textResult = await callGxserverRpc(
    "/api/sendSessionText",
    { ...params, text: `/rename ${title}` },
    flags,
  );
  const delayMs = renameCommandSubmitDelayMs(flags);
  if (delayMs > 0) {
    await sleep(delayMs);
  }
  const enterResult = await callGxserverRpc("/api/sendSessionEnter", params, flags);
  return {
    ok: true,
    enter: enterResult,
    session: enterResult.session ?? textResult.session,
    text: textResult,
  };
}

function renameCommandSubmitDelayMs(flags = {}) {
  const override = Number(flags.renameSubmitDelayMs ?? flags.submitDelayMs);
  if (Number.isFinite(override)) {
    return Math.max(0, override);
  }
  return GXSERVER_RENAME_COMMAND_SUBMIT_DELAY_MS;
}

function terminalTextForCliKey(key) {
  switch (String(key)) {
    case "ctrl-c":
    case "Control+C":
      return "\u0003";
    case "escape":
    case "Escape":
      return "\u001b";
    case "tab":
    case "Tab":
      return "\t";
    case "arrow-up":
    case "ArrowUp":
      return "\u001b[A";
    case "arrow-down":
    case "ArrowDown":
      return "\u001b[B";
    case "arrow-right":
    case "ArrowRight":
      return "\u001b[C";
    case "arrow-left":
    case "ArrowLeft":
      return "\u001b[D";
    default:
      return undefined;
  }
}

async function fetchGxserverState(flags = {}) {
  const [projectsResult, sessionsResult] = await Promise.all([
    callGxserverRpc("/api/listProjects", {}, flags),
    fetchGxserverSessionList(flags),
  ]);
  return {
    ok: true,
    product: GXSERVER_PRODUCT,
    projects: projectsResult.projects ?? [],
    sessions: sessionsResult.sessions ?? [],
  };
}

async function createGxserverQuickTerminal(payload = {}, flags = {}) {
  const cwd = path.resolve(String(payload.cwd ?? process.cwd()));
  const projectId = flags.projectId ?? payload.projectId ?? (await ensureGxserverProjectForPath(cwd, flags)).projectId;
  return createGxserverSession(
    {
      command: payload.command,
      cwd,
      projectId,
      title: payload.title ?? (payload.command ? String(payload.command).slice(0, 80) : "Terminal"),
    },
    flags,
  );
}

async function createGxserverSession(payload = {}, flags = {}) {
  const projectId = normalizeRequiredProjectId(payload.projectId ?? flags.projectId, "create-session");
  const params = {
    cwd: payload.cwd,
    kind: "terminal",
    launchSettings: payload.command ? { startupCommand: payload.command } : undefined,
    projectId,
    title: payload.title || "Terminal",
  };
  return callGxserverRpc("/api/createSession", compactObject(params), flags);
}

async function createGxserverAgentSession(payload = {}, flags = {}) {
  const projectId = normalizeRequiredProjectId(payload.projectId ?? flags.projectId, "create-agent");
  const agentId = String(payload.agentId ?? flags.agentId ?? "").trim();
  if (!agentId) {
    throw new Error("create-agent requires an agent id.");
  }
  return callGxserverRpc(
    "/api/createAgentSession",
    compactObject({
      agentId,
      projectId,
      title: flags.title,
    }),
    flags,
  );
}

async function ensureGxserverProjectForPath(projectPath, flags = {}) {
  const result = await callGxserverRpc("/api/addProjectPath", { path: projectPath }, flags);
  return result.project;
}

function normalizeRequiredProjectId(value, commandName) {
  const projectId = String(value ?? "").trim();
  if (!projectId) {
    throw new Error(`${commandName} requires --project-id until gxserver active-project routing lands.`);
  }
  return projectId;
}

function withResolvedSessionParams(payload = {}, flags = {}) {
  const globalParts = isGxserverGlobalSessionRef(payload.sessionId) ? String(payload.sessionId).split(":") : undefined;
  return compactObject({
    ...payload,
    globalRef: payload.globalRef ?? (globalParts ? payload.sessionId : undefined),
    projectId: payload.projectId ?? flags.projectId ?? globalParts?.[1],
    sessionId: globalParts?.[2] ?? payload.sessionId,
  });
}

async function withResolvedGxserverSessionParams(payload = {}, flags = {}) {
  const params = withResolvedSessionParams(payload, flags);
  if (params.projectId || !params.sessionId) {
    return params;
  }
  /*
   * CDXC:GxserverSessionLifecycle 2026-05-31-08:45:
   * Android, iOS, the gx TUI, and plain `gx` lifecycle commands send stable
   * `--session-id G...` selectors from `ghostex sessions --json`. gxserver
   * lifecycle RPCs require projectId too, so resolve bare session ids through
   * the daemon inventory instead of falling back to the retired macOS bridge or
   * making every client learn project-scoped RPC payloads.
   */
  const session = await resolveGxserverInventorySession(params.sessionId, flags);
  return compactObject({
    ...params,
    projectId: session.projectId,
    sessionId: session.sessionId,
  });
}

async function resolveGxserverInventorySession(sessionId, flags = {}) {
  const selector = String(sessionId ?? "").trim();
  if (!selector) {
    throw new Error("Session action requires --session-id.");
  }
  const result = await fetchGxserverSessionList({ ...flags, all: true, includeStopped: true });
  const matches = (result.sessions ?? []).filter(
    (session) => session.sessionId === selector || session.globalRef === selector,
  );
  if (matches.length === 1) {
    return matches[0];
  }
  if (matches.length > 1) {
    throw new Error(`Multiple gxserver sessions matched "${selector}". Use the full globalRef from ghostex sessions --json.`);
  }
  throw new Error(`No gxserver session matched "${selector}".`);
}

async function fetchGxserverSessionList(flags = {}) {
  try {
    return await fetchLiveGxserverSessionList(flags);
  } catch (error) {
    const fallback = await readPersistedGxserverSessionList(error, flags);
    if (fallback) {
      return fallback;
    }
    throw error;
  }
}

async function fetchLiveGxserverSessionList(flags = {}) {
  const [projectsResponse, sessionsResponse, presentationResponse] = await Promise.all([
    callGxserverRpc("/api/listProjects", {}, flags),
    callGxserverRpc("/api/listSessions", {}, flags),
    callGxserverRpc("/api/readPresentationSnapshot", {}, flags),
  ]);
  const projects = Array.isArray(projectsResponse.projects) ? projectsResponse.projects : [];
  const sessions = Array.isArray(sessionsResponse.sessions) ? sessionsResponse.sessions : [];
  const presentationBySessionKey = presentationSessionMap(presentationResponse.snapshot?.sessions);
  const projectById = new Map(projects.map((project) => [project.projectId, project]));
  /*
   * CDXC:GxserverSessionInventory 2026-05-31-08:45:
   * All five clients render the same gxserver inventory contract: macOS owns
   * local tab/split layout, but gxserver owns which zmx sessions still exist.
   * Default lists include running and sleeping sessions and hide stopped rows;
   * diagnostic callers may opt into stopped rows with --all/--include-stopped.
   *
   * CDXC:GxserverSessionInventory 2026-06-04-03:33:
   * Mobile session rows still consume `ghostex sessions --json` over SSH, while
   * gxserver computes working/attention in the presentation projector. Overlay
   * presentation activity onto the CLI inventory so Android, iOS, TUI, and gx
   * share the same status contract without guessing from lifecycle `running`.
   */
  const listedSessions = shouldIncludeStoppedGxserverSessions(flags)
    ? sessions
    : sessions.filter((session) => !isStoppedGxserverSession(session));
  return {
    ok: true,
    product: GXSERVER_PRODUCT,
    projects,
    revision: sessionsResponse.requestId,
    sessions: listedSessions.map((session, index) =>
      toCliSession(
        session,
        projectById.get(session.projectId),
        index,
        presentationBySessionKey.get(cliSessionKey(session.projectId, session.sessionId)),
      )
    ),
  };
}

async function readPersistedGxserverSessionList(cause, flags = {}) {
  if (!shouldUseLocalGxserverStateFallback(flags)) {
    return undefined;
  }
  if (!existsSync(GXSERVER_STATE_DB_PATH)) {
    return undefined;
  }
  const sqlitePath = existsSync("/usr/bin/sqlite3") ? "/usr/bin/sqlite3" : "sqlite3";
  const sql = [
    "SELECT 'project' AS rowType, projectId, name, path, NULL AS sessionId, NULL AS kind, NULL AS title, NULL AS lifecycleState, NULL AS providerStateJson, NULL AS zmxName, NULL AS cwd, NULL AS agentId, NULL AS updatedAt, NULL AS lastActiveAt FROM projects",
    "UNION ALL",
    "SELECT 'session' AS rowType, projectId, NULL AS name, NULL AS path, sessionId, kind, title, lifecycleState, providerStateJson, zmxName, cwd, agentId, updatedAt, lastActiveAt FROM sessions",
    "ORDER BY rowType ASC, updatedAt DESC, projectId ASC, sessionId ASC",
  ].join(" ");
  const { stdout } = await execFileAsync(sqlitePath, ["-json", GXSERVER_STATE_DB_PATH, sql], {
    timeout: 2_000,
  }).catch(() => ({ stdout: "" }));
  const rows = parseJson(stdout);
  if (!Array.isArray(rows)) {
    return undefined;
  }
  const serverId = await readPersistedGxserverServerId();
  const projects = rows
    .filter((row) => row?.rowType === "project")
    .map((row) => ({
      name: row.name,
      path: row.path,
      projectId: row.projectId,
    }));
  const projectById = new Map(projects.map((project) => [project.projectId, project]));
  const sessions = rows
    .filter((row) => row?.rowType === "session")
    .map((row) => ({
      agentId: row.agentId ?? undefined,
      cwd: row.cwd ?? undefined,
      globalRef: serverId && row.projectId && row.sessionId ? `${serverId}:${row.projectId}:${row.sessionId}` : undefined,
      kind: row.kind,
      lastActiveAt: row.lastActiveAt ?? undefined,
      lifecycleState: row.lifecycleState,
      projectId: row.projectId,
      providerState: parseJson(row.providerStateJson) ?? {},
      sessionId: row.sessionId,
      title: row.title,
      updatedAt: row.updatedAt,
      zmxName: row.zmxName,
    }));
  const listedSessions = shouldIncludeStoppedGxserverSessions(flags)
    ? sessions
    : sessions.filter((session) => !isStoppedGxserverSession(session));
  /*
   * CDXC:CliSessions 2026-06-03-20:28:
   * After the gxserver cutover, bridge-down session inventory should degrade
   * from gxserver's own durable state instead of the retired native-sidebar
   * project JSON. Keep the fallback read-only and visibly marked so humans and
   * Android can distinguish stale persisted rows from live daemon data.
   */
  return {
    error: cause instanceof Error ? cause.message : String(cause),
    fallback: "persisted-gxserver-state",
    ok: true,
    product: GXSERVER_PRODUCT,
    projects,
    sessions: listedSessions.map((session, index) => toCliSession(session, projectById.get(session.projectId), index)),
  };
}

function shouldUseLocalGxserverStateFallback(flags = {}) {
  const server = String(flags.server ?? process.env.GHOSTEX_GXSERVER_SERVER ?? "local").trim() || "local";
  return server === "local";
}

async function readPersistedGxserverServerId() {
  const identity = parseJson(await readFile(GXSERVER_IDENTITY_PATH, "utf8").catch(() => ""));
  return typeof identity?.serverId === "string" ? identity.serverId : undefined;
}

function shouldIncludeStoppedGxserverSessions(flags = {}) {
  return flags.all === true || flags.includeStopped === true || flags.stopped === true;
}

function isStoppedGxserverSession(session) {
  return String(session?.lifecycleState ?? "") === "stopped";
}

function toCliSession(session, project, index, presentationSession) {
  const lifecycleState = String(session.lifecycleState ?? "");
  const providerState = String(session.providerState?.lifecycleState ?? "");
  const activity = normalizeCliSessionActivity(presentationSession?.activity);
  const status =
    lifecycleState === "sleeping"
      ? "sleep"
      : lifecycleState === "stopped"
        ? "stopped"
        : lifecycleState === "running"
          ? "running"
          : providerState || lifecycleState || "unknown";
  const providerSessionName = session.zmxName ?? session.providerState?.zmxName;
  const title = presentationSession?.title ?? session.title;
  /*
   * CDXC:GxserverSessionTitles 2026-06-07-09:33:
   * CLI and mobile inventory should expose gxserver's rendered display title separately from the raw durable title, so clients can show the same unsynced/placeholder title chrome without leaking display glyphs into rename or restore payloads.
   */
  const displayTitle = presentationSession?.displayTitle ?? title;
  return {
    actions: presentationSession?.actions,
    agent: session.agentId,
    agentId: session.agentId,
    agentIcon: presentationSession?.agentIcon ?? session.agentId,
    agentName: presentationSession?.agentName,
    alias: index + 1,
    attention: presentationSession?.attention,
    createdAt: presentationSession?.createdAt ?? session.createdAt,
    globalRef: session.globalRef,
    groupId: presentationSession?.groupId,
    displayTitle,
    displayTitleTooltip: presentationSession?.displayTitleTooltip ?? displayTitle,
    isFocused: false,
    isFavorite: presentationSession?.isFavorite ?? session.isFavorite,
    isLocalOnly: false,
    isPinned: presentationSession?.isPinned ?? session.isPinned,
    isLive: lifecycleState === "running" || providerState === "exists",
    isPrimaryTitleTerminalTitle: presentationSession?.isPrimaryTitleTerminalTitle,
    isSleeping: lifecycleState === "sleeping",
    isTemporaryTitle: presentationSession?.isTemporaryTitle,
    kind: presentationSession?.kind ?? session.kind,
    lastActiveAt: presentationSession?.lastActiveAt ?? session.lastActiveAt,
    lastInteractionAt: presentationSession?.lastActiveAt ?? session.lastActiveAt ?? session.updatedAt,
    lifecycleState,
    ownership: "gxserver",
    primaryTitle: presentationSession?.primaryTitle,
    projectId: session.projectId,
    projectName: project?.name ?? session.projectId,
    projectPath: presentationSession?.cwd ?? session.cwd ?? project?.path ?? "",
    provider: "zmx",
    providerSessionName,
    providerSessionState: providerState || undefined,
    sessionId: session.sessionId,
    sessionPersistenceName: providerSessionName,
    sessionPersistenceProvider: "zmx",
    sidebarOrder: presentationSession?.sidebarOrder,
    sortKey: presentationSession?.sortKey,
    status,
    activity,
    surface: presentationSession?.surface,
    terminalTitle: presentationSession?.terminalTitle,
    title,
    titleSource: presentationSession?.titleSource,
    trustedResumeTitle: presentationSession?.trustedResumeTitle,
    updatedAt: presentationSession?.updatedAt ?? session.updatedAt,
    visibleInSidebarByDefault: presentationSession?.visibleInSidebarByDefault,
    zmxName: presentationSession?.zmxName ?? session.zmxName,
  };
}

function presentationSessionMap(sessions) {
  const map = new Map();
  if (!Array.isArray(sessions)) {
    return map;
  }
  for (const session of sessions) {
    const key = cliSessionKey(session?.projectId, session?.sessionId);
    if (key) {
      map.set(key, session);
    }
  }
  return map;
}

function cliSessionKey(projectId, sessionId) {
  const normalizedProjectId = String(projectId ?? "").trim();
  const normalizedSessionId = String(sessionId ?? "").trim();
  return normalizedProjectId && normalizedSessionId ? `${normalizedProjectId}:${normalizedSessionId}` : "";
}

function normalizeCliSessionActivity(value) {
  const normalized = String(value ?? "").trim().toLowerCase().replaceAll("_", "-").replaceAll(" ", "-");
  if (normalized === "attention" || normalized === "needs-attention" || normalized === "attention-required") {
    return "attention";
  }
  if (normalized === "working" || normalized === "active" || normalized === "busy" || normalized === "processing") {
    return "working";
  }
  return "idle";
}

async function fetchAttachMetadataForSession(session, flags = {}) {
  const promptEditor = promptEditorAttachModeFromFlags(flags);
  const result = await callGxserverRpc(
    "/api/attachSessionMetadata",
    compactObject({
      projectId: session.projectId ?? projectIdFromGlobalRef(session.globalRef),
      promptEditor,
      sessionId: session.sessionId,
    }),
    flags,
  );
  return result.attach;
}

async function startMissingProviderForCliAttach(session, attach, flags = {}) {
  if (!shouldStartMissingProviderForCliAttach(attach)) {
    return attach;
  }
  /**
   * CDXC:GxserverCliAttach 2026-06-09-09:53:
   * CLI, TUI, Android, and SSH attach commands can create a zmx provider before macOS ever sees the row. Start missing providers through gxserver before launching the blocking interactive attach so gxserver persists providerState=exists and publishes the sidebar presentation delta instead of leaving clients split between live zmx and stale daemon state.
   */
  await callGxserverRpc(
    "/api/startSessionProvider",
    compactObject({
      projectId: attach.session?.projectId ?? session.projectId ?? projectIdFromGlobalRef(session.globalRef),
      promptEditor: promptEditorAttachModeFromFlags(flags),
      sessionId: attach.session?.sessionId ?? session.sessionId,
      startupText: attach.startupText,
    }),
    flags,
  );
  return fetchAttachMetadataForSession(session, flags);
}

function promptEditorAttachModeFromFlags(flags = {}) {
  /*
   * CDXC:PromptEditor 2026-06-11-18:24:
   * `ghostex attach --prompt-editor monaco` is the SSH-safe desktop capability
   * advertisement. Android, iOS, TUI, and human SSH attaches omit this flag, so
   * gxserver returns zmx attach commands without Monaco capability and Ctrl+G
   * stays terminal-native through gte.
   */
  const value = String(flags.promptEditor ?? "").trim().toLowerCase();
  return value === "monaco" ? "monaco" : undefined;
}

function shouldStartMissingProviderForCliAttach(attach) {
  return (
    attach &&
    !attach.restoreBlocked &&
    attach.provider === "zmx" &&
    attach.providerState?.lifecycleState === "missing"
  );
}

function applyAttachMetadataToCliSession(session, attach) {
  if (!attach) {
    return session;
  }
  if (attach.restoreBlocked) {
    const cwd = attach.restoreBlocked.cwd ? ` (${attach.restoreBlocked.cwd})` : "";
    throw new Error(`Session ${session.title ?? session.sessionId} cannot be restored because its cwd is missing${cwd}.`);
  }
  const providerLifecycle = attach.providerState?.lifecycleState;
  const resumeCommand = normalizeStartupTextForShell(attach.startupText);
  return {
    ...session,
    attachCommand: attach.attachCommand,
    projectPath: attach.cwd ?? session.projectPath,
    provider: attach.provider ?? session.provider,
    providerSessionName: attach.zmxName ?? session.providerSessionName,
    resumeCommand,
    status: providerLifecycle === "exists" ? "running" : resumeCommand ? "sleep" : session.status,
  };
}

function normalizeStartupTextForShell(value) {
  const text = String(value ?? "").replace(/\r+$/u, "").trim();
  return text || undefined;
}

async function callGxserverRpc(pathname, params = {}, flags = {}) {
  const target = await resolveGxserverServerTarget(flags, params);
  return requestGxserverRpc(target, pathname, params, flags);
}

async function requestGxserverRpc(target, pathname, params = {}, flags = {}) {
  const releaseSshTunnel = await ensureGxserverSshTunnelForRpc(target, flags);
  const controller = new AbortController();
  const timeoutMs = Number(flags.timeout ?? flags.timeoutMs ?? 15_000);
  const timeout = timeoutMs > 0 ? setTimeout(() => controller.abort(), timeoutMs) : undefined;
  try {
    const response = await fetch(`${target.baseUrl}${pathname}`, {
      body: JSON.stringify({
        params,
        protocolVersion: GXSERVER_PROTOCOL_VERSION,
      }),
      headers: {
        authorization: `Bearer ${target.token}`,
        "content-type": "application/json",
        [GXSERVER_PROTOCOL_HEADER]: String(GXSERVER_PROTOCOL_VERSION),
      },
      method: "POST",
      signal: controller.signal,
    });
    const body = await response.json().catch(() => undefined);
    if (body?.product === GXSERVER_PRODUCT && body.protocolVersion !== undefined && body.protocolVersion !== GXSERVER_PROTOCOL_VERSION) {
      throw new GxserverCliRpcError({
        error: "protocolMismatch",
        message: `gxserver protocol mismatch. Expected protocol ${GXSERVER_PROTOCOL_VERSION}, got ${String(body.protocolVersion)}. Update Ghostex and gxserver so their protocol versions match.`,
        ok: false,
      });
    }
    if (!response.ok || body?.ok === false) {
      throw new GxserverCliRpcError(body ?? { error: `http-${response.status}`, message: `gxserver HTTP ${response.status}`, ok: false });
    }
    return { ok: true, ...(body?.result ?? {}), requestId: body?.requestId };
  } catch (error) {
    if (error instanceof GxserverCliRpcError) {
      throw error;
    }
    const localTarget = isLocalGxserverTarget(target);
    throw new GxserverCliConnectionError(
      target.kind === "ssh"
        ? `Could not connect to SSH gxserver profile${target.profileId ? ` "${target.profileId}"` : ""} at ${target.baseUrl}. Check SSH access, remote gxserver status, and the local tunnel, then retry.`
        : localTarget
          ? `Could not connect to local gxserver at ${target.baseUrl}. Start it with "gx server start" and retry.`
          : `Could not connect to remote gxserver at ${target.baseUrl}. Start gxserver on that host and retry.`,
      { cause: error },
    );
  } finally {
    if (timeout) {
      clearTimeout(timeout);
    }
    releaseSshTunnel();
  }
}

function isLocalGxserverTarget(target) {
  if (target?.kind === "local") {
    return true;
  }
  try {
    const url = new URL(String(target?.baseUrl ?? ""));
    return ["127.0.0.1", "localhost", "::1", "[::1]"].includes(url.hostname);
  } catch {
    return false;
  }
}

async function ensureGxserverSshTunnelForRpc(target, flags = {}) {
  if (target?.kind !== "ssh" || !target.forwardPlan) {
    return () => {};
  }
  const existingHealth = await fetchGxserverHealth(target, { timeoutMs: 600 }).catch(() => undefined);
  if (isExpectedGxserverHealth(existingHealth, target)) {
    return () => {};
  }

  installGxserverSshTunnelExitHooks();
  const key = gxserverSshTunnelCacheKey(target);
  let record = activeGxserverSshTunnels.get(key);
  if (!record) {
    record = {
      child: undefined,
      idleTimer: undefined,
      ready: startGxserverSshTunnel(target, flags),
      refs: 0,
    };
    record.ready
      .then((child) => {
        record.child = child;
      })
      .catch(() => {
        activeGxserverSshTunnels.delete(key);
      });
    activeGxserverSshTunnels.set(key, record);
  }
  record.refs += 1;
  if (record.idleTimer) {
    clearTimeout(record.idleTimer);
    record.idleTimer = undefined;
  }
  let released = false;
  const release = () => {
    if (released) {
      return;
    }
    released = true;
    record.refs = Math.max(0, record.refs - 1);
    if (record.refs === 0) {
      record.idleTimer = setTimeout(() => {
        if (record.refs === 0) {
          stopGxserverSshTunnelRecord(key, record);
        }
      }, Number(flags.sshTunnelIdleKillMs ?? GXSERVER_SSH_TUNNEL_IDLE_KILL_MS));
      record.idleTimer.unref?.();
    }
  };

  try {
    await record.ready;
    return release;
  } catch (error) {
    release();
    throw error;
  }
}

async function startGxserverSshTunnel(target, flags = {}) {
  const { forwardPlan } = target;
  /**
   * CDXC:GxserverRemoteCli 2026-05-30-20:18:
   * SSH gxserver profiles must establish a verified local tunnel before any RPC fetch. The CLI checks remote gxserver status through SSH, starts it when stopped, spawns a loopback forward only after the remote daemon is expected to be running, and tears down CLI-owned tunnel children after the request burst so failed or interrupted setup does not leave obvious `ssh -N -L` processes behind.
   */
  const before = await runGxserverSshStatusCommand(forwardPlan, flags).catch((error) => {
    throw formatGxserverSshSetupError("check", target, forwardPlan, error);
  });
  if (before?.state !== "running") {
    await runGxserverSshCommand(forwardPlan.startCommand, flags, "start").catch((error) => {
      throw formatGxserverSshSetupError("start", target, forwardPlan, error);
    });
    const after = await runGxserverSshStatusCommand(forwardPlan, flags).catch((error) => {
      throw formatGxserverSshSetupError("check", target, forwardPlan, error);
    });
    if (after?.state !== "running") {
      throw new GxserverCliConnectionError(
        `SSH gxserver profile${target.profileId ? ` "${target.profileId}"` : ""} started remote gxserver, but status is still ${String(after?.state ?? "unknown")}. Run ${formatCommand(forwardPlan.checkCommand)} on the remote profile to inspect it.`,
      );
    }
  }

  const child = spawnGxserverSshTunnel(forwardPlan.portForwardCommand, flags);
  try {
    await waitForGxserverSshTunnelHealth(target, child, flags);
    return child;
  } catch (error) {
    stopGxserverSshTunnelChild(child);
    throw error;
  }
}

async function runGxserverSshStatusCommand(forwardPlan, flags = {}) {
  const result = await runGxserverSshCommand(forwardPlan.checkCommand, flags, "check");
  const parsed = parseJson(result.stdout);
  if (parsed?.product === GXSERVER_PRODUCT) {
    return parsed;
  }
  return undefined;
}

async function runGxserverSshCommand(command, flags = {}, phase = "ssh") {
  const runner = flags.sshCommandRunner ?? defaultGxserverSshCommandRunner;
  const result = await runner(command, {
    phase,
    timeoutMs: Number(flags.sshCommandTimeoutMs ?? GXSERVER_SSH_COMMAND_TIMEOUT_MS),
  });
  if (typeof result === "string") {
    return { stderr: "", stdout: result };
  }
  return {
    stderr: String(result?.stderr ?? ""),
    stdout: String(result?.stdout ?? ""),
  };
}

async function defaultGxserverSshCommandRunner(command, options = {}) {
  const [file, ...args] = command;
  return execFileAsync(file, args, {
    encoding: "utf8",
    maxBuffer: 128 * 1024,
    timeout: options.timeoutMs,
    windowsHide: true,
  });
}

function spawnGxserverSshTunnel(command, flags = {}) {
  const spawner = flags.sshTunnelSpawner ?? defaultGxserverSshTunnelSpawner;
  return spawner(command, {
    timeoutMs: Number(flags.sshTunnelReadyTimeoutMs ?? GXSERVER_SSH_TUNNEL_READY_TIMEOUT_MS),
  });
}

function defaultGxserverSshTunnelSpawner(command) {
  const [file, ...args] = command;
  return spawn(file, args, { stdio: "ignore", windowsHide: true });
}

async function waitForGxserverSshTunnelHealth(target, child, flags = {}) {
  const timeoutMs = Number(flags.sshTunnelReadyTimeoutMs ?? GXSERVER_SSH_TUNNEL_READY_TIMEOUT_MS);
  const deadline = Date.now() + timeoutMs;
  let exited;
  child.once?.("exit", (code, signal) => {
    exited = { code, signal };
  });
  while (Date.now() < deadline) {
    if (exited) {
      throw new GxserverCliConnectionError(
        `SSH tunnel for gxserver profile${target.profileId ? ` "${target.profileId}"` : ""} exited before it became ready (code ${String(exited.code)}, signal ${String(exited.signal)}). Check SSH forwarding permissions and port ${target.forwardPlan.localPort}.`,
      );
    }
    const health = await fetchGxserverHealth(target, { timeoutMs: 600 }).catch(() => undefined);
    if (isExpectedGxserverHealth(health, target)) {
      return;
    }
    await sleep(Number(flags.sshTunnelPollMs ?? 100));
  }
  throw new GxserverCliConnectionError(
    `SSH tunnel for gxserver profile${target.profileId ? ` "${target.profileId}"` : ""} did not become healthy on ${target.baseUrl}. Check that ${formatCommand(target.forwardPlan.portForwardCommand)} can connect and that the remote gxserver token is valid.`,
  );
}

function isExpectedGxserverHealth(health, target) {
  return (
    health?.product === GXSERVER_PRODUCT &&
    health.protocolVersion === GXSERVER_PROTOCOL_VERSION &&
    (!target.serverId || health.serverId === target.serverId)
  );
}

function formatGxserverSshSetupError(phase, target, forwardPlan, error) {
  if (error?.code === "ENOENT") {
    return new GxserverCliConnectionError(
      `Could not set up SSH gxserver profile${target.profileId ? ` "${target.profileId}"` : ""} because the "ssh" executable was not found on PATH. Install OpenSSH, or use a direct/Tailscale gxserver profile instead.`,
      { cause: error },
    );
  }
  const stderr = String(error?.stderr ?? "").trim();
  const stdout = String(error?.stdout ?? "").trim();
  const output = [stderr, stdout].filter(Boolean).join("\n").slice(0, 1200);
  const command = phase === "start" ? forwardPlan.startCommand : forwardPlan.checkCommand;
  const guidance = phase === "check" ? ` ${forwardPlan.installGuidance}` : "";
  return new GxserverCliConnectionError(
    `Could not ${phase} remote gxserver for SSH profile${target.profileId ? ` "${target.profileId}"` : ""} with ${formatCommand(command)}.${guidance}${output ? `\nSSH output:\n${output}` : ""}`,
    { cause: error },
  );
}

function gxserverSshTunnelCacheKey(target) {
  return [
    target.profileId ?? "",
    target.serverId ?? "",
    target.baseUrl,
    target.forwardPlan.localPort,
    target.forwardPlan.remoteLocalPort,
    ...target.forwardPlan.portForwardCommand,
  ].join("\0");
}

function formatCommand(command) {
  return command.map(shellQuote).join(" ");
}

function stopGxserverSshTunnelRecord(key, record) {
  activeGxserverSshTunnels.delete(key);
  if (record.idleTimer) {
    clearTimeout(record.idleTimer);
    record.idleTimer = undefined;
  }
  stopGxserverSshTunnelChild(record.child);
  record.ready
    .then((child) => stopGxserverSshTunnelChild(child))
    .catch(() => {});
}

function stopGxserverSshTunnelChild(child) {
  if (!child || child.killed) {
    return;
  }
  child.kill?.("SIGTERM");
  setTimeout(() => {
    if (!child.killed) {
      child.kill?.("SIGKILL");
    }
  }, 1_000).unref?.();
}

function installGxserverSshTunnelExitHooks() {
  if (gxserverSshTunnelExitHooksInstalled) {
    return;
  }
  gxserverSshTunnelExitHooksInstalled = true;
  const stopAll = () => {
    for (const [key, record] of activeGxserverSshTunnels) {
      stopGxserverSshTunnelRecord(key, record);
    }
  };
  process.once("exit", stopAll);
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.once(signal, () => {
      stopAll();
      process.kill(process.pid, signal);
    });
  }
}

async function resolveGxserverServerTarget(flags = {}, params = {}) {
  const server = String(flags.server ?? process.env.GHOSTEX_GXSERVER_SERVER ?? "local").trim() || "local";
  const globalRef = findGlobalRefCandidate(params);
  if (globalRef) {
    const serverId = globalRef.split(":")[0];
    const local = await resolveLocalGxserverTarget();
    const health = await fetchGxserverHealth(local).catch(() => undefined);
    if (health?.serverId === serverId) {
      return local;
    }
    const profile = await findGxserverConnectionProfileByServerId(serverId);
    if (!profile) {
      throw new Error(`Global session ref ${globalRef} targets ${serverId}, but no gxserver connection profile exists for that server.`);
    }
    return resolveGxserverProfileTarget(profile, flags);
  }
  if (server === "local") {
    return resolveLocalGxserverTarget();
  }
  if (/^ssh:\/\//u.test(server)) {
    return resolveGxserverProfileTarget(
      {
        id: server,
        name: server,
        sshUrl: server,
        transport: "ssh",
      },
      flags,
    );
  }
  if (/^https?:\/\//u.test(server)) {
    return {
      baseUrl: server.replace(/\/+$/u, ""),
      kind: "direct",
      token: await readGxserverCredentialSecretFromFlags(flags),
    };
  }
  const profile = await readGxserverConnectionProfile(server);
  if (!profile) {
    throw new Error(`gxserver profile "${server}" was not found in ${GXSERVER_CONNECTIONS_PATH}.`);
  }
  return resolveGxserverProfileTarget(profile, flags);
}

async function resolveLocalGxserverTarget() {
  const token = await readLocalGxserverAuthToken();
  return {
    baseUrl: `http://${GXSERVER_LOCAL_API_HOST}:${GXSERVER_LOCAL_API_PORT}`,
    kind: "local",
    token,
  };
}

async function readLocalGxserverAuthToken() {
  const token = (await readFile(GXSERVER_AUTH_TOKEN_PATH, "utf8").catch(() => "")).trim();
  if (!token) {
    throw new GxserverCliConnectionError(
      `Could not read local gxserver auth token at ${GXSERVER_AUTH_TOKEN_PATH}. Start gxserver with "gx server start" and retry.`,
    );
  }
  return token;
}

async function fetchGxserverHealth(target, options = {}) {
  const controller = new AbortController();
  const timeoutMs = Number(options.timeoutMs ?? 1_000);
  const timeout = timeoutMs > 0 ? setTimeout(() => controller.abort(), timeoutMs) : undefined;
  try {
    const response = await fetch(`${target.baseUrl}/api/health/server?protocolVersion=${GXSERVER_PROTOCOL_VERSION}`, {
      headers: {
        authorization: `Bearer ${target.token}`,
        [GXSERVER_PROTOCOL_HEADER]: String(GXSERVER_PROTOCOL_VERSION),
      },
      method: "GET",
      signal: controller.signal,
    });
    if (!response.ok) {
      return undefined;
    }
    return response.json();
  } finally {
    if (timeout) {
      clearTimeout(timeout);
    }
  }
}

async function readGxserverConnectionProfile(name) {
  const text = await readFile(GXSERVER_CONNECTIONS_PATH, "utf8").catch(() => "");
  const parsed = text ? parseJson(text) : undefined;
  if (!parsed || typeof parsed !== "object") {
    return undefined;
  }
  if (Array.isArray(parsed.profiles)) {
    return parsed.profiles.find((profile) => profile?.name === name || profile?.id === name);
  }
  return parsed[name];
}

async function findGxserverConnectionProfileByServerId(serverId) {
  const text = await readFile(GXSERVER_CONNECTIONS_PATH, "utf8").catch(() => "");
  const parsed = text ? parseJson(text) : undefined;
  if (!parsed || typeof parsed !== "object") {
    return undefined;
  }
  const profiles = Array.isArray(parsed.profiles) ? parsed.profiles : Object.values(parsed);
  return profiles.find((profile) => profile?.serverId === serverId);
}

async function resolveGxserverProfileTarget(profile, flags = {}) {
  /**
   * CDXC:GxserverRemoteCli 2026-05-30-15:25:
   * gx remote routing supports multiple named profiles and S:P:G refs from
   * `~/.ghostex/clients/connections.json`. Direct/Tailscale profiles connect
   * to their trusted-network URL with an OS credential-store token; SSH
   * profiles expose a deterministic check/start/forward plan and use the
   * forwarded local URL for gxserver RPCs.
   */
  const transport = String(profile.transport ?? (profile.sshUrl ? "ssh" : "direct"));
  const token = await readGxserverConnectionProfileToken(profile, flags);
  if (transport === "ssh") {
    const explicitLocalPort = Number(flags.localPort ?? flags.forwardPort ?? localPortFromBaseUrl(flags.baseUrl) ?? 0) || undefined;
    const localPort = await selectCliSshForwardLocalPort(explicitLocalPort);
    const forwardPlan = createCliSshForwardPlan(profile, {
      localPort,
      remoteLocalPort: Number(flags.remotePort ?? GXSERVER_LOCAL_API_PORT),
    });
    return {
      baseUrl: String(flags.baseUrl ?? forwardPlan.baseUrl),
      forwardPlan,
      kind: "ssh",
      profileId: profile.id,
      serverId: profile.serverId,
      token,
    };
  }
  const baseUrl = String(profile.baseUrl ?? "").replace(/\/+$/u, "");
  if (!baseUrl) {
    throw new Error(`gxserver profile "${profile.name ?? profile.id}" is missing baseUrl.`);
  }
  return {
    baseUrl,
    kind: transport === "tailscale" ? "tailscale" : "direct",
    profileId: profile.id,
    serverId: profile.serverId,
    token,
  };
}

async function selectCliSshForwardLocalPort(explicitLocalPort) {
  if (explicitLocalPort) {
    return explicitLocalPort;
  }
  /**
   * CDXC:GxserverRemoteCli 2026-05-30-20:18:
   * SSH profiles cannot default their local forward to gxserver's own local API port. A running local daemon owns 58744, so SSH RPCs choose the next available loopback port and scan upward before falling back to an ephemeral port.
   */
  for (
    let port = GXSERVER_SSH_FORWARD_DEFAULT_PORT;
    port < GXSERVER_SSH_FORWARD_DEFAULT_PORT + GXSERVER_SSH_FORWARD_PORT_SCAN_LIMIT;
    port += 1
  ) {
    if (await isLoopbackPortAvailable(port)) {
      return port;
    }
  }
  return reserveEphemeralLoopbackPort();
}

function localPortFromBaseUrl(baseUrl) {
  if (!baseUrl) {
    return undefined;
  }
  try {
    const parsed = new URL(String(baseUrl));
    return parsed.hostname === "127.0.0.1" || parsed.hostname === "localhost" ? Number(parsed.port) : undefined;
  } catch {
    return undefined;
  }
}

async function isLoopbackPortAvailable(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.listen(port, "127.0.0.1", () => {
      server.close(() => resolve(true));
    });
  });
}

async function reserveEphemeralLoopbackPort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address && typeof address === "object") {
          resolve(address.port);
          return;
        }
        reject(new Error("Could not reserve an ephemeral local port for the SSH gxserver tunnel."));
      });
    });
  });
}

async function readGxserverConnectionProfileToken(profile, flags = {}) {
  if (flags.token || flags.tokenStdin || flags.tokenFromStdin) {
    return readGxserverCredentialSecretFromFlags(flags);
  }
  if (profile.token) {
    throw new Error(`gxserver profile "${profile.name ?? profile.id}" contains a plaintext token. Move it to the OS credential store and keep only tokenSecretRef in ${GXSERVER_CONNECTIONS_PATH}.`);
  }
  if (!profile.tokenSecretRef) {
    return readGxserverCredentialSecretFromFlags(flags);
  }
  return readGxserverCredentialSecret(profile.tokenSecretRef);
}

async function readGxserverCredentialSecretFromFlags(flags = {}) {
  if (flags.tokenStdin || flags.tokenFromStdin) {
    const token = (await readGxserverOneShotTokenFromStdin(flags)).trim();
    if (!token) {
      throw new Error("Remote gxserver --token-stdin did not receive a token.");
    }
    return token;
  }
  if (flags.token) {
    return String(flags.token);
  }
  throw new Error("Remote gxserver profiles require an auth token stored in the OS credential store. Add tokenSecretRef to the profile. For temporary one-shot use, pass --token-stdin; --token remains available for legacy scripts but can expose the token in shell history and process listings.");
}

async function readGxserverOneShotTokenFromStdin(flags = {}) {
  /**
   * CDXC:GxserverRemoteCredentials 2026-05-30-20:47:
   * Direct/Tailscale one-shot tokens should have a stdin path so humans and scripts do not need to put bearer tokens in argv. The legacy `--token` flag remains accepted for compatibility but is not the recommended setup path.
   */
  const reader = flags.stdinReader ?? defaultReadGxserverOneShotTokenFromStdin;
  return reader();
}

async function defaultReadGxserverOneShotTokenFromStdin() {
  if (process.stdin.isTTY) {
    throw new Error("Remote gxserver --token-stdin requires a token piped on stdin.");
  }
  let text = "";
  process.stdin.setEncoding("utf8");
  for await (const chunk of process.stdin) {
    text += chunk;
  }
  return text;
}

async function readGxserverCredentialSecret(ref) {
  const service = String(ref?.service ?? "ghostex.gxserver");
  const account = String(ref?.account ?? "");
  if (!account) {
    throw new Error("gxserver credential secret ref is missing account.");
  }
  if (process.platform === "darwin") {
    const { stdout } = await execFileAsync("security", ["find-generic-password", "-s", service, "-a", account, "-w"]);
    return stdout.trim();
  }
  if (process.platform === "linux") {
    const { stdout } = await execFileAsync("secret-tool", ["lookup", "service", service, "account", account]);
    return stdout.trim();
  }
  throw new Error("This platform needs a gxserver OS credential-store integration before remote tokens can be read.");
}

function createCliSshForwardPlan(profile, options = {}) {
  const sshUrl = String(profile.sshUrl ?? profile.id ?? "");
  const target = parseCliSshUrl(sshUrl);
  const localPort = Number(options.localPort ?? GXSERVER_SSH_FORWARD_DEFAULT_PORT) || GXSERVER_SSH_FORWARD_DEFAULT_PORT;
  const remoteLocalPort = Number(options.remoteLocalPort ?? GXSERVER_LOCAL_API_PORT);
  return {
    baseUrl: `http://127.0.0.1:${localPort}`,
    checkCommand: ["ssh", ...cliSshTargetArgs(target), "command -v gxserver >/dev/null && gxserver status --json"],
    installGuidance:
      "gxserver is not installed on the remote host. Install the Ghostex server package there, then retry; the SSH helper does not install software silently.",
    localPort,
    portForwardCommand: ["ssh", "-N", "-o", "ExitOnForwardFailure=yes", "-L", `${localPort}:127.0.0.1:${remoteLocalPort}`, ...cliSshTargetArgs(target)],
    remoteLocalPort,
    startCommand: ["ssh", ...cliSshTargetArgs(target), "gxserver start --background"],
  };
}

function parseCliSshUrl(value) {
  const url = new URL(value);
  if (url.protocol !== "ssh:" || !url.hostname) {
    throw new Error("SSH gxserver profiles must use ssh://user@host.");
  }
  return {
    host: url.hostname,
    port: url.port ? Number(url.port) : undefined,
    user: url.username ? decodeURIComponent(url.username) : undefined,
  };
}

function cliSshTargetArgs(target) {
  return [...(target.port ? ["-p", String(target.port)] : []), target.user ? `${target.user}@${target.host}` : target.host];
}

function findGlobalRefCandidate(value) {
  if (typeof value === "string" && isGxserverGlobalSessionRef(value)) {
    return value;
  }
  if (value && typeof value === "object") {
    for (const key of ["globalRef", "sessionId", "target", "selector"]) {
      if (isGxserverGlobalSessionRef(value[key])) {
        return value[key];
      }
    }
  }
  return undefined;
}

function projectIdFromGlobalRef(value) {
  return isGxserverGlobalSessionRef(value) ? value.split(":")[1] : undefined;
}

function isGxserverGlobalSessionRef(value) {
  return /^S[0-9][a-z0-9]*:P[0-9][a-z0-9]*:G[0-9][a-z0-9]*$/u.test(String(value ?? ""));
}

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entryValue]) => entryValue !== undefined),
  );
}

async function sendLegacySidebarCliCommand(action, payload, flags = {}) {
  const port = Number(
    flags.port ??
      process.env.GHOSTEX_CLI_PORT ??
      (process.env.GHOSTEX_APP_VARIANT === "dev" ? DEV_PORT : DEFAULT_PORT),
  );
  const requestId = `cli-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
  const authToken = await readBridgeAuthToken(flags);
  const socket = await connectBridge(port);
  try {
    return await new Promise((resolve, reject) => {
      let settled = false;
      const timeoutMs = Number(flags.timeout ?? 15_000);
      const settle = (callback, value) => {
        if (settled) {
          return;
        }
        settled = true;
        if (timeout) {
          clearTimeout(timeout);
        }
        callback(value);
      };
      const timeout =
        timeoutMs > 0
          ? setTimeout(
              () => {
                settle(reject, new Error(`Timed out waiting for Ghostex sidebar CLI result (${action}).`));
              },
              timeoutMs,
            )
          : undefined;
      const bridgeClosedError = () =>
        new Error(
          `Ghostex bridge closed before returning sidebar CLI result (${action}). This usually means the CLI token is stale because another Ghostex app instance refreshed ${BRIDGE_TOKEN_PATH} while an older instance still owns port ${port}. Quit all Ghostex copies, open one Ghostex app, then rerun the command.`,
        );
      addSocketListener(socket, "close", () => settle(reject, bridgeClosedError()), { once: true });
      addSocketListener(
        socket,
        "error",
        (error) =>
          settle(
            reject,
            new Error(
              `Ghostex bridge failed before returning sidebar CLI result (${action}): ${
                error instanceof Error ? error.message : String(error)
              }`,
            ),
          ),
        { once: true },
      );
      addSocketMessageListener(socket, (data) => {
        const event = parseJson(String(data));
        if (event?.type !== "sidebarCliResult" || event.requestId !== requestId) {
          return;
        }
        const payload = parseJson(event.payloadJson) ?? { rawPayloadJson: event.payloadJson };
        settle(resolve, { ...payload, bridgeOk: event.ok });
      });
      socket.send(
        JSON.stringify({
          action,
          authToken,
          payloadJson: JSON.stringify(payload),
          requestId,
          type: "sidebarCliCommand",
        }),
      );
    });
  } finally {
    closeSocket(socket);
  }
}

async function connectBridge(port) {
  /**
   * CDXC:CliBridgeTransport 2026-05-15-20:03:
   * Native listens on a loopback-only newline JSON TCP bridge because the
   * Network.framework WebSocket listener failed before binding on macOS, which
   * made Ctrl+G rich prompt editing fall back to inline vi. Keep the CLI API
   * shaped like the previous socket wrapper so command handlers stay focused on
   * HostCommand payloads instead of transport details.
   */
  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host: "127.0.0.1", port }, () => {
      socket.setEncoding("utf8");
      socket.send = (message) => {
        socket.write(`${message}\n`);
      };
      resolve(socket);
    });
    socket.once("error", () => {
      reject(new Error(`Could not connect to ghostex bridge on port ${port}. Is ghostex running?`));
    });
  });
}

function addSocketListener(socket, eventName, handler, options = {}) {
  if (typeof socket.addEventListener === "function") {
    socket.addEventListener(eventName, handler, options);
    return;
  }
  if (options.once && typeof socket.once === "function") {
    socket.once(eventName, handler);
    return;
  }
  socket.on(eventName, handler);
}

function addSocketMessageListener(socket, handler) {
  if (typeof socket.addEventListener === "function") {
    socket.addEventListener("message", (event) => handler(event.data));
    return;
  }
  let buffer = "";
  socket.on("data", (chunk) => {
    buffer += String(chunk);
    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex >= 0) {
      const line = buffer.slice(0, newlineIndex);
      buffer = buffer.slice(newlineIndex + 1);
      if (line) {
        handler(line);
      }
      newlineIndex = buffer.indexOf("\n");
    }
  });
}

async function floatingEditorCommand(args) {
  throw new GxserverCliUnsupportedError("floating-editor");
  const { flags, rest } = parseArgs(args);
  const commandArgs = rest.filter((arg) => arg !== "");
  if (commandArgs.length === 0) {
    throw new Error("Usage: ghostex floating-editor -- <editor> [args...]");
  }

  const port = bridgePortFromFlags(flags);
  const timeoutMs = Number(flags.timeoutMs ?? 5_000);
  const cwd = path.resolve(String(flags.cwd ?? process.cwd()));
  const requestId = `floating-editor-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;
  const workDir = await mkdtemp(path.join(tmpdir(), "ghostex-floating-editor-"));
  const statusFile = path.join(workDir, "status");
  const wrapperPath = path.join(workDir, "run.zsh");
  const debuggingMode = await readDebuggingMode();
  const logPath = debuggingMode ? floatingEditorLogPath() : "/dev/null";
  const resolvedCommandArgs = [...commandArgs];
  resolvedCommandArgs[0] = await resolveExecutable(commandArgs[0]);
  await writeFile(wrapperPath, floatingEditorWrapperScript(resolvedCommandArgs, cwd, statusFile, logPath));
  const originatingSessionId = promptEditorOriginatingSessionIdFromEnvironment();

  await appendFloatingEditorLog({
    command: resolvedCommandArgs.join(" "),
    cwd,
    event: "cli.request",
    originatingSessionId: originatingSessionId ?? "",
    port,
    requestId,
    statusFile,
  });

  let socket;
  try {
    const authToken = await readBridgeAuthToken(flags);
    socket = await connectBridge(port);
    socket.send(
      JSON.stringify({
        authToken,
        command: `/bin/zsh ${shellQuote(wrapperPath)}`,
        cwd,
        env: floatingEditorEnvironment(),
        originatingSessionId,
        requestId,
        statusFile,
        title: "gte",
        type: "openFloatingEditor",
      }),
    );
    await waitForStatus(statusFile, (status) => status.includes("started"), timeoutMs);
    const status = await waitForStatus(
      statusFile,
      (nextStatus) =>
        nextStatus.match(/^exit:(\d+)/m) ||
        nextStatus.match(/^signal:/m) ||
        nextStatus.match(/^cancelled$/m),
      Number(flags.exitTimeoutMs ?? 0),
    );
    const exitMatch = status.match(/^exit:(\d+)/m);
    if (exitMatch) {
      process.exitCode = Number(exitMatch[1]);
    } else {
      process.exitCode = 1;
    }
  } catch (error) {
    await appendFloatingEditorLog({
      error: error instanceof Error ? error.message : String(error),
      event: "cli.fallback_inline",
      requestId,
    });
    await runEditorInline(commandArgs, cwd);
  } finally {
    closeSocket(socket);
    if (flags.keepTemp !== true && flags.keepTemp !== "true") {
      await rm(workDir, { force: true, recursive: true }).catch(() => undefined);
    }
  }
}

async function promptEditorCommand(args) {
  /**
   * CDXC:PromptEditor 2026-05-31-11:58:
   * Agent Ctrl+G launches through a single-file EDITOR wrapper because prompt
   * editor callers such as zehn execute EDITOR as argv[0] and do not split
   * command strings. Runtime selection must keep macOS app terminals on Monaco
   * while Android, iOS, plain SSH, CLI, and TUI attaches use terminal-native
   * gte even when Settings selected Monaco.
   *
   * CDXC:PromptEditor 2026-06-06-16:40:
   * zmx sessions are long-lived, so shell environment can describe the client
   * that created the session instead of the client that just pressed Ctrl+G.
   * Query zmx for the current leader client's explicit prompt-editor
   * capability; missing capability means gte so SSH, mobile, and TUI attaches
   * never open a host-only Monaco popup by accident.
   */
  const { flags, rest } = parseArgs(args);
  const filePath = rest.find((arg) => arg && arg.trim() !== "");
  if (!filePath) {
    throw new Error("Usage: ghostex prompt-editor <file>");
  }

  const cwd = path.resolve(String(flags.cwd ?? process.cwd()));
  const resolvedFilePath = path.resolve(cwd, filePath);
  const backend = promptEditorBackendFromEnvironment();
  const clientCapability = await zmxPromptEditorCapability();
  const selection = selectPromptEditorCommand({
    backend,
    clientCapability,
    filePath: resolvedFilePath,
  });
  const originatingSessionId = promptEditorOriginatingSessionIdFromEnvironment();

  await appendFloatingEditorLog({
    backend,
    command: selection.commandArgs.join(" "),
    cwd,
    event: "cli.prompt_editor_select",
    globalSessionRef: process.env.GHOSTEX_GLOBAL_SESSION_REF ?? "",
    gxserverBaseUrl: process.env.GHOSTEX_GXSERVER_BASE_URL ?? "",
    macosAppClient: isMacosAppPromptEditorClient(clientCapability),
    originatingSessionId: originatingSessionId ?? "",
    promptEditorClientCapability: clientCapability ?? "",
  });

  if (selection.kind === "monaco") {
    await floatingMonacoEditorCommand(args);
    return;
  }
  await runEditorInline(selection.commandArgs, cwd);
}

function promptEditorBackendFromEnvironment() {
  const backend = String(process.env.GHOSTEX_PROMPT_EDITOR_BACKEND ?? "").trim();
  if (backend === "monaco" || backend === "gte" || backend === "custom") {
    return backend;
  }
  if (process.env.GHOSTEX_RICH_PROMPT_EDITING_WITH_GTE === "1") {
    return "gte";
  }
  return "gte";
}

async function zmxPromptEditorCapability() {
  if (!String(process.env.ZMX_SESSION ?? "").trim()) {
    return undefined;
  }
  /**
   * CDXC:PromptEditor 2026-06-07-08:09:
   * zmx-backed Ctrl+G routing must query the same app/gxserver-provided zmx
   * binary that created or attached the session. PATH can contain a stale
   * Homebrew zmx without prompt-editor-capability, so zmx sessions without an
   * explicit GHOSTEX_ZMX_BIN stay terminal-native instead of probing PATH.
   */
  const zmxCommand = String(process.env.GHOSTEX_ZMX_BIN ?? "").trim();
  if (!zmxCommand) {
    return "gte";
  }
  try {
    const result = await execFileAsync(zmxCommand, ["prompt-editor-capability"], {
      env: process.env,
      timeout: 750,
    });
    const capability = String(result.stdout ?? result[0] ?? "").trim();
    if (capability === "monaco" || capability === "gte") {
      return capability;
    }
  } catch {
    return "gte";
  }
  return "gte";
}

function isMacosAppPromptEditorClient(clientCapability) {
  if (clientCapability) {
    return clientCapability === "monaco";
  }
  return process.env.GHOSTEX_PROMPT_EDITOR_CLIENT === "macos-app";
}

function selectPromptEditorCommand({ backend, clientCapability, filePath }) {
  if (backend === "custom") {
    const customCommand = String(process.env.GHOSTEX_CUSTOM_PROMPT_EDITOR_COMMAND ?? "").trim() || "code --wait";
    return {
      commandArgs: ["/bin/zsh", "-lc", `exec ${customCommand} "$@"`, "ghostex-prompt-editor", filePath],
      kind: "custom",
    };
  }
  if (backend === "monaco" && isMacosAppPromptEditorClient(clientCapability)) {
    return { commandArgs: ["ghostex", "floating-monaco-editor", filePath], kind: "monaco" };
  }
  return { commandArgs: ["gte", filePath], kind: "gte" };
}

function promptEditorOriginatingSessionIdFromEnvironment() {
  /*
   * CDXC:PromptEditor 2026-06-09-21:50:
   * zmx prompt-editor shells can inherit stale GHOSTEX_NATIVE_SESSION_ID from
   * the app or gxserver launch environment. The current gxserver S:P:G ref is
   * refreshed per session, so derive the native P:G focus id from it before
   * falling back to the legacy native env key for older direct terminals.
   */
  return nativeFocusSessionIdFromGlobalSessionRef(process.env.GHOSTEX_GLOBAL_SESSION_REF)
    ?? normalizedEnvironmentString(process.env.GHOSTEX_NATIVE_SESSION_ID);
}

function nativeFocusSessionIdFromGlobalSessionRef(globalSessionRef) {
  const parts = String(globalSessionRef ?? "").trim().split(":");
  if (
    parts.length === 3 &&
    /^S[0-9][a-z0-9]$/u.test(parts[0]) &&
    /^P[0-9][a-z0-9]{3}$/u.test(parts[1]) &&
    /^G[0-9][a-z0-9]{3}$/u.test(parts[2])
  ) {
    return `${parts[1]}:${parts[2]}`;
  }
  return undefined;
}

function normalizedEnvironmentString(value) {
  const trimmed = String(value ?? "").trim();
  return trimmed || undefined;
}

async function floatingMonacoEditorCommand(args) {
  /**
   * CDXC:PromptEditor 2026-05-31-10:24:
   * Monaco prompt editing is still rendered by the running macOS app, not
   * gxserver. Keep this EDITOR-facing command on the native bridge until
   * gxserver owns an equivalent blocking save/cancel endpoint, otherwise Ctrl+G
   * prompt editing exits before the floating editor can open.
   */
  const { flags, rest } = parseArgs(args);
  const filePath = rest.find((arg) => arg && arg.trim() !== "");
  if (!filePath) {
    throw new Error("Usage: ghostex floating-monaco-editor <file>");
  }

  const port = bridgePortFromFlags(flags);
  const timeoutMs = Number(flags.timeoutMs ?? 5_000);
  const cwd = path.resolve(String(flags.cwd ?? process.cwd()));
  const requestId = `floating-monaco-editor-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;
  const workDir = await mkdtemp(path.join(tmpdir(), "ghostex-floating-monaco-editor-"));
  const statusFile = path.join(workDir, "status");
  const resolvedFilePath = path.resolve(cwd, filePath);
  const originatingSessionId = promptEditorOriginatingSessionIdFromEnvironment();

  await appendFloatingEditorLog({
    cwd,
    event: "cli.monaco_request",
    filePath: resolvedFilePath,
    originatingSessionId: originatingSessionId ?? "",
    port,
    requestId,
    statusFile,
  });

  let socket;
  try {
    const authToken = await readBridgeAuthToken(flags);
    socket = await connectBridge(port);
    socket.send(
      JSON.stringify({
        authToken,
        cwd,
        editorKind: "monaco",
        filePath: resolvedFilePath,
        language: "markdown",
        originatingSessionId,
        requestId,
        statusFile,
        title: "Prompt Editor",
        type: "openFloatingEditor",
      }),
    );
    const status = await waitForStatus(
      statusFile,
      (nextStatus) => nextStatus.match(/^saved$/m) || nextStatus.match(/^cancelled$/m),
      Number(flags.exitTimeoutMs ?? 0),
    );
    process.exitCode = status.match(/^saved$/m) ? 0 : 1;
  } catch (error) {
    await appendFloatingEditorLog({
      error: error instanceof Error ? error.message : String(error),
      event: "cli.monaco_fallback_inline",
      filePath: resolvedFilePath,
      requestId,
    });
    await runEditorInline(["vi", resolvedFilePath], cwd);
  } finally {
    closeSocket(socket);
    if (flags.keepTemp !== true && flags.keepTemp !== "true") {
      await rm(workDir, { force: true, recursive: true }).catch(() => undefined);
    }
  }
}

function closeSocket(socket) {
  if (!socket) {
    return;
  }
  if (typeof socket.destroy === "function") {
    socket.destroy();
    return;
  }
  if (typeof socket.terminate === "function") {
    socket.terminate();
    return;
  }
  socket.close();
}

function bridgePortFromFlags(flags) {
  return Number(
    flags.port ??
      process.env.GHOSTEX_CLI_PORT ??
      (process.env.GHOSTEX_APP_VARIANT === "dev" ? DEV_PORT : DEFAULT_PORT),
  );
}

async function readBridgeAuthToken(flags = {}) {
  /**
   * CDXC:CliBridgeSecurity 2026-05-15-18:25
   * The native host rejects unauthenticated loopback WebSocket commands because
   * arbitrary browser pages can also connect to 127.0.0.1. CLI commands read the
   * per-launch token that the app writes under ~/.ghostex[-dev]/cli.
   */
  const explicitToken = String(flags.token ?? flags.bridgeToken ?? process.env.GHOSTEX_BRIDGE_TOKEN ?? "")
    .trim();
  if (explicitToken) {
    return explicitToken;
  }
  const token = (await readFile(BRIDGE_TOKEN_PATH, "utf8").catch(() => "")).trim();
  if (!token) {
    throw new Error(
      `Could not read Ghostex bridge token at ${BRIDGE_TOKEN_PATH}. Is Ghostex running?`,
    );
  }
  return token;
}

function floatingEditorEnvironment() {
  const environment = {
    HOME: process.env.HOME ?? homedir(),
    LANG: process.env.LANG ?? "en_US.UTF-8",
    PATH: process.env.PATH ?? "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
    SHELL: process.env.SHELL ?? "/bin/zsh",
    TERM: process.env.TERM ?? "xterm-256color",
    USER: process.env.USER ?? "",
    GHOSTEX_FLOATING_EDITOR: "1",
  };
  if (process.env.GHOSTEX_APP_VARIANT) {
    environment.GHOSTEX_APP_VARIANT = process.env.GHOSTEX_APP_VARIANT;
  }
  return environment;
}

function floatingEditorWrapperScript(commandArgs, cwd, statusFile, logPath) {
  const command = commandArgs.map(shellQuote).join(" ");
  return `#!/bin/zsh
set +e
mkdir -p ${shellQuote(path.dirname(statusFile))} ${shellQuote(path.dirname(logPath))} 2>/dev/null
printf 'started\\n' > ${shellQuote(statusFile)}
{
  printf '[%s] child.start cwd=%s command=%s\\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" ${shellQuote(cwd)} ${shellQuote(command)}
} >> ${shellQuote(logPath)} 2>/dev/null
cd ${shellQuote(cwd)} || {
  _ghostex_status=$?
  printf 'exit:%s\\n' "$_ghostex_status" >> ${shellQuote(statusFile)}
  exit "$_ghostex_status"
}
${command}
_ghostex_status=$?
{
  printf '[%s] child.exit status=%s\\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$_ghostex_status"
} >> ${shellQuote(logPath)} 2>/dev/null
printf 'exit:%s\\n' "$_ghostex_status" >> ${shellQuote(statusFile)}
exit "$_ghostex_status"
`;
}

async function resolveExecutable(command) {
  if (command.includes("/")) {
    return command;
  }
  const { stdout } = await execFileAsync("/bin/zsh", ["-lc", `command -v -- ${shellQuote(command)}`]);
  return stdout.trim().split(/\r?\n/)[0] || command;
}

async function waitForStatus(statusFile, predicate, timeoutMs) {
  const startedAt = Date.now();
  while (true) {
    const status = await readFile(statusFile, "utf8").catch(() => "");
    if (predicate(status)) {
      return status;
    }
    if (timeoutMs > 0 && Date.now() - startedAt > timeoutMs) {
      throw new Error(`Timed out waiting for floating editor status at ${statusFile}.`);
    }
    await sleep(100);
  }
}

async function runEditorInline(commandArgs, cwd) {
  await new Promise((resolve, reject) => {
    const child = spawn(commandArgs[0], commandArgs.slice(1), {
      cwd,
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
        return;
      }
      process.exitCode = code ?? 0;
      resolve();
    });
  });
}

function floatingEditorLogPath() {
  return path.join(homedir(), "Library", "Logs", "ghostex", "floating-editor.log");
}

async function appendFloatingEditorLog(details) {
  /**
   * CDXC:Diagnostics 2026-05-16-07:23:
   * Floating-editor CLI breadcrumbs are regular app diagnostics written to a
   * persistent log file. Honor the shared Settings Debugging Mode switch before
   * creating or appending ~/Library/Logs/ghostex/floating-editor.log.
   */
  if (!(await readDebuggingMode())) {
    return;
  }
  const logPath = floatingEditorLogPath();
  await mkdir(path.dirname(logPath), { recursive: true });
  await appendFile(
    logPath,
    `${JSON.stringify({
      ...details,
      source: "ghostex-cli",
      timestamp: new Date().toISOString(),
    })}\n`,
  );
}

async function readDebuggingMode() {
  const settingsJson = await readFile(SHARED_SETTINGS_PATH, "utf8").catch(() => "");
  if (!settingsJson) {
    return false;
  }
  try {
    return JSON.parse(settingsJson)?.debuggingMode === true;
  } catch {
    return false;
  }
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, "'\\''")}'`;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function screenshotCommand(args) {
  const { flags, rest } = parseArgs(args);
  const output = path.resolve(rest[0] ?? path.join(CLI_DIR, `screenshot-${timestampSlug()}.png`));
  await captureScreenshot(output, flags);
  printJson({ ok: true, output });
}

async function logsCommand(args) {
  const { flags } = parseArgs(args);
  const lines = Number(flags.lines ?? 200);
  try {
    const result = await callGxserverRpc(
      "/api/queryLogs",
      compactObject({
        event: flags.event,
        eventPrefix: flags.eventPrefix,
        level: flags.level,
        limit: lines,
        order: flags.order,
        reverse: flags.reverse === undefined ? undefined : parseBoolean(flags.reverse),
        since: flags.since,
        until: flags.until,
      }),
      flags,
    );
    const entries = Array.isArray(result.entries) ? result.entries : [];
    const filtered = flags.grep
      ? entries.filter((entry) => JSON.stringify(entry).includes(String(flags.grep)))
      : entries;
    if (flags.json) {
      printJson({ ...result, entries: filtered, ok: true, source: "gxserver-api" });
      return;
    }
    for (const entry of filtered) {
      console.log(JSON.stringify(entry));
    }
    return;
  } catch (error) {
    if (!(error instanceof GxserverCliConnectionError)) {
      throw error;
    }
  }

  const file = String(flags.file ?? path.basename(GXSERVER_LOG_PATH));
  const logPath = path.isAbsolute(file)
    ? file
    : file === path.basename(GXSERVER_LOG_PATH)
      ? GXSERVER_LOG_PATH
      : path.join(LOG_DIR, file);
  const text = await readFile(logPath, "utf8").catch((error) => {
    throw new Error(`Could not read ${logPath}: ${error.message}`);
  });
  const filtered = filterLogLines(text, flags).slice(-lines);
  if (flags.json) {
    printJson({ file: logPath, lines: filtered, ok: true, source: "local-file" });
    return;
  }
  console.log(filtered.join("\n"));
}

async function bundleCommand(args) {
  const { flags, rest } = parseArgs(args);
  const outputDir = path.resolve(rest[0] ?? path.join(CLI_DIR, `bundle-${timestampSlug()}`));
  await mkdir(outputDir, { recursive: true });
  const state = await sendSidebarCliCommand("state", {}, flags);
  const screenshot = path.join(outputDir, "screenshot.png");
  await captureScreenshot(screenshot, flags);
  const logs = await collectLogs(Number(flags.lines ?? 500));
  await writeFile(path.join(outputDir, "state.json"), JSON.stringify(state, null, 2));
  await writeFile(path.join(outputDir, "logs.json"), JSON.stringify(logs, null, 2));
  printJson({ logs: path.join(outputDir, "logs.json"), ok: true, outputDir, screenshot });
}

async function captureScreenshot(output, flags = {}) {
  await mkdir(path.dirname(output), { recursive: true });
  if (flags.activate !== "false") {
    await execFileAsync("osascript", ["-e", 'tell application "Ghostex" to activate']).catch(
      () => undefined,
    );
  }
  await execFileAsync("screencapture", ["-x", output]);
}

async function collectLogs(lines) {
  const entries = await readdir(LOG_DIR).catch(() => []);
  const result = {};
  for (const file of entries.filter((entry) => entry.endsWith(".log"))) {
    const text = await readFile(path.join(LOG_DIR, file), "utf8").catch(() => "");
    result[file] = text.split(/\r?\n/).filter(Boolean).slice(-lines);
  }
  return result;
}

async function sessionsCommand(args) {
  const { flags } = parseArgs(args);
  const result = await fetchSessionList(flags, { writeCache: true });
  if (flags.json) {
    printJson(result);
    return;
  }
  printSessionList(result.sessions ?? [], {
    grouped: flags.ungrouped !== true && flags.u !== true,
  });
}

async function androidCheckCommand(args) {
  const { flags } = parseArgs(args);
  const result = await runAndroidReadinessCheck(flags);
  /**
   * CDXC:AndroidConnectionManagement 2026-05-17-18:20:
   * Ghostex Android needs one Mac-side readiness contract instead of inferring release support from generic session listing.
   *
   * CDXC:AndroidConnectionManagement 2026-05-30-15:15:
   * The hard cutover readiness check uses authenticated gxserver health and
   * inventory APIs. zmx availability comes from gxserver tool capabilities, and
   * the macOS app bridge is not a readiness fallback.
   */
  if (flags.json) {
    printJson(result);
  } else if (result.ok) {
    console.log(`Ghostex Android ready: ${result.sessions} sessions, persistence ${result.sessionPersistenceProvider}.`);
  } else {
    console.error(result.error);
  }
  if (!result.ok) {
    process.exitCode = 1;
  }
}

async function runAndroidReadinessCheck(flags = {}) {
  const target = await resolveGxserverServerTarget(flags).catch((error) => ({
    error: error instanceof Error ? error.message : String(error),
    ok: false,
  }));
  if (target.ok === false) {
    return target;
  }
  const health = await fetchGxserverHealth(target).catch(() => undefined);
  if (!health) {
    return {
      error: `Could not load gxserver health from ${target.baseUrl}. Start gxserver and try again.`,
      ok: false,
    };
  }
  const zmxTool = Array.isArray(health.tools) ? health.tools.find((tool) => tool?.tool === "zmx") : undefined;
  if (zmxTool?.availability !== "available") {
    return {
      error: zmxTool?.message ?? "gxserver zmx capability is unavailable.",
      ok: false,
      serverId: health.serverId,
    };
  }

  const result = await sendSidebarCliCommand("listSessions", {}, flags).catch((error) => ({
    error: error instanceof Error ? error.message : String(error),
    ok: false,
  }));
  if (isFailedCliResult(result)) {
    return {
      error: result.error ?? "Could not load Ghostex sessions from gxserver.",
      ok: false,
      serverId: health.serverId,
      zmxPath: zmxTool.executablePath,
    };
  }

  return {
    ok: true,
    serverId: health.serverId,
    sessions: Array.isArray(result.sessions) ? result.sessions.length : 0,
    zmxPath: zmxTool.executablePath,
  };
}

async function readAndroidReadinessSettings(settingsPath = SHARED_SETTINGS_PATH) {
  const text = await readFile(settingsPath, "utf8").catch(() => "");
  if (!text.trim()) {
    return {
      error: `Ghostex settings were not found at ${settingsPath}. Start Ghostex, set Session persistence to zmx, and try again.`,
      ok: false,
    };
  }
  const settings = parseJson(text);
  if (!settings || typeof settings !== "object") {
    return {
      error: `Ghostex settings at ${settingsPath} are not valid JSON. Open Ghostex settings, save Session persistence as zmx, and try again.`,
      ok: false,
    };
  }
  /**
   * CDXC:AndroidConnectionManagement 2026-05-17-20:39:
   * Android supports zmx only for this release, but readiness should not depend
   * on presentation casing or accidental surrounding whitespace in the shared
   * settings JSON. Normalize the provider token before enforcing the contract.
   */
  const provider = settings.sessionPersistenceProvider;
  const normalizedProvider = String(provider ?? "").trim().toLowerCase();
  if (normalizedProvider !== "zmx") {
    return {
      error: `Ghostex session persistence is set to ${provider || "off"}. Open Ghostex Settings and set Session persistence to zmx before connecting from Android.`,
      ok: false,
      sessionPersistenceProvider: provider || "off",
    };
  }
  return { ok: true, sessionPersistenceProvider: normalizedProvider };
}

async function resolveCommandPath(command) {
  const { stdout } = await execFileAsync("/bin/zsh", ["-lc", `command -v -- ${shellQuote(command)}`]).catch(() => ({
    stdout: "",
  }));
  return stdout.trim().split(/\r?\n/)[0] || "";
}

async function attachSessionCommand(args) {
  const { flags, rest } = parseArgs(args);
  /**
   * CDXC:AndroidRemoteSessions 2026-05-17-13:59:
   * Ghostex Android attaches by stable session id and should use the same
   * documented `--session-id` form as focus, wake, sleep, kill, and rename.
   * Keep positional selectors for human usage.
   *
   * CDXC:CliSessionPicker 2026-05-25-16:05:
   * `ghostex attach`, `gx attach`, and `gx a` without a selector should open
   * the lightweight attach picker, not the full TUI. Bare `gx` owns the full
   * TUI experience while empty attach keeps the fast single-session picker.
   */
  const selector = flags.sessionId ?? rest.join(" ").trim();
  if (!selector) {
    await interactiveSessionPickerCommand(args);
    return;
  }
  const result = await fetchSessionList(flags);
  const session = await resolveOneListedSession(selector, result.sessions ?? [], flags);
  await attachResolvedSession(session, flags);
}

async function attachResolvedSession(session, flags = {}) {
  /**
   * CDXC:CliSessions 2026-05-17-01:33:
   * Sleeping a provider-backed agent session stops the tmux/zmx/zellij runtime to
   * release the agent CLI memory. External attach should therefore prefer the
   * agent resume command for sleeping rows; provider attach remains first for
   * awake rows where the named session is still live.
   */
  let attachMetadata = await fetchAttachMetadataForSession(session, flags);
  attachMetadata = await startMissingProviderForCliAttach(session, attachMetadata, flags);
  const attachableSession = applyAttachMetadataToCliSession(session, attachMetadata);
  const command = buildSessionAttachCommand(attachableSession);
  if (!command) {
    throw new Error(
      `Session ${session.alias} has no provider attach command or supported agent resume command.`,
    );
  }
  await runInteractiveShellCommand(command, attachableSession.projectPath);
}

async function interactiveSessionPickerCommand(args) {
  const { flags } = parseArgs(args);
  const result = await fetchSessionList(flags, { writeCache: true });
  const sessions = result.sessions ?? [];
  if (sessions.length === 0) {
    console.log("No running terminal sessions.");
    return;
  }
  if (!isInteractiveTerminal()) {
    printSessionPickerRows(sessions);
    return;
  }
  const session = await runInteractiveSessionPicker(sessions);
  if (!session) {
    return;
  }
  await attachResolvedSession(session, flags);
}

async function ghostexTuiCommand(args) {
  const { flags } = parseArgs(args);
  /**
   * CDXC:GhostexTui 2026-05-24-19:18:
   * Bare `ghostex` / `gx` launches the full Ghostex terminal TUI. Direct
   * attach commands such as `gx a <session>` stay on the Node attach path so
   * scripts and muscle-memory single-session attaches keep their old behavior.
   *
   * CDXC:GhostexTui 2026-05-30-15:15:
   * The TUI still calls back into this CLI for inventory and attach, but those
   * callbacks now route to gxserver so the terminal UI no longer depends on the
   * macOS app bridge.
   */
  if (!isInteractiveTerminal()) {
    await interactiveSessionPickerCommand(args);
    return;
  }
  const tui = resolveGhostexTuiLaunch(flags);
  /**
   * CDXC:GhostexTui 2026-05-25-15:11:
   * The bare `gx` launcher must pass TUI environment through `spawn({ env })`.
   * Putting these keys at the top-level options object makes the app build try
   * the full upstream Herdr/Ghostty path and drops the callback command the TUI
   * uses to list and attach Ghostex sessions.
   */
  await runInteractiveProcess(tui.command, tui.args, {
    env: {
      ...process.env,
      ...tui.env,
      GHOSTEX_TUI_CLI_COMMAND: `${shellQuote(process.execPath)} ${shellQuote(fileURLToPath(import.meta.url))}`,
    },
  });
}

async function zehnSearchCommand(args) {
  const launch = resolveZehnLaunch();
  const zehnArgs = await resolveZehnSearchArgs(args);
  /**
   * CDXC:AgentHistorySearch 2026-05-29-12:27:
   * `gx find` and `gx f` should show the pinned zehn CLI for
   * cross-agent prompt history search. Keep `gx s` on the existing sessions
   * command because that alias was already part of the public Ghostex CLI.
   * Forward zehn flags untouched so modes such as --print, --project, --list,
   * --version, and --help remain owned by zehn rather than Ghostex parsing.
   *
   * CDXC:AgentHistorySearch 2026-06-04-23:31:
   * `gx find` is the Ghostex-owned launcher for zehn, so it should honor the
   * gxserver global Accept All setting by passing zehn's explicit
   * `--accept-all` resume flag. Standalone zehn remains independent, and
   * user-provided `--accept-all` or `--no-accept-all` wins over daemon state.
   */
  await runInteractiveProcess(launch.command, [...launch.args, ...zehnArgs], {
    cwd: launch.cwd,
    env: launch.env,
  });
}

async function resolveZehnSearchArgs(args) {
  if (hasZehnAcceptAllOverride(args)) {
    return args;
  }
  const result = await callGxserverRpc("/api/readAgentSettings", {}).catch(() => undefined);
  return applyZehnAcceptAllArgs(args, result?.settings?.agentAcceptAllEnabled === true);
}

function applyZehnAcceptAllArgs(args, acceptAllEnabled) {
  if (hasZehnAcceptAllOverride(args) || acceptAllEnabled !== true) {
    return args;
  }
  return ["--accept-all", ...args];
}

function hasZehnAcceptAllOverride(args) {
  return args.some((arg) => arg === "--accept-all" || arg === "--no-accept-all");
}

function resolveZehnLaunch() {
  const explicitBin = String(process.env.GHOSTEX_ZEHN_BIN ?? "").trim();
  if (explicitBin) {
    return { args: [], command: explicitBin, cwd: undefined, env: process.env };
  }

  const cliDir = path.dirname(fileURLToPath(import.meta.url));
  for (const bundledRoot of ghostexBundledWebResourceRoots(cliDir)) {
    const bundledBin = path.resolve(bundledRoot, "bin", "zehn");
    if (fileExistsSync(bundledBin)) {
      return { args: [], command: bundledBin, cwd: undefined, env: process.env };
    }
  }

  const repoRoot = path.resolve(cliDir, "..");
  const roots = uniquePaths([
    repoRoot,
    process.env.GHOSTEX_SOURCE_ROOT,
    findGhostexSourceRoot(process.cwd()),
  ]);
  for (const root of roots) {
    const launch = resolveZehnLaunchFromRoot(root);
    if (launch) {
      return launch;
    }
  }

  throw new Error(
    "Bundled zehn was not found. Initialize the submodule with `git submodule update --init zehn`, build it with Zig 0.16+, or set GHOSTEX_ZEHN_BIN to a reviewed zehn binary.",
  );
}

function resolveZehnLaunchFromRoot(root) {
  if (!root) {
    return undefined;
  }
  const bin = path.join(root, "zehn", "zig-out", "bin", "zehn");
  if (fileExistsSync(bin)) {
    return { args: [], command: bin, cwd: undefined, env: process.env };
  }
  const manifestPath = path.join(root, "zehn", "build.zig");
  if (!fileExistsSync(manifestPath)) {
    return undefined;
  }
  const zigBin = String(process.env.GHOSTEX_ZEHN_ZIG ?? process.env.ZEHN_ZIG ?? "zig").trim();
  return {
    args: ["build", "run", "--"],
    command: zigBin || "zig",
    cwd: path.join(root, "zehn"),
    env: process.env,
  };
}

function resolveGxserverCliLaunch() {
  const explicitCli = String(
    process.env.GHOSTEX_GXSERVER_CLI ?? process.env.GHOSTEX_GXSERVER_BIN ?? "",
  ).trim();
  if (explicitCli) {
    return resolveGxserverCliLaunchForPath(explicitCli);
  }

  const cliDir = path.dirname(fileURLToPath(import.meta.url));
  const roots = uniquePaths([
    ...ghostexBundledWebResourceRoots(cliDir),
    path.resolve(cliDir, ".."),
    process.env.GHOSTEX_SOURCE_ROOT,
    findGhostexSourceRoot(process.cwd()),
  ]);
  for (const root of roots) {
    const cliPath = path.join(root, "gxserver", "dist", "src", "cli.js");
    if (fileExistsSync(cliPath)) {
      return resolveGxserverCliLaunchForPath(cliPath);
    }
  }

  throw new Error(
    "gxserver CLI build output is missing. Run `npm run build` in gxserver/ for development, or reinstall Ghostex so gxserver/dist/src/cli.js is present.",
  );
}

function resolveGxserverCliLaunchForPath(cliPath) {
  const resolvedPath = path.resolve(cliPath);
  if (!fileExistsSync(resolvedPath)) {
    throw new Error(`gxserver CLI path does not exist: ${resolvedPath}`);
  }
  return path.extname(resolvedPath) === ".js"
    ? { args: [resolvedPath], command: process.execPath, cwd: undefined, env: process.env }
    : { args: [], command: resolvedPath, cwd: undefined, env: process.env };
}

function resolveGhostexTuiLaunch(flags = {}) {
  const explicitBin = String(flags.tuiBin ?? process.env.GHOSTEX_TUI_BIN ?? "").trim();
  if (explicitBin) {
    return { args: [], command: explicitBin, env: {} };
  }
  const cliDir = path.dirname(fileURLToPath(import.meta.url));
  const repoRoot = path.resolve(cliDir, "..");
  /**
   * CDXC:GhostexTui 2026-05-25-15:11:
   * Installed Homebrew/app CLIs run from the application resource directory,
   * while local development runs from the source checkout. Probe both the CLI
   * bundle root and the current checkout root so bare `gx` can find the TUI
   * binary or Cargo manifest instead of emitting cargo errors for a missing
   * bundled `tui/` directory.
   *
   * CDXC:GhostexTui 2026-06-07-12:13:
   * Installed `gx` must open the TUI without requiring the user to run it from
   * a source checkout. Treat the app resource root as a packaged runtime root
   * and launch Web/bin/ghostex-tui before considering source-only Cargo paths.
   *
   * CDXC:CliInstall 2026-06-07-13:53:
   * DMG and Homebrew command links now target Contents/Resources/CLI while helper
   * binaries remain in Contents/Resources/Web/bin. Probe the sibling Web resource
   * root before source roots so the installed CLI keeps using bundled tools.
   */
  const roots = uniquePaths([
    ...ghostexBundledWebResourceRoots(cliDir),
    repoRoot,
    process.env.GHOSTEX_SOURCE_ROOT,
    findGhostexSourceRoot(process.cwd()),
  ]);
  for (const root of roots) {
    const launch = resolveGhostexTuiLaunchFromRoot(root);
    if (launch) {
      return launch;
    }
  }
  throw new Error(
    "Ghostex TUI binary was not found. Build the TUI with `ZIG=/opt/homebrew/opt/zig@0.15/bin/zig cargo build --bin ghostex-tui --manifest-path tui/Cargo.toml`, pass `--tui-bin <path>`, or set GHOSTEX_TUI_BIN.",
  );
}

function resolveBundledBeadsLaunch() {
  const cliDir = path.dirname(fileURLToPath(import.meta.url));
  const repoRoot = path.resolve(cliDir, "..");
  /**
   * CDXC:ProjectBoardBeads 2026-06-10-09:31:
   * `gx bd` is the supported agent/user shell boundary for Beads operations.
   * Resolve only Ghostex-bundled or source-staged Beads binaries so Project/Kanban, agent prompts, and manual commands cannot split state across different shell-installed `bd` versions.
   */
  const roots = uniquePaths([
    ...ghostexBundledWebResourceRoots(cliDir),
    repoRoot,
    process.env.GHOSTEX_SOURCE_ROOT,
    findGhostexSourceRoot(process.cwd()),
  ]);
  for (const root of roots) {
    const launch = resolveBundledBeadsLaunchFromRoot(root);
    if (launch) {
      return launch;
    }
  }
  throw new Error(
    "Bundled bd was not found. Rebuild or reinstall Ghostex so Web/bin/bd is staged; shell-installed bd is intentionally ignored.",
  );
}

function resolveBundledBeadsLaunchFromRoot(root) {
  if (!root) {
    return undefined;
  }
  for (const candidate of [
    path.join(root, "bin", "bd"),
    path.join(root, "native", "macos", "ghostexHost", "Web", "bin", "bd"),
  ]) {
    if (fileExistsSync(candidate)) {
      return { args: [], command: candidate, env: process.env };
    }
  }
  return undefined;
}

function ghostexBundledWebResourceRoots(cliDir) {
  /**
   * CDXC:CliInstall 2026-06-07-13:53:
   * Installed app CLIs moved from Web/cli to CLI, but zmx/zehn/gxserver/TUI
   * runtime assets still live under Web. Check both the new sibling Web folder
   * and the legacy parent layout so old dev bundles and new release bundles
   * resolve app-owned tools without PATH fallbacks.
   */
  return uniquePaths([
    path.resolve(cliDir, "..", "Web"),
    path.resolve(cliDir, ".."),
  ]);
}

function resolveGhostexTuiLaunchFromRoot(root) {
  if (!root) {
    return undefined;
  }
  const bundledBin = path.join(root, "bin", "ghostex-tui");
  if (fileExistsSync(bundledBin)) {
    return { args: [], command: bundledBin, env: {} };
  }
  const debugBin = path.join(root, "tui", "target", "debug", "ghostex-tui");
  const releaseBin = path.join(root, "tui", "target", "release", "ghostex-tui");
  if (fileExistsSync(releaseBin)) {
    return { args: [], command: releaseBin, env: {} };
  }
  if (fileExistsSync(debugBin)) {
    return { args: [], command: debugBin, env: {} };
  }
  const manifestPath = path.join(root, "tui", "Cargo.toml");
  if (!fileExistsSync(manifestPath)) {
    return undefined;
  }
  return {
    args: ["run", "--quiet", "--bin", "ghostex-tui", "--manifest-path", manifestPath],
    command: "cargo",
    env: ghostexTuiCargoEnv(),
  };
}

function ghostexTuiCargoEnv() {
  /**
   * CDXC:GhostexTui 2026-05-26-11:06:
   * Bare `gx` now needs Herdr's Ghostty-backed runtime, so Cargo fallback
   * builds must not use the earlier `GHOSTEX_TUI_LIGHT=1` vt100-only path.
   * On macOS 26.4+, unpatched Zig 0.15.2 cannot link libc from Xcode 26 SDKs;
   * prefer Homebrew's patched `zig@0.15` keg when it exists so first-run
   * fallback builds can produce the real Ghostty terminal backend.
   */
  const env = {};
  const patchedHomebrewZig = "/opt/homebrew/opt/zig@0.15/bin/zig";
  if (fileExistsSync(patchedHomebrewZig)) {
    env.ZIG = patchedHomebrewZig;
  }
  return env;
}

function findGhostexSourceRoot(startPath) {
  let current = path.resolve(startPath || process.cwd());
  while (true) {
    if (fileExistsSync(path.join(current, "scripts", "ghostex-cli.mjs")) && fileExistsSync(path.join(current, "tui", "Cargo.toml"))) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return undefined;
    }
    current = parent;
  }
}

function ghostexAgentSkillInstallDir(skillName) {
  return path.join(GHOSTEX_AGENT_SKILL_INSTALL_ROOT, skillName);
}

function resolveGhostexAgentSkillSourceDir(skillName, envVars = []) {
  const cliDir = path.dirname(fileURLToPath(import.meta.url));
  const explicitSources = envVars.map((envVar) => stringFlag(process.env[envVar]));
  const sourceRoot = findGhostexSourceRoot(process.cwd());
  const candidates = uniquePaths([
    ...explicitSources,
    path.join(cliDir, "skills", skillName),
    sourceRoot && path.join(sourceRoot, "scripts", "skills", skillName),
    path.join(path.resolve(cliDir, ".."), ".agents", "skills", skillName),
    sourceRoot && path.join(sourceRoot, ".agents", "skills", skillName),
  ]);
  for (const candidate of candidates) {
    if (fileExistsSync(path.join(candidate, "SKILL.md"))) {
      return candidate;
    }
  }
  throw new Error(
    `Could not find ${skillName}. Reinstall Ghostex or set ${envVars[0] ?? "GHOSTEX_SKILL_SOURCE"} to the skill directory.`,
  );
}

function uniquePaths(paths) {
  const seen = new Set();
  const unique = [];
  for (const candidate of paths) {
    if (!candidate) {
      continue;
    }
    const normalized = path.resolve(String(candidate));
    if (!seen.has(normalized)) {
      seen.add(normalized);
      unique.push(normalized);
    }
  }
  return unique;
}

function fileExistsSync(filePath) {
  try {
    realpathSync(filePath);
    return true;
  } catch {
    return false;
  }
}

function buildSessionAttachCommand(session) {
  if (shouldCreateMissingZmxSessionWithResume(session)) {
    return buildZmxAttachOrResumeCommand(session);
  }
  if (isZmxSession(session) && session.status !== "sleep" && session.attachCommand) {
    /**
     * CDXC:CliSessions 2026-05-26-11:29:
     * Live zmx attaches should use the stored provider attach command for every
     * client, including GTX TUI and mobile, so reattach restores the full zmx
     * terminal state and scrollback instead of only the current viewport.
     */
    return session.attachCommand;
  }
  return session.status === "sleep"
    ? session.resumeCommand || session.attachCommand
    : session.attachCommand || session.resumeCommand;
}

function isZmxSession(session) {
  return String(session.provider ?? "").trim().toLowerCase() === "zmx";
}

function shouldCreateMissingZmxSessionWithResume(session) {
  return (
    isZmxSession(session) &&
    Boolean(String(session.providerSessionName ?? "").trim()) &&
    Boolean(String(session.resumeCommand ?? "").trim())
  );
}

function buildZmxAttachOrResumeCommand(session) {
  const sessionName = String(session.providerSessionName).trim();
  const resumeCommand = String(session.resumeCommand).trim();
  const resumeFallbackCommand = String(session.resumeFallbackCommand ?? "").trim();
  const cwd = String(session.projectPath || ".").trim() || ".";
  const script = `
zmx_session=${shellQuote(sessionName)}
zmx_resume_command=${shellQuote(resumeCommand)}
zmx_resume_fallback_command=${shellQuote(resumeFallbackCommand)}
zmx_cwd=${shellQuote(cwd)}
export zmx_resume_command zmx_resume_fallback_command
unset ZMX_SESSION ZMX_SESSION_PREFIX
if ! command -v zmx >/dev/null 2>&1; then
  printf '%s\\n' 'zmx was not found on PATH.'
  exit 127
fi
if zmx list --short 2>/dev/null | grep -F -x -- "$zmx_session" >/dev/null 2>&1; then
  exec zmx attach "$zmx_session"
fi
cd "$zmx_cwd" || exit
zmx_resume_launcher='
set +e
/bin/zsh -lc "$zmx_resume_command"
zmx_resume_status=$?
if [ "$zmx_resume_status" -ne 0 ] && [ -n "$zmx_resume_fallback_command" ] && [ "$zmx_resume_fallback_command" != "$zmx_resume_command" ]; then
  printf '"'"'%s\\n'"'"' "Exact resume failed; trying saved fallback resume command."
  /bin/zsh -lc "$zmx_resume_fallback_command"
  zmx_resume_status=$?
fi
if [ "$zmx_resume_status" -ne 0 ]; then
  printf '"'"'\\n%s\\n'"'"' "Resume command exited with status $zmx_resume_status. Leaving this pane open for inspection."
  exec "\${SHELL:-/bin/zsh}" -l
fi
exit 0
'
exec zmx attach "$zmx_session" /bin/zsh -lc "$zmx_resume_launcher"
`;
  /**
   * CDXC:AndroidRemoteSessions 2026-05-21-07:21:
   * Mobile sidebar taps run through `ghostex attach --session-id`. If the card's
   * zmx session is gone but the agent has a resume command, recreate the named
   * zmx session and run the agent resume command there instead of opening an
   * attach terminal that immediately exits or becomes an empty shell.
   */
  return `/bin/zsh -lc ${shellQuote(script)}`;
}

function sessionActionCommand(action, pastTense, extraPayload = {}) {
  return async (args) => {
    const { flags, rest } = parseArgs(args);
    /**
     * CDXC:AndroidRemoteSessions 2026-05-17-13:57:
     * Ghostex Android uses `--session-id <id> --json` for remote wake, sleep,
     * and kill actions so every context-menu command has the same stable
     * selector shape as rename. Keep positional selectors for human usage.
     */
    const selector = flags.sessionId ?? rest.join(" ").trim();
    const result = await fetchSessionList(flags);
    const sessions =
      selector.toLowerCase() === "all"
        ? (result.sessions ?? [])
        : [await resolveOneListedSession(selector, result.sessions ?? [], flags)];
    if (sessions.length === 0) {
      throw new Error("No running terminal sessions matched.");
    }
    const affected = [];
    for (const session of sessions) {
      const actionResult = await sendSidebarCliCommand(
        action,
        {
          ...extraPayload,
          projectId: session.projectId,
          sessionId: session.sessionId,
        },
        flags,
      );
      /**
       * CDXC:AndroidRemoteSessions 2026-05-17-20:58:
       * Android relies on `ghostex sleep|wake|kill --session-id --json` exit
       * status for recovery UI. Treat bridge-level failures the same as
       * command-level `{ ok: false }`, and preserve JSON output when Android
       * requested it so the phone can extract concise error/message fields.
       */
      if (isFailedCliResult(actionResult)) {
        if (flags.json) {
          printJson(actionResult);
          process.exitCode = 1;
          return;
        }
        throw new Error(actionResult.error ?? `Could not ${action} ${session.title}.`);
      }
      affected.push({ ok: actionResult.ok !== false, session });
    }
    if (flags.json) {
      printJson({ ok: affected.every((item) => item.ok), sessions: affected });
      return;
    }
    for (const item of affected) {
      console.log(`${pastTense} ${item.session.alias}: ${item.session.title}`);
    }
  };
}

async function forkSessionCommand(args) {
  const { flags, rest } = parseArgs(args);
  const selector = flags.sessionId ?? rest.join(" ").trim();
  const result = await fetchSessionList(flags);
  const session = await resolveOneListedSession(selector, result.sessions ?? [], flags);
  /*
   * CDXC:GxserverForkSession 2026-06-04-07:42:
   * CLI/mobile Fork must call gxserver directly, not the macOS app bridge. The daemon owns provider-specific fork command construction, creates the new G-session, starts zmx, and returns the created session for Android/iOS refresh flows.
   */
  const actionResult = await sendSidebarCliCommand(
    "forkSession",
    { projectId: session.projectId, sessionId: session.sessionId },
    flags,
  );
  if (isFailedCliResult(actionResult)) {
    if (flags.json) {
      printJson(actionResult);
      process.exitCode = 1;
      return;
    }
    throw new Error(actionResult.error ?? `Could not fork ${session.title}.`);
  }
  if (flags.json) {
    printJson(actionResult);
    return;
  }
  const forkedSession = actionResult.fork?.session;
  console.log(`forked ${session.alias}: ${session.title}${forkedSession?.sessionId ? ` -> ${forkedSession.sessionId}` : ""}`);
}

async function focusSmartSessionCommand(args) {
  const { flags, rest } = parseArgs(args);
  /**
   * CDXC:AndroidRemoteSessions 2026-05-17-13:57:
   * Android focus is a remote sidebar context action and should use the same
   * structured session-id flag form as lifecycle actions.
   */
  const selector = flags.sessionId ?? rest.join(" ").trim();
  const result = await fetchSessionList(flags);
  const session = await resolveOneListedSession(selector, result.sessions ?? [], flags);
  const actionResult = await sendSidebarCliCommand(
    "focusSession",
    { projectId: session.projectId, sessionId: session.sessionId },
    flags,
  );
  /**
   * CDXC:AndroidRemoteSessions 2026-05-17-14:24:
   * Android treats the SSH process exit status as the remote action contract.
   * If focus returns `{ ok: false }`, exit nonzero instead of printing a JSON
   * failure with status 0, otherwise Android would report a failed focus as
   * successful.
   */
  if (isFailedCliResult(actionResult)) {
    if (flags.json) {
      printJson(actionResult);
      process.exitCode = 1;
      return;
    }
    throw new Error(actionResult.error ?? `Could not focus ${session.title}.`);
  }
  if (flags.json) {
    printJson(actionResult);
    return;
  }
  console.log(`focused ${session.alias}: ${session.title}`);
}

async function readSessionTextCommand(args) {
  const { flags, rest } = parseArgs(args);
  const selector = sessionSelectorFromArgs(rest, flags);
  const payload = {
    source: flags.visible === true || flags.source === "visible" ? "visible" : "screen",
    timeoutMs: flags.timeoutMs === undefined ? undefined : Number(flags.timeoutMs),
  };
  if (selector) {
    const session = await resolveCliSessionSelector(selector, flags);
    payload.projectId = session.projectId;
    payload.sessionId = session.sessionId;
  }
  const result = await sendSidebarCliCommand("readSessionText", payload, flags);
  if (isFailedCliResult(result)) {
    if (flags.json) {
      printJson(result);
      process.exitCode = 1;
      return;
    }
    throw new Error(result.error ?? "Could not read terminal text.");
  }
  const text = String(result.text ?? "");
  const lines = flags.lines === undefined ? undefined : Number(flags.lines);
  if (flags.json) {
    printJson({ ...result, text: limitTextLines(text, lines) });
    return;
  }
  process.stdout.write(limitTextLines(text, lines));
  if (!text.endsWith("\n")) {
    process.stdout.write("\n");
  }
}

async function sendMessageCommand(args) {
  const { flags, rest } = parseArgs(args);
  const explicitSelector = sessionSelectorFromArgs([], flags);
  let selector = explicitSelector;
  let agentId = typeof flags.agent === "string" ? flags.agent : undefined;
  let textStartIndex = 0;

  if (!selector && !agentId && rest[0]) {
    const firstArg = rest[0];
    const result = await fetchSessionList(flags);
    const matches = await resolveListedSessions(firstArg, result.sessions ?? [], flags);
    if (matches.length > 1) {
      throw new Error(`Multiple sessions matched "${firstArg}":\n${formatSessionMatches(matches)}`);
    }
    if (matches.length === 1) {
      selector = firstArg;
      textStartIndex = 1;
    } else {
      agentId = firstArg;
      textStartIndex = 1;
    }
  }

  const text = String(flags.text ?? rest.slice(textStartIndex).join(" "));
  const payload = {
    groupId: flags.groupId,
    sendDelayMs: flags.sendDelayMs === undefined ? undefined : Number(flags.sendDelayMs),
    submit: flags.submit === undefined ? true : parseBoolean(flags.submit),
    text,
  };
  if (selector) {
    const session = await resolveCliSessionSelector(selector, flags);
    payload.projectId = session.projectId;
    payload.sessionId = session.sessionId;
  } else {
    payload.agentId = agentId;
  }
  const result = await sendSidebarCliCommand("sendMessage", payload, flags);
  if (isFailedCliResult(result)) {
    printJson(result);
    process.exitCode = 1;
    return;
  }
  printJson(result);
}

async function fetchSessionList(flags = {}, options = {}) {
  const result = await sendSidebarCliCommand("listSessions", {}, flags);
  /**
   * CDXC:AndroidRemoteSessions 2026-06-11-23:52:
   * `ghostex sessions --json` is the Android and iOS reconnect/status contract. The inventory must come from gxserver list/snapshot APIs, including gxserver's server-side hook sidecar ingestion, and must not read the retired macOS sidebar persistence file when the daemon is unreachable.
   */
  if (isFailedCliResult(result)) {
    throw new Error(result.error ?? "Could not list Ghostex sessions.");
  }
  const sessions = Array.isArray(result.sessions) ? result.sessions : [];
  if (options.writeCache === true) {
    await writeSessionAliasCache({
      createdAt: new Date().toISOString(),
      revision: result.revision,
      sessions,
    });
  }
  return { ...result, sessions };
}

async function writeSessionAliasCache(cache) {
  /**
   * CDXC:CliSessions 2026-05-07-21:22
   * The human sessions CLI uses global aliases from the last printed live list
   * so follow-up commands such as `ghostex a 2` and `ghostex k 4` target the rows the
   * user just saw, independent of grouped or ungrouped table formatting.
   */
  await mkdir(CLI_DIR, { recursive: true });
  await writeFile(SESSION_ALIAS_CACHE_PATH, JSON.stringify(cache, null, 2));
}

async function readSessionAliasCache() {
  const text = await readFile(SESSION_ALIAS_CACHE_PATH, "utf8").catch(() => undefined);
  return text ? parseJson(text) : undefined;
}

function sessionSelectorFromArgs(rest, flags) {
  return String(
    flags.sessionId ??
      flags.selector ??
      flags.session ??
      flags.sessionTitle ??
      flags.target ??
      rest[0] ??
      "",
  ).trim();
}

async function resolveCliSessionSelector(selector, flags) {
  /**
   * CDXC:CliSessionSelectors 2026-05-23-13:18:
   * Cross-session CLI actions need the same id/title/project:title selector
   * behavior as attach/focus so agents can address another visible sidebar
   * thread without knowing its raw runtime id.
   */
  const result = await fetchSessionList(flags);
  return resolveOneListedSession(selector, result.sessions ?? [], flags);
}

async function resolveOneListedSession(selector, sessions, flags = {}) {
  const matches = await resolveListedSessions(selector, sessions, flags);
  if (matches.length === 1) {
    return matches[0];
  }
  if (matches.length === 0) {
    throw new Error(`No matching session found for "${selector}". Run "ghostex sessions" or "gx sessions" to list sessions.`);
  }
  throw new Error(`Multiple sessions matched "${selector}":\n${formatSessionMatches(matches)}`);
}

async function resolveListedSessions(selector, sessions, flags = {}) {
  const normalizedSelector = selector.trim();
  if (!normalizedSelector) {
    throw new Error("Provide a session alias, id, provider session name, title, or project:title selector.");
  }
  /**
   * CDXC:CliSessionSelectors 2026-06-04-03:20:
   * Bare G session ids can repeat across projects. Honor --project-id when a
   * caller has inventory context, and keep unscoped duplicates ambiguous so the
   * CLI does not silently attach to a different zmx session than the user
   * selected.
   */
  const scopedSessions = projectScopedSessions(sessions, flags);
  if (/^\d+$/.test(normalizedSelector)) {
    const alias = Number(normalizedSelector);
    const cache = await readSessionAliasCache();
    const cachedSessionId = cache?.sessions?.find?.((session) => session.alias === alias)?.sessionId;
    if (cachedSessionId) {
      const liveSession = scopedSessions.find((session) => session.sessionId === cachedSessionId);
      if (liveSession) {
        return [liveSession];
      }
    }
    const liveAliasMatch = scopedSessions.find((session) => session.alias === alias);
    return liveAliasMatch ? [liveAliasMatch] : [];
  }
  const exactIdMatches = scopedSessions.filter((session) => session.sessionId === normalizedSelector);
  if (exactIdMatches.length > 0) {
    return exactIdMatches;
  }
  const exactGlobalRef = scopedSessions.find((session) => session.globalRef === normalizedSelector);
  if (exactGlobalRef) {
    return [exactGlobalRef];
  }
  /**
   * CDXC:CliSessionSelectors 2026-05-28-10:55:
   * Terminals export GHOSTEX_SESSION_ID as the provider persistence name
   * (for example `g-0527-090339`). Cross-session CLI actions must resolve that
   * id before falling back to title matching so generate-title and agent
   * orchestration can target the current pane reliably.
   */
  const providerMatches = rankProviderSessionMatches(scopedSessions, normalizedSelector);
  if (providerMatches.length > 0) {
    return providerMatches;
  }
  const projectSeparatorIndex = normalizedSelector.indexOf(":");
  if (projectSeparatorIndex > 0) {
    const projectSelector = normalizedSelector.slice(0, projectSeparatorIndex).trim().toLowerCase();
    const titleSelector = normalizedSelector.slice(projectSeparatorIndex + 1).trim().toLowerCase();
    return rankSessionTitleMatches(
      scopedSessions.filter(
        (session) =>
          session.projectName?.toLowerCase() === projectSelector ||
          session.projectPath?.toLowerCase().includes(projectSelector),
      ),
      titleSelector,
    );
  }
  return rankSessionTitleMatches(scopedSessions, normalizedSelector.toLowerCase());
}

function projectScopedSessions(sessions, flags = {}) {
  const projectId = String(flags.projectId ?? "").trim();
  if (!projectId) {
    return sessions;
  }
  return sessions.filter((session) => sessionProjectId(session) === projectId);
}

function sessionProjectId(session) {
  return String(session.projectId ?? projectIdFromGlobalRef(session.globalRef) ?? "").trim();
}

function rankProviderSessionMatches(sessions, selector) {
  const normalizedSelector = selector.trim();
  if (!normalizedSelector) {
    return [];
  }

  const slashIndex = normalizedSelector.indexOf("/");
  if (slashIndex > 0) {
    const provider = normalizedSelector.slice(0, slashIndex).trim().toLowerCase();
    const providerSessionName = normalizedSelector.slice(slashIndex + 1).trim();
    if (!provider || !providerSessionName) {
      return [];
    }
    return sessions.filter(
      (session) =>
        session.provider?.toLowerCase() === provider &&
        session.providerSessionName === providerSessionName,
    );
  }

  return sessions.filter((session) => session.providerSessionName === normalizedSelector);
}

function rankSessionTitleMatches(sessions, selector) {
  const exact = sessions.filter((session) =>
    session.title?.toLowerCase() === selector ||
    session.displayTitle?.toLowerCase() === selector
  );
  if (exact.length > 0) {
    return exact;
  }
  return sessions.filter((session) =>
    session.title?.toLowerCase().includes(selector) ||
    session.displayTitle?.toLowerCase().includes(selector)
  );
}

function formatSessionMatches(sessions) {
  return sessions
    .map((session) => `${session.alias}. ${session.projectName} - ${session.displayTitle ?? session.title}`)
    .join("\n");
}

function limitTextLines(text, lines) {
  if (!Number.isFinite(lines) || lines <= 0) {
    return text;
  }
  return text.split(/\r?\n/).slice(-lines).join("\n");
}

function printSessionList(sessions, { grouped }) {
  if (sessions.length === 0) {
    console.log("No running terminal sessions.");
    return;
  }
  /**
   * CDXC:CliSessions 2026-05-20-12:20:
   * The human session list should stay compact on narrow terminals: group by
   * project with the project path as the section header, print each session as
   * a short two-line block without field labels, and preserve the sidebar order
   * returned by the native inventory instead of re-sorting in the CLI.
   */
  const projectGroups = groupSessionsPreservingSidebarOrder(sessions);
  if (!grouped) {
    for (const project of projectGroups) {
      for (const session of project.sessions) {
        console.log(
          formatCompactSessionLine(session, {
            projectLabel: project.projectName,
          }),
        );
      }
    }
    return;
  }
  projectGroups.forEach((project, projectIndex) => {
    if (projectIndex > 0) {
      console.log("");
    }
    console.log(project.projectName);
    if (project.projectPath) {
      console.log(project.projectPath);
    }
    for (const session of project.sessions) {
      console.log(formatCompactSessionLine(session));
    }
  });
}

function printSessionPickerRows(sessions) {
  for (const row of buildSessionPickerRows(sessions)) {
    console.log(row.text);
  }
}

function isInteractiveTerminal(input = process.stdin, output = process.stdout) {
  return Boolean(input.isTTY && output.isTTY);
}

function buildSessionPickerModel(sessions) {
  /**
   * CDXC:CliSessionPicker 2026-05-24-18:10:
   * The bare CLI picker is a visual mirror of the macOS sidebar order: project
   * rows are separators named only by project, while selectable session rows
   * render only the saved title string with no alias, status, path, or wrapping
   * metadata. Left/right navigation moves by project boundaries; enter and
   * space select the session for the normal attach flow.
   *
   * CDXC:CliSessionPicker 2026-05-24-18:25:
   * The first no-project terminal group is labeled "Quick Terminals" so
   * scratch terminals are recognizable, section headers get a blank line plus
   * bold colored styling, and each session title keeps an agent-specific color
   * marker in front of the saved title.
   *
   * CDXC:CliSessionPicker 2026-05-24-18:31:
   * Selected sessions recolor the full row, not only the leading agent marker,
   * so the active target remains obvious in terminals where isolated marker
   * color is hard to scan.
   *
   * CDXC:CliSessionPicker 2026-05-24-18:45:
   * The picker opens with an explicit attach prompt and separator before the
   * session sections. Agent marks are colored three-character indicators in
   * square brackets, not glyphs, so every session row has a stable text width.
   *
   * CDXC:CliSessionPicker 2026-05-24-18:47:
   * The picker header is a bright-white bold title with a real terminal rule
   * below it, no blank spacer rows. Navigation wraps at list ends, and Page Up
   * / Page Down jump five sessions at a time for faster long-list movement.
   */
  const groups = groupSessionsPreservingSidebarOrder(sessions).filter(
    (project) => project.sessions.length > 0,
  );
  const items = [
    {
      kind: "title",
      plainText: PICKER_TITLE,
      renderText: `${PICKER_TITLE_STYLE}${PICKER_TITLE}${RESET_ANSI}`,
    },
    {
      kind: "separator",
      plainText: "─",
      renderText: "─",
    },
  ];
  const sessionItems = [];
  let sessionIndex = 0;
  groups.forEach((project, projectIndex) => {
    const startSessionIndex = sessionIndex;
    items.push({
      kind: "project",
      projectIndex,
      plainText: project.projectName,
      renderText: `${PROJECT_HEADER_STYLE}${project.projectName}${RESET_ANSI}`,
    });
    for (const session of project.sessions) {
      const agentIndicator = resolveSessionPickerAgentIndicator(session);
      const title = String(session.title ?? "");
      const item = {
        agentIndicator,
        kind: "session",
        plainText: `[${agentIndicator.label}] ${title}`,
        projectIndex,
        renderText: `${ansiColor(agentIndicator.color)}[${agentIndicator.label}]${RESET_ANSI} ${title}`,
        session,
        sessionIndex,
      };
      items.push(item);
      sessionItems.push(item);
      sessionIndex += 1;
    }
    project.startSessionIndex = startSessionIndex;
    project.endSessionIndex = sessionIndex - 1;
  });
  return { groups, items, sessionItems };
}

function buildSessionPickerRows(sessions, selectedSessionIndex = 0) {
  const model = buildSessionPickerModel(sessions);
  return model.items.map((item) => ({
    ...(item.kind === "session" ? { agentIndicator: item.agentIndicator } : {}),
    kind: item.kind,
    selected: item.kind === "session" && item.sessionIndex === selectedSessionIndex,
    text: item.plainText,
  }));
}

function moveSessionPickerSelection(model, selectedSessionIndex, direction) {
  const sessionCount = model.sessionItems.length;
  if (sessionCount === 0) {
    return 0;
  }
  if (direction === "up") {
    return wrapSessionPickerIndex(selectedSessionIndex - 1, sessionCount);
  }
  if (direction === "down") {
    return wrapSessionPickerIndex(selectedSessionIndex + 1, sessionCount);
  }
  if (direction === "pageup") {
    return wrapSessionPickerIndex(selectedSessionIndex - 5, sessionCount);
  }
  if (direction === "pagedown") {
    return wrapSessionPickerIndex(selectedSessionIndex + 5, sessionCount);
  }
  if (direction === "left" || direction === "right") {
    const current = model.sessionItems[selectedSessionIndex];
    const delta = direction === "left" ? -1 : 1;
    const targetProjectIndex = wrapSessionPickerIndex(current.projectIndex + delta, model.groups.length);
    const targetProject = model.groups[targetProjectIndex];
    return targetProject ? targetProject.startSessionIndex : selectedSessionIndex;
  }
  return selectedSessionIndex;
}

function wrapSessionPickerIndex(index, count) {
  return ((index % count) + count) % count;
}

async function runInteractiveSessionPicker(sessions, input = process.stdin, output = process.stdout) {
  const model = buildSessionPickerModel(sessions);
  if (model.sessionItems.length === 0) {
    return undefined;
  }
  let selectedSessionIndex = 0;
  let viewportStart = 0;
  const wasRaw = input.isRaw === true;
  emitKeypressEvents(input);

  return await new Promise((resolve) => {
    const cleanup = () => {
      input.off("keypress", onKeypress);
      if (input.isTTY && !wasRaw) {
        input.setRawMode(false);
      }
      output.write("\x1b[?7h\x1b[?25h\x1b[?1049l");
    };
    const selectCurrentSession = () => {
      const selected = model.sessionItems[selectedSessionIndex]?.session;
      cleanup();
      resolve(selected);
    };
    const cancel = (exitCode) => {
      if (exitCode !== undefined) {
        process.exitCode = exitCode;
      }
      cleanup();
      resolve(undefined);
    };
    const render = () => {
      viewportStart = renderSessionPicker(model, selectedSessionIndex, viewportStart, output);
    };
    const onKeypress = (_chunk, key = {}) => {
      if (key.ctrl && key.name === "c") {
        cancel(130);
        return;
      }
      if (key.name === "escape" || key.name === "q") {
        cancel();
        return;
      }
      if (key.name === "return" || key.name === "enter" || key.name === "space") {
        selectCurrentSession();
        return;
      }
      const nextSelection = moveSessionPickerSelection(model, selectedSessionIndex, key.name);
      if (nextSelection !== selectedSessionIndex) {
        selectedSessionIndex = nextSelection;
        render();
      }
    };

    output.write("\x1b[?1049h\x1b[?25l\x1b[?7l");
    if (input.isTTY && !wasRaw) {
      input.setRawMode(true);
    }
    input.resume();
    input.on("keypress", onKeypress);
    render();
  });
}

function renderSessionPicker(model, selectedSessionIndex, viewportStart, output) {
  const terminalRows = Math.max(1, Number(output.rows ?? 24));
  const selectedLineIndex = model.items.findIndex(
    (item) => item.kind === "session" && item.sessionIndex === selectedSessionIndex,
  );
  const maxViewportStart = Math.max(0, model.items.length - terminalRows);
  let nextViewportStart = Math.min(viewportStart, maxViewportStart);
  if (selectedLineIndex < nextViewportStart) {
    nextViewportStart = selectedLineIndex;
  } else if (selectedLineIndex >= nextViewportStart + terminalRows) {
    nextViewportStart = selectedLineIndex - terminalRows + 1;
  }

  output.write("\x1b[H");
  for (let row = 0; row < terminalRows; row += 1) {
    const item = model.items[nextViewportStart + row];
    let line = "";
    if (item) {
      const text =
        item.kind === "separator"
          ? `${PROJECT_HEADER_STYLE}${"─".repeat(output.columns ?? 80)}${RESET_ANSI}`
          : item.renderText;
      line =
        item.kind === "session" && item.sessionIndex === selectedSessionIndex
          ? `${SELECTED_SESSION_STYLE}${stripAnsi(text)}${RESET_ANSI}`
          : text;
    }
    output.write(`\x1b[2K${line}${row === terminalRows - 1 ? "" : "\r\n"}`);
  }
  return nextViewportStart;
}

function resolveSessionPickerProjectName(session, isFirstGroup) {
  if (isFirstGroup && !String(session.projectPath ?? "").trim()) {
    return QUICK_TERMINALS_PROJECT_NAME;
  }
  return session.projectName || session.projectPath || QUICK_TERMINALS_PROJECT_NAME;
}

function resolveSessionPickerAgentIndicator(session) {
  const candidates = [
    session.agent,
    session.agentIcon,
    session.agentId,
    session.agentName,
    session.provider,
  ];
  for (const candidate of candidates) {
    const key = normalizeAgentIndicatorKey(candidate);
    if (key && AGENT_PICKER_INDICATORS.has(key)) {
      return AGENT_PICKER_INDICATORS.get(key);
    }
  }
  return DEFAULT_PICKER_AGENT_INDICATOR;
}

function normalizeAgentIndicatorKey(value) {
  return String(value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, "-")
    .replace(/-cli$/u, "-cli");
}

function ansiColor(hexColor) {
  const match = String(hexColor).match(/^#?([0-9a-f]{6})$/iu);
  if (!match) {
    return "";
  }
  const value = match[1];
  const red = Number.parseInt(value.slice(0, 2), 16);
  const green = Number.parseInt(value.slice(2, 4), 16);
  const blue = Number.parseInt(value.slice(4, 6), 16);
  return `\x1b[38;2;${red};${green};${blue}m`;
}

function stripAnsi(value) {
  return String(value).replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "");
}

function formatCompactSessionLine(session, { projectLabel } = {}) {
  const marker = session.isFocused ? "›" : " ";
  const title = session.displayTitle || session.title || "-";
  const headline = projectLabel
    ? `${marker} #${session.alias}  ${projectLabel} · ${title}`
    : `${marker} #${session.alias}  ${title}`;
  const details = [
    session.agent,
    formatCompactProvider(session),
    session.status,
    formatActiveTime(session.lastInteractionAt),
  ]
    .map((value) => String(value ?? "").trim())
    .filter((value) => value.length > 0 && value !== "-");
  if (details.length === 0) {
    return headline;
  }
  return `${headline}\n    ${details.join(" · ")}`;
}

function formatCompactProvider(session) {
  const provider = session.provider?.trim();
  const providerSessionName = session.providerSessionName?.trim();
  if (!provider) {
    return undefined;
  }
  if (providerSessionName) {
    return `${provider}/${providerSessionName}`;
  }
  return provider;
}

function groupSessionsPreservingSidebarOrder(sessions) {
  const groups = [];
  const groupsByProjectId = new Map();
  for (const session of sessions) {
    let group = groupsByProjectId.get(session.projectId);
    if (!group) {
      const isFirstGroup = groups.length === 0;
      group = {
        projectName: resolveSessionPickerProjectName(session, isFirstGroup),
        projectPath: session.projectPath || "",
        sessions: [],
      };
      groupsByProjectId.set(session.projectId, group);
      groups.push(group);
    }
    group.sessions.push(session);
  }
  return groups;
}

function formatActiveTime(value) {
  const timestamp = Date.parse(value ?? "");
  if (!Number.isFinite(timestamp)) {
    return "-";
  }
  const seconds = Math.max(0, Math.round((Date.now() - timestamp) / 1000));
  if (seconds < 60) {
    return `${seconds}s ago`;
  }
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ago`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return `${hours}h ago`;
  }
  const days = Math.round(hours / 24);
  return `${days}d ago`;
}

async function runInteractiveShellCommand(command, cwd) {
  await runInteractiveProcess("/bin/zsh", ["-lc", command], { cwd });
}

async function runInteractiveProcess(command, args, options = {}) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env,
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (signal) {
        process.kill(process.pid, signal);
        return;
      }
      process.exitCode = code ?? 0;
      resolve();
    });
  });
}

function parseCreateSession(rest, flags) {
  /**
   * CDXC:AndroidRemoteSessions 2026-05-18-02:31:
   * Ghostex Android creates new terminals through `ghostex create-session`
   * over SSH.
   *
   * CDXC:GxserverCliCutover 2026-05-30-15:15:
   * Preserve project/group flags so gxserver owns the creation path and applies
   * zmx-backed session persistence after the hard cutover.
   */
  return {
    groupId: flags.groupId,
    input: flags.input ?? rest.slice(1).join(" "),
    projectId: flags.projectId,
    title: flags.title ?? rest[0],
  };
}

function parseAgent(rest, flags) {
  return {
    agentId: flags.agentId ?? rest[0],
    groupId: flags.groupId,
  };
}

function parseCommandButton(rest, flags) {
  return { commandId: flags.commandId ?? rest[0] };
}

function parseClickButton(rest, flags) {
  return {
    id: flags.id ?? rest[1],
    kind: flags.kind ?? rest[0],
  };
}

function parseSaveAgent(rest, flags) {
  return {
    acceptAllMode: flags.acceptAllMode,
    agentId: flags.agentId ?? rest[0],
    command: flags.command ?? rest.slice(2).join(" "),
    icon: flags.icon,
    name: flags.name ?? rest[1],
  };
}

function parseAutomationProject(rest, flags) {
  return {
    projectId: flags.projectId,
    projectPath: flags.projectPath ?? flags.path ?? rest[0],
  };
}

function parseAutomationSave(rest, flags) {
  const definitionJson = flags.definitionJson ?? flags.payloadJson ?? rest.join(" ");
  return {
    ...parseAutomationProject([], flags),
    definition: typeof definitionJson === "string" ? parseJson(definitionJson) : undefined,
  };
}

function parseAutomationId(rest, flags) {
  return {
    ...parseAutomationProject([], flags),
    automationId: flags.automationId ?? flags.id ?? rest[0],
  };
}

function parseAutomationEnabled(rest, flags) {
  return {
    ...parseAutomationId(rest, flags),
    enabled: parseBoolean(flags.enabled ?? flags.value ?? rest[1] ?? "true"),
  };
}

function parseAutomationRun(rest, flags) {
  return {
    ...parseAutomationProject([], flags),
    removeWorktree: parseBoolean(flags.removeWorktree ?? "false"),
    runId: flags.runId ?? flags.id ?? rest[0],
  };
}

function parseGroup(rest, flags) {
  return { groupId: flags.groupId ?? rest[0] };
}

function parseProject(rest, flags) {
  return {
    name: flags.name,
    path: flags.path ?? rest[0],
    projectId: flags.projectId,
  };
}

function parseProjectMove(rest, flags) {
  /**
   * CDXC:AndroidSidebar 2026-05-18-16:13:
   * Ghostex Android reorders project groups through the Mac CLI, not local phone
   * state. The desktop sidebar remains the source of truth and later inventory
   * calls return the persisted order to mobile.
   */
  return {
    direction: flags.direction ?? flags.dir ?? rest[1],
    projectId: flags.projectId ?? rest[0],
  };
}

function parseProjectPath(rest, flags) {
  return {
    name: flags.name,
    path: flags.path ?? rest[0],
  };
}

function parseSessionSelector(rest, flags) {
  return {
    index: flags.index === undefined ? undefined : Number(flags.index),
    sessionId: flags.sessionId ?? rest[0],
    sessionNumber: flags.sessionNumber === undefined ? undefined : Number(flags.sessionNumber),
  };
}

function parseRename(rest, flags) {
  /**
   * CDXC:AndroidRemoteSessions 2026-05-17-13:23:
   * Ghostex Android invokes remote rename through `ghostex rename-session --session-id <id> --title <title> --json` so SSH quoting can keep the stable session id and user-entered title as separate CLI arguments. Keep positional parsing for human CLI usage, but treat the flag form as part of the Android gxserver CLI contract.
   */
  return {
    ...parseSessionSelector(rest, flags),
    title: flags.title ?? rest.slice(1).join(" "),
  };
}

function parseSessionBoolean(name) {
  return (rest, flags) => {
    const hasFlagSelector =
      flags.sessionId !== undefined || flags.index !== undefined || flags.sessionNumber !== undefined;
    return {
      ...parseSessionSelector(rest, flags),
      [name]: parseBoolean(flags[name] ?? flags.value ?? rest[hasFlagSelector ? 0 : 1] ?? "true"),
    };
  };
}

function parseSendText(rest, flags) {
  const hasFlagSelector =
    flags.sessionId || flags.selector || flags.session || flags.sessionTitle || flags.target;
  return {
    ...parseSessionSelector(rest, flags),
    text: flags.text ?? rest.slice(hasFlagSelector ? 0 : 1).join(" "),
  };
}

function parseSendKey(rest, flags) {
  const hasFlagSelector =
    flags.sessionId || flags.selector || flags.session || flags.sessionTitle || flags.target;
  return {
    ...parseSessionSelector(rest, flags),
    key: flags.key ?? rest[hasFlagSelector ? 0 : 1],
  };
}

function parseVisibleCount(rest, flags) {
  return { count: Number(flags.count ?? rest[0]) };
}

function parseViewMode(rest, flags) {
  return { mode: flags.mode ?? rest[0] };
}

function parseUrl(rest, flags) {
  return { url: flags.url ?? rest[0] };
}

function parseBrowserOpen(rest, flags) {
  return {
    groupId: flags.groupId,
    projectId: flags.projectId,
    projectName: flags.projectName ?? flags.name,
    projectPath: flags.projectPath ?? flags.path ?? (parseBoolean(flags.activeProject) ? undefined : process.cwd()),
    reuse: flags.new ? "none" : flags.reuse ?? "similar",
    url: flags.url ?? rest[0],
  };
}

function parseOpenPaths(rest, flags) {
  const targets = rest.length > 0 ? rest : flags.path ? [flags.path] : [];
  return {
    mode: "open",
    targets: targets.map((target) => parseOpenPathTarget(target)),
  };
}

function parseEditPaths(rest, flags) {
  const waitConsumedTarget = typeof flags.wait === "string" ? flags.wait : undefined;
  const targets =
    rest.length > 0
      ? rest
      : flags.goto
        ? [flags.goto]
        : flags.path
          ? [flags.path]
          : waitConsumedTarget
            ? [waitConsumedTarget]
            : [];
  const wait = flags.wait === true || waitConsumedTarget !== undefined || parseBoolean(flags.wait ?? false);
  return {
    mode: "edit",
    targets: targets.map((target) => parseOpenPathTarget(target, wait)),
    wait,
  };
}

function parseQuickTerminal(rest, flags) {
  const commandSeparatorIndex = rest.indexOf("--");
  const commandRest = commandSeparatorIndex >= 0 ? rest.slice(commandSeparatorIndex + 1) : rest;
  return {
    command: commandRest.length > 0 ? commandRest.join(" ") : undefined,
    cwd: flags.cwd ?? flags.path,
    title: flags.title ?? flags.name,
  };
}

function parseOpenPathTarget(value, wait = false) {
  const raw = String(value ?? "").trim();
  const parsed = parseVsCodePathPosition(raw);
  const target = {
    column: parsed.column,
    line: parsed.line,
    path: path.resolve(parsed.path),
    raw,
  };
  if (wait) {
    /**
     * CDXC:OSIntegration 2026-05-27-18:06:
     * `ghostex edit --wait` waits for a concrete opened editor item, so each
     * target carries a stable per-command wait token across the native bridge.
     */
    target.waitToken = `wait-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
  }
  return target;
}

function parseVsCodePathPosition(value) {
  /**
   * CDXC:OSIntegration 2026-05-27-18:06:
   * Default-editor CLI opens need VS Code-style `file:line:column` targets so
   * external tools can hand Ghostex positioned file references without learning
   * a Ghostex-specific flag shape.
   */
  const match = /^(?<path>.+?)(?::(?<line>[1-9]\d*))?(?::(?<column>[1-9]\d*))?$/u.exec(value);
  if (!match?.groups?.path) {
    return { path: value };
  }
  const candidatePath = match.groups.path;
  const line = match.groups.line ? Number(match.groups.line) : undefined;
  const column = match.groups.column ? Number(match.groups.column) : undefined;
  return { column, line, path: candidatePath };
}

function isExistingBarePathArgument(value) {
  if (!value || value.startsWith("-")) {
    return false;
  }
  const parsed = parseVsCodePathPosition(value);
  return existsSync(path.resolve(parsed.path));
}

function parseAssertCard(rest, flags) {
  return {
    ...parseSessionSelector(rest, flags),
    agentIcon: flags.agentIcon,
    agentName: flags.agentName,
    visible: flags.visible === undefined ? undefined : parseBoolean(flags.visible),
  };
}

function parseWaitFor(rest, flags) {
  return {
    ...parseAssertCard(rest, flags),
    intervalMs: flags.intervalMs === undefined ? undefined : Number(flags.intervalMs),
    timeoutMs: flags.timeoutMs === undefined ? undefined : Number(flags.timeoutMs),
  };
}

function parseArgs(args) {
  const flags = {};
  const rest = [];
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--") {
      rest.push(...args.slice(index + 1));
      break;
    }
    if (arg.startsWith("-") && !arg.startsWith("--") && arg.length > 1) {
      for (const shortFlag of arg.slice(1)) {
        flags[shortFlag] = true;
      }
      continue;
    }
    if (!arg.startsWith("--")) {
      rest.push(arg);
      continue;
    }
    const body = arg.slice(2);
    const equalsIndex = body.indexOf("=");
    if (equalsIndex >= 0) {
      flags[toCamelCase(body.slice(0, equalsIndex))] = body.slice(equalsIndex + 1);
      continue;
    }
    const key = toCamelCase(body);
    const next = args[index + 1];
    if (!next || next.startsWith("--")) {
      flags[key] = true;
      continue;
    }
    flags[key] = next;
    index += 1;
  }
  return { flags, rest };
}

function filterLogLines(text, flags) {
  let lines = text.split(/\r?\n/).filter(Boolean);
  if (flags.since) {
    lines = lines.filter((line) => line.includes(String(flags.since)) || line > `[${flags.since}`);
  }
  if (flags.grep) {
    lines = lines.filter((line) => line.includes(String(flags.grep)));
  }
  return lines;
}

function parseBoolean(value) {
  return value === true || value === "true" || value === "1" || value === "yes";
}

function parseJson(value) {
  try {
    return JSON.parse(value);
  } catch {
    return undefined;
  }
}

function isFailedCliResult(result) {
  return result?.ok === false || result?.bridgeOk === false;
}

function toCamelCase(value) {
  return value.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
}

function timestampSlug() {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function printJson(value) {
  console.log(JSON.stringify(value, null, 2));
}

function helpCommand() {
  console.log(usage());
}

function formatHelpCommand(signature, description) {
  const commandColumnWidth = 58;
  const gap = " ".repeat(Math.max(2, commandColumnWidth - signature.length));
  return `  ${signature}${gap}${description}`;
}

function usage() {
  /**
   * CDXC:CliHelp 2026-05-15-20:33
   * The public Ghostex help menu should follow the organized zellij/zmx shape: a short product description, compact usage lines, aligned command groups with aliases beside the command name, and separate explanatory sections for selectors and workflows that would make the command table noisy.
   */
  const sessionCommands = [
    formatHelpCommand("sessions | s | ls [--ungrouped|-u] [--json]", "List running terminal sessions"),
    formatHelpCommand("find | f [zehn args...]", "Search agent prompt history with bundled zehn"),
    formatHelpCommand("android-check [--json]", "Verify this Mac is ready for Ghostex Android"),
    formatHelpCommand("attach | a [selector]", "Attach to a provider session, or open the picker without a selector"),
    formatHelpCommand("resume | r [selector]", "Alias for attach"),
    formatHelpCommand("attach | a --session-id <id> [--project-id id] [--prompt-editor monaco]", "Flag form used by mobile and desktop remote session attach"),
    formatHelpCommand("kill | k <selector|all> [--json]", "Close one session or every listed session"),
    formatHelpCommand("sleep <selector|all> [--json]", "Sleep one session or every listed session"),
    formatHelpCommand("wake <selector|all> [--json]", "Wake one session or every listed session"),
    formatHelpCommand("focus <selector> [--json]", "Unsupported in gxserver cutover until renderer focus events land"),
    formatHelpCommand("(sleep|wake|kill) --session-id <id> [--json]", "Flag form used by Android sidebar actions"),
  ].join("\n");

  const workspaceCommands = [
    formatHelpCommand("state | dump-state", "Print sidebar state as JSON"),
    formatHelpCommand("open | o <path...>", "Open files or folders in Ghostex"),
    formatHelpCommand("edit | e [--wait] [--goto] <file...>", "Open files in embedded Code"),
    formatHelpCommand("terminal | t [--cwd path] [--title title] [-- command...]", "Create a Quick terminal"),
    formatHelpCommand("create-session [title] [--input text] [--project-id id] [--group-id id]", "Create a terminal session"),
    formatHelpCommand("create-agent <agentId> [--group-id id]", "Create a configured agent session"),
    formatHelpCommand("run-agent <agentId>", "Run a configured agent button"),
    formatHelpCommand("run-command <commandId>", "Run a configured command button"),
    formatHelpCommand("click-button <agent|command> <id>", "Trigger a sidebar button"),
    formatHelpCommand("switch-project (--project-id|--path|--name) <value>", "Switch active project"),
    formatHelpCommand("move-project --project-id id --direction up|down", "Move a project in the desktop sidebar order"),
    formatHelpCommand("add-project <path> [--name name]", "Add a project to Ghostex"),
    formatHelpCommand("focus-session <id|--index n|--session-number n>", "Focus a session by raw selector"),
    formatHelpCommand("acknowledge-session-attention <selector>", "Mark a session's shared attention event as seen"),
    formatHelpCommand("focus-group <groupId>", "Focus a project group"),
  ].join("\n");

  const automationCommands = [
    formatHelpCommand("save-agent --agent-id id --name name --command command", "Create or update an agent button"),
    formatHelpCommand("automation-state [--path path|--project-id id]", "Print project automations and run history"),
    formatHelpCommand("automation-save --path path --definition-json json", "Create or update an automation"),
    formatHelpCommand("automation-run-now <automationId> --path path", "Queue an automation immediately"),
    formatHelpCommand("automation-set-enabled <automationId> <true|false> --path path", "Pause or resume an automation"),
    formatHelpCommand("automation-archive-run --run-id id --path path [--remove-worktree true]", "Archive a completed run"),
    formatHelpCommand("automation-mark-run-read --run-id id --path path", "Mark a run as read"),
    formatHelpCommand("bd <args...>", "Run Ghostex's bundled Beads CLI for the current project"),
  ].join("\n");

  const inputCommands = [
    formatHelpCommand("send-text <selector> <text>", "Type text into a session by id or quoted title"),
    formatHelpCommand("send-enter <selector>", "Send Enter to a session by id or quoted title"),
    formatHelpCommand("send-key <selector> <key>", "Send ctrl-c, escape, tab, or arrow keys"),
    formatHelpCommand("send-message <selector> <text>", "Type text and Enter into an existing session"),
    formatHelpCommand("send-message <agentId> <text>", "Unsupported in gxserver cutover until renderer-created visible sessions land"),
    formatHelpCommand("read-text <selector> [--lines n] [--visible] [--json]", "Read terminal text by id or quoted title"),
    formatHelpCommand("rename-session <sessionId> <title> [--json]", "Rename a session"),
    formatHelpCommand("rename-session --session-id <id> --title <title> [--json]", "Flag form used by Android SSH actions"),
    formatHelpCommand("rename-command <selector> <title>", "Send the agent rename command"),
  ].join("\n");

  const uiCommands = [
    formatHelpCommand("floating-editor | fe -- <editor> [args...]", "Open a draggable terminal overlay"),
    formatHelpCommand("floating-monaco-editor | fme <file>", "Open a draggable Monaco editor overlay"),
    formatHelpCommand("(close|restart|fork|reload)-session <id>", "Manage a session lifecycle"),
    formatHelpCommand(
      "sleep-session|favorite-session|pin-session <id> [true|false]",
      "Set raw session flags",
    ),
    formatHelpCommand("set-visible-count <1|2|3|4|6|9>", "Set visible session count"),
    formatHelpCommand("set-view-mode <grid|horizontal|vertical>", "Set session layout mode"),
    formatHelpCommand("browser --help", "Show embedded CEF browser control and MCP setup"),
    formatHelpCommand("computer-use --help", "Show Ghostex Computer Use skill setup for Cua Driver"),
    formatHelpCommand("agent-orchestration --help", "Show Ghostex Agent Orchestration skill setup"),
    formatHelpCommand("generate-title --help", "Show Ghostex Generate Title skill setup"),
    formatHelpCommand("manage-beads --help", "Show Ghostex Manage Beads skill setup"),
    formatHelpCommand("toggle-sidebar", "Collapse or expand the sidebar"),
    formatHelpCommand("move-sidebar", "Move the sidebar"),
  ].join("\n");

  const serverCommands = [
    formatHelpCommand("server", "Run gxserver in the foreground"),
    formatHelpCommand("server start [--json]", "Start gxserver in the background"),
    formatHelpCommand("server stop [--json]", "Stop only the gxserver control plane"),
    formatHelpCommand("server stop-all [--json]", "Stop gxserver and kill tracked zmx sessions"),
    formatHelpCommand("server status [--json]", "Print gxserver runtime state"),
    formatHelpCommand("server version | server --version", "Print the gxserver package version"),
    formatHelpCommand("server --help", "Show gxserver lifecycle command help"),
  ].join("\n");

  const evidenceCommands = [
    formatHelpCommand("screenshot [output.png]", "Capture the Ghostex window"),
    formatHelpCommand("logs [--file name] [--lines n] [--grep text] [--json]", "Print recent logs"),
    formatHelpCommand("bundle [output-dir] [--lines n]", "Save state, logs, and a screenshot"),
    formatHelpCommand("assert-card <id> [--agent-icon codex] [--visible true]", "Assert card projection"),
    formatHelpCommand("wait-for <id> [--agent-icon codex] [--timeout-ms n]", "Wait for card projection"),
  ].join("\n");

  return `Ghostex CLI - manage running Ghostex terminal sessions

Usage:
	  ghostex
	  gx
	  ghostex <path...>
	  ghostex <command> [args...] [--flags]
	  gx <command> [args...] [--flags]
  bun scripts/ghostex-cli.mjs <command> [args...] [--flags]

Commands:
${sessionCommands}

Workspace:
${workspaceCommands}

Automations:
${automationCommands}

Input:
${inputCommands}

UI:
${uiCommands}

Server:
${serverCommands}

Evidence:
${evidenceCommands}

Selectors:
  <selector> can be an alias, session id, provider session name, title, or project:title.
  Numeric aliases come from the last "ghostex sessions" or "gx sessions" list.
  Titles match exact first, then case-insensitive substring.

Sessions:
  Running ghostex or gx with no subcommand opens the Ghostex terminal TUI.
  gx find and gx f launch bundled zehn for prompt-history search; gx s remains sessions.
  The TUI shows the attached session, with a top switch button for project/session switching.
  The switcher lists Ghostex projects and sessions in macOS sidebar order and attaches through the existing zmx path.
  Direct attach stays available through attach/a/resume/r without opening the TUI.
  Projects and sessions follow the macOS sidebar order, including the active Last Active sort mode.
  Each project prints its path once as the section header, then compact session rows without field labels.
  --ungrouped/-u prints one flat list and prefixes each row with the project name.

Attach:
  attach/resume uses the stored tmux, zmx, or zellij provider session when present.
  Without provider metadata, it runs the supported agent resume command in the session project.

Global flags:
  --port <number>       Native bridge port
  --token-stdin         Read a temporary remote gxserver token from stdin
  --token <token>       Bridge token; legacy remote one-shot only because argv can expose secrets
  --timeout <ms>        Bridge request timeout
  server --help         Show server command help
  help | h              Show this help
  -h, --help            Show this help
`;
}

function serverUsage() {
  /*
   * CDXC:GxserverCli 2026-06-02-18:36:
   * `gx server --help` should expose every existing gxserver lifecycle command
   * under the user-facing CLI while still naming gxserver as the background
   * control plane. Do not add new daemon behavior here; this help mirrors the
   * existing gxserver command surface.
   */
  const commands = [
    formatHelpCommand("server", "Run gxserver in the foreground"),
    formatHelpCommand("server start [--json]", "Start gxserver in the background"),
    formatHelpCommand("server stop [--json]", "Stop only the gxserver control plane"),
    formatHelpCommand("server stop-all [--json]", "Stop gxserver and kill tracked zmx sessions"),
    formatHelpCommand("server status [--json]", "Print gxserver runtime state"),
    formatHelpCommand("server version", "Print the gxserver package version"),
    formatHelpCommand("server --version", "Alias for server version"),
    formatHelpCommand("server help | server --help", "Show this help"),
  ].join("\n");

  return `Ghostex Server - manage the gxserver background process

Usage:
  gx server
  gx server <command> [args...] [--flags]
  ghostex server <command> [args...] [--flags]

Commands:
${commands}

Lifecycle:
  gxserver is the Ghostex background control plane for projects, sessions,
  zmx lifecycle, auth, local APIs, logs, and remote/headless access.
  Closing the macOS app does not stop gxserver.
  gx server stop stops only the control plane; it does not kill zmx, tmux,
  zellij, shell, or agent sessions.
  gx server stop-all is destructive: it kills gxserver-tracked zmx sessions,
  marks killed sessions stopped, then stops the control plane.

Compatibility:
  The gxserver command remains available for server-only/headless installs.
  These gx server commands forward to the same gxserver implementation.
`;
}

function browserUsage() {
  /**
   * CDXC:BrowserAgentControl 2026-05-27-01:59:
   * `gx browser --help` is the agent-facing entry point for embedded CEF
   * control. Document the MCP command, install command, tool names, and common
   * debugging workflow here so agents do not need to infer browser setup from
   * the general Ghostex CLI help.
   *
   * CDXC:BrowserAgentControl 2026-05-27-06:43:
   * Browser help must prevent agents from creating duplicate tabs and from
   * opening panes in whichever project is currently active. Document project
   * scoping flags, cwd-based defaults, reuse behavior, and page-id reuse so
   * agents keep working in their own worktree and reuse similar browser tabs.
   */
  const setupCommands = [
    formatHelpCommand("browser mcp [--port n] [--target id|--page id]", "Run the stdio MCP server for CEF DevTools control"),
    formatHelpCommand("browser install-skill [--json]", "Install the $ghostex-browser-use skill into ~/agents/skills"),
    formatHelpCommand("browser open [url] [project/reuse flags]", "Open or reuse an embedded browser pane"),
    formatHelpCommand("browser open-pane [url] [project/reuse flags]", "Alias for browser open"),
  ].join("\n");

  const mcpTools = [
    formatHelpCommand("ghostex_list_pages", "List CEF DevTools targets and current page ids"),
    formatHelpCommand("ghostex_select_page", "Choose the target page for later tool calls"),
    formatHelpCommand("ghostex_navigate", "Navigate the selected CEF page"),
    formatHelpCommand("ghostex_console_logs", "Read console messages, Log entries, and exceptions captured after attach"),
    formatHelpCommand("ghostex_snapshot", "Get an accessibility-like DOM snapshot with @e element refs"),
    formatHelpCommand("ghostex_click / ghostex_fill", "Interact with @e refs or CSS selectors"),
    formatHelpCommand("ghostex_press_key", "Send Enter, Tab, Escape, arrows, or printable keys"),
    formatHelpCommand("ghostex_evaluate", "Run JavaScript in the selected page for inspection"),
    formatHelpCommand("ghostex_screenshot", "Capture a PNG screenshot as base64 MCP image content"),
  ].join("\n");

  return `Ghostex Browser Use - control embedded CEF panes from agents

Usage:
  gx browser --help
  gx browser mcp [--port n] [--target id|--page id] [--timeout ms]
  gx browser install-skill [--json]
  gx browser open [url] [--project-path path|--project-id id] [--reuse similar|exact|none]
  gx browser open-pane [url] [--project-path path|--project-id id] [--reuse similar|exact|none]
Agent MCP config:
  [mcp_servers.ghostex-browser]
  command = "ghostex"
  args = ["browser", "mcp"]

Commands:
${setupCommands}

Project scoping:
  browser open/open-pane default to the CLI process cwd as --project-path.
  Agents running in a worktree should keep that default, or pass --project-path "$PWD".
  Use --project-id when you already know the Ghostex project id from ghostex sessions --json.
  Use --group-id to place the browser in a specific project group.
  Use --active-project only for intentional manual control of the currently focused Ghostex project.

Tab reuse:
  browser open/open-pane default to --reuse similar, so an existing browser pane in the same project with the same origin is reused instead of creating a duplicate tab.
  Use --reuse exact when only the exact same URL should be reused.
  Use --reuse none or --new only when a separate browser pane is required.
  When a pane is reused for a different URL on the same origin, Ghostex focuses that pane and navigates it instead of creating another tab.
  After creating or selecting a page, keep the returned session id and the MCP page id from ghostex_list_pages; pass --target <pageId> to gx browser mcp or call ghostex_select_page before follow-up actions.

MCP tools exposed to the agent:
${mcpTools}

Recommended agent workflow:
  1. Run ghostex_list_pages to find browser targets.
  2. Run ghostex_select_page when more than one page is open.
  3. Run ghostex_console_logs before reproducing a bug, then again after the action.
  4. Run ghostex_snapshot and use @e refs with ghostex_click or ghostex_fill.
  5. Use ghostex_screenshot for visual proof and ghostex_evaluate for focused inspection.

Connection details:
  The MCP server talks directly to Ghostex's embedded CEF Chrome DevTools Protocol endpoint.
  It scans the default Ghostex CEF ports automatically. Pass --port or set
  GHOSTEX_CEF_REMOTE_DEBUGGING_PORT only when the app is using a non-default port.

Legacy aliases:
  browser-devtools-mcp and browser-mcp still run the MCP server.
  install-browser-skill still installs the skill, but new docs should use browser install-skill.
`;
}

function generateTitleUsage() {
  /**
   * CDXC:GenerateTitleSkill 2026-05-27-07:28:
   * Help documents only installation because title generation itself happens in
   * the `$ghostex-generate-title` skill. The skill owns the 47-character title
   * limit and the self-session `/rename <title>` contract.
   *
   * CDXC:GenerateTitleSkill 2026-06-09-17:49:
   * Document `rename-command` rather than `send-text` because generated titles
   * need Ghostex to stage `/rename <title>` and submit Enter as one supported
   * command contract.
   *
   * CDXC:GenerateTitleSkill 2026-06-12-04:10:
   * Document the same stable self-selector chain as the installed skill so zmx
   * terminals without GHOSTEX_SESSION_ID do not report missing-session.
   */
  return `Ghostex Generate Title - install the agent skill for naming Ghostex sessions

Usage:
  gx generate-title --help
  gx generate-title install-skill [--json]

Agent skill:
  Use $ghostex-generate-title when a task needs a concise Ghostex session title.

What the skill does:
  Generate one title shorter than 60 characters.
  Then submit /rename <title> in the current Ghostex session with rename-command.

Self-session command:
  ghostex rename-command --session-id "\${GHOSTEX_GLOBAL_SESSION_REF:-\${GHOSTEX_SESSION_ID:-\${ZMX_SESSION:-}}}" --title "<title>"
`;
}

function manageBeadsUsage() {
  /**
   * CDXC:ProjectBoardBeads 2026-06-04-03:32:
   * The bead workflow is agent-facing guidance rather than an app runtime API.
   * Keep the manage-beads surface focused on installing `$ghostex-manage-beads`; the
   * skill owns the exact Beads commands for creating review beads, adding
   * session-association comments, and moving beads through review.
   *
   * CDXC:ProjectBoardBeads 2026-06-10-09:31:
   * The skill must teach `gx bd` instead of raw `bd` so agents and users operate
   * on the same pinned Beads binary as Project/Kanban.
   */
  return `Ghostex Manage Beads - install the agent skill for project board beads

Usage:
  gx manage-beads --help
  gx manage-beads install-skill [--json]

Agent skill:
  Use $ghostex-manage-beads when a task needs project board bead management,
  including creating review beads, moving beads through statuses, adding
  comments, and associating a bead with the current Ghostex or Codex session.

What the skill teaches:
  Inspect existing beads with gx bd list/show/comments, create review beads with
  external refs such as codex-thread:$CODEX_THREAD_ID, move work to review, and
  add a session-association comment containing Ghostex and Codex ids when those
  environment variables are available.

Boundary:
  The skill teaches agents to use gx bd, which forwards to Ghostex's bundled
  Beads CLI. Ghostex does not invent a second project-board API.
`;
}

function agentOrchestrationUsage() {
  /**
   * CDXC:AgentOrchestration 2026-05-27-07:15:
   * The orchestration skill is intentionally lightweight: it installs guidance
   * that tells agents to read `ghostex --help` first, then use the supported
   * Ghostex CLI commands for session creation, cross-agent messaging, status
   * checks, and last-lines reads.
   */
  return `Ghostex Agent Orchestration - install the agent skill for Ghostex CLI coordination

Usage:
  gx agent-orchestration --help
  gx agent-orchestration install-skill [--json]

Agent skill:
  Use $ghostex-agent-orchestration when a task needs Ghostex session or agent
  coordination from the CLI.

What the skill teaches:
  Read ghostex --help first, then use commands such as sessions --json,
  create-session, create-agent, send-message, read-text --lines, focus, sleep,
  wake, kill, wait-for, and assert-card.

Boundary:
  Use Ghostex CLI commands instead of raw zmx/tmux control when coordinating
  panes inside Ghostex.
`;
}

function computerUseUsage() {
  /**
   * CDXC:ComputerAgentControl 2026-05-27-06:58:
   * Ghostex needs an agent-facing Computer Use entry point that maps Desktop
   * Control setup to the underlying Cua Driver workflow. Keep this help focused
   * on installing `$ghostex-computer-use`; native app automation itself stays in
   * Cua Driver so agents do not invent a second desktop-control interface.
   */
  return `Ghostex Computer Use - install the agent skill for native macOS app control

Usage:
  gx computer-use --help
  gx computer-use install-skill [--json]

Agent skill:
  Use $ghostex-computer-use when a task needs native macOS app automation.
  The skill is a Ghostex-named wrapper around $cua-driver, so agents get the
  Cua Driver workflow without requiring the user to remember the lower-level name.

Desktop Control requirements:
  Install Desktop Control from Ghostex setup or Settings > Integrations.
  Cua Driver must be installed, and macOS Accessibility plus Screen Recording
  permissions must be granted before desktop automation can work.

Boundary:
  Use $ghostex-computer-use for native macOS apps.
  Use $ghostex-browser-use and gx browser --help for embedded Ghostex browser panes.
`;
}

export {
  agentOrchestrationUsage,
  applyZehnAcceptAllArgs,
  browserUsage,
  buildSessionPickerModel,
  buildSessionPickerRows,
  buildSessionAttachCommand,
  computerUseUsage,
  createCliSshForwardPlan,
  fetchGxserverSessionList,
  formatCompactSessionLine,
  generateTitleUsage,
  groupSessionsPreservingSidebarOrder,
  isFailedCliResult,
  manageBeadsUsage,
  moveSessionPickerSelection,
  parseArgs,
  parseCreateSession,
  parseEditPaths,
  parseOpenPaths,
  parseQuickTerminal,
  parseRename,
  parseVsCodePathPosition,
  readAndroidReadinessSettings,
  requestGxserverRpc,
  resolveBundledBeadsLaunchFromRoot,
  resolveGxserverServerTarget,
  resolveListedSessions,
  resolveGhostexTuiLaunchFromRoot,
  resolveZehnLaunchFromRoot,
  sendGxserverCliAction,
  serverUsage,
  usage,
};
