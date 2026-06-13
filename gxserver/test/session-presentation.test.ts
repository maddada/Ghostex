import test from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  applySessionRenameRequest,
  applySessionStateEvent,
  createAgentTitleDebouncer,
  GxserverPresentationDeltaCoalescer,
  parseAgentResumeIdentity,
  projectGxserverPresentationSnapshot,
  reconcileAgentMetadataTitle,
  searchGxserverPreviousSessions,
  searchGxserverPresentationSessions,
} from "../src/session-presentation/index.js";
import type {
  GxserverPresentationDelta,
  GxserverPresentationRevision,
  GxserverProjectDomainState,
  GxserverProjectId,
  GxserverSessionDomainState,
  GxserverSessionId,
  GxserverUpdateSessionParams,
} from "../protocol/index.js";

test("session state events resolve Codex resume identity to previous trusted title", () => {
  const codexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
  const project = projectFixture({
    previousSessionHistory: [
      {
        agentSessionId: codexSessionId,
        closedAt: "2026-05-31T12:04:13.807Z",
        primaryTitle: "Shorter native tabs bar",
        sessionRecord: {
          agentName: "codex",
          agentSessionId: codexSessionId,
          title: "Shorter native tabs bar",
          titleSource: "terminal-auto",
        },
      },
    ],
  });
  const session = sessionFixture({
    runtimeSettings: { titleSource: "placeholder" },
    title: "Terminal Session",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    projectId: session.projectId,
    sessionId: session.sessionId,
    startupText: `cd '/Users/madda/dev/_active/zmux' && codex resume "${codexSessionId}"`,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "codex");
  assert.equal(result.session.kind, "agent");
  assert.equal(result.session.runtimeSettings.agentSessionId, codexSessionId);
  assert.equal(result.session.runtimeSettings.titleSource, "terminal-auto");
  assert.equal(result.session.title, "Shorter native tabs bar");
  assert.equal(result.projection.primaryTitle, "Shorter native tabs bar");
});

test("resume identity parser recognizes Kiro and OMP startup text", () => {
  /*
  CDXC:AgentResume 2026-06-11-22:49:
  Startup text is a server-owned identity observation. Kiro and OMP restore commands should classify sessions before hook metadata arrives, matching the rest of the gxserver resume parser.
  */
  assert.deepEqual(
    parseAgentResumeIdentity('kiro-cli chat --agent ghostex --resume-id "kiro-thread-123"'),
    { agentId: "kiro", agentSessionId: "kiro-thread-123" },
  );
  assert.deepEqual(
    parseAgentResumeIdentity("omp --session 'omp-thread-456'"),
    { agentId: "omp", agentSessionId: "omp-thread-456" },
  );
});

test("session state events preserve an already trusted current title", () => {
  const codexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
  const project = projectFixture({
    previousSessionHistory: [
      {
        agentSessionId: codexSessionId,
        primaryTitle: "Older history title",
        sessionRecord: {
          agentName: "codex",
          agentSessionId: codexSessionId,
          title: "Older history title",
          titleSource: "terminal-auto",
        },
      },
    ],
  });
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: { titleSource: "user" },
    title: "Current user title",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    agentSessionId: codexSessionId,
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.title, "Current user title");
  assert.equal(result.session.runtimeSettings.agentSessionId, codexSessionId);
  assert.equal(result.projection.trustedResumeTitle, "Current user title");
});

test("passive Cursor hooks correct stale Codex session identity", () => {
  const staleCodexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
  const cursorSessionId = "866a452b-3a52-4f27-9b26-fd717a2f1c16";
  const cursorSessionPath =
    "/Users/person/.cursor/projects/Users-person-dev-active-zmux/agent-transcripts/866a452b-3a52-4f27-9b26-fd717a2f1c16/866a452b-3a52-4f27-9b26-fd717a2f1c16.jsonl";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: staleCodexSessionId,
      agentSessionPath: "/Users/person/.codex/sessions/private-thread.jsonl",
      titleSource: "terminal-auto",
    },
    title: "macOS icon and search padding",
  });
  const repository = new MockPresentationRepository(project, [session]);
  const conflicts: unknown[] = [];

  const result = applySessionStateEvent(repository, {
    agentName: "cursor",
    agentSessionId: cursorSessionId,
    agentSessionPath: cursorSessionPath,
    firstPromptTitleGenerationAgent: "codex",
    identityUpdateSource: "passive",
    onIdentityConflict: (conflict) => conflicts.push(conflict),
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  /*
  CDXC:GxserverSessionIdentity 2026-06-09-09:58:
  Cursor hook identity must repair stale Codex domain rows in gxserver itself so sidebar, CLI aliases, search, resume, and every other client consume the same Cursor session identity instead of painting over a Codex row in one UI.
  */
  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "cursor");
  assert.equal(result.session.runtimeSettings.agentName, "cursor");
  assert.equal(result.session.runtimeSettings.agentSessionId, cursorSessionId);
  assert.equal(result.session.runtimeSettings.agentSessionPath, cursorSessionPath);
  assert.equal(result.session.runtimeSettings.firstPromptTitleGenerationAgent, "codex");
  assert.deepEqual(conflicts, []);

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [result.session],
  });
  assert.equal(snapshot.sessions[0]?.agentId, "cursor");
  assert.equal(snapshot.sessions[0]?.agentName, "cursor");
  assert.equal(snapshot.sessions[0]?.agentIcon, "cursor");
});

test("passive Cursor hooks cannot rewrite a gxserver-launched Codex session", () => {
  const cursorSessionId = "866a452b-3a52-4f27-9b26-fd717a2f1c16";
  const cursorSessionPath =
    "/Users/person/.cursor/projects/Users-person-dev-active-zmux/agent-transcripts/866a452b-3a52-4f27-9b26-fd717a2f1c16/866a452b-3a52-4f27-9b26-fd717a2f1c16.jsonl";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    launchSettings: {
      agentLaunchPlan: {
        agentCommand: "codex",
        command: "codex fork 019e7af5-c610-7f62-a129-db7bb510b48d",
      },
      forkedFromSessionId: "G4c7u",
    },
    runtimeSettings: {
      agentCommand: "codex",
      agentName: "codex",
      forkedFromSessionId: "G4c7u",
      launchAgentId: "codex",
      titleSource: "terminal-auto",
    },
    title: "Codex fork",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "cursor",
    agentSessionId: cursorSessionId,
    agentSessionPath: cursorSessionPath,
    identityUpdateSource: "passive",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, false);
  assert.equal(result.reason, "launch-agent-mismatch");
  assert.equal(result.session.agentId, "codex");
  assert.equal(result.session.runtimeSettings.agentName, "codex");
  assert.equal(result.session.runtimeSettings.agentSessionId, undefined);
  assert.equal(result.session.runtimeSettings.agentSessionPath, undefined);
});

