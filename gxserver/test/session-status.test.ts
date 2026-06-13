import test from "node:test";
import assert from "node:assert/strict";
import { applyAgentActivityTransition } from "../src/session-status/index.js";

test("explicit hook activity bypasses launch suppression and plain title downgrades", () => {
  /*
  CDXC:SessionStatus 2026-06-07-09:22:
  gxserver hook events are the authoritative working-state source. A direct hook
  working event must win during launch suppression, and unrelated terminal
  titles such as Monaco Ctrl+G must not downgrade explicit working. A stopped
  spinner title for the same semantic title still clears working because some
  agents do not reliably emit a hook stop event.
  */
  const launched = applyAgentActivityTransition({
    agentId: "codex",
    event: "launch",
    nowMs: Date.parse("2026-06-07T06:47:25.000Z"),
  });
  assert.equal(launched.activity, "idle");
  assert.equal(launched.suppressedUntil, "2026-06-07T06:47:37.000Z");

  const working = applyAgentActivityTransition({
    activity: "working",
    agentId: "codex",
    nowMs: Date.parse("2026-06-07T06:47:30.000Z"),
    previous: launched,
  });
  assert.equal(working.activity, "working");
  assert.equal(working.workingSource, "explicit");

  const plainTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-07T06:47:31.000Z"),
    previous: working,
    title: "Monaco Ctrl+G Switch",
  });
  assert.equal(plainTitle.activity, "working");
  assert.equal(plainTitle.workingSource, "explicit");

  const attention = applyAgentActivityTransition({
    activity: "attention",
    agentId: "codex",
    nowMs: Date.parse("2026-06-07T06:47:31.500Z"),
    previous: plainTitle,
  });
  assert.equal(attention.activity, "attention");
});

test("wake suppression ignores title-derived attention from a resumed sleeping session", () => {
  const wake = applyAgentActivityTransition({
    agentId: "codex",
    event: "wake",
    nowMs: Date.parse("2026-06-10T07:27:00.000Z"),
    previous: {
      activity: "working",
      agentName: "codex",
      hasSeenWorking: true,
      isAcknowledged: false,
      lastChangedAt: "2026-06-10T07:20:00.000Z",
      workingStartedAt: "2026-06-10T07:20:00.000Z",
    },
  });

  assert.equal(wake.activity, "idle");
  assert.equal(wake.hasSeenWorking, false);
  assert.equal(wake.isAcknowledged, true);
  assert.equal(wake.suppressedUntil, "2026-06-10T07:27:12.000Z");

  const wakeTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-10T07:27:05.000Z"),
    previous: wake,
    title: "[ ! ] Action Required",
  });

  assert.equal(wakeTitle.activity, "idle");
  assert.equal(wakeTitle.hasSeenWorking, false);
  assert.equal(wakeTitle.isAcknowledged, true);
  assert.equal(wakeTitle.suppressedUntil, "2026-06-10T07:27:12.000Z");
});

test("escape suppresses done attention for five seconds without clearing working", () => {
  const attention = applyAgentActivityTransition({
    activity: "attention",
    agentId: "codex",
    nowMs: Date.parse("2026-06-11T04:46:00.000Z"),
    previous: {
      activity: "working",
      agentName: "codex",
      hasSeenWorking: true,
      isAcknowledged: false,
      lastChangedAt: "2026-06-11T04:45:54.000Z",
      workingStartedAt: "2026-06-11T04:45:54.000Z",
    },
  });
  assert.equal(attention.activity, "attention");
  assert.equal(attention.attentionEventId, "attn_mq90lq2o");

  const escapedAttention = applyAgentActivityTransition({
    agentId: "codex",
    event: "escape",
    nowMs: Date.parse("2026-06-11T04:46:01.000Z"),
    previous: attention,
  });
  assert.equal(escapedAttention.activity, "idle");
  assert.equal(escapedAttention.attentionEventId, undefined);
  assert.equal(escapedAttention.attentionSuppressedUntil, "2026-06-11T04:46:06.000Z");
  assert.equal(escapedAttention.isAcknowledged, true);

  const working = applyAgentActivityTransition({
    activity: "working",
    agentId: "codex",
    nowMs: Date.parse("2026-06-11T04:46:10.000Z"),
    previous: escapedAttention,
  });
  assert.equal(working.activity, "working");
  assert.equal(working.attentionSuppressedUntil, undefined);

  const escapedWorking = applyAgentActivityTransition({
    agentId: "codex",
    event: "escape",
    nowMs: Date.parse("2026-06-11T04:46:11.000Z"),
    previous: working,
  });
  assert.equal(escapedWorking.activity, "working");
  assert.equal(escapedWorking.attentionSuppressedUntil, "2026-06-11T04:46:16.000Z");
  assert.equal(escapedWorking.workingSource, "explicit");
  assert.equal(escapedWorking.workingStartedAt, "2026-06-11T04:46:10.000Z");

  const suppressedTitleDone = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-11T04:46:12.000Z"),
    previous: escapedWorking,
    title: "[ ! ] Action Required",
  });
  assert.equal(suppressedTitleDone.activity, "idle");
  assert.equal(suppressedTitleDone.attentionSuppressedUntil, "2026-06-11T04:46:16.000Z");
  assert.equal(suppressedTitleDone.hasSeenWorking, false);
  assert.equal(suppressedTitleDone.isAcknowledged, true);

  const explicitDoneAfterWindow = applyAgentActivityTransition({
    activity: "attention",
    agentId: "codex",
    nowMs: Date.parse("2026-06-11T04:46:17.000Z"),
    previous: suppressedTitleDone,
  });
  assert.equal(explicitDoneAfterWindow.activity, "attention");
  assert.equal(explicitDoneAfterWindow.attentionSuppressedUntil, undefined);
});

