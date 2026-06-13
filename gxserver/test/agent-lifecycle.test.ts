import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  applyAgentActivityTransition,
  buildAgentForkPlan,
  buildAgentLaunchPlan,
  buildAgentResumeCommand,
  buildAgentResumeFallbackCommand,
  buildAgentResumePlan,
  buildAgentResumeStartupText,
  createAgentForkSessionParams,
  getEffectiveAgentActivityState,
} from "../src/agents/lifecycle.js";
import type { GxserverProjectDomainState, GxserverSessionDomainState } from "../protocol/index.js";

const OPENCODE_ACCEPT_ALL_ENV = `OPENCODE_CONFIG_CONTENT='{"permission":"allow"}'`;

test("agent launch command generation preserves Accept All handling and delayed send metadata", () => {
  const codex = buildAgentLaunchPlan({
    acceptAllMode: "inherit",
    agentId: "codex",
    command: "codex --yolo --yolo",
    delayedSendDeadlineAt: "2026-05-30T16:00:00.000Z",
    firstUserMessage: "ship it",
    globalAcceptAllEnabled: true,
  });
  assert.equal(codex.command, "codex --yolo");
  assert.equal(codex.startupText, " codex --yolo\r");
  assert.equal(codex.startupTextDisposition, "queueAfterTerminalReady");
  assert.deepEqual(codex.delayedSend, {
    deadlineAt: "2026-05-30T16:00:00.000Z",
    disposition: "scheduled",
  });

  const antigravity = buildAgentLaunchPlan({
    agentId: "antigravity",
    command: "agy",
    globalAcceptAllEnabled: true,
  });
  assert.equal(antigravity.command, "agy --dangerously-skip-permissions");

  const opencode = buildAgentLaunchPlan({
    agentId: "opencode",
    command: "opencode --dangerously-skip-permissions --yolo",
    globalAcceptAllEnabled: true,
  });
  assert.equal(opencode.command, `${OPENCODE_ACCEPT_ALL_ENV} opencode`);
  assert.equal(opencode.startupText, ` ${OPENCODE_ACCEPT_ALL_ENV} opencode\r`);

  const disabled = buildAgentLaunchPlan({
    acceptAllMode: "disabled",
    agentId: "codex",
    command: "codex --yolo",
    globalAcceptAllEnabled: true,
  });
  assert.equal(disabled.command, "codex");

  const disabledOpenCode = buildAgentLaunchPlan({
    acceptAllMode: "disabled",
    agentId: "opencode",
    command: `${OPENCODE_ACCEPT_ALL_ENV} opencode --dangerously-skip-permissions`,
    globalAcceptAllEnabled: true,
  });
  assert.equal(disabledOpenCode.command, "opencode");
});

test("Cursor launch appends resume after runtime Accept All handling when a chat id exists", () => {
  const plan = buildAgentLaunchPlan({
    agentId: "cursor",
    agentSessionId: "8B16E7E6-3CE1-4D0B-9F35-78261B7F0767",
    command: "cursor-agent",
    globalAcceptAllEnabled: true,
  });
  assert.equal(
    plan.command,
    'cursor-agent --yolo --resume "8b16e7e6-3ce1-4d0b-9f35-78261b7f0767"',
  );
});

test("Cursor resume uses exact identity before title lookup", () => {
  const project = projectFixture();
  const session = sessionFixture({
    agentId: "cursor",
    runtimeSettings: {
      agentCommand: "cursor-agent",
      agentSessionId: "E10971DA-CBD7-459A-9AC3-B9B0313199A3",
      titleSource: "user",
    },
    title: "∗ Cursor CLI Session",
  });

  const plan = buildAgentResumePlan(project, session);
  assert.equal(plan.primaryCommand, 'cursor-agent --resume "e10971da-cbd7-459a-9ac3-b9b0313199a3"');
  assert.equal(plan.displayCommand, plan.primaryCommand);
  assert.equal(plan.fallbackCommand, undefined);
  assert.doesNotMatch(plan.startupText ?? "", /lookup chat id|Cursor CLI Session/);
});

