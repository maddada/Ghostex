import { type AppState } from '@excalidraw/excalidraw/types';
import { ManageQuickLabel } from './types';

export const MANAGE_FILES_RESPONSE_EVENT = 'ghostex-manage-files-response';
export const MANAGE_FILES_CHANGED_EVENT = 'ghostex-manage-files-changed';
export const MANAGE_DRAG_DATA_TYPE = 'application/x-ghostex-manage-path';
export const MANAGE_BRIDGE_TIMEOUT_MS = 15_000;
export const MANAGE_DOCS_ROOT_PATH = 'docs';
export const MANAGE_SELECTION_MAX_LENGTH = 700;
export const MANAGE_ANNOTATIONS_SIDECAR_PATH = '.ghostex/manage-annotations.json';
export const MANAGE_ANNOTATION_SCHEMA_VERSION = 1;
export const MANAGE_ANNOTATION_IMAGE_MAX_BYTES = 512 * 1024;
export const MANAGE_ANNOTATION_MAX_IMAGES = 4;
/*
 * CDXC:ManageAutosave 2026-06-28-02:36:
 * Markdown and Excalidraw edits should persist automatically shortly after the user stops changing content because those artifact surfaces do not expose a visible Save button. Debounce saves for one second so normal typing and drawing gestures coalesce into a single bridge write.
 */
export const MANAGE_CONTENT_AUTOSAVE_DELAY_MS = 1_000;
export const MANAGE_GPUI_FILE_CHANGE_POLL_INTERVAL_MS = 400;
export const MANAGE_GPUI_FILE_CHANGE_DEBOUNCE_MS = 500;
export const MANAGE_SIDEBAR_DEFAULT_WIDTH = 292;
export const MANAGE_SIDEBAR_MIN_WIDTH = 230;
export const MANAGE_SIDEBAR_MAX_WIDTH = 560;
export const MANAGE_FLOATING_SIDEBAR_MAX_WIDTH = 690;
export const MANAGE_SIDEBAR_SIDE_STORAGE_KEY = 'ghostex.manage.sidebarSide';
export const MANAGE_SIDEBAR_WIDTH_STORAGE_KEY = 'ghostex.manage.sidebarWidth';
/*
 * CDXC:ManageDrawings 2026-06-28-04:56:
 * Manage Excalidraw uses Excalidraw's dark theme, where the visually dark canvas is serialized as viewBackgroundColor #ffffff. Default new drawings to that saved value so created artifacts open with the same dark-looking background users get after choosing a dark canvas inside Excalidraw.
 */
export const MANAGE_EXCALIDRAW_CANVAS_BACKGROUND = '#ffffff';
/*
 * CDXC:ManageDrawings 2026-06-28-01:43:
 * Manage should keep Excalidraw in dark mode so drawings match the macOS app's dark workarea instead of reopening through Excalidraw's light scheme. Apply the theme at the editor boundary so existing files and newly created artifacts render dark.
 */
export const MANAGE_EXCALIDRAW_CANVAS_THEME: AppState['theme'] = 'dark';
export const MANAGE_COMMENT_ANNOTATION_COLOR = '#e2b340';
export const MANAGE_REDLINE_ANNOTATION_COLOR = '#fda4af';
export const MANAGE_DISMISS_TOOLBAR_COLOR = '#f87171';
export const MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN = 18;
export const MANAGE_SELECTION_TOOLBAR_WIDTH_ESTIMATE = 228;
export const MANAGE_MEO_CONTENT_MAX_WIDTH = '800px';

/*
 * CDXC:ManageMarkdownToolbar 2026-06-28-06:00:
 * Manage Markdown should keep Ghostex annotations as the default selection toolbar while letting users switch that floating surface to Meo's inline formatting controls.
 * The annotation toolbar width estimate includes the formatting switch so first-column selections still keep a real left edge margin.
 *
 * CDXC:ManageMarkdownTheme 2026-06-28-06:00:
 * Markdown headings in Manage's embedded Meo editor should use #42a5f5 instead of the previous red heading token so heading color matches the requested macOS Manage styling.
 *
 * CDXC:ManageMarkdownTheme 2026-06-28-06:50:
 * Inline markdown code in the macOS Docs Project/Manage editor should use a dedicated orange code token instead of the yellow #fde68a token. Override the Meo monospace token directly so warning, frontmatter, and other base07 uses keep their existing yellow affordance.
 *
 * CDXC:ManageMarkdownTheme 2026-06-28-06:54:
 * Variables inside Manage Docs markdown code blocks should render as normal text with #e5e7eb instead of the purple #c084fc token. Override variable-like Meo syntax tokens directly so base08 can still style non-variable purple affordances.
 *
 * CDXC:ManageMarkdownTheme 2026-06-28-06:59:
 * Bash strings in Manage Docs markdown code blocks should stop using the yellow #fde68a token, and multiline bash variables should not keep the purple #c084fc token through alternate highlighter scopes.
 * Code blocks in the same editor should use #2a2d30 with a 1px border while preserving CodeMirror's line-owned layout.
 *
 * CDXC:ManageMarkdownTheme 2026-06-28-07:10:
 * Inline backtick code in Manage Docs should share the #2a2d30 code-block background and use a lighter orange than the previous #e8912c code token.
 */
export const MANAGE_MEO_HEADING_COLOR = '#42a5f5';
export const MANAGE_MEO_CODE_COLOR = '#f2b35f';
export const MANAGE_MEO_VARIABLE_COLOR = '#e5e7eb';
export const MANAGE_MEO_CODE_BLOCK_BACKGROUND = '#2a2d30';

export const MANAGE_QUICK_LABELS: ManageQuickLabel[] = [
  { color: '#a78bfa', id: 'clarify', text: 'Clarify' },
  { color: '#f59e0b', id: 'needs-tests', text: 'Needs tests' },
  { color: '#86efac', id: 'looks-good', text: 'Looks good' },
];