test("passive Cursor transcript paths correct stale Codex identity without agent name", () => {
  const cursorSessionId = "866a452b-3a52-4f27-9b26-fd717a2f1c16";
  const cursorSessionPath =
    "/Users/person/.cursor/projects/Users-person-dev-active-zmux/agent-transcripts/866a452b-3a52-4f27-9b26-fd717a2f1c16/866a452b-3a52-4f27-9b26-fd717a2f1c16.jsonl";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: "019e7af5-c610-7f62-a129-db7bb510b48d",
      titleSource: "terminal-auto",
    },
    title: "macOS icon and search padding",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentSessionId: cursorSessionId,
    agentSessionPath: cursorSessionPath,
    identityUpdateSource: "passive",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "cursor");
  assert.equal(result.session.runtimeSettings.agentName, "cursor");
  assert.equal(result.session.runtimeSettings.agentSessionId, cursorSessionId);
  assert.equal(result.session.runtimeSettings.agentSessionPath, cursorSessionPath);
});

test("Claude transcript paths promote terminal rows to Claude sessions without agent name", () => {
  /*
  CDXC:ClaudeSessionIdentity 2026-06-11-21:43:
  Hook/session-state payloads may carry only a Claude transcript path. gxserver must infer the Claude identity from that path so presentation rows, session search, resume metadata, and native pane icons do not depend on macOS-local agent detection.
  */
  const claudeSessionId = "9970b270-b39f-4d63-a764-fa8d88083995";
  const claudeSessionPath =
    `/Users/person/.claude/projects/-Users-person-dev-active-zmux/${claudeSessionId}.jsonl`;
  const project = projectFixture({});
  const session = sessionFixture({
    runtimeSettings: { titleSource: "placeholder" },
    title: "Terminal Session",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentSessionId: claudeSessionId,
    agentSessionPath: claudeSessionPath,
    identityUpdateSource: "passive",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.kind, "agent");
  assert.equal(result.session.agentId, "claude");
  assert.equal(result.session.runtimeSettings.agentName, "claude");
  assert.equal(result.session.runtimeSettings.agentSessionId, claudeSessionId);
  assert.equal(result.session.runtimeSettings.agentSessionPath, claudeSessionPath);
});

test("stored Cursor transcript paths repair stale Codex rows on metadata-only events", () => {
  const cursorSessionId = "866a452b-3a52-4f27-9b26-fd717a2f1c16";
  const cursorSessionPath =
    "/Users/person/.cursor/projects/Users-person-dev-active-zmux/agent-transcripts/866a452b-3a52-4f27-9b26-fd717a2f1c16/866a452b-3a52-4f27-9b26-fd717a2f1c16.jsonl";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: cursorSessionId,
      agentSessionPath: cursorSessionPath,
      titleSource: "terminal-auto",
    },
    title: "macOS icon and search padding",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    identityUpdateSource: "passive",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "cursor");
  assert.equal(result.session.runtimeSettings.agentName, "cursor");
  assert.equal(result.session.runtimeSettings.agentSessionId, cursorSessionId);
  assert.equal(result.session.runtimeSettings.agentSessionPath, cursorSessionPath);
});

test("cross-agent corrections clear stale Codex session metadata when Cursor has no transcript yet", () => {
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: "019e7af5-c610-7f62-a129-db7bb510b48d",
      agentSessionPath: "/Users/person/.codex/sessions/private-thread.jsonl",
      titleSource: "terminal-auto",
    },
    title: "Manual Cursor start",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "cursor",
    identityUpdateSource: "passive",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "cursor");
  assert.equal(result.session.runtimeSettings.agentName, "cursor");
  assert.equal(result.session.runtimeSettings.agentSessionId, undefined);
  assert.equal(result.session.runtimeSettings.agentSessionPath, undefined);
});

test("passive Codex hooks correct stale non-Codex session identity", () => {
  /*
  CDXC:GxserverSessionIdentity 2026-06-12-02:44:
  When multiple hook stores have observations for the same surface, gxserver must let an explicit Codex hook replace stale Claude/other identity. Only an existing Codex-owned thread id should block passive Codex replacement; otherwise a previous UUID-shaped agent id can keep sidebar tooltips, status, search, and resume metadata on the wrong agent.
  */
  const staleClaudeSessionId = "870ae852-ddf1-4cfb-b49e-a9ee1d97dae3";
  const codexSessionId = "019eb834-893d-71b2-97e3-3ad431f4ef46";
  const codexSessionPath =
    `/Users/person/.codex/sessions/2026/06/11/rollout-2026-06-11T23-41-51-${codexSessionId}.jsonl`;
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "claude",
    kind: "agent",
    runtimeSettings: {
      agentActivity: {
        activity: "idle",
        agentName: "claude",
        hasSeenWorking: false,
        isAcknowledged: true,
        lastChangedAt: "2026-06-11T22:41:40.403Z",
      },
      agentName: "claude",
      agentSessionId: staleClaudeSessionId,
      agentSessionPath: `/Users/person/.claude-profiles/personal/projects/-repo/${staleClaudeSessionId}.jsonl`,
      titleSource: "terminal-auto",
    },
    title: "Agents Pane Resize Issue",
  });
  const repository = new MockPresentationRepository(project, [session]);
  const conflicts: unknown[] = [];

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    agentSessionId: codexSessionId,
    agentSessionPath: codexSessionPath,
    identityUpdateSource: "passive",
    onIdentityConflict: (conflict) => conflicts.push(conflict),
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "codex");
  assert.equal(result.session.runtimeSettings.agentName, "codex");
  assert.equal(result.session.runtimeSettings.agentSessionId, codexSessionId);
  assert.equal(result.session.runtimeSettings.agentSessionPath, codexSessionPath);
  assert.equal(result.session.runtimeSettings.agentActivity, undefined);
  assert.deepEqual(conflicts, []);
});

