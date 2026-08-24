import { describe, expect, test } from "vitest";
import type { SessionChatMessage } from "../../shared/session-chat";
import {
  applySessionChatCommandMarkerBoundaries,
  assignSessionChatPendingOccurrence,
  countLeadingPendingTextsGluedToUserText,
  isSessionChatClearCommand,
  pruneSessionChatPendingSends,
  sessionChatCommandMarkersAsMessages,
  sessionChatPendingContentKey,
  visibleSessionChatPendingSends,
  type SessionChatPendingSend,
} from "./session-chat-pending";

function userMsg(id: string, text: string, timestamp: number): SessionChatMessage {
  return {
    blocks: [{ text, type: "text" }],
    id,
    role: "user",
    source: "transcript",
    timestamp,
  };
}

function assistantMsg(id: string, timestamp: number): SessionChatMessage {
  return {
    blocks: [{ text: "reply", type: "text" }],
    id,
    role: "assistant",
    source: "transcript",
    timestamp,
  };
}

function pendingEntry(
  id: string,
  text: string,
  overrides: Partial<SessionChatPendingSend> = {},
): SessionChatPendingSend {
  return {
    afterMessageId: null,
    afterMessageTimestamp: null,
    id,
    sentAt: 1000,
    text,
    ...overrides,
  };
}

describe("pending prune (§10.3)", () => {
  test("echo is KEPT while only the matching user row exists (user-only phase)", () => {
    const pending = [pendingEntry("p1", "hello")];
    const messages = [userMsg("u1", "hello", 1500)];
    expect(pruneSessionChatPendingSends(pending, messages)).toBe(pending);
  });

  test("echo is pruned once an assistant turn lands after the user row", () => {
    const pending = [pendingEntry("p1", "hello")];
    const messages = [userMsg("u1", "hello", 1500), assistantMsg("a1", 1600)];
    expect(pruneSessionChatPendingSends(pending, messages)).toEqual([]);
  });

  test("visibility hides the echo as soon as the user row exists", () => {
    const pending = [pendingEntry("p1", "hello")];
    const messages = [userMsg("u1", "hello", 1500)];
    expect(visibleSessionChatPendingSends(pending, messages)).toEqual([]);
  });

  test("messages before the send-time boundary do not consume the echo", () => {
    const pending = [pendingEntry("p1", "hello", { sentAt: 2000 })];
    const messages = [userMsg("old", "hello", 500), assistantMsg("a0", 600)];
    expect(pruneSessionChatPendingSends(pending, messages)).toBe(pending);
  });

  test("two identical sends need two advanced occurrences", () => {
    const first = pendingEntry("p1", "again");
    const second = assignSessionChatPendingOccurrence(
      [first],
      pendingEntry("p2", "again"),
    );
    expect(second.matchingOccurrence).toBe(2);
    const oneOccurrence = [
      userMsg("u1", "again", 1500),
      assistantMsg("a1", 1600),
    ];
    const afterOne = pruneSessionChatPendingSends([first, second], oneOccurrence);
    expect(afterOne.map((entry) => entry.id)).toEqual(["p2"]);
    const twoOccurrences = [
      ...oneOccurrence,
      userMsg("u2", "again", 1700),
      assistantMsg("a2", 1800),
    ];
    expect(pruneSessionChatPendingSends([first, second], twoOccurrences)).toEqual([]);
  });

  test("occurrence survives pruning of the earlier echo (no reuse)", () => {
    const first = pendingEntry("p1", "again");
    const second = assignSessionChatPendingOccurrence(
      [first],
      pendingEntry("p2", "again"),
    );
    // p1 already pruned; p2 still must not match the FIRST transcript
    // occurrence.
    const oneOccurrence = [userMsg("u1", "again", 1500), assistantMsg("a1", 1600)];
    expect(pruneSessionChatPendingSends([second], oneOccurrence)).toEqual([second]);
  });
});