test("Cursor resume extracts exact identity from stored raw resume commands", () => {
  const project = projectFixture();
  const session = sessionFixture({
    agentId: "cursor",
    runtimeSettings: {
      agentCommand: "cursor-agent",
      resumeCommand: "cd '/repo/ghostex' && cursor-agent --resume \"e10971da-cbd7-459a-9ac3-b9b0313199a3\"",
      titleSource: "user",
    },
    title: "∗ Cursor CLI Session",
  });

  const plan = buildAgentResumePlan(project, session);
  assert.equal(plan.primaryCommand, 'cursor-agent --resume "e10971da-cbd7-459a-9ac3-b9b0313199a3"');
  assert.equal(plan.fallbackCommand, undefined);
  assert.doesNotMatch(plan.startupText ?? "", /lookup chat id|Unable to find Cursor/);
});

test("Claude resume extracts exact identity from transcript path", () => {
  const project = projectFixture();
  const claudeSessionId = "9970b270-b39f-4d63-a764-fa8d88083995";
  const session = sessionFixture({
    agentId: "claude",
    runtimeSettings: {
      agentCommand: "claude",
      agentSessionPath: `/Users/example/.claude/projects/-repo-ghostex/${claudeSessionId}.jsonl`,
      titleSource: "user",
    },
    title: "Readable Claude title",
  });

  const plan = buildAgentResumePlan(project, session);

  assert.equal(plan.primaryCommand, `claude --resume "${claudeSessionId}"`);
  assert.equal(plan.displayCommand, plan.primaryCommand);
  assert.equal(plan.copyCommand, `claude --resume "${claudeSessionId}"`);
  assert.match(plan.fallbackCommand ?? "", /CLAUDE_RESUME_SESSION_ID/);
  assert.doesNotMatch(plan.startupText ?? "", /claude --resume "Readable Claude title"/);
});

test("Claude resume with only migrated title and first prompt looks up a transcript id", () => {
  const tempDir = mkdtempSync(path.join(os.tmpdir(), "ghostex-claude-resume-"));
  const binDir = path.join(tempDir, "bin");
  const homeDir = path.join(tempDir, "home");
  const transcriptDir = path.join(homeDir, ".claude", "projects", "-repo-ghostex");
  mkdirSync(binDir, { recursive: true });
  mkdirSync(transcriptDir, { recursive: true });

  const claudeSessionId = "9970b270-b39f-4d63-a764-fa8d88083995";
  const title = "Migration Fix";
  const firstPrompt = "Review the worktree migration";
  writeFileSync(
    path.join(transcriptDir, `${claudeSessionId}.jsonl`),
    [
      JSON.stringify({
        customTitle: title,
        cwd: "/repo/ghostex",
        sessionId: claudeSessionId,
        timestamp: "2026-06-10T13:00:00.000Z",
        type: "custom-title",
      }),
      JSON.stringify({
        cwd: "/repo/ghostex",
        message: { content: [{ text: firstPrompt, type: "text" }] },
        sessionId: claudeSessionId,
        timestamp: "2026-06-10T13:01:00.000Z",
        type: "user",
      }),
    ].join("\n") + "\n",
    "utf8",
  );
  const fakeClaude = path.join(binDir, "claude");
  writeFileSync(fakeClaude, "#!/bin/sh\nprintf '%s\\n' \"$*\"\n", "utf8");
  chmodSync(fakeClaude, 0o755);

  const project = projectFixture();
  const session = sessionFixture({
    agentId: "claude",
    runtimeSettings: {
      agentCommand: "claude",
      firstUserMessage: firstPrompt,
      titleSource: "user",
    },
    title,
  });
  const plan = buildAgentResumePlan(project, session);

  assert.match(plan.primaryCommand ?? "", /CLAUDE_RESUME_SESSION_ID/);
  assert.equal((plan.primaryCommand ?? "").includes(process.execPath), true);
  assert.doesNotMatch(plan.primaryCommand ?? "", /python/i);
  assert.equal(plan.displayCommand, 'claude --resume "Migration Fix"  # lookup Claude session id by title');
  assert.equal(plan.copyCommand, undefined);
  assert.doesNotMatch(plan.primaryCommand ?? "", /claude --resume "Migration Fix"/);

  const output = execFileSync("/bin/sh", ["-c", (plan.startupText ?? "").trim()], {
    cwd: tempDir,
    encoding: "utf8",
    env: {
      ...process.env,
      HOME: homeDir,
      PATH: `${binDir}:${process.env.PATH ?? ""}`,
    },
  });
  assert.match(output, new RegExp(`--resume ${claudeSessionId}`));
});

