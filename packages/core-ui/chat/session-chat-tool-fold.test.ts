import { describe, expect, test } from "vitest";
import type {
  SessionChatBlock,
  SessionChatMessage,
} from "../../shared/session-chat";
import {
  foldSessionChatToolMessages,
  isToolOnlySessionChatMessage,
  pairSessionChatToolBlocks,
  splitSessionChatBlocks,
} from "./session-chat-tool-fold";

function call(name: string): SessionChatBlock {
  return { input: { arg: name }, name, type: "tool-call" };
}

function result(output: string): SessionChatBlock {
  return { output, type: "tool-result" };
}

function message(
  id: string,
  role: SessionChatMessage["role"],
  blocks: SessionChatBlock[],
): SessionChatMessage {
  return { blocks, id, role, source: "transcript", timestamp: 1 };
}

describe("FIFO tool pairing (§6.6b)", () => {
  test("Nth result pairs with Nth call in document order", () => {
    const pairs = pairSessionChatToolBlocks([
      call("Read"),
      call("Bash"),
      result("read output"),
      result("bash output"),
    ]);
    expect(pairs).toHaveLength(2);
    expect(pairs[0]?.call?.name).toBe("Read");
    expect(pairs[0]?.result?.output).toBe("read output");
    expect(pairs[1]?.call?.name).toBe("Bash");
    expect(pairs[1]?.result?.output).toBe("bash output");
  });

  test("orphan results become their own pair", () => {
    const pairs = pairSessionChatToolBlocks([result("stray")]);
    expect(pairs).toHaveLength(1);
    expect(pairs[0]?.call).toBeUndefined();
    expect(pairs[0]?.result?.output).toBe("stray");
  });

  test("limit caps pairs but keeps result ordinals aligned", () => {
    const pairs = pairSessionChatToolBlocks(
      [call("A"), call("B"), result("ra"), result("rb")],
      1,
    );
    expect(pairs).toHaveLength(1);
    expect(pairs[0]?.call?.name).toBe("A");
    // rb belongs to the over-limit B call: it must NOT attach to A.
    expect(pairs[0]?.result?.output).toBe("ra");
  });

  test("interleaved call/result/call/result pairs correctly", () => {
    const pairs = pairSessionChatToolBlocks([
      call("A"),
      result("ra"),
      call("B"),
      result("rb"),
    ]);
    expect(pairs[0]?.result?.output).toBe("ra");
    expect(pairs[1]?.result?.output).toBe("rb");
  });
});

describe("tool fold (§6.6b)", () => {
  test("consecutive tool-only messages fold into the preceding assistant turn", () => {
    const assistant = message("a1", "assistant", [
      { text: "let me look", type: "text" },
      call("Read"),
    ]);
    const toolRow = message("t1", "tool", [result("file contents")]);
    const trailing = message("a2", "assistant", [{ text: "done", type: "text" }]);
    const folded = foldSessionChatToolMessages([assistant, toolRow, trailing]);
    expect(folded).toHaveLength(2);
    expect(folded[0]?.blocks).toHaveLength(3);
    expect(folded[0]?.blocks.at(-1)?.type).toBe("tool-result");
    // The original assistant message must not be mutated.
    expect(assistant.blocks).toHaveLength(2);
  });

  test("tool-only messages without a preceding assistant stay standalone", () => {
    const toolRow = message("t1", "tool", [result("orphan")]);
    const user = message("u1", "user", [{ text: "hi", type: "text" }]);
    expect(foldSessionChatToolMessages([toolRow, user])).toHaveLength(2);
  });

  test("tool-only messages fold into the preceding reasoning summary", () => {
    const reasoning = message("r1", "reasoning", [
      { text: "Inspecting the renderer", type: "text" },
    ]);
    const callRow = message("a1", "assistant", [call("exec")]);
    const resultRow = message("t1", "tool", [result("done")]);
    const folded = foldSessionChatToolMessages([reasoning, callRow, resultRow]);
    expect(folded).toHaveLength(1);
    expect(folded[0]?.blocks.map((block) => block.type)).toEqual([
      "text",
      "tool-call",
      "tool-result",
    ]);
  });

  test("a user turn breaks the fold chain", () => {
    const assistant = message("a1", "assistant", [{ text: "x", type: "text" }]);
    const user = message("u1", "user", [{ text: "hi", type: "text" }]);
    const toolRow = message("t1", "tool", [result("out")]);
    const folded = foldSessionChatToolMessages([assistant, user, toolRow]);
    expect(folded).toHaveLength(3);
  });

  test("isToolOnly requires nonempty all-tool blocks", () => {
    expect(isToolOnlySessionChatMessage(message("m", "tool", [result("x")]))).toBe(true);
    expect(isToolOnlySessionChatMessage(message("m", "tool", []))).toBe(false);
    expect(
      isToolOnlySessionChatMessage(
        message("m", "assistant", [{ text: "t", type: "text" }, call("A")]),
      ),
    ).toBe(false);
  });

  test("splitSessionChatBlocks separates prose from tools preserving order", () => {
    const { prose, tools } = splitSessionChatBlocks([
      { text: "a", type: "text" },
      call("A"),
      { alt: "img", type: "image-ref", url: "http://x" },
      result("ra"),
    ]);
    expect(prose).toHaveLength(2);
    expect(tools).toHaveLength(2);
  });
});
