import { createRoot } from 'react-dom/client';
import '@/packages/core-ui/styles/shadcn.generated.css';
import { ProjectBoardApp } from './project-board/project-board-app';
import { PROJECT_BOARD_STYLES } from './project-board/styles';

const styleElement = document.createElement('style');
styleElement.textContent = PROJECT_BOARD_STYLES;
document.head.append(styleElement);

document.addEventListener(
  'contextmenu',
  (event) => {
    /*
     * Kanban and Automate are first-party app views, so Chromium's page menu
     * does not belong in either surface. Preventing the browser default here
     * still lets the event reach Kanban cards, which open their own app menu.
     */
    event.preventDefault();
  },
  true
);

createRoot(document.getElementById('root')!).render(<ProjectBoardApp />);