test("Cursor placeholder titles are not used for chat-store lookup", () => {
  const project = projectFixture();
  const session = sessionFixture({
    agentId: "cursor",
    runtimeSettings: {
      agentCommand: "cursor-agent",
      titleSource: "user",
    },
    title: "∗ Cursor CLI Session",
  });

  assert.equal(buildAgentResumeCommand(project, session), undefined);
  assert.equal(buildAgentResumeFallbackCommand(project, session), undefined);
  assert.equal(buildAgentResumeStartupText(project, session), undefined);
});

test("resume and fallback command construction follows current sidebar rules", () => {
  const project = projectFixture();
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      agentSessionId: "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56",
      titleSource: "user",
    },
    title: "Readable thread title",
  });

  assert.match(buildAgentResumeCommand(project, session) ?? "", /CODEX_RESUME_SESSION_ID/);
  assert.match(
    buildAgentResumeCommand(project, session) ?? "",
    /codex resume "\$CODEX_RESUME_SESSION_ID"/,
  );
  assert.match(buildAgentResumeCommand(project, session) ?? "", /--exact/);
  assert.match(buildAgentResumeFallbackCommand(project, session) ?? "", /--title/);
  assert.match(
    buildAgentResumeFallbackCommand(project, session) ?? "",
    /codex resume "\$CODEX_RESUME_SESSION_ID"/,
  );
  assert.equal(buildAgentResumePlan(project, session).copyCommand, 'codex resume "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"');
  const startupText = buildAgentResumeStartupText(project, session);
  assert.match(startupText ?? "", /Restoring session/);
  assert.match(startupText ?? "", /Exact resume failed; trying saved fallback resume command/);
  assert.match(startupText ?? "", /__ghostex_restore_resume_primary/);
});

test("resume startup commands apply Accept All at runtime without changing stored command", () => {
  const project = {
    ...projectFixture(),
    launchSettings: { agentAcceptAllEnabled: true },
  };
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      agentSessionId: "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56",
      titleSource: "user",
    },
    title: "Readable thread title",
  });

  assert.equal(
    buildAgentResumePlan(project, session).displayCommand,
    'codex --yolo resume "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"',
  );
  assert.equal(
    buildAgentResumePlan(project, session).copyCommand,
    'codex --yolo resume "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"',
  );
  assert.match(buildAgentResumeCommand(project, session) ?? "", /codex --yolo resume "\$CODEX_RESUME_SESSION_ID"/);
  assert.match(buildAgentResumeFallbackCommand(project, session) ?? "", /codex --yolo resume "\$CODEX_RESUME_SESSION_ID"/);
  assert.equal(session.runtimeSettings.agentCommand, "codex");
});

