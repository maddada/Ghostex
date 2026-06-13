import { spawn } from "node:child_process";
import { stat } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import type {
  GxserverProviderKillResult,
  GxserverProviderLifecycleState,
  GxserverProviderProbeResult,
  GxserverSessionDomainState,
  GxserverSessionId,
  GxserverZmxSessionName,
} from "../protocol/index.js";
import {
  normalizeCodexSessionId,
  normalizeText,
  type GxserverResolvedSessionIdentity,
} from "./session-presentation/identity.js";

export interface GxserverZmxCommandResult {
  exitCode: number;
  stderr: string;
  stderrLimitBytes?: number;
  stderrTruncated?: boolean;
  stdout: string;
  stdoutLimitBytes?: number;
  stdoutTruncated?: boolean;
}

export interface GxserverZmxCommandOptions {
  stderrLimitBytes?: number;
  stdin?: string;
  stdoutLimitBytes?: number;
  timeoutMs?: number;
}

export type GxserverZmxCommandRunner = (script: string, options?: GxserverZmxCommandOptions) => Promise<GxserverZmxCommandResult>;
export type GxserverCwdExists = (cwd: string) => Promise<boolean>;
export type GxserverZmxProcessIdentity = GxserverResolvedSessionIdentity;
export type GxserverZmxProcessIdentityReader = (input: {
  runZsh?: GxserverZmxCommandRunner;
  sessionNames: readonly GxserverZmxSessionName[];
  zmxExecutablePath: string;
}) => Promise<ReadonlyMap<GxserverZmxSessionName, GxserverZmxProcessIdentity>>;

/*
CDXC:GxserverZmxLifecycle 2026-05-30-19:37:
Provider probe execution errors, non-existence-command exits, and command timeouts must produce provider lifecycle `unknown` with an error. Only a completed zmx list that lacks the exact session name is allowed to report `missing`.

CDXC:GxserverSessionIO 2026-05-30-23:32:
gxserver session reads and sends run through zmx subprocesses with explicit byte limits. History output is capped before stdout can grow without bound, and send payloads go through stdin so valid JSON requests do not become oversized `/bin/zsh -lc` argv strings.
*/
const ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS = 5_000;
export const GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES = 512 * 1024;
export const GXSERVER_ZMX_COMMAND_STDERR_LIMIT_BYTES = 64 * 1024;
export const GXSERVER_ZMX_HISTORY_STDOUT_LIMIT_BYTES = 256 * 1024;
export const GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES = 512 * 1024;
const GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES = 1024 * 1024;

const GXSERVER_COLOR_DISABLING_ENVIRONMENT_KEYS = [
  "ANSI_COLORS_DISABLED",
  "NO_COLOR",
  "NODE_DISABLE_COLORS",
] as const;
const GXSERVER_TERMINAL_ENVIRONMENT_KEYS = [
  "COLORTERM",
  "TERM",
  "TERMINFO",
  "TERM_PROGRAM",
  "TERM_PROGRAM_VERSION",
] as const;
const GXSERVER_ZMX_PROVIDER_COLORTERM = "truecolor";
const GXSERVER_ZMX_PROVIDER_GENERIC_TERM = "xterm-256color";
const GXSERVER_ZMX_PROVIDER_GHOSTTY_TERM = "xterm-ghostty";
const GXSERVER_ZMX_PROVIDER_TERM_PROGRAM = "ghostty";
const GXSERVER_MACOS_LAUNCH_IDENTITY_ENVIRONMENT_KEYS = [
  "LaunchInstanceID",
  "XPC_FLAGS",
  "XPC_SERVICE_NAME",
  "__CFBundleIdentifier",
] as const;
const GXSERVER_SESSION_IDENTITY_ENVIRONMENT_KEYS = [
  "GHOSTEX_AGENT",
  "GHOSTEX_GLOBAL_SESSION_REF",
  "GHOSTEX_GXSERVER_AUTH_TOKEN_FILE",
  "GHOSTEX_GXSERVER_BASE_URL",
  "GHOSTEX_GXSERVER_PROTOCOL_VERSION",
  "GHOSTEX_NATIVE_SESSION_ID",
  "GHOSTEX_SESSION_ID",
  "GHOSTEX_SESSION_STATE_FILE",
  "GHOSTEX_WORKSPACE_ID",
  "GHOSTEX_WORKSPACE_ROOT",
  "VSMUX_AGENT",
  "VSMUX_SESSION_ID",
  "VSMUX_SESSION_STATE_FILE",
  "VSMUX_WORKSPACE_ID",
  "VSMUX_WORKSPACE_ROOT",
  "ZMX_SESSION",
  "ZMX_SESSION_PREFIX",
  "ghostex_AGENT",
  "ghostex_SESSION_ID",
  "ghostex_SESSION_STATE_FILE",
  "ghostex_WORKSPACE_ID",
  "ghostex_WORKSPACE_ROOT",
] as const;

export interface GxserverZmxChildEnvironmentSanitizationSummary {
  colorDisablingKeyCount: number;
  macosLaunchIdentityKeyCount: number;
  preservedSshAuthSock: boolean;
  sessionIdentityKeyCount: number;
  strippedKeyCount: number;
  terminalEnvironmentKeyCount: number;
}

export interface GxserverZmxAttachCommandInput {
  cwd: string;
  globalSessionRef?: string;
  gxserverAuthTokenFile?: string;
  gxserverBaseUrl?: string;
  gxserverProtocolVersion?: number;
  promptEditor?: "monaco";
  sessionName: GxserverZmxSessionName;
  title?: string;
  zmxExecutablePath: string;
}

export interface GxserverZmxRunCommandInput {
  cwd: string;
  globalSessionRef?: string;
  gxserverAuthTokenFile?: string;
  gxserverBaseUrl?: string;
  gxserverProtocolVersion?: number;
  promptEditor?: "monaco";
  sessionName: GxserverZmxSessionName;
  startupText: string;
  zmxExecutablePath: string;
}

export type GxserverZmxShellProviderCommandInput = Omit<GxserverZmxRunCommandInput, "startupText">;

export interface GxserverZmxStartupTextDecisionInput {
  providerState: GxserverProviderLifecycleState;
  startupText?: string;
}

