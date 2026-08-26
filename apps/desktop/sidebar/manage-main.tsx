import { installManageCefBridge } from './project-workarea-cef-bridge';
import '@/packages/core-ui/styles/shadcn.generated.css';
import '@/packages/core-ui/styles/theme.css';
/*
 * CDXC:UnifiedAppModal 2026-08-26:
 * The Docs surface renders its dialogs through the shared AppModalShell, so it
 * needs the one `.gx-app-modal` token/slot sheet that owns the app-modal design
 * language. Import it here (not a Docs-local copy) so restyling every app modal
 * still means editing packages/core-ui/styles/modals.css alone.
 */
import '@/packages/core-ui/styles/modals.css';

installManageCefBridge();

await import('../views/manage');
