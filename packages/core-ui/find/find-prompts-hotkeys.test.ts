import { describe, expect, it } from "vitest";
import {
  FIND_PROMPTS_HINTS,
  resolveFindPromptsAction,
  type FindPromptsKeyEvent,
  type FindPromptsMode,
} from "./find-prompts-hotkeys";

function key(
  k: string,
  modifiers: Partial<Omit<FindPromptsKeyEvent, "key">> = {},
): FindPromptsKeyEvent {
  return {
    altKey: false,
    ctrlKey: false,
    key: k,
    metaKey: false,
    shiftKey: false,
    ...modifiers,
  };
}

function resolve(k: FindPromptsKeyEvent, mode: FindPromptsMode = "list") {
  return resolveFindPromptsAction(k, mode);
}

describe("find prompts hotkeys", () => {
  it("keeps every gx f control key on the same letter", () => {
    expect(resolve(key("d", { ctrlKey: true }))).toEqual({ type: "toggleDayGrouping" });
    expect(resolve(key("f", { ctrlKey: true }))).toEqual({ type: "toggleFavorite" });
    expect(resolve(key("e", { ctrlKey: true }))).toEqual({ type: "viewPrompt" });
    expect(resolve(key("y", { ctrlKey: true }))).toEqual({ type: "copyPrompt" });
    expect(resolve(key("o", { ctrlKey: true }))).toEqual({ type: "forkPicker" });
    expect(resolve(key("k", { ctrlKey: true }))).toEqual({ type: "killToEnd" });
    expect(resolve(key("u", { ctrlKey: true }))).toEqual({ type: "killToStart" });
    expect(resolve(key("n", { ctrlKey: true }))).toEqual({ delta: 1, type: "move" });
    expect(resolve(key("p", { ctrlKey: true }))).toEqual({ delta: -1, type: "move" });
  });

  it("moves the two browser-reserved pickers onto ^g and ^j", () => {
    expect(resolve(key("g", { ctrlKey: true }))).toEqual({ type: "openAgentPicker" });
    expect(resolve(key("j", { ctrlKey: true }))).toEqual({ type: "openProjectPicker" });
    // The letters a browser will not surrender must do nothing here.
    expect(resolve(key("t", { ctrlKey: true }))).toBeNull();
    expect(resolve(key("r", { ctrlKey: true }))).toBeNull();
    expect(resolve(key("w", { ctrlKey: true }))).toBeNull();
  });

  it("advertises the remapped keys in the hint strip", () => {
    const hints = FIND_PROMPTS_HINTS.map((hint) => `${hint.key} ${hint.label}`);
    expect(hints).toContain("^g agents");
    expect(hints).toContain("^j projects");
    expect(hints).not.toContain("^t agents");
    expect(hints).not.toContain("^r projects");
  });

  it("uses Ctrl rather than Cmd, so macOS behaves like the terminal", () => {
    expect(resolve(key("f", { metaKey: true }))).toBeNull();
    expect(resolve(key("g", { metaKey: true }))).toBeNull();
    expect(resolve(key("d", { metaKey: true }))).toBeNull();
  });

  it("navigates, resumes, and closes", () => {
    expect(resolve(key("ArrowDown"))).toEqual({ delta: 1, type: "move" });
    expect(resolve(key("ArrowUp"))).toEqual({ delta: -1, type: "move" });
    expect(resolve(key("Enter"))).toEqual({ type: "resumePrompt" });
    expect(resolve(key("Escape"))).toEqual({ type: "close" });
    expect(resolve(key("c", { ctrlKey: true }))).toEqual({ type: "close" });
    expect(resolve(key("Tab"))).toEqual({ type: "togglePreviewFocus" });
  });

  it("jumps day groups with PageUp/PageDown and Ctrl-arrows in the list", () => {
    expect(resolve(key("PageDown"))).toEqual({ delta: 1, type: "jumpDay" });
    expect(resolve(key("PageUp"))).toEqual({ delta: -1, type: "jumpDay" });
    expect(resolve(key("ArrowDown", { ctrlKey: true }))).toEqual({ delta: 1, type: "jumpDay" });
    expect(resolve(key("ArrowUp", { ctrlKey: true }))).toEqual({ delta: -1, type: "jumpDay" });
  });

  it("re-targets preview keys only while the preview holds focus", () => {
    expect(resolve(key("PageDown"), "preview")).toEqual({ delta: 1, type: "scrollPreview" });
    expect(resolve(key("W"), "preview")).toEqual({ type: "toggleWrap" });
    expect(resolve(key("F"), "preview")).toEqual({ type: "toggleFullscreenPreview" });
    expect(resolve(key("f", { ctrlKey: true }), "preview")).toEqual({
      type: "toggleFullscreenPreview",
    });
    // In the list they are ordinary characters and must reach the query input.
    expect(resolve(key("W"))).toBeNull();
    expect(resolve(key("F"))).toBeNull();
  });

  it("deletes words with the terminal picker's editing keys", () => {
    expect(resolve(key("Backspace", { ctrlKey: true }))).toEqual({ type: "deleteWordBackward" });
    expect(resolve(key("Delete", { ctrlKey: true }))).toEqual({ type: "deleteWordForward" });
  });

  it("turns fork mode into a one-keystroke agent choice", () => {
    expect(resolve(key("1"), "forkPicker")).toEqual({ index: 0, type: "pickIndex" });
    expect(resolve(key("6"), "forkPicker")).toEqual({ index: 5, type: "pickIndex" });
    expect(resolve(key("7"), "forkPicker")).toEqual({ type: "cancelOverlay" });
    expect(resolve(key("Escape"), "forkPicker")).toEqual({ type: "cancelOverlay" });
  });

  it("drives the agent and project overlays with arrows, Enter, and Space", () => {
    for (const mode of ["agentPicker", "projectPicker"] as const) {
      expect(resolve(key("ArrowDown"), mode)).toEqual({ delta: 1, type: "move" });
      expect(resolve(key("p", { ctrlKey: true }), mode)).toEqual({ delta: -1, type: "move" });
      expect(resolve(key("Enter"), mode)).toEqual({ type: "togglePickerSelection" });
      expect(resolve(key(" "), mode)).toEqual({ type: "togglePickerSelection" });
      expect(resolve(key("Escape"), mode)).toEqual({ type: "cancelOverlay" });
    }
    // Digits are an agent shortcut only; the project overlay types to search.
    expect(resolve(key("3"), "agentPicker")).toEqual({ index: 2, type: "pickIndex" });
    expect(resolve(key("3"), "projectPicker")).toBeNull();
  });
});