test("live process identity without a session id clears stale cross-agent transcript metadata", () => {
  const staleClaudeSessionId = "ef30d096-b233-4895-b4e7-e9d4abca61b8";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "claude",
    kind: "agent",
    runtimeSettings: {
      agentActivity: {
        activity: "idle",
        agentName: "claude",
        hasSeenWorking: false,
        isAcknowledged: true,
        lastChangedAt: "2026-06-12T07:58:06.269Z",
      },
      agentName: "claude",
      agentSessionId: staleClaudeSessionId,
      agentSessionPath: `/Users/person/.claude-profiles/work/projects/-repo/${staleClaudeSessionId}.jsonl`,
      titleSource: "terminal-auto",
    },
    title: "Hide Buttons Except Restart",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    identityUpdateSource: "live-process",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "codex");
  assert.equal(result.session.runtimeSettings.agentName, "codex");
  assert.equal(result.session.runtimeSettings.agentSessionId, undefined);
  assert.equal(result.session.runtimeSettings.agentSessionPath, undefined);
  assert.equal(result.session.runtimeSettings.agentActivity, undefined);
});

test("passive Codex hooks clear stale non-Codex activity after identity was already corrected", () => {
  const codexSessionId = "019eb834-893d-71b2-97e3-3ad431f4ef46";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentActivity: {
        activity: "idle",
        agentName: "claude",
        hasSeenWorking: false,
        isAcknowledged: true,
        lastChangedAt: "2026-06-11T22:41:40.403Z",
      },
      agentName: "codex",
      agentSessionId: codexSessionId,
      titleSource: "terminal-auto",
    },
    title: "Agents Pane Resize Issue",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    agentSessionId: codexSessionId,
    identityUpdateSource: "passive",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.agentId, "codex");
  assert.equal(result.session.runtimeSettings.agentName, "codex");
  assert.equal(result.session.runtimeSettings.agentSessionId, codexSessionId);
  assert.equal(result.session.runtimeSettings.agentActivity, undefined);
});

test("passive session state events cannot replace an existing Codex identity", () => {
  const currentCodexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
  const incomingCodexSessionId = "019e7c39-7ba7-7ac3-b79c-02757e299516";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: currentCodexSessionId,
      titleSource: "terminal-auto",
    },
    title: "Current Codex Thread",
  });
  const repository = new MockPresentationRepository(project, [session]);
  const conflicts: unknown[] = [];

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    agentSessionId: incomingCodexSessionId,
    firstUserMessage: "prompt from the wrong Codex thread",
    identityUpdateSource: "passive",
    onIdentityConflict: (conflict) => conflicts.push(conflict),
    projectId: session.projectId,
    sessionId: session.sessionId,
    title: "Wrong Codex Thread",
    titleSource: "terminal-auto",
  });

  /*
  CDXC:GxserverSessionIdentity 2026-06-09-08:55:
  Hook and generic session-state events are passive identity evidence. They may fill missing Codex metadata, but replacing an existing thread id would make gxserver reconcile the shared sidebar title, resume target, and search metadata to the wrong Codex transcript.
  */
  assert.equal(result.changed, false);
  assert.equal(result.reason, "passive-session-identity-conflict");
  assert.equal(result.session.runtimeSettings.agentSessionId, currentCodexSessionId);
  assert.equal(result.session.runtimeSettings.firstUserMessage, undefined);
  assert.equal(result.session.title, "Current Codex Thread");
  assert.deepEqual(result.identityConflict, {
    agentId: "codex",
    currentAgentSessionId: currentCodexSessionId,
    incomingAgentSessionId: incomingCodexSessionId,
    reason: "passive-agent-session-id-replacement",
    source: "passive",
  });
  assert.deepEqual(conflicts, [
    {
      agentId: "codex",
      currentAgentSessionId: currentCodexSessionId,
      incomingAgentSessionId: incomingCodexSessionId,
      reason: "passive-agent-session-id-replacement",
      source: "passive",
    },
  ]);
});

test("passive session state events cannot claim a Codex identity owned by another active session", () => {
  const codexSessionId = "019e7c39-7ba7-7ac3-b79c-02757e299516";
  const project = projectFixture({});
  const target = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: { agentName: "codex", titleSource: "terminal-auto" },
    sessionId: "G1new" as GxserverSessionId,
    title: "Current Codex Thread",
  });
  const owner = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: codexSessionId,
      titleSource: "terminal-auto",
    },
    sessionId: "G2own" as GxserverSessionId,
    title: "Other Live Thread",
  });
  const repository = new MockPresentationRepository(project, [target, owner]);
  const conflicts: unknown[] = [];

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    agentSessionId: codexSessionId,
    firstUserMessage: "prompt from another active Codex thread",
    identityUpdateSource: "passive",
    onIdentityConflict: (conflict) => conflicts.push(conflict),
    projectId: target.projectId,
    sessionId: target.sessionId,
    title: "Other Live Thread Title",
    titleSource: "terminal-auto",
  });

  assert.equal(result.changed, false);
  assert.equal(result.reason, "passive-session-identity-conflict");
  assert.equal(result.session.runtimeSettings.agentSessionId, undefined);
  assert.equal(result.session.runtimeSettings.firstUserMessage, undefined);
  assert.equal(result.session.title, "Current Codex Thread");
  assert.deepEqual(result.identityConflict, {
    agentId: "codex",
    incomingAgentSessionId: codexSessionId,
    ownerProjectId: owner.projectId,
    ownerSessionId: owner.sessionId,
    reason: "active-agent-session-id-owned",
    source: "passive",
  });
  assert.deepEqual(conflicts, [
    {
      agentId: "codex",
      incomingAgentSessionId: codexSessionId,
      ownerProjectId: owner.projectId,
      ownerSessionId: owner.sessionId,
      reason: "active-agent-session-id-owned",
      source: "passive",
    },
  ]);
});

