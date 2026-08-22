import { AppTooltip } from "@/packages/core-ui/app-tooltip";
import {
  type CSSProperties,
  type DragEvent as ReactDragEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  IconLayoutSidebarLeftExpand,
  IconLayoutSidebarRightExpand,
  IconRefresh,
  IconSearch,
  IconX,
} from "@tabler/icons-react";
import {
  requestProjectDocsFromHost,
  type ProjectDocsFileEntry as ManageFileEntry,
  type ProjectDocsFilePreview as ManageFilePreview,
  type ProjectDocsRequest as ManageFilesBridgeRequest,
  type ProjectDocsResponse as ManageFilesBridgeResponse,
} from "@/packages/shared/project-docs";
import { MANAGE_ANNOTATIONS_SIDECAR_PATH, MANAGE_BRIDGE_TIMEOUT_MS, MANAGE_CONTENT_AUTOSAVE_DELAY_MS, MANAGE_DOCS_ROOT_PATH, MANAGE_DRAG_DATA_TYPE, MANAGE_FILES_CHANGED_EVENT, MANAGE_FILES_RESPONSE_EVENT, MANAGE_FLOATING_SIDEBAR_MAX_WIDTH, MANAGE_GPUI_FILE_CHANGE_DEBOUNCE_MS, MANAGE_GPUI_FILE_CHANGE_POLL_INTERVAL_MS, MANAGE_SIDEBAR_DEFAULT_WIDTH, MANAGE_SIDEBAR_MAX_WIDTH, MANAGE_SIDEBAR_MIN_WIDTH, MANAGE_SIDEBAR_SIDE_STORAGE_KEY, MANAGE_SIDEBAR_WIDTH_STORAGE_KEY } from "./constants";
import { ManageAnnotation, ManageArtifactKind, ManageDocsOpenFileWindow, ManageDragState, ManageDropTarget, ManageFileContextMenuState, ManageFileOperationState, ManageRenameDialogState, ManageSidebarSide, ManageWebKitWindow } from "./types";
import { ManageEmptyState, ManageFileContextMenu, ManageFileRow, ManageRenameDialog, ManageSidebarActions } from "./file-tree-ui";
import { ManagePreview } from "./preview/manage-preview";
import { ManageTooltipButton } from "./manage-tooltip-button";
import { canCreateManageEntryChildren, canMoveManageEntryToDirectory, canOpenManageEntryContextMenu, canRenameOrDeleteManageEntry, createDuplicateManageFilePath, createInitialArtifactContent, createInitialCollapsedManageDirectoryPaths, createUniqueArtifactPath, createUniqueFolderPath, dropDirectoryPathForManageEntry, filterManageEntriesForSearch, hasCollapsedManageAncestor, isExcalidrawPath, isHtmlPath, isManageDescendantPath, isMarkdownPath, manageFileMetadataSignature, moveManagePathToDirectory, orderManageEntriesForTree, parentManagePath, remapManageAnnotationPathsForMove, remapManagePathByMove, remapManagePathSetForMove, removeManageAnnotationPathsForDeletedEntry, removeManagePathSetForDeletedEntry, renameManageFilePath, shouldAutosaveManageFile, validateManageRenameFileName } from "./file-tree-utils";
import { parseManageAnnotationStore, serializeManageAnnotationStore, stableManageAnnotationStoreKey, writeTextToClipboard } from "./annotation-store";

