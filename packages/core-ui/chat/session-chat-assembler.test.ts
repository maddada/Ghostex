import { describe, expect, test } from "vitest";
import type { SessionChatMessage, SessionChatSource } from "../../shared/session-chat";
import {
  applySessionChatAppends,
  assembleSessionChatMessages,
  compareSessionChatMessages,
  createIncrementalSessionChatAssembler,
  resetIncrementalSessionChatAssembler,
} from "./session-chat-assembler";

function msg(
  id: string,
  overrides: Partial<SessionChatMessage> & { source?: SessionChatSource } = {},
): SessionChatMessage {
  return {
    blocks: [{ text: `body of ${id}`, type: "text" }],
    id,
    role: "assistant",
    source: "transcript",
    timestamp: null,
    ...overrides,
  };
}

describe("cross-source turn dedup (§6.2)", () => {
  test("same text from a different source collapses to the higher priority copy", () => {
    const transcriptCopy = msg("t1", {
      blocks: [{ text: "hello world", type: "text" }],
      role: "user",
      timestamp: 100,
    });
    const clientCopy = msg("pending:1", {
      blocks: [{ text: "hello   world", type: "text" }],
      role: "user",
      source: "client",
      timestamp: 90,
    });
    const assembled = assembleSessionChatMessages({
      client: [clientCopy],
      transcript: [transcriptCopy],
    });
    expect(assembled).toHaveLength(1);
    expect(assembled[0]?.id).toBe("t1");
    expect(assembled[0]?.source).toBe("transcript");
  });

  test("a lower-priority copy arriving after never replaces (strict supersedes)", () => {
    const assembler = createIncrementalSessionChatAssembler();
    resetIncrementalSessionChatAssembler(assembler, [
      msg("t1", { blocks: [{ text: "same", type: "text" }], role: "user", timestamp: 5 }),
    ]);
    applySessionChatAppends(assembler, [
      msg("pending:9", {
        blocks: [{ text: "same", type: "text" }],
        role: "user",
        source: "client",
        timestamp: 6,
      }),
    ]);
    expect(assembler.messages).toHaveLength(1);
    expect(assembler.messages[0]?.id).toBe("t1");
  });

  test("a higher-priority copy replaces the client echo in place", () => {
    const assembler = createIncrementalSessionChatAssembler();
    resetIncrementalSessionChatAssembler(assembler, [
      msg("pending:1", {
        blocks: [{ text: "run tests", type: "text" }],
        role: "user",
        source: "client",
        timestamp: 10,
      }),
    ]);
    applySessionChatAppends(assembler, [
      msg("t2", {
        blocks: [{ text: "run tests", type: "text" }],
        role: "user",
        timestamp: 12,
      }),
    ]);
    expect(assembler.messages).toHaveLength(1);
    expect(assembler.messages[0]?.id).toBe("t2");
    expect(assembler.messages[0]?.source).toBe("transcript");
  });

  test("two identical same-source prompts stay distinct (cross-source only)", () => {
    const first = msg("u1", {
      blocks: [{ text: "continue", type: "text" }],
      role: "user",
      timestamp: 1,
    });
    const second = msg("u2", {
      blocks: [{ text: "continue", type: "text" }],
      role: "user",
      timestamp: 2,
    });
    const assembled = assembleSessionChatMessages({ transcript: [first, second] });
    expect(assembled).toHaveLength(2);
  });

  test("explicit turnId dedups across sources regardless of text", () => {
    const hookCopy = msg("h1", {
      blocks: [{ text: "partial…", type: "text" }],
      source: "hook",
      timestamp: null,
      turnId: "turn-a",
    });
    const transcriptCopy = msg("t1", {
      blocks: [{ text: "full reply", type: "text" }],
      timestamp: 50,
      turnId: "turn-a",
    });
    const assembled = assembleSessionChatMessages({
      hook: [hookCopy],
      transcript: [transcriptCopy],
    });
    expect(assembled).toHaveLength(1);
    expect(assembled[0]?.id).toBe("t1");
  });

  test("tool-call-only turns with no text do not collapse (non-text digest)", () => {
    const callA = msg("a", {
      blocks: [{ input: { cmd: "ls" }, name: "Bash", type: "tool-call" }],
      timestamp: 1,
    });
    const callB = msg("b", {
      blocks: [{ input: { cmd: "pwd" }, name: "Bash", type: "tool-call" }],
      source: "hook",
      timestamp: 2,
    });
    const assembled = assembleSessionChatMessages({
      hook: [callB],
      transcript: [callA],
    });
    expect(assembled).toHaveLength(2);
  });
});

