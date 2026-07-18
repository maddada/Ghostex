import { createRoot } from "react-dom/client";
import "../../sidebar/styles.css";
import { SidebarApp } from "../../sidebar/sidebar-app";
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
persistent cache_path (see cef_app_ui_profile_cache_path in gpui/src/cef/shell.rs),
so no Rust-owned state file or startup seeding bridge is needed.
*/
const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Ghostex sidebar root element was not found.");
}

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
      />
    </main>
  </div>,
);

gpuiSidebarRuntime.start();