/*
 * CDXC:ManageEditing 2026-06-20-06:14:
 * Manage is an editable bundled WKWebView project workarea beside Kanban. The page opens project-relative text, Markdown, and drawing files; Swift owns root resolution and save scoping, so the WK URL and JavaScript bridge never carry absolute workspace paths.
 *
 * CDXC:ManageAnnotations 2026-06-20-06:14:
 * Markdown review in Manage needs lightweight annotation behavior in the same workarea as editing. Keep annotations path-scoped in page state, capture selected source or preview text, mark matching Markdown text in the preview, and surface counts in the file tree without persisting user text to logs.
 *
 * CDXC:ManageAnnotations 2026-06-20-06:35:
 * Markdown feedback must behave like a local review tool: Select mode exposes a nearby action toolbar, Redline mode turns selected text into deletion annotations immediately, Comment mode focuses the comment composer, global comments work without selected text, quick labels add preset feedback, and structured Markdown export copies review data without logging annotation text.
 *
 * CDXC:ManageAnnotationPersistence 2026-06-20-06:35:
 * Annotation state should survive Manage reloads when the native project bridge is available. Store a versioned JSON sidecar under a Ghostex-owned project folder through the same project-relative read/save bridge, so Swift keeps path normalization and traversal checks at the writer boundary.
 *
 * CDXC:ManageAnnotationAttachments 2026-06-20-06:35:
 * Annotation images are user-provided feedback artifacts. Keep them local to the annotation sidecar as bounded data URLs, render compact thumbnails, and include attachment references in copied Markdown only when the user explicitly copies feedback.
 *
 * CDXC:ManageAnnotations 2026-06-26-23:35:
 * Markdown artifacts should use a rendered-document review shape with floating selection actions, an anchored comment popover, and a side annotation timeline. Do not show Manage's old Edit/Split/Preview tabs or fixed bottom annotation composer for Markdown files.
 *
 * CDXC:ManageMarkdownRendering 2026-06-26-23:35:
 * Manage Markdown rendering should use a local block parser and consistent visual scale for headings, lists, blockquotes, code, tables, alerts, directives, and raw HTML blocks instead of a generic Markdown preview.
 *
 * CDXC:ManageMarkdownEditing 2026-06-27-12:40:
 * Markdown artifacts must be editable and richly rendered in one surface, matching Meo's live Markdown editor instead of a split edit/preview or review-only view.
 * Mount Meo's copied CodeMirror live editor for Markdown files while keeping Ghostex annotations in the same Manage workarea.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-27-12:40:
 * Users need to edit Markdown text and annotate selections at the same time.
 * Feed Meo editor selections into the existing annotation toolbar and render sidecar comments/redlines as CodeMirror decorations so annotation review remains visible during editing.
 *
 * CDXC:ManageMarkdownHeader 2026-06-27-13:01:
 * Markdown artifacts need a single top row: show the project-relative file path in the header, remove the separate path/status row, move Comment/Copy controls into the header, and expose a collapsible annotation rail with the active annotation count.
 * Annotation cards must size to their own content instead of stretching to fill the rail.
 *
 * CDXC:ManageArtifactHeader 2026-06-28-00:13:
 * HTML and Excalidraw artifacts need the same compact header cleanup as Markdown: show the project-relative artifact path in the title, keep type/size/edit state in that one row, and remove the separate path row.
 *
 * CDXC:ManageHtmlRendering 2026-06-28-01:25:
 * HTML artifacts in Manage should render as page DOM instead of source text.
 *
 * CDXC:ManageHtmlRendering 2026-06-29-17:25:
 * HTML Docs need to look like the same real page users see in a browser. Preserve full-document head CSS, stylesheet links, and meta tags inside an isolated srcdoc frame instead of stripping styles and injecting only body markup into Ghostex's dark Manage document.
 *
 * CDXC:ManageHtmlRendering 2026-07-01-18:12:
 * HTML Docs are an interactive document preview. Preserve page-authored scripts, event handlers, script-like URLs, frames, and base tags so generated docs can use full browser JavaScript instead of a passive sanitized snapshot.
 *
 * CDXC:ManageHtmlAgentation 2026-06-28-01:46:
 * Rendered HTML artifacts need their own Agentation launch control because Manage hides the native browser toolbar that normally exposes feedback tools.
 *
 * CDXC:ManageHtmlAgentation 2026-06-28-02:29:
 * The control is named Annotate, behaves as a toggle, and defaults on for HTML artifacts.
 * When enabled, the rendered HTML document includes the Agentation bootstrap; when disabled, the document reloads without that bootstrap so no annotation overlay remains.
 *
 * CDXC:ManageHtmlAgentation 2026-06-29-18:20:
 * Agentation must be injected into the loaded HTML document itself, not mounted by the parent Manage page into the iframe wrapper. Append only the fixed Ghostex bootstrap module after parsing the authored document so page scripts remain intact while the annotation runtime executes in the rendered page context.
 *
 * CDXC:ManageHtmlAgentation 2026-06-30-04:41:
 * The embedded HTML document must run page-authored JavaScript and the fixed Agentation bootstrap with its normal document origin so remote module imports and DOM overlays initialize reliably inside the loaded page. Allow scripts and same-origin for the full srcdoc output.
 *
 * CDXC:ManageHtmlRendering 2026-06-30-04:57:
 * Embedded HTML Docs should keep page-owned layout and colors while Ghostex owns only the viewer chrome. Inject a final document-scoped scrollbar style so all page scrollbars are 4px wide with transparent tracks and corners instead of a visible background gutter.
 *
 * CDXC:ManageHtmlRendering 2026-06-30-11:58:
 * Do not use standards `scrollbar-width: thin` for embedded HTML Docs because Chromium/WebKit can render that as a wider browser-defined scrollbar. Reset standards scrollbar properties to `auto`, then rely on the WebKit scrollbar pseudo-elements for exact 4px sizing and the required #3e444c thumb color.
 *
 * CDXC:ManageHtmlAgentation 2026-06-28-07:58:
 * Opening an HTML Docs page should show Agentation's bottom-left control but must not auto-enter feedback mode because immediate activation steals mouse focus from users who only want to read or interact with the page.
 *
 * CDXC:ManageDefaultHtml 2026-06-28-07:17:
 * New HTML Docs files should start with a dark Ghostex-styled onboarding page that explains how to ask an agent for an explanatory HTML document and how to use Agentation to annotate the rendered result.
 * The starter document stays self-contained with document-owned styles and no scripts, while the HTML renderer now preserves author CSS in an isolated document so future generated pages render like browser HTML instead of inheriting Ghostex UI styles.
 *
 * CDXC:ManageDefaultHtml 2026-06-30-04:41:
 * The starter page should not leave an empty fourth grid cell on narrower Docs widths. Use document-owned CSS for a max two-column feature grid, move the good-request/good-annotation guidance into a fourth card, and keep the page background covering the full embedded viewport including scrollbar gutters.
 *
 * CDXC:ManageMarkdownSelectionToolbar 2026-06-27-22:41:
 * The floating Markdown selection toolbar should be icon-only: remove Copy/Delete, keep Comment plus quick labels and Dismiss, show hover tooltips, and color each annotation action to match the highlight it writes into the selected text.
 * Plain comments use #e2b340 so the comment icon and unlabeled comment highlight stay visually paired.
 *
 * CDXC:ManageMarkdownSelectionToolbar 2026-06-28-01:49:
 * The floating selection toolbar should stay visually inset from the Manage window edge even when the selected text starts at the first column.
 * Clamp the centered toolbar by its real compact width so it does not sit flush against the left side.
 *
 * CDXC:ManageMarkdownToolbar 2026-06-28-06:00:
 * Markdown Manage in the macOS app should expose Meo's editor-native formatting toolbar and Meo's inline formatting selection toolbar while keeping Ghostex annotation actions active in the same editor.
 * Selected text opens the annotation toolbar by default, and the floating toolbar provides an explicit switch between annotation actions and formatting actions.
 *
 * CDXC:ManageMarkdownToolbar 2026-06-28-07:56:
 * The Live/Source segmented control must make the selected mode visually explicit. Manage overrides Meo's neutral active state with a tinted fill and inset outline while keeping the copied toolbar's stable button dimensions.
 *
 * CDXC:ManageMarkdownTheme 2026-06-28-06:00:
 * Manage Markdown headings should use #42a5f5 for the Meo heading token instead of the previous red heading color.
 *
 * CDXC:ManageMarkdownGitGutter 2026-06-28-06:17:
 * Markdown artifacts should show Meo's Git changes gutter in the same live editor surface by comparing the current editor text with the file's Git HEAD baseline. Native supplies only the Meo-compatible baseline fields needed for rendering so repo roots and Git paths do not cross into the bundled WK page.
 *
 * CDXC:ManageMarkdownEditing 2026-06-28-01:49:
 * Markdown editing should keep the line-number gutter tight in Manage.
 * Scope the gutter width and content padding overrides to Manage's Meo wrapper so the gap between line numbers, Meo's 3px Git gutter, and text is minimal without changing the shared Meo editor.
 *
 * CDXC:ManageMarkdownLineNumbers 2026-06-29-01:53:
 * Wrapped Markdown lines should keep their line number aligned with the first visual row instead of centering the number across the wrapped block. Override Meo's flex-centered line-number gutter only inside Manage so source and live Markdown text stay visually scan-aligned.
 *
 * CDXC:ManageAnnotationComposer 2026-06-28-01:49:
 * The anchored comment composer should feel like a compact dark panel: show only the note textarea, close from a top-right X, keep image upload as a plain action button, and submit with a green Submit button instead of a Cancel/Comment action row.
 *
 * CDXC:ManageAnnotationComposer 2026-06-28-07:56:
 * The Add global comment composer opens from the compact Docs header and must render above Meo's copied toolbar layer, matching the annotation dropdown's overlay ownership instead of being hidden behind editor chrome.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-27-22:52:
 * The annotation list should open as a top-row dropdown instead of occupying a persistent sidebar.
 * Keep cards compact, subtly tint their background from the annotation type or quick-label color, avoid repeating quick-label text as body copy, and expose a persistent top-right remove X.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-28-05:24:
 * Manage Markdown annotations must accept selections that span multiple rendered lines and still resolve their normalized quote back onto the raw Markdown text.
 * When the caret rests inside an existing annotated range, show a passive floating card above the full annotated range with a short preview of the saved comment so users can recover annotation context without opening the dropdown.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-28-06:49:
 * The Docs annotation dropdown opens from the compact file header and must render above Meo's copied toolbar layer. Keep the dropdown owned by the header action but give it a higher stack level than Meo's z-index 500 toolbar so the menu is not hidden until below the editor toolbar.
 *
 * CDXC:ManageDrawings 2026-06-20-06:14:
 * .excalidraw files should open as editable drawings instead of raw JSON. Use the upstream Excalidraw component for canvas behavior, serialize full scene JSON through the normal Manage save bridge, and keep invalid drawings editable as source text so users can repair them.
 *
 * CDXC:ManageDrawings 2026-06-28-01:43:
 * The Manage Excalidraw canvas should use Excalidraw's dark scheme so the drawing surface is dark in the macOS Manage view. This intentionally prioritizes app dark-mode consistency over the previous light-theme literal color behavior.
 *
 * CDXC:ManageEditing 2026-06-21-18:00:
 * The macOS Manage editor header should not show an explicit Save button. Keep edited/saved status visible in metadata while retaining the existing bridge-backed save behavior through the keyboard shortcut and editor flows.
 *
 * CDXC:ManageSidebar 2026-06-20-17:15:
 * Manage's file-sidebar refresh control is an overflow menu with Refresh and Switch sidebar side actions. A separate adjacent icon hides the file sidebar, and the editor area provides a small restore affordance so hiding is reversible.
 *
 * CDXC:ManageSidebar 2026-06-30-01:35:
 * The Docs sidebar overflow dropdown should read as a compact polished popover instead of a flat black rectangle. Inset it from the sidebar edge, round the menu surface, soften the shadow, and keep each action as a clear icon/text row with a visible hover state.
 *
 * CDXC:ManageSidebar 2026-06-30-02:30:
 * The Docs sidebar dropdown should not have a pointer arrow and should use a flat #0e0e0e background with a 1px #595959 border instead of a gradient surface.
 *
 * CDXC:ManageSidebar 2026-06-30-02:45:
 * Docs dropdown corners should be only slightly rounded, using a 4px menu radius and 3px row radius so the popover feels sharper.
 *
 * CDXC:ManageArtifacts 2026-06-26-13:59:
 * Manage started as an artifacts-focused project surface with first-class sidebar actions for new Markdown, HTML, and Excalidraw files.
 *
 * CDXC:Docs 2026-06-28-06:24:
 * The Manage-backed surface is user-facing Docs and reads/writes project
 * documents under ./docs. New Markdown, HTML, and Excalidraw documents should
 * be created in that docs root instead of the previous artifacts root.
 *
 * CDXC:ManageFileActions 2026-06-28-04:35:
 * Users need to right-click files in the Manage sidebar and rename or delete them from a context menu. Keep the menu file-scoped, require a second destructive click before delete, preserve annotations across rename, and send only project-relative paths through the native bridge.
 *
 * CDXC:ManageSidebar 2026-06-26-23:14:
 * The Manage file sidebar needs a visible resizer so users can widen the artifacts tree on either sidebar side without overlapping the preview/editor. Persist the width locally and clamp it to the current workarea so the preview keeps usable space.
 *
 * CDXC:ManageSidebar 2026-06-28-05:18:
 * The Manage artifact sidebar should visually match Ghostex's left reference sidebar: use the same near-black surface, muted section hierarchy, borderless navigation-style controls, larger lightweight rows, and neutral selected-row chrome instead of boxed blue file-list styling.
 *
 * CDXC:ManageFolders 2026-06-28-06:39:
 * The Docs sidebar needs first-class folders: users can create folders, collapse or expand folder rows, and drag files or folders into another folder or back to the docs root. Keep the drag feedback aligned with the main sidebar by dimming the dragged row, using the same neutral insertion-line treatment for root drops, and using a dark row target for folder drops.
 *
 * CDXC:ManageFolders 2026-06-28-07:02:
 * Native preserves a flat listing order that protects root docs from nested-folder entry caps, but the UI must render that data as a real tree. Reorder entries in the web layer so each folder's children appear directly below their parent before applying collapsed-folder filtering.
 *
 * CDXC:ManageCreateMenu 2026-06-28-07:04:
 * The Docs sidebar create actions should live behind one header plus button instead of consuming a permanent four-button row below the project title. Keep Folder, Markdown, HTML, and Draw as menu items so the left sidebar starts with Search and file content after the header.
 *
 * CDXC:ManageFolders 2026-06-28-07:12:
 * Dragging over a file row should target that row's containing folder, and dragging over a root-level file should target docs/ so users can move items out of folders without needing blank sidebar space.
 * File rows should not show file size badges.
 *
 * CDXC:DocsSidebar 2026-06-28-15:05:
 * Docs sidebar chrome should match the compact macOS sidebar: keep the project title non-selectable, remove the file count/selected-file summary block, show a 2px scrollbar only on hover/focus, and mirror the native sidebar divider's five-point rail with a one-point edge line plus three-point hover affordance.
 *
 * CDXC:DocsSidebar 2026-06-28-15:57:
 * Docs file rows should use tighter button padding. The active file keeps the selected-row surface, while every ancestor folder of the active file turns full white without gaining a background so users can track the open document through collapsed or nested folder context.
 *
 * CDXC:DocsSidebar 2026-06-28-16:29:
 * Docs sidebar search and file row buttons should fill the sidebar width with no outer horizontal gutter. Keep spacing as internal padding so hover, active, and focus backgrounds reach both sidebar edges.
 *
 * CDXC:DocsHeader 2026-06-28-18:02:
 * The Docs main header should be a compact titlebar-like chrome strip. Reduce title/meta/action text, keep action buttons full-height with square corners and separator borders, and use hover/open fills that match the macOS titlebar button treatment.
 *
 * CDXC:DocsHeader 2026-06-29-03:43:
 * Manage's sidebar header and hidden-sidebar restore affordance should share the editor header's compact titlebar strip: compact title typography, full-height square buttons with separator borders, and expand icons that communicate reopening the sidebar.
 *
 * CDXC:DocsHeader 2026-06-29-13:00:
 * The compact editor header hosts dropdown actions such as the annotations button. Keep text truncation on the title span, but let the header overflow visibly so action popovers are not clipped to the titlebar strip.
 *
 * CDXC:DocsHeader 2026-06-29-13:45:
 * Drawing-mode compact headers do not have a right-side action group, so keep a right inset on the header metadata instead of letting the file type and size touch the expanded sidebar divider.
 *
 * CDXC:DocsHeader 2026-06-29-21:48:
 * The Docs editor and sidebar headers were raised to 36px, three pixels taller than the earlier compact strip, with title line-height and full-height header buttons matching that height.
 *
 * CDXC:DocsHeader 2026-06-29-23:39:
 * The Docs editor and sidebar titlebars should now be one pixel shorter at
 * 35px, while keeping the same full-height button and title line-height
 * geometry so the internal Docs chrome matches the native project-editor
 * companion titlebar height.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-29-20:13:
 * Markdown header annotations need a two-step Clear All action beside Copy: first click arms a three-second red Confirm state, the second click clears the current file's annotations, and the annotations count button keeps a 7px inset from the right edge.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-29-20:16:
 * Annotation cards need a persistent remove X in the card's top-right corner so deletion is discoverable without depending on hover-only opacity.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-29-20:54:
 * The caret-triggered floating annotation preview uses a separate card from the dropdown. It needs the same top-right remove X, with pointer events enabled only for that button so the preview remains passive while the remove action is clickable.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-29-21:02:
 * Annotation dropdown and caret-preview cards should use flat, subtle tinted surfaces instead of gradient backgrounds so annotations read as quieter UI chrome.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-29-21:21:
 * Annotation-card remove X controls should not draw a left divider or boxed chrome; they sit directly on the card surface as simple icon affordances.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-30-11:14:
 * The Docs annotation dropdown should not repeat the annotation count because the titlebar trigger already owns that indicator.
 * Keep the dropdown, annotation cards, and count indicator slightly rounded, and force card remove controls to opt out of titlebar button separators inside the dropdown.
 *
 * CDXC:ManageMarkdownAnnotations 2026-06-30-15:15:
 * Annotation quote overflow should use a 2px transparent scrollbar with no visible track, and the thumb should appear only while the user hovers or focuses within that card.
 *
 * CDXC:DocsSidebar 2026-06-29-04:08:
 * Root-level artifact files and docs/ content share the same Docs sidebar, so docs/ must render as an explicit expandable folder instead of an invisible tree root. Keep creation/drop defaults targeting docs/, but order rows from the real repo root and provide a header button to collapse or expand docs/.
 *
 * CDXC:DocsSidebar 2026-06-30-00:15:
 * The Docs header folder control should use the same diagonal-arrows icon language as the macOS sidebar Projects Collapse All / Expand Previous control, but Docs does not remember previous expansion state. Collapse All must collapse every expandable nested folder, and Expand All must clear every collapsed folder so all descendants reopen.
 *
 * CDXC:DocsSidebar 2026-06-30-01:46:
 * The Docs sidebar header should be actions-only; do not repeat the root docs folder icon/name in the titlebar. Keep the search-to-file-list gap tight so the file tree begins immediately below Search.
 *
 * CDXC:ManageFileActions 2026-06-29-03:27:
 * Docs sidebar context actions apply to folders as well as files. Right-clicking empty sidebar chrome must suppress the browser/WebKit default context menu, while folder rename/delete remaps nested selected paths and annotation keys through the same docs-relative bridge.
 *
 * CDXC:ManageFileActions 2026-06-30-09:48:
 * Files and folders need a Copy path action in the Docs sidebar. Copy the same relative path used by Manage file operations so users can paste stable docs paths without exposing absolute workspace paths to WebKit. The docs root may open this copy-only menu, but rename/delete remain unavailable for that fixed root.
 *
 * CDXC:ManageFileActions 2026-07-01-00:59:
 * File context menus need a Duplicate action that creates a same-folder copy named with the next available " (n)" suffix before the extension. Save the selected dirty file before duplicating it so the copy matches the visible editor content, but keep folders out of the duplicate action.
 *
 * CDXC:ManageFileActions 2026-07-02-13:14:
 * Docs sidebar context menus should feel like a macOS file navigator: reveal any visible file or folder in Finder, copy the docs-relative path label explicitly, create Markdown/HTML/Excalidraw files or folders inside the clicked folder, and stage readable files into the current agent session as context. Keep create-here folder-scoped, keep Duplicate file-only, and preserve Rename/Delete as the core destructive pair.
 */
