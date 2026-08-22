import { installKanbanCefBridge } from "./project-workarea-cef-bridge";
import "@/packages/core-ui/styles/shadcn.generated.css";

installKanbanCefBridge();

await import("../views/tasks-placeholder");