test("Codex resume rejects internal exact ids and falls back to restorable title matches", () => {
  const tempDir = mkdtempSync(path.join(os.tmpdir(), "ghostex-codex-resume-"));
  const binDir = path.join(tempDir, "bin");
  const codexHome = path.join(tempDir, "codex-home");
  const sessionsDir = path.join(codexHome, "sessions", "2026", "06", "07");
  mkdirSync(binDir, { recursive: true });
  mkdirSync(sessionsDir, { recursive: true });

  const realSessionId = "11111111-1111-4111-8111-111111111111";
  const internalSessionId = "22222222-2222-4222-8222-222222222222";
  const title = "Readable thread title";
  writeFileSync(
    path.join(codexHome, "session_index.jsonl"),
    [
      JSON.stringify({ id: realSessionId, thread_name: title, updated_at: "2026-06-06T21:00:00.000Z" }),
      JSON.stringify({ id: internalSessionId, thread_name: title, updated_at: "2026-06-06T21:03:00.000Z" }),
    ].join("\n") + "\n",
    "utf8",
  );
  writeFileSync(
    path.join(sessionsDir, `rollout-2026-06-07T01-00-00-${realSessionId}.jsonl`),
    JSON.stringify({
      payload: { id: realSessionId, originator: "codex-tui", source: "cli" },
      type: "session_meta",
    }) + "\n",
    "utf8",
  );
  writeFileSync(
    path.join(sessionsDir, `rollout-2026-06-07T01-03-00-${internalSessionId}.jsonl`),
    JSON.stringify({
      payload: { id: internalSessionId, originator: "codex_exec", source: "exec" },
      type: "session_meta",
    }) + "\n",
    "utf8",
  );
  const fakeCodex = path.join(binDir, "codex");
  writeFileSync(fakeCodex, "#!/bin/sh\nprintf '%s\\n' \"$*\"\n", "utf8");
  chmodSync(fakeCodex, 0o755);

  const project = projectFixture();
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      agentSessionId: internalSessionId,
      titleSource: "user",
    },
    title,
  });
  const startupText = buildAgentResumeStartupText(project, session);
  assert.match(startupText ?? "", /CODEX_RESUME_SESSION_ID/);
  assert.match(startupText ?? "", /--exact/);
  assert.match(startupText ?? "", /--title/);

  const output = execFileSync("/bin/zsh", ["-lc", (startupText ?? "").trim()], {
    cwd: tempDir,
    encoding: "utf8",
    env: {
      ...process.env,
      CODEX_HOME: codexHome,
      PATH: `${binDir}:${process.env.PATH ?? ""}`,
    },
  });
  assert.match(output, /Exact resume failed; trying saved fallback resume command/);
  assert.match(output, new RegExp(`resume ${realSessionId}`));
  assert.doesNotMatch(output, new RegExp(`resume ${internalSessionId}`));
});

test("fork plan uses gxserver Codex runtime identity and Accept All policy", () => {
  const project = {
    ...projectFixture(),
    launchSettings: { agentAcceptAllEnabled: true },
  };
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      agentName: "codex",
      agentSessionId: "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56",
      titleSource: "user",
    },
  });

  const plan = buildAgentForkPlan(project, session);
  assert.equal(plan.primaryCommand, 'codex --yolo fork "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"');
  assert.equal(plan.startupText, ' codex --yolo fork "6a6c2672-6b45-45fe-a1a8-a73f9a3a9c56"\r');
  assert.equal(plan.startupTextDisposition, "queueAfterTerminalReady");
});

test("fork plan can use trusted Codex title lookup when exact identity is missing", () => {
  const project = projectFixture();
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      agentName: "codex",
      titleSource: "user",
    },
    title: "Readable thread title",
  });

  const plan = buildAgentForkPlan(project, session);
  assert.match(plan.primaryCommand ?? "", /CODEX_FORK_SESSION_ID/);
  assert.match(plan.primaryCommand ?? "", /codex fork "\$CODEX_FORK_SESSION_ID"/);
  assert.equal(plan.displayCommand, 'codex fork "Readable thread title"  # lookup Codex session id by title');
});

test("fork session params persist server-owned startup plan for clients", () => {
  const project = projectFixture();
  const sourceSession = sessionFixture({
    agentId: "claude",
    sessionId: "G1src",
    runtimeSettings: {
      agentCommand: "claude",
      agentName: "claude",
      agentSessionId: "claude-thread",
      titleSource: "user",
    },
    title: "Review work",
  });
  const plan = buildAgentForkPlan(project, sourceSession);

  const params = createAgentForkSessionParams(project, sourceSession, plan);
  assert.equal(params.agentId, "claude");
  assert.equal(params.kind, "agent");
  assert.equal(params.providerState?.lifecycleState, "missing");
  assert.equal(params.providerState?.provider, "zmx");
  assert.equal(params.restoredFromSessionId, "G1src");
  assert.equal(params.runtimeSettings?.agentName, "claude");
  assert.equal(params.runtimeSettings?.forkedFromSessionId, "G1src");
  const launchPlan = params.launchSettings?.agentLaunchPlan as Record<string, unknown> | undefined;
  assert.equal(launchPlan?.command, 'claude --resume "claude-thread" --fork-session');
  assert.equal(launchPlan?.startupText, ' claude --resume "claude-thread" --fork-session\r');
});

