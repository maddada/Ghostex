import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const terminalWorkspaceSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/TerminalWorkspaceView.swift", import.meta.url),
  "utf8",
);

const appDelegateSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/AppDelegate.swift", import.meta.url),
  "utf8",
);

describe("native terminal IME input", () => {
  test("routes normal text keyDown through AppKit text input before Ghostty", () => {
    const surfaceClassIndex = terminalWorkspaceSource.indexOf("final class GhostexGhosttySurfaceView: NSView");
    const keyDownIndex = terminalWorkspaceSource.indexOf("override func keyDown(with event: NSEvent)", surfaceClassIndex);
    const programmaticEnterIndex = terminalWorkspaceSource.indexOf("func sendProgrammaticTerminalEnter", keyDownIndex);
    const keyDownSource = terminalWorkspaceSource.slice(keyDownIndex, programmaticEnterIndex);

    expect(surfaceClassIndex).toBeGreaterThan(-1);
    expect(keyDownIndex).toBeGreaterThan(surfaceClassIndex);
    expect(terminalWorkspaceSource).toContain("CDXC:NativeIME 2026-06-13-01:28");
    expect(terminalWorkspaceSource).toContain("CDXC:NativeIME 2026-06-13-02:22");
    expect(terminalWorkspaceSource).toContain("CDXC:NativeIME 2026-06-13-02:32");
    expect(terminalWorkspaceSource).toContain("CDXC:NativeIME 2026-06-13-03:35");
    expect(keyDownSource).toContain("shouldBypassTextInput(for: event, action: action, surface: surface)");
    expect(keyDownSource).toContain("sendKeyEvent(event, action: action, includeText: false, composing: false)");
    expect(keyDownSource.indexOf("shouldBypassTextInput(for: event")).toBeLessThan(
      keyDownSource.indexOf("let markedTextBefore = hasMarkedText()"),
    );
    expect(keyDownSource).toContain("let markedTextBefore = hasMarkedText()");
    expect(keyDownSource).toContain("let translationEvent = translatedTextInputEvent(for: event, surface: surface)");
    expect(keyDownSource).toContain("keyTextAccumulator = []");
    expect(keyDownSource).toContain("defer { keyTextAccumulator = nil }");
    expect(keyDownSource).toContain("interpretKeyEvents([translationEvent])");
    expect(keyDownSource).toContain("syncPreedit(clearIfNeeded: markedTextBefore)");
    expect(keyDownSource).toContain("let composing = hasMarkedText() || markedTextBefore");
    expect(keyDownSource).toMatch(/if markedTextBefore \{\s+sendCommittedText\(text, action: action\)/);
    expect(keyDownSource).toMatch(/consumedMods: consumedTextInputMods\(from: translationEvent\),\s+text: text\)/);
    expect(keyDownSource).toContain("composing: composing");
  });

  test("keeps interpreted control keys on the raw Ghostty event path", () => {
    const sendKeyIndex = terminalWorkspaceSource.indexOf("private func sendKeyEvent(");
    const committedTextIndex = terminalWorkspaceSource.indexOf("private func sendCommittedText", sendKeyIndex);
    const sendKeySource = terminalWorkspaceSource.slice(sendKeyIndex, committedTextIndex);

    expect(sendKeySource).toContain("Self.shouldAttachGhosttyText(text)");
    expect(sendKeySource).toContain("private static func shouldAttachGhosttyText(_ text: String) -> Bool");
    expect(sendKeySource).toContain("guard let firstByte = text.utf8.first else");
    expect(sendKeySource).toContain("return firstByte >= 0x20");
  });

  test("preserves Ghostty keybindings before AppKit IME interpretation", () => {
    const bypassIndex = terminalWorkspaceSource.indexOf("private func shouldBypassTextInput");
    const escapeHelperIndex = terminalWorkspaceSource.indexOf("private func sendTerminalEscapeSideBandIfNeeded", bypassIndex);
    const bypassSource = terminalWorkspaceSource.slice(bypassIndex, escapeHelperIndex);

    expect(bypassSource).toContain("guard !hasMarkedText() else");
    expect(bypassSource).toContain("if isGhosttyKeyBinding(event, action: action, surface: surface)");
    expect(bypassSource).toContain("var keyEvent = event.ghosttyKeyEvent(action)");
    expect(bypassSource).toContain("if let text = event.characters, !text.isEmpty");
    expect(bypassSource).toContain("ghostty_surface_key_is_binding(surface, keyEvent, &bindingFlags)");
    expect(bypassSource.indexOf("isGhosttyKeyBinding")).toBeLessThan(
      bypassSource.indexOf("flags.contains(.command)"),
    );
  });

  test("loads current Ghostty config filename before legacy config", () => {
    const preferredIndex = appDelegateSource.indexOf("private static func preferredGhosttyConfig()");
    const startupLogIndex = appDelegateSource.indexOf("private func logGhosttyConfigStartup()", preferredIndex);
    const preferredSource = appDelegateSource.slice(preferredIndex, startupLogIndex);
    const writableIndex = appDelegateSource.indexOf("private static func defaultWritableGhosttyConfigURL()");
    const mergeIndex = appDelegateSource.indexOf("private static func mergeGhosttyTerminalSettings", writableIndex);
    const writableSource = appDelegateSource.slice(writableIndex, mergeIndex);

    expect(appDelegateSource).toContain("CDXC:NativeIME 2026-06-13-02:32");
    expect(preferredSource.indexOf("com.mitchellh.ghostty/config.ghostty")).toBeLessThan(
      preferredSource.indexOf("com.mitchellh.ghostty/config\""),
    );
    expect(preferredSource.indexOf("com.ghostty.org/config.ghostty")).toBeLessThan(
      preferredSource.indexOf("com.ghostty.org/config\""),
    );
    expect(writableSource).toContain('appendingPathComponent("com.mitchellh.ghostty/config.ghostty")');
  });

  test("does not forward terminal Escape or modifier changes while IME is composing", () => {
    const escapeHelperIndex = terminalWorkspaceSource.indexOf("private func sendTerminalEscapeSideBandIfNeeded");
    const translationIndex = terminalWorkspaceSource.indexOf("private func translatedTextInputEvent", escapeHelperIndex);
    const escapeHelperSource = terminalWorkspaceSource.slice(escapeHelperIndex, translationIndex);
    const flagsChangedIndex = terminalWorkspaceSource.indexOf("override func flagsChanged(with event: NSEvent)");
    const firstPromptIndex = terminalWorkspaceSource.indexOf("func setFirstPromptTitleGenerationInputSuppressed", flagsChangedIndex);
    const flagsChangedSource = terminalWorkspaceSource.slice(flagsChangedIndex, firstPromptIndex);

    expect(escapeHelperSource).toContain("guard event.keyCode == 53, !composing, let ghostexSessionId else");
    expect(escapeHelperSource).toContain("Escape belongs to AppKit's marked-text state");
    expect(flagsChangedSource).toContain("if hasMarkedText() {\n      return\n    }");
    expect(flagsChangedSource.indexOf("if hasMarkedText()")).toBeLessThan(flagsChangedSource.indexOf("sendKeyEvent("));
  });

  test("accumulates IME commits and keeps marked ranges UTF-16 safe", () => {
    const textInputIndex = terminalWorkspaceSource.indexOf("extension GhostexGhosttySurfaceView: NSTextInputClient");
    const nextClassIndex = terminalWorkspaceSource.indexOf("private final class BrowserAddressTextFieldCell", textInputIndex);
    const textInputSource = terminalWorkspaceSource.slice(textInputIndex, nextClassIndex);

    expect(textInputSource).toContain("if var accumulator = keyTextAccumulator");
    expect(textInputSource).toContain("accumulator.append(text)");
    expect(textInputSource).toContain("markedText.utf16.count");
    expect(textInputSource).toContain("selectedTextRange = clampedMarkedTextRange(selectedRange)");
    expect(textInputSource).toContain("if keyTextAccumulator == nil {\n      syncPreedit()\n    }");
    expect(textInputSource).toContain("(markedText as NSString).substring(with: safeRange)");
    expect(textInputSource).toContain("ghostty_surface_ime_point(surface, &x, &y, &width, &height)");
  });
});
