import { createRoot } from "react-dom/client";
import "@/packages/core-ui/styles.css";
import { SidebarApp } from "@/packages/core-ui/sidebar-app";
import { dismissSidebarTooltips } from "@/packages/core-ui/app-tooltip";
import { dismissAllSidebarContextMenus } from "@/packages/core-ui/sidebar-context-menu-portal";
import { createGpuiSidebarRuntime } from "./gxserver-runtime";
import "./sidebar.css";

/*
CDXC:GPUISidebarGxserverRuntime 2026-06-24-11:00:
GPUI sidebar production runtime mounts the shared SidebarApp directly and feeds it through the local gxserver message source. Storybook fixtures are not a runtime fallback; missing or invalid Rust/CEF gxserver bootstrap publishes the explicit gxserver-unavailable sidebar state until real presentation data arrives.
*/
document.body.dataset.sidebarTheme = "plain-dark";
// Reuse the native sidebar edge contract so reference-sidebar bleed stays inside the GPUI viewport.
document.body.classList.add("vscode-dark", "native-sidebar-body");

/*
CDXC:GPUISidebarCollapseRestore 2026-07-09:
Sidebar collapse and Show more/less state persist through plain localStorage,
exactly like the macOS sidebar WKWebView: the GPUI sidebar CEF profile has a
persistent cache_path (see cef_app_ui_profile_cache_path in apps/desktop/src/cef/shell.rs),
so no Rust-owned state file or startup seeding bridge is needed.
*/
const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Ghostex sidebar root element was not found.");
}

/*
CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
The sidebar CEF surface is mouse-focus passive on the native side: clicking
its background leaves keyboard focus on the active terminal/pane. The page
does NOT watch DOM focus events for this — Chromium defers focus/blur events
while the document lacks native focus, which is the sidebar's normal state.
The CEF helper's renderer-side focused-node callback reports editable-focus
transitions to Rust instead, which grants/releases native keyboard focus.
*/
/*
CDXC:GPUISidebarPointerTracking 2026-08-02:
An open sidebar context menu closes on Escape, on its own in-sidebar backdrop,
and on window blur — but none of those fire when the click lands on a native
sibling. The sidebar CEF surface is mouse-focus passive, so clicking a terminal
pane or a titlebar button never blurs a browsing context that never held focus,
and the backdrop only covers the sidebar document. Rust's AppKit pointer
observer sees those clicks and calls this, the page-side half of the same
dismissal contract the deprecated macOS host had.
*/
window.ghostexGpui = window.ghostexGpui ?? {};
window.ghostexGpui.dismissSidebarContextMenus = () => {
  dismissAllSidebarContextMenus();
};

/*
CDXC:GPUISidebarPointerTracking 2026-08-20:
A tooltip opens on pointer-enter and closes on pointer-leave, and the sidebar
CEF surface never receives that leave when the pointer crosses into a native
sibling: the tooltip for a session row stayed on screen with the pointer over a
terminal pane. The same AppKit observer that owns `data-native-pointer-inside`
reports the crossing here.

This is a dismissal, not a suppression: `data-sidebar-tooltips-suppressed` is
deliberately drag-only (see CDXC:TooltipLifecycle 2026-06-13-02:30 in
app-tooltip.tsx), because a persistent CSS flag would also keep the *next*
hover from opening a tooltip until something cleared it. Closing the open
tooltips leaves the next pointer-enter free to open a new one.
*/
window.ghostexGpui.dismissSidebarTooltips = () => {
  dismissSidebarTooltips();
};

const gpuiSidebarRuntime = createGpuiSidebarRuntime();
const root = createRoot(rootElement);

root.render(
  <div
    className="native-sidebar-shell gpui-sidebar"
    data-sidebar-mode="combined"
    onPointerDownCapture={() => {
      /*
       * The sidebar CEF surface is a normal native sibling of GPUI titlebar
       * dropdowns. Let any real sidebar pointer-down ask Rust to close the
       * currently open titlebar surface while the original sidebar event keeps
       * its normal target and behavior.
       */
      window.webkit?.messageHandlers?.ghostexNativeHost?.postMessage({
        type: "closeTitlebarDropdownPanel",
      });
    }}
    onContextMenu={(event) => {
      if (!event.defaultPrevented) {
        event.preventDefault();
      }
    }}
  >
    <main className="native-sidebar-main">
      <SidebarApp
        enableProjectCollections={true}
        messageSource={gpuiSidebarRuntime.messageSource}
        nativeHostEventSource={null}
        onStartGxserver={() => gpuiSidebarRuntime.startLocalGxserver()}
        vscode={gpuiSidebarRuntime.vscode}
        windowScopeId="main"
      />
    </main>
  </div>,
);

gpuiSidebarRuntime.start();