test("resume plan separates OpenCode lookup command from runtime Accept All command", () => {
  const project = {
    ...projectFixture(),
    launchSettings: { agentAcceptAllEnabled: true },
  };
  const session = sessionFixture({
    agentId: "opencode",
    runtimeSettings: {
      agentCommand: "opencode",
      titleSource: "user",
    },
    title: "Readable thread title",
  });

  const plan = buildAgentResumePlan(project, session);
  assert.equal(plan.runtimeCommand, `${OPENCODE_ACCEPT_ALL_ENV} opencode`);
  assert.equal(plan.lookupCommand, "opencode");
  assert.match(
    plan.primaryCommand ?? "",
    /OPENCODE_CONFIG_CONTENT='\{"permission":"allow"\}' opencode -s "\$\(opencode session list --format json/,
  );
  assert.doesNotMatch(
    plan.primaryCommand ?? "",
    /OPENCODE_CONFIG_CONTENT='\{"permission":"allow"\}' opencode session list/,
  );
  assert.equal(plan.copyCommand, undefined);
});

test("OpenCode exact resume uses runtime permission config without legacy flags", () => {
  const project = {
    ...projectFixture(),
    launchSettings: { agentAcceptAllEnabled: true },
  };
  const session = sessionFixture({
    agentId: "opencode",
    runtimeSettings: {
      agentCommand: "opencode --dangerously-skip-permissions",
      agentSessionId: "opencode-session-123",
      titleSource: "user",
    },
    title: "Readable thread title",
  });

  const plan = buildAgentResumePlan(project, session);
  assert.equal(plan.primaryCommand, `${OPENCODE_ACCEPT_ALL_ENV} opencode --session "opencode-session-123"`);
  assert.equal(plan.copyCommand, `${OPENCODE_ACCEPT_ALL_ENV} opencode --session "opencode-session-123"`);
});

test("lower-priority resume plan supports Kiro and OMP exact ids", () => {
  /*
  CDXC:AgentResume 2026-06-11-22:49:
  Kiro and OMP are lower-priority hook integrations, but their exact session ids still need first-class gxserver resume plans so clients do not need separate local restore rules.
  */
  const project = projectFixture();
  const kiro = sessionFixture({
    agentId: "kiro",
    runtimeSettings: {
      agentCommand: "kiro-cli chat --agent ghostex",
      agentSessionId: "kiro-thread-123",
      titleSource: "user",
    },
    title: "Kiro thread",
  });
  const kiroPlan = buildAgentResumePlan(project, kiro);
  assert.equal(kiroPlan.primaryCommand, 'kiro-cli chat --agent ghostex --resume-id "kiro-thread-123"');
  assert.equal(kiroPlan.displayCommand, kiroPlan.primaryCommand);
  assert.equal(kiroPlan.copyCommand, kiroPlan.primaryCommand);

  const omp = sessionFixture({
    agentId: "omp",
    runtimeSettings: {
      agentCommand: "omp",
      agentSessionId: "omp-thread-456",
      titleSource: "user",
    },
    title: "OMP thread",
  });
  const ompPlan = buildAgentResumePlan(project, omp);
  assert.equal(ompPlan.primaryCommand, 'omp --session "omp-thread-456"');
  assert.equal(ompPlan.displayCommand, ompPlan.primaryCommand);
  assert.equal(ompPlan.copyCommand, ompPlan.primaryCommand);
});

test("temporary Search by Text titles are not trusted resume fallbacks", () => {
  const project = projectFixture();
  const searchSession = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      titleSource: "user",
    },
    title: "Search by Text",
  });

  assert.equal(buildAgentResumeCommand(project, searchSession), undefined);
  assert.equal(buildAgentResumeFallbackCommand(project, searchSession), undefined);
  assert.equal(buildAgentResumeStartupText(project, searchSession), undefined);

  const identifiedSession = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentCommand: "codex",
      agentSessionId: "019e7c39-7ba7-7ac3-b79c-02757e299516",
      titleSource: "placeholder",
    },
    title: "Search by Text",
  });
  assert.match(buildAgentResumeCommand(project, identifiedSession) ?? "", /CODEX_RESUME_SESSION_ID/);
  assert.match(buildAgentResumeCommand(project, identifiedSession) ?? "", /--exact/);
  assert.match(
    buildAgentResumeCommand(project, identifiedSession) ?? "",
    /codex resume "\$CODEX_RESUME_SESSION_ID"/,
  );
  assert.equal(buildAgentResumeFallbackCommand(project, identifiedSession), undefined);
});

