import { installManageCefBridge } from "./project-workarea-cef-bridge";
import "@/sidebar/styles/shadcn.generated.css";
import "@/sidebar/styles/theme.css";

installManageCefBridge();

await import("../views/manage");
