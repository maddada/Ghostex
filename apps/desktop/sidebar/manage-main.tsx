import { installManageCefBridge } from "./project-workarea-cef-bridge";
import "@/packages/core-ui/styles/shadcn.generated.css";
import "@/packages/core-ui/styles/theme.css";

installManageCefBridge();

await import("../views/manage");
