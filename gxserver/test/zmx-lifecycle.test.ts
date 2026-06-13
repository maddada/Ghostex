import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { tmpdir } from "node:os";
import {
  buildZmxAttachCommand,
  buildZmxExistsCommand,
  buildZmxHistoryCommand,
  buildZmxKillCommand,
  buildGxserverZmxChildEnvironment,
  buildZmxRunCommand,
  buildZmxSendCommand,
  buildZmxShellProviderCommand,
  decideStartupTextDisposition,
  GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES,
  parseZmxSessionProcessIdentities,
  probeZmxSession,
  runZshScript,
  selectStartupRestoreSessionIds,
  summarizeZmxChildEnvironmentSanitization,
} from "../src/zmx-lifecycle.js";
import type { GxserverSessionDomainState, GxserverZmxSessionName } from "../protocol/index.js";
import type { GxserverZmxCommandResult } from "../src/zmx-lifecycle.js";

test("zmx attach command preserves the renderer shell contract", () => {
  const command = buildZmxAttachCommand({
    cwd: "/repo/ghostex",
    globalSessionRef: "S1a:P3a91:G8v20",
    gxserverAuthTokenFile: "/Users/test/.ghostex/gxserver/auth/token",
    gxserverBaseUrl: "http://127.0.0.1:58744",
    gxserverProtocolVersion: 5,
    promptEditor: "monaco",
    sessionName: "S1a-P3a91-G8v20",
    title: "Agent task",
    zmxExecutablePath: "/Applications/Ghostex.app/Contents/Resources/Web/bin/zmx",
  });

  assert.match(command, /^\/bin\/zsh -lc '/);
  assert.match(command, /zmx_bin=.*\/Applications\/Ghostex\.app\/Contents\/Resources\/Web\/bin\/zmx/);
  assert.match(command, /unset .*GHOSTEX_NATIVE_SESSION_ID/);
  assert.match(command, /unset .*GHOSTEX_GLOBAL_SESSION_REF/);
  assert.match(command, /unset .*ZMX_SESSION ZMX_SESSION_PREFIX/);
  assert.match(command, /export GHOSTEX_GLOBAL_SESSION_REF="\$zmx_global_session_ref"/);
  assert.match(command, /export GHOSTEX_SESSION_ID="\$zmx_session"/);
  assert.match(command, /export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="\$zmx_gxserver_auth_token_file"/);
  assert.match(command, /export GHOSTEX_GXSERVER_BASE_URL="\$zmx_gxserver_base_url"/);
  assert.match(command, /export GHOSTEX_GXSERVER_PROTOCOL_VERSION="\$zmx_gxserver_protocol_version"/);
  assert.match(command, /export GHOSTEX_ZMX_BIN="\$zmx_bin"/);
  assert.match(command, /S1a:P3a91:G8v20/);
  assert.match(command, /http:\/\/127\.0\.0\.1:58744/);
  assert.match(command, /\/Users\/test\/\.ghostex\/gxserver\/auth\/token/);
  assert.match(command, /zmx_prompt_editor_attach_args=/);
  assert.match(command, /--prompt-editor=monaco/);
  assert.match(command, /"\$zmx_bin" list --short/);
  assert.match(command, /\/bin\/zsh -lc "\$zmx_title_notice_command"/);
  assert.match(command, /\/bin\/zsh -lc "\$zmx_persistence_notice_command"/);
  assert.match(command, /cd "\$zmx_cwd" \|\| exit/);
  assert.match(command, /exec "\$zmx_bin" attach \$zmx_prompt_editor_attach_args "\$zmx_session"/);
  assert.doesNotMatch(command, /command -v zmx/);
});

test("zmx attach command defaults to gte when prompt editor capability is omitted", () => {
  /*
  CDXC:GxserverVerification 2026-06-11-18:24:
  zmx attach capability is explicit per current client. If a client does not ask
  for Monaco, the generated attach command must not infer it from environment so
  TUI, Android, iOS, and SSH attaches keep using gte.
  */
  const command = buildZmxAttachCommand({
    cwd: "/repo/ghostex",
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/bundle/zmx",
  });

  assert.match(command, /zmx_prompt_editor_attach_args=/);
  assert.doesNotMatch(command, /--prompt-editor=monaco/);
  assert.doesNotMatch(command, /GHOSTEX_PROMPT_EDITOR_BACKEND/);
});

test("startup text is queued only for missing provider sessions", () => {
  assert.equal(
    decideStartupTextDisposition({ providerState: "exists", startupText: "codex resume abc\n" }),
    "discardExistingProvider",
  );
  assert.equal(
    decideStartupTextDisposition({ providerState: "unknown", startupText: "codex resume abc\n" }),
    "discardUnknownProvider",
  );
  assert.equal(
    decideStartupTextDisposition({ providerState: "missing", startupText: "codex resume abc\n" }),
    "queueAfterTerminalReady",
  );
  assert.equal(decideStartupTextDisposition({ providerState: "missing", startupText: "" }), "none");
});

test("zmx existence probes distinguish exists, missing, and unknown", async () => {
  const exists = await probeZmxSession({
    runZsh: async () => ({ exitCode: 0, stderr: "", stdout: "" }),
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/fake/zmx",
  });
  const missing = await probeZmxSession({
    runZsh: async () => ({ exitCode: 1, stderr: "", stdout: "" }),
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/fake/zmx",
  });
  const unknown = await probeZmxSession({
    runZsh: async () => ({ exitCode: 127, stderr: "zmx broken", stdout: "" }),
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/fake/zmx",
  });

  assert.equal(exists.lifecycleState, "exists");
  assert.equal(missing.lifecycleState, "missing");
  assert.equal(unknown.lifecycleState, "unknown");
  assert.equal(unknown.error, "zmx broken");
});

test("zmx list command failure probes as unknown instead of missing", async () => {
  const tempDir = await mkdtemp(path.join(tmpdir(), "gxserver-zmx-probe-"));
  const zmxPath = path.join(tempDir, "zmx");
  try {
    await writeFile(
      zmxPath,
      `#!/bin/sh
if [ "$1" = "list" ] && [ "$2" = "--short" ]; then
  exit 1
fi
exit 64
`,
    );
    await chmod(zmxPath, 0o755);

    const probe = await probeZmxSession({
      runZsh: runZshCommand,
      sessionName: "S1a-P3a91-G8v20",
      zmxExecutablePath: zmxPath,
    });

    assert.equal(probe.lifecycleState, "unknown");
    assert.match(probe.error ?? "", /zmx list --short failed with exit 1/);
  } finally {
    await rm(tempDir, { force: true, recursive: true });
  }
});

test("successful zmx list without session probes as missing", async () => {
  const tempDir = await mkdtemp(path.join(tmpdir(), "gxserver-zmx-probe-"));
  const zmxPath = path.join(tempDir, "zmx");
  try {
    await writeFile(
      zmxPath,
      `#!/bin/sh
if [ "$1" = "list" ] && [ "$2" = "--short" ]; then
  printf '%s\\n' 'S1a-P3a91-G1111'
  exit 0
fi
exit 64
`,
    );
    await chmod(zmxPath, 0o755);

    const probe = await probeZmxSession({
      runZsh: runZshCommand,
      sessionName: "S1a-P3a91-G8v20",
      zmxExecutablePath: zmxPath,
    });

    assert.equal(probe.lifecycleState, "missing");
    assert.equal(probe.error, undefined);
  } finally {
    await rm(tempDir, { force: true, recursive: true });
  }
});

test("probe runner failures probe as unknown with error details", async () => {
  const error = new Error("spawn /bin/zsh ENOENT") as NodeJS.ErrnoException;
  error.code = "ENOENT";
  const probe = await probeZmxSession({
    runZsh: async () => {
      throw error;
    },
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/fake/zmx",
  });

  assert.equal(probe.lifecycleState, "unknown");
  assert.match(probe.error ?? "", /ENOENT/);
  assert.match(probe.error ?? "", /spawn \/bin\/zsh ENOENT/);
});

test("zmx process identity parser prefers live Codex child over unrelated agent processes", () => {
  const sessionName = "S90-P3lv0-G0p1k" as GxserverZmxSessionName;
  const identities = parseZmxSessionProcessIdentities({
    psOutput: `
81395     1 /bundle/zmx run S90-P3lv0-G0p1k -d --initial-command /bin/zsh -lic gx f
81396 81395 /bin/zsh -lic gx f
81557 81396 node /Applications/Ghostex.app/Contents/Resources/CLI/ghostex-cli.mjs f
81582 81557 /Applications/Ghostex.app/Contents/Resources/Web/bin/zehn --accept-all
82148 81582 node /Users/madda/.local/bin/codex --yolo resume 019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78
82149 82148 /Users/madda/.local/lib/codex --yolo resume 019eb8d0-d27b-7f30-b6d7-7a04ab8fae78
94784 93944 /Users/madda/.local/bin/claude --resume 303d77cf-4871-48da-871f-47782e834307
`.trim(),
    sessionNames: [sessionName],
    zmxListOutput: `  name=${sessionName}\tpid=81396\tclients=1\tcreated=1781219985\tstart_dir=/repo`,
  });
  assert.deepEqual(identities.get(sessionName), {
    agentId: "codex",
    agentSessionId: "019eb8d0-d27b-7f30-b6d7-7a04ab8fae78",
  });
});

test("zmx process identity parser classifies path-based Codex descendants without a visible resume id", () => {
  const sessionName = "S90-P3lv0-G8z6g" as GxserverZmxSessionName;
  const identities = parseZmxSessionProcessIdentities({
    psOutput: `
82603 82602 /bin/zsh -li
91171 82603 node /Users/person/.local/share/mise/installs/node/24.14.1/bin/codex --yolo resume
91172 91171 /Users/person/.local/share/mise/installs/node/24.14.1/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex --yolo resume
93898 91172 /Applications/Codex.app/Contents/Resources/cua_node/bin/node_repl
`.trim(),
    sessionNames: [sessionName],
    zmxListOutput: `  name=${sessionName}\tpid=82603\tclients=1\tcreated=1781251054\tstart_dir=/repo`,
  });

  assert.deepEqual(identities.get(sessionName), {
    agentId: "codex",
  });
});

test("zmx process identity parser ignores helper payload text that mentions another agent", () => {
  const sessionName = "S90-P3lv0-G8cl2" as GxserverZmxSessionName;
  const identities = parseZmxSessionProcessIdentities({
    psOutput: `
23572     1 -zsh
23754 23572 node /Users/person/.local/share/mise/installs/node/24.14.1/bin/codex --yolo
23755 23754 /Users/person/.local/share/mise/installs/node/24.14.1/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex --yolo
34225 23755 /Users/person/.codex/computer-use/Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/MacOS/SkyComputerUseClient turn-ended {"input-messages":["compare codex hotkeys with claude code"]}
`.trim(),
    sessionNames: [sessionName],
    zmxListOutput: `  name=${sessionName}\tpid=23572\tclients=1\tcreated=1781317239\tstart_dir=/repo`,
  });

  /*
  CDXC:GxserverSessionIdentity 2026-06-13-09:08:
  Codex helper clients may serialize user prompts and assistant summaries into their process argv. Agent detection must ignore that payload text so words like "claude" inside a completed Codex turn cannot relabel the active zmx session.
  */
  assert.deepEqual(identities.get(sessionName), {
    agentId: "codex",
  });
});

test("zmx process identity parser keeps the real agent when helper payload contains a resume command", () => {
  const sessionName = "S90-P3lv0-G9cla" as GxserverZmxSessionName;
  const claudeSessionId = "9970b270-b39f-4d63-a764-fa8d88083995";
  const identities = parseZmxSessionProcessIdentities({
    psOutput: `
41000     1 -zsh
41020 41000 /Users/person/.local/bin/claude --resume ${claudeSessionId}
41030 41020 /Users/person/.codex/computer-use/Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/MacOS/SkyComputerUseClient turn-ended {"last-assistant-message":"codex resume 019ebec8-0f6d-7e51-824c-981a501916fc"}
`.trim(),
    sessionNames: [sessionName],
    zmxListOutput: `  name=${sessionName}\tpid=41000\tclients=1\tcreated=1781317239\tstart_dir=/repo`,
  });

  assert.deepEqual(identities.get(sessionName), {
    agentId: "claude",
    agentSessionId: claudeSessionId,
  });
});

test("sleep and close kill commands use bundled zmx directly", () => {
  const command = buildZmxKillCommand({
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/bundle/zmx",
  });

  assert.match(command, /zmx_bin='\/bundle\/zmx'/);
  assert.match(command, /unset ZMX_SESSION ZMX_SESSION_PREFIX/);
  assert.match(command, /exec "\$zmx_bin" kill "\$zmx_session" --force/);
  assert.doesNotMatch(command, /command -v zmx/);
});

test("zmx session interaction commands use bundled zmx for history and raw input", () => {
  const history = buildZmxHistoryCommand({
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/bundle/zmx",
  });
  const send = buildZmxSendCommand({
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/bundle/zmx",
  });

  assert.match(history, /exec "\$zmx_bin" history "\$zmx_session"/);
  assert.match(send, /exec "\$zmx_bin" send "\$zmx_session"/);
  assert.doesNotMatch(send, /zmx_text=/);
  assert.doesNotMatch(`${history}\n${send}`, /command -v zmx/);
});

test("zmx run startup command is prefixed once for Atuin history ignore", () => {
  /*
  CDXC:GxserverVerification 2026-06-08-20:49:
  Provider-start restore must keep startup commands out of post-ready terminal input while preserving the user's shell after the agent exits. Assert the generated zmx run command starts an interactive login zsh, keeps the single Atuin-ignore prefix, and appends a login shell instead of execing the agent.
  */
  const command = buildZmxRunCommand({
    cwd: "/repo/ghostex",
    globalSessionRef: "S1a:P3a91:G8v20",
    gxserverAuthTokenFile: "/Users/test/.ghostex/gxserver/auth/token",
    gxserverBaseUrl: "http://127.0.0.1:58744",
    gxserverProtocolVersion: 5,
    sessionName: "S1a-P3a91-G8v20",
    startupText: "codex resume abc\r",
    zmxExecutablePath: "/bundle/zmx",
  });

  assert.match(command, /zmx_startup_text=' codex resume abc'/);
  assert.match(command, /ghostex_prompt_editor_home="\$\{GHOSTEX_HOME:-\$HOME\/\.ghostex\}"/);
  assert.match(command, /ghostex_prompt_editor_wrapper="\$ghostex_prompt_editor_home\/state\/prompt-editor"/);
  assert.match(command, /mkdir -p "\$\{ghostex_prompt_editor_wrapper:h\}"/);
  assert.match(command, /export EDITOR="\$ghostex_prompt_editor_wrapper"/);
  assert.match(command, /export GHOSTEX_PROMPT_EDITOR_BACKEND="\$\{GHOSTEX_PROMPT_EDITOR_BACKEND:-monaco\}"/);
  assert.match(command, /if \[ -n "\$\{GHOSTEX_ZMX_BIN:-\}" \] && \[ -x "\$\{GHOSTEX_ZMX_BIN:-\}" \]; then/);
  assert.match(command, /GHOSTEX_CLI_EXECUTABLE/);
  assert.match(command, /exec "\$GHOSTEX_CLI_EXECUTABLE" prompt-editor "\$@"/);
  assert.match(command, /exec ghostex prompt-editor "\$@"/);
  assert.match(command, /exec gte "\$@"/);
  assert.doesNotMatch(command, /\\\$\{/);
  assert.match(command, /zmx_startup_command='[\s\S]* codex resume abc\nexec \/bin\/zsh -li'/);
  assert.match(command, /export GHOSTEX_ZMX_BIN="\$zmx_bin"/);
  assert.match(command, /unset .*GHOSTEX_NATIVE_SESSION_ID/);
  assert.match(command, /export GHOSTEX_GLOBAL_SESSION_REF="\$zmx_global_session_ref"/);
  assert.match(command, /export GHOSTEX_SESSION_ID="\$zmx_session"/);
  assert.match(command, /exec "\$zmx_bin" run "\$zmx_session" -d --initial-command \/bin\/zsh -lic "\$zmx_startup_command"/);

  const alreadyPrefixed = buildZmxRunCommand({
    cwd: "/repo/ghostex",
    sessionName: "S1a-P3a91-G8v20",
    startupText: " codex resume abc\r",
    zmxExecutablePath: "/bundle/zmx",
  });
  assert.match(alreadyPrefixed, /zmx_startup_text=' codex resume abc'/);
  assert.doesNotMatch(alreadyPrefixed, /zmx_startup_command='  codex/);
});

test("zmx shell provider command installs the neutral prompt-editor wrapper before login shell", () => {
  /*
  CDXC:GxserverVerification 2026-06-11-18:24:
  Plain terminal providers can be created by non-desktop clients and later used
  from macOS/Electron. Assert the provider starts with the same neutral wrapper
  as agent providers without writing setup text into an already-live terminal.

  CDXC:GxserverVerification 2026-06-11-18:15:
  The provider script must expose expanded editor paths to the agent process.
  Escaped shell parameter syntax leaks literal `${...}` strings into EDITOR and
  makes Ctrl+G fail before Ghostex can choose Monaco or gte.
  */
  const command = buildZmxShellProviderCommand({
    cwd: "/repo/ghostex",
    globalSessionRef: "S1a:P3a91:G8v20",
    gxserverAuthTokenFile: "/Users/test/.ghostex/gxserver/auth/token",
    gxserverBaseUrl: "http://127.0.0.1:58744",
    gxserverProtocolVersion: 5,
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/bundle/zmx",
  });

  assert.match(command, /ghostex_prompt_editor_home="\$\{GHOSTEX_HOME:-\$HOME\/\.ghostex\}"/);
  assert.match(command, /zmx_shell_command='[\s\S]*ghostex_prompt_editor_wrapper="\$ghostex_prompt_editor_home\/state\/prompt-editor"/);
  assert.match(command, /mkdir -p "\$\{ghostex_prompt_editor_wrapper:h\}"/);
  assert.match(command, /export EDITOR="\$ghostex_prompt_editor_wrapper"/);
  assert.match(command, /export GHOSTEX_PROMPT_EDITOR_BACKEND="\$\{GHOSTEX_PROMPT_EDITOR_BACKEND:-monaco\}"/);
  assert.match(command, /exec "\$GHOSTEX_CLI_EXECUTABLE" prompt-editor "\$@"/);
  assert.match(command, /exec ghostex prompt-editor "\$@"/);
  assert.match(command, /exec gte "\$@"/);
  assert.doesNotMatch(command, /\\\$\{/);
  assert.match(command, /export GHOSTEX_GLOBAL_SESSION_REF="\$zmx_global_session_ref"/);
  assert.match(command, /export GHOSTEX_SESSION_ID="\$zmx_session"/);
  assert.match(command, /exec "\$zmx_bin" run "\$zmx_session" -d --initial-command \/bin\/zsh -lic "\$zmx_shell_command"/);
  assert.doesNotMatch(command, /--initial-command \/bin\/zsh -li$/);
});

test("zmx command runner caps output and pipes stdin without argv payloads", async () => {
  /*
  CDXC:GxserverVerification 2026-05-30-23:32:
  The subprocess runner regression covers the security boundary directly: output caps stop unbounded zmx history accumulation, and stdin support lets gxserver send payloads without placing user text in the shell argv script.
  */
  const stdin = await runZshScript("exec /bin/cat", {
    stdin: "hello from stdin",
    stdoutLimitBytes: GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES,
  });
  assert.equal(stdin.exitCode, 0);
  assert.equal(stdin.stdout, "hello from stdin");

  const capped = await runZshScript("while true; do printf 0123456789; done", {
    stdoutLimitBytes: 25,
    timeoutMs: 5_000,
  });
  assert.equal(capped.exitCode, 125);
  assert.equal(capped.stdout, "0123456789012345678901234");
  assert.equal(capped.stdoutTruncated, true);
  assert.equal(capped.stdoutLimitBytes, 25);
  assert.match(capped.stderr, /stdout exceeded 25 bytes/);
});

test("zmx child environment strips inherited terminal, color, macOS launch, and session identity variables", () => {
  /*
  CDXC:GxserverZmxLifecycle 2026-06-07-00:38:
  Forked agent providers must stay color-capable even when gxserver starts from an agent terminal with NO_COLOR-style variables. Assert the zmx runner removes those inherited keys before spawning the shell that starts zmx and agent CLIs.

  CDXC:GxserverZmxLifecycle 2026-06-08-05:49:
  Cursor-agent and other keychain-backed CLIs launched by zmx providers must not inherit the Ghostex app's LaunchServices/XPC identity. Assert the runner removes those keys while preserving login security-session context and terminal authentication sockets needed by normal developer workflows.

  CDXC:PromptEditor 2026-06-09-21:50:
  zmx providers must not inherit stale Ghostex local/native session identity
  from the terminal that launched gxserver. Assert those keys are stripped while
  provider scripts re-export the current global session identity explicitly.

  CDXC:GxserverZmxLifecycle 2026-06-10-23:19:
  Command panes can start zmx providers through gxserver before native Ghostty creates the attach surface. Assert gxserver replaces inherited TERM=dumb and stale terminal identity with a terminal-capable environment before zmx execs provider shells, using xterm-ghostty only when matching bundled TERMINFO is available.
  */
  const preservedEnvironmentKeys = ["SECURITYSESSIONID", "SSH_AUTH_SOCK"] as const;
  const sessionIdentityEnvironmentKeys = [
    "GHOSTEX_GLOBAL_SESSION_REF",
    "GHOSTEX_NATIVE_SESSION_ID",
    "GHOSTEX_SESSION_ID",
    "GHOSTEX_WORKSPACE_ID",
    "VSMUX_SESSION_ID",
    "ZMX_SESSION",
  ] as const;
  const terminalEnvironmentKeys = [
    "COLORTERM",
    "TERM",
    "TERMINFO",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
  ] as const;
  const environmentKeys = [
    "ANSI_COLORS_DISABLED",
    "LaunchInstanceID",
    "NO_COLOR",
    "NODE_DISABLE_COLORS",
    "XPC_FLAGS",
    "XPC_SERVICE_NAME",
    "__CFBundleIdentifier",
    ...sessionIdentityEnvironmentKeys,
    ...terminalEnvironmentKeys,
    ...preservedEnvironmentKeys,
  ] as const;
  const environment: NodeJS.ProcessEnv = {};
  for (const key of environmentKeys) {
    environment[key] = "present";
  }
  environment.COLORTERM = "false";
  environment.GHOSTTY_RESOURCES_DIR = "/Applications/Ghostex.app/Contents/Resources/ghostty";
  environment.PATH = "/usr/bin:/bin";
  environment.TERM = "dumb";
  environment.TERMINFO = "/stale/terminfo";
  environment.TERM_PROGRAM = "not-ghostty";
  environment.TERM_PROGRAM_VERSION = "stale";

  assert.deepEqual(summarizeZmxChildEnvironmentSanitization(environment), {
    colorDisablingKeyCount: 3,
    macosLaunchIdentityKeyCount: 4,
    preservedSshAuthSock: true,
    sessionIdentityKeyCount: 6,
    strippedKeyCount: 18,
    terminalEnvironmentKeyCount: 5,
  });
  const sanitized = buildGxserverZmxChildEnvironment(environment);
  for (const key of environmentKeys) {
    if (
      preservedEnvironmentKeys.includes(key as (typeof preservedEnvironmentKeys)[number]) ||
      terminalEnvironmentKeys.includes(key as (typeof terminalEnvironmentKeys)[number])
    ) {
      continue;
    }
    assert.equal(sanitized[key], undefined);
  }
  assert.equal(sanitized.COLORTERM, "truecolor");
  assert.equal(sanitized.GHOSTTY_RESOURCES_DIR, "/Applications/Ghostex.app/Contents/Resources/ghostty");
  assert.equal(sanitized.PATH, "/usr/bin:/bin");
  assert.equal(sanitized.SECURITYSESSIONID, "present");
  assert.equal(sanitized.SSH_AUTH_SOCK, "present");
  assert.equal(sanitized.TERM, "xterm-ghostty");
  assert.equal(sanitized.TERMINFO, "/Applications/Ghostex.app/Contents/Resources/terminfo");
  assert.equal(sanitized.TERM_PROGRAM, "ghostty");
  assert.equal(sanitized.TERM_PROGRAM_VERSION, undefined);

  const sanitizedWithoutGhosttyResources = buildGxserverZmxChildEnvironment({
    TERM: "dumb",
    TERMINFO: "/stale/terminfo",
  });
  assert.equal(sanitizedWithoutGhosttyResources.TERM, "xterm-256color");
  assert.equal(sanitizedWithoutGhosttyResources.TERMINFO, undefined);
});

test("startup restore selects active visible sessions, not every stored session", () => {
  const session = (sessionId: string, lifecycleState: GxserverSessionDomainState["lifecycleState"]) =>
    ({ kind: "agent", lifecycleState, sessionId }) as Pick<
      GxserverSessionDomainState,
      "kind" | "lifecycleState" | "sessionId"
    >;

  assert.deepEqual(
    selectStartupRestoreSessionIds({
      activeProjectId: "P3a91",
      projects: [
        {
          projectId: "P3a91",
          sessions: [session("G8v20", "running"), session("G1z99", "running"), session("G2abc", "sleeping")],
        },
        {
          projectId: "P4b22",
          sessions: [session("G3def", "running")],
        },
      ],
      visibleSessionIdsByProjectId: new Map([
        ["P3a91", ["G8v20", "G2abc"]],
        ["P4b22", ["G3def"]],
      ]),
    }),
    ["G8v20"],
  );
});

test("zmx exists command does not use PATH zmx", () => {
  const command = buildZmxExistsCommand({
    sessionName: "S1a-P3a91-G8v20",
    zmxExecutablePath: "/bundle/zmx",
  });

  assert.match(command, /"\$zmx_bin" list --short/);
  assert.doesNotMatch(command, /command -v zmx/);
});

function runZshCommand(script: string): Promise<GxserverZmxCommandResult> {
  return new Promise((resolve, reject) => {
    const child = spawn("/bin/zsh", ["-lc", script], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      resolve({
        exitCode: code ?? 1,
        stderr: Buffer.concat(stderr).toString("utf8").trim(),
        stdout: Buffer.concat(stdout).toString("utf8").trim(),
      });
    });
  });
}
