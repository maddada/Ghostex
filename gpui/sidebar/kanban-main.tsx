import { installKanbanCefBridge } from "./project-workarea-cef-bridge";
import "@/sidebar/styles/shadcn.generated.css";

installKanbanCefBridge();

await import("../../native/sidebar/tasks-placeholder");