/*
 * CDXC:GPUISessionChatLinks 2026-08-03:
 * The gpui app asks Docs to open one specific docs-relative file when a chat
 * file link points inside the Docs scope. The request can land before this
 * page mounts (the workarea surface is created while the mode switches), so
 * the hook is installed at module load and the last pending path is replayed
 * once ManageApp registers its handler.
 */

let pendingManageDocsOpenPath: string | undefined;
let manageDocsOpenFileHandler: ((path: string) => void) | undefined;

export function registerManageDocsOpenFileHandler(handler?: (path: string) => void): void {
  manageDocsOpenFileHandler = handler;
  if (handler === undefined || pendingManageDocsOpenPath === undefined) {
    return;
  }
  const path = pendingManageDocsOpenPath;
  pendingManageDocsOpenPath = undefined;
  handler(path);
}

(window as ManageDocsOpenFileWindow).ghostexOpenDocsFile = (path: unknown) => {
  if (typeof path !== "string" || path.length === 0) {
    return;
  }
  if (manageDocsOpenFileHandler !== undefined) {
    manageDocsOpenFileHandler(path);
    return;
  }
  pendingManageDocsOpenPath = path;
};

/** Every ancestor folder of a docs-relative path ("a/b/c.md" → ["a", "a/b"]). */
export function manageAncestorDirectoryPaths(path: string): string[] {
  const segments = path.split("/").filter((segment) => segment.length > 0);
  segments.pop();
  return segments.map((_, index) => segments.slice(0, index + 1).join("/"));
}