export interface GxserverStartupRestoreSelectionInput {
  activeProjectId?: string;
  projects: readonly {
    projectId: string;
    sessions: readonly Pick<GxserverSessionDomainState, "kind" | "lifecycleState" | "sessionId">[];
  }[];
  visibleSessionIdsByProjectId: ReadonlyMap<string, readonly string[]>;
}

/*
CDXC:GxserverZmxLifecycle 2026-05-30-14:48:
gxserver owns zmx lifecycle decisions without owning daemon lifetime. Starting or stopping the gxserver control plane must not signal provider sessions; zmx is resolved and executed only for explicit attach, wake, probe, sleep, or kill requests.

CDXC:GxserverZmxLifecycle 2026-05-30-14:48:
The renderer still receives one `/bin/zsh -lc` attach command string. That script resolves the bundled zmx path, clears inherited zmx client/session identity, checks existing sessions, prints title context for existing sessions, prints the persistence notice for new sessions, changes to the saved cwd, then execs direct `zmx attach` so startup/resume text remains outside the attach script.

CDXC:PromptEditor 2026-05-31-11:58:
New zmx provider sessions need canonical gxserver identity in their launch environment so prompt-editor wrappers can address S:P:G sessions without assuming a single connected server. Attach scripts export only stable server/session identity here; client-specific Monaco vs gte routing remains a wrapper/runtime decision.

CDXC:PromptEditor 2026-06-06-16:40:
Desktop render surfaces must advertise Monaco support to zmx at attach time only when their runtime terminal environment selected the Monaco backend. Missing attach capability defaults to gte, so mobile, TUI, and SSH clients cannot inherit stale macOS app prompt-editor markers from an existing shell.

CDXC:GxserverSessionTitle 2026-06-04-04:05:
Agent hooks running inside server-created zmx sessions must report identity and first prompts back to gxserver even when no macOS state file exists. Export the gxserver base URL, protocol version, global session ref, and auth token file path so hooks can call the authenticated session-state API without embedding the bearer token in attach/run command text.

CDXC:GxserverSessionIO 2026-06-06-16:58:
Server-owned zmx run startup commands can execute without the macOS sidebar queue. Preserve or add a single leading shell-history ignore space before passing startup text to the interactive zsh command so automated resume/fork/launch commands do not enter Atuin history.

CDXC:PromptEditor 2026-06-07-08:09:
Prompt-editor capability checks must use the same bundled zmx binary that
gxserver uses for attach/run. Export GHOSTEX_ZMX_BIN into zmx shells so Ctrl+G
does not resolve a stale PATH zmx before deciding whether Monaco is available.

CDXC:GenerateTitleSkill 2026-06-12-04:10:
Generate-title and other self-session agent workflows may run inside plain zmx
shells that only know the provider session name. Export GHOSTEX_SESSION_ID from
the current zmx provider name during attach/run launches so legacy skill copies
do not report missing-session, while GHOSTEX_GLOBAL_SESSION_REF remains the
preferred exact selector when available.
*/

export function buildZmxAttachCommand(input: GxserverZmxAttachCommandInput): string {
  const persistenceNoticeCommand = persistenceNoticeShellCommand(input.sessionName);
  const titleNoticeCommand = sessionTitleShellCommand(input.title);
  const promptEditorAttachArgs = input.promptEditor === "monaco" ? "--prompt-editor=monaco" : "";
  const script = `
zmx_session=${shellQuote(input.sessionName)}
zmx_cwd=${shellQuote(input.cwd)}
zmx_global_session_ref=${shellQuote(input.globalSessionRef ?? "")}
zmx_gxserver_auth_token_file=${shellQuote(input.gxserverAuthTokenFile ?? "")}
zmx_gxserver_base_url=${shellQuote(input.gxserverBaseUrl ?? "")}
zmx_gxserver_protocol_version=${shellQuote(String(input.gxserverProtocolVersion ?? ""))}
zmx_persistence_notice_command=${shellQuote(persistenceNoticeCommand)}
zmx_title_notice_command=${shellQuote(titleNoticeCommand)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
zmx_prompt_editor_attach_args=${shellQuote(promptEditorAttachArgs)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
${zmxSessionIdentityResetShellCommand()}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
if "$zmx_bin" list --short 2>/dev/null | grep -F -x -- "$zmx_session" >/dev/null 2>&1; then
  if [ -n "$zmx_title_notice_command" ]; then
    /bin/zsh -lc "$zmx_title_notice_command"
  fi
  exec "$zmx_bin" attach $zmx_prompt_editor_attach_args "$zmx_session"
fi
if [ -n "$zmx_persistence_notice_command" ]; then
  /bin/zsh -lc "$zmx_persistence_notice_command"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" attach $zmx_prompt_editor_attach_args "$zmx_session"
`.trim();
  return `/bin/zsh -lc ${shellQuote(script)}`;
}

export function buildZmxKillCommand(input: {
  sessionName: GxserverZmxSessionName;
  zmxExecutablePath: string;
}): string {
  const script = `
zmx_session=${shellQuote(input.sessionName)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" kill "$zmx_session" --force
`.trim();
  return script;
}

export function buildZmxHistoryCommand(input: {
  sessionName: GxserverZmxSessionName;
  zmxExecutablePath: string;
}): string {
  return `
zmx_session=${shellQuote(input.sessionName)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" history "$zmx_session"
`.trim();
}

export function buildZmxSendCommand(input: {
  sessionName: GxserverZmxSessionName;
  zmxExecutablePath: string;
}): string {
  return `
zmx_session=${shellQuote(input.sessionName)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" send "$zmx_session"
`.trim();
}