test("lifecycle session state events may replace Codex identity", () => {
  const currentCodexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
  const incomingCodexSessionId = "019e7c39-7ba7-7ac3-b79c-02757e299516";
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: currentCodexSessionId,
      titleSource: "terminal-auto",
    },
    title: "Current Codex Thread",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    agentSessionId: incomingCodexSessionId,
    identityUpdateSource: "lifecycle",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  assert.equal(result.changed, true);
  assert.equal(result.session.runtimeSettings.agentSessionId, incomingCodexSessionId);
  assert.equal(result.session.title, "Current Codex Thread");
});

test("session state events persist first-prompt title generation settings", () => {
  const project = projectFixture({});
  const session = sessionFixture({
    runtimeSettings: {
      firstPromptTitleGenerationCommand: "old-title-command",
      titleSource: "placeholder",
    },
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionStateEvent(repository, {
    agentName: "codex",
    firstPromptTitleGenerationAgent: "custom",
    firstPromptTitleGenerationCommand: "",
    firstUserMessage: "Please wire the title generator selector",
    projectId: session.projectId,
    sessionId: session.sessionId,
  });

  /*
  CDXC:GxserverSessionTitle 2026-06-04-08:24:
  The first-prompt state event must carry the Settings-selected title generator into gxserver runtime state, including an explicit empty custom command so clearing a custom command cannot reuse stale session metadata.
  */
  assert.equal(result.changed, true);
  assert.equal(result.session.runtimeSettings.firstPromptTitleGenerationAgent, "custom");
  assert.equal(result.session.runtimeSettings.firstPromptTitleGenerationCommand, "");
  assert.equal(result.session.runtimeSettings.firstUserMessage, "Please wire the title generator selector");
});

test("agent rename requests stay pending until Codex metadata supplies the canonical title", async () => {
  const homeDir = await mkdtemp(path.join(tmpdir(), "gxserver-agent-title-home-"));
  const codexSessionId = "019e7af5-c610-7f62-a129-db7bb510b48d";
  try {
    await mkdir(path.join(homeDir, ".codex"), { recursive: true });
    await writeFile(
      path.join(homeDir, ".codex", "session_index.jsonl"),
      `${JSON.stringify({
        id: codexSessionId,
        thread_name: "Real Metadata Title",
        updated_at: "2026-06-01T05:03:00.000Z",
      })}\n`,
      "utf8",
    );
    const project = projectFixture({});
    const session = sessionFixture({
      agentId: "codex",
      kind: "agent",
      runtimeSettings: {
        agentName: "codex",
        agentSessionId: codexSessionId,
        titleSource: "terminal-auto",
      },
      title: "Old Title",
    });
    const repository = new MockPresentationRepository(project, [session]);

    const requested = applySessionRenameRequest(repository, {
      projectId: session.projectId,
      sessionId: session.sessionId,
      title: "Wrong Requested Title",
      titleSource: "user",
    });

    assert.equal(requested.pendingAgentMetadata, true);
    assert.equal(requested.shouldSendAgentRenameCommand, true);
    assert.equal(requested.session.title, "Old Title");
    assert.equal(requested.session.runtimeSettings.pendingAgentTitleRequestStatus, "pending");

    const reconciled = reconcileAgentMetadataTitle(repository, {
      homeDir,
      projectId: session.projectId,
      sessionId: session.sessionId,
      nowIso: "2026-06-01T05:03:00.000Z",
    });

    assert.equal(reconciled.changed, true);
    assert.equal(reconciled.session?.title, "Real Metadata Title");
    assert.equal(reconciled.session?.runtimeSettings.titleMetadataSource, "agent-metadata");
    assert.equal(reconciled.session?.runtimeSettings.pendingAgentTitleRequestStatus, "metadata-mismatch");
  } finally {
    await rm(homeDir, { force: true, recursive: true });
  }
});

test("non-agent rename requests apply the client title immediately", () => {
  const project = projectFixture({});
  const session = sessionFixture({
    runtimeSettings: { titleSource: "terminal-auto" },
    title: "Shell",
  });
  const repository = new MockPresentationRepository(project, [session]);

  const result = applySessionRenameRequest(repository, {
    projectId: session.projectId,
    sessionId: session.sessionId,
    title: "Build Watch",
    titleSource: "user",
  });

  assert.equal(result.pendingAgentMetadata, false);
  assert.equal(result.shouldSendAgentRenameCommand, false);
  assert.equal(result.session.title, "Build Watch");
  assert.equal(result.session.runtimeSettings.titleSource, "user");
});

test("presentation sessions expose gxserver first-prompt title generation state", () => {
  /*
  CDXC:GxserverSessionTitle 2026-06-04-07:11:
  Clients need a server-owned loading signal for first-prompt title generation so the terminal overlay and sidebar "Generating title" text can render during gxserver-owned auto-title jobs and clear from the next presentation delta.

  CDXC:GxserverSessionTitle 2026-06-12-07:08:
  Native macOS submits staged first-prompt title commands with a real Enter after gxserver applies the command. Presentation must expose that submit signal separately from generated title provenance so Claude can receive bare `/rename` without changing the visible title first.
  */
  const project = projectFixture({});
  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [
      sessionFixture({
        runtimeSettings: {
          gxserverFirstPromptAutoTitleStatus: "running",
        },
      }),
    ],
  });

  assert.equal(snapshot.sessions[0]?.isGeneratingFirstPromptTitle, true);

  const appliedSnapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 2 as GxserverPresentationRevision,
    sessions: [
      sessionFixture({
        runtimeSettings: {
          gxserverFirstPromptAutoTitleShouldSubmitStagedCommand: true,
          gxserverFirstPromptAutoTitleStatus: "applied",
        },
      }),
    ],
  });

  assert.equal(appliedSnapshot.sessions[0]?.isGeneratingFirstPromptTitle, false);
  assert.equal(appliedSnapshot.sessions[0]?.shouldSubmitStagedFirstPromptTitleCommand, true);
});