test("escape done suppression does not suppress terminal error attention", () => {
  const escapedWorking = applyAgentActivityTransition({
    agentId: "codex",
    event: "escape",
    nowMs: Date.parse("2026-06-11T04:46:11.000Z"),
    previous: {
      activity: "working",
      agentName: "codex",
      hasSeenWorking: true,
      isAcknowledged: false,
      lastChangedAt: "2026-06-11T04:46:10.000Z",
      workingSource: "explicit",
      workingStartedAt: "2026-06-11T04:46:10.000Z",
    },
  });
  assert.equal(escapedWorking.activity, "working");
  assert.equal(escapedWorking.attentionSuppressedUntil, "2026-06-11T04:46:16.000Z");

  const terminalError = applyAgentActivityTransition({
    agentId: "codex",
    event: "terminalError",
    nowMs: Date.parse("2026-06-11T04:46:12.000Z"),
    previous: escapedWorking,
  });
  assert.equal(terminalError.activity, "attention");
  assert.equal(terminalError.attentionEventId, "attn_mq90lzc0");
});

test("same-title Codex spinner stop clears explicit hook working", () => {
  const titleWorking = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-07T04:53:27.000Z"),
    title: "⠏ Ghostex 4.0.0 Beta",
  });
  assert.equal(titleWorking.activity, "working");
  assert.equal(titleWorking.workingSource, "title");

  const explicitWorking = applyAgentActivityTransition({
    activity: "working",
    agentId: "codex",
    nowMs: Date.parse("2026-06-07T04:53:29.000Z"),
    previous: titleWorking,
  });
  assert.equal(explicitWorking.activity, "working");
  assert.equal(explicitWorking.workingSource, "explicit");

  const unrelatedTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-07T04:53:31.000Z"),
    previous: explicitWorking,
    title: "Monaco Ctrl+G Switch",
  });
  assert.equal(unrelatedTitle.activity, "working");
  assert.equal(unrelatedTitle.workingSource, "explicit");
  assert.equal(unrelatedTitle.lastTitle, "⠏ Ghostex 4.0.0 Beta");

  const stoppedSpinner = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-07T05:00:55.000Z"),
    previous: unrelatedTitle,
    title: "Ghostex 4.0.0 Beta",
  });
  assert.equal(stoppedSpinner.activity, "attention");
  assert.equal(stoppedSpinner.workingSource, undefined);
  assert.equal(stoppedSpinner.lastTitle, "Ghostex 4.0.0 Beta");
});

test("title-only Codex spinner stop settles idle without attention", () => {
  const titleWorking = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T15:46:50.000Z"),
    title: "⠦ Agent Detection Sync",
  });
  assert.equal(titleWorking.activity, "working");
  assert.equal(titleWorking.workingSource, "title");

  const settledTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T15:46:58.000Z"),
    previous: titleWorking,
    title: "Agent Detection Sync",
  });
  assert.equal(settledTitle.activity, "idle");
  assert.equal(settledTitle.attentionEventId, undefined);
  assert.equal(settledTitle.workingSource, undefined);

  const staleTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T15:46:58.000Z"),
    previous: titleWorking,
    title: "⠦ Agent Detection Sync",
  });
  assert.equal(staleTitle.activity, "idle");
  assert.equal(staleTitle.attentionEventId, undefined);
  assert.equal(staleTitle.workingSource, undefined);
});