export function buildZmxRunCommand(input: GxserverZmxRunCommandInput): string {
  /*
  CDXC:GxserverRemoteAgents 2026-06-03-02:18:
  Remote gxserver can create agent sessions without a local macOS renderer, so it also needs a bounded way to start the backing zmx provider. This command launches only the named session with server-owned startup text, cwd, bundled zmx, and gxserver identity; it does not expose generic process execution.

  CDXC:GxserverSessionRestore 2026-06-08-20:49:
  macOS zmx restore/startup now runs through provider creation instead of post-ready terminal input. Keep the provider shell alive after the foreground agent exits by appending a login shell to the startup command; do not `exec` the agent command because ending the agent must return the user to a shell rather than close the terminal.

  CDXC:GxserverSessionRestore 2026-06-08-21:18:
  `zmx run` normally sends command text through the provider PTY, which makes large restore wrappers appear in scrollback. Use zmx's initial-command mode so the missing provider execs the zsh wrapper as its first process instead of typing it into a shell.
  */
  const startupCommand = withAtuinIgnoredShellHistoryPrefix(input.startupText.replace(/[\r\n]+$/u, ""));
  const providerShellCommand = `${zmxProviderPromptEditorSetupShellCommand()}\n${startupCommand}\nexec /bin/zsh -li`;
  return `
zmx_session=${shellQuote(input.sessionName)}
zmx_cwd=${shellQuote(input.cwd)}
zmx_global_session_ref=${shellQuote(input.globalSessionRef ?? "")}
zmx_gxserver_auth_token_file=${shellQuote(input.gxserverAuthTokenFile ?? "")}
zmx_gxserver_base_url=${shellQuote(input.gxserverBaseUrl ?? "")}
zmx_gxserver_protocol_version=${shellQuote(String(input.gxserverProtocolVersion ?? ""))}
zmx_startup_text=${shellQuote(startupCommand)}
zmx_startup_command=${shellQuote(providerShellCommand)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
if [ -z "$zmx_startup_text" ]; then
  printf '%s\\n' 'gxserver startSessionProvider requires startup text.' >&2
  exit 64
fi
${zmxSessionIdentityResetShellCommand()}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" run "$zmx_session" -d --initial-command /bin/zsh -lic "$zmx_startup_command"
`.trim();
}

export function buildZmxShellProviderCommand(input: GxserverZmxShellProviderCommandInput): string {
  /*
  CDXC:GxserverZmxLifecycle 2026-06-09-09:53:
  TUI, CLI, and mobile clients can create plain terminal sessions before any macOS renderer exists. A missing blank terminal must still get a real detached zmx provider through gxserver so the daemon can persist providerState=exists, publish the presentation row, and keep every client sidebar in sync before the interactive attach process takes over.

  CDXC:PromptEditor 2026-06-11-18:24:
  Plain zmx providers also need the neutral prompt-editor wrapper because a shell
  created first by TUI/mobile can later run an agent CLI from macOS/Electron.
  Install the wrapper through zmx initial-command before the login shell starts,
  preserving the gxserver restore rule that no setup text is pasted into a live
  terminal.
  */
  const providerShellCommand = `${zmxProviderPromptEditorSetupShellCommand()}\nexec /bin/zsh -li`;
  return `
zmx_session=${shellQuote(input.sessionName)}
zmx_cwd=${shellQuote(input.cwd)}
zmx_global_session_ref=${shellQuote(input.globalSessionRef ?? "")}
zmx_gxserver_auth_token_file=${shellQuote(input.gxserverAuthTokenFile ?? "")}
zmx_gxserver_base_url=${shellQuote(input.gxserverBaseUrl ?? "")}
zmx_gxserver_protocol_version=${shellQuote(String(input.gxserverProtocolVersion ?? ""))}
zmx_shell_command=${shellQuote(providerShellCommand)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
${zmxSessionIdentityResetShellCommand()}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" run "$zmx_session" -d --initial-command /bin/zsh -lic "$zmx_shell_command"
`.trim();
}

function zmxProviderPromptEditorSetupShellCommand(): string {
  /*
  CDXC:PromptEditor 2026-06-11-18:24:
  gxserver-created zmx providers may start before any desktop renderer attaches,
  and the provider can later be driven by macOS/Electron, TUI, Android, or iOS.
  Export a neutral Ghostex prompt-editor wrapper before the agent startup command
  runs; the wrapper asks zmx for the current leader client's advertised Monaco
  capability, so the same long-lived session opens Monaco from desktop attaches
  and gte from clients that did not advertise Monaco. Keep this in the zmx
  initial-command shell, not terminalReady startup text, so restore wrappers are
  never pasted into an already-live terminal.

  CDXC:PromptEditor 2026-06-11-18:15:
  The generated provider script must contain real zsh parameter expansions, not
  literal escaped strings. Agent CLIs execute EDITOR as argv[0], so exporting
  `${GHOSTEX_HOME:-...}/state/prompt-editor` literally makes Ctrl+G fail before
  Ghostex can route Monaco vs gte.
  */
  return `
ghostex_prompt_editor_home="\${GHOSTEX_HOME:-$HOME/.ghostex}"
ghostex_prompt_editor_wrapper="$ghostex_prompt_editor_home/state/prompt-editor"
mkdir -p "\${ghostex_prompt_editor_wrapper:h}" 2>/dev/null || true
cat > "$ghostex_prompt_editor_wrapper" <<'__GHOSTEX_PROMPT_EDITOR_WRAPPER__'
#!/bin/zsh
if [ -n "\${GHOSTEX_ZMX_BIN:-}" ] && [ -x "\${GHOSTEX_ZMX_BIN:-}" ]; then
  export GHOSTEX_ZMX_BIN
fi
if [ -n "\${GHOSTEX_CLI_EXECUTABLE:-}" ] && [ -x "\${GHOSTEX_CLI_EXECUTABLE:-}" ]; then
  exec "$GHOSTEX_CLI_EXECUTABLE" prompt-editor "$@"
fi
if command -v ghostex >/dev/null 2>&1; then
  exec ghostex prompt-editor "$@"
fi
exec gte "$@"
__GHOSTEX_PROMPT_EDITOR_WRAPPER__
chmod 755 "$ghostex_prompt_editor_wrapper" 2>/dev/null || true
export EDITOR="$ghostex_prompt_editor_wrapper"
export VISUAL="$ghostex_prompt_editor_wrapper"
export GHOSTEX_PROMPT_EDITOR_BACKEND="\${GHOSTEX_PROMPT_EDITOR_BACKEND:-monaco}"
export GHOSTEX_PROMPT_EDITING_ENABLED=1
`.trim();
}

function withAtuinIgnoredShellHistoryPrefix(text: string): string {
  const trimmedRight = text.trimEnd();
  if (!trimmedRight.trim()) {
    return "";
  }
  return trimmedRight.startsWith(" ") ? trimmedRight : ` ${trimmedRight.trimStart()}`;
}