test("agent title metadata debounce runs leading and trailing checks for a burst", () => {
  let nowMs = 0;
  const timers: Array<{ callback: () => void; dueAt: number }> = [];
  const calls: string[] = [];
  const debouncer = createAgentTitleDebouncer({
    delayMs: 3_000,
    nowMs: () => nowMs,
    setTimeout: (callback, delayMs) => {
      timers.push({ callback, dueAt: nowMs + delayMs });
      return 0 as unknown as ReturnType<typeof setTimeout>;
    },
  });

  debouncer.schedule({
    key: "session",
    run: (decision) => calls.push(`${decision.edge}:${decision.suppressedCount}`),
  });
  nowMs = 1_000;
  debouncer.schedule({
    key: "session",
    run: (decision) => calls.push(`${decision.edge}:${decision.suppressedCount}`),
  });
  nowMs = 2_000;
  debouncer.schedule({
    key: "session",
    run: (decision) => calls.push(`${decision.edge}:${decision.suppressedCount}`),
  });
  assert.deepEqual(calls, ["leading:0"]);

  nowMs = 3_000;
  timers.find((timer) => timer.dueAt === 3_000)?.callback();

  assert.deepEqual(calls, ["leading:0", "trailing:2"]);
});

test("presentation delta coalescer flushes the latest session projection once per cadence", () => {
  const timers: Array<() => void> = [];
  const flushes: Array<{ coalescedCount: number; delta: GxserverPresentationDelta; reason: string }> = [];
  const coalescer = new GxserverPresentationDeltaCoalescer({
    delayMs: 250,
    setTimeout: ((callback: () => void) => {
      timers.push(callback);
      return { unref() {} } as ReturnType<typeof setTimeout>;
    }) as typeof setTimeout,
  });
  const projectId = "P3lv0" as GxserverProjectId;
  const sessionId = "G5tpf" as GxserverSessionId;

  coalescer.schedule(
    { projectId, sessionId },
    "title-1",
    presentationDeltaFixture("One"),
    (decision) => flushes.push(decision),
  );
  coalescer.schedule(
    { projectId, sessionId },
    "title-2",
    presentationDeltaFixture("Two"),
    (decision) => flushes.push(decision),
  );

  assert.equal(timers.length, 1);
  timers[0]!();
  assert.equal(flushes.length, 1);
  assert.equal(flushes[0]?.coalescedCount, 1);
  assert.equal(flushes[0]?.reason, "title-2");
  assert.equal(flushes[0]?.delta.type, "sessionPresentationChanged");
  assert.equal(flushes[0]?.delta.type === "sessionPresentationChanged" ? flushes[0].delta.session.title : undefined, "Two");
});

test("presentation snapshot marks command sessions without showing them in workspace sidebar by default", () => {
  const project = projectFixture({});
  const workspace = sessionFixture({
    sessionId: "G5tpf",
    title: "Workspace Agent",
  });
  const command = sessionFixture({
    commandId: "lint",
    sessionId: "G6cmd",
    surface: "commands",
    title: "Lint Command",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-01T11:08:00.000Z",
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [workspace, command],
  });

  assert.deepEqual(snapshot.sessions.map((session) => [session.sessionId, session.surface, session.visibleInSidebarByDefault]), [
    ["G5tpf", "workspace", true],
    ["G6cmd", "commands", false],
  ]);
  assert.deepEqual(snapshot.groups[0]?.sessionIds, ["G5tpf", "G6cmd"]);
});

test("presentation snapshot includes empty projects before their first session", () => {
  const project = projectFixture({
    name: "opencode",
    path: "/Users/madda/dev/_references/opencode",
    projectId: "Popen" as GxserverProjectId,
  });

  const snapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-01T21:14:00.000Z",
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [],
  });

  assert.equal(snapshot.projects[0]?.projectId, "Popen");
  assert.equal(snapshot.projects[0]?.title, "opencode");
  assert.deepEqual(snapshot.projects[0]?.groupIds, ["Popen:active"]);
  assert.deepEqual(snapshot.groups[0]?.sessionIds, []);
  assert.equal(snapshot.sessions.length, 0);
});

test("presentation snapshot carries worktree project metadata", () => {
  const project = projectFixture({
    name: "zmux-feature",
    path: "/Users/madda/dev/_active/zmux-feature",
    projectId: "Pwt01" as GxserverProjectId,
    worktree: {
      branch: "feature",
      name: "feature",
      parentProjectId: "P3lv0",
      parentProjectName: "zmux",
      parentProjectPath: "/Users/madda/dev/_active/zmux",
    },
  });

  const snapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-02T04:16:00.000Z",
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [],
  });

  assert.deepEqual(snapshot.projects[0]?.worktree, project.worktree);
});

test("presentation snapshot carries gxserver title projection semantics", () => {
  const project = projectFixture({});
  const session = sessionFixture({
    runtimeSettings: { titleSource: "terminal-auto" },
    title: "Missing sidebar sessions",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-01T11:08:00.000Z",
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [session],
  });

  const presentation = snapshot.sessions[0];
  assert.ok(presentation, "presentation session exists");
  assert.equal(presentation.title, "Missing sidebar sessions");
  assert.equal(presentation.primaryTitle, "Missing sidebar sessions");
  assert.equal(presentation.terminalTitle, undefined);
  assert.equal(presentation.isPrimaryTitleTerminalTitle, true);
  assert.equal(presentation.isTemporaryTitle, false);
  assert.equal(presentation.titleSource, "terminal-auto");
  assert.equal(presentation.trustedResumeTitle, "Missing sidebar sessions");
});

test("presentation snapshot carries captured agent session identity", () => {
  /*
  CDXC:GxserverPresentationIdentity 2026-06-11-23:58:
  Sidebar tooltips and resume actions need the provider session id from gxserver presentation, not only the routed Ghostex session id. Project the captured identity fields from runtime settings so clients do not have to read hook stores or terminal state files.
  */
  const codexSessionId = "019eb83e-9f8a-7743-9c35-01f5dfd27e1c";
  const codexSessionPath = `/Users/person/.codex/sessions/2026/06/11/rollout-2026-06-11T23-52-52-${codexSessionId}.jsonl`;
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    kind: "agent",
    runtimeSettings: {
      agentName: "codex",
      agentSessionId: codexSessionId,
      agentSessionPath: codexSessionPath,
      titleSource: "generated",
    },
    title: "General Greeting",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-11T19:53:08.000Z",
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [session],
  });

  const presentation = snapshot.sessions[0];
  assert.ok(presentation, "presentation session exists");
  assert.equal(presentation.agentId, "codex");
  assert.equal(presentation.agentName, "codex");
  assert.equal(presentation.agentSessionId, codexSessionId);
  assert.equal(presentation.agentSessionPath, codexSessionPath);
});

