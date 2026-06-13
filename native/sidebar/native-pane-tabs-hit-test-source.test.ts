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

function sourceSection(source: string, startNeedle: string, endNeedle: string, fromIndex = 0): string {
  const startIndex = source.indexOf(startNeedle, fromIndex);
  expect(startIndex).toBeGreaterThan(-1);
  const endIndex = source.indexOf(endNeedle, startIndex + startNeedle.length);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("native pane tab titlebar hit testing", () => {
  test("routes workspace pane titlebars from the root before embedded siblings", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-06:33:
     * Split-pane tab bars can be visible while an embedded sibling wins root
     * hit testing first. The app root must ask the mounted workspace titlebar
     * resolver before ordinary AppKit traversal so each visible native tab,
     * Close button, and tab-bar action owns its click.
     */
    const rootHitTestIndex = appDelegateSource.indexOf(
      "override func hitTest(_ point: NSPoint) -> NSView?",
      appDelegateSource.indexOf("final class ghostexRootView"),
    );
    const resizePrepassIndex = appDelegateSource.indexOf(
      "if let resizeHandleHitView = workspaceResizeHandleHitView(at: point)",
      rootHitTestIndex,
    );
    const titlebarPrepassIndex = appDelegateSource.indexOf(
      "if let paneTitleBarHitView = workspacePaneTitleBarHitView(at: point)",
      rootHitTestIndex,
    );
    const superHitIndex = appDelegateSource.indexOf("return super.hitTest(point)", rootHitTestIndex);
    const titlebarHelperIndex = appDelegateSource.indexOf(
      "private func workspacePaneTitleBarHitView(at point: NSPoint)",
      rootHitTestIndex,
    );
    const titlebarHelperSource = appDelegateSource.slice(
      titlebarHelperIndex,
      appDelegateSource.indexOf("private func rootLayoutFrames", titlebarHelperIndex),
    );

    expect(rootHitTestIndex).toBeGreaterThan(-1);
    expect(resizePrepassIndex).toBeGreaterThan(rootHitTestIndex);
    expect(titlebarPrepassIndex).toBeGreaterThan(resizePrepassIndex);
    expect(titlebarPrepassIndex).toBeLessThan(superHitIndex);
    expect(titlebarHelperSource).toContain("workspaceView.paneTitleBarHitView(at: workspacePoint)");
    expect(titlebarHelperSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.root.hitTest.titleBarPrepass"');
  });

  test("routes right-side sidebar content before workspace prepasses", () => {
    /*
     * CDXC:SidebarPlacement 2026-06-13-08:14:
     * Right-side sidebar controls must remain clickable after AppKit moves the
     * sidebar webview to the trailing edge. The root hit-test path should ask
     * visible sidebar content first, but only after the sidebar webview confirms
     * the point is outside its native resize-divider exclusion band.
     */
    const rootHitTestIndex = appDelegateSource.indexOf(
      "override func hitTest(_ point: NSPoint) -> NSView?",
      appDelegateSource.indexOf("final class ghostexRootView"),
    );
    const sidebarPrepassIndex = appDelegateSource.indexOf(
      "if let sidebarHitView = sidebarContentHitView(at: point)",
      rootHitTestIndex,
    );
    const resizePrepassIndex = appDelegateSource.indexOf(
      "if let resizeHandleHitView = workspaceResizeHandleHitView(at: point)",
      rootHitTestIndex,
    );
    const titlebarPrepassIndex = appDelegateSource.indexOf(
      "if let paneTitleBarHitView = workspacePaneTitleBarHitView(at: point)",
      rootHitTestIndex,
    );
    const sidebarHelperSource = sourceSection(
      appDelegateSource,
      "private func sidebarContentHitView(at point: NSPoint)",
      "private func workspaceResizeHandleHitView",
      rootHitTestIndex,
    );
    const sidebarWebViewSource = sourceSection(
      appDelegateSource,
      "final class SidebarWebView: WKWebView",
      "final class SidebarModalBackdropView",
    );

    expect(rootHitTestIndex).toBeGreaterThan(-1);
    expect(sidebarPrepassIndex).toBeGreaterThan(rootHitTestIndex);
    expect(sidebarPrepassIndex).toBeLessThan(resizePrepassIndex);
    expect(sidebarPrepassIndex).toBeLessThan(titlebarPrepassIndex);
    expect(sidebarHelperSource).toContain("!isSidebarCollapsed");
    expect(sidebarHelperSource).toContain("!sidebarView.isHidden");
    expect(sidebarHelperSource).toContain("sidebarView.frame.contains(point)");
    expect(sidebarHelperSource).toContain("let sidebarPoint = convert(point, to: sidebarView)");
    expect(sidebarHelperSource).toContain("sidebarView.containsInteractiveHitPoint(sidebarPoint)");
    expect(sidebarHelperSource).toContain("return sidebarView.hitTest(sidebarPoint) ?? sidebarView");
    expect(sidebarWebViewSource).toContain("func containsInteractiveHitPoint(_ point: NSPoint) -> Bool");
    expect(sidebarWebViewSource).toContain("isInteractivePoint(point)");
    expect(sidebarWebViewSource).toContain("case .right:");
    expect(sidebarWebViewSource).toContain("return point.x > bounds.minX + excludedWidth");
  });

  test("routes pane titlebar controls from the window before React titlebar dispatch", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-07:11:
     * The GitHub project tab Add button can be in a top workspace band where
     * AppKit sends the mouse stream through NSWindow chrome handling before root
     * hit testing. Window-boundary routing must ask the workspace's mounted pane
     * titlebars first, then fall through to React titlebar controls only on miss.
     */
    const routeIndex = appDelegateSource.indexOf("func routeTitlebarMouseEventFromWindow(_ event: NSEvent) -> Bool");
    const routeSource = appDelegateSource.slice(
      routeIndex,
      appDelegateSource.indexOf("func showTitlebarDropdownPanel", routeIndex),
    );
    const workspaceRouteIndex = routeSource.indexOf("routeWorkspacePaneTitleBarMouseEventFromWindow(event)");
    const reactRouteIndex = routeSource.indexOf("titlebarChromeView.routeWindowMouseEvent(event)");
    const workspaceHelperIndex = appDelegateSource.indexOf(
      "private func routeWorkspacePaneTitleBarMouseEventFromWindow(_ event: NSEvent) -> Bool",
      routeIndex,
    );
    const workspaceHelperSource = appDelegateSource.slice(
      workspaceHelperIndex,
      appDelegateSource.indexOf("private static func isWorkspacePaneTitleBarWindowMouseEvent", workspaceHelperIndex),
    );
    const eventGuardIndex = appDelegateSource.indexOf(
      "private static func isWorkspacePaneTitleBarWindowMouseEvent",
      workspaceHelperIndex,
    );
    const eventGuardSource = appDelegateSource.slice(
      eventGuardIndex,
      appDelegateSource.indexOf("private static func workspacePaneTitleBarWindowMouseEventTypeName", eventGuardIndex),
    );

    expect(routeIndex).toBeGreaterThan(-1);
    expect(workspaceRouteIndex).toBeGreaterThan(-1);
    expect(reactRouteIndex).toBeGreaterThan(workspaceRouteIndex);
    expect(workspaceHelperSource).toContain(
      'workspaceView.routePaneTitleBarMouseEvent(event, at: workspacePoint, source: source)',
    );
    expect(workspaceHelperSource).toContain('let source = "windowPaneTitleBarPrepass"');
    expect(workspaceHelperSource).toContain(
      'NativePaneTabDragReproLog.append(event: "nativePaneTabs.root.windowMouseEvent.titleBarPrepass"',
    );
    expect(eventGuardSource).toContain("case .leftMouseDown, .leftMouseDragged, .leftMouseUp:");
  });

  test("routes workspace pane titlebars before embedded child hit testing", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-04:28:
     * The 04:18 repro showed top-band mouseDown/mouseUp reaching the pane
     * container instead of tab buttons. Workspace hit testing must check actual
     * visible pane titlebars in z-order before Ghostty, CEF, or WKWebView.
     */
    const workspaceHitTestIndex = terminalWorkspaceSource.indexOf(
      "override func hitTest(_ point: NSPoint) -> NSView?",
      terminalWorkspaceSource.indexOf("private func paneResizeHandleFrame"),
    );
    const titleBarRouteIndex = terminalWorkspaceSource.indexOf(
      "if let titleBarHitView = paneTitleBarHitView(at: point)",
      workspaceHitTestIndex,
    );
    const superHitIndex = terminalWorkspaceSource.indexOf("return super.hitTest(point)", workspaceHitTestIndex);
    const titleBarHelperIndex = terminalWorkspaceSource.indexOf("func paneTitleBarHitView(at point: NSPoint)");
    const helperSource = terminalWorkspaceSource.slice(
      titleBarHelperIndex,
      terminalWorkspaceSource.indexOf("func setNativeChromeInteractivitySuppressed", titleBarHelperIndex),
    );

    expect(workspaceHitTestIndex).toBeGreaterThan(-1);
    expect(titleBarRouteIndex).toBeGreaterThan(workspaceHitTestIndex);
    expect(titleBarRouteIndex).toBeLessThan(superHitIndex);
    expect(helperSource).toContain("private func paneTitleBarEventTarget(at point: NSPoint)");
    expect(helperSource).toContain("guard let target = paneTitleBarEventTarget(at: point)");
    expect(helperSource).toContain("for view in subviews.reversed()");
    expect(helperSource).toContain("if let titleBarView = view as? TerminalSessionTitleBarView");
    expect(helperSource).toContain("directPaneTitleBarEventTarget(titleBarView, at: point)");
    expect(helperSource).toContain("let containerPoint = convert(point, to: containerView)");
    expect(helperSource).toContain("titleBarView.frame.contains(containerPoint)");
    expect(helperSource).toContain("return target.titleBarView.hitTest(target.titleBarPoint) ?? target.titleBarView");
  });

  test("includes directly mounted GitHub project titlebars in workspace prepass", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-07:35:
     * GitHub project tab strips are mounted directly under TerminalWorkspaceView,
     * not inside TerminalPaneLeafContainerView. The workspace and NSWindow
     * prepasses must resolve those direct titlebars by their current AppKit bounds
     * so the visible Git tab Add button owns its click.
     */
    const helperIndex = terminalWorkspaceSource.indexOf("private func paneTitleBarEventTarget(at point: NSPoint)");
    const helperEndIndex = terminalWorkspaceSource.indexOf("func setNativeChromeInteractivitySuppressed", helperIndex);
    const helperSource = terminalWorkspaceSource.slice(helperIndex, helperEndIndex);
    const projectEditorIndex = terminalWorkspaceSource.indexOf("func createProjectEditorPane");
    const projectEditorSource = terminalWorkspaceSource.slice(
      projectEditorIndex,
      terminalWorkspaceSource.indexOf("private func makeProjectEditorBrowserTab", projectEditorIndex),
    );

    expect(helperIndex).toBeGreaterThan(-1);
    expect(projectEditorIndex).toBeGreaterThan(-1);
    expect(projectEditorSource).toContain("view.setDebugContext(ownerSessionId: command.projectId, paneKind: \"projectEditorGit\")");
    expect(projectEditorSource).toContain("addSubview(titleBarView)");
    expect(helperSource).toContain("private func directPaneTitleBarEventTarget(");
    expect(helperSource).toContain("let titleBarPoint = convert(point, to: titleBarView)");
    expect(helperSource).toContain("titleBarView.bounds.contains(titleBarPoint)");
    expect(helperSource).toContain("return PaneTitleBarEventTarget(");
    expect(helperSource.indexOf("if let titleBarView = view as? TerminalSessionTitleBarView")).toBeLessThan(
      helperSource.indexOf("guard let containerView = view as? TerminalPaneLeafContainerView"),
    );
  });

  test("keeps window-routed titlebar mouse streams on the same pane titlebar", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-07:11:
     * A window-boundary pane-titlebar mouseDown must keep later drag/up events on
     * that same titlebar, even if tab addition, close hover, or pointer movement
     * changes normal AppKit hit testing before mouseUp.
     */
    const workspaceRouteIndex = terminalWorkspaceSource.indexOf(
      "func routePaneTitleBarMouseEvent(_ event: NSEvent, at point: NSPoint, source: String) -> Bool",
    );
    const workspaceRouteSource = terminalWorkspaceSource.slice(
      workspaceRouteIndex,
      terminalWorkspaceSource.indexOf("private func paneTitleBarEventTarget", workspaceRouteIndex),
    );
    const titleBarIndex = terminalWorkspaceSource.indexOf("private final class TerminalSessionTitleBarView");
    const titleBarSource = terminalWorkspaceSource.slice(
      titleBarIndex,
      terminalWorkspaceSource.indexOf("override func mouseDragged(with event: NSEvent)", titleBarIndex),
    );

    expect(terminalWorkspaceSource).toContain("private weak var windowRoutedPaneTitleBar: TerminalSessionTitleBarView?");
    expect(workspaceRouteIndex).toBeGreaterThan(-1);
    expect(workspaceRouteSource).toContain("case .leftMouseDown:");
    expect(workspaceRouteSource).toContain("windowRoutedPaneTitleBar = target.titleBarView");
    expect(workspaceRouteSource).toContain("case .leftMouseDragged:");
    expect(workspaceRouteSource).toContain("guard let titleBarView = windowRoutedPaneTitleBar");
    expect(workspaceRouteSource).toContain("case .leftMouseUp:");
    expect(workspaceRouteSource).toContain("windowRoutedPaneTitleBar = nil");
    expect(workspaceRouteSource).toContain("routeWindowTitleBarMouseEvent(");
    expect(titleBarSource).toContain("fileprivate func routeWindowTitleBarMouseEvent(");
    expect(titleBarSource).toContain("handleReroutedTitleBarMouseDown(");
    expect(titleBarSource).toContain("handleReroutedTitleBarMouseDragged(");
    expect(titleBarSource).toContain("handleReroutedTitleBarMouseUp(");
    expect(titleBarSource).toContain("pendingReroutedTitleBarTarget = nil");
  });

  test("routes normal pane titlebar bands before embedded content hit testing", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-04:08:
     * Normal workspace panes must use the same simple ownership rule as popped
     * out panes: points inside the titlebar band go to the titlebar first, before
     * Ghostty, CEF, or WKWebView content can win mouseDown/mouseUp.
     */
    const containerIndex = terminalWorkspaceSource.indexOf("private final class TerminalPaneLeafContainerView");
    const hitTestIndex = terminalWorkspaceSource.indexOf("override func hitTest(_ point: NSPoint) -> NSView?", containerIndex);
    const superHitIndex = terminalWorkspaceSource.indexOf("return super.hitTest(point)", hitTestIndex);

    expect(containerIndex).toBeGreaterThan(-1);
    expect(hitTestIndex).toBeGreaterThan(containerIndex);
    expect(superHitIndex).toBeGreaterThan(hitTestIndex);
    expect(terminalWorkspaceSource).toContain("weak var titleBarHitTestView: NSView?");
    expect(terminalWorkspaceSource).toContain("session.containerView.titleBarHitTestView = session.titleBarView");
    expect(terminalWorkspaceSource).toContain("titleBarHitTestView ?? subviews.first { $0 is TerminalSessionTitleBarView }");
    expect(terminalWorkspaceSource).toContain("titleBarView.frame.contains(point)");
    expect(terminalWorkspaceSource).toContain("return titleBarView.hitTest(convert(point, to: titleBarView))");
    expect(terminalWorkspaceSource.indexOf("titleBarView.frame.contains(point)", hitTestIndex)).toBeLessThan(superHitIndex);
  });

  test("returns concrete tab controls and blocks tab-strip child fallback", () => {
    /**
     * CDXC:NativeWorkspaceHitTesting 2026-06-12-02:36:
     * Hover can still update through tracking areas when click hit testing
     * falls through to pane content. Keep pane-tab buttons on explicit
     * titlebar hit ownership so mouseDown, drag, and mouseUp reach the native
     * tab gesture handlers.
     *
     * CDXC:NativePaneTabClicks 2026-06-12-05:41:
     * Scrolled tab strips must not fall back to AppKit child hit testing after
     * explicit tab resolution. The child hierarchy can pick a tab one or two
     * slots to the right, so tab-strip misses return the titlebar itself.
     */
    const tabHitIndex = terminalWorkspaceSource.indexOf("if let tabHit = tabButtonHit(at: point)");
    const tabViewportFallbackIndex = terminalWorkspaceSource.indexOf("if tabViewportFrame.contains(point)", tabHitIndex);
    const superHitIndex = terminalWorkspaceSource.indexOf("if let hitView = super.hitTest(point)");

    expect(tabHitIndex).toBeGreaterThan(-1);
    expect(tabViewportFallbackIndex).toBeGreaterThan(tabHitIndex);
    expect(superHitIndex).toBeGreaterThan(-1);
    expect(tabHitIndex).toBeLessThan(superHitIndex);
    expect(tabViewportFallbackIndex).toBeLessThan(superHitIndex);
    expect(terminalWorkspaceSource).toContain(
      'NativePaneTabDragReproLog.append(event: "nativePaneTabs.titleBar.hitTest.tabButton"',
    );
    expect(terminalWorkspaceSource).toContain("return tabHit.button");
    expect(terminalWorkspaceSource.slice(tabViewportFallbackIndex, superHitIndex)).toContain("return self");
  });

  test("uses explicit scroll-offset tab geometry instead of AppKit child lookup", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-05:22:
     * Each native tab button must own its own click. Titlebar hit testing should
     * resolve the concrete button from its converted visible frame instead of
     * asking the shifted clipped-strip view to rediscover a subview.
     *
     * CDXC:NativePaneTabClicks 2026-06-12-05:41:
     * The 05:33 repro proved AppKit conversion can still hand mouseDown to a
     * tab whose local point is outside its bounds. Resolve tab ownership from
     * the layout scroll offset and each tab button frame directly.
     */
    const helperIndex = terminalWorkspaceSource.indexOf("private func tabButtonHit(at point: NSPoint)");
    const helperEndIndex = terminalWorkspaceSource.indexOf("func containsTab(_ sessionId: String) -> Bool", helperIndex);
    const helperSource = terminalWorkspaceSource.slice(helperIndex, helperEndIndex);

    expect(helperIndex).toBeGreaterThan(-1);
    expect(helperSource).toContain("return visibleTabButtonHit(at: point)");
    expect(helperSource).toContain("private func tabButtonHitRecords(at point: NSPoint)");
    expect(helperSource).toContain("private func visibleTabButtonFrame(for button: TerminalTitleBarTabButton) -> CGRect");
    expect(helperSource).toContain("private func tabContentPoint(forTitleBarPoint point: NSPoint) -> NSPoint");
    expect(helperSource).toContain("point.x - tabViewportFrame.minX + tabScrollOffsetX");
    expect(helperSource).toContain("contentPoint.x - button.frame.minX");
    expect(helperSource).toContain("tabViewportFrame.minX + button.frame.minX - tabScrollOffsetX");
    expect(helperSource).toContain("frameInTitleBar.intersection(tabViewportFrame)");
    expect(helperSource).toContain("visibleFrame.contains(point)");
    expect(helperSource).toContain("button.bounds.contains(localPoint)");
    expect(helperSource).not.toContain("tabClipView.hitTest");
    expect(helperSource).not.toContain("let localPoint = convert(point, to: button)");
  });

  test("prefers each tab button's local hover owner before clipped-strip fallback", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-04:45:
     * When a scrolled native tab strip paints/hover-tracks the intended tab but
     * titlebar click picking selects the tab to its right, click ownership must
     * first follow the tab button's own AppKit tracking-area hover state.
     */
    const buttonIndex = terminalWorkspaceSource.indexOf("private final class TerminalTitleBarTabButton");
    const titleBarIndex = terminalWorkspaceSource.indexOf("private final class TerminalSessionTitleBarView");
    const helperIndex = terminalWorkspaceSource.indexOf("private func tabButtonHit(at point: NSPoint)");
    const helperEndIndex = terminalWorkspaceSource.indexOf("func containsTab(_ sessionId: String) -> Bool", helperIndex);
    const buttonSource = terminalWorkspaceSource.slice(buttonIndex, titleBarIndex);
    const helperSource = terminalWorkspaceSource.slice(helperIndex, helperEndIndex);

    expect(buttonIndex).toBeGreaterThan(-1);
    expect(helperIndex).toBeGreaterThan(-1);
    expect(buttonSource).toContain("fileprivate var isPointerLocallyInside = false");
    expect(buttonSource).toContain("isPointerLocallyInside = isInside");
    expect(buttonSource).toContain("isPointerLocallyInside = false");
    expect(helperSource).toContain("if let hoverHit = locallyHoveredTabButtonHit(at: point)");
    expect(helperSource).toContain("return hoverHit");
    expect(helperSource).toContain("private func locallyHoveredTabButtonHit(at point: NSPoint)");
    expect(helperSource.indexOf("if let hoverHit = locallyHoveredTabButtonHit(at: point)")).toBeLessThan(
      helperSource.indexOf("return visibleTabButtonHit(at: point)"),
    );
    expect(helperSource).toContain("$0.button.isPointerLocallyInside");
    expect(helperSource).toContain("guard hoveredHits.count == 1, let hoverHit = hoveredHits.first");
  });

  test("uses local hover ownership for tab close and fixed tab-bar action buttons", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-04:57:
     * Inline tab Close plus the sticky active-tab chevron, New Terminal, New
     * Browser, and overflow buttons should click the same control that AppKit
     * already reports as locally hovered.
     *
     * CDXC:NativePaneTabClicks 2026-06-12-05:31:
     * Fixed action-button hover must only win while the click point is still
     * inside that button. Stale New Terminal hover must not steal clicks from
     * New Browser, overflow, or the sticky active-tab button to its right.
     */
    const actionButtonIndex = terminalWorkspaceSource.indexOf("private final class TerminalTitleBarActionButton");
    const tabButtonIndex = terminalWorkspaceSource.indexOf("private final class TerminalTitleBarTabButton");
    const titleBarIndex = terminalWorkspaceSource.indexOf("private final class TerminalSessionTitleBarView");
    const hitTestIndex = terminalWorkspaceSource.indexOf("override func hitTest(_ point: NSPoint) -> NSView?", titleBarIndex);
    const collapsedActionIndex = terminalWorkspaceSource.indexOf("if let collapsedActionMenuButton", hitTestIndex);
    const hoverActionIndex = terminalWorkspaceSource.indexOf("if let hoveredTitleBarButton", hitTestIndex);
    const actionButtonSource = terminalWorkspaceSource.slice(actionButtonIndex, tabButtonIndex);
    const tabButtonSource = terminalWorkspaceSource.slice(tabButtonIndex, titleBarIndex);
    const titleBarSource = terminalWorkspaceSource.slice(titleBarIndex, terminalWorkspaceSource.indexOf("private func isEmptyTitleBarDoubleClickPoint", titleBarIndex));

    expect(actionButtonIndex).toBeGreaterThan(-1);
    expect(tabButtonIndex).toBeGreaterThan(-1);
    expect(hitTestIndex).toBeGreaterThan(-1);
    expect(actionButtonSource).toContain("fileprivate var isPointerLocallyInside: Bool");
    expect(tabButtonSource).toContain("fileprivate var locallyHoveredInlineAction: InlineAction?");
    expect(tabButtonSource).toContain("locallyHoveredInlineAction ?? inlineActionAtPoint");
    expect(tabButtonSource).toContain("private var hasValidMouseDown = false");
    expect(tabButtonSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.mouseDown.outsideBounds"');
    expect(tabButtonSource).toContain("guard bounds.contains(point) else");
    expect(tabButtonSource).toContain("guard hasValidMouseDown || bounds.contains(point) else");
    expect(tabButtonSource).toContain("if let titleBar = owningTitleBarView()");
    expect(tabButtonSource).toContain("titleBar.rerouteMisdirectedOwnedTitleBarMouseDown(");
    expect(tabButtonSource).toContain("titleBar.rerouteMisdirectedOwnedTitleBarMouseDragged(");
    expect(tabButtonSource).toContain("titleBar.rerouteMisdirectedOwnedTitleBarMouseUp(");
    expect(tabButtonSource).toContain("TerminalSessionTitleBarView.rerouteMisdirectedTitleBarMouseDown(");
    expect(tabButtonSource).toContain("TerminalSessionTitleBarView.rerouteMisdirectedTitleBarMouseUp(");
    expect(actionButtonSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.actionButton.mouseDown.outsideBounds"');
    expect(actionButtonSource).toContain("if let titleBar = owningTitleBarView()");
    expect(actionButtonSource).toContain("titleBar.rerouteMisdirectedOwnedTitleBarMouseDown(");
    expect(actionButtonSource).toContain("titleBar.rerouteMisdirectedOwnedTitleBarMouseUp(");
    expect(actionButtonSource).toContain("TerminalSessionTitleBarView.rerouteMisdirectedTitleBarMouseDown(");
    expect(hoverActionIndex).toBeGreaterThan(hitTestIndex);
    expect(hoverActionIndex).toBeLessThan(collapsedActionIndex);
    expect(titleBarSource).toContain("private func locallyHoveredTitleBarActionButton(at point: NSPoint)");
    expect(titleBarSource).toContain("stickyActiveTabButton");
    expect(titleBarSource).toContain("tabAddButton");
    expect(titleBarSource).toContain("tabBrowserButton");
    expect(titleBarSource).toContain("actionMenuButton");
    expect(titleBarSource).toContain("$0.isPointerLocallyInside");
    expect(titleBarSource).toContain("convertedTitleBarActionButtonFrame($0).contains(point)");
    expect(titleBarSource).toContain("$0.bounds.contains(convert(point, to: $0))");
    expect(titleBarSource).toContain("guard hoveredButtons.count == 1, let button = hoveredButtons.first");
    expect(titleBarSource).toContain("private func convertedTitleBarActionButtonFrame(_ button: TerminalTitleBarActionButton) -> CGRect");
    expect(titleBarSource).toContain("convert(button.bounds, from: button)");
    expect(titleBarSource).toContain("logFixedTitleBarActionButtonHit(");
    expect(titleBarSource).toContain('buttonKind": fixedTitleBarActionButtonKind(button)');
    expect(tabButtonSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.inlineMouseDown"');
    expect(tabButtonSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.button.inlineMouseUp"');
    expect(terminalWorkspaceSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.actionButton.perform"');
  });

  test("reroutes misdelivered tab and fixed-button events through titlebar geometry", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-05:52:
     * The stale receiver guard must not turn bad AppKit child hit testing into
     * a dead click. Misdirected tab/close/action events need to be resolved
     * once from the visible titlebar under the event's real window point.
     */
    const titleBarIndex = terminalWorkspaceSource.indexOf("private final class TerminalSessionTitleBarView");
    const titleBarSource = terminalWorkspaceSource.slice(
      titleBarIndex,
      terminalWorkspaceSource.indexOf("final class ProjectEditorInitialLoadingOverlayView", titleBarIndex),
    );

    expect(titleBarSource).toContain("fileprivate static func rerouteMisdirectedTitleBarMouseDown(");
    expect(titleBarSource).toContain("fileprivate static func rerouteMisdirectedTitleBarMouseDragged(");
    expect(titleBarSource).toContain("fileprivate static func rerouteMisdirectedTitleBarMouseUp(");
    expect(titleBarSource).toContain("fileprivate func rerouteMisdirectedOwnedTitleBarMouseDown(");
    expect(titleBarSource).toContain("fileprivate func rerouteMisdirectedOwnedTitleBarMouseDragged(");
    expect(titleBarSource).toContain("fileprivate func rerouteMisdirectedOwnedTitleBarMouseUp(");
    expect(titleBarSource).toContain("handleReroutedTitleBarMouseDown(");
    expect(titleBarSource).toContain("handleReroutedTitleBarMouseDragged(");
    expect(titleBarSource).toContain("handleReroutedTitleBarMouseUp(");
    expect(titleBarSource).toContain('source: "titleBarDirect"');
    expect(titleBarSource).toContain("override func mouseDragged(with event: NSEvent)");
    expect(titleBarSource).toContain("override func mouseUp(with event: NSEvent)");
    expect(titleBarSource).toContain("private static func titleBarRerouteCandidate(for event: NSEvent)");
    expect(titleBarSource).toContain("collectTitleBarsForReroute(in: root, into: &titleBars)");
    expect(titleBarSource).toContain("for subview in view.subviews.reversed()");
    expect(titleBarSource).toContain("let point = titleBar.convert(event.locationInWindow, from: nil)");
    expect(titleBarSource).toContain("let target = titleBar.reroutedTitleBarTarget(at: point)");
    expect(titleBarSource).toContain("private var pendingReroutedTitleBarTarget: ReroutedTitleBarTarget?");
    expect(titleBarSource).toContain("private func reroutedTitleBarTarget(at point: NSPoint) -> ReroutedTitleBarTarget?");
    expect(titleBarSource).toContain("return .inlineClose(sessionId: tabHit.button.sessionId)");
    expect(titleBarSource).toContain("return .tab(sessionId: tabHit.button.sessionId)");
    expect(titleBarSource).toContain("return .fixedAction(.newTerminal)");
    expect(titleBarSource).toContain("return .fixedAction(.newBrowser)");
    expect(titleBarSource).toContain("return .fixedAction(.stickyActiveTab)");
    expect(titleBarSource).toContain("return .fixedAction(.overflowMenu)");
    expect(titleBarSource).toContain("onTabMouseDown?(event, sessionId)");
    expect(titleBarSource).toContain("onTabMouseUp?(event, sessionId)");
    expect(titleBarSource).toContain("onTabCloseRequested?(sessionId, .close)");
    expect(titleBarSource).toContain("performReroutedFixedAction(kind, source: \"reroutedMouseDown\")");
    expect(titleBarSource).toContain("performReroutedFixedAction(kind, source: \"reroutedMouseUp\")");
    expect(titleBarSource).toContain('event: "nativePaneTabs.titleBar.reroute.mouseDown"');
    expect(titleBarSource).toContain('event: "nativePaneTabs.titleBar.reroute.mouseUp"');
    expect(titleBarSource).toContain('NativePaneTabDragReproLog.append(event: "nativePaneTabs.titleBar.reroute.miss"');
  });

  test("keeps commands-panel resize rail outside the command tab strip", () => {
    /**
     * CDXC:NativePaneTabClicks 2026-06-12-04:08:
     * The command-panel resize rail is allowed to sit on the panel boundary, but
     * not across the native command tab bar where it can steal clicks from tabs.
     */
    const resizeHandleIndex = terminalWorkspaceSource.indexOf("private func syncCommandsPanelResizeHandle");
    const frameIndex = terminalWorkspaceSource.indexOf("commandsPanelResizeHandleView.frame = CGRect(", resizeHandleIndex);
    const functionEndIndex = terminalWorkspaceSource.indexOf("private func syncPaneResizeHandleViews", resizeHandleIndex);
    const functionSource = terminalWorkspaceSource.slice(resizeHandleIndex, functionEndIndex);

    expect(resizeHandleIndex).toBeGreaterThan(-1);
    expect(frameIndex).toBeGreaterThan(resizeHandleIndex);
    expect(functionSource).toContain("let railY = min(");
    expect(functionSource).toContain("max(commandPanelBounds.maxY, bounds.minY)");
    expect(functionSource).toContain("y: railY");
    expect(functionSource).not.toContain("commandPanelBounds.maxY - railHeight / 2");
  });

  test("releases native resize cursors when rails end, reset, or hide", () => {
    /**
     * CDXC:NativePaneResize 2026-06-13-03:40:
     * Native resize cursors must be owned by visible concrete rails only. End,
     * double-click reset, and hide/collapse paths should refresh from the rail
     * currently under the pointer instead of reasserting resize cursors after
     * the resize gesture or visible hit target has ended.
     */
    const beginProjectResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func beginProjectEditorCompanionResize(with event: NSEvent) -> Bool",
      "@discardableResult\n  private func continueProjectEditorCompanionResize",
    );
    const beginProjectResetSource = beginProjectResizeSource.slice(
      beginProjectResizeSource.indexOf("if event.clickCount >= 2"),
      beginProjectResizeSource.indexOf("let point = convert"),
    );
    const endProjectResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func endProjectEditorCompanionResize(with event: NSEvent) -> Bool",
      "private func keepCommandsPanelAboveWorkspacePanes",
    );
    const beginPaneResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func beginPaneResize(hit: PaneResizeHit, event: NSEvent) -> Bool",
      "@discardableResult\n  private func continuePaneResize",
    );
    const beginPaneResetSource = beginPaneResizeSource.slice(
      beginPaneResizeSource.indexOf("if event.clickCount >= 2"),
      beginPaneResizeSource.indexOf("let currentRatios"),
    );
    const continuePaneResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func continuePaneResize(with event: NSEvent) -> Bool",
      "private func paneResizeCursor(for direction",
    );
    const endPaneResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func endPaneResize(with event: NSEvent) -> Bool",
      "private func resetPaneHeaderInteractionState",
    );
    const beginCommandsResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func beginCommandsPanelResize(with event: NSEvent) -> Bool",
      "private func resetCommandsPanelHeightRatio",
    );
    const beginCommandsResetSource = beginCommandsResizeSource.slice(
      beginCommandsResizeSource.indexOf("if event.clickCount >= 2"),
      beginCommandsResizeSource.indexOf("let point = convert"),
    );
    const endCommandsResizeSource = sourceSection(
      terminalWorkspaceSource,
      "private func endCommandsPanelResize(with event: NSEvent) -> Bool",
      "@discardableResult\n  private func endPaneResize",
    );
    const workspaceHandleIndex = terminalWorkspaceSource.indexOf(
      "final class TerminalWorkspacePaneResizeHandleView: NSView",
    );
    const workspaceHandleSource = sourceSection(
      terminalWorkspaceSource,
      "final class TerminalWorkspacePaneResizeHandleView: NSView",
      "final class TerminalPaneBorderView",
    );
    const workspaceHandleMouseExitedSource = sourceSection(
      workspaceHandleSource,
      "override func mouseExited(with event: NSEvent)",
      "override func cursorUpdate(with event: NSEvent)",
    );
    const workspaceHandleMouseUpSource = sourceSection(
      terminalWorkspaceSource,
      "override func mouseUp(with event: NSEvent)",
      "final class TerminalPaneBorderView",
      workspaceHandleIndex,
    );
    const rootLayoutSource = sourceSection(
      appDelegateSource,
      "override func layout()",
      "private func promoteSidebarChrome",
      appDelegateSource.indexOf("final class ghostexRootView"),
    );
    const sidebarHandleIndex = appDelegateSource.indexOf("final class PaneResizeHandleView: NSView");
    const sidebarHandleSource = sourceSection(
      appDelegateSource,
      "final class PaneResizeHandleView: NSView",
      "extension ghostexRootView: WKNavigationDelegate",
    );
    const sidebarHandleMouseExitedSource = sourceSection(
      sidebarHandleSource,
      "override func mouseExited(with event: NSEvent)",
      "override func resetCursorRects",
    );
    const sidebarHandleMouseDraggedSource = sourceSection(
      sidebarHandleSource,
      "override func mouseDragged(with event: NSEvent)",
      "override func mouseUp(with event: NSEvent)",
    );
    const sidebarHandleMouseUpSource = sourceSection(
      appDelegateSource,
      "override func mouseUp(with event: NSEvent)",
      "extension ghostexRootView: WKNavigationDelegate",
      sidebarHandleIndex,
    );

    expect(terminalWorkspaceSource).toContain("private func refreshResizeCursorForCurrentPointer()");
    expect(terminalWorkspaceSource).toContain("refreshCursorForCurrentPointerIfInside()");
    expect(terminalWorkspaceSource).toContain("needsCursorRefreshBeforeRemoval()");
    expect(terminalWorkspaceSource).toContain("let shouldRefreshCursor = paneResizeHandleViews.contains");
    expect(terminalWorkspaceSource).toContain("commandsPanelResizeHandleView.needsCursorRefreshBeforeRemoval()");
    expect(terminalWorkspaceSource).toContain("projectEditorCompanionResizeHandleView.needsCursorRefreshBeforeRemoval()");
    expect(workspaceHandleSource).toContain("isResizeDragging || isCurrentPointerInsideVisibleHandle()");
    expect(workspaceHandleSource).toContain("private func isCurrentPointerInsideVisibleHandle() -> Bool");
    expect(workspaceHandleMouseExitedSource).toContain("if !isResizeDragging");
    expect(workspaceHandleMouseExitedSource).toContain("refreshCursorForCurrentPointer()");
    expect(workspaceHandleMouseExitedSource).not.toContain("NSCursor.arrow.set()");
    expect(workspaceHandleMouseUpSource).toContain("isResizeDragging = false");
    expect(workspaceHandleMouseUpSource).toContain("refreshCursorForCurrentPointer()");

    expect(beginProjectResetSource).toContain("refreshResizeCursorForCurrentPointer()");
    expect(beginProjectResetSource).not.toContain("NSCursor.resizeLeftRight.set()");
    expect(endProjectResizeSource).toContain("refreshResizeCursorForCurrentPointer()");
    expect(endProjectResizeSource).not.toContain("NSCursor.resizeLeftRight.set()");
    expect(beginPaneResetSource).toContain("refreshResizeCursorForCurrentPointer()");
    expect(beginPaneResetSource).not.toContain("paneResizeCursor(for: hit.direction).set()");
    expect(beginPaneResizeSource).toContain("paneResizeCursor(for: hit.direction).set()");
    expect(continuePaneResizeSource).toContain("paneResizeCursor(for: drag.direction).set()");
    expect(endPaneResizeSource).toContain("refreshResizeCursorForCurrentPointer()");
    expect(endPaneResizeSource).not.toContain("paneResizeCursor(for: drag.direction).set()");
    expect(beginCommandsResetSource).toContain("refreshResizeCursorForCurrentPointer()");
    expect(beginCommandsResetSource).not.toContain("NSCursor.resizeUpDown.set()");
    expect(endCommandsResizeSource).toContain("refreshResizeCursorForCurrentPointer()");
    expect(endCommandsResizeSource).not.toContain("NSCursor.resizeUpDown.set()");

    expect(rootLayoutSource).toContain("divider.needsCursorRefreshBeforeHide()");
    expect(rootLayoutSource).toContain("divider.refreshCursorAfterVisibilityChange()");
    expect(appDelegateSource).not.toContain("addCursorRect(divider.frame, cursor: .resizeLeftRight)");
    expect(sidebarHandleSource).toContain("private var isResizeDragging = false");
    expect(sidebarHandleSource).toContain("private func appendSidebarResizeCursorLog(");
    expect(sidebarHandleSource).toContain("guard NativeDebugLogging.isEnabled");
    expect(sidebarHandleSource).toContain("NativePaneTabDragReproLog.append(event: eventName, details: details)");
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.mouseEntered"');
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.mouseExited"');
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.cursorRefresh"');
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.resetCursorRects"');
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.mouseDown"');
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.mouseDragged"');
    expect(sidebarHandleSource).toContain('"nativeSidebarResize.handle.mouseUp"');
    expect(sidebarHandleSource).toContain('"cursorAction": cursorAction');
    expect(sidebarHandleSource).toContain('"pointerInside": pointerInside ?? isCurrentPointerInsideVisibleHandle()');
    expect(sidebarHandleSource).toContain('"currentWindowPoint"');
    expect(sidebarHandleSource).toContain('"eventWindowPoint"');
    expect(sidebarHandleSource).toContain("if Self.sidebarResizeCursorEventSupportsClickCount(event.type)");
    expect(sidebarHandleSource).toContain("private static func sidebarResizeCursorEventSupportsClickCount");
    expect(sidebarHandleSource).toContain("case .leftMouseDown, .leftMouseDragged, .leftMouseUp,");
    expect(sidebarHandleSource).toContain(".rightMouseDown, .rightMouseDragged, .rightMouseUp,");
    expect(sidebarHandleSource).toContain(".otherMouseDown, .otherMouseDragged, .otherMouseUp:");
    expect(sidebarHandleSource).toContain("func needsCursorRefreshBeforeHide() -> Bool");
    expect(sidebarHandleSource).toContain("func refreshCursorAfterVisibilityChange()");
    expect(sidebarHandleSource).toContain("private func isCurrentPointerInsideVisibleHandle() -> Bool");
    expect(sidebarHandleSource).toContain("addCursorRect(bounds, cursor: .resizeLeftRight)");
    expect(sidebarHandleMouseExitedSource).toContain("if !isResizeDragging");
    expect(sidebarHandleMouseExitedSource).not.toContain("clickCount");
    expect(sidebarHandleMouseDraggedSource).toContain("NSCursor.resizeLeftRight.set()");
    expect(sidebarHandleMouseUpSource).toContain("isResizeDragging = false");
    expect(sidebarHandleMouseUpSource).toContain('refreshCursorForCurrentPointer(reason: "mouseUp", event: event)');
  });
});