export function buildZmxExistsCommand(input: {
  sessionName: GxserverZmxSessionName;
  zmxExecutablePath: string;
}): string {
  /*
  CDXC:GxserverZmxLifecycle 2026-05-30-19:37:
  A failed `zmx list --short` means provider state is unknown, not missing. Capture the list exit before matching session names so transient zmx failures cannot make live sessions look absent and trigger restore/recreate behavior.
  */
  return `
zmx_session=${shellQuote(input.sessionName)}
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
zmx_sessions=$("$zmx_bin" list --short)
zmx_list_status=$?
if [ "$zmx_list_status" -ne 0 ]; then
  printf '%s\\n' "zmx list --short failed with exit $zmx_list_status" >&2
  exit 2
fi
printf '%s\\n' "$zmx_sessions" | grep -F -x -- "$zmx_session" >/dev/null 2>&1
`.trim();
}

export function buildZmxProcessSnapshotCommand(input: {
  zmxExecutablePath: string;
}): string {
  /*
  CDXC:GxserverSessionIdentity 2026-06-12-04:41:
  Generic terminal launches such as helper commands can start a real agent after gxserver creates the zmx provider. Capture live zmx root PIDs plus their process descendants in one bounded server-side snapshot so gxserver can repair stale hook identity for every client without logging command text or pushing process parsing into macOS.
  */
  return `
zmx_bin=${shellQuote(input.zmxExecutablePath)}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
printf '%s\\n' '__GHOSTEX_ZMX_LIST__'
"$zmx_bin" list
printf '%s\\n' '__GHOSTEX_PS__'
ps -axo pid=,ppid=,command=
`.trim();
}

export async function readZmxSessionProcessIdentities(input: {
  runZsh?: GxserverZmxCommandRunner;
  sessionNames: readonly GxserverZmxSessionName[];
  zmxExecutablePath: string;
}): Promise<ReadonlyMap<GxserverZmxSessionName, GxserverZmxProcessIdentity>> {
  if (input.sessionNames.length === 0) {
    return new Map();
  }
  const result = await (input.runZsh ?? runZshScript)(buildZmxProcessSnapshotCommand(input), {
    stdoutLimitBytes: GXSERVER_ZMX_PROCESS_SNAPSHOT_STDOUT_LIMIT_BYTES,
  });
  if (result.exitCode !== 0) {
    return new Map();
  }
  const { psOutput, zmxListOutput } = parseZmxProcessSnapshotSections(result.stdout);
  return parseZmxSessionProcessIdentities({
    psOutput,
    sessionNames: input.sessionNames,
    zmxListOutput,
  });
}

export function parseZmxSessionProcessIdentities(input: {
  psOutput: string;
  sessionNames: readonly GxserverZmxSessionName[];
  zmxListOutput: string;
}): ReadonlyMap<GxserverZmxSessionName, GxserverZmxProcessIdentity> {
  const rootPidsBySessionName = parseZmxRootPids(input.zmxListOutput, input.sessionNames);
  const processes = parseProcessRows(input.psOutput);
  const childrenByParentPid = groupProcessesByParentPid(processes);
  const identities = new Map<GxserverZmxSessionName, GxserverZmxProcessIdentity>();
  for (const sessionName of input.sessionNames) {
    const rootPid = rootPidsBySessionName.get(sessionName);
    if (rootPid === undefined) {
      continue;
    }
    const identity = resolveProcessTreeAgentIdentity(rootPid, processes, childrenByParentPid);
    if (identity?.agentId) {
      identities.set(sessionName, identity);
    }
  }
  return identities;
}

export async function probeZmxSession(input: {
  now?: () => string;
  runZsh?: GxserverZmxCommandRunner;
  sessionName: GxserverZmxSessionName;
  zmxExecutablePath: string;
}): Promise<GxserverProviderProbeResult> {
  const probedAt = (input.now ?? (() => new Date().toISOString()))();
  let result: GxserverZmxCommandResult;
  try {
    result = await (input.runZsh ?? runZshScript)(
      buildZmxExistsCommand({
        sessionName: input.sessionName,
        zmxExecutablePath: input.zmxExecutablePath,
      }),
    );
  } catch (error) {
    return {
      error: zmxProbeThrownErrorMessage(error),
      lifecycleState: "unknown",
      probedAt,
      zmxName: input.sessionName,
    };
  }
  const lifecycleState: GxserverProviderLifecycleState =
    result.exitCode === 0 ? "exists" : result.exitCode === 1 ? "missing" : "unknown";
  const error = lifecycleState === "unknown" ? zmxProbeExitErrorMessage(result) : undefined;
  return {
    ...(error ? { error } : {}),
    lifecycleState,
    probedAt,
    zmxName: input.sessionName,
  };
}

export async function killZmxSession(input: {
  runZsh?: GxserverZmxCommandRunner;
  sessionName: GxserverZmxSessionName;
  zmxExecutablePath: string;
}): Promise<GxserverProviderKillResult> {
  let result: GxserverZmxCommandResult;
  try {
    result = await (input.runZsh ?? runZshScript)(
      buildZmxKillCommand({
        sessionName: input.sessionName,
        zmxExecutablePath: input.zmxExecutablePath,
      }),
    );
  } catch (error) {
    const message = zmxKillThrownErrorMessage(error);
    return {
      error: message,
      exitCode: 1,
      killed: false,
      stderr: message,
      stdout: "",
      zmxName: input.sessionName,
    };
  }
  return {
    ...(result.exitCode === 0 ? {} : { error: result.stderr || `exit-${result.exitCode}` }),
    exitCode: result.exitCode,
    killed: result.exitCode === 0,
    stderr: result.stderr,
    stdout: result.stdout,
    zmxName: input.sessionName,
  };
}

export function decideStartupTextDisposition(
  input: GxserverZmxStartupTextDecisionInput,
): "discardExistingProvider" | "discardUnknownProvider" | "none" | "queueAfterTerminalReady" {
  if (!input.startupText?.trim()) {
    return "none";
  }
  if (input.providerState === "exists") {
    return "discardExistingProvider";
  }
  if (input.providerState === "unknown") {
    return "discardUnknownProvider";
  }
  return "queueAfterTerminalReady";
}

export async function defaultCwdExists(cwd: string): Promise<boolean> {
  const trimmed = cwd.trim();
  if (!trimmed) {
    return false;
  }
  try {
    return (await stat(trimmed)).isDirectory();
  } catch {
    return false;
  }
}