test("agent activity transitions suppress bootstrap noise and require real working before attention", () => {
  const launched = applyAgentActivityTransition({
    event: "launch",
    nowMs: Date.parse("2026-05-30T12:00:01.000Z"),
    nowIso: "2026-05-30T12:00:01.000Z",
  });
  assert.equal(launched.activity, "idle");
  assert.equal(launched.hasSeenWorking, false);
  assert.equal(launched.suppressedUntil, "2026-05-30T12:00:13.000Z");

  const suppressed = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-05-30T12:00:02.000Z"),
    nowIso: "2026-05-30T12:00:02.000Z",
    previous: launched,
    title: "⠏ Bootstrapping",
  });
  assert.equal(suppressed.activity, "idle");
  assert.equal(suppressed.hasSeenWorking, false);

  const working = applyAgentActivityTransition({
    activity: "working",
    nowMs: Date.parse("2026-05-30T12:00:14.000Z"),
    nowIso: "2026-05-30T12:00:14.000Z",
    previous: launched,
  });
  assert.equal(working.activity, "working");
  assert.equal(working.workingStartedAt, "2026-05-30T12:00:14.000Z");

  const tooFast = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-05-30T12:00:15.000Z"),
    nowIso: "2026-05-30T12:00:15.000Z",
    previous: working,
    title: "[ ! ] Action Required",
  });
  assert.equal(tooFast.activity, "idle");

  const attention = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-05-30T12:00:20.000Z"),
    nowIso: "2026-05-30T12:00:20.000Z",
    previous: working,
    title: "[ ! ] Action Required",
  });
  assert.equal(attention.activity, "attention");
});

test("Codex action-required title blink remains one attention transition", () => {
  const working = applyAgentActivityTransition({
    activity: "working",
    agentId: "codex",
    nowIso: "2026-05-30T12:00:14.000Z",
    nowMs: Date.parse("2026-05-30T12:00:14.000Z"),
  });

  const attention = applyAgentActivityTransition({
    agentId: "codex",
    nowIso: "2026-05-30T12:00:20.000Z",
    nowMs: Date.parse("2026-05-30T12:00:20.000Z"),
    previous: working,
    title: "[ ! ] Action Required",
  });
  assert.equal(attention.activity, "attention");
  assert.equal(attention.lastChangedAt, "2026-05-30T12:00:20.000Z");

  const dotFrame = applyAgentActivityTransition({
    agentId: "codex",
    nowIso: "2026-05-30T12:00:21.000Z",
    nowMs: Date.parse("2026-05-30T12:00:21.000Z"),
    previous: attention,
    title: "[ . ] Action Required",
  });
  assert.equal(dotFrame.activity, "attention");
  assert.equal(dotFrame.lastChangedAt, attention.lastChangedAt);

  const middleDotFrame = applyAgentActivityTransition({
    agentId: "codex",
    nowIso: "2026-05-30T12:00:22.000Z",
    nowMs: Date.parse("2026-05-30T12:00:22.000Z"),
    previous: {
      ...attention,
      activity: "idle",
      isAcknowledged: true,
    },
    title: "[ · ] Action Required",
  });
  assert.equal(middleDotFrame.activity, "idle");

  const nonCodex = applyAgentActivityTransition({
    agentId: "claude",
    nowIso: "2026-05-30T12:00:20.000Z",
    nowMs: Date.parse("2026-05-30T12:00:20.000Z"),
    previous: working,
    title: "[ . ] Action Required",
  });
  assert.equal(nonCodex.activity, "idle");
});

