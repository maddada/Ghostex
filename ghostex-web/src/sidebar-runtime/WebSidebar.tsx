import { useEffect, useMemo } from "react";
import { SidebarApp } from "@/sidebar/sidebar-app";
import "@/sidebar/styles.css";
import { createWebSidebarRuntime } from "./sidebar-runtime";

export function WebSidebar() {
  const runtime = useMemo(createWebSidebarRuntime, []);

  useEffect(() => {
    document.body.dataset.sidebarTheme = "plain-dark";
    document.body.classList.add("vscode-dark", "native-sidebar-body");
    runtime.start();
    return () => runtime.stop();
  }, [runtime]);

  return (
    <div className="native-sidebar-shell web-sidebar__shell" data-sidebar-host="web" data-sidebar-mode="combined">
      <main className="native-sidebar-main">
        <SidebarApp
          enableProjectCollections
          messageSource={runtime.messageSource}
          nativeHostEventSource={null}
          vscode={runtime.vscode}
        />
      </main>
    </div>
  );
}