export function selectStartupRestoreSessionIds(input: GxserverStartupRestoreSelectionInput): string[] {
  const activeProjectId = input.activeProjectId?.trim();
  if (!activeProjectId) {
    return [];
  }
  const activeProject = input.projects.find((project) => project.projectId === activeProjectId);
  if (!activeProject) {
    return [];
  }
  const visibleSessionIds = new Set(input.visibleSessionIdsByProjectId.get(activeProjectId) ?? []);
  return activeProject.sessions
    .filter((session) => visibleSessionIds.has(session.sessionId))
    .filter((session) => session.lifecycleState !== "sleeping" && session.lifecycleState !== "stopped")
    .map((session) => session.sessionId);
}

export function providerStatePatch(
  session: Pick<GxserverSessionDomainState, "providerState" | "zmxName">,
  probe: GxserverProviderProbeResult,
): GxserverSessionDomainState["providerState"] {
  return {
    ...session.providerState,
    killError: undefined,
    lifecycleState: probe.lifecycleState,
    probeError: probe.error,
    probedAt: probe.probedAt,
    zmxName: probe.zmxName,
  };
}

export function missingProviderStatePatch(
  session: Pick<GxserverSessionDomainState, "providerState" | "zmxName">,
  timestamp: string,
): GxserverSessionDomainState["providerState"] {
  return {
    ...session.providerState,
    killError: undefined,
    lifecycleState: "missing",
    probeError: undefined,
    probedAt: timestamp,
    zmxName: providerZmxSessionName(session),
  };
}

export function failedKillProviderStatePatch(
  session: Pick<GxserverSessionDomainState, "providerState" | "zmxName">,
  kill: GxserverProviderKillResult,
  timestamp: string,
): GxserverSessionDomainState["providerState"] {
  const error = (kill.error ?? kill.stderr) || `zmx kill command exited ${kill.exitCode}`;
  return {
    ...session.providerState,
    killError: error,
    lifecycleState: "unknown",
    probeError: error,
    probedAt: timestamp,
    zmxName: providerZmxSessionName(session),
  };
}

export function normalizeLifecycleReason(reason: unknown, fallback: string): string {
  const trimmed = typeof reason === "string" ? reason.trim() : "";
  return trimmed || fallback;
}

export function assertValidSessionName(value: string): asserts value is GxserverZmxSessionName {
  if (!/^[A-Za-z0-9_-]+-[A-Za-z0-9_-]+$/.test(value)) {
    throw new Error(`Invalid zmx session name: ${value}`);
  }
}

export function providerZmxSessionName(
  session: Pick<GxserverSessionDomainState, "providerState" | "zmxName">,
): GxserverZmxSessionName {
  /*
  CDXC:GxserverZmxLifecycle 2026-06-04-01:40:
  Reconnects should use one canonical server/project/session provider name. If the `S-P-G` zmx session is absent, `zmx attach` may create it and the wake path can run resume startup text there; do not route migrated sessions back to legacy `g-*` names.
  */
  return session.zmxName as GxserverZmxSessionName;
}

function persistenceNoticeShellCommand(sessionName: GxserverZmxSessionName): string {
  return `printf '%s\\n' ${shellQuote(`This session is using zmx persistence: ${sessionName}`)}`;
}

function sessionTitleShellCommand(title: string | undefined): string {
  const trimmedTitle = title?.trim() ?? "";
  if (!trimmedTitle) {
    return "";
  }
  return `printf '%s\\n' ${shellQuote(trimmedTitle)}`;
}

function zmxSessionIdentityResetShellCommand(): string {
  /*
  CDXC:PromptEditor 2026-06-09-21:50:
  zmx attach/run scripts must clear inherited local Ghostex session identity
  before exporting their current gxserver S:P:G identity. This prevents stale
  GHOSTEX_NATIVE_SESSION_ID and legacy VSMUX/Ghostex state keys from routing
  Ctrl+G prompt-editor saves back to the terminal that launched gxserver.
  */
  return `unset ${GXSERVER_SESSION_IDENTITY_ENVIRONMENT_KEYS.join(" ")}`;
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function zmxProbeExitErrorMessage(result: GxserverZmxCommandResult): string {
  const stderr = result.stderr.trim();
  if (stderr) {
    return stderr;
  }
  const stdout = result.stdout.trim();
  if (stdout) {
    return stdout;
  }
  return `zmx probe command exited ${result.exitCode}`;
}

function zmxProbeThrownErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    const code = typeof (error as NodeJS.ErrnoException).code === "string" ? ` ${(error as NodeJS.ErrnoException).code}` : "";
    return `${error.name || "Error"}${code}: ${error.message}`;
  }
  return `zmx probe command failed: ${String(error)}`;
}

function zmxKillThrownErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    const code = typeof (error as NodeJS.ErrnoException).code === "string" ? ` ${(error as NodeJS.ErrnoException).code}` : "";
    return `${error.name || "Error"}${code}: ${error.message}`;
  }
  return `zmx kill command failed: ${String(error)}`;
}

function parseZmxProcessSnapshotSections(stdout: string): { psOutput: string; zmxListOutput: string } {
  const zmxMarker = "__GHOSTEX_ZMX_LIST__";
  const psMarker = "__GHOSTEX_PS__";
  const zmxIndex = stdout.indexOf(zmxMarker);
  const psIndex = stdout.indexOf(psMarker);
  if (zmxIndex < 0 || psIndex < 0 || psIndex <= zmxIndex) {
    return { psOutput: "", zmxListOutput: "" };
  }
  return {
    psOutput: stdout.slice(psIndex + psMarker.length).trim(),
    zmxListOutput: stdout.slice(zmxIndex + zmxMarker.length, psIndex).trim(),
  };
}

function parseZmxRootPids(
  zmxListOutput: string,
  sessionNames: readonly GxserverZmxSessionName[],
): Map<GxserverZmxSessionName, number> {
  const wanted = new Set(sessionNames);
  const rootPids = new Map<GxserverZmxSessionName, number>();
  for (const line of zmxListOutput.split(/\r?\n/gu)) {
    const name = /(?:^|[\s→])name=([^\t\s]+)/u.exec(line)?.[1] as GxserverZmxSessionName | undefined;
    if (!name || !wanted.has(name)) {
      continue;
    }
    const pid = Number(/(?:^|\s)pid=(\d+)/u.exec(line)?.[1]);
    if (Number.isInteger(pid) && pid > 0) {
      rootPids.set(name, pid);
    }
  }
  return rootPids;
}

