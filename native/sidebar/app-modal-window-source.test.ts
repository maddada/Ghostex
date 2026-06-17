import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const appDelegateSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/AppDelegate.swift", import.meta.url),
  "utf8",
);
const modalStylesSource = readFileSync(
  new URL("../../sidebar/styles/modals.css", import.meta.url),
  "utf8",
);
const delayedSendModalSource = readFileSync(
  new URL("../../sidebar/delayed-send-modal.tsx", import.meta.url),
  "utf8",
);
const sidebarStylesSource = readFileSync(
  new URL("../../sidebar/styles.css", import.meta.url),
  "utf8",
);
const modalHostSource = readFileSync(new URL("./modal-host.tsx", import.meta.url), "utf8");
const nativeSidebarSource = readFileSync(new URL("./native-sidebar.tsx", import.meta.url), "utf8");
const sidebarAppSource = readFileSync(
  new URL("../../sidebar/sidebar-app.tsx", import.meta.url),
  "utf8",
);
const sortableSessionCardSource = readFileSync(
  new URL("../../sidebar/sortable-session-card.tsx", import.meta.url),
  "utf8",
);
const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");
const commandPaletteSource = readFileSync(
  new URL("../../sidebar/command-palette.tsx", import.meta.url),
  "utf8",
);
const settingsModalSource = readFileSync(
  new URL("../../sidebar/settings-modal.tsx", import.meta.url),
  "utf8",
);
const worktreeCreateModalSource = readFileSync(
  new URL("../../sidebar/worktree-create-modal.tsx", import.meta.url),
  "utf8",
);

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("native app modal window source", () => {
  test("keeps floating prompt editor input owned by its native child window", () => {
    /*
    CDXC:PromptEditor 2026-06-13-13:48:
    The floating prompt editor must not publish root-window input regions from React. Its native child window owns frame, focus, movement, resize, and event delivery.
    */
    expect(modalHostSource).toContain("The floating prompt editor is a native child window");
    expect(modalHostSource).not.toContain("floatingPromptEditorHitRegion");
    expect(modalHostSource).not.toContain("PromptEditor:hitRegion");
    expect(modalHostSource).not.toContain("react.hitRegion");
    expect(appDelegateSource).not.toContain("updateFloatingPromptEditorHitRegion");
    expect(appDelegateSource).not.toContain("native.hitRegion");
  });

  test("shows the native prompt editor resize handle and opens previews from the full thumbnail", () => {
    /*
    CDXC:PromptEditor 2026-06-16-10:23:
    The native child-window prompt editor needs a visible bottom-right resize handle with a resize cursor, and every visible part of an image thumbnail should open the image preview except the explicit remove button.

    CDXC:PromptEditor 2026-06-16-21:32:
    The bottom thumbnail shelf must own normal WebKit clicks. Native prompt-window drag/resize handling may intercept only the explicit bottom-right resize handle, not the whole bottom edge.
    */
    const nativeResizeRule = sourceBetween(
      modalStylesSource,
      ".app-modal-host-native-window-body .floating-prompt-editor-resize {",
      ".floating-prompt-editor-titlebar {",
    );
    expect(nativeResizeRule).toContain("display: block");
    expect(nativeResizeRule).not.toContain("display: none");

    const resizeRule = sourceBetween(
      modalStylesSource,
      "\n.floating-prompt-editor-resize {",
      "\n.floating-prompt-editor-resize::before",
    );
    expect(resizeRule).toContain("cursor: nwse-resize");
    expect(resizeRule).toContain("height: 24px");
    expect(resizeRule).toContain("width: 24px");

    expect(modalHostSource).toContain('className="floating-prompt-editor-image-open"');
    expect(modalHostSource).toContain('onClick={() => setOpenImagePreview(preview)}');
    expect(modalHostSource).toContain("onPointerDown={isNativeWindowSurface ? undefined : startResize}");
    expect(appDelegateSource).toContain("promptEditorBottomRightResizeHandleSize");
    expect(appDelegateSource).toContain("return [.right, .bottom]");
    const promptEditorResizeEdges = sourceBetween(
      appDelegateSource,
      "private func promptEditorResizeEdges(for point: CGPoint) -> ResizeEdges {",
      "private func resizePromptEditorWindow(from event: NSEvent, edges: ResizeEdges) {",
    );
    expect(promptEditorResizeEdges).not.toContain("point.y <= promptEditorResizeMargin");
    expect(promptEditorResizeEdges).toContain("point.x <= promptEditorResizeMargin");
    expect(promptEditorResizeEdges).toContain("point.x >= frame.width - promptEditorResizeMargin");
    expect(appDelegateSource).toContain(
      "panel.promptEditorBottomRightResizeHandleSize = Self.floatingPromptEditorResizeHandleSize",
    );
  });

  test("keeps prompt editor text through app and window close", () => {
    /*
    CDXC:PromptEditor 2026-06-16-10:36:
    Prompt editor text must not be lost when the app or main window closes before the user presses Save. React should live-write Monaco edits to the native prompt file, and AppKit lifecycle close should mark that current draft saved instead of cancelling the editor.
    */
    expect(modalHostSource).toContain('type: "floatingPromptEditorDraftUpdate"');
    expect(modalHostSource).toContain("postDraftUpdate");
    expect(modalHostSource).toContain("refreshEditorTextDerivedState");
    expect(modalHostSource).toContain('refreshEditorTextDerivedState(existingEditor, "contentChanged")');
    expect(modalHostSource).toContain('refreshEditorTextDerivedState(monacoEditor, "contentChanged")');
    expect(modalHostSource).toContain('refreshEditorTextDerivedState(monacoEditor, "pageLifecycle", { force: true })');

    expect(appDelegateSource).toContain('case "floatingPromptEditorDraftUpdate":');
    expect(appDelegateSource).toContain("updateFloatingPromptEditorDraft(message: message)");
    expect(appDelegateSource).toContain("func saveActiveFloatingPromptEditorForAppLifecycleClose(reason: String)");
    expect(appDelegateSource).toContain("writeFloatingPromptEditorStatusFile(active.statusFile, status: \"saved\")");
    expect(appDelegateSource).toContain(
      "saveActiveFloatingPromptEditorForAppLifecycleClose(\n      reason: \"mainWindowWillClose\")",
    );
    expect(appDelegateSource).toContain(
      "saveActiveFloatingPromptEditorForAppLifecycleClose(\n      reason: \"applicationWillTerminate\")",
    );
  });

  test("animates native app toasts from the bottom center of the app window", () => {
    /*
    CDXC:AppToasts 2026-06-13-19:57:
    Native macOS toasts should center on the app window, start from a lower bottom-center frame, and fade upward into the stack so sidebar placement and the initial NSPanel origin never leak into the visible animation.
    */
    const rootLayoutToastAnchor = sourceBetween(
      appDelegateSource,
      "workspaceView.frame = frames.workspace",
      "layoutRootChromeLayers(frames: frames)",
    );
    expect(rootLayoutToastAnchor).toContain("anchorFrame: bounds");
    expect(rootLayoutToastAnchor).not.toContain("anchorFrame: frames.workspace");

    const nativeToastController = sourceBetween(
      appDelegateSource,
      "private final class NativeAppToastController",
      "private final class NativeAppToastView",
    );
    expect(nativeToastController).toContain("private static let enterYOffset: CGFloat = 24");
    expect(nativeToastController).toContain(
      "layoutPanels(animated: true, enteringToastId: enteringToastId)",
    );
    expect(nativeToastController).toContain("frame.offsetBy(dx: 0, dy: -Self.enterYOffset)");
    expect(nativeToastController).toContain(
      "item.panel.setFrame(Self.enterStartFrame(for: frame), display: true)",
    );
    expect(nativeToastController).toContain("context.timingFunction = Self.toastAnimationTimingFunction()");
    expect(nativeToastController).toContain("item.panel.animator().alphaValue = 1");
    expect(nativeToastController).toContain("x: floor(screenAnchorFrame.midX - size.width / 2)");
  });

  test("sizes native app toasts from wrapped description text", () => {
    /*
    CDXC:AppToasts 2026-06-16-18:41:
    Native app toasts should measure wrapped description text and grow the
    panel height instead of using a fixed two-line frame that can cut off Git
    error messages.
    */
    const nativeToastView = sourceBetween(
      appDelegateSource,
      "private final class NativeAppToastView",
      "private final class NativeToastActionButton",
    );
    expect(nativeToastView).toContain("descriptionField.lineBreakMode = .byWordWrapping");
    expect(nativeToastView).toContain("descriptionField.maximumNumberOfLines = 0");
    expect(nativeToastView).toContain("descriptionField.cell?.wraps = true");
    expect(nativeToastView).toContain("measuredDescriptionHeight(description, width: textWidth)");
    expect(nativeToastView).toContain("boundingRect(");
    expect(nativeToastView).not.toContain("descriptionField.lineBreakMode = .byTruncatingTail");
    expect(nativeToastView).not.toContain("let baseHeight: CGFloat = hasDescription ? 72 : 52");
    expect(nativeToastView).not.toContain(
      "descriptionField.frame = CGRect(x: leadingContentX, y: 37, width: textWidth, height: bounds.height - 49)",
    );
  });

  test("opens first-launch setup 90px taller than the generic management modals", () => {
    /*
    CDXC:FirstLaunchSetup 2026-06-12-07:13:
    The macOS first-launch setup modal must open 90px taller than its old 1120x760 native child window so onboarding steps with hook status and footer actions are not clipped.
    Keep Agents Hub at the generic management-modal height while firstLaunchSetup and the legacy tipsAndTricks alias use the taller frame.

    CDXC:HighlightedFeatures 2026-06-16-08:17:
    Highlighted Features uses the same 1120x850 native child-window footprint while keeping the existing discoverGhostex modal id.
    */
    const defaultSize = sourceBetween(
      appDelegateSource,
      "private func defaultSize(for modal: String) -> CGSize",
      "private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize",
    );
    expect(defaultSize).toContain('case "agentsHub":');
    expect(defaultSize).toContain("return CGSize(width: 1120, height: 760)");
    expect(defaultSize).toContain('case "firstLaunchSetup", "tipsAndTricks":');
    expect(defaultSize).toContain("return CGSize(width: 1120, height: 850)");
    expect(defaultSize).toContain('case "discoverGhostex":');

    const modalTitle = sourceBetween(
      appDelegateSource,
      "private func title(for modal: String) -> String",
      "private final class TitlebarDropdownPanelController",
    );
    expect(modalTitle).toContain('case "discoverGhostex":');
    expect(modalTitle).toContain('return "Highlighted Features"');
  });

  test("opens Settings over the exact macOS workspace area", () => {
    /*
    CDXC:AppModals 2026-06-15-10:12:
    Settings must cover the whole workspace area while leaving the sidebar,
    divider, and titlebar owned by their existing native sibling frames.
    */
    const preferredFrame = sourceBetween(
      appDelegateSource,
      "private func preferredNativeAppModalContentFrame(",
      "private func closeNativeAppModalWindow",
    );
    expect(preferredFrame).toContain('guard modal == "settings"');
    expect(preferredFrame).toContain("let workspaceFrame = rootLayoutFrames().workspace");
    expect(preferredFrame).toContain("parentWindow.convertToScreen(convert(workspaceFrame, to: nil))");

    const openNativeModal = sourceBetween(
      appDelegateSource,
      "private func openNativeAppModalWindow(",
      "private func preferredNativeAppModalContentFrame",
    );
    expect(openNativeModal).toContain("let resolvedPreferredContentFrame = preferredNativeAppModalContentFrame(");
    expect(openNativeModal).toContain("preferredContentFrame: resolvedPreferredContentFrame");

    const appModalWindowController = sourceBetween(
      appDelegateSource,
      "private final class AppModalWindowController",
      "private final class TitlebarDropdownPanelController",
    );
    expect(appModalWindowController).toContain(
      "panel.hasShadow = !shouldUseExactContentFrame(modal: modal)",
    );
    expect(appModalWindowController).toContain("if shouldUseExactContentFrame(modal: modal)");
    expect(appModalWindowController).toContain("private func shouldUseExactContentFrame(modal: String?) -> Bool");
    expect(appModalWindowController).toContain('modal == "settings"');
    expect(appModalWindowController).toContain('case "settings":');
    expect(appModalWindowController).toContain("return CGSize(width: 1, height: 1)");

    const settingsStyles = sourceBetween(
      sidebarStylesSource,
      ".app-modal-host-native-window-body .ghostex-settings-shadcn.settings-modal-dialog {",
      ".ghostex-settings-shadcn.settings-modal-dialog .ghostex-modal-heading-bar",
    );
    expect(settingsStyles).toContain("border-bottom: 1px solid #252525 !important;");
    expect(settingsStyles).toContain("border-right: 1px solid #252525 !important;");
    expect(settingsStyles).toContain("border-top: 1px solid #252525 !important;");
    expect(settingsStyles).toContain("box-sizing: border-box;");
    expect(settingsStyles).toContain("height: 100vh;");
    expect(settingsStyles).toContain("max-height: 100vh;");
    expect(settingsStyles).toContain("max-width: 100vw;");
    expect(settingsStyles).toContain("width: 100vw;");
  });

  test("opens titlebar dropdown panels as keyable child windows for hover", () => {
    /*
    CDXC:TitlebarDropdowns 2026-06-16-09:22:
    Resources, Git, Tips, Keep Awake, Actions, Open In, and mode dropdowns all
    use the titlebar child-window controller. Those panels must be keyable and
    mouse-move aware so WKWebView hover detection works like Settings instead
    of inheriting nonactivating-panel hover gaps.
    */
    const dropdownPanelClass = sourceBetween(
      appDelegateSource,
      "private final class TitlebarDropdownPanel",
      "private final class TitlebarDropdownPanelController",
    );
    expect(dropdownPanelClass).toContain("override var canBecomeKey: Bool { true }");
    expect(dropdownPanelClass).toContain("override var canBecomeMain: Bool { false }");

    const dropdownController = sourceBetween(
      appDelegateSource,
      "private final class TitlebarDropdownPanelController",
      "final class ReactTitlebarChromeView",
    );
    expect(dropdownController).toContain("let panel = TitlebarDropdownPanel(");
    expect(dropdownController).toContain("styleMask: [.borderless]");
    expect(dropdownController).toContain("panel.acceptsMouseMovedEvents = true");
    expect(dropdownController).toContain("panel.makeKeyAndOrderFront(nil)");
    expect(dropdownController).toContain("panel.makeFirstResponder(webView)");
    expect(dropdownController).not.toContain(".nonactivatingPanel");
  });

  test("reframes Settings when workspace geometry changes", () => {
    /*
    CDXC:SettingsLayout 2026-06-15-14:07:
    Settings is a full-workspace native child window. Main-window resize,
    sidebar collapse, and sidebar side changes should reframe Settings to the
    current workspace instead of closing it or keeping the old frame.

    CDXC:AppModals 2026-06-16-19:50:
    Resize/layout refreshes now go through the shared child-window frame helper
    so Settings and the onboarding AppKit backdrop stay aligned together.
    */
    const resizeStart = sourceBetween(
      appDelegateSource,
      "func windowWillStartLiveResize(_ notification: Notification)",
      "func windowDidResize(_ notification: Notification)",
    );
    expect(resizeStart).toContain("resizedWindow === mainWindow");
    expect(resizeStart).toContain("updateAppModalChildWindowFramesIfNeeded()");

    const updateSettingsFrame = sourceBetween(
      appDelegateSource,
      "fileprivate func updateSettingsModalWorkspaceFrameIfNeeded()",
      "private func shouldShowOnboardingAppModalBackdrop",
    );
    expect(updateSettingsFrame).toContain('currentModalKind == "settings"');
    expect(updateSettingsFrame).toContain("preferredNativeAppModalContentFrame(");
    expect(updateSettingsFrame).toContain("nativeAppModalWindowController?.updateContentFrame(");
    expect(updateSettingsFrame).not.toContain("closeNativeAppModalWindow(");

    const rootChrome = sourceBetween(
      appDelegateSource,
      "func setSidebarSide(_ side: SidebarSide)",
      "private func setTitlebarSidebarCollapsed",
    );
    expect(rootChrome).toContain("updateAppModalChildWindowFramesIfNeeded()");

    const appModalWindowController = sourceBetween(
      appDelegateSource,
      "private final class AppModalWindowController",
      "private static func sidebarTheme",
    );
    expect(appModalWindowController).toContain("func updateContentFrame(");
    expect(appModalWindowController).toContain("panel.setFrame(panel.frameRect(forContentRect: contentFrame), display: true)");
  });

  test("dismisses Settings from sidebar and titlebar navigation", () => {
    /*
    CDXC:SettingsDismissal 2026-06-15-14:07:
    Settings should close when users navigate through sidebar sessions, sidebar
    creation/search/modal actions, titlebar mode/action buttons, or native
    create-session and rename hotkeys.

    CDXC:SettingsDismissal 2026-06-16-02:12:
    Quick-row creation actions replaced the old reference-session label.
    Assert the current terminal, browser, and agent quick-create dismissal
    markers so Settings still closes before those actions run.
    */
    expect(sidebarAppSource).toContain("dismissAppModalForSidebarNavigation");
    expect(sidebarAppSource).toContain("SettingsDismissal:sessionClick");
    expect(sidebarAppSource).toContain("SettingsDismissal:createQuickTerminal");
    expect(sidebarAppSource).toContain("SettingsDismissal:createQuickBrowser");
    expect(sidebarAppSource).toContain("SettingsDismissal:createQuickAgent");
    expect(sidebarAppSource).toContain("SettingsDismissal:previousSessionsTextSearch");
    expect(sidebarAppSource).toContain("SettingsDismissal:agentsHub");
    expect(sidebarAppSource).toContain("SettingsDismissal:renameSession");

    expect(sortableSessionCardSource).toContain("SettingsDismissal:sessionRowRename");
    expect(titlebarHostSource).toContain("closeAppModalFromTitlebarNavigation");
    expect(titlebarHostSource).toContain("SettingsDismissal:titlebarAction");
    expect(titlebarHostSource).toContain("SettingsDismissal:titlebarAgentsMode");
    expect(titlebarHostSource).toContain("SettingsDismissal:titlebarSourceMode");
    expect(titlebarHostSource).toContain("SettingsDismissal:titlebarBrowserMode");
    expect(titlebarHostSource).toContain("SettingsDismissal:titlebarKanbanMode");

    expect(nativeSidebarSource).toContain("SettingsDismissal:nativeHotkeyCreateSession");
    expect(nativeSidebarSource).toContain("SettingsDismissal:nativeHotkeyRename");
    expect(nativeSidebarSource).toContain("SettingsDismissal:nativeSidebarCreateSession");
  });

  test("keeps titlebar action crash breadcrumbs behind Debugging Mode", () => {
    /*
    CDXC:GxserverLogs 2026-06-15-20:39:
    Titlebar action crash traces are breadcrumbs from an isolated webview, not
    normal-mode crash warnings. The titlebar host should only post them while
    Debugging Mode is enabled.
    */
    expect(titlebarHostSource).toContain("function appendTitlebarActionCrashDebugLog(");
    expect(titlebarHostSource).toContain("if (!debuggingMode) {");
    expect(titlebarHostSource).toContain("projectState.debuggingMode");
    expect(titlebarHostSource).toContain("nativeSidebar.actionCrashTrace.titlebarClick");
  });

  test("relays dropdown Quick Action selection to the main titlebar button", () => {
    /*
    CDXC:TitlebarActions 2026-06-16-18:31:
    The Quick Actions dropdown runs in a native child WKWebView, so selecting an
    action there must explicitly update the main titlebar WKWebView's selected
    action id before forwarding execution to the sidebar runner.
    */
    expect(titlebarHostSource).toContain("setLastActionCommandId: (commandId: string) => void");
    expect(titlebarHostSource).toContain("setLastActionCommandId: (commandId) => {");
    expect(titlebarHostSource).toContain("setSelectedActionCommandId(commandId);");
    expect(appDelegateSource).toContain("setLastActionCommandId?.(\\(commandIdJson))");
    expect(appDelegateSource).toContain("runSidebarCommandFromTitlebar?.(\\(commandIdJson))");
  });

  test("rotates AppDelegate-owned support logs", () => {
    /*
    CDXC:GxserverLogs 2026-06-15-20:39:
    Shared AppDelegate log files such as native-host-lifecycle should rotate at
    the common support-bundle limit instead of growing without bound during long
    Debugging Mode sessions.

    CDXC:Diagnostics 2026-06-16-12:22:
    App startup should also schedule a one-minute delayed line-retention pass so
    shared support logs stay bounded even when old debug storms predate byte
    rotation.

    CDXC:Diagnostics 2026-06-16-14:09:
    Retention should keep one current split file per `.log`/`.jsonl` basename,
    delete older rotated siblings from that group, then trim the retained file
    to 25,000 lines.
    */
    expect(appDelegateSource).toContain("sharedLogMaxFileBytes: UInt64 = 25 * 1024 * 1024");
    expect(appDelegateSource).toContain("sharedLogMaxRotatedFiles = 3");
    expect(appDelegateSource).toContain("sharedLogMaxRetainedLines = 25_000");
    expect(appDelegateSource).toContain("sharedLogRetentionStartupDelay: TimeInterval = 60");
    expect(appDelegateSource).toContain("rotateSharedLogIfNeeded(logURL: logURL");
    expect(appDelegateSource).toContain("Self.scheduleSupportLogLineRetentionAfterStartup()");
    expect(appDelegateSource).toContain("private static func scheduleSupportLogLineRetentionAfterStartup()");
    expect(appDelegateSource).toContain("private static func pruneSupportLogFile(_ logURL: URL, maxLines: Int) throws");
    expect(appDelegateSource).toContain("private static func sharedSupportLogBaseName(_ fileName: String) -> String?");
    expect(appDelegateSource).toContain("private static func preferredSharedSupportLogFile(in fileURLs: [URL]) -> URL?");
    expect(appDelegateSource).toContain("try manager.removeItem(at: fileURL)");
    expect(appDelegateSource).toContain('baseName.hasSuffix(".log") || baseName.hasSuffix(".jsonl")');
    expect(appDelegateSource).toContain("native-host-lifecycle.log");
    expect(appDelegateSource).toContain("sampledNativeHostLifecycleMessage");
    expect(appDelegateSource).toContain('"activationBoundaryInput"');
    expect(appDelegateSource).toContain('"workspaceApplicationActivated"');
  });

  test("ignores duplicate native app-modal opens except command-palette mode switches", () => {
    /*
    CDXC:AppModals 2026-06-15-10:27:
    Opening the same native app modal again should be a no-op for Settings,
    Rename Session, Previous Sessions, and the rest of the app-modal family.
    Command Palette remains the exception because repeat Cmd+P/Cmd+Shift+P
    requests switch the already-visible palette between files and commands.
    */
    const openNativeModal = sourceBetween(
      appDelegateSource,
      "private func openNativeAppModalWindow(",
      "private func preferredNativeAppModalContentFrame",
    );
    expect(openNativeModal).toContain("shouldIgnoreDuplicateNativeAppModalOpen(");
    expect(openNativeModal).toContain("if !isVisibleCommandPaletteModeSwitch(modal: modal)");

    const duplicateGuard = sourceBetween(
      appDelegateSource,
      "private func shouldIgnoreDuplicateNativeAppModalOpen(",
      "private func preferredNativeAppModalContentFrame",
    );
    expect(duplicateGuard).toContain('guard modal != "commandPalette"');
    expect(duplicateGuard).toContain("isActiveOrPendingModal(modal)");
    expect(duplicateGuard).toContain("nativeBridge.appModal.open.duplicateIgnored");
    expect(duplicateGuard).toContain("private func isVisibleCommandPaletteModeSwitch(modal: String) -> Bool");

    const appModalWindowController = sourceBetween(
      appDelegateSource,
      "private final class AppModalWindowController",
      "private final class TitlebarDropdownPanelController",
    );
    expect(appModalWindowController).toContain("func isActiveOrPendingModal(_ modal: String) -> Bool");

    expect(modalHostSource).toContain("commandPaletteOpenRequestSequence");
    expect(modalHostSource).toContain("setCommandPaletteOpenRequestSequence((sequence) => sequence + 1)");
    expect(modalHostSource).toContain("openRequestSequence={commandPaletteOpenRequestSequence}");
  });

  test("keeps Add Worktree fixed at 570x574 with exact native-window padding", () => {
    /*
    CDXC:WorktreeModal 2026-06-12-10:51:
    Add Worktree must open as an exact 570x550 native child window in the macOS app, separate from the larger Git Commit review modal size.

    CDXC:WorktreeModal 2026-06-12-11:10:
    Add Worktree must keep the 570px fixed width and own its native child-window WebView padding directly.

    CDXC:WorktreeModal 2026-06-13-18:39:
    Add Worktree must use the same top-right shadcn close X pattern as Rename Session, remove the footer Cancel button, use 17px native-window edge padding, and fit the shorter footer stack into a 570x574 child window.

    */
    const defaultSize = sourceBetween(
      appDelegateSource,
      "private func defaultSize(for modal: String) -> CGSize",
      "private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize",
    );
    expect(defaultSize).toContain('case "gitCommit":');
    expect(defaultSize).toContain("return CGSize(width: 1020, height: 760)");
    expect(defaultSize).toContain('case "worktree":');
    expect(defaultSize).toContain("return CGSize(width: 570, height: 574)");

    const shouldLockContentSize = sourceBetween(
      appDelegateSource,
      "private func shouldLockContentSize(modal: String) -> Bool",
      "private func minimumContentSize(for modal: String?) -> CGSize",
    );
    expect(shouldLockContentSize).toContain('|| modal == "worktree"');

    const worktreeStyles = sourceBetween(
      modalStylesSource,
      ".worktree-create-modal-shadcn {",
      ".delayed-send-modal-shadcn {",
    );
    expect(worktreeStyles).toContain(
      ".app-modal-host-native-window-body .worktree-create-modal-shadcn",
    );
    expect(worktreeStyles).toContain("height: 100vh;");
    expect(worktreeStyles).toContain("max-height: 100vh;");
    expect(worktreeStyles).toContain("max-width: 100vw;");
    expect(worktreeStyles).toContain("padding: 17px;");
    expect(worktreeStyles).toContain("width: 100vw;");
    expect(worktreeStyles).toContain(
      '.app-modal-host-native-window-body .worktree-create-modal-shadcn [data-slot="dialog-close"]',
    );
    expect(worktreeStyles).toContain("right: 17px;");
    expect(worktreeStyles).toContain("top: 17px;");

    const worktreeDialogContent = sourceBetween(
      worktreeCreateModalSource,
      "<DialogContent",
      "<form",
    );
    expect(worktreeDialogContent).toContain("showCloseButton");
    expect(worktreeDialogContent).toContain("initialFocus={focusInput}");

    const worktreeFooter = sourceBetween(
      worktreeCreateModalSource,
      "<DialogFooter>",
      "</DialogFooter>",
    );
    expect(worktreeFooter).not.toContain("Cancel");
  });

  test("retries Add Worktree first-prompt focus across native child-window focus", () => {
    /*
    CDXC:WorktreeModal 2026-06-15-11:30:
    Add Worktree is presented from the hidden native app-modal host. Source
    coverage keeps the first-prompt textarea focused for immediate typing,
    retries across native window activation, and preserves user interaction by
    stopping delayed focus after the user clicks or types in the modal.
    */
    expect(worktreeCreateModalSource).toContain("userInteractedAfterOpenRef.current = false;");
    expect(worktreeCreateModalSource).toContain("input.focus({ preventScroll: true });");
    expect(worktreeCreateModalSource).toContain(
      "input.setSelectionRange(selectionIndex, selectionIndex);",
    );
    expect(worktreeCreateModalSource).toContain(
      "const retryDelaysMs = [0, 16, 50, 100, 250, 500, 1000, 1600, 2400];",
    );
    expect(worktreeCreateModalSource).toContain('window.addEventListener("focus", handleWindowFocus);');
    expect(worktreeCreateModalSource).toContain(
      'window.removeEventListener("focus", handleWindowFocus);',
    );
    expect(worktreeCreateModalSource).toContain("onKeyDownCapture={markUserInteractedAfterOpen}");
    expect(worktreeCreateModalSource).toContain("onPointerDownCapture={markUserInteractedAfterOpen}");

    const enterHandler = sourceBetween(
      worktreeCreateModalSource,
      "const handleKeyDown = (event: ReactKeyboardEvent<HTMLTextAreaElement>) => {",
      "const handlePaste =",
    );
    expect(enterHandler).toContain('event.key !== "Enter"');
    expect(enterHandler).toContain("event.preventDefault();");
    expect(enterHandler).toContain("event.stopPropagation();");
    expect(enterHandler).toContain("onConfirm(createDraft(");
  });

  test("uses Settings search focus treatment for modal text controls", () => {
    /*
    CDXC:ModalTextFocus 2026-06-15-11:30:
    Editable text fields and textareas in app modals should use the same focus
    look as Settings search: white input border, no outer shadcn focus ring.

    CDXC:ModalTextFocus 2026-06-16-14:33:
    Settings text controls now share the Settings search focus token instead of
    hard-coding white, so theme tuning can change one variable without
    reintroducing the outer shadcn ring.
    */
    const modalTextFocusStyles = sourceBetween(
      modalStylesSource,
      '.command-config-modal-shadcn [data-slot="input"]:is(:focus, :focus-visible),',
      ".command-config-textarea-shadcn {",
    );
    expect(modalTextFocusStyles).toContain(
      '.command-config-modal-shadcn [data-slot="textarea"]:is(:focus, :focus-visible)',
    );
    expect(modalTextFocusStyles).toContain("border-color: #fff;");
    expect(modalTextFocusStyles).toContain("box-shadow: none;");
    expect(modalTextFocusStyles).toContain("outline: none;");

    const settingsTextFocusStyles = sourceBetween(
      sidebarStylesSource,
      '.ghostex-settings-shadcn [data-slot="input"]:is(:focus, :focus-visible),',
      ".ghostex-settings-shadcn .settings-modal-search-toolbar .session-search-input-icon",
    );
    expect(settingsTextFocusStyles).toContain(
      '.ghostex-settings-shadcn [data-slot="textarea"]:is(:focus, :focus-visible)',
    );
    expect(settingsTextFocusStyles).toContain("border-color: var(--settings-focus-border-color);");
    expect(settingsTextFocusStyles).toContain("box-shadow: none;");
    expect(settingsTextFocusStyles).toContain("outline: none;");
  });

  test("widens Git Commit 20px from the right side in the macOS app", () => {
    /*
    CDXC:TitlebarGit 2026-06-12-11:30:
    Git Commit review must be 20px wider than its prior 1000px native child window, with the old left edge preserved so the added width appears on the right diff side.
    */
    const defaultSize = sourceBetween(
      appDelegateSource,
      "private func defaultSize(for modal: String) -> CGSize",
      "private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize",
    );
    expect(defaultSize).toContain('case "gitCommit":');
    expect(defaultSize).toContain("return CGSize(width: 1020, height: 760)");

    const gitCommitFrame = sourceBetween(
      appDelegateSource,
      "private func gitCommitContentFrame(size: CGSize, parentWindow: NSWindow) -> CGRect",
      "private func clampFrameToVisibleScreen",
    );
    expect(gitCommitFrame).toContain("let previousCenteredWidth: CGFloat = 1000");
    expect(gitCommitFrame).toContain("x: parentWindow.frame.midX - previousCenteredWidth / 2");

    const constrainedContentFrame = sourceBetween(
      appDelegateSource,
      "private func constrainedContentFrame(",
      "private func shouldLockContentSize(modal: String) -> Bool",
    );
    expect(constrainedContentFrame).toContain('if modal == "gitCommit"');
    expect(constrainedContentFrame).toContain(
      "return gitCommitContentFrame(size: size, parentWindow: parentWindow)",
    );
  });

  test("keeps Rename Session fixed at 570x480 in the macOS app", () => {
    /*
    CDXC:SidebarRename 2026-06-12-05:05:
    Rename Session must keep a 540px React dialog cap and 9px side padding so it is 20px wider while reducing left/right content padding by 15px.

    CDXC:SidebarRename 2026-06-12-06:35:
    Rename Session must keep its 570px width but gain 80px of native-window height, opening as 570x480 so the generated-name controls and bottom action area fit.
    */
    const defaultSize = sourceBetween(
      appDelegateSource,
      "private func defaultSize(for modal: String) -> CGSize",
      "private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize",
    );
    expect(defaultSize).toContain('case "renameSession":');
    expect(defaultSize).toContain("return CGSize(width: 570, height: 480)");

    const shouldLockContentSize = sourceBetween(
      appDelegateSource,
      "private func shouldLockContentSize(modal: String) -> Bool",
      "private func minimumContentSize(for modal: String?) -> CGSize",
    );
    expect(shouldLockContentSize).toContain('modal == "previousSessions" || modal == "renameSession"');

    const renameStyles = sourceBetween(
      modalStylesSource,
      ".session-rename-modal-shadcn {",
      ".add-repository-modal-shadcn {",
    );
    expect(renameStyles).toContain("max-width: min(540px, calc(100vw - 2rem));");
    expect(renameStyles).toContain("padding-left: 9px;");
    expect(renameStyles).toContain("padding-right: 9px;");
  });

  test("routes unconfigured action setup to Settings Actions", () => {
    /*
    CDXC:ProjectActions 2026-06-15-15:29:
    The standalone Configure Action modal is removed. Empty or unconfigured
    action clicks should open Settings on the Actions page, which also explains
    that frequent commands can be run with one click or a hotkey.
    */
    expect(titlebarHostSource).toContain("openSidebarActionsSettings");
    expect(titlebarHostSource).toContain('initialTab: "actions"');
    expect(commandPaletteSource).toContain('initialTab: "actions"');
    expect(nativeSidebarSource).toContain("openNativeSidebarActionsSettings");
    expect(nativeSidebarSource).toContain('initialTab: "actions"');
    expect(settingsModalSource).toContain("Set frequently used terminal or browser commands here");
    expect(settingsModalSource).toContain("click or a hotkey.");
    expect(settingsModalSource).toContain("isSidebarCommandConfigured");
    expect(modalHostSource).not.toContain('"commandConfig"');
    expect(modalHostSource).not.toContain("CommandConfigModal");
    expect(titlebarHostSource).not.toContain('modal: "commandConfig"');
    expect(commandPaletteSource).not.toContain('modal: "commandConfig"');
    expect(nativeSidebarSource).not.toContain('modal: "commandConfig"');
  });

  test("keeps Delayed Send fixed at 472x269 in the macOS app", () => {
    /*
    CDXC:DelayedSend 2026-06-12-04:07:
    Delayed Send must open as a fixed 472x269 macOS child window, including a matching modal-specific minimum because the shared app-modal minimum is larger than this timer dialog.
    */
    const defaultSize = sourceBetween(
      appDelegateSource,
      "private func defaultSize(for modal: String) -> CGSize",
      "private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize",
    );
    expect(defaultSize).toContain('case "delayedSend":');
    expect(defaultSize).toContain("return CGSize(width: 472, height: 269)");

    const shouldLockContentSize = sourceBetween(
      appDelegateSource,
      "private func shouldLockContentSize(modal: String) -> Bool",
      "private func minimumContentSize(for modal: String?) -> CGSize",
    );
    expect(shouldLockContentSize).toContain(
      'modal == "previousSessions" || modal == "renameSession" || modal == "delayedSend"',
    );

    const minimumContentSize = sourceBetween(
      appDelegateSource,
      "private func minimumContentSize(for modal: String?) -> CGSize",
      "private func appModalStyleMask(for modal: String) -> NSWindow.StyleMask",
    );
    expect(minimumContentSize).toContain('case "delayedSend":');
    expect(minimumContentSize).toContain("return CGSize(width: 472, height: 269)");
  });

  test("keeps Delayed Send duration input to hours and minutes", () => {
    /*
    CDXC:DelayedSend 2026-06-16-17:57:
    Delayed Send should expose only hours and minutes. The modal must not render a seconds input, active timers should prefill by rounding up to whole minutes, and native must reject sub-minute or non-whole-minute bridge requests.
    */
    expect(delayedSendModalSource).toContain('aria-label="Hours"');
    expect(delayedSendModalSource).toContain('aria-label="Minutes"');
    expect(delayedSendModalSource).not.toContain('aria-label="Seconds"');
    expect(delayedSendModalSource).not.toContain("setSeconds");
    expect(delayedSendModalSource).toContain("Enter a delay between 1 minute and 24 days.");
    expect(delayedSendModalSource).toContain("Math.ceil(delayMs / MINUTE_MS)");

    const durationGrid = sourceBetween(
      modalStylesSource,
      ".delayed-send-duration-grid {",
      ".session-rename-form",
    );
    expect(durationGrid).toContain("grid-template-columns: repeat(2, minmax(0, 1fr));");

    const delayedSendValidation = sourceBetween(
      nativeSidebarSource,
      "function scheduleDelayedSend(",
      "function cancelDelayedSend(",
    );
    expect(delayedSendValidation).toContain("DELAYED_SEND_MIN_DELAY_MS");
    expect(delayedSendValidation).toContain("Number.isInteger(delayMs / DELAYED_SEND_MIN_DELAY_MS)");
    expect(delayedSendValidation).toContain("Choose a Delayed Send timer between 1 minute and 24 days.");
  });

  test("sizes the macOS Command Palette to the adjusted native content area", () => {
    /*
    CDXC:CommandPalette 2026-06-12-05:04:
    Command Palette should open 15px narrower on both left and right than the old 720px native frame, while adding 15px of vertical WebView/modal room for the React command list.

    CDXC:CommandPalette 2026-06-12-05:14:
    The extra height must be added at the bottom rather than recentering the modal vertically, so the native placement keeps the previous 520px top edge and extends down to 535px.
    */
    const defaultSize = sourceBetween(
      appDelegateSource,
      "private func defaultSize(for modal: String) -> CGSize",
      "private func constrainedSize(_ size: CGSize, parentWindow: NSWindow) -> CGSize",
    );
    expect(defaultSize).toContain('case "commandPalette":');
    expect(defaultSize).toContain("return CGSize(width: 690, height: 535)");

    const commandPaletteFrame = sourceBetween(
      appDelegateSource,
      "private func commandPaletteContentFrame(size: CGSize, parentWindow: NSWindow) -> CGRect",
      "private func clampFrameToVisibleScreen",
    );
    expect(commandPaletteFrame).toContain("let previousCenteredHeight: CGFloat = 520");
    expect(commandPaletteFrame).toContain(
      "let bottomOnlyHeightIncrease = max(0, size.height - previousCenteredHeight)",
    );

    const constrainedContentFrame = sourceBetween(
      appDelegateSource,
      "private func constrainedContentFrame(",
      "private func shouldLockContentSize(modal: String) -> Bool",
    );
    expect(constrainedContentFrame).toContain('if modal == "commandPalette"');
    expect(constrainedContentFrame).toContain(
      "return commandPaletteContentFrame(size: size, parentWindow: parentWindow)",
    );
  });

  test("keeps the macOS Command Palette React surface inset compact", () => {
    /*
    CDXC:CommandPalette 2026-06-12-05:23:
    Command Palette should reduce the combined search-bar top gap and the whole component's left/right inset by 5px, producing a 3px scoped inset without changing shared CommandInput defaults.
    */
    const commandPaletteStyles = sourceBetween(
      sidebarStylesSource,
      ".ghostex-command-palette-dialog {",
      ".ghostex-command-palette-list {",
    );
    expect(commandPaletteStyles).toContain(
      '.ghostex-command-palette-dialog [data-slot="command"]',
    );
    expect(commandPaletteStyles).toContain("padding: 0 0 4px;");
    expect(commandPaletteStyles).toContain(
      '.ghostex-command-palette-dialog [data-slot="command-input-wrapper"]',
    );
    expect(commandPaletteStyles).toContain("padding: 3px 3px 0;");
    expect(commandPaletteStyles).toContain(
      '.ghostex-command-palette-dialog [data-slot="command-group"]',
    );
    expect(commandPaletteStyles).toContain("padding-left: 3px;");
    expect(commandPaletteStyles).toContain("padding-right: 3px;");
  });

  test("closes compact macOS app modals from outside clicks but switches repeat hotkeys", () => {
    /*
    CDXC:CommandPalette 2026-06-12-05:45:
    The native Command Palette is now a child window. AppKit must close it when the user clicks back into the parent Ghostex window.

    CDXC:CommandPalette 2026-06-15-10:27:
    Repeat command-mode and session-search-mode hotkeys share the visible
    child window and switch modes in-place instead of closing the palette.

    CDXC:AppModals 2026-06-15-13:30:
    Configure Action, Rename Session, and Previous Sessions are compact native
    child-window modals too, so they must close on parent-window mouse-downs
    outside their NSPanel.

    CDXC:HighlightedFeatures 2026-06-16-19:50:
    Highlighted Features should not use the compact outside-click close monitor.
    It closes from its in-modal X, Escape, or native close lifecycle while the
    native backdrop absorbs parent-window clicks.
    */
    const dispatchNativeHotkey = sourceBetween(
      appDelegateSource,
      "private func dispatchNativeHotkey(_ actionId: String)",
      "private func shouldHandleHotkeyWhileWebChromeOwnsFocus",
    );
    expect(dispatchNativeHotkey).toContain(
      "if Self.isCommandPaletteHotkeyActionId(actionId), isCommandPaletteNativeModalOpenOrPending()",
    );
    expect(dispatchNativeHotkey).toContain("nativeHotkeys.commandPaletteModeSwitch");
    expect(dispatchNativeHotkey).not.toContain("commandPaletteHotkeyToggle");
    expect(dispatchNativeHotkey).not.toContain("nativeHotkeys.commandPaletteToggleClose");
    expect(appDelegateSource).toContain(
      'actionId == "openCommandPalette" || actionId == "openSessionSearchPalette"',
    );
    expect(dispatchNativeHotkey).toContain('activeNativeAppModalKind == "commandPalette"');
    expect(dispatchNativeHotkey).toContain(
      'commandPaletteNativeAppModalWindowController?.currentModalKind ?? ""',
    );

    const webChromeHotkeyGuard = sourceBetween(
      appDelegateSource,
      "private func shouldHandleHotkeyWhileWebChromeOwnsFocus(actionId: String) -> Bool",
      "private func logNativeHotkeyDebug",
    );
    expect(webChromeHotkeyGuard).toContain(
      "if Self.isCommandPaletteHotkeyActionId(actionId), isCommandPaletteNativeModalOpenOrPending()",
    );
    expect(webChromeHotkeyGuard).toContain("return true");

    const appModalWindowController = sourceBetween(
      appDelegateSource,
      "private final class AppModalWindowController",
      "private final class TitlebarDropdownPanelController",
    );
    expect(appModalWindowController).toContain("private var outsideEventMonitor: Any?");
    expect(appModalWindowController).toContain("installOutsideEventMonitorIfNeeded(for: modal)");
    expect(appModalWindowController).toContain("guard shouldCloseFromOutsideMouseDown(modal: modal)");
    expect(appModalWindowController).toContain(
      'case "commandPalette", "renameSession", "previousSessions":',
    );
    expect(appModalWindowController).toContain(
      "Do not install the compact\n     outside-click monitor for discoverGhostex.",
    );
    expect(appModalWindowController).toContain(
      "matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]",
    );
    expect(appModalWindowController).toContain("currentModal == modal");
    expect(appModalWindowController).toContain("event.window === panel");
    expect(appModalWindowController).toContain("self.closeFromOutsideMouseDown()");
    expect(appModalWindowController).toContain('onClosed("outsideMouseDown", closedModal)');
    expect(appModalWindowController).toContain("removeOutsideEventMonitor()");
  });

  test("top-aligns Previous Sessions inside its macOS native child window", () => {
    /*
    CDXC:PreviousSessions 2026-06-17-12:02:
    The macOS Previous Sessions modal should keep the title, search field, and rows at the top of the fixed child window instead of centering the shorter result list vertically.
    */
    const previousSessionsRootRule = sourceBetween(
      modalStylesSource,
      ".app-modal-host-native-window-body .confirm-modal-root:has(.previous-sessions-modal) {",
      ".app-modal-host-body .previous-sessions-modal {",
    );
    expect(previousSessionsRootRule).toContain("align-items: start");
    expect(previousSessionsRootRule).toContain("justify-items: center");

    const previousSessionsNativeRule = sourceBetween(
      modalStylesSource,
      ".app-modal-host-native-window-body .previous-sessions-modal {",
      ".app-modal-host-body .pinned-prompts-modal,",
    );
    expect(previousSessionsNativeRule).toContain("margin: 0 auto auto");
    expect(previousSessionsNativeRule).toContain("max-height: calc(100vh - 16px)");
  });

  test("shows an AppKit backdrop behind first-launch and highlighted-feature modals", () => {
    /*
    CDXC:AppModals 2026-06-16-19:50:
    First Time Setup and Highlighted Features should dim and block the full app
    behind them with a native 40% black AppKit child panel, then remove that
    panel when those onboarding modals close.
    */
    expect(appDelegateSource).toContain("private var onboardingAppModalBackdropPanel: AppModalBackdropPanel?");
    expect(appDelegateSource).toContain("private final class AppModalBackdropPanel: NSPanel");
    expect(appDelegateSource).toContain("private final class AppModalBackdropView: NSView");
    expect(appDelegateSource).toContain("override var canBecomeKey: Bool { false }");
    expect(appDelegateSource).toContain("override func mouseDown(with event: NSEvent) {}");

    const backdropHelpers = sourceBetween(
      appDelegateSource,
      "private func shouldShowOnboardingAppModalBackdrop",
      "private func takeFirstLaunchSetupAfterDiscoverClose",
    );
    expect(backdropHelpers).toContain('case "discoverGhostex", "firstLaunchSetup", "tipsAndTricks":');
    expect(backdropHelpers).toContain("NSColor.black.withAlphaComponent(0.4)");
    expect(backdropHelpers).toContain("styleMask: [.borderless, .nonactivatingPanel]");
    expect(backdropHelpers).toContain("window.addChildWindow(panel, ordered: .above)");
    expect(backdropHelpers).toContain("panel.setFrame(window.frame, display: true)");
    expect(backdropHelpers).toContain("removeOnboardingAppModalBackdrop()");
    expect(backdropHelpers).toContain("panel.orderOut(nil)");

    const openNativeModal = sourceBetween(
      appDelegateSource,
      "private func openNativeAppModalWindow(",
      "private func rememberFirstLaunchSetupAfterDiscoverCloseRequest",
    );
    expect(openNativeModal).toContain("updateOnboardingAppModalBackdrop(for: modal)");

    const closeNativeModal = sourceBetween(
      appDelegateSource,
      "private func closeNativeAppModalWindow",
      "private func dispatchNativeAppModalWindowMessage",
    );
    expect(closeNativeModal).toContain("updateOnboardingAppModalBackdrop(for: nil)");

    expect(appDelegateSource).toContain("fileprivate func updateAppModalChildWindowFramesIfNeeded()");
    expect(appDelegateSource).toContain("updateOnboardingAppModalBackdropFrameIfNeeded()");
  });

  test("prewarms and reuses the macOS Command Palette native window", () => {
    /*
    CDXC:CommandPalette 2026-06-13-09:53:
    Command Palette should prewarm a hidden native child-window modal host after launch and reuse that loaded WKWebView for the configured command-palette hotkey, without evicting the separate Monaco prompt-editor prewarm host.
    */
    expect(appDelegateSource).toContain("root.scheduleAppModalPrewarmsAfterLaunch()");
    expect(appDelegateSource).toContain(
      'private static let commandPalettePrewarmRequestId = "ghostex-command-palette-prewarm"',
    );
    expect(appDelegateSource).toContain("private var commandPaletteNativeAppModalWindowController");
    expect(appDelegateSource).toContain("private func prewarmCommandPaletteIfNeeded()");
    expect(appDelegateSource).toContain('"modal": "commandPalette"');
    expect(appDelegateSource).toContain('"prewarm": true');
    expect(appDelegateSource).toContain('"requestId": Self.commandPalettePrewarmRequestId');
    expect(appDelegateSource).toContain("finishCommandPalettePrewarm()");
    expect(appDelegateSource).toContain('hostId: "commandPalette"');

    const appModalWindowController = sourceBetween(
      appDelegateSource,
      "private final class AppModalWindowController",
      "private final class TitlebarDropdownPanelController",
    );
    expect(appModalWindowController).toContain(
      'modal == "floatingPromptEditor" || modal == "commandPalette"',
    );
    expect(appModalWindowController).toContain("func hideReusableModal(");
    expect(appModalWindowController).toContain("func isVisibleModal(_ modal: String) -> Bool");
    expect(appModalWindowController).toContain(
      'window.__ghostex_APP_MODAL_HOST_ID__ = \\(encodedHostId);',
    );
    expect(appModalWindowController).toContain("hideReusableModal(modal: \"commandPalette\"");

    expect(modalHostSource).toContain("activeModalRequestId");
    expect(modalHostSource).toContain("presentedMessage.requestId = activeModalRequestId");
    expect(modalHostSource).toContain("nativeWindowHostId: window.__ghostex_APP_MODAL_HOST_ID__");
  });

  test("does not treat a hidden reusable Command Palette host as already open", () => {
    /*
    CDXC:CommandPalette 2026-06-13-10:31:
    The configured command-palette hotkey must open the palette when the reusable command-palette WKWebView is hidden after prewarm or close. Only a visible command-palette child window should take the repeat-hotkey close path.
    */
    const commandPaletteOpenCheck = sourceBetween(
      appDelegateSource,
      "private func isCommandPaletteNativeModalOpenOrPending() -> Bool",
      "private func shouldHandleHotkeyWhileWebChromeOwnsFocus",
    );
    expect(commandPaletteOpenCheck).toContain('isVisibleModal("commandPalette") == true');
    expect(commandPaletteOpenCheck).toContain('activeNativeAppModalKind == "commandPalette", !isVisible');
    expect(commandPaletteOpenCheck).toContain("activeNativeAppModalKind = nil");
    expect(commandPaletteOpenCheck).toContain("return isVisible");
  });

  test("presents Command Palette through its dedicated native window controller", () => {
    /*
    CDXC:CommandPalette 2026-06-13-10:58:
    Cmd+K should show the Command Palette after React reports presented from the dedicated command-palette modal host. The native bridge must route that acknowledgement by modal kind instead of always presenting the primary app-modal controller.
    */
    const presentedHandler = sourceBetween(
      appDelegateSource,
      'case "presented":',
      'case "close":',
    );
    expect(presentedHandler).toContain("activeNativeAppModalKind = modal");
    expect(presentedHandler).toContain("appModalWindowController(for: modal)?.presentIfCurrent(modal: modal)");
    expect(presentedHandler).not.toContain("nativeAppModalWindowController?.presentIfCurrent(modal: modal)");
  });

  test("makes the native command palette WebView first responder when presented", () => {
    /*
    CDXC:CommandPalette 2026-06-16-19:24:
    The visible macOS command-palette child window must own keyboard focus so
    Cmd+Shift+P opens directly into a typeable search field.
    */
    const presentIfCurrent = sourceBetween(
      appDelegateSource,
      "func presentIfCurrent(modal: String?)",
      "func presentBackgroundPrewarmIfCurrent(modal: String?)",
    );
    expect(presentIfCurrent).toContain("panel.makeKeyAndOrderFront(nil)");
    expect(presentIfCurrent).toContain('if modal == "commandPalette", let webView');
    expect(presentIfCurrent).toContain("panel.makeFirstResponder(webView)");
  });

  test("keeps the prewarmed rich prompt editor mounted for the first Ctrl+G", () => {
    /*
    CDXC:PromptEditor 2026-06-13-11:09:
    Ctrl+G rich prompt editor prewarm must keep the hidden native child-window host and mounted Monaco editor alive so the first real prompt open swaps the buffer and focuses immediately instead of recreating Monaco.

    CDXC:PromptEditor 2026-06-16-10:23:
    Launch-time prompt-editor prewarm must retry when another startup modal or pending child-window presentation blocks the first attempt, so a busy launch cannot leave the first Ctrl+G cold.

    CDXC:PromptEditor 2026-06-16-10:41:
    Prompt-editor prewarm must briefly order the native child window in a transparent non-interactive background state so WebKit, React, and Monaco become live the same way they do after the first real Ctrl+G open.
    */
    expect(appDelegateSource).toContain("private static let floatingPromptEditorPrewarmRetryDelay");
    expect(appDelegateSource).toContain("private var hasPendingFloatingPromptEditorPrewarmRetry");
    expect(appDelegateSource).toContain("private func scheduleFloatingPromptEditorPrewarmRetryIfNeeded");
    expect(appDelegateSource).toContain('event: "native.prewarm.retryScheduled"');
    expect(appDelegateSource).toContain(
      'scheduleFloatingPromptEditorPrewarmRetryIfNeeded(reason: "modalBusy")',
    );
    expect(appDelegateSource).toContain(
      'scheduleFloatingPromptEditorPrewarmRetryIfNeeded(reason: "modalClosed")',
    );

    const finishPrewarm = sourceBetween(
      appDelegateSource,
      "private func finishFloatingPromptEditorPrewarm()",
      "private func cleanupFloatingPromptEditorPrewarmTempFile()",
    );
    expect(finishPrewarm).toContain('sendReactClose: false');

    const presentedHandler = sourceBetween(
      appDelegateSource,
      'case "presented":',
      'case "close":',
    );
    expect(presentedHandler).toContain(
      "appModalWindowController(for: modal)?.presentBackgroundPrewarmIfCurrent(modal: modal)",
    );

    const appModalWindowController = sourceBetween(
      appDelegateSource,
      "private final class AppModalWindowController",
      "private final class TitlebarDropdownPanel",
    );
    expect(appModalWindowController).toContain("func presentBackgroundPrewarmIfCurrent(modal: String?)");
    expect(appModalWindowController).toContain("panel.ignoresMouseEvents = true");
    expect(appModalWindowController).toContain("panel.alphaValue = 0");
    expect(appModalWindowController).toContain("panel.orderFront(nil)");
    expect(appModalWindowController).toContain("private func resetPanelBackgroundPrewarmState()");
    expect(appModalWindowController).toContain("panel?.ignoresMouseEvents = false");
    expect(appModalWindowController).toContain("panel?.alphaValue = 1");

    const monacoRequestEffect = sourceBetween(
      modalHostSource,
      'appendPromptEditorDebugLog("react.monaco.loadStart"',
      "  useEffect(() => {\n    editorRef.current?.layout();",
    );
    expect(monacoRequestEffect).toContain("const existingEditor = editorRef.current");
    expect(monacoRequestEffect).toContain("existingEditor.setValue(editor.initialText)");
    expect(monacoRequestEffect).toContain('appendPromptEditorDebugLog("react.monaco.reusedAndFocused"');
    expect(monacoRequestEffect).toContain("retainedEditor: true");

    const closedLifecycle = sourceBetween(
      modalHostSource,
      'appendPromptEditorDebugLog("react.lifecycle.closed"',
      'appendPromptEditorDebugLog("react.lifecycle.opened"',
    );
    expect(closedLifecycle).toContain("editorRef.current?.dispose()");
    expect(closedLifecycle).toContain("editorRef.current = null");
  });
});
