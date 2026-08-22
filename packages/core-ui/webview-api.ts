import type { SidebarToExtensionMessage } from "../shared/session-grid-contract";

export type WebviewApi = {
  postMessage: (message: SidebarToExtensionMessage) => void;
};