interface GxserverProcessRow {
  command: string;
  pid: number;
  ppid: number;
}

interface GxserverProcessIdentityCandidate {
  confidence: number;
  depth: number;
  identity: GxserverZmxProcessIdentity;
}

interface GxserverProcessIdentityObservation {
  confidence: number;
  identity: GxserverZmxProcessIdentity;
}

const GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT = 12;
const GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE = 300;
const GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE = 275;
const GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS = 25;
const GXSERVER_AGENT_PROCESS_WRAPPER_EXECUTABLES = new Set(["bun", "node"]);
const GXSERVER_AGENT_PROCESS_HELPER_EXECUTABLES = new Set([
  "gte",
  "node_repl",
  "prompt-editor",
  "skycomputeruseclient",
]);
const GXSERVER_AGENT_PROCESS_HELPER_COMMAND_MARKERS = [
  "skycomputeruseclient",
  "/node_repl",
  " node_repl",
];
const GXSERVER_AGENT_PROCESS_EXECUTABLES = new Map<string, string>([
  ["agy", "antigravity"],
  ["amp", "amp"],
  ["claude", "claude"],
  ["codebuddy", "codebuddy"],
  ["codex", "codex"],
  ["copilot", "copilot"],
  ["cursor-agent", "cursor"],
  ["droid", "droid"],
  ["gemini", "gemini"],
  ["grok", "grok"],
  ["hermes", "hermes-agent"],
  ["kiro-cli", "kiro"],
  ["omp", "omp"],
  ["opencode", "opencode"],
  ["pi", "pi"],
  ["qodercli", "qoder"],
  ["rovodev", "rovodev"],
]);

function parseProcessRows(psOutput: string): GxserverProcessRow[] {
  const rows: GxserverProcessRow[] = [];
  for (const line of psOutput.split(/\r?\n/gu)) {
    const match = /^\s*(\d+)\s+(\d+)\s+(.*)$/u.exec(line);
    if (!match) {
      continue;
    }
    rows.push({
      command: match[3] ?? "",
      pid: Number(match[1]),
      ppid: Number(match[2]),
    });
  }
  return rows;
}

function groupProcessesByParentPid(processes: readonly GxserverProcessRow[]): Map<number, GxserverProcessRow[]> {
  const grouped = new Map<number, GxserverProcessRow[]>();
  for (const processRow of processes) {
    const children = grouped.get(processRow.ppid) ?? [];
    children.push(processRow);
    grouped.set(processRow.ppid, children);
  }
  return grouped;
}

function resolveProcessTreeAgentIdentity(
  rootPid: number,
  processes: readonly GxserverProcessRow[],
  childrenByParentPid: ReadonlyMap<number, readonly GxserverProcessRow[]>,
): GxserverZmxProcessIdentity | undefined {
  const rowsByPid = new Map<number, GxserverProcessRow>(
    processes.map((processRow) => [processRow.pid, processRow] as const),
  );
  const candidates: GxserverProcessIdentityCandidate[] = [];
  const queue: Array<{ depth: number; pid: number }> = [{ depth: 0, pid: rootPid }];
  const seen = new Set<number>();
  for (let index = 0; index < queue.length; index += 1) {
    const item = queue[index];
    if (!item || seen.has(item.pid)) {
      continue;
    }
    seen.add(item.pid);
    const row = rowsByPid.get(item.pid);
    if (row) {
      const observation = resolveProcessCommandAgentIdentity(row.command);
      if (observation?.identity.agentId) {
        candidates.push({ depth: item.depth, ...observation });
      }
    }
    for (const child of childrenByParentPid.get(item.pid) ?? []) {
      queue.push({ depth: item.depth + 1, pid: child.pid });
    }
  }
  candidates.sort((left, right) => {
    const confidence = scoreProcessIdentityCandidate(right) - scoreProcessIdentityCandidate(left);
    if (confidence) {
      return confidence;
    }
    const idScore = Number(Boolean(right.identity.agentSessionId)) - Number(Boolean(left.identity.agentSessionId));
    return idScore || right.depth - left.depth;
  });
  return candidates[0]?.identity;
}

function scoreProcessIdentityCandidate(candidate: GxserverProcessIdentityCandidate): number {
  return candidate.confidence + (candidate.identity.agentSessionId ? GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS : 0);
}

function resolveProcessCommandAgentIdentity(command: string): GxserverProcessIdentityObservation | undefined {
  const tokens = splitProcessCommandPrefix(command, GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT);
  if (tokens.length === 0 || shouldIgnoreProcessCommandForAgentIdentity(command, tokens)) {
    return undefined;
  }
  /*
  CDXC:GxserverSessionIdentity 2026-06-13-09:08:
  Live zmx process identity must come from actual agent executables, not full descendant command text. Codex helper clients can include serialized prompts containing words such as "claude"; matching those payloads relabels Codex sessions for every client.
  */
  return (
    resolveAgentProcessInvocation(tokens, 0, GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE) ??
    resolveWrappedAgentProcessInvocation(tokens)
  );
}

function shouldIgnoreProcessCommandForAgentIdentity(command: string, tokens: readonly string[]): boolean {
  const executableName = normalizeProcessExecutableName(tokens[0]);
  if (executableName && GXSERVER_AGENT_PROCESS_HELPER_EXECUTABLES.has(executableName)) {
    return true;
  }
  const lowerCommand = command.toLowerCase();
  return GXSERVER_AGENT_PROCESS_HELPER_COMMAND_MARKERS.some((marker) => lowerCommand.includes(marker));
}

function resolveWrappedAgentProcessInvocation(tokens: readonly string[]): GxserverProcessIdentityObservation | undefined {
  const executableName = normalizeProcessExecutableName(tokens[0]);
  if (executableName === "env") {
    const invocationIndex = findEnvWrappedCommandIndex(tokens);
    if (invocationIndex !== undefined) {
      return resolveAgentProcessInvocation(tokens, invocationIndex, GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE);
    }
  }
  if (!executableName || !GXSERVER_AGENT_PROCESS_WRAPPER_EXECUTABLES.has(executableName)) {
    return undefined;
  }
  for (let index = 1; index < Math.min(tokens.length, 8); index += 1) {
    const observation = resolveAgentProcessInvocation(tokens, index, GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE);
    if (observation) {
      return observation;
    }
  }
  return undefined;
}