test("Codex spinner title working expires when the title frame is stuck", () => {
  const firstFrame = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:00.000Z",
    nowMs: Date.parse("2026-06-01T12:00:00.000Z"),
    title: "⠏ Skip migration issue 2 options",
  });
  assert.equal(firstFrame.activity, "working");
  assert.equal(firstFrame.lastTitle, "⠏ Skip migration issue 2 options");
  assert.equal(firstFrame.lastTitleChangeAt, "2026-06-01T12:00:00.000Z");
  assert.equal(firstFrame.workingSource, "title");

  const sameFrameInsideWindow = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:02.000Z",
    nowMs: Date.parse("2026-06-01T12:00:02.000Z"),
    previous: firstFrame,
    title: "⠏ Skip migration issue 2 options",
  });
  assert.equal(sameFrameInsideWindow.activity, "working");
  assert.equal(sameFrameInsideWindow.lastTitleChangeAt, "2026-06-01T12:00:00.000Z");

  const rapidChangedSpinnerFrame = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:00.100Z",
    nowMs: Date.parse("2026-06-01T12:00:00.100Z"),
    previous: firstFrame,
    title: "⠋ Skip migration issue 2 options",
  });
  assert.equal(rapidChangedSpinnerFrame.activity, "working");
  assert.equal(rapidChangedSpinnerFrame.lastTitle, "⠋ Skip migration issue 2 options");
  assert.equal(rapidChangedSpinnerFrame.lastTitleChangeAt, "2026-06-01T12:00:00.000Z");

  /*
  CDXC:SessionStatus 2026-06-06-23:07:
  Same-semantic spinner frames should refresh the gxserver working window only
  at the title heartbeat cadence. This preserves "still working" detection
  while preventing native or zmx title storms from publishing every frame.
  */
  const heartbeatChangedSpinnerFrame = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:02.100Z",
    nowMs: Date.parse("2026-06-01T12:00:02.100Z"),
    previous: rapidChangedSpinnerFrame,
    title: "⠙ Skip migration issue 2 options",
  });
  assert.equal(heartbeatChangedSpinnerFrame.activity, "working");
  assert.equal(heartbeatChangedSpinnerFrame.lastTitleChangeAt, "2026-06-01T12:00:02.100Z");

  const sameFrameAfterOldWindow = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:03.025Z",
    nowMs: Date.parse("2026-06-01T12:00:03.025Z"),
    previous: sameFrameInsideWindow,
    title: "⠏ Skip migration issue 2 options",
  });
  assert.equal(sameFrameAfterOldWindow.activity, "working");
  assert.equal(sameFrameAfterOldWindow.workingSource, "title");

  /*
  CDXC:SessionStatus 2026-06-06-23:26:
  The old 3s stale boundary is too close to zmx's 2s coalesced heartbeat and
  previously could show active sessions as attention between sparse title
  observations.
  Keep the session working inside the 5s slow-spinner freshness window, then
  expire a truly frozen title-derived working state after that window.
  */
  const sameFrameAfterFreshnessWindow = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:05.025Z",
    nowMs: Date.parse("2026-06-01T12:00:05.025Z"),
    previous: sameFrameInsideWindow,
    title: "⠏ Skip migration issue 2 options",
  });
  assert.equal(sameFrameAfterFreshnessWindow.activity, "idle");
  assert.equal(sameFrameAfterFreshnessWindow.workingSource, undefined);

  const refreshedSpinnerFrame = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:04.000Z",
    nowMs: Date.parse("2026-06-01T12:00:04.000Z"),
    previous: sameFrameInsideWindow,
    title: "⠋ Skip migration issue 2 options",
  });
  assert.equal(refreshedSpinnerFrame.activity, "working");
  assert.equal(refreshedSpinnerFrame.lastTitleChangeAt, "2026-06-01T12:00:04.000Z");
});

