import type { GxserverSidebarHudCommandButton } from "@/shared/gxserver-protocol";

export interface RunTitlebarActionDetail {
  action: GxserverSidebarHudCommandButton;
  machineId: string;
  projectId: string;
}

declare global {
  interface WindowEventMap {
    "ghostex-web:openCommandPane": CustomEvent;
    "ghostex-web:runTitlebarAction": CustomEvent<RunTitlebarActionDetail>;
  }
}

