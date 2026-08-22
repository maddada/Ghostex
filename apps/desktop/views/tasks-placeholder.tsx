import { createRoot } from "react-dom/client";
import "@/packages/core-ui/styles/shadcn.generated.css";
import { ProjectBoardApp } from "./project-board/project-board-app";
import { PROJECT_BOARD_STYLES } from "./project-board/styles";

const styleElement = document.createElement("style");
styleElement.textContent = PROJECT_BOARD_STYLES;
document.head.append(styleElement);

createRoot(document.getElementById("root")!).render(<ProjectBoardApp />);