test("presentation activity expires stale title-derived working without expiring explicit working", () => {
  const titleDerived = getEffectiveAgentActivityState(
    {
      activity: "working",
      agentName: "codex",
      hasSeenWorking: true,
      isAcknowledged: false,
      lastTitle: "⠏ Skip migration issue 2 options",
      lastTitleChangeAt: "2026-06-01T12:00:00.000Z",
      workingSource: "title",
      workingStartedAt: "2026-06-01T12:00:00.000Z",
    },
    { activity: "idle" },
    Date.parse("2026-06-01T12:00:06.000Z"),
  );
  assert.equal(titleDerived.activity, "idle");

  const explicit = getEffectiveAgentActivityState(
    {
      activity: "working",
      agentName: "codex",
      hasSeenWorking: true,
      isAcknowledged: false,
      lastTitle: "⠏ Skip migration issue 2 options",
      lastTitleChangeAt: "2026-06-01T12:00:00.000Z",
      workingSource: "explicit",
      workingStartedAt: "2026-06-01T12:00:00.000Z",
    },
    { activity: "idle" },
    Date.parse("2026-06-01T12:10:00.000Z"),
  );
  assert.equal(explicit.activity, "working");
});

test("title-derived activity preserves macOS agent edge cases", () => {
  const cursor = applyAgentActivityTransition({
    agentId: "claude",
    event: "title",
    nowIso: "2026-06-01T12:00:00.000Z",
    nowMs: Date.parse("2026-06-01T12:00:00.000Z"),
    title: "My Task - ⏳ Working ..·",
  });
  assert.equal(cursor.agentName, "cursor");
  assert.equal(cursor.activity, "working");

  const cursorReady = applyAgentActivityTransition({
    event: "title",
    nowIso: "2026-06-01T12:00:01.000Z",
    nowMs: Date.parse("2026-06-01T12:00:01.000Z"),
    previous: cursor,
    title: "My Task - ✅ Ready",
  });
  assert.equal(cursorReady.agentName, "cursor");
  assert.equal(cursorReady.activity, "idle");

  const antigravityAttention = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowIso: "2026-06-01T12:00:00.000Z",
    nowMs: Date.parse("2026-06-01T12:00:00.000Z"),
    title: "🔔 agy",
  });
  assert.equal(antigravityAttention.agentName, "antigravity");
  assert.equal(antigravityAttention.activity, "attention");

  const pi = applyAgentActivityTransition({
    event: "title",
    nowIso: "2026-06-01T12:00:00.000Z",
    nowMs: Date.parse("2026-06-01T12:00:00.000Z"),
    title: "π - ghostex",
  });
  assert.equal(pi.agentName, "pi");
  assert.equal(pi.activity, "idle");

  const piWorking = applyAgentActivityTransition({
    event: "title",
    nowIso: "2026-06-01T12:00:01.000Z",
    nowMs: Date.parse("2026-06-01T12:00:01.000Z"),
    previous: pi,
    title: "⠸ π - Restore Pi support - ghostex",
  });
  assert.equal(piWorking.agentName, "pi");
  assert.equal(piWorking.activity, "working");
});

function projectFixture(): GxserverProjectDomainState {
  return {
    attentionRules: {},
    completionRules: {},
    createdAt: "2026-05-30T12:00:00.000Z",
    customAgentOrder: [],
    customAgents: [],
    customCommandOrder: [],
    customCommands: [],
    deletedDefaultCommandIds: [],
    gitConfig: {},
    isFavorite: false,
    isPinned: false,
    launchSettings: {},
    name: "Ghostex",
    notificationRules: {},
    path: "/repo/ghostex",
    previousSessionHistory: [],
    projectBoardConfig: {},
    projectId: "P3a91",
    runtimeSettings: {},
    updatedAt: "2026-05-30T12:00:00.000Z",
  };
}

function sessionFixture(
  partial: Partial<GxserverSessionDomainState>,
): GxserverSessionDomainState {
  return {
    agentId: "codex",
    attentionRules: {},
    completionRules: {},
    createdAt: "2026-05-30T12:00:00.000Z",
    globalRef: "S1abcd:P3a91:G8v20",
    hiddenMetadata: {},
    isFavorite: false,
    isPinned: false,
    kind: "agent",
    launchSettings: {},
    lifecycleState: "running",
    notificationRules: {},
    projectId: "P3a91",
    providerState: { lifecycleState: "missing", zmxName: "S1abcd-P3a91-G8v20" },
    runtimeSettings: {},
    sessionId: "G8v20",
    surface: "workspace",
    title: "Agent task",
    updatedAt: "2026-05-30T12:00:00.000Z",
    zmxName: "S1abcd-P3a91-G8v20",
    ...partial,
  };
}
