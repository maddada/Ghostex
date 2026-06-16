import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const appDelegateSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/AppDelegate.swift", import.meta.url),
  "utf8",
);
const cefBridgeHeaderSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/GhostexCEFBridge.h", import.meta.url),
  "utf8",
);
const cefBridgeSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/GhostexCEFBridge.mm", import.meta.url),
  "utf8",
);
const hostProtocolSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/HostProtocol.swift", import.meta.url),
  "utf8",
);
const nativeBrowserProfilesSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/NativeBrowserProfiles.swift", import.meta.url),
  "utf8",
);
const terminalWorkspaceSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/TerminalWorkspaceView.swift", import.meta.url),
  "utf8",
);
const sharedHostProtocolSource = readFileSync(
  new URL("../../shared/native-ghostty-host-protocol.ts", import.meta.url),
  "utf8",
);
const nativeSidebarSource = readFileSync(new URL("./native-sidebar.tsx", import.meta.url), "utf8");

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("chromium browser source", () => {
  test("routes Cmd+F to CEF browser search before generic hotkeys", () => {
    /*
     * CDXC:BrowserSearch 2026-06-13-00:00:
     * Cmd+F in focused embedded CEF panes should open browser page search, not
     * terminal search or the app-wide hotkey path.
     */
    const hotkeyEquivalentSource = sourceBetween(
      appDelegateSource,
      "func handleHotkeyEquivalent(_ event: NSEvent) -> Bool",
      "private func shouldUseNativeAppModalWindow",
    );
    const findShortcutIndex = hotkeyEquivalentSource.indexOf("handleFocusedChromiumFindShortcut(event)");
    const genericHotkeyIndex = hotkeyEquivalentSource.indexOf("let hotkeyText = Self.hotkeyText(for: event)");
    expect(findShortcutIndex).toBeGreaterThanOrEqual(0);
    expect(findShortcutIndex).toBeLessThan(genericHotkeyIndex);

    expect(cefBridgeHeaderSource).toContain("findResultHandler");
    expect(cefBridgeHeaderSource).toContain("findText(_:forward:findNext:)");
    expect(cefBridgeSource).toContain("public CefFindHandler");
    expect(cefBridgeSource).toContain("GetFindHandler() override");
    expect(cefBridgeSource).toContain("OnFindResult(CefRefPtr<CefBrowser> browser");
    expect(cefBridgeSource).toContain("browser_->GetHost()->Find(");
    expect(cefBridgeSource).toContain("browser_->GetHost()->StopFinding");
    expect(terminalWorkspaceSource).toContain("private final class BrowserFindBarView");
    expect(terminalWorkspaceSource).toContain("func handleFocusedChromiumFindShortcut(_ event: NSEvent) -> Bool");
    expect(terminalWorkspaceSource).toContain('case "f":');
    expect(terminalWorkspaceSource).toContain("return flags.contains(.shift) ? nil : .open");
    expect(terminalWorkspaceSource).toContain("chromiumView.findText(query, forward: forward, findNext: findNext)");
    expect(terminalWorkspaceSource).toContain("openBrowserFind(reason: \"keyboardShortcut\")");
  });

  test("keeps CEF browser find bar styled and typable like terminal search", () => {
    /*
     * CDXC:BrowserSearch 2026-06-13-00:44:
     * CEF browser find should use terminal-search-style native chrome and must
     * explicitly activate an editable field editor so users can type immediately.
     */
    const browserFindBarSource = sourceBetween(
      terminalWorkspaceSource,
      "private final class BrowserFindBarView",
      "private final class TerminalSearchTextFieldCell",
    );
    const browserFindLayoutSource = sourceBetween(
      terminalWorkspaceSource,
      "private func layoutBrowserFindBar(webFrame: CGRect)",
      "  @discardableResult\n  func openBrowserFind",
    );
    expect(browserFindBarSource).toContain("private static let barHeight: CGFloat = 34");
    expect(browserFindBarSource).toContain("private static let preferredWidth: CGFloat = 300");
    expect(browserFindBarSource).toContain("layer?.cornerRadius = 8");
    expect(browserFindBarSource).toContain("TerminalSearchTextFieldCell(textCell: \"\")");
    expect(browserFindBarSource).toContain("private let previousButton = TerminalSearchButton()");
    expect(browserFindBarSource).toContain("button.bezelStyle = .regularSquare");
    expect(browserFindBarSource).toContain("textField.isEditable = true");
    expect(browserFindBarSource).toContain("textField.isSelectable = true");
    expect(browserFindBarSource).toContain("window?.fieldEditor(true, for: textField)");
    expect(browserFindBarSource).toContain("window?.makeFirstResponder(editor)");
    expect(browserFindBarSource).not.toContain("override func hitTest(_ point: NSPoint) -> NSView?");
    expect(browserFindBarSource).toContain("focusSearchField(reason: \"barMouseDown\", selectAll: false)");
    expect(browserFindLayoutSource).toContain("let horizontalMargin: CGFloat = 26");
    expect(browserFindLayoutSource).toContain("let verticalMargin: CGFloat = 8");
    expect(browserFindLayoutSource).toContain("x: webFrame.maxX - width - horizontalMargin");
    expect(browserFindLayoutSource).toContain("y: webFrame.maxY - height - verticalMargin");
  });

  test("keeps CEF browser input on normal AppKit child dispatch", () => {
    /*
     * CDXC:ChromiumBrowserPanes 2026-06-13-13:40:
     * CEF browser panes should use exact parent/child native frames. The wrapper
     * must not manually replay mouse events into Chromium because that recreates
     * coordinate-routing bugs.
     */
    expect(cefBridgeSource).toContain("The CEF wrapper is a normal container");
    expect(cefBridgeSource).not.toContain("ghostexCEFDispatchMouseEventToHostedView");
    expect(cefBridgeSource).not.toContain("ghostexCEFEventIsInsideHostedView");
    expect(cefBridgeSource).not.toContain("[cefView_ mouseDown:event]");
    expect(cefBridgeSource).not.toContain("[cefView_ mouseDragged:event]");
    expect(cefBridgeSource).not.toContain("[cefView_ rightMouseDown:event]");
    expect(cefBridgeSource).not.toContain("[cefView_ scrollWheel:event]");
  });

  test("creates async CEF browser panes with a real frame and the first URL", () => {
    /*
     * CDXC:ChromiumBrowserPanes 2026-06-15-23:10:
     * Cmd+N browser panes can appear as a black surface when async GPUI CEF creation starts before AppKit assigns a real child-view frame, or when the requested URL is replayed after Chromium is already bootstrapping the first navigation.
     * Keep the first browser load attached to CEF creation itself and guard creation until the wrapper has non-zero bounds so the rendered page and address bar are initialized from the same navigation.
     */
    const didCreateBrowserSource = sourceBetween(
      cefBridgeSource,
      "- (void)ghostexCEFDidCreateBrowser:(CefRefPtr<CefBrowser>)browser",
      "- (void)ghostexCEFPinHostedViewToBoundsWithReason:(NSString*)reason",
    );
    const createBrowserSource = sourceBetween(
      cefBridgeSource,
      "- (void)createBrowserIfNeeded",
      "- (void)loadURLString:(NSString*)urlString",
    );

    expect(createBrowserSource).toContain(
      "if (self.bounds.size.width <= 0 || self.bounds.size.height <= 0)",
    );
    expect(createBrowserSource).toContain("Wait for a real AppKit size");
    expect(createBrowserSource).toContain(
      'bool runsUnderGPUI = NSApp && [NSStringFromClass([NSApp class]) isEqualToString:@"GPUIApplication"];',
    );
    expect(createBrowserSource).toContain(
      'NSString* creationURL = initialURL_.length > 0 ? initialURL_ : @"about:blank";',
    );
    expect(createBrowserSource).toContain("didGiveInitialURLToBrowserCreate_ = initialURL_.length > 0;");
    expect(createBrowserSource).toContain("CefBrowserHost::CreateBrowser(");
    expect(createBrowserSource).toContain("CefString([creationURL UTF8String])");
    expect(didCreateBrowserSource).toContain(
      "if (initialURL_.length > 0 && !didGiveInitialURLToBrowserCreate_)",
    );
    expect(didCreateBrowserSource).toContain("[self loadURLString:initialURL_];");
  });

  test("converts CEF new-window intents into current-surface Ghostex tabs", () => {
    /*
     * CDXC:BrowserTabs 2026-06-13-00:00:
     * Middle-click, target-blank, and context-menu open-in-new-tab/window in
     * CEF must create a Git tab in Git mode or a sibling Agents browser tab in
     * the normal workspace browser view.
     */
    expect(cefBridgeSource).toContain('#include "include/cef_command_ids.h"');
    expect(cefBridgeSource).toContain("OpenRequestedURLInGhostexTab");
    expect(cefBridgeSource).toContain("IDC_CONTENT_CONTEXT_OPENLINKNEWTAB");
    expect(cefBridgeSource).toContain("IDC_CONTENT_CONTEXT_OPENLINKNEWWINDOW");
    expect(terminalWorkspaceSource).toContain(
      ".browserOpenInNewTabRequested(sourceSessionId: command.sessionId, url: url)",
    );
    expect(terminalWorkspaceSource).toContain("self.addProjectEditorGitTab(");
    expect(hostProtocolSource).toContain(
      "case browserOpenInNewTabRequested(sourceSessionId: String, url: String)",
    );
    expect(hostProtocolSource).toContain('try container.encode("browserOpenInNewTabRequested", forKey: .type)');
    expect(sharedHostProtocolSource).toContain('type: "browserOpenInNewTabRequested";');
    expect(nativeSidebarSource).toContain(
      '| { sourceSessionId: string; type: "browserOpenInNewTabRequested"; url: string }',
    );

    const sidebarHandlerSource = sourceBetween(
      nativeSidebarSource,
      "function handleBrowserOpenInNewTabRequested",
      "function findBrowserSessionInProjectByUrl",
    );
    expect(sidebarHandlerSource).toContain("sidebarSessionIdForNativeSession(hostEvent.sourceSessionId)");
    expect(sidebarHandlerSource).toContain("sourceSession?.kind !== \"browser\"");
    expect(sidebarHandlerSource).toContain("createNativeBrowserSession(hostEvent.url, findSessionGroupId(sidebarSessionId), {");
    expect(sidebarHandlerSource).toContain('kind: "appendToTabGroup"');
    expect(sidebarHandlerSource).toContain('position: "after"');
    expect(sidebarHandlerSource).toContain("targetSessionId: sidebarSessionId");

    const hostEventHandlerSource = sourceBetween(
      nativeSidebarSource,
      'if (hostEvent.type === "projectEditorTabSelected")',
      'if (hostEvent.type === "osIntegrationStatus")',
    );
    expect(hostEventHandlerSource).toContain('if (hostEvent.type === "browserOpenInNewTabRequested")');
    expect(hostEventHandlerSource).toContain("handleBrowserOpenInNewTabRequested(hostEvent);");
  });

  test("labels CEF browser profile beta actions without adding an action", () => {
    /*
     * CDXC:BrowserProfiles 2026-06-13-22:09:
     * The CEF browser address-bar profile dropdown should show a disabled Beta
     * Features section label immediately above the beta profile commands.
     */
    const profilePickerSource = sourceBetween(
      nativeBrowserProfilesSource,
      'let menu = NSMenu(title: "Profiles")',
      "let location = NSEvent.mouseLocation",
    );
    const betaIndex = profilePickerSource.indexOf('NSMenuItem(title: "Beta Features:", action: nil, keyEquivalent: "")');
    const newProfileIndex = profilePickerSource.indexOf('title: "New Profile..."');
    const importIndex = profilePickerSource.indexOf('title: "Import Browser Data..."');
    expect(betaIndex).toBeGreaterThanOrEqual(0);
    expect(newProfileIndex).toBeGreaterThan(betaIndex);
    expect(importIndex).toBeGreaterThan(newProfileIndex);
    expect(profilePickerSource).toContain("betaItem.isEnabled = false");
  });
});
