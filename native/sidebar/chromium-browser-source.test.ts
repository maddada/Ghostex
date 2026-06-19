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
const nativeTooltipSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/NativeTooltip.swift", import.meta.url),
  "utf8",
);
const gpuiMainSource = readFileSync(new URL("../../gpui/src/main.rs", import.meta.url), "utf8");
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

  test("creates CEF browser panes with a real frame and the first URL", () => {
    /*
     * CDXC:ChromiumBrowserPanes 2026-06-15-23:10:
     * Cmd+N browser panes can appear as a black surface when async GPUI CEF creation starts before AppKit assigns a real child-view frame, or when the requested URL is replayed after Chromium is already bootstrapping the first navigation.
     * Keep the first browser load attached to CEF creation itself and guard creation until the wrapper has non-zero bounds so the rendered page and address bar are initialized from the same navigation.
     *
     * CDXC:ChromiumBrowserPanes 2026-06-18-23:58:
     * The production CreateBrowserSync path needs the same first-URL behavior as GPUI async creation. Starting on about:blank and then calling LoadURL adds a visible blank render turn when Cmd+N opens a new browser pane.
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
    expect(createBrowserSource).toContain("CefBrowserHost::CreateBrowserSync(");
    expect(createBrowserSource).not.toContain('CefString("about:blank")');
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

  test("persists project Browser tabs outside the active project editor mode", () => {
    /*
     * CDXC:ProjectBrowserTabs 2026-06-15-10:15:
     * Browser tabs must restore after app launch even when the user last
     * switched away to Agents, Source, or Kanban. Keep the Browser tab group on
     * project-local Browser memory instead of coupling it to projectEditor.isOpen.
     */
    const projectTypeSource = sourceBetween(
      nativeSidebarSource,
      "type NativeProject =",
      "function isQuickProject",
    );
    expect(projectTypeSource).toContain("projectBrowser?: NativeProjectBrowserRestoreState;");

    const projectNormalizerSource = sourceBetween(
      nativeSidebarSource,
      "function normalizeStoredNativeProject",
      "function normalizeStoredProjectEditorRestoreState",
    );
    expect(projectNormalizerSource).toContain(
      "normalizeStoredProjectBrowserRestoreState(project.projectBrowser)",
    );
    expect(projectNormalizerSource).toContain("projectBrowser: normalizedProjectBrowser");

    const browserMemoryWriterSource = sourceBetween(
      nativeSidebarSource,
      "function setProjectBrowserPersistedState",
      "function setProjectEditorCompanionPaneHidden",
    );
    expect(browserMemoryWriterSource).toContain("project.projectBrowser");
    expect(browserMemoryWriterSource).toContain("return { ...project, projectBrowser: nextProjectBrowser };");

    const browserOpenSource = sourceBetween(
      nativeSidebarSource,
      "function openProjectGitEditorSurface",
      "function openProjectTasksEditorSurface",
    );
    expect(browserOpenSource).toContain(
      "surfaceState?.mode === \"git\" ? surfaceState : project.projectBrowser",
    );
    expect(browserOpenSource).toContain("setProjectBrowserPersistedState(");

    const projectEditorTabSelectedSource = sourceBetween(
      nativeSidebarSource,
      "function handleProjectEditorTabSelected",
      "function disposeProjectEditorSurface",
    );
    expect(projectEditorTabSelectedSource).toContain("setProjectBrowserPersistedState(");
  });

  test("opens Browser new tabs to the project GitHub remote or Google", () => {
    /*
     * CDXC:ProjectBrowserTabs 2026-06-16-12:02:
     * Opening a native Browser + tab should use the project's GitHub remote URL, or Google when no GitHub remote is available. Ghostex-created Browser tabs should not intentionally start on about:blank because that can leave CEF looking stuck on a loading blank page.
     */
    expect(nativeSidebarSource).toContain('const DEFAULT_PROJECT_BROWSER_URL = "https://www.google.com/";');
    expect(nativeSidebarSource).toContain("newBrowserTabUrl?: string;");
    expect(sharedHostProtocolSource).toContain("newBrowserTabUrl?: string;");
    expect(hostProtocolSource).toContain("let newBrowserTabUrl: String?");
    expect(terminalWorkspaceSource).toContain('private static let projectBrowserDefaultNewTabURL = "https://www.google.com/"');
    expect(terminalWorkspaceSource).toContain("let newBrowserTabUrl = projectEditorBrowserNewTabURL(command.newBrowserTabUrl, fallback: command.url)");
    expect(terminalWorkspaceSource).toContain("url: newBrowserTabUrl");
    expect(terminalWorkspaceSource).toContain("initialURL: tabUrl");
    expect(terminalWorkspaceSource).not.toContain('initialURL: "about:blank"');

    const browserOpenSource = sourceBetween(
      nativeSidebarSource,
      "function openProjectGitEditorSurface",
      "function openProjectTasksEditorSurface",
    );
    expect(browserOpenSource).toContain("newBrowserTabUrl: string = seedUrl");
    expect(browserOpenSource).toContain("const browserNewTabUrl = normalizeProjectBrowserUrl(newBrowserTabUrl) ?? DEFAULT_PROJECT_BROWSER_URL;");
    expect(browserOpenSource).toContain("newBrowserTabUrl: browserNewTabUrl");

    const browserHandlerSource = sourceBetween(
      nativeSidebarSource,
      "async function openGitHubProjectFromTitlebar",
      "async function resolveProjectBrowserSeedUrl",
    );
    expect(browserHandlerSource).toContain("openProjectGitEditorSurface(project, rememberedUrl ?? browserSeedUrl, browserSeedUrl);");
    expect(browserHandlerSource).toContain("openProjectGitEditorSurface(project, browserSeedUrl, browserSeedUrl);");

    const browserSeedSource = sourceBetween(
      nativeSidebarSource,
      "async function resolveProjectBrowserSeedUrl",
      "function openTasksPlaceholderFromTitlebar",
    );
    expect(browserSeedSource).toContain("return DEFAULT_PROJECT_BROWSER_URL;");
    expect(browserSeedSource).toContain("return githubUrl;");
  });

  test("resets the last Browser top-mode tab to a non-CEF placeholder", () => {
    /*
     * CDXC:ProjectBrowserTabs 2026-06-15-20:48:
     * Closing the last Browser top-mode tab should free the Chromium view while keeping one New Tab placeholder with the address bar. The placeholder persists through React/native tab state and becomes a CEF-backed browser only when the user commits an address.
     *
     * CDXC:ProjectBrowserTabs 2026-06-16-01:46:
     * The final New Tab placeholder is selectable but not closable, because it is the memory-saving empty browser state rather than a real browser tab.
     *
     * CDXC:ProjectBrowserTabs 2026-06-16-12:59:
     * The New Tab placeholder becomes closable when another Browser tab exists. Only the final placeholder remains protected so the tab strip cannot be emptied.
     */
    expect(sharedHostProtocolSource).toContain("isPlaceholder?: boolean;");
    expect(nativeSidebarSource).toContain("isPlaceholder?: boolean;");
    expect(hostProtocolSource).toContain("let isPlaceholder: Bool?");
    expect(terminalWorkspaceSource).toContain("var isPlaceholder: Bool");
    expect(terminalWorkspaceSource).toContain("allowsClose: !tab.isPlaceholder || session.tabs.count > 1");
    expect(terminalWorkspaceSource).toContain("button.setAllowsClose(allowsTabClosing && tab.allowsClose)");
    expect(terminalWorkspaceSource).toContain("makeProjectEditorBrowserPlaceholderView()");
    expect(terminalWorkspaceSource).toContain("realizeProjectEditorBrowserPlaceholderTab(");
    expect(terminalWorkspaceSource).toContain("projectEditorGitLastTabClosed");
    expect(terminalWorkspaceSource).toContain("onAddressNavigation?(url) == true");

    const closeTabSource = sourceBetween(
      terminalWorkspaceSource,
      "private func closeProjectEditorGitTab(",
      "private func createProjectEditorGitTabId()",
    );
    expect(closeTabSource).toContain("session.tabs.count == 1");
    expect(closeTabSource).toContain("removedTab.chromiumView?.closeBrowser()");
    expect(closeTabSource).toContain('title: "New Tab"');
    expect(closeTabSource).not.toContain("session.tabs.count > 1");

    const browserWakeSource = sourceBetween(
      nativeSidebarSource,
      "function wakeProjectEditorSurface",
      "function restoreActiveProjectEditorAtStartup",
    );
    expect(browserWakeSource).toContain("activeBrowserTabIsPlaceholder");
    expect(browserWakeSource).toContain('status: hasAwakeTargetMode || activeBrowserTabIsPlaceholder ? "running" : "opening"');
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

  test("defaults CEF browser color scheme to Light and applies menu changes", () => {
    /*
     * CDXC:ChromiumBrowserPanes 2026-06-18-22:50:
     * Hidden browser address-bar color-scheme controls should behave as if Light
     * is selected by default. CEF must receive real Chromium color-scheme
     * emulation so pages observe the choice through prefers-color-scheme.
     */
    expect(cefBridgeHeaderSource).toContain("setPreferredColorScheme(_:)");
    expect(cefBridgeSource).toContain('#include "include/cef_values.h"');
    expect(cefBridgeSource).toContain('"Emulation.setEmulatedMedia"');
    expect(cefBridgeSource).toContain('"prefers-color-scheme"');

    const themeModeSource = sourceBetween(
      terminalWorkspaceSource,
      "private enum BrowserPaneThemeMode",
      "private static let browserToolbarHeight",
    );
    expect(themeModeSource).toContain("var preferredColorSchemeOverride: String?");
    expect(themeModeSource).toContain('return "light"');
    expect(themeModeSource).toContain('return "dark"');
    expect(terminalWorkspaceSource).toContain("private var browserThemeMode: BrowserPaneThemeMode = .light");

    const hostInitSource = sourceBetween(
      terminalWorkspaceSource,
      "init(\n    browserView: NSView",
      "  /*\n   CDXC:SourceCEFDragDrop",
    );
    expect(hostInitSource).toContain("applyBrowserThemeMode(browserThemeMode)");

    const replaceSource = sourceBetween(
      terminalWorkspaceSource,
      "func replaceHostedBrowserView(",
      "  func focusAddressField",
    );
    expect(replaceSource).toContain("applyBrowserThemeMode(browserThemeMode)");

    const applySource = sourceBetween(
      terminalWorkspaceSource,
      "private func applyBrowserThemeMode",
      "  @objc private func showImportSettings",
    );
    expect(applySource).toContain("chromiumView?.setPreferredColorScheme(mode.preferredColorSchemeOverride)");
    expect(applySource).not.toContain("leaves Chromium rendering alone");
  });

  test("disables browser feedback tool buttons on GitHub pages", () => {
    /*
     * CDXC:BrowserFeedbackTools 2026-06-15-01:52:
     * Browser feedback tools are unavailable on github.com. The GPUI and
     * native AppKit browser toolbars must disable the feedback button on that
     * host and show the site-specific tooltip instead of accepting clicks.
     */
    expect(gpuiMainSource).toContain(
      'const BROWSER_FEEDBACK_TOOL_UNAVAILABLE_TOOLTIP: &str = "This site disallows using this tool";',
    );
    const gpuiToolbarSource = sourceBetween(
      gpuiMainSource,
      "fn render_browser_toolbar(&self",
      "fn render_browser_address_field",
    );
    expect(gpuiToolbarSource).toContain(
      "let feedback_tool_unavailable = browser_feedback_tool_unavailable_url(&self.browser_url);",
    );
    expect(gpuiToolbarSource).toContain("!feedback_tool_unavailable");
    expect(gpuiToolbarSource).toContain(".then_some(BROWSER_FEEDBACK_TOOL_UNAVAILABLE_TOOLTIP)");

    const gpuiPredicateSource = sourceBetween(
      gpuiMainSource,
      "fn browser_feedback_tool_unavailable_url",
      "fn project_name",
    );
    expect(gpuiPredicateSource).toContain('host == "github.com" || host.ends_with(".github.com")');
    expect(gpuiPredicateSource).toContain("trim_end_matches('.')");

    expect(terminalWorkspaceSource).toContain(
      'private static let feedbackToolUnavailableTooltip = "This site disallows using this tool"',
    );
    const nativeFeedbackButtonSource = sourceBetween(
      terminalWorkspaceSource,
      "private func updateBrowserFeedbackToolButton()",
      "  @objc private func injectFeedbackTool()",
    );
    expect(nativeFeedbackButtonSource).toContain(
      "let feedbackToolUnavailable = Self.browserFeedbackToolUnavailable(urlString: currentURLString())",
    );
    expect(nativeFeedbackButtonSource).toContain("reactGrabButton.isEnabled = !feedbackToolUnavailable");
    expect(nativeFeedbackButtonSource).toContain(
      "reactGrabButton.toolTip = NativeTooltip.text(",
    );

    const nativePredicateSource = sourceBetween(
      terminalWorkspaceSource,
      "private static func browserFeedbackToolUnavailable",
      "  private func canGoBack()",
    );
    expect(nativePredicateSource).toContain('host == "github.com" || host.hasSuffix(".github.com")');
    expect(nativePredicateSource).toContain('CharacterSet(charactersIn: ".")');

    const nativeInjectionSource = sourceBetween(
      terminalWorkspaceSource,
      "  @objc private func injectFeedbackTool()",
      "  @objc private func showProfilePicker()",
    );
    expect(nativeInjectionSource).toContain(
      "guard !Self.browserFeedbackToolUnavailable(urlString: currentURLString()) else",
    );
  });

  test("adds project-family browser history to native browser toolbar", () => {
    /*
     * CDXC:BrowserHistory 2026-06-15-10:25:
     * The native browser address toolbar should show a History button
     * immediately left of Profile. Its menu reads project-family history owned
     * by the sidebar, de-duplicates URLs, starts with the latest 20, and pages
     * additional rows through Show More while retaining no more than 140 links.
     * Long page titles and URLs must truncate before NSMenu measures the row so
     * the dropdown stays within the 350px max-width budget.
     * Rows without page favicons should still show a browser icon instead of
     * leaving the menu image column empty.
     * Show More must reopen the larger menu at the original History button
     * anchor, the menu must label itself with "History", icons should align with
     * the title line, and selecting a row should open a new tab instead of
     * replacing the current page.
     */
    expect(sharedHostProtocolSource).toContain('type: "setBrowserHistory";');
    expect(sharedHostProtocolSource).toContain("browserHistoryScopeId: string;");
    expect(nativeSidebarSource).toContain("type NativeProjectBrowserHistoryItem");
    expect(nativeSidebarSource).toContain("const PROJECT_BROWSER_HISTORY_MAX_ITEMS = 140");
    expect(nativeSidebarSource).toContain("function projectBrowserHistoryScopeIdForProject");
    expect(nativeSidebarSource).toContain("project.worktree?.parentProjectId?.trim() || project.projectId");
    expect(nativeSidebarSource).toContain("function normalizeProjectBrowserHistory");
    expect(nativeSidebarSource).toContain("recordBrowserSessionHistoryVisit");
    expect(nativeSidebarSource).toContain("recordProjectBrowserHistoryVisit(project, {");

    const toolbarSource = sourceBetween(
      terminalWorkspaceSource,
      "private static let browserToolbarHeight",
      "private static func describeFrame",
    );
    expect(toolbarSource).toContain("private static let browserHistoryPageSize = 20");
    expect(toolbarSource).toContain("private static let browserHistoryMenuMaxWidth: CGFloat = 350");
    expect(terminalWorkspaceSource).toContain("private static let browserHistoryMaxItems = 140");
    expect(terminalWorkspaceSource).toContain(".prefix(Self.browserHistoryMaxItems)");
    expect(toolbarSource).toContain("private let historyButton = WebPaneHostView.makeToolbarButton(");
    expect(toolbarSource).toContain('systemSymbolName: "clock.arrow.circlepath"');
    expect(toolbarSource).toContain(
      "let rightButtons = [zoomButton, reactGrabButton, historyButton, profileButton, appearanceButton, devToolsButton]",
    );
    expect(toolbarSource).toContain("Array(browserHistoryItems.prefix(browserHistoryVisibleLimit))");
    expect(toolbarSource).toContain("NSMenuItem.separator()");
    expect(toolbarSource).toContain('titleItem.attributedTitle = Self.browserHistoryMenuHeaderTitle()');
    expect(toolbarSource).toContain("browserHistoryMenuAnchorPointInWindow = resolvedBrowserHistoryMenuAnchorPointInWindow()");
    expect(toolbarSource).toContain("NSMenu.popUpContextMenu(menu, with: browserHistoryMenuEvent(), for: historyButton)");
    expect(toolbarSource).toContain('title: "Show More"');
    expect(toolbarSource).toContain("onHistoryNavigation?(url) == true");
    expect(toolbarSource).toContain("private static func browserHistoryMenuHeaderTitle() -> NSAttributedString");
    expect(toolbarSource).toContain("NativeTooltip.browserHistory(");
    expect(toolbarSource).toContain("truncatedBrowserHistoryMenuText(");
    expect(toolbarSource).toContain("browserHistoryMenuTextWidth(");
    expect(toolbarSource).toContain("private static let browserHistoryMenuIconCanvasSize = CGSize(width: 16, height: 24)");
    expect(toolbarSource).toContain("return topAlignedBrowserHistoryMenuImage(image)");
    expect(toolbarSource).toContain("private static func topAlignedBrowserHistoryMenuImage(_ sourceImage: NSImage) -> NSImage");
    expect(toolbarSource).toContain("return topAlignedBrowserHistoryMenuImage(browserHistoryFallbackMenuImage())");
    expect(toolbarSource).toContain("private static func browserHistoryFallbackMenuImage() -> NSImage");
    expect(toolbarSource).toContain("History rows without page favicons still need a visible browser identity icon.");
    expect(toolbarSource).toContain("private func browserHistoryMenuEvent() -> NSEvent");
    expect(terminalWorkspaceSource).toContain(".browserOpenInNewTabRequested(sourceSessionId: command.sessionId, url: url.absoluteString)");
    expect(terminalWorkspaceSource).toContain('reason: "projectEditorBrowserHistory"');
  });

  test("caps native tooltip text width through shared helper", () => {
    /*
     * CDXC:NativeTooltips 2026-06-15-10:25:
     * Native AppKit tooltips should use one 225px wrapping helper so long
     * browser history titles/URLs and titlebar labels do not create wide hover
     * bubbles.
     */
    expect(nativeTooltipSource).toContain("static let maxWidth: CGFloat = 225");
    expect(nativeTooltipSource).toContain("static func browserHistory(title: String, url: String) -> String");
    expect(nativeTooltipSource).toContain('text("\\(title)\\n\\n\\(url)")');
    expect(terminalWorkspaceSource).toContain("button.toolTip = NativeTooltip.text(tooltip)");
    expect(terminalWorkspaceSource).toContain("menuItem.toolTip = NativeTooltip.browserHistory(");
    expect(appDelegateSource).toContain("appTitlebarLabel?.toolTip = NativeTooltip.text(normalizedTitle)");
  });
});
