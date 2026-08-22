import { createRoot } from "react-dom/client";
import "@/packages/core-ui/styles.css";
import { App } from "./titlebar/app";
import { initialTitlebarDropdownPanelKind } from "./titlebar/constants";
import { TITLEBAR_STYLES } from "./titlebar/styles";

export { GhostexTitlebarHost } from "./titlebar/app";

document.body.style.margin = "0";
document.documentElement.style.margin = "0";
document.documentElement.style.padding = "0";
document.body.style.background = "transparent";
document.body.style.overflow = "hidden";
document.body.style.padding = "0";
if (initialTitlebarDropdownPanelKind) {
  document.documentElement.dataset.titlebarDropdownPanel = "true";
  document.documentElement.style.height = "100%";
  document.documentElement.style.overflow = "hidden";
  document.documentElement.style.width = "100%";
  document.body.dataset.titlebarDropdownPanel = "true";
  document.body.style.display = "block";
  document.body.style.height = "100%";
  document.body.style.width = "100%";
}
const styleElement = document.createElement("style");
styleElement.textContent = TITLEBAR_STYLES;
document.head.append(styleElement);

const titlebarRootElement = document.getElementById("root");
if (titlebarRootElement && initialTitlebarDropdownPanelKind) {
  titlebarRootElement.dataset.titlebarDropdownPanel = "true";
  titlebarRootElement.style.display = "block";
  titlebarRootElement.style.height = "100%";
  titlebarRootElement.style.margin = "0";
  titlebarRootElement.style.overflow = "hidden";
  titlebarRootElement.style.padding = "0";
  titlebarRootElement.style.width = "100%";
}
if (titlebarRootElement && titlebarRootElement.dataset.ghostexTitlebar !== "false") {
  createRoot(titlebarRootElement).render(<App />);
}