test("presentation snapshot resolves missing last-active timestamps from createdAt", () => {
  const project = projectFixture({});
  const missingActivity = sessionFixture({
    createdAt: "2026-06-01T09:00:00.000Z",
    sessionId: "G1old",
    title: "Metadata Refreshed",
    updatedAt: "2026-06-07T05:17:00.000Z",
  });
  const realActivity = sessionFixture({
    createdAt: "2026-06-01T09:30:00.000Z",
    lastActiveAt: "2026-06-02T11:00:00.000Z",
    sessionId: "G2run",
    title: "Actually Active",
    updatedAt: "2026-06-07T05:18:00.000Z",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 2 as GxserverPresentationRevision,
    sessions: [missingActivity, realActivity],
  });

  const projectedMissingActivity = snapshot.sessions.find((session) => session.sessionId === "G1old");
  assert.equal(projectedMissingActivity?.lastActiveAt, missingActivity.createdAt);
  assert.equal(projectedMissingActivity?.updatedAt, missingActivity.updatedAt);
  assert.equal(projectedMissingActivity?.sortKey.includes(missingActivity.createdAt), true);
  assert.equal(projectedMissingActivity?.sortKey.includes(missingActivity.updatedAt), false);
  assert.equal(
    snapshot.sessions.find((session) => session.sessionId === "G2run")?.lastActiveAt,
    realActivity.lastActiveAt,
  );
});

test("presentation snapshot excludes unpinned stopped history but keeps pinned previous sessions", () => {
  const project = projectFixture({});
  const stoppedNoise = sessionFixture({
    lifecycleState: "stopped",
    sessionId: "G1old",
    title: "Old Placeholder",
    updatedAt: "2026-05-01T11:08:00.000Z",
  });
  const pinnedStopped = sessionFixture({
    isPinned: true,
    lifecycleState: "stopped",
    sessionId: "G2pin",
    title: "Pinned History",
    updatedAt: "2026-05-02T11:08:00.000Z",
  });
  const running = sessionFixture({
    lifecycleState: "running",
    sessionId: "G3run",
    title: "Running Shell",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 2 as GxserverPresentationRevision,
    sessions: [stoppedNoise, pinnedStopped, running],
  });

  assert.deepEqual(snapshot.sessions.map((session) => session.sessionId), ["G3run", "G2pin"]);
});

test("presentation snapshot treats existing provider rows as active when domain lifecycle is stale", () => {
  const project = projectFixture({});
  const tuiCreated = sessionFixture({
    kind: "agent",
    lifecycleState: "unknown",
    providerState: { lifecycleState: "exists", zmxName: "S90-P3lv0-G24da" },
    sessionId: "G24da",
    title: "TUI session indicator placement",
    zmxName: "S90-P3lv0-G24da",
  });
  const stoppedWithStaleProvider = sessionFixture({
    lifecycleState: "stopped",
    providerState: { lifecycleState: "exists", zmxName: "S90-P3lv0-G2old" },
    sessionId: "G2old",
    title: "Stopped With Stale Provider",
    zmxName: "S90-P3lv0-G2old",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 3 as GxserverPresentationRevision,
    sessions: [stoppedWithStaleProvider, tuiCreated],
  });

  assert.deepEqual(snapshot.sessions.map((session) => session.sessionId), ["G24da"]);
  assert.equal(snapshot.sessions[0]?.lifecycleState, "running");
  assert.equal(snapshot.sessions[0]?.visibleInSidebarByDefault, true);
  assert.equal(snapshot.sessions[0]?.actions.attach, true);
  assert.equal(snapshot.sessions[0]?.actions.sendMessage, true);
});

test("presentation snapshot orders pinned project sessions by sidebar order", () => {
  const project = projectFixture({});
  const first = sessionFixture({
    isPinned: true,
    sessionId: "G1aaa",
    sidebarOrder: 1000,
    title: "First",
    updatedAt: "2026-06-02T18:00:00.000Z",
  });
  const second = sessionFixture({
    isPinned: true,
    sessionId: "G2bbb",
    sidebarOrder: 0,
    title: "Second",
    updatedAt: "2026-06-02T17:00:00.000Z",
  });

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 2 as GxserverPresentationRevision,
    sessions: [first, second],
  });

  assert.deepEqual(snapshot.sessions.map((session) => session.sessionId), ["G2bbb", "G1aaa"]);
  assert.deepEqual(snapshot.sessions.map((session) => session.sidebarOrder), [0, 1000]);
});

test("presentation snapshot applies stale spinner activity semantics", () => {
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentActivity: {
        activity: "working",
        agentName: "codex",
        hasSeenWorking: true,
        isAcknowledged: false,
        lastTitle: "⠏ Skip migration issue 2 options",
        lastTitleChangeAt: "2026-06-01T12:00:00.000Z",
        workingSource: "title",
        workingStartedAt: "2026-06-01T12:00:00.000Z",
      },
      agentName: "codex",
      titleSource: "terminal-auto",
    },
    title: "Skip migration issue 2 options",
  });

  const freshSnapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-01T12:00:02.000Z",
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [session],
  });
  assert.equal(freshSnapshot.sessions[0]?.activity, "working");
  assert.deepEqual(freshSnapshot.sessions[0]?.actions, {
    acknowledgeAttention: false,
    attach: true,
    focus: true,
    kill: true,
    readText: true,
    sendMessage: true,
    sendText: true,
    sleep: true,
    wake: false,
  });

  const oldBoundarySnapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-01T12:00:04.000Z",
    projects: [project],
    revision: 2 as GxserverPresentationRevision,
    sessions: [session],
  });
  assert.equal(oldBoundarySnapshot.sessions[0]?.activity, "working");
  assert.equal(oldBoundarySnapshot.sessions[0]?.actions.acknowledgeAttention, false);

  const staleSnapshot = projectGxserverPresentationSnapshot({
    generatedAt: "2026-06-01T12:00:06.000Z",
    projects: [project],
    revision: 3 as GxserverPresentationRevision,
    sessions: [session],
  });
  assert.equal(staleSnapshot.sessions[0]?.activity, "idle");
  assert.equal(staleSnapshot.sessions[0]?.actions.acknowledgeAttention, false);
});

