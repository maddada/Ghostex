import { RecentProjectsModal } from "@/packages/core-ui/recent-projects-modal";
import type { ExtensionToSidebarMessage } from "@/packages/shared/session-grid-contract";
import { useEffect, useState } from "react";
import type { OpenRecentProjectsModalDetail } from "./action-events";
import type { WebSidebarRuntime } from "../sidebar-runtime/sidebar-runtime";

export function RecentProjectsModalHost({
  runtime,
}: {
  runtime: WebSidebarRuntime;
}) {
  const [modalState, setModalState] =
    useState<OpenRecentProjectsModalDetail>();

  useEffect(() => {
    const openModal = (
      event: WindowEventMap["ghostex-web:openRecentProjectsModal"],
    ) => setModalState(event.detail);
    const closeModal = () => setModalState(undefined);
    const forwardRuntimeMessage = (event: Event) => {
      if (!(event instanceof MessageEvent)) {
        return;
      }
      const message = event.data as ExtensionToSidebarMessage | undefined;
      if (message?.type === "recentProjectsResult") {
        window.dispatchEvent(new MessageEvent("message", { data: message }));
      }
    };

    window.addEventListener("ghostex-web:openRecentProjectsModal", openModal);
    window.addEventListener("ghostex-web:closeAppModal", closeModal);
    runtime.messageSource.addEventListener("message", forwardRuntimeMessage);
    return () => {
      window.removeEventListener("ghostex-web:openRecentProjectsModal", openModal);
      window.removeEventListener("ghostex-web:closeAppModal", closeModal);
      runtime.messageSource.removeEventListener("message", forwardRuntimeMessage);
    };
  }, [runtime]);

  return (
    <RecentProjectsModal
      isOpen={modalState !== undefined}
      machineId={modalState?.machineId}
      machineName={modalState?.machineName}
      onClose={() => setModalState(undefined)}
      vscode={runtime.vscode}
    />
  );
}