describe("rapid-send glue (§10.3)", () => {
  test("exact prefix concatenation counts; substring alone does not", () => {
    expect(countLeadingPendingTextsGluedToUserText(["hi"], "history")).toBe(0);
    expect(
      countLeadingPendingTextsGluedToUserText(["joke", "continue"], "jokecontinue"),
    ).toBe(2);
    expect(countLeadingPendingTextsGluedToUserText(["joke"], "joke")).toBe(1);
  });

  test("glued pending entries are dropped once the glued user text advanced", () => {
    const a = pendingEntry("p1", "joke");
    const b = pendingEntry("p2", "continue");
    const messages = [userMsg("u1", "jokecontinue", 1500), assistantMsg("a1", 1600)];
    expect(pruneSessionChatPendingSends([a, b], messages)).toEqual([]);
  });

  test("a single entry is never glue-pruned (left to occurrence counting)", () => {
    const a = pendingEntry("p1", "joke");
    const messages = [userMsg("u1", "jokes are fun", 1500), assistantMsg("a1", 1600)];
    expect(pruneSessionChatPendingSends([a], messages)).toEqual([a]);
  });
});

describe("content keys", () => {
  test("image-only sends key on the path list; whitespace collapses", () => {
    expect(sessionChatPendingContentKey({ text: "  a   b \n c " })).toBe("text:a b c");
    expect(
      sessionChatPendingContentKey({ imagePaths: ["/tmp/x.png"], text: "  " }),
    ).toBe('images:["/tmp/x.png"]');
    expect(sessionChatPendingContentKey({ text: "" })).toBe("empty");
  });

  test("[Image #N] markers are stripped before matching", () => {
    expect(sessionChatPendingContentKey({ text: "[Image #1] describe this" })).toBe(
      "text:describe this",
    );
  });
});

describe("/clear boundary (§10.3)", () => {
  test("model configuration commands rely on one authoritative transcript status", () => {
    const markers = [
      { command: "/model sonnet", id: "model", sentAt: 100 },
      { command: "/effort xhigh", id: "effort", sentAt: 200 },
      { command: "/compact", compactionRecordsBefore: 0, id: "compact", sentAt: 300 },
      { command: "/clear", id: "clear", sentAt: 400 },
    ];
    // Pill-dispatched commands never get a row; /compact holds one until the
    // agent's own compaction record arrives.
    expect(
      sessionChatCommandMarkersAsMessages(markers, 0).map(
        (message) => message.blocks,
      ),
    ).toEqual([
      [{ text: "Ran /compact", type: "text" }],
      [{ text: "Ran /clear", type: "text" }],
    ]);
    expect(
      sessionChatCommandMarkersAsMessages(markers, 1).map(
        (message) => message.blocks,
      ),
    ).toEqual([[{ text: "Ran /clear", type: "text" }]]);
  });

  test("only /clear (first token) counts as a clear command", () => {
    expect(isSessionChatClearCommand("/clear")).toBe(true);
    expect(isSessionChatClearCommand("  /CLEAR now ")).toBe(true);
    expect(isSessionChatClearCommand("/clearall")).toBe(false);
    expect(isSessionChatClearCommand("say /clear")).toBe(false);
  });

  test("messages at or before the clear marker are hidden, null timestamps too", () => {
    const messages = [
      userMsg("u1", "old", 100),
      assistantMsg("a1", 200),
      { ...assistantMsg("a2", 300), timestamp: null },
      assistantMsg("a3", 900),
    ];
    const markers = [{ command: "/clear", id: "m1", sentAt: 500 }];
    const filtered = applySessionChatCommandMarkerBoundaries(messages, markers);
    expect(filtered.map((message) => message.id)).toEqual(["a3"]);
  });

  test("no clear marker returns the same reference", () => {
    const messages = [userMsg("u1", "old", 100)];
    const markers = [{ command: "/model", id: "m1", sentAt: 500 }];
    expect(applySessionChatCommandMarkerBoundaries(messages, markers)).toBe(messages);
  });
});