test("presentation attention includes a stable event id", () => {
  /*
  CDXC:SessionAttention 2026-06-08-13:19:
  macOS presentation deltas are a first-class attention source. Publish the
  stable event id with the attention projection so clients can play completion
  sound once for a fresh attention event without replaying audio for snapshots.
  */
  const project = projectFixture({});
  const session = sessionFixture({
    agentId: "codex",
    runtimeSettings: {
      agentActivity: {
        activity: "attention",
        agentName: "codex",
        attentionEventId: "attn_mq4zf2ae",
        hasSeenWorking: true,
        isAcknowledged: false,
        lastChangedAt: "2026-06-08T09:01:44.918Z",
      },
      agentName: "codex",
    },
  });

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 4 as GxserverPresentationRevision,
    sessions: [session],
  });

  assert.equal(snapshot.sessions[0]?.activity, "attention");
  assert.deepEqual(snapshot.sessions[0]?.attention, {
    acknowledged: false,
    enteredAt: "2026-06-08T09:01:44.918Z",
    eventId: "attn_mq4zf2ae",
  });
});

test("metadata search can page previous sessions without hydrating them into the active snapshot", () => {
  const project = projectFixture({ name: "Ghostex" });
  const active = sessionFixture({
    lifecycleState: "running",
    sessionId: "G3run",
    title: "Active Build",
  });
  const previous = sessionFixture({
    agentId: "codex",
    cwd: "/Users/madda/dev/_active/zmux",
    lifecycleState: "stopped",
    runtimeSettings: { titleSource: "terminal-auto" },
    sessionId: "G4old",
    title: "Presentation Cutover",
    updatedAt: "2026-06-01T10:08:00.000Z",
  });

  const activeSnapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 3 as GxserverPresentationRevision,
    sessions: [active, previous],
  });
  assert.deepEqual(activeSnapshot.sessions.map((session) => session.sessionId), ["G3run"]);

  const search = searchGxserverPresentationSessions(
    { projects: [project], sessions: [active, previous] },
    {
      includeActive: false,
      includePrevious: true,
      query: "cutover",
    },
  );

  assert.equal(search.results.length, 1);
  assert.equal(search.results[0]?.sessionId, "G4old");
  assert.equal(search.results[0]?.match?.field, "title");
  assert.equal(search.results[0]?.primaryTitle, "Presentation Cutover");
  assert.equal(search.results[0]?.terminalTitle, undefined);
  assert.equal(search.results[0]?.isPrimaryTitleTerminalTitle, true);
  assert.equal(search.results[0]?.isTemporaryTitle, false);
  assert.equal(search.results[0]?.titleSource, "terminal-auto");
  assert.equal(search.results[0]?.trustedResumeTitle, "Presentation Cutover");
  assert.equal(search.results[0]?.surface, "workspace");
});

test("presentation search resolves missing last-active timestamps before ranking results", () => {
  const project = projectFixture({ name: "Ghostex" });
  const metadataRefreshed = sessionFixture({
    createdAt: "2026-06-01T09:00:00.000Z",
    sessionId: "G1meta",
    title: "Metadata Refreshed",
    updatedAt: "2026-06-07T05:17:00.000Z",
  });
  const actuallyRecent = sessionFixture({
    createdAt: "2026-06-01T09:30:00.000Z",
    lastActiveAt: "2026-06-02T11:00:00.000Z",
    sessionId: "G2real",
    title: "Actually Recent",
    updatedAt: "2026-06-01T09:31:00.000Z",
  });

  const search = searchGxserverPresentationSessions(
    { projects: [project], sessions: [metadataRefreshed, actuallyRecent] },
    {},
  );

  assert.deepEqual(
    search.results.map((result) => result.sessionId),
    ["G2real", "G1meta"],
  );
  assert.equal(search.results.find((result) => result.sessionId === "G1meta")?.lastActiveAt, metadataRefreshed.createdAt);
  assert.equal(search.results.find((result) => result.sessionId === "G1meta")?.updatedAt, metadataRefreshed.updatedAt);
});

test("previous sessions search hides placeholder inactive rows but keeps restorable history", () => {
  const project = projectFixture({ name: "Ghostex" });
  const trusted = sessionFixture({
    agentId: "codex",
    createdAt: "2026-06-01T10:00:00.000Z",
    lifecycleState: "stopped",
    runtimeSettings: { titleSource: "terminal-auto" },
    sessionId: "G1trust",
    title: "Fix previous session list",
    updatedAt: "2026-06-01T10:08:00.000Z",
  });
  const placeholder = sessionFixture({
    lifecycleState: "stopped",
    runtimeSettings: { titleSource: "placeholder" },
    sessionId: "G2noise",
    title: "Terminal Session",
  });
  const unknown = sessionFixture({
    lifecycleState: "unknown",
    runtimeSettings: { titleSource: "terminal-auto" },
    sessionId: "G3unkn",
    title: "Unknown but titled",
  });
  const favoritePlaceholder = sessionFixture({
    isFavorite: true,
    lifecycleState: "stopped",
    runtimeSettings: { titleSource: "placeholder" },
    sessionId: "G4fav",
    title: "Codex Session",
  });
  const commandPane = sessionFixture({
    commandId: "start",
    lifecycleState: "stopped",
    runtimeSettings: { titleSource: "terminal-auto" },
    sessionId: "G5cmd",
    surface: "commands",
    title: "bun run start",
  });
  const pinnedCommandPane = sessionFixture({
    commandId: "test",
    isPinned: true,
    lifecycleState: "stopped",
    runtimeSettings: { titleSource: "terminal-auto" },
    sessionId: "G6pin",
    surface: "commands",
    title: "bun test",
  });

  /*
  CDXC:PreviousSessions 2026-06-04-20:21:
  listPreviousSessions should be a useful restore list, not every inactive gxserver row. Hide unpinned placeholder and unknown rows while preserving trusted stopped rows and rows the user explicitly kept with Favorite/Pin.

  CDXC:PreviousSessions 2026-06-07-05:28:
  Command-pane sessions are not previous workspace sessions. Keep `surface: "commands"` rows out of listPreviousSessions even when they have trusted terminal titles or pinned state, because clients should not show command runs like `bun run start` in the Previous Sessions modal.
  */
  const search = searchGxserverPreviousSessions(
    { projects: [project], sessions: [trusted, placeholder, unknown, favoritePlaceholder, commandPane, pinnedCommandPane] },
    { includeActive: false, includePrevious: true },
  );

  assert.deepEqual(
    search.results.map((result) => result.sessionId),
    ["G1trust", "G4fav"],
  );
  assert.equal(search.results.find((result) => result.sessionId === "G1trust")?.createdAt, trusted.createdAt);
  assert.equal(search.results.find((result) => result.sessionId === "G1trust")?.updatedAt, trusted.updatedAt);
});