function resolveAgentProcessInvocation(
  tokens: readonly string[],
  executableIndex: number,
  confidence: number,
): GxserverProcessIdentityObservation | undefined {
  const agentId = inferAgentIdFromProcessExecutable(tokens, executableIndex);
  if (!agentId) {
    return undefined;
  }
  const agentSessionId = extractAgentProcessSessionId(agentId, tokens, executableIndex);
  return {
    confidence,
    identity: {
      agentId,
      ...(agentSessionId ? { agentSessionId } : {}),
    },
  };
}

function inferAgentIdFromProcessExecutable(tokens: readonly string[], executableIndex: number): string | undefined {
  const executableName = normalizeProcessExecutableName(tokens[executableIndex]);
  if (!executableName) {
    return undefined;
  }
  if (
    executableName === "acli" &&
    normalizeProcessToken(tokens[executableIndex + 1]) === "rovodev" &&
    normalizeProcessToken(tokens[executableIndex + 2]) === "run"
  ) {
    return "rovodev";
  }
  return GXSERVER_AGENT_PROCESS_EXECUTABLES.get(executableName);
}

function extractAgentProcessSessionId(
  agentId: string,
  tokens: readonly string[],
  executableIndex: number,
): string | undefined {
  const args = tokens.slice(executableIndex + 1);
  if (agentId === "codex") {
    for (let index = 0; index < args.length - 1; index += 1) {
      const token = normalizeProcessToken(args[index]);
      if (token === "resume" || token === "fork") {
        return normalizeAgentProcessSessionId(agentId, args[index + 1]);
      }
    }
    return undefined;
  }
  if (agentId === "claude" || agentId === "cursor") {
    return readAgentProcessFlagValue(agentId, args, "--resume");
  }
  if (agentId === "opencode") {
    return readAgentProcessFlagValue(agentId, args, "--session") ?? readAgentProcessFlagValue(agentId, args, "-s");
  }
  if (agentId === "pi" || agentId === "omp") {
    return readAgentProcessFlagValue(agentId, args, "--session");
  }
  if (agentId === "kiro") {
    return readAgentProcessFlagValue(agentId, args, "--resume-id");
  }
  return undefined;
}

function readAgentProcessFlagValue(
  agentId: string,
  args: readonly string[],
  flag: string,
): string | undefined {
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === flag) {
      return normalizeAgentProcessSessionId(agentId, args[index + 1]);
    }
    if (arg?.startsWith(`${flag}=`)) {
      return normalizeAgentProcessSessionId(agentId, arg.slice(flag.length + 1));
    }
  }
  return undefined;
}

function normalizeAgentProcessSessionId(agentId: string, value: unknown): string | undefined {
  const normalized = normalizeText(value)?.replace(/[;&|]+$/u, "");
  if (!normalized) {
    return undefined;
  }
  return agentId === "codex" ? (normalizeCodexSessionId(normalized) ?? normalized) : normalized;
}

function findEnvWrappedCommandIndex(tokens: readonly string[]): number | undefined {
  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (!token || token.startsWith("-") || /^[A-Za-z_][A-Za-z0-9_]*=/u.test(token)) {
      continue;
    }
    return index;
  }
  return undefined;
}

function splitProcessCommandPrefix(command: string, maxTokens: number): string[] {
  const tokens: string[] = [];
  let current = "";
  let quote: "\"" | "'" | undefined;
  let escaped = false;
  for (const char of command) {
    if (escaped) {
      current += char;
      escaped = false;
      continue;
    }
    if (char === "\\") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (char === quote) {
        quote = undefined;
      } else {
        current += char;
      }
      continue;
    }
    if (char === "\"" || char === "'") {
      quote = char;
      continue;
    }
    if (/\s/u.test(char)) {
      if (current) {
        tokens.push(current);
        if (tokens.length >= maxTokens) {
          return tokens;
        }
        current = "";
      }
      continue;
    }
    current += char;
  }
  if (current && tokens.length < maxTokens) {
    tokens.push(current);
  }
  return tokens;
}

function normalizeProcessExecutableName(token: unknown): string | undefined {
  const normalized = normalizeProcessToken(token);
  if (!normalized) {
    return undefined;
  }
  return basename(normalized).replace(/\.(?:cjs|cmd|exe|js|mjs|ts)$/u, "");
}

