import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const appDelegateSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/AppDelegate.swift", import.meta.url),
  "utf8",
);
const hostProtocolSource = readFileSync(
  new URL("../macos/ghostexHost/Sources/ghostexHost/HostProtocol.swift", import.meta.url),
  "utf8",
);
const nativeSidebarSource = readFileSync(new URL("./native-sidebar.tsx", import.meta.url), "utf8");
const titlebarHostSource = readFileSync(new URL("./titlebar-host.tsx", import.meta.url), "utf8");
const sharedHotkeysSource = readFileSync(
  new URL("../../shared/ghostex-hotkeys.ts", import.meta.url),
  "utf8",
);

describe("native sidebar collapse source", () => {
  test("routes Cmd+B to complete sidebar collapse instead of side switching", () => {
    /*
     * CDXC:SidebarCollapse 2026-06-12-02:23:
     * Cmd+B must bind to toggleSidebarCollapsed across shared hotkeys, native
     * AppKit defaults, and the bridge command while moveSidebar stays unbound
     * by default.
     */
    expect(sharedHotkeysSource).toContain('| "toggleSidebarCollapsed"');
    expect(sharedHotkeysSource).toContain('defaultKey: "cmd+b",\n    description: "Collapse or expand the sidebar."');
    expect(sharedHotkeysSource).toContain('action: { id: "moveSidebar", kind: "moveSidebar" },');
    expect(sharedHotkeysSource).toContain('defaultKey: "",\n    description: "Move the sidebar to the other side."');
    expect(appDelegateSource).toContain('"moveSidebar": ""');
    expect(appDelegateSource).toContain('"toggleSidebarCollapsed": "cmd+b"');
    expect(appDelegateSource).toContain('if actionId == "toggleSidebarCollapsed"');
    expect(hostProtocolSource).toContain("case toggleSidebarCollapsed");
    expect(nativeSidebarSource).toContain('postNative({ type: "toggleSidebarCollapsed" });');
  });

  test("runs Toggle Sidebar from native command palette at the AppKit bridge", () => {
    /*
     * CDXC:SidebarCollapse 2026-06-12-10:57:
     * Command Palette lives in a native app-modal child window, so its Toggle
     * Sidebar action must be handled before the sidebarCommand envelope is
     * forwarded to the sidebar webview.
     */
    expect(appDelegateSource).toContain("private func handleNativeSidebarModalCommand(_ message: Any) -> Bool");
    expect(appDelegateSource).toContain('command["type"] as? String == "runGhostexHotkeyAction"');
    expect(appDelegateSource).toContain('command["actionId"] as? String == "toggleSidebarCollapsed"');
    expect(appDelegateSource).toContain('closeNativeAppModalWindow(reason: "commandPaletteToggleSidebar"');
    expect(appDelegateSource).toContain(
      "if handleNativeSidebarModalCommand(sidebarMessage) {\n        return\n      }",
    );
  });

  test("collapsed layout removes native sidebar chrome frames", () => {
    expect(appDelegateSource).toContain("private var isSidebarCollapsed = false");
    expect(appDelegateSource).toContain("func toggleSidebarCollapsed()");
    expect(appDelegateSource).toContain("setTitlebarSidebarCollapsed(isSidebarCollapsed)");
    expect(appDelegateSource).toContain("sidebarView.isHidden = isSidebarCollapsed");
    expect(appDelegateSource).toContain("divider.isHidden = isSidebarCollapsed");
    expect(appDelegateSource).toContain("divider: .zero");
    expect(appDelegateSource).toContain("sidebar: .zero");
    expect(appDelegateSource).toContain("sidebarWorkareaBorder: .zero");
  });

  test("titlebar exposes a traffic-light-sized sidebar collapse button", () => {
    /*
     * CDXC:SidebarCollapse 2026-06-12-10:57:
     * The React titlebar needs a 14px gray chevron button before the project
     * name and must mirror native collapse state pushed from AppKit.
     *
     * CDXC:TitlebarResources 2026-06-12-20:06:
     * The Resources dropdown also needs a visible expand/collapse button in
     * its right-side header action rail immediately before Sleep Inactive.
     *
     * CDXC:SidebarCollapse 2026-06-12-20:09:
     * The traffic-light-side collapse button previously kept a 14x14px footprint
     * without drawing a border outline around the gray fill.
     *
     * CDXC:SidebarCollapse 2026-06-12-21:03:
     * The traffic-light-side collapse button has a tiny visible dot inside a
     * 33x33px square hit target with invisible space around the painted button.
     *
     * CDXC:SidebarCollapse 2026-06-13-10:53:
     * Keep the expanded hit target inside the titlebar vertically and show a
     * hover tooltip containing only the assigned Toggle Sidebar hotkey.
     *
     * CDXC:SidebarCollapse 2026-06-13-01:00:
     * Move only the visible dot 2px lower while keeping the expanded native hit
     * region fixed.
     *
     * CDXC:SidebarCollapse 2026-06-13-02:59:
     * The hotkey hover label uses the same AppTooltip wrapper as sidebar
     * buttons, with only titlebar-local placement around that shared component.
     *
     * CDXC:SidebarCollapse 2026-06-13-09:18:
     * The visible collapse dot is white and the chevron icon inside it is
     * almost black, without changing the expanded invisible hit target.
     *
     * CDXC:SidebarCollapse 2026-06-13-09:22:
     * Visual review changed the dot to #4699d9 with a white chevron, keeping
     * the same dimensions and invisible hit target.
     *
     * CDXC:SidebarCollapse 2026-06-13-09:24:
     * The visible dot is 14x14px to match the macOS traffic-light buttons while
     * the invisible 33x33px hit target stays unchanged.
     *
     * CDXC:SidebarCollapse 2026-06-13-10:57:
     * The visible collapse dot should paint #313131 when the macOS window is
     * not focused, while preserving the 14x14px visible size and 33x33px hit
     * target.
     *
     * CDXC:SidebarCollapse 2026-06-13-11:05:
     * The chevron direction should flip when the sidebar is placed on the right
     * side, so the icon keeps pointing toward the current collapse or expand
     * motion.
     *
     * CDXC:TitlebarResources 2026-06-12-23:33:
     * The Resources header button collapses and expands individual expandable
     * resource items, not Projects, Browser Tabs, or Orphaned / Detached
     * sections, and it does not show Toggle Sidebar hotkey tooltip content.
     *
     * CDXC:TitlebarResources 2026-06-13-01:54:
     * The Resources bulk item button uses the same diagonal-arrow icons as the
     * sidebar Projects Collapse All / Expand Previous control.
     *
     * CDXC:TitlebarResources 2026-06-13-02:02:
     * The Resources modal collapses all expandable item rows on every open and
     * shows the expand action when all item targets are already collapsed.
     */
    expect(titlebarHostSource).toContain("sidebarCollapsed: boolean;");
    expect(titlebarHostSource).toContain("sidebarSide: SidebarSide;");
    expect(titlebarHostSource).toContain("type SidebarSide");
    expect(titlebarHostSource).toContain("toggleSidebarHotkeyLabel: string;");
    expect(titlebarHostSource).toContain("formatSidebarHotkeyLabel(settings.hotkeys.toggleSidebarCollapsed)");
    expect(titlebarHostSource).toContain(
      'sidebarSide: bootstrap.sidebarSide === "right" ? "right" : settings.sidebarSide',
    );
    expect(titlebarHostSource).toContain("sidebarSide: state.sidebarSide ?? current.sidebarSide");
    expect(titlebarHostSource).toContain("__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__?: boolean;");
    expect(titlebarHostSource).toContain("setWindowFocused: (isFocused: boolean) => void;");
    expect(titlebarHostSource).toContain("function setTitlebarWindowFocused(isFocused: boolean): void");
    expect(titlebarHostSource).toContain('document.body.dataset.windowFocused = isFocused ? "true" : "false";');
    expect(titlebarHostSource).toContain('| { type: "toggleSidebarCollapsed" }');
    expect(titlebarHostSource).toContain('className="titlebar-sidebar-collapse-button"');
    expect(titlebarHostSource).toContain('className="titlebar-sidebar-collapse-button-visual"');
    expect(titlebarHostSource).toContain('className="titlebar-resources-collapse-all-button"');
    expect(titlebarHostSource).toContain('onClick={() => postNative({ type: "toggleSidebarCollapsed" })}');
    expect(titlebarHostSource).toContain(
      "onSetResourceItemsCollapsed(resourceItemCollapseTargets, !allResourceItemsCollapsed)",
    );
    expect(titlebarHostSource).toContain("createResourceItemCollapseTargets(allBundles)");
    expect(titlebarHostSource).toContain("createResourceItemCollapseTarget(bundle)");
    expect(titlebarHostSource).toContain("isResourceItemCollapsed(target, collapsedKeys)");
    expect(titlebarHostSource).toContain("collapsedWhenKeyPresent");
    expect(titlebarHostSource).toContain("resourcesOpenCollapseSeededRef");
    expect(titlebarHostSource).toContain("useLayoutEffect(() => {");
    expect(titlebarHostSource).toContain("createResourceViewItemCollapseTargets(resourceViews)");
    expect(titlebarHostSource).toContain(
      "applyResourceItemCollapsedState(current, resourceItemCollapseTargets, true)",
    );
    expect(titlebarHostSource).toContain("resourceItemCollapseTargets.every");
    expect(titlebarHostSource).toContain('? "Expand all resource items"');
    expect(titlebarHostSource).toContain(': "Collapse all resource items"');
    expect(titlebarHostSource).toContain("IconArrowsDiagonalMinimize");
    expect(titlebarHostSource).toContain("IconArrowsDiagonal2");
    expect(titlebarHostSource).not.toContain("RESOURCES_MENU_FIRST_OPEN_STORAGE_KEY");
    expect(titlebarHostSource).not.toContain("RESOURCE_BROWSER_SECTION_KEY");
    expect(titlebarHostSource).not.toContain("RESOURCE_ORPHANED_SECTION_KEY");
    expect(titlebarHostSource).not.toContain("createResourceAreaCollapseKeys");
    expect(titlebarHostSource).not.toContain("collapseKey={");
    expect(titlebarHostSource).toContain("const sidebarCollapseChevronPointsRight =");
    expect(titlebarHostSource).toContain(
      'projectState.sidebarSide === "right" ? !projectState.sidebarCollapsed : projectState.sidebarCollapsed',
    );
    expect(titlebarHostSource).toContain("sidebarCollapseChevronPointsRight ? (");
    expect(titlebarHostSource).toContain("<IconChevronRight");
    expect(titlebarHostSource).toContain("<IconChevronLeft");
    expect(titlebarHostSource).toContain("<IconChevronDown");
    expect(titlebarHostSource).toContain("size={10}");
    expect(titlebarHostSource).toContain("size={13}");
    expect(titlebarHostSource).toContain("flex: 0 0 33px;");
    expect(titlebarHostSource).toContain("border: 0 !important;");
    expect(titlebarHostSource).toContain("flex: 0 0 24px;");
    expect(titlebarHostSource).toContain("height: 33px !important;");
    expect(titlebarHostSource).toContain("height: 14px;");
    expect(titlebarHostSource).toContain("background: #4699d9;");
    expect(titlebarHostSource).toContain("background: #5aa7e1;");
    expect(titlebarHostSource).toContain('body[data-window-focused="false"] .titlebar-sidebar-collapse-button-visual');
    expect(titlebarHostSource).toContain("background: #313131;");
    expect(titlebarHostSource).toContain("color: #ffffff !important;");
    expect(titlebarHostSource).toContain("height: 24px;");
    expect(titlebarHostSource).toContain("margin: 0 0 0 -9px;");
    expect(titlebarHostSource).toContain("width: 33px !important;");
    expect(titlebarHostSource).toContain("width: 14px;");
    expect(titlebarHostSource).toContain("import { AppTooltip, TooltipProvider } from");
    expect(titlebarHostSource).toContain("function TitlebarAppTooltip");
    expect(titlebarHostSource).toContain("setWindowFocused: setTitlebarWindowFocused");
    expect(titlebarHostSource).toContain("content={projectState.toggleSidebarHotkeyLabel}");
    expect(titlebarHostSource).toContain('side="right"');
    expect(titlebarHostSource).not.toContain(
      "data-tooltip={projectState.toggleSidebarHotkeyLabel || undefined}",
    );
    expect(titlebarHostSource).not.toContain(".titlebar-sidebar-collapse-button[data-tooltip]::after");
    expect(titlebarHostSource).not.toContain("content: attr(data-tooltip);");
    expect(titlebarHostSource).not.toContain(
      '<TooltipContent side="bottom">{projectState.toggleSidebarHotkeyLabel}</TooltipContent>',
    );
    expect(titlebarHostSource).toContain("height: 10px;");
    expect(titlebarHostSource).toContain("width: 10px;");
    expect(titlebarHostSource).toContain("transform: translateY(2px);");
    expect(titlebarHostSource).toContain("margin-left: 0;");
    expect(appDelegateSource).toContain('"sidebarCollapsed": isSidebarCollapsed');
    expect(appDelegateSource).toContain('"sidebarSide": sidebarSide.rawValue');
    expect(appDelegateSource).toContain("setTitlebarSidebarSide(side)");
    expect(appDelegateSource).toContain("private func setTitlebarSidebarCollapsed(_ collapsed: Bool)");
    expect(appDelegateSource).toContain(String.raw`let json = "{\"sidebarCollapsed\":\(collapsedLiteral)}"`);
    expect(appDelegateSource).toContain("private func setTitlebarSidebarSide(_ side: SidebarSide)");
    expect(appDelegateSource).toContain(String.raw`let json = "{\"sidebarSide\":\"\(side.rawValue)\"}"`);
    expect(appDelegateSource).toContain("final class ReactTitlebarChromeView: NSView, WKNavigationDelegate");
    expect(appDelegateSource).toContain("private var nativeWindowFocused: Bool?");
    expect(appDelegateSource).toContain("webView.navigationDelegate = self");
    expect(appDelegateSource).toContain("func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!)");
    expect(appDelegateSource).toContain("window.__ghostex_PENDING_TITLEBAR_WINDOW_FOCUSED__ = focused;");
    expect(appDelegateSource).toContain("window.__ghostex_TITLEBAR__?.setWindowFocused?.(focused);");
    expect(appDelegateSource).toContain("self?.setWindowFocused(true)");
    expect(appDelegateSource).toContain("self?.setWindowFocused(false)");
  });
});