test("presentation projects sanitized zmx title-observer health", () => {
  const project = projectFixture({});
  const session = sessionFixture({
    runtimeSettings: {
      zmxTitleObservation: {
        failureCount: 2,
        lastFailedAt: "2026-06-07T00:29:59.000Z",
        lastObservedAt: "2026-06-07T00:29:40.000Z",
        lastStartedAt: "2026-06-07T00:29:58.000Z",
        nextRetryAt: "2026-06-07T00:30:00.000Z",
        rawTitle: "private terminal title",
        status: "retrying",
      },
    },
  });

  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 4 as GxserverPresentationRevision,
    sessions: [session],
  });

  assert.deepEqual(snapshot.sessions[0]?.titleObservation, {
    failureCount: 2,
    lastFailedAt: "2026-06-07T00:29:59.000Z",
    lastObservedAt: "2026-06-07T00:29:40.000Z",
    lastStartedAt: "2026-06-07T00:29:58.000Z",
    nextRetryAt: "2026-06-07T00:30:00.000Z",
    status: "retrying",
  });
  assert.equal(JSON.stringify(snapshot.sessions[0]).includes("private terminal title"), false);
});

class MockPresentationRepository {
  readonly #project: GxserverProjectDomainState;
  #sessions: GxserverSessionDomainState[];

  constructor(project: GxserverProjectDomainState, sessions: GxserverSessionDomainState[]) {
    this.#project = project;
    this.#sessions = sessions;
  }

  getProject(projectId: GxserverProjectId): GxserverProjectDomainState | undefined {
    return projectId === this.#project.projectId ? this.#project : undefined;
  }

  getSession(projectId: GxserverProjectId, sessionId: GxserverSessionId): GxserverSessionDomainState | undefined {
    return this.#sessions.find((session) => session.projectId === projectId && session.sessionId === sessionId);
  }

  listSessions(projectId?: GxserverProjectId): GxserverSessionDomainState[] {
    return projectId ? this.#sessions.filter((session) => session.projectId === projectId) : this.#sessions;
  }

  updateSession(input: GxserverUpdateSessionParams): GxserverSessionDomainState {
    const current = this.getSession(input.projectId, input.sessionId);
    assert.ok(current, "mock session exists");
    const next: GxserverSessionDomainState = {
      ...current,
      ...(input.agentId !== undefined ? { agentId: input.agentId } : {}),
      ...(input.kind !== undefined ? { kind: input.kind } : {}),
      ...(input.runtimeSettings !== undefined ? { runtimeSettings: input.runtimeSettings } : {}),
      ...(input.title !== undefined ? { title: input.title } : {}),
      updatedAt: "2026-05-31T21:10:00.000Z",
    };
    this.#sessions = this.#sessions.map((session) =>
      session.projectId === input.projectId && session.sessionId === input.sessionId ? next : session,
    );
    return next;
  }
}

function projectFixture(partial: Partial<GxserverProjectDomainState>): GxserverProjectDomainState {
  return {
    attentionRules: {},
    completionRules: {},
    createdAt: "2026-05-31T21:00:00.000Z",
    customAgentOrder: [],
    customAgents: [],
    customCommandOrder: [],
    customCommands: [],
    deletedDefaultCommandIds: [],
    gitConfig: {},
    isFavorite: false,
    isPinned: false,
    launchSettings: {},
    name: "zmux",
    notificationRules: {},
    path: "/Users/madda/dev/_active/zmux",
    previousSessionHistory: [],
    projectBoardConfig: {},
    projectId: "P3lv0",
    runtimeSettings: {},
    updatedAt: "2026-05-31T21:00:00.000Z",
    ...partial,
  };
}

function sessionFixture(partial: Partial<GxserverSessionDomainState>): GxserverSessionDomainState {
  return {
    attentionRules: {},
    completionRules: {},
    createdAt: "2026-05-31T21:00:00.000Z",
    globalRef: "S90:P3lv0:G5tpf",
    hiddenMetadata: {},
    isFavorite: false,
    isPinned: false,
    kind: "terminal",
    launchSettings: {},
    lifecycleState: "running",
    notificationRules: {},
    projectId: "P3lv0",
    providerState: { lifecycleState: "exists", zmxName: "S90-P3lv0-G5tpf" },
    runtimeSettings: {},
    sessionId: "G5tpf",
    surface: "workspace",
    title: "Terminal Session",
    updatedAt: "2026-05-31T21:00:00.000Z",
    zmxName: "S90-P3lv0-G5tpf",
    ...partial,
  };
}

function presentationDeltaFixture(title: string): GxserverPresentationDelta {
  const project = projectFixture({});
  const snapshot = projectGxserverPresentationSnapshot({
    projects: [project],
    revision: 1 as GxserverPresentationRevision,
    sessions: [sessionFixture({ title })],
  });
  const session = snapshot.sessions[0];
  assert.ok(session, "presentation session exists");
  return {
    session,
    type: "sessionPresentationChanged",
  };
}