describe("sort order (§6.3)", () => {
  test("streaming ranks after transcript, pending ranks last", () => {
    const transcriptMsg = msg("t", { timestamp: 100 });
    const streaming = msg("streaming", { source: "hook", timestamp: null });
    const pendingMsg = msg("pending:1", { source: "client", timestamp: 50 });
    const ordered = [pendingMsg, streaming, transcriptMsg].sort(
      compareSessionChatMessages,
    );
    expect(ordered.map((m) => m.id)).toEqual(["t", "streaming", "pending:1"]);
  });

  test("null timestamps sort before real timestamps within a rank", () => {
    const a = msg("a", { timestamp: null });
    const b = msg("b", { timestamp: 1 });
    expect([b, a].sort(compareSessionChatMessages).map((m) => m.id)).toEqual([
      "a",
      "b",
    ]);
  });
});

describe("incremental assembler invariant (§6.4)", () => {
  test("applyAppends deep-equals a full rebuild for every append prefix", () => {
    const base = [
      msg("t1", { role: "user", timestamp: 10 }),
      msg("t2", { timestamp: 20 }),
      msg("third", {
        blocks: [{ input: {}, name: "Read", type: "tool-call" }],
        timestamp: 30,
      }),
    ];
    const appendBatches: SessionChatMessage[][] = [
      // Plain tail append.
      [msg("t4", { timestamp: 40 })],
      // Duplicate id re-emit (same source refresh via merger; here same id
      // lower/equal priority is a no-op in the assembler).
      [msg("t4", { blocks: [{ text: "updated", type: "text" }], timestamp: 40 })],
      // Cross-source echo that must collapse into t5.
      [
        msg("pending:1", {
          blocks: [{ text: "do it", type: "text" }],
          role: "user",
          source: "client",
          timestamp: 45,
        }),
      ],
      [
        msg("t5", {
          blocks: [{ text: "do it", type: "text" }],
          role: "user",
          timestamp: 50,
        }),
      ],
      // Out-of-order timestamp (not a tail append) and a null timestamp.
      [msg("t0", { timestamp: 5 }), msg("tnull", { timestamp: null })],
      // Empty batch.
      [],
      [msg("t6", { timestamp: 60 }), msg("t7", { timestamp: 60 })],
    ];

    const assembler = createIncrementalSessionChatAssembler();
    resetIncrementalSessionChatAssembler(assembler, base);
    let allAppends: SessionChatMessage[] = [];
    for (const batch of appendBatches) {
      applySessionChatAppends(assembler, batch);
      allAppends = [...allAppends, ...batch];
      const oracle = createIncrementalSessionChatAssembler();
      resetIncrementalSessionChatAssembler(oracle, [...base, ...allAppends]);
      expect(assembler.messages).toEqual(oracle.messages);
    }
  });

  test("reset matches one-shot assembly byte for byte", () => {
    const sources = {
      client: [msg("pending:1", { role: "user", source: "client" as const, timestamp: 3 })],
      hook: [msg("h1", { source: "hook" as const, timestamp: 2 })],
      transcript: [msg("t1", { timestamp: 1 })],
    };
    const oneShot = assembleSessionChatMessages(sources);
    const assembler = createIncrementalSessionChatAssembler();
    resetIncrementalSessionChatAssembler(assembler, [
      ...sources.transcript,
      ...sources.hook,
      ...sources.client,
    ]);
    expect(assembler.messages).toEqual(oneShot);
  });
});