test("trusted settled Codex title clears explicit working when spinner was missed", () => {
  /*
  CDXC:SessionStatus 2026-06-12-04:06:
  A missed Codex Stop hook must not leave a session orange forever when gxserver later observes the stable task title that matches the trusted session title. Keep the fallback exact-title and age gated so editor titles or startup snapshots do not downgrade real work.
  */
  const working = applyAgentActivityTransition({
    activity: "working",
    agentId: "codex",
    nowMs: Date.parse("2026-06-12T00:02:06.814Z"),
    previous: {
      activity: "idle",
      agentName: "codex",
      hasSeenWorking: false,
      isAcknowledged: true,
      lastChangedAt: "2026-06-12T00:02:00.000Z",
    },
  });
  assert.equal(working.activity, "working");
  assert.equal(working.workingSource, "explicit");

  const startupSnapshot = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T00:02:09.000Z"),
    previous: working,
    settledTitle: "Agents Hub Visibility Fix",
    title: "Agents Hub Visibility Fix",
  });
  assert.equal(startupSnapshot.activity, "working");
  assert.equal(startupSnapshot.workingSource, "explicit");

  const unrelatedTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T00:02:20.000Z"),
    previous: startupSnapshot,
    settledTitle: "Agents Hub Visibility Fix",
    title: "Monaco Ctrl+G Switch",
  });
  assert.equal(unrelatedTitle.activity, "working");
  assert.equal(unrelatedTitle.workingSource, "explicit");

  const settledTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T00:04:03.548Z"),
    previous: unrelatedTitle,
    settledTitle: "Agents Hub Visibility Fix",
    title: "Agents Hub Visibility Fix",
  });
  assert.equal(settledTitle.activity, "attention");
  assert.equal(settledTitle.workingSource, undefined);
  assert.equal(settledTitle.lastTitle, "Agents Hub Visibility Fix");
});

test("trusted settled Codex title respects acknowledged working state", () => {
  const settledTitle = applyAgentActivityTransition({
    agentId: "codex",
    event: "title",
    nowMs: Date.parse("2026-06-12T00:04:03.548Z"),
    previous: {
      activity: "working",
      agentName: "codex",
      attentionSuppressedUntil: "2026-06-12T00:03:33.187Z",
      hasSeenWorking: true,
      isAcknowledged: true,
      lastChangedAt: "2026-06-12T00:02:06.814Z",
      workingSource: "explicit",
      workingStartedAt: "2026-06-12T00:02:06.814Z",
    },
    settledTitle: "Agents Hub Visibility Fix",
    title: "Agents Hub Visibility Fix",
  });

  assert.equal(settledTitle.activity, "idle");
  assert.equal(settledTitle.workingSource, undefined);
  assert.equal(settledTitle.workingStartedAt, undefined);
});

test("Claude idle terminal titles settle without synthesizing done attention", () => {
  /*
  CDXC:ClaudeSessionStatus 2026-06-11-21:43:
  Claude Code uses idle terminal titles as a settled-running UI state. gxserver must not convert those title observations into Done/attention because Claude's explicit notification hooks are the user-attention signal shared by every client.
  */
  const titleWorking = applyAgentActivityTransition({
    agentId: "claude",
    event: "title",
    nowMs: Date.parse("2026-06-11T17:43:00.000Z"),
    title: "✶ Claude Code",
  });
  assert.equal(titleWorking.activity, "working");
  assert.equal(titleWorking.agentName, "claude");

  const explicitWorking = applyAgentActivityTransition({
    activity: "working",
    agentId: "claude",
    nowMs: Date.parse("2026-06-11T17:43:01.000Z"),
    previous: titleWorking,
  });
  assert.equal(explicitWorking.activity, "working");

  const settledIdle = applyAgentActivityTransition({
    agentId: "claude",
    event: "title",
    nowMs: Date.parse("2026-06-11T17:43:08.000Z"),
    previous: explicitWorking,
    title: "✳ Claude Code",
  });
  assert.equal(settledIdle.activity, "idle");
  assert.equal(settledIdle.attentionEventId, undefined);

  const staleWorking = applyAgentActivityTransition({
    agentId: "claude",
    event: "title",
    nowMs: Date.parse("2026-06-11T17:43:20.000Z"),
    previous: titleWorking,
    title: "✶ Claude Code",
  });
  assert.equal(staleWorking.activity, "idle");
  assert.equal(staleWorking.attentionEventId, undefined);
});