export function ManageApp() {
  const params = useMemo(() => new URLSearchParams(window.location.search), []);
  const projectId = params.get("projectId") ?? "";
  const projectEditorId = params.get("projectEditorId") ?? projectId;
  const [entries, setEntries] = useState<ManageFileEntry[]>([]);
  const [query, setQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const [selectedPath, setSelectedPath] = useState<string>();
  const selectedPathRef = useRef<string | undefined>(undefined);
  const [preview, setPreview] = useState<ManageFilePreview>();
  const [draftContent, setDraftContent] = useState("");
  const [lastSavedContent, setLastSavedContent] = useState("");
  const [listState, setListState] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [previewState, setPreviewState] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [hasExternalChanges, setHasExternalChanges] = useState(false);
  const saveResetTimerRef = useRef<number | undefined>(undefined);
  const contentAutosaveTimerRef = useRef<number | undefined>(undefined);
  const [error, setError] = useState<string>();
  const [annotationsByPath, setAnnotationsByPath] = useState<Record<string, ManageAnnotation[]>>({});
  const [annotationPersistenceState, setAnnotationPersistenceState] =
    useState<"idle" | "loading" | "ready" | "saving" | "saved" | "error">("idle");
  const [sidebarSide, setSidebarSide] = useState<ManageSidebarSide>(() => readStoredManageSidebarSide());
  const [sidebarWidth, setSidebarWidth] = useState(() => readStoredManageSidebarWidth());
  const [sidebarHidden, setSidebarHidden] = useState(false);
  const [sidebarFloating, setSidebarFloating] = useState(() => window.innerWidth < MANAGE_FLOATING_SIDEBAR_MAX_WIDTH);
  const [collapsedDirectoryPaths, setCollapsedDirectoryPaths] = useState<Set<string>>(() => new Set());
  const [creatingArtifactKind, setCreatingArtifactKind] = useState<ManageArtifactKind>();
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [fileContextMenu, setFileContextMenu] = useState<ManageFileContextMenuState>();
  const [fileOperation, setFileOperation] = useState<ManageFileOperationState>();
  const [renameDialog, setRenameDialog] = useState<ManageRenameDialogState>();
  const [dragState, setDragState] = useState<ManageDragState>();
  const [dropTarget, setDropTarget] = useState<ManageDropTarget>();
  const shellRef = useRef<HTMLElement | null>(null);
  const sidebarRef = useRef<HTMLElement | null>(null);
  const annotationsLoadedRef = useRef(false);
  const annotationsSaveTimerRef = useRef<number | undefined>(undefined);
  const hasInitializedDirectoryCollapseRef = useRef(false);
  const lastPersistedAnnotationsRef = useRef("");
  const isEditablePreview = preview?.kind === "text";
  const isDirty = isEditablePreview && draftContent !== lastSavedContent;

  const readFile = useCallback(
    async (path: string) => {
      setHasExternalChanges(false);
      setSelectedPath(path);
      selectedPathRef.current = path;
      setPreview(undefined);
      setDraftContent("");
      setLastSavedContent("");
      setPreviewState("loading");
      setSaveState("idle");
      setError(undefined);
      try {
        const response = await requestManageFiles({
          action: "read",
          path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        const openedFile = response.file;
        setPreview(openedFile);
        const nextContent = openedFile?.content ?? "";
        setDraftContent(nextContent);
        setLastSavedContent(nextContent);
        if (openedFile) {
          setEntries((currentEntries) =>
            currentEntries.map((entry) =>
              entry.path === openedFile.path
                ? {
                    ...entry,
                    modifiedAt: openedFile.modifiedAt,
                    size: openedFile.size,
                  }
                : entry,
            ),
          );
        }
        setPreviewState("ready");
      } catch (readError) {
        setPreviewState("error");
        setError(readError instanceof Error ? readError.message : "Could not open file.");
      }
    },
    [projectEditorId, projectId],
  );

  /*
   * CDXC:GPUISessionChatLinks 2026-08-03:
   * Docs opens the file a chat link asked for and expands the folders leading
   * to it, so the sidebar shows where the opened file lives instead of
   * selecting a row hidden inside a collapsed folder.
   */
  useEffect(() => {
    registerManageDocsOpenFileHandler((path) => {
      const ancestors = manageAncestorDirectoryPaths(path);
      if (ancestors.length > 0) {
        setCollapsedDirectoryPaths((current) => {
          const next = new Set(current);
          for (const ancestor of ancestors) {
            next.delete(ancestor);
          }
          return next.size === current.size ? current : next;
        });
      }
      void readFile(path);
    });
    return () => registerManageDocsOpenFileHandler(undefined);
  }, [readFile]);

  const refreshFiles = useCallback(async () => {
    setListState("loading");
    setError(undefined);
    try {
      const response = await requestManageFiles({
        action: "list",
        projectEditorId,
        projectId,
      });
      if (response.error) {
        throw new Error(response.error);
      }
      const nextEntries = response.entries ?? [];
      setEntries(nextEntries);
      if (!hasInitializedDirectoryCollapseRef.current) {
        /*
         * CDXC:DocsSidebar 2026-06-30-12:40:
         * Opening Docs should start with every expandable folder and subfolder collapsed in the file-list sidebar. Initialize this once from the first successful listing so later refreshes preserve the user's manual expand/collapse choices.
         */
        hasInitializedDirectoryCollapseRef.current = true;
        setCollapsedDirectoryPaths(createInitialCollapsedManageDirectoryPaths(nextEntries));
      }
      setListState("ready");
      const currentSelectedPath = selectedPathRef.current;
      const selectedStillExists =
        currentSelectedPath &&
        nextEntries.some((entry) => entry.kind === "file" && entry.path === currentSelectedPath);
      if (!selectedStillExists) {
        const firstFile = nextEntries.find((entry) => entry.kind === "file");
        if (firstFile) {
          void readFile(firstFile.path);
        } else {
          selectedPathRef.current = undefined;
          setSelectedPath(undefined);
          setPreview(undefined);
          setDraftContent("");
          setLastSavedContent("");
          setPreviewState("idle");
        }
      }
    } catch (listError) {
      setListState("error");
      setError(listError instanceof Error ? listError.message : "Could not load project files.");
    }
  }, [projectEditorId, projectId, readFile]);

  const openDocsFoldersSettings = useCallback(async () => {
    setError(undefined);
    try {
      const response = await requestManageFiles({
        action: "openDocsFoldersSettings",
        projectEditorId,
        projectId,
      });
      if (response.error) {
        throw new Error(response.error);
      }
    } catch (settingsError) {
      setError(settingsError instanceof Error ? settingsError.message : "Could not open Docs settings.");
    }
  }, [projectEditorId, projectId]);

  useEffect(() => {
    void refreshFiles();
  }, [refreshFiles]);

  useEffect(() => {
    /*
     * CDXC:GPUIDocsFileRefresh 2026-07-15:
     * GPUI's bundled Docs page has no native WKWebView file-presenter callback. Poll only
     * the selected artifact's lightweight metadata through the GPUI bridge, then apply a
     * trailing debounce before rereading it. HTML and Excalidraw are preview artifacts and
     * reload automatically; Markdown stays in place so an active editor is never replaced
     * without an explicit click, and exposes the pending change on its reload control.
     */
    const gpuiApi = (window as ManageWebKitWindow).ghostexGpui;
    if (
      gpuiApi?.supportsManageFileChangePolling !== true ||
      !selectedPath ||
      !preview ||
      (!isHtmlPath(selectedPath) && !isExcalidrawPath(selectedPath) && !isMarkdownPath(selectedPath))
    ) {
      return undefined;
    }

    const path = selectedPath;
    const automaticallyReload = isHtmlPath(path) || isExcalidrawPath(path);
    let cancelled = false;
    let pollInFlight = false;
    let observedSignature = manageFileMetadataSignature(preview);
    let debounceTimer: number | undefined;

    const pollSelectedFile = async () => {
      if (cancelled || pollInFlight) {
        return;
      }
      pollInFlight = true;
      try {
        const response = await requestManageFiles({
          action: "stat",
          path,
          projectEditorId,
          projectId,
        });
        const changedFile = response.file;
        if (cancelled || response.error || !changedFile || selectedPathRef.current !== path) {
          return;
        }
        const nextSignature = manageFileMetadataSignature(changedFile);
        if (nextSignature === observedSignature) {
          return;
        }
        if (isDirty || saveState === "saving") {
          return;
        }
        observedSignature = nextSignature;
        setEntries((currentEntries) =>
          currentEntries.map((entry) =>
            entry.path === path
              ? {
                  ...entry,
                  modifiedAt: changedFile.modifiedAt,
                  size: changedFile.size,
                }
              : entry,
          ),
        );
        if (debounceTimer !== undefined) {
          window.clearTimeout(debounceTimer);
        }
        debounceTimer = window.setTimeout(() => {
          debounceTimer = undefined;
          if (cancelled || selectedPathRef.current !== path) {
            return;
          }
          if (automaticallyReload) {
            void readFile(path);
          } else {
            setHasExternalChanges(true);
          }
        }, MANAGE_GPUI_FILE_CHANGE_DEBOUNCE_MS);
      } catch {
        // A transient stat failure should not replace the open document with an error surface.
      } finally {
        pollInFlight = false;
      }
    };

    const interval = window.setInterval(
      () => void pollSelectedFile(),
      MANAGE_GPUI_FILE_CHANGE_POLL_INTERVAL_MS,
    );
    return () => {
      cancelled = true;
      window.clearInterval(interval);
      if (debounceTimer !== undefined) {
        window.clearTimeout(debounceTimer);
      }
    };
  }, [isDirty, preview, projectEditorId, projectId, readFile, saveState, selectedPath]);

  useEffect(() => {
    /*
     * CDXC:DocsSidebar 2026-06-30-19:47:
     * Native watches the active project's Docs scan roots for file additions, removals, and renames. Treat the event as a path-free invalidation signal and reuse the normal list bridge so the sidebar refreshes without requiring an app refresh.
     */
    const handleFilesChanged = () => {
      void refreshFiles();
    };
    window.addEventListener(MANAGE_FILES_CHANGED_EVENT, handleFilesChanged);
    return () => window.removeEventListener(MANAGE_FILES_CHANGED_EVENT, handleFilesChanged);
  }, [refreshFiles]);

  useEffect(() => {
    window.localStorage.setItem(MANAGE_SIDEBAR_SIDE_STORAGE_KEY, sidebarSide);
  }, [sidebarSide]);

  useEffect(() => {
    window.localStorage.setItem(MANAGE_SIDEBAR_WIDTH_STORAGE_KEY, String(Math.round(sidebarWidth)));
  }, [sidebarWidth]);

  useLayoutEffect(() => {
    const shell = shellRef.current;
    if (!shell) {
      return undefined;
    }
    /*
     * CDXC:DocsSidebar 2026-06-30-13:45:
     * The Docs sidebar becomes a floating panel when the Manage page itself is narrower than 690px, not when the whole app window crosses a generic breakpoint. Measure the shell element so embedded and resized Manage surfaces use the same behavior.
     *
     * CDXC:DocsSidebar 2026-06-30-22:58:
     * Startup must apply floating sidebar mode before the first Docs paint when the project editor pane is already narrow. Use a layout effect so the shell width, not the larger app window width, decides the initial rendered mode.
     */
    const updateManageSidebarLayout = () => {
      const shellWidth = shell.getBoundingClientRect().width;
      setSidebarWidth((currentWidth) => clampManageSidebarWidth(currentWidth, shellWidth));
      setSidebarFloating(shellWidth < MANAGE_FLOATING_SIDEBAR_MAX_WIDTH);
    };
    updateManageSidebarLayout();
    const resizeObserver =
      typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(updateManageSidebarLayout);
    if (resizeObserver) {
      resizeObserver.observe(shell);
    } else {
      window.addEventListener("resize", updateManageSidebarLayout);
    }
    return () => {
      resizeObserver?.disconnect();
      if (!resizeObserver) {
        window.removeEventListener("resize", updateManageSidebarLayout);
      }
    };
  }, []);

  useEffect(() => {
    if (!sidebarFloating || sidebarHidden) {
      return undefined;
    }
    const hideFloatingSidebarOnOutsidePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) {
        return;
      }
      if (sidebarRef.current?.contains(target)) {
        return;
      }
      if (target instanceof Element && target.closest(".manage-file-context-menu")) {
        return;
      }
      setSidebarHidden(true);
    };
    window.addEventListener("pointerdown", hideFloatingSidebarOnOutsidePointerDown, true);
    return () => {
      window.removeEventListener("pointerdown", hideFloatingSidebarOnOutsidePointerDown, true);
    };
  }, [sidebarFloating, sidebarHidden]);

  useEffect(() => {
    let isCancelled = false;
    annotationsLoadedRef.current = false;
    setAnnotationPersistenceState("loading");
    async function loadAnnotations() {
      try {
        const response = await requestManageFiles({
          action: "read",
          path: MANAGE_ANNOTATIONS_SIDECAR_PATH,
          projectEditorId,
          projectId,
        });
        if (isCancelled) {
          return;
        }
        const content = response.error ? "" : (response.file?.content ?? "");
        const nextAnnotations = parseManageAnnotationStore(content);
        lastPersistedAnnotationsRef.current = stableManageAnnotationStoreKey(nextAnnotations);
        setAnnotationsByPath(nextAnnotations);
        annotationsLoadedRef.current = true;
        setAnnotationPersistenceState("ready");
      } catch {
        if (isCancelled) {
          return;
        }
        lastPersistedAnnotationsRef.current = stableManageAnnotationStoreKey({});
        setAnnotationsByPath({});
        annotationsLoadedRef.current = true;
        setAnnotationPersistenceState("ready");
      }
    }
    void loadAnnotations();
    return () => {
      isCancelled = true;
    };
  }, [projectEditorId, projectId]);

  useEffect(() => {
    if (!annotationsLoadedRef.current) {
      return;
    }
    const annotationStoreKey = stableManageAnnotationStoreKey(annotationsByPath);
    if (annotationStoreKey === lastPersistedAnnotationsRef.current) {
      return;
    }
    const serialized = serializeManageAnnotationStore(annotationsByPath);
    if (annotationsSaveTimerRef.current !== undefined) {
      window.clearTimeout(annotationsSaveTimerRef.current);
    }
    setAnnotationPersistenceState("saving");
    annotationsSaveTimerRef.current = window.setTimeout(() => {
      annotationsSaveTimerRef.current = undefined;
      void (async () => {
        try {
          const response = await requestManageFiles({
            action: "save",
            content: serialized,
            path: MANAGE_ANNOTATIONS_SIDECAR_PATH,
            projectEditorId,
            projectId,
          });
          if (response.error) {
            throw new Error(response.error);
          }
          lastPersistedAnnotationsRef.current = annotationStoreKey;
          setAnnotationPersistenceState("saved");
        } catch {
          setAnnotationPersistenceState("error");
        }
      })();
    }, 550);
  }, [annotationsByPath, projectEditorId, projectId]);

  const switchSidebarSide = useCallback(() => {
    setSidebarHidden(false);
    setSidebarSide((current) => (current === "left" ? "right" : "left"));
  }, []);

  const dismissFileContextMenu = useCallback(() => {
    setFileContextMenu(undefined);
  }, []);

  const copyEntryPath = useCallback(async (entry: ManageFileEntry) => {
    setFileContextMenu(undefined);
    try {
      /*
       * CDXC:DocsRootAdditive 2026-08-10:
       * Copy the path the tree shows, not the routing address. For a file under
       * a configured Docs directory those differ: the address leads with the
       * reserved mount segment, which is meaningless anywhere it would be
       * pasted.
       */
      await writeTextToClipboard(entry.displayPath ?? entry.path);
    } catch (copyError) {
      setError(copyError instanceof Error ? copyError.message : "Could not copy path.");
    }
  }, []);

  const copyEntryFullPath = useCallback(
    async (entry: ManageFileEntry) => {
      if (fileOperation) {
        return;
      }
      setFileOperation({ action: "copyFullPath", path: entry.path });
      setError(undefined);
      try {
        const response = await requestManageFiles({
          action: "copyFullPath",
          path: entry.path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        setFileContextMenu(undefined);
      } catch (copyError) {
        setError(copyError instanceof Error ? copyError.message : "Could not copy full path.");
      } finally {
        setFileOperation((current) =>
          current?.action === "copyFullPath" && current.path === entry.path ? undefined : current,
        );
      }
    },
    [fileOperation, projectEditorId, projectId],
  );

  const revealEntryInFinder = useCallback(
    async (entry: ManageFileEntry) => {
      if (fileOperation) {
        return;
      }
      setFileOperation({ action: "revealInFinder", path: entry.path });
      setError(undefined);
      try {
        const response = await requestManageFiles({
          action: "revealInFinder",
          path: entry.path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        setFileContextMenu(undefined);
      } catch (revealError) {
        setError(revealError instanceof Error ? revealError.message : "Could not reveal item in Finder.");
      } finally {
        setFileOperation((current) =>
          current?.action === "revealInFinder" && current.path === entry.path ? undefined : current,
        );
      }
    },
    [fileOperation, projectEditorId, projectId],
  );

  const openFileContextMenu = useCallback((entry: ManageFileEntry, point: { x: number; y: number }) => {
    if (!canOpenManageEntryContextMenu(entry)) {
      return;
    }
    setFileContextMenu({
      path: entry.path,
      x: point.x,
      y: point.y,
    });
  }, []);

  const suppressSidebarDefaultContextMenu = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    const target = event.target;
    if (target instanceof Element && target.closest(".manage-file-row, input, textarea")) {
      return;
    }
    event.preventDefault();
    setFileContextMenu(undefined);
  }, []);

  const updateSidebarWidthFromClientX = useCallback(
    (clientX: number) => {
      const shellRect = shellRef.current?.getBoundingClientRect();
      if (!shellRect) {
        return;
      }
      const nextWidth = sidebarSide === "right" ? shellRect.right - clientX : clientX - shellRect.left;
      setSidebarWidth(clampManageSidebarWidth(nextWidth, shellRect.width));
    },
    [sidebarSide],
  );

  const resizeSidebarBy = useCallback((delta: number) => {
    const containerWidth = shellRef.current?.getBoundingClientRect().width ?? window.innerWidth;
    setSidebarWidth((currentWidth) => clampManageSidebarWidth(currentWidth + delta, containerWidth));
  }, []);

  const handleSidebarResizePointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (sidebarHidden) {
        return;
      }
      event.preventDefault();
      updateSidebarWidthFromClientX(event.clientX);
      const handlePointerMove = (moveEvent: PointerEvent) => {
        updateSidebarWidthFromClientX(moveEvent.clientX);
      };
      const handlePointerUp = () => {
        window.removeEventListener("pointermove", handlePointerMove);
        window.removeEventListener("pointerup", handlePointerUp);
        window.removeEventListener("pointercancel", handlePointerUp);
      };
      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerUp);
      window.addEventListener("pointercancel", handlePointerUp);
    },
    [sidebarHidden, updateSidebarWidthFromClientX],
  );

  const handleSidebarResizeKeyDown = useCallback(
    (event: ReactKeyboardEvent<HTMLDivElement>) => {
      const direction = sidebarSide === "right" ? -1 : 1;
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        resizeSidebarBy(-12 * direction);
        return;
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        resizeSidebarBy(12 * direction);
        return;
      }
      if (event.key === "Home") {
        event.preventDefault();
        const containerWidth = shellRef.current?.getBoundingClientRect().width ?? window.innerWidth;
        setSidebarWidth(clampManageSidebarWidth(MANAGE_SIDEBAR_MIN_WIDTH, containerWidth));
        return;
      }
      if (event.key === "End") {
        event.preventDefault();
        const containerWidth = shellRef.current?.getBoundingClientRect().width ?? window.innerWidth;
        setSidebarWidth(clampManageSidebarWidth(MANAGE_SIDEBAR_MAX_WIDTH, containerWidth));
      }
    },
    [resizeSidebarBy, sidebarSide],
  );

  useEffect(
    () => () => {
      if (saveResetTimerRef.current !== undefined) {
        window.clearTimeout(saveResetTimerRef.current);
      }
      if (contentAutosaveTimerRef.current !== undefined) {
        window.clearTimeout(contentAutosaveTimerRef.current);
      }
      if (annotationsSaveTimerRef.current !== undefined) {
        window.clearTimeout(annotationsSaveTimerRef.current);
      }
    },
    [],
  );

  const annotationsForSelectedPath = selectedPath ? (annotationsByPath[selectedPath] ?? []) : [];
  const annotationCountsByPath = useMemo(() => {
    const nextCounts = new Map<string, number>();
    for (const [path, annotations] of Object.entries(annotationsByPath)) {
      if (annotations.length > 0) {
        nextCounts.set(path, annotations.length);
      }
    }
    return nextCounts;
  }, [annotationsByPath]);

  const saveContentSnapshot = useCallback(async ({
    content,
    path,
    throwOnError = false,
  }: {
    content: string;
    path: string;
    throwOnError?: boolean;
  }) => {
    if (saveState === "saving") {
      if (throwOnError) {
        throw new Error("Wait for the current save to finish.");
      }
      return;
    }
    if (saveResetTimerRef.current !== undefined) {
      window.clearTimeout(saveResetTimerRef.current);
      saveResetTimerRef.current = undefined;
    }
    setSaveState("saving");
    setError(undefined);
    try {
      const response = await requestManageFiles({
        action: "save",
        content,
        path,
        projectEditorId,
        projectId,
      });
      if (response.error) {
        throw new Error(response.error);
      }
      const savedFile = response.file;
      if (!savedFile) {
        throw new Error("Docs did not return saved file metadata.");
      }
      const savedContent = savedFile.content ?? content;
      /*
       * CDXC:ManageAutosave 2026-06-28-02:36:
       * Autosave may finish after another Markdown keystroke or Excalidraw gesture. Update file metadata and the saved baseline, but only replace editor content when the user has not changed the snapshot that was sent to native.
       */
      if (selectedPathRef.current === savedFile.path) {
        setPreview(savedFile);
        setDraftContent((currentContent) => (currentContent === content ? savedContent : currentContent));
        setLastSavedContent(savedContent);
      }
      setEntries((currentEntries) =>
        currentEntries.map((entry) =>
          entry.path === savedFile.path
            ? {
                ...entry,
                modifiedAt: savedFile.modifiedAt,
                size: savedFile.size,
              }
            : entry,
        ),
      );
      if (selectedPathRef.current === savedFile.path) {
        setSaveState("saved");
        saveResetTimerRef.current = window.setTimeout(() => {
          setSaveState("idle");
          saveResetTimerRef.current = undefined;
        }, 1_600);
      }
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : "Could not save file.";
      if (selectedPathRef.current === path) {
        setSaveState("error");
        setError(message);
      }
      if (throwOnError) {
        throw new Error(message);
      }
    }
  }, [projectEditorId, projectId, saveState]);

  const saveFile = useCallback(async () => {
    if (!selectedPath || !preview || preview.kind !== "text") {
      return;
    }
    await saveContentSnapshot({ content: draftContent, path: selectedPath });
  }, [draftContent, preview, saveContentSnapshot, selectedPath]);

  useEffect(() => {
    if (contentAutosaveTimerRef.current !== undefined) {
      window.clearTimeout(contentAutosaveTimerRef.current);
      contentAutosaveTimerRef.current = undefined;
    }
    if (
      !selectedPath ||
      !preview ||
      preview.kind !== "text" ||
      !isDirty ||
      saveState === "saving" ||
      !shouldAutosaveManageFile(selectedPath)
    ) {
      return;
    }
    const pathToSave = selectedPath;
    const contentToSave = draftContent;
    contentAutosaveTimerRef.current = window.setTimeout(() => {
      contentAutosaveTimerRef.current = undefined;
      void saveContentSnapshot({ content: contentToSave, path: pathToSave });
    }, MANAGE_CONTENT_AUTOSAVE_DELAY_MS);
    return () => {
      if (contentAutosaveTimerRef.current !== undefined) {
        window.clearTimeout(contentAutosaveTimerRef.current);
        contentAutosaveTimerRef.current = undefined;
      }
    };
  }, [draftContent, isDirty, preview, saveContentSnapshot, saveState, selectedPath]);

  const createArtifactFile = useCallback(
    async (kind: ManageArtifactKind, directoryPath = MANAGE_DOCS_ROOT_PATH) => {
      if (creatingArtifactKind || isCreatingFolder) {
        return;
      }
      const path = createUniqueArtifactPath(entries, kind, directoryPath);
      const content = createInitialArtifactContent(kind);
      setCreatingArtifactKind(kind);
      setFileOperation({ action: "createFile", path: directoryPath });
      setSaveState("saving");
      setError(undefined);
      try {
        const response = await requestManageFiles({
          action: "save",
          content,
          path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        const createdFile = response.file;
        if (!createdFile) {
          throw new Error("Docs did not return created file metadata.");
        }
        selectedPathRef.current = createdFile.path;
        setSelectedPath(createdFile.path);
        setPreview(createdFile);
        const nextContent = createdFile.content ?? content;
        setDraftContent(nextContent);
        setLastSavedContent(nextContent);
        setPreviewState("ready");
        setSaveState("saved");
        if (saveResetTimerRef.current !== undefined) {
          window.clearTimeout(saveResetTimerRef.current);
        }
        saveResetTimerRef.current = window.setTimeout(() => {
          setSaveState("idle");
          saveResetTimerRef.current = undefined;
        }, 1_600);
        setFileContextMenu(undefined);
        setCollapsedDirectoryPaths((current) => {
          const next = new Set(current);
          next.delete(directoryPath);
          return next;
        });
        await refreshFiles();
      } catch (createError) {
        setSaveState("error");
        setError(createError instanceof Error ? createError.message : "Could not create document.");
      } finally {
        setCreatingArtifactKind(undefined);
        setFileOperation((current) =>
          current?.action === "createFile" && current.path === directoryPath ? undefined : current,
        );
      }
    },
    [creatingArtifactKind, entries, isCreatingFolder, projectEditorId, projectId, refreshFiles],
  );

  const createFolder = useCallback(async (directoryPath = MANAGE_DOCS_ROOT_PATH) => {
    if (creatingArtifactKind || isCreatingFolder) {
      return;
    }
    const path = createUniqueFolderPath(entries, directoryPath);
    setIsCreatingFolder(true);
    setFileOperation({ action: "createFolder", path: directoryPath });
    setError(undefined);
    try {
      const response = await requestManageFiles({
        action: "createFolder",
        path,
        projectEditorId,
        projectId,
      });
      if (response.error) {
        throw new Error(response.error);
      }
      setCollapsedDirectoryPaths((current) => {
        const next = new Set(current);
        next.delete(path);
        next.delete(directoryPath);
        return next;
      });
      setFileContextMenu(undefined);
      await refreshFiles();
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "Could not create folder.");
    } finally {
      setIsCreatingFolder(false);
      setFileOperation((current) =>
        current?.action === "createFolder" && current.path === directoryPath ? undefined : current,
      );
    }
  }, [creatingArtifactKind, entries, isCreatingFolder, projectEditorId, projectId, refreshFiles]);

  const clearPendingContentAutosave = useCallback(() => {
    if (contentAutosaveTimerRef.current !== undefined) {
      window.clearTimeout(contentAutosaveTimerRef.current);
      contentAutosaveTimerRef.current = undefined;
    }
  }, []);

  const startRenameFile = useCallback((entry: ManageFileEntry) => {
    setFileContextMenu(undefined);
    setRenameDialog({
      path: entry.path,
      value: entry.name,
    });
  }, []);

  const renameFile = useCallback(
    async (path: string, nextNameInput: string) => {
      const currentEntry = entries.find((entry) => entry.path === path);
      if (!currentEntry) {
        setRenameDialog((current) =>
          current?.path === path ? { ...current, error: "This item is no longer available." } : current,
        );
        return;
      }
      const nextName = nextNameInput.trim();
      const validationError = validateManageRenameFileName(nextName);
      if (validationError) {
        setRenameDialog((current) =>
          current?.path === path ? { ...current, error: validationError } : current,
        );
        return;
      }
      const nextPath = renameManageFilePath(path, nextName);
      if (nextPath === path) {
        setRenameDialog(undefined);
        return;
      }
      if (
        entries.some(
          (entry) =>
            entry.path !== path &&
            entry.path.toLocaleLowerCase() === nextPath.toLocaleLowerCase(),
        )
      ) {
        setRenameDialog((current) =>
          current?.path === path ? { ...current, error: "A file or folder with that name already exists." } : current,
        );
        return;
      }
      const selectedPathBeforeRename = selectedPathRef.current;
      const renamedSelectedPath =
        selectedPathBeforeRename && remapManagePathByMove(selectedPathBeforeRename, path, nextPath);
      if (renamedSelectedPath && saveState === "saving") {
        setRenameDialog((current) =>
          current?.path === path
            ? { ...current, error: "Wait for the current save to finish before renaming." }
            : current,
        );
        return;
      }
      if (currentEntry.kind === "directory" && renamedSelectedPath && isDirty) {
        setRenameDialog((current) =>
          current?.path === path
            ? { ...current, error: "Save the current file before renaming its folder." }
            : current,
        );
        return;
      }
      setFileOperation({ action: "rename", path });
      setError(undefined);
      try {
        if (selectedPathRef.current === path && isDirty) {
          clearPendingContentAutosave();
        }
        const response = await requestManageFiles({
          action: "rename",
          newPath: nextPath,
          path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        const renamedFile = response.file;
        if (currentEntry.kind === "file" && !renamedFile) {
          throw new Error("Docs did not return renamed file metadata.");
        }
        setAnnotationsByPath((current) => remapManageAnnotationPathsForMove(current, path, nextPath));
        setCollapsedDirectoryPaths((current) => remapManagePathSetForMove(current, path, nextPath));
        if (currentEntry.kind === "file" && renamedFile && selectedPathRef.current === path) {
          selectedPathRef.current = renamedFile.path;
          setSelectedPath(renamedFile.path);
          setPreview(renamedFile);
          const savedContent = renamedFile.content ?? "";
          const nextContent = isDirty ? draftContent : savedContent;
          setDraftContent(nextContent);
          setLastSavedContent(savedContent);
          setPreviewState("ready");
          setSaveState("idle");
        }
        setRenameDialog(undefined);
        await refreshFiles();
        if (currentEntry.kind === "directory" && renamedSelectedPath) {
          selectedPathRef.current = renamedSelectedPath;
          setSelectedPath(renamedSelectedPath);
          await readFile(renamedSelectedPath);
        }
      } catch (renameError) {
        const message = renameError instanceof Error ? renameError.message : "Could not rename item.";
        setRenameDialog((current) => (current?.path === path ? { ...current, error: message } : current));
        setError(message);
      } finally {
        setFileOperation((current) =>
          current?.action === "rename" && current.path === path ? undefined : current,
        );
      }
    },
    [
      clearPendingContentAutosave,
      draftContent,
      entries,
      isDirty,
      projectEditorId,
      projectId,
      readFile,
      refreshFiles,
      saveState,
    ],
  );

  const deleteFile = useCallback(
    async (path: string) => {
      const currentEntry = entries.find((entry) => entry.path === path);
      if (!currentEntry || fileOperation) {
        return;
      }
      const selectedPathBeforeDelete = selectedPathRef.current;
      const deletesSelectedPath =
        selectedPathBeforeDelete === path ||
        (currentEntry.kind === "directory" &&
          selectedPathBeforeDelete !== undefined &&
          isManageDescendantPath(selectedPathBeforeDelete, path));
      if (currentEntry.kind === "directory" && deletesSelectedPath && (isDirty || saveState === "saving")) {
        setError("Save the current file before deleting its folder.");
        return;
      }
      setFileOperation({ action: "delete", path });
      setError(undefined);
      if (deletesSelectedPath) {
        clearPendingContentAutosave();
      }
      try {
        const response = await requestManageFiles({
          action: "delete",
          path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        setAnnotationsByPath((current) => removeManageAnnotationPathsForDeletedEntry(current, path));
        setCollapsedDirectoryPaths((current) => removeManagePathSetForDeletedEntry(current, path));
        setFileContextMenu(undefined);
        if (deletesSelectedPath) {
          selectedPathRef.current = undefined;
          setSelectedPath(undefined);
          setPreview(undefined);
          setDraftContent("");
          setLastSavedContent("");
          setPreviewState("idle");
          setSaveState("idle");
        }
        await refreshFiles();
      } catch (deleteError) {
        setError(deleteError instanceof Error ? deleteError.message : "Could not delete item.");
      } finally {
        setFileOperation((current) =>
          current?.action === "delete" && current.path === path ? undefined : current,
        );
      }
    },
    [
      clearPendingContentAutosave,
      entries,
      fileOperation,
      isDirty,
      projectEditorId,
      projectId,
      refreshFiles,
      saveState,
    ],
  );

  const duplicateFile = useCallback(
    async (entry: ManageFileEntry) => {
      if (entry.kind !== "file" || fileOperation) {
        return;
      }
      const nextPath = createDuplicateManageFilePath(entries, entry.path);
      setFileOperation({ action: "duplicate", path: entry.path });
      setError(undefined);
      try {
        if (selectedPathRef.current === entry.path && isDirty) {
          clearPendingContentAutosave();
          await saveContentSnapshot({
            content: draftContent,
            path: entry.path,
            throwOnError: true,
          });
        }
        const response = await requestManageFiles({
          action: "duplicate",
          newPath: nextPath,
          path: entry.path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        const duplicatedFile = response.file;
        if (!duplicatedFile) {
          throw new Error("Docs did not return duplicated file metadata.");
        }
        setFileContextMenu(undefined);
        setCollapsedDirectoryPaths((current) => {
          const next = new Set(current);
          next.delete(parentManagePath(duplicatedFile.path));
          return next;
        });
        await refreshFiles();
        selectedPathRef.current = duplicatedFile.path;
        setSelectedPath(duplicatedFile.path);
        setPreview(duplicatedFile);
        const nextContent = duplicatedFile.content ?? "";
        setDraftContent(nextContent);
        setLastSavedContent(nextContent);
        setPreviewState("ready");
        setSaveState("idle");
      } catch (duplicateError) {
        setError(duplicateError instanceof Error ? duplicateError.message : "Could not duplicate file.");
      } finally {
        setFileOperation((current) =>
          current?.action === "duplicate" && current.path === entry.path ? undefined : current,
        );
      }
    },
    [
      clearPendingContentAutosave,
      draftContent,
      entries,
      fileOperation,
      isDirty,
      projectEditorId,
      projectId,
      refreshFiles,
      saveContentSnapshot,
    ],
  );

  const addFileToSessionContext = useCallback(
    async (entry: ManageFileEntry) => {
      if (entry.kind !== "file" || fileOperation) {
        return;
      }
      setFileOperation({ action: "addToSessionContext", path: entry.path });
      setError(undefined);
      try {
        if (selectedPathRef.current === entry.path && isDirty) {
          clearPendingContentAutosave();
          await saveContentSnapshot({
            content: draftContent,
            path: entry.path,
            throwOnError: true,
          });
        }
        const response = await requestManageFiles({
          action: "addToSessionContext",
          path: entry.path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        setFileContextMenu(undefined);
      } catch (contextError) {
        setError(contextError instanceof Error ? contextError.message : "Could not add file to session context.");
      } finally {
        setFileOperation((current) =>
          current?.action === "addToSessionContext" && current.path === entry.path ? undefined : current,
        );
      }
    },
    [
      clearPendingContentAutosave,
      draftContent,
      fileOperation,
      isDirty,
      projectEditorId,
      projectId,
      saveContentSnapshot,
    ],
  );

  const moveEntryToDirectory = useCallback(
    async (entry: ManageFileEntry, targetDirectoryPath: string) => {
      if (fileOperation) {
        return;
      }
      const nextPath = moveManagePathToDirectory(entry.path, targetDirectoryPath);
      if (!nextPath || nextPath === entry.path) {
        return;
      }
      if (
        entries.some(
          (candidate) =>
            candidate.path !== entry.path &&
            candidate.path.toLocaleLowerCase() === nextPath.toLocaleLowerCase(),
        )
      ) {
        setError("A file or folder with that name already exists.");
        return;
      }
      const selectedPathBeforeMove = selectedPathRef.current;
      const movedSelectedPath =
        selectedPathBeforeMove && remapManagePathByMove(selectedPathBeforeMove, entry.path, nextPath);
      if (movedSelectedPath && (isDirty || saveState === "saving")) {
        setError("Save the current file before moving it.");
        return;
      }
      setFileOperation({ action: "move", path: entry.path });
      setDropTarget(undefined);
      setError(undefined);
      try {
        const response = await requestManageFiles({
          action: "move",
          newPath: nextPath,
          path: entry.path,
          projectEditorId,
          projectId,
        });
        if (response.error) {
          throw new Error(response.error);
        }
        setAnnotationsByPath((current) => remapManageAnnotationPathsForMove(current, entry.path, nextPath));
        setCollapsedDirectoryPaths((current) => remapManagePathSetForMove(current, entry.path, nextPath));
        if (movedSelectedPath) {
          selectedPathRef.current = movedSelectedPath;
          setSelectedPath(movedSelectedPath);
        }
        await refreshFiles();
        if (movedSelectedPath) {
          await readFile(movedSelectedPath);
        }
      } catch (moveError) {
        setError(moveError instanceof Error ? moveError.message : "Could not move item.");
      } finally {
        setFileOperation((current) =>
          current?.action === "move" && current.path === entry.path ? undefined : current,
        );
      }
    },
    [entries, fileOperation, isDirty, projectEditorId, projectId, readFile, refreshFiles, saveState],
  );

  const submitRenameDialog = useCallback(() => {
    if (!renameDialog) {
      return;
    }
    void renameFile(renameDialog.path, renameDialog.value);
  }, [renameDialog, renameFile]);

  const toggleDirectory = useCallback((path: string) => {
    setCollapsedDirectoryPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const clearDragState = useCallback(() => {
    setDragState(undefined);
    setDropTarget(undefined);
  }, []);

  const startEntryDrag = useCallback((entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData(MANAGE_DRAG_DATA_TYPE, entry.path);
    event.dataTransfer.setData("text/plain", entry.path);
    setDragState({ kind: entry.kind, path: entry.path });
    setDropTarget(undefined);
  }, []);

  const dragEntry = useMemo(
    () => (dragState ? entries.find((entry) => entry.path === dragState.path) : undefined),
    [dragState, entries],
  );

  const updateEntryDropTarget = useCallback(
    (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => {
      const targetDirectoryPath = dropDirectoryPathForManageEntry(entry);
      if (
        !dragEntry ||
        !targetDirectoryPath ||
        !canMoveManageEntryToDirectory(dragEntry, targetDirectoryPath, entries)
      ) {
        if (dragEntry) {
          setDropTarget(undefined);
        }
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      event.dataTransfer.dropEffect = "move";
      setDropTarget({ kind: "entry", path: entry.path, targetDirectoryPath });
    },
    [dragEntry, entries],
  );

  const dropOnEntry = useCallback(
    (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => {
      const targetDirectoryPath = dropDirectoryPathForManageEntry(entry);
      if (
        !dragEntry ||
        !targetDirectoryPath ||
        !canMoveManageEntryToDirectory(dragEntry, targetDirectoryPath, entries)
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      clearDragState();
      void moveEntryToDirectory(dragEntry, targetDirectoryPath);
    },
    [clearDragState, dragEntry, entries, moveEntryToDirectory],
  );

  const updateRootDropTarget = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      if (!dragEntry) {
        return;
      }
      const target = event.target;
      if (target instanceof Element && target.closest(".manage-file-row")) {
        return;
      }
      if (!canMoveManageEntryToDirectory(dragEntry, MANAGE_DOCS_ROOT_PATH, entries)) {
        return;
      }
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      setDropTarget({ kind: "root", path: MANAGE_DOCS_ROOT_PATH });
    },
    [dragEntry, entries],
  );

  const dropOnRoot = useCallback(
    (event: ReactDragEvent<HTMLElement>) => {
      if (!dragEntry || dropTarget?.kind !== "root") {
        return;
      }
      const target = event.target;
      if (target instanceof Element && target.closest(".manage-file-row")) {
        return;
      }
      if (!canMoveManageEntryToDirectory(dragEntry, MANAGE_DOCS_ROOT_PATH, entries)) {
        return;
      }
      event.preventDefault();
      clearDragState();
      void moveEntryToDirectory(dragEntry, MANAGE_DOCS_ROOT_PATH);
    },
    [clearDragState, dragEntry, dropTarget, entries, moveEntryToDirectory],
  );

  const handleSidebarDragLeave = useCallback((event: ReactDragEvent<HTMLElement>) => {
    const relatedTarget = event.relatedTarget;
    if (relatedTarget instanceof Node && event.currentTarget.contains(relatedTarget)) {
      return;
    }
    setDropTarget(undefined);
  }, []);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLocaleLowerCase() === "s") {
        if (!selectedPath || !isDirty) {
          return;
        }
        event.preventDefault();
        void saveFile();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isDirty, saveFile, selectedPath]);

  const directoryPathsWithChildren = useMemo(() => {
    const paths = new Set<string>();
    for (const entry of entries) {
      const parentPath = parentManagePath(entry.path);
      if (parentPath) {
        paths.add(parentPath);
      }
    }
    return paths;
  }, [entries]);

  const treeOrderedEntries = useMemo(() => orderManageEntriesForTree(entries), [entries]);
  const expandableDirectoryPaths = useMemo(() => {
    const paths = new Set<string>();
    for (const entry of entries) {
      if (entry.kind === "directory" && directoryPathsWithChildren.has(entry.path)) {
        paths.add(entry.path);
      }
    }
    return paths;
  }, [directoryPathsWithChildren, entries]);
  const hasExpandableDirectories = expandableDirectoryPaths.size > 0;
  const hasExpandedDirectories = useMemo(() => {
    for (const path of expandableDirectoryPaths) {
      if (!collapsedDirectoryPaths.has(path)) {
        return true;
      }
    }
    return false;
  }, [collapsedDirectoryPaths, expandableDirectoryPaths]);
  const toggleAllDirectories = useCallback(() => {
    setCollapsedDirectoryPaths((current) => {
      for (const path of expandableDirectoryPaths) {
        if (!current.has(path)) {
          return new Set(expandableDirectoryPaths);
        }
      }
      return new Set();
    });
  }, [expandableDirectoryPaths]);

  const visibleEntries = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) {
      return treeOrderedEntries.filter((entry) => !hasCollapsedManageAncestor(entry.path, collapsedDirectoryPaths));
    }
    return filterManageEntriesForSearch(treeOrderedEntries, normalizedQuery);
  }, [collapsedDirectoryPaths, query, treeOrderedEntries]);
  const isFileSearchActive = query.trim().length > 0;

  const contextMenuEntry = fileContextMenu
    ? entries.find((entry) => entry.path === fileContextMenu.path)
    : undefined;
  const contextMenuOperation =
    contextMenuEntry && fileOperation?.path === contextMenuEntry.path ? fileOperation.action : undefined;
  const contextMenuCanRenameOrDelete =
    contextMenuEntry !== undefined && canRenameOrDeleteManageEntry(contextMenuEntry);
  const contextMenuCanCreateHere =
    contextMenuEntry !== undefined && canCreateManageEntryChildren(contextMenuEntry);

  useEffect(() => {
    if (fileContextMenu && !entries.some((entry) => entry.path === fileContextMenu.path)) {
      setFileContextMenu(undefined);
    }
  }, [entries, fileContextMenu]);

  const updateAnnotationsForSelectedFile = useCallback(
    (updater: (annotations: ManageAnnotation[]) => ManageAnnotation[]) => {
      if (!selectedPath) {
        return;
      }
      setAnnotationsByPath((current) => {
        const nextAnnotations = updater(current[selectedPath] ?? []);
        if (nextAnnotations.length === 0) {
          const { [selectedPath]: _removed, ...remaining } = current;
          return remaining;
        }
        return {
          ...current,
          [selectedPath]: nextAnnotations,
        };
      });
    },
    [selectedPath],
  );

  return (
    <main
      className="manage-shell"
      data-sidebar-floating={String(sidebarFloating)}
      data-sidebar-hidden={String(sidebarHidden)}
      data-sidebar-side={sidebarSide}
      ref={shellRef}
      style={{ "--manage-sidebar-width": `${sidebarWidth}px` } as CSSProperties}
    >
      {!sidebarHidden ? (
        <aside
          className="manage-sidebar"
          data-drag-active={String(Boolean(dragEntry))}
          onContextMenu={suppressSidebarDefaultContextMenu}
          onDragLeave={handleSidebarDragLeave}
          onDragOver={updateRootDropTarget}
          onDrop={dropOnRoot}
          ref={sidebarRef}
        >
          <div
            className="manage-sidebar-header"
            data-root-drop-target={String(dropTarget?.kind === "root")}
          >
            <ManageSidebarActions
              creatingKind={creatingArtifactKind}
              isRefreshing={listState === "loading"}
              isCreatingFolder={isCreatingFolder}
              hasExpandableDirectories={hasExpandableDirectories}
              hasExpandedDirectories={hasExpandedDirectories}
              onCreate={(kind) => void createArtifactFile(kind)}
              onCreateFolder={() => void createFolder()}
              onHideSidebar={() => setSidebarHidden(true)}
              onOpenDocsFoldersSettings={() => void openDocsFoldersSettings()}
              onRefresh={() => void refreshFiles()}
              onSwitchSide={switchSidebarSide}
              onToggleAllDirectories={toggleAllDirectories}
              sidebarSide={sidebarSide}
            />
          </div>
          <div
            className="manage-search"
            onMouseDown={(event) => {
              if (event.target instanceof Element && event.target.closest(".manage-search-clear-button")) {
                return;
              }
              searchInputRef.current?.focus({ preventScroll: true });
            }}
          >
            <IconSearch aria-hidden="true" size={15} stroke={1.8} />
            <input
              aria-label="Search files"
              onChange={(event) => setQuery(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key !== "Escape") {
                  return;
                }
                event.preventDefault();
                event.stopPropagation();
                setQuery("");
                searchInputRef.current?.focus({ preventScroll: true });
              }}
              placeholder="Search"
              ref={searchInputRef}
              value={query}
            />
            {query.length > 0 ? (
              <ManageTooltipButton
                aria-label="Clear file search"
                className="manage-search-clear-button"
                onClick={() => {
                  setQuery("");
                  searchInputRef.current?.focus({ preventScroll: true });
                }}
                tooltip="Clear file search"
                type="button"
              >
                <IconX aria-hidden="true" size={14} stroke={1.8} />
              </ManageTooltipButton>
            ) : null}
          </div>
          <div
            className="manage-file-list"
            data-root-drop-target={String(dropTarget?.kind === "root")}
            role="tree"
          >
            {listState === "loading" && entries.length === 0 ? (
              <ManageEmptyState icon={<IconRefresh aria-hidden="true" size={18} />} text="Loading files" />
            ) : null}
            {listState !== "loading" && visibleEntries.length === 0 ? (
              <ManageEmptyState icon={<IconSearch aria-hidden="true" size={18} />} text="No files found" />
            ) : null}
            {visibleEntries.map((entry) => (
              <ManageFileRow
                annotationCount={annotationCountsByPath.get(entry.path) ?? 0}
                isContextMenuOpen={fileContextMenu?.path === entry.path}
                hasChildren={directoryPathsWithChildren.has(entry.path)}
                entry={entry}
                hasActiveFileDescendant={
                  entry.kind === "directory" && selectedPath !== undefined && isManageDescendantPath(selectedPath, entry.path)
                }
                isDragging={dragState?.path === entry.path}
                isDropTarget={dropTarget?.kind === "entry" && dropTarget.path === entry.path}
                isExpanded={isFileSearchActive || !collapsedDirectoryPaths.has(entry.path)}
                isSelected={entry.path === selectedPath}
                key={entry.path}
                canOpenContextMenu={canOpenManageEntryContextMenu(entry)}
                onEntryDragOver={updateEntryDropTarget}
                onEntryDrop={dropOnEntry}
                onDragEnd={clearDragState}
                onDragStart={startEntryDrag}
                onOpenContextMenu={openFileContextMenu}
                onSelect={() => {
                  if (entry.kind === "file") {
                    void readFile(entry.path);
                    return;
                  }
                  if (entry.kind === "directory" && directoryPathsWithChildren.has(entry.path)) {
                    toggleDirectory(entry.path);
                  }
                }}
              />
            ))}
          </div>
        </aside>
      ) : (
        <button
          aria-label="Show file sidebar"
          className="manage-sidebar-restore-button manage-icon-button"
          onClick={() => setSidebarHidden(false)}
          type="button"
        >
          {sidebarSide === "right" ? (
            <IconLayoutSidebarRightExpand aria-hidden="true" size={16} stroke={1.8} />
          ) : (
            <IconLayoutSidebarLeftExpand aria-hidden="true" size={16} stroke={1.8} />
          )}
        </button>
      )}
      {!sidebarHidden && !sidebarFloating ? (
        <AppTooltip content="Resize file sidebar">
          <div
            aria-label="Resize file sidebar"
            aria-orientation="vertical"
            aria-valuemax={MANAGE_SIDEBAR_MAX_WIDTH}
            aria-valuemin={MANAGE_SIDEBAR_MIN_WIDTH}
            aria-valuenow={Math.round(sidebarWidth)}
            className="manage-sidebar-resizer"
            onKeyDown={handleSidebarResizeKeyDown}
            onPointerDown={handleSidebarResizePointerDown}
            role="separator"
            tabIndex={0}
          />
        </AppTooltip>
      ) : null}
      <section className="manage-preview">
        <ManagePreview
          annotations={annotationsForSelectedPath}
          annotationPersistenceState={annotationPersistenceState}
          draftContent={draftContent}
          error={error}
          isDirty={isDirty}
          hasExternalChanges={hasExternalChanges}
          onAnnotationsChange={updateAnnotationsForSelectedFile}
          onDraftContentChange={setDraftContent}
          onOpenDocument={(path) => void readFile(path)}
          onReload={() => {
            if (selectedPath) {
              void readFile(selectedPath);
            }
          }}
          preview={preview}
          previewState={previewState}
          saveState={saveState}
          selectedPath={selectedPath}
        />
      </section>
      {fileContextMenu && contextMenuEntry ? (
        <ManageFileContextMenu
          canAddToSessionContext={contextMenuEntry.kind === "file"}
          canCreateHere={contextMenuCanCreateHere}
          canDuplicate={contextMenuEntry.kind === "file"}
          canRenameOrDelete={contextMenuCanRenameOrDelete}
          confirmingDelete={fileContextMenu.confirmingDelete === true}
          creatingKind={contextMenuCanCreateHere ? creatingArtifactKind : undefined}
          isCreatingFolder={
            contextMenuCanCreateHere &&
            fileOperation?.action === "createFolder" &&
            fileOperation.path === contextMenuEntry.path
          }
          onAddToSessionContext={() => void addFileToSessionContext(contextMenuEntry)}
          onCopyFullPath={() => void copyEntryFullPath(contextMenuEntry)}
          onCopyPath={() => void copyEntryPath(contextMenuEntry)}
          onCreateFileHere={(kind) => {
            if (contextMenuCanCreateHere) {
              void createArtifactFile(kind, contextMenuEntry.path);
            }
          }}
          onCreateFolderHere={() => {
            if (contextMenuCanCreateHere) {
              void createFolder(contextMenuEntry.path);
            }
          }}
          onDuplicate={() => void duplicateFile(contextMenuEntry)}
          onDelete={() => {
            if (!contextMenuCanRenameOrDelete) {
              return;
            }
            if (!fileContextMenu.confirmingDelete) {
              setFileContextMenu((current) =>
                current?.path === contextMenuEntry.path
                  ? {
                      ...current,
                      confirmingDelete: true,
                    }
                  : current,
              );
              return;
            }
            void deleteFile(contextMenuEntry.path);
          }}
          onDismiss={dismissFileContextMenu}
          onRename={() => {
            if (contextMenuCanRenameOrDelete) {
              startRenameFile(contextMenuEntry);
            }
          }}
          onRevealInFinder={() => void revealEntryInFinder(contextMenuEntry)}
          pendingAction={contextMenuOperation}
          position={fileContextMenu}
        />
      ) : null}
      {renameDialog ? (
        <ManageRenameDialog
          error={renameDialog.error}
          isRenaming={fileOperation?.action === "rename" && fileOperation.path === renameDialog.path}
          onCancel={() => setRenameDialog(undefined)}
          onChange={(value) =>
            setRenameDialog((current) =>
              current ? { ...current, error: undefined, value } : current,
            )
          }
          onSubmit={submitRenameDialog}
          value={renameDialog.value}
        />
      ) : null}
    </main>
  );
}

export function requestManageFiles(
  request: Omit<ManageFilesBridgeRequest, "requestId">,
): Promise<ManageFilesBridgeResponse> {
  const bridge = (window as ManageWebKitWindow).webkit?.messageHandlers?.ghostexManageFiles;
  if (!bridge) {
    return Promise.reject(new Error("Docs is unavailable in this host."));
  }
  return requestProjectDocsFromHost(request, {
    eventName: MANAGE_FILES_RESPONSE_EVENT,
    eventTarget: window,
    postMessage: (message) => bridge.postMessage(message),
    timeoutMs: MANAGE_BRIDGE_TIMEOUT_MS,
  });
}

export function readStoredManageSidebarSide(): ManageSidebarSide {
  return window.localStorage.getItem(MANAGE_SIDEBAR_SIDE_STORAGE_KEY) === "left" ? "left" : "right";
}

export function readStoredManageSidebarWidth(): number {
  const parsedWidth = Number(window.localStorage.getItem(MANAGE_SIDEBAR_WIDTH_STORAGE_KEY));
  return clampManageSidebarWidth(
    Number.isFinite(parsedWidth) && parsedWidth > 0 ? parsedWidth : MANAGE_SIDEBAR_DEFAULT_WIDTH,
    window.innerWidth,
  );
}

export function clampManageSidebarWidth(width: number, containerWidth: number): number {
  const maxForContainer = Math.max(
    MANAGE_SIDEBAR_MIN_WIDTH,
    Math.min(MANAGE_SIDEBAR_MAX_WIDTH, Math.floor(containerWidth * 0.46)),
  );
  return Math.min(Math.max(Math.round(width), MANAGE_SIDEBAR_MIN_WIDTH), maxForContainer);
}