function normalizeProcessToken(token: unknown): string | undefined {
  const text = normalizeText(token)?.toLowerCase();
  return text?.replace(/^["'`]+|["'`]+$/gu, "");
}

export function buildGxserverZmxChildEnvironment(environment: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  /*
  CDXC:GxserverZmxLifecycle 2026-06-07-00:38:
  Fork/resume provider sessions are color-capable terminal workloads. gxserver may be launched from a shell or desktop app that has NO_COLOR set, so every zmx lifecycle child must strip color-disabling keys before zmx starts shells or agents.

  CDXC:GxserverZmxLifecycle 2026-06-08-05:49:
  Agent CLIs launched inside gxserver-created zmx providers must look like terminal children, not macOS app-extension children. Strip inherited LaunchServices/XPC identity keys from zmx lifecycle subprocesses so macOS keychain-backed tools do not bind credential access to the Ghostex GUI process.

  CDXC:PromptEditor 2026-06-09-21:50:
  gxserver can be launched from an existing Ghostex terminal, so inherited
  local/native session env may name the wrong pane. Strip those keys before zmx
  provider scripts export the current global S:P:G identity for prompt-editor
  hooks and return-focus routing.

  CDXC:GxserverZmxLifecycle 2026-06-10-23:19:
  gxserver-started zmx providers can launch before native Ghostty sanitizes terminal env, so inherited GUI or LaunchServices TERM=dumb must not reach provider login shells. Strip terminal identity keys and publish Ghostex's color-capable terminal env at the gxserver boundary; use xterm-ghostty only when bundled TERMINFO can be published.
  */
  const sanitized = { ...environment };
  for (const key of [
    ...GXSERVER_COLOR_DISABLING_ENVIRONMENT_KEYS,
    ...GXSERVER_TERMINAL_ENVIRONMENT_KEYS,
    ...GXSERVER_MACOS_LAUNCH_IDENTITY_ENVIRONMENT_KEYS,
    ...GXSERVER_SESSION_IDENTITY_ENVIRONMENT_KEYS,
  ]) {
    delete sanitized[key];
  }
  sanitized.COLORTERM = GXSERVER_ZMX_PROVIDER_COLORTERM;
  sanitized.TERM_PROGRAM = GXSERVER_ZMX_PROVIDER_TERM_PROGRAM;
  const ghosttyResourcesDir = normalizedEnvironmentValue(sanitized.GHOSTTY_RESOURCES_DIR);
  if (ghosttyResourcesDir) {
    sanitized.TERM = GXSERVER_ZMX_PROVIDER_GHOSTTY_TERM;
    sanitized.TERMINFO = join(dirname(ghosttyResourcesDir), "terminfo");
  } else {
    sanitized.TERM = GXSERVER_ZMX_PROVIDER_GENERIC_TERM;
  }
  return sanitized;
}

export function summarizeZmxChildEnvironmentSanitization(
  environment: NodeJS.ProcessEnv = process.env,
): GxserverZmxChildEnvironmentSanitizationSummary {
  const colorDisablingKeyCount = countPresentEnvironmentKeys(environment, GXSERVER_COLOR_DISABLING_ENVIRONMENT_KEYS);
  const macosLaunchIdentityKeyCount = countPresentEnvironmentKeys(
    environment,
    GXSERVER_MACOS_LAUNCH_IDENTITY_ENVIRONMENT_KEYS,
  );
  const sessionIdentityKeyCount = countPresentEnvironmentKeys(environment, GXSERVER_SESSION_IDENTITY_ENVIRONMENT_KEYS);
  const terminalEnvironmentKeyCount = countPresentEnvironmentKeys(environment, GXSERVER_TERMINAL_ENVIRONMENT_KEYS);
  return {
    colorDisablingKeyCount,
    macosLaunchIdentityKeyCount,
    preservedSshAuthSock: environment.SSH_AUTH_SOCK !== undefined,
    sessionIdentityKeyCount,
    strippedKeyCount:
      colorDisablingKeyCount + macosLaunchIdentityKeyCount + sessionIdentityKeyCount + terminalEnvironmentKeyCount,
    terminalEnvironmentKeyCount,
  };
}

function countPresentEnvironmentKeys(environment: NodeJS.ProcessEnv, keys: readonly string[]): number {
  return keys.reduce((count, key) => count + (environment[key] === undefined ? 0 : 1), 0);
}

function normalizedEnvironmentValue(value: string | undefined): string | undefined {
  const trimmed = value?.trim() ?? "";
  return trimmed || undefined;
}

export function runZshScript(script: string, options: GxserverZmxCommandOptions = {}): Promise<GxserverZmxCommandResult> {
  const stdoutLimitBytes = options.stdoutLimitBytes ?? GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES;
  const stderrLimitBytes = options.stderrLimitBytes ?? GXSERVER_ZMX_COMMAND_STDERR_LIMIT_BYTES;
  const timeoutMs = options.timeoutMs ?? ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS;
  return new Promise((resolve, reject) => {
    const child = spawn("/bin/zsh", ["-lc", script], {
      env: buildGxserverZmxChildEnvironment(),
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let stdoutTruncated = false;
    let stderrTruncated = false;
    let timedOut = false;
    let forceKillTimeout: NodeJS.Timeout | undefined;

    const terminate = (): void => {
      if (child.exitCode !== null || child.signalCode !== null) {
        return;
      }
      child.kill("SIGTERM");
      if (!forceKillTimeout) {
        forceKillTimeout = setTimeout(() => child.kill("SIGKILL"), 1_000);
        forceKillTimeout.unref();
      }
    };

    const timeout = setTimeout(() => {
      timedOut = true;
      terminate();
    }, timeoutMs);
    timeout.unref();

    child.stdin.on("error", () => undefined);
    child.stdin.end(options.stdin ?? "");
    child.stdout.on("data", (chunk: Buffer) => {
      if (stdoutTruncated) {
        return;
      }
      const remaining = stdoutLimitBytes - stdoutBytes;
      if (chunk.byteLength > remaining) {
        if (remaining > 0) {
          stdout.push(chunk.subarray(0, remaining));
        }
        stdoutBytes = stdoutLimitBytes;
        stdoutTruncated = true;
        terminate();
        return;
      }
      stdout.push(chunk);
      stdoutBytes += chunk.byteLength;
    });
    child.stderr.on("data", (chunk: Buffer) => {
      if (stderrTruncated) {
        return;
      }
      const remaining = stderrLimitBytes - stderrBytes;
      if (chunk.byteLength > remaining) {
        if (remaining > 0) {
          stderr.push(chunk.subarray(0, remaining));
        }
        stderrBytes = stderrLimitBytes;
        stderrTruncated = true;
        terminate();
        return;
      }
      stderr.push(chunk);
      stderrBytes += chunk.byteLength;
    });
    child.on("error", (error) => {
      clearTimeout(timeout);
      if (forceKillTimeout) {
        clearTimeout(forceKillTimeout);
      }
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timeout);
      if (forceKillTimeout) {
        clearTimeout(forceKillTimeout);
      }
      const stderrText = Buffer.concat(stderr).toString("utf8").trim();
      const limitMessages = [
        stdoutTruncated ? `zmx command stdout exceeded ${stdoutLimitBytes} bytes` : "",
        stderrTruncated ? `zmx command stderr exceeded ${stderrLimitBytes} bytes` : "",
      ].filter(Boolean);
      resolve({
        exitCode: timedOut ? 124 : stdoutTruncated || stderrTruncated ? 125 : (code ?? 1),
        stderr: timedOut
          ? [stderrText, `zmx lifecycle command timed out after ${timeoutMs}ms`, ...limitMessages]
              .filter(Boolean)
              .join("\n")
          : [stderrText, ...limitMessages].filter(Boolean).join("\n"),
        ...(stderrTruncated ? { stderrLimitBytes, stderrTruncated } : {}),
        stdout: Buffer.concat(stdout).toString("utf8").trim(),
        ...(stdoutTruncated ? { stdoutLimitBytes, stdoutTruncated } : {}),
      });
    });
  });
}
