import { Excalidraw } from "@excalidraw/excalidraw";
import "@excalidraw/excalidraw/index.css";
import { StateEffect, StateField, RangeSetBuilder, type Extension } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type {
  AppState,
  BinaryFiles,
  ExcalidrawImperativeAPI,
} from "@excalidraw/excalidraw/types";
import {
  IconAlertTriangle,
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconCheck,
  IconChevronRight,
  IconCircleCheck,
  IconCopy,
  IconCopyPlus,
  IconEdit,
  IconFile,
  IconFileText,
  IconFileTypeHtml,
  IconFolder,
  IconFolderOpen,
  IconFolderPlus,
  IconHelpCircle,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconMarkdown,
  IconLayoutSidebarRightCollapse,
  IconLayoutSidebarRightExpand,
  IconMenu2,
  IconMessagePlus,
  IconMessages,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconSettings,
  IconTestPipe,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import {
  Bold as MeoBoldIcon,
  Brackets as MeoBracketsIcon,
  CaseSensitive as MeoCaseSensitiveIcon,
  ChevronDown as MeoChevronDownIcon,
  ChevronUp as MeoChevronUpIcon,
  Code as MeoCodeIcon,
  GitCompare as MeoGitCompareIcon,
  Hash as MeoHashIcon,
  Heading as MeoHeadingIcon,
  Heading1 as MeoHeading1Icon,
  Heading2 as MeoHeading2Icon,
  Heading3 as MeoHeading3Icon,
  Heading4 as MeoHeading4Icon,
  Heading5 as MeoHeading5Icon,
  Heading6 as MeoHeading6Icon,
  Image as MeoImageIcon,
  Italic as MeoItalicIcon,
  Keyboard as MeoKeyboardIcon,
  Link as MeoLinkIcon,
  List as MeoListIcon,
  ListOrdered as MeoListOrderedIcon,
  ListTodo as MeoListTodoIcon,
  Minus as MeoMinusIcon,
  PanelLeftRightDashed as MeoPanelLeftRightDashedIcon,
  Quote as MeoQuoteIcon,
  Replace as MeoReplaceIcon,
  ReplaceAll as MeoReplaceAllIcon,
  Search as MeoSearchIcon,
  Strikethrough as MeoStrikethroughIcon,
  Table2 as MeoTable2Icon,
  Terminal as MeoTerminalIcon,
  WholeWord as MeoWholeWordIcon,
  X as MeoXIcon,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type CSSProperties,
  type DragEvent as ReactDragEvent,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { createRoot } from "react-dom/client";
import { AppTooltip, TooltipProvider } from "@/packages/core-ui/app-tooltip";
import { SidebarContextMenuPortal } from "@/packages/core-ui/sidebar-context-menu-portal";
import "@/packages/core-ui/styles/session-overlays.css";
import {
  requestProjectDocsFromHost,
  type ProjectDocsFileEntry as ManageFileEntry,
  type ProjectDocsFilePreview as ManageFilePreview,
  type ProjectDocsGitBaseline as ManageGitBaseline,
  type ProjectDocsGitBaselineReason as ManageGitBaselineReason,
  type ProjectDocsRequest as ManageFilesBridgeRequest,
  type ProjectDocsResponse as ManageFilesBridgeResponse,
} from "@/packages/shared/project-docs";
import { createEditor as createMeoEditor } from "./meo/editor";
import { applyThemeSettings as applyMeoThemeSettings } from "./meo/helpers/theme";
import "./meo/styles.css";

type ManageAnnotationType = "comment" | "redline";

type ManageAnnotationScope = "global" | "selection";

type ManageQuickLabelId = "clarify" | "needs-tests" | "looks-good";

type ManageQuickLabel = {
  color: string;
  id: ManageQuickLabelId;
  text: string;
};

type ManageTooltipButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  tooltip: ReactNode;
};

function ManageTooltipButton({ tooltip, ...buttonProps }: ManageTooltipButtonProps) {
  return (
    <AppTooltip content={tooltip}>
      <button {...buttonProps} />
    </AppTooltip>
  );
}

type ManageAnnotationImage = {
  dataUrl: string;
  id: string;
  mimeType: string;
  name: string;
  size: number;
};

type ManageAnnotation = {
  attachments: ManageAnnotationImage[];
  createdAt: string;
  id: string;
  labelId?: ManageQuickLabelId;
  note: string;
  quote: string;
  scope: ManageAnnotationScope;
  type: ManageAnnotationType;
};

type ManageAnnotationStore = {
  annotationsByPath: Record<string, ManageAnnotation[]>;
  updatedAt: string;
  version: 1;
};

type ManageSelectionAnchor = {
  left: number;
  top: number;
};

type ManageCapturedSelection = {
  anchor: ManageSelectionAnchor;
  text: string;
};

type ManageAnnotationPreview = {
  anchor: ManageSelectionAnchor;
  annotation: ManageAnnotation;
};

type ManageCommentDraft = {
  anchor: ManageSelectionAnchor;
  attachmentError: string;
  attachments: ManageAnnotationImage[];
  note: string;
  quote: string;
};

type ManageSidebarSide = "left" | "right";

type ManageArtifactKind = "excalidraw" | "html" | "markdown";

type ManageFileContextMenuState = {
  confirmingDelete?: boolean;
  path: string;
  x: number;
  y: number;
};

type ManageFileOperationState = {
  action:
    | "addToSessionContext"
    | "copyFullPath"
    | "createFile"
    | "createFolder"
    | "delete"
    | "duplicate"
    | "move"
    | "rename"
    | "revealInFinder";
  path: string;
};

type ManageDragState = {
  kind: ManageFileEntry["kind"];
  path: string;
};

type ManageDropTarget =
  | {
      kind: "entry";
      path: string;
      targetDirectoryPath: string;
    }
  | {
      kind: "root";
      path: typeof MANAGE_DOCS_ROOT_PATH;
    };

type ManageRenameDialogState = {
  error?: string;
  path: string;
  value: string;
};

type ManageMarkdownAlertKind = "caution" | "important" | "note" | "tip" | "warning";

type ManageMarkdownBlock = {
  alertKind?: ManageMarkdownAlertKind;
  checked?: boolean;
  content: string;
  directiveKind?: string;
  id: string;
  language?: string;
  level?: number;
  order: number;
  ordered?: boolean;
  orderedIndex?: number;
  orderedStart?: number;
  startLine: number;
  type: "blockquote" | "code" | "directive" | "heading" | "hr" | "html" | "list-item" | "paragraph" | "table";
};

type ManageMeoEditor = {
  countMatches?: (query: string, options?: { caseSensitive?: boolean; wholeWord?: boolean }) => number;
  destroy: () => void;
  findNext?: (
    query: string,
    options?: { caseSensitive?: boolean; focusEditor?: boolean; wholeWord?: boolean },
  ) => { current?: number; found?: boolean; total?: number } | null;
  findPrevious?: (
    query: string,
    options?: { caseSensitive?: boolean; focusEditor?: boolean; wholeWord?: boolean },
  ) => { current?: number; found?: boolean; total?: number } | null;
  focus: () => void;
  getText: () => string;
  insertFormat: (action: string, level?: number | { cols?: number; rows?: number }) => void;
  refreshLayout?: () => void;
  replaceAll?: (
    query: string,
    replacement: string,
    options?: { caseSensitive?: boolean; wholeWord?: boolean },
  ) => { replaced?: number; total?: number };
  replaceCurrent?: (
    query: string,
    replacement: string,
    options?: { caseSensitive?: boolean; wholeWord?: boolean },
  ) => { current?: number; found?: boolean; replaced?: boolean; total?: number };
  setGitBaseline?: (snapshot?: ManageGitBaseline | null) => void;
  setGitGutterVisible?: (visible: boolean) => void;
  setLineNumbers?: (visible: boolean) => void;
  setMode?: (mode: ManageMeoMode) => void;
  setSearchQuery?: (query: string, options?: { caseSensitive?: boolean; wholeWord?: boolean }) => void;
  setText: (text: string) => void;
  view: EditorView;
};

type ManageMeoMode = "live" | "source";

type ManageMeoSelectionState = {
  align?: "center" | "start";
  anchorBottomY?: number;
  anchorX?: number;
  anchorY?: number;
  from?: number;
  to?: number;
  visible?: boolean;
};

type ManageSelectionToolbarMode = "annotations" | "formatting";

type ManageMeoAnnotationDecoration = {
  from: number;
  labelId?: ManageQuickLabelId;
  to: number;
  type: ManageAnnotationType;
};

type ManageResolvedAnnotationRange = ManageMeoAnnotationDecoration & {
  annotation: ManageAnnotation;
};

type ManageWebKitWindow = Window & {
  ghostexGpui?: {
    manageDocsResourceBaseUrl?: string;
    postManageFilesRequest?: (payload: string) => boolean;
    supportsManageFileChangePolling?: boolean;
  };
  webkit?: {
    messageHandlers?: {
      ghostexManageFiles?: {
        postMessage: (message: ManageFilesBridgeRequest) => void;
      };
    };
  };
};

type ExcalidrawFileData = {
  appState?: Record<string, unknown>;
  elements?: readonly ExcalidrawElement[];
  files?: BinaryFiles;
  source?: string;
  type?: string;
  version?: number;
};

const MANAGE_FILES_RESPONSE_EVENT = "ghostex-manage-files-response";
const MANAGE_FILES_CHANGED_EVENT = "ghostex-manage-files-changed";
const MANAGE_DRAG_DATA_TYPE = "application/x-ghostex-manage-path";
const MANAGE_BRIDGE_TIMEOUT_MS = 15_000;
const MANAGE_DOCS_ROOT_PATH = "docs";
const MANAGE_SELECTION_MAX_LENGTH = 700;
const MANAGE_ANNOTATIONS_SIDECAR_PATH = ".ghostex/manage-annotations.json";
const MANAGE_ANNOTATION_SCHEMA_VERSION = 1;
const MANAGE_ANNOTATION_IMAGE_MAX_BYTES = 512 * 1024;
const MANAGE_ANNOTATION_MAX_IMAGES = 4;
/*
 * CDXC:ManageAutosave 2026-06-28-02:36:
 * Markdown and Excalidraw edits should persist automatically shortly after the user stops changing content because those artifact surfaces do not expose a visible Save button. Debounce saves for one second so normal typing and drawing gestures coalesce into a single bridge write.
 */
const MANAGE_CONTENT_AUTOSAVE_DELAY_MS = 1_000;
const MANAGE_GPUI_FILE_CHANGE_POLL_INTERVAL_MS = 400;
const MANAGE_GPUI_FILE_CHANGE_DEBOUNCE_MS = 500;
const MANAGE_SIDEBAR_DEFAULT_WIDTH = 292;
const MANAGE_SIDEBAR_MIN_WIDTH = 230;
const MANAGE_SIDEBAR_MAX_WIDTH = 560;
const MANAGE_FLOATING_SIDEBAR_MAX_WIDTH = 690;
const MANAGE_SIDEBAR_SIDE_STORAGE_KEY = "ghostex.manage.sidebarSide";
const MANAGE_SIDEBAR_WIDTH_STORAGE_KEY = "ghostex.manage.sidebarWidth";
/*
 * CDXC:ManageDrawings 2026-06-28-04:56:
 * Manage Excalidraw uses Excalidraw's dark theme, where the visually dark canvas is serialized as viewBackgroundColor #ffffff. Default new drawings to that saved value so created artifacts open with the same dark-looking background users get after choosing a dark canvas inside Excalidraw.
 */
const MANAGE_EXCALIDRAW_CANVAS_BACKGROUND = "#ffffff";
/*
 * CDXC:ManageDrawings 2026-06-28-01:43:
 * Manage should keep Excalidraw in dark mode so drawings match the macOS app's dark workarea instead of reopening through Excalidraw's light scheme. Apply the theme at the editor boundary so existing files and newly created artifacts render dark.
 */
const MANAGE_EXCALIDRAW_CANVAS_THEME: AppState["theme"] = "dark";
const MANAGE_COMMENT_ANNOTATION_COLOR = "#e2b340";
const MANAGE_REDLINE_ANNOTATION_COLOR = "#fda4af";
const MANAGE_DISMISS_TOOLBAR_COLOR = "#f87171";
const MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN = 18;
const MANAGE_SELECTION_TOOLBAR_WIDTH_ESTIMATE = 228;
const MANAGE_MEO_CONTENT_MAX_WIDTH = "800px";
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
const MANAGE_MEO_HEADING_COLOR = "#42a5f5";
const MANAGE_MEO_CODE_COLOR = "#f2b35f";
const MANAGE_MEO_VARIABLE_COLOR = "#e5e7eb";
const MANAGE_MEO_CODE_BLOCK_BACKGROUND = "#2a2d30";
const MANAGE_AGENTATION_VERSION = "3.0.2";
const MANAGE_AGENTATION_REACT_VERSION = "18.2.0";
const MANAGE_AGENTATION_PACKAGE_URL =
  `https://esm.sh/agentation@${MANAGE_AGENTATION_VERSION}?bundle&deps=react@${MANAGE_AGENTATION_REACT_VERSION},react-dom@${MANAGE_AGENTATION_REACT_VERSION}`;
const MANAGE_AGENTATION_REACT_URL = `https://esm.sh/react@${MANAGE_AGENTATION_REACT_VERSION}`;
const MANAGE_AGENTATION_REACT_DOM_CLIENT_URL =
  `https://esm.sh/react-dom@${MANAGE_AGENTATION_REACT_VERSION}/client?deps=react@${MANAGE_AGENTATION_REACT_VERSION}`;
const MANAGE_QUICK_LABELS: ManageQuickLabel[] = [
  { color: "#a78bfa", id: "clarify", text: "Clarify" },
  { color: "#f59e0b", id: "needs-tests", text: "Needs tests" },
  { color: "#86efac", id: "looks-good", text: "Looks good" },
];
const MANAGE_MEO_THEME = {
  backgroundColor: "#101112",
  colors: {
    base01: "#e5e7eb",
    base02: "#8b949e",
    base03: "#30363d",
    base04: MANAGE_MEO_HEADING_COLOR,
    base05: "#7dd3fc",
    base06: "#67e8f9",
    base07: "#fde68a",
    base08: "#c084fc",
    base09: "#86efac",
  },
  fonts: {
    liveFont: "",
    sourceFont: "",
    liveFontWeight: "450",
    sourceFontWeight: "450",
    liveFontSize: 14,
    sourceFontSize: 14,
    h1FontSize: 1.5,
    h1FontWeight: "720",
    h2FontSize: 1.35,
    h2FontWeight: "700",
    h3FontSize: 1.18,
    h3FontWeight: "700",
    h4FontSize: 1.08,
    h4FontWeight: "680",
    h5FontSize: 1,
    h5FontWeight: "660",
    h6FontSize: 0.94,
    h6FontWeight: "650",
    liveLineHeight: 1.55,
    sourceLineHeight: 1.55,
  },
  id: "ghostex-manage-meo",
  name: "Ghostex Docs Meo",
  syntaxTokens: {
    atom: MANAGE_MEO_VARIABLE_COLOR,
    bool: MANAGE_MEO_VARIABLE_COLOR,
    constant: MANAGE_MEO_VARIABLE_COLOR,
    definedVariable: MANAGE_MEO_VARIABLE_COLOR,
    monospace: MANAGE_MEO_CODE_COLOR,
    regexp: MANAGE_MEO_VARIABLE_COLOR,
    specialVariable: MANAGE_MEO_VARIABLE_COLOR,
    specialString: MANAGE_MEO_VARIABLE_COLOR,
    string: MANAGE_MEO_VARIABLE_COLOR,
    variableName: MANAGE_MEO_VARIABLE_COLOR,
  },
};
const manageMeoAnnotationEffect = StateEffect.define<ManageMeoAnnotationDecoration[]>();

const manageMeoAnnotationField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(value, transaction) {
    let nextValue = value.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(manageMeoAnnotationEffect)) {
        nextValue = buildManageMeoAnnotationDecorations(effect.value);
      }
    }
    return nextValue;
  },
  provide(field) {
    return EditorView.decorations.from(field);
  },
});

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
type ManageDocsOpenFileWindow = Window & {
  ghostexOpenDocsFile?: (path: unknown) => void;
};

let pendingManageDocsOpenPath: string | undefined;
let manageDocsOpenFileHandler: ((path: string) => void) | undefined;

function registerManageDocsOpenFileHandler(handler?: (path: string) => void): void {
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
function manageAncestorDirectoryPaths(path: string): string[] {
  const segments = path.split("/").filter((segment) => segment.length > 0);
  segments.pop();
  return segments.map((_, index) => segments.slice(0, index + 1).join("/"));
}

function ManageApp() {
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

function ManageSidebarActions({
  creatingKind,
  isRefreshing,
  isCreatingFolder,
  hasExpandableDirectories,
  hasExpandedDirectories,
  onCreate,
  onCreateFolder,
  onHideSidebar,
  onOpenDocsFoldersSettings,
  onRefresh,
  onSwitchSide,
  onToggleAllDirectories,
  sidebarSide,
}: {
  creatingKind?: ManageArtifactKind;
  isRefreshing: boolean;
  isCreatingFolder: boolean;
  hasExpandableDirectories: boolean;
  hasExpandedDirectories: boolean;
  onCreate: (kind: ManageArtifactKind) => void;
  onCreateFolder: () => void;
  onHideSidebar: () => void;
  onOpenDocsFoldersSettings: () => void;
  onRefresh: () => void;
  onSwitchSide: () => void;
  onToggleAllDirectories: () => void;
  sidebarSide: ManageSidebarSide;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [createMenuOpen, setCreateMenuOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const HideSidebarIcon = sidebarSide === "right" ? IconLayoutSidebarRightCollapse : IconLayoutSidebarLeftCollapse;
  const BulkDirectoryIcon = hasExpandedDirectories ? IconArrowsDiagonalMinimize : IconArrowsDiagonal2;
  const bulkDirectoryActionLabel = hasExpandedDirectories ? "Collapse All" : "Expand All";
  const isCreating = Boolean(creatingKind) || isCreatingFolder;

  useEffect(() => {
    if (!menuOpen && !createMenuOpen) {
      return;
    }
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && wrapperRef.current?.contains(target)) {
        return;
      }
      setMenuOpen(false);
      setCreateMenuOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenuOpen(false);
        setCreateMenuOpen(false);
      }
    };
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [createMenuOpen, menuOpen]);

  const runMenuAction = (action: () => void) => {
    setMenuOpen(false);
    action();
  };

  const runCreateAction = (action: () => void) => {
    setCreateMenuOpen(false);
    action();
  };

  return (
    <div className="manage-sidebar-actions" ref={wrapperRef}>
      <ManageTooltipButton
        aria-label={bulkDirectoryActionLabel}
        className="manage-icon-button manage-sidebar-tree-toggle"
        disabled={!hasExpandableDirectories}
        onClick={() => {
          setCreateMenuOpen(false);
          setMenuOpen(false);
          onToggleAllDirectories();
        }}
        tooltip={bulkDirectoryActionLabel}
        type="button"
      >
        <BulkDirectoryIcon aria-hidden="true" size={14} stroke={1.9} />
      </ManageTooltipButton>
      <ManageTooltipButton
        aria-expanded={createMenuOpen}
        aria-haspopup="menu"
        aria-label="Create docs item"
        className="manage-icon-button"
        disabled={isCreating}
        onClick={() => {
          setCreateMenuOpen((current) => !current);
          setMenuOpen(false);
        }}
        tooltip="Create docs item"
        type="button"
      >
        <IconPlus aria-hidden="true" size={15} stroke={1.9} />
      </ManageTooltipButton>
      {/*
        CDXC:DocsSidebar 2026-06-30-21:26:
        The Docs sidebar header should place the overflow menu before the Hide sidebar control so the two rightmost buttons match the requested visual order while keeping their existing actions unchanged.
      */}
      <button
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        aria-label="Docs sidebar menu"
        className="manage-icon-button"
        onClick={() => {
          setMenuOpen((current) => !current);
          setCreateMenuOpen(false);
        }}
        type="button"
      >
        <IconMenu2 aria-hidden="true" size={15} stroke={1.8} />
      </button>
      <button
        aria-label="Hide file sidebar"
        className="manage-icon-button"
        onClick={onHideSidebar}
        type="button"
      >
        <HideSidebarIcon aria-hidden="true" size={15} stroke={1.8} />
      </button>
      {createMenuOpen ? (
        <div className="manage-sidebar-menu manage-create-menu" role="menu">
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(onCreateFolder)}
            role="menuitem"
            type="button"
          >
            <IconFolderPlus aria-hidden="true" size={14} stroke={1.8} />
            {isCreatingFolder ? "Creating folder" : "New folder"}
          </button>
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(() => onCreate("markdown"))}
            role="menuitem"
            type="button"
          >
            <IconMarkdown aria-hidden="true" size={14} stroke={1.8} />
            {creatingKind === "markdown" ? "Creating Markdown" : "New Markdown"}
          </button>
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(() => onCreate("html"))}
            role="menuitem"
            type="button"
          >
            <IconFileTypeHtml aria-hidden="true" size={14} stroke={1.8} />
            {creatingKind === "html" ? "Creating HTML" : "New HTML"}
          </button>
          <button
            className="manage-sidebar-menu-item"
            disabled={isCreating}
            onClick={() => runCreateAction(() => onCreate("excalidraw"))}
            role="menuitem"
            type="button"
          >
            <IconEdit aria-hidden="true" size={14} stroke={1.8} />
            {creatingKind === "excalidraw" ? "Creating drawing" : "New drawing"}
          </button>
        </div>
      ) : null}
      {menuOpen ? (
        <div className="manage-sidebar-menu" role="menu">
          <button
            className="manage-sidebar-menu-item"
            disabled={isRefreshing}
            onClick={() => runMenuAction(onRefresh)}
            role="menuitem"
            type="button"
          >
            <IconRefresh aria-hidden="true" size={14} stroke={1.8} />
            Refresh
          </button>
          <button
            className="manage-sidebar-menu-item"
            onClick={() => runMenuAction(onSwitchSide)}
            role="menuitem"
            type="button"
          >
            {sidebarSide === "right" ? (
              <IconLayoutSidebarLeftCollapse aria-hidden="true" size={14} stroke={1.8} />
            ) : (
              <IconLayoutSidebarRightCollapse aria-hidden="true" size={14} stroke={1.8} />
            )}
            Switch sidebar side
          </button>
          {/*
            CDXC:DocsSidebarSettings 2026-06-30-11:42:
            The Docs overflow menu should deep-link to Settings -> Projects -> Global Settings so users can configure the project-relative folders that Docs scans for files without leaving the Docs context.
          */}
          <button
            className="manage-sidebar-menu-item"
            onClick={() => runMenuAction(onOpenDocsFoldersSettings)}
            role="menuitem"
            type="button"
          >
            <IconSettings aria-hidden="true" size={14} stroke={1.8} />
            Configure docs folders
          </button>
        </div>
      ) : null}
    </div>
  );
}

function ManageFileRow({
  annotationCount,
  canOpenContextMenu,
  entry,
  hasActiveFileDescendant,
  hasChildren,
  isContextMenuOpen,
  isDragging,
  isDropTarget,
  isExpanded,
  isSelected,
  onEntryDragOver,
  onEntryDrop,
  onDragEnd,
  onDragStart,
  onOpenContextMenu,
  onSelect,
}: {
  annotationCount: number;
  canOpenContextMenu: boolean;
  entry: ManageFileEntry;
  hasActiveFileDescendant: boolean;
  hasChildren: boolean;
  isContextMenuOpen: boolean;
  isDragging: boolean;
  isDropTarget: boolean;
  isExpanded: boolean;
  isSelected: boolean;
  onEntryDragOver: (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => void;
  onEntryDrop: (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => void;
  onDragEnd: () => void;
  onDragStart: (entry: ManageFileEntry, event: ReactDragEvent<HTMLButtonElement>) => void;
  onOpenContextMenu: (entry: ManageFileEntry, point: { x: number; y: number }) => void;
  onSelect: () => void;
}) {
  const Icon = entry.kind === "directory" ? (isExpanded ? IconFolderOpen : IconFolder) : fileIconForPath(entry.path);
  return (
    <button
      aria-expanded={entry.kind === "directory" && hasChildren ? isExpanded : undefined}
      aria-haspopup={canOpenContextMenu ? "menu" : undefined}
      aria-selected={entry.kind === "file" ? isSelected : undefined}
      className="manage-file-row"
      data-active-descendant={String(hasActiveFileDescendant)}
      data-context-menu-open={String(isContextMenuOpen)}
      data-dragging={String(isDragging)}
      data-drop-target={String(isDropTarget)}
      data-kind={entry.kind}
      data-selected={String(isSelected)}
      draggable={entry.kind === "file" || entry.kind === "directory"}
      onClick={onSelect}
      onContextMenu={(event: ReactMouseEvent<HTMLButtonElement>) => {
        event.preventDefault();
        event.stopPropagation();
        if (!canOpenContextMenu) {
          return;
        }
        onOpenContextMenu(entry, { x: event.clientX, y: event.clientY });
      }}
      onKeyDown={(event: ReactKeyboardEvent<HTMLButtonElement>) => {
        if (!canOpenContextMenu) {
          return;
        }
        if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) {
          return;
        }
        event.preventDefault();
        const bounds = event.currentTarget.getBoundingClientRect();
        onOpenContextMenu(entry, {
          x: bounds.left + 28,
          y: bounds.top + Math.min(22, bounds.height),
        });
      }}
      onDragEnd={onDragEnd}
      onDragOver={(event) => onEntryDragOver(entry, event)}
      onDragStart={(event) => onDragStart(entry, event)}
      onDrop={(event) => onEntryDrop(entry, event)}
      role="treeitem"
      style={{ "--depth": entry.depth } as CSSProperties}
      type="button"
    >
      <span
        aria-hidden="true"
        className="manage-file-disclosure"
        data-visible={String(entry.kind === "directory" && hasChildren)}
      >
        <IconChevronRight size={14} stroke={1.9} />
      </span>
      <Icon aria-hidden="true" className="manage-file-icon" size={15} stroke={1.75} />
      <span className="manage-file-name">{entry.name}</span>
      <span className="manage-file-badges">
        {annotationCount > 0 ? <span className="manage-count-badge">{annotationCount}</span> : null}
      </span>
    </button>
  );
}

function ManageFileContextMenu({
  canAddToSessionContext,
  canCreateHere,
  canDuplicate,
  canRenameOrDelete,
  confirmingDelete,
  creatingKind,
  isCreatingFolder,
  onAddToSessionContext,
  onCopyFullPath,
  onCopyPath,
  onCreateFileHere,
  onCreateFolderHere,
  onDuplicate,
  onDelete,
  onDismiss,
  onRename,
  onRevealInFinder,
  pendingAction,
  position,
}: {
  canAddToSessionContext: boolean;
  canCreateHere: boolean;
  canDuplicate: boolean;
  canRenameOrDelete: boolean;
  confirmingDelete: boolean;
  creatingKind?: ManageArtifactKind;
  isCreatingFolder: boolean;
  onAddToSessionContext: () => void;
  onCopyFullPath: () => void;
  onCopyPath: () => void;
  onCreateFileHere: (kind: ManageArtifactKind) => void;
  onCreateFolderHere: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onDismiss: () => void;
  onRename: () => void;
  onRevealInFinder: () => void;
  pendingAction?: ManageFileOperationState["action"];
  position: Pick<ManageFileContextMenuState, "x" | "y">;
}) {
  const [createFileMenuOpen, setCreateFileMenuOpen] = useState(false);
  const isBusy = Boolean(pendingAction);
  return (
    <SidebarContextMenuPortal
      menuClassName="session-context-menu manage-file-context-menu"
      menuStyle={{
        left: `${position.x}px`,
        position: "fixed",
        top: `${position.y}px`,
      }}
      onDismiss={onDismiss}
    >
      <button
        className="session-context-menu-item manage-file-context-menu-item"
        disabled={isBusy}
        onClick={onRevealInFinder}
        role="menuitem"
        type="button"
      >
        <IconFolderOpen aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
        {pendingAction === "revealInFinder" ? "Revealing" : "Reveal in Finder"}
      </button>
      <button
        className="session-context-menu-item manage-file-context-menu-item"
        onClick={onCopyPath}
        role="menuitem"
        type="button"
      >
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
        Copy Relative Path
      </button>
      <button
        className="session-context-menu-item manage-file-context-menu-item"
        disabled={isBusy}
        onClick={onCopyFullPath}
        role="menuitem"
        type="button"
      >
        <IconCopy aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
        {pendingAction === "copyFullPath" ? "Copying Full Path" : "Copy Full Path"}
      </button>
      {canAddToSessionContext ? (
        <button
          className="session-context-menu-item manage-file-context-menu-item"
          disabled={isBusy}
          onClick={onAddToSessionContext}
          role="menuitem"
          type="button"
        >
          <IconMessagePlus aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
          {pendingAction === "addToSessionContext" ? "Adding context" : "Add to Session Context"}
        </button>
      ) : null}
      {canCreateHere ? (
        <>
          <div className="session-context-menu-divider manage-file-context-menu-divider" role="separator" />
          <button
            aria-expanded={createFileMenuOpen}
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={() => setCreateFileMenuOpen((current) => !current)}
            role="menuitem"
            type="button"
          >
            <IconFile aria-hidden="true" className="session-context-menu-icon" size={14} stroke={1.8} />
            <span>New File Here</span>
            <span className="manage-file-context-menu-spacer" />
            <IconChevronRight
              aria-hidden="true"
              className="manage-file-context-menu-chevron"
              data-open={String(createFileMenuOpen)}
              size={14}
              stroke={1.8}
            />
          </button>
          {createFileMenuOpen ? (
            <div className="manage-file-context-menu-nested" role="group">
              <button
                className="session-context-menu-item manage-file-context-menu-item manage-file-context-menu-subitem"
                disabled={isBusy}
                onClick={() => onCreateFileHere("markdown")}
                role="menuitem"
                type="button"
              >
                <IconMarkdown aria-hidden="true" size={14} stroke={1.8} />
                {creatingKind === "markdown" ? "Creating Markdown" : "Markdown"}
              </button>
              <button
                className="session-context-menu-item manage-file-context-menu-item manage-file-context-menu-subitem"
                disabled={isBusy}
                onClick={() => onCreateFileHere("html")}
                role="menuitem"
                type="button"
              >
                <IconFileTypeHtml aria-hidden="true" size={14} stroke={1.8} />
                {creatingKind === "html" ? "Creating HTML" : "HTML"}
              </button>
              <button
                className="session-context-menu-item manage-file-context-menu-item manage-file-context-menu-subitem"
                disabled={isBusy}
                onClick={() => onCreateFileHere("excalidraw")}
                role="menuitem"
                type="button"
              >
                <IconEdit aria-hidden="true" size={14} stroke={1.8} />
                {creatingKind === "excalidraw" ? "Creating Excalidraw" : "Excalidraw"}
              </button>
            </div>
          ) : null}
          <button
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={onCreateFolderHere}
            role="menuitem"
            type="button"
          >
            <IconFolderPlus aria-hidden="true" size={14} stroke={1.8} />
            {isCreatingFolder ? "Creating Folder" : "New Folder Here"}
          </button>
        </>
      ) : null}
      {canDuplicate ? (
        <>
          <div className="session-context-menu-divider manage-file-context-menu-divider" role="separator" />
          <button
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={onDuplicate}
            role="menuitem"
            type="button"
          >
            <IconCopyPlus aria-hidden="true" size={14} stroke={1.8} />
            {pendingAction === "duplicate" ? "Duplicating" : "Duplicate"}
          </button>
        </>
      ) : null}
      {canRenameOrDelete ? (
        <>
          {!canDuplicate ? <div className="session-context-menu-divider manage-file-context-menu-divider" role="separator" /> : null}
          <button
            className="session-context-menu-item manage-file-context-menu-item"
            disabled={isBusy}
            onClick={onRename}
            role="menuitem"
            type="button"
          >
            <IconEdit aria-hidden="true" size={14} stroke={1.8} />
            Rename
          </button>
          <button
            className="session-context-menu-item session-context-menu-item-danger manage-file-context-menu-item manage-file-context-menu-item-danger"
            data-confirming={String(confirmingDelete)}
            disabled={isBusy}
            onClick={onDelete}
            role="menuitem"
            type="button"
          >
            <IconTrash aria-hidden="true" size={14} stroke={1.8} />
            {pendingAction === "delete" ? "Deleting" : confirmingDelete ? "Confirm delete" : "Delete"}
          </button>
        </>
      ) : null}
    </SidebarContextMenuPortal>
  );
}

function ManageRenameDialog({
  error,
  isRenaming,
  onCancel,
  onChange,
  onSubmit,
  value,
}: {
  error?: string;
  isRenaming: boolean;
  onCancel: () => void;
  onChange: (value: string) => void;
  onSubmit: () => void;
  value: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const isSubmitDisabled = isRenaming || value.trim().length === 0;

  useEffect(() => {
    const input = inputRef.current;
    if (!input) {
      return;
    }
    input.focus();
    input.select();
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCancel]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (isSubmitDisabled) {
      return;
    }
    onSubmit();
  };

  return createPortal(
    <>
      <button
        aria-label="Cancel rename"
        className="manage-rename-backdrop"
        onClick={onCancel}
        type="button"
      />
      <form className="manage-rename-dialog" onSubmit={submit}>
        <div className="manage-rename-header">
          <span>Rename item</span>
          <button
            aria-label="Cancel rename"
            className="manage-icon-button manage-rename-close"
            onClick={onCancel}
            type="button"
          >
            <IconX aria-hidden="true" size={15} stroke={1.8} />
          </button>
        </div>
        <input
          aria-label="Item name"
          className="manage-rename-input"
          disabled={isRenaming}
          onChange={(event) => onChange(event.currentTarget.value)}
          ref={inputRef}
          value={value}
        />
        {error ? <div className="manage-rename-error">{error}</div> : null}
        <div className="manage-rename-actions">
          <button className="manage-rename-secondary" disabled={isRenaming} onClick={onCancel} type="button">
            Cancel
          </button>
          <button className="manage-rename-primary" disabled={isSubmitDisabled} type="submit">
            {isRenaming ? "Renaming" : "Rename"}
          </button>
        </div>
      </form>
    </>,
    document.body,
  );
}

function ManagePreview({
  annotations,
  annotationPersistenceState,
  draftContent,
  error,
  hasExternalChanges,
  isDirty,
  onAnnotationsChange,
  onDraftContentChange,
  onOpenDocument,
  onReload,
  preview,
  previewState,
  saveState,
  selectedPath,
}: {
  annotations: ManageAnnotation[];
  annotationPersistenceState: "idle" | "loading" | "ready" | "saving" | "saved" | "error";
  draftContent: string;
  error?: string;
  hasExternalChanges: boolean;
  isDirty: boolean;
  onAnnotationsChange: (updater: (annotations: ManageAnnotation[]) => ManageAnnotation[]) => void;
  onDraftContentChange: (content: string) => void;
  onOpenDocument: (path: string) => void;
  onReload: () => void;
  preview?: ManageFilePreview;
  previewState: "idle" | "loading" | "ready" | "error";
  saveState: "idle" | "saving" | "saved" | "error";
  selectedPath?: string;
}) {
  const [selection, setSelection] = useState<ManageCapturedSelection>();
  const [selectionToolbarMode, setSelectionToolbarMode] = useState<ManageSelectionToolbarMode>("annotations");
  const [commentDraft, setCommentDraft] = useState<ManageCommentDraft>();
  const [annotationPreview, setAnnotationPreview] = useState<ManageAnnotationPreview>();
  const [feedbackCopyState, setFeedbackCopyState] = useState<"idle" | "copied" | "error">("idle");
  const [clearAnnotationsConfirming, setClearAnnotationsConfirming] = useState(false);
  const [annotationsDropdownOpen, setAnnotationsDropdownOpen] = useState(false);
  const [htmlAnnotationEnabled, setHtmlAnnotationEnabled] = useState(true);
  const annotationsDropdownRef = useRef<HTMLDivElement | null>(null);
  const clearAnnotationsTimerRef = useRef<number | undefined>(undefined);
  const selectedPathRef = useRef<string | undefined>(selectedPath);

  const resetClearAnnotationsConfirm = useCallback(() => {
    if (clearAnnotationsTimerRef.current !== undefined) {
      window.clearTimeout(clearAnnotationsTimerRef.current);
      clearAnnotationsTimerRef.current = undefined;
    }
    setClearAnnotationsConfirming(false);
  }, []);

  useEffect(() => {
    if (selectedPathRef.current !== selectedPath) {
      selectedPathRef.current = selectedPath;
      setSelection(undefined);
      setSelectionToolbarMode("annotations");
      setCommentDraft(undefined);
      setAnnotationPreview(undefined);
      setFeedbackCopyState("idle");
      resetClearAnnotationsConfirm();
      setAnnotationsDropdownOpen(false);
    }
  }, [resetClearAnnotationsConfirm, selectedPath]);

  useEffect(() => {
    if (annotations.length === 0) {
      resetClearAnnotationsConfirm();
    }
  }, [annotations.length, resetClearAnnotationsConfirm]);

  useEffect(
    () => () => {
      if (clearAnnotationsTimerRef.current !== undefined) {
        window.clearTimeout(clearAnnotationsTimerRef.current);
      }
    },
    [],
  );

  useEffect(() => {
    if (!annotationsDropdownOpen) {
      return;
    }
    function handlePointerDown(event: PointerEvent) {
      const dropdownElement = annotationsDropdownRef.current;
      if (!dropdownElement || !event.target || dropdownElement.contains(event.target as Node)) {
        return;
      }
      setAnnotationsDropdownOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setAnnotationsDropdownOpen(false);
      }
    }
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [annotationsDropdownOpen]);

  const addAnnotation = useCallback(
    ({
      attachments = [],
      labelId,
      note = "",
      quote = "",
      type,
    }: {
      attachments?: ManageAnnotationImage[];
      labelId?: ManageQuickLabelId;
      note?: string;
      quote?: string;
      type: ManageAnnotationType;
    }) => {
      const normalizedQuote = normalizeAnnotationQuote(quote);
      if (type === "redline" && !normalizedQuote) {
        return;
      }
      const normalizedNote = note.trim();
      if (type === "comment" && !normalizedQuote && !normalizedNote && attachments.length === 0) {
        return;
      }
      const nextAnnotation: ManageAnnotation = {
        attachments,
        createdAt: new Date().toISOString(),
        id: `manage-annotation-${Date.now()}-${Math.random().toString(16).slice(2)}`,
        labelId,
        note: normalizedNote,
        quote: normalizedQuote,
        scope: normalizedQuote ? "selection" : "global",
        type,
      };
      onAnnotationsChange((current) => [...current, nextAnnotation]);
      setSelection(undefined);
      setSelectionToolbarMode("annotations");
      setCommentDraft(undefined);
    },
    [onAnnotationsChange],
  );

  const captureSelectedText = useCallback((capturedSelection: ManageCapturedSelection) => {
    const normalized = normalizeAnnotationQuote(capturedSelection.text);
    if (!normalized) {
      return;
    }
    setAnnotationPreview(undefined);
    setCommentDraft(undefined);
    setSelectionToolbarMode("annotations");
    setSelection({
      anchor: capturedSelection.anchor,
      text: normalized,
    });
  }, []);

  const clearSelectedText = useCallback(() => {
    setSelection(undefined);
    setSelectionToolbarMode("annotations");
  }, []);

  const openCommentDraft = useCallback(
    (quote: string, anchor: ManageSelectionAnchor, initialNote = "") => {
      setAnnotationPreview(undefined);
      setSelection(undefined);
      setSelectionToolbarMode("annotations");
      setCommentDraft({
        anchor,
        attachmentError: "",
        attachments: [],
        note: initialNote,
        quote: normalizeAnnotationQuote(quote),
      });
    },
    [],
  );

  const addSelectedRedline = useCallback(() => {
    if (!selection) {
      return;
    }
    addAnnotation({
      quote: selection.text,
      type: "redline",
    });
  }, [addAnnotation, selection]);

  const addQuickLabel = useCallback(
    (label: ManageQuickLabel) => {
      addAnnotation({
        labelId: label.id,
        note: "",
        quote: selection?.text ?? commentDraft?.quote ?? "",
        type: "comment",
      });
    },
    [addAnnotation, commentDraft?.quote, selection?.text],
  );

  const submitCommentDraft = useCallback(() => {
    if (!commentDraft) {
      return;
    }
    addAnnotation({
      attachments: commentDraft.attachments,
      note: commentDraft.note,
      quote: commentDraft.quote,
      type: "comment",
    });
  }, [addAnnotation, commentDraft]);

  const updateCommentDraftNote = useCallback((note: string) => {
    setCommentDraft((current) => (current ? { ...current, note } : current));
  }, []);

  const addAttachmentFiles = useCallback((files: FileList | File[]) => {
    const imageFiles = Array.from(files).filter((file) => file.type.startsWith("image/"));
    if (imageFiles.length === 0) {
      return;
    }
    setCommentDraft((current) => {
      if (!current) {
        return current;
      }
      const availableSlots = Math.max(0, MANAGE_ANNOTATION_MAX_IMAGES - current.attachments.length);
      if (availableSlots === 0) {
        return {
          ...current,
          attachmentError: `Use ${MANAGE_ANNOTATION_MAX_IMAGES} images or fewer per annotation.`,
        };
      }
      let attachmentError =
        imageFiles.length > availableSlots ? `Use ${MANAGE_ANNOTATION_MAX_IMAGES} images or fewer per annotation.` : "";
      for (const file of imageFiles.slice(0, availableSlots)) {
        if (file.size > MANAGE_ANNOTATION_IMAGE_MAX_BYTES) {
          attachmentError = "Images must be 512 KB or smaller.";
          continue;
        }
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = typeof reader.result === "string" ? reader.result : "";
          if (!dataUrl) {
            return;
          }
          setCommentDraft((latest) => {
            if (!latest || latest.attachments.length >= MANAGE_ANNOTATION_MAX_IMAGES) {
              return latest;
            }
            return {
              ...latest,
              attachmentError: "",
              attachments: [
                ...latest.attachments,
                {
                  dataUrl,
                  id: `manage-annotation-image-${Date.now()}-${Math.random().toString(16).slice(2)}`,
                  mimeType: file.type,
                  name: normalizeAttachmentName(file.name),
                  size: file.size,
                },
              ],
            };
          });
        };
        reader.onerror = () => {
          setCommentDraft((latest) =>
            latest ? { ...latest, attachmentError: "Could not read image attachment." } : latest,
          );
        };
        reader.readAsDataURL(file);
      }
      return {
        ...current,
        attachmentError,
      };
    });
  }, []);

  const removeDraftAttachment = useCallback((attachmentId: string) => {
    setCommentDraft((current) =>
      current
        ? {
            ...current,
            attachments: current.attachments.filter((attachment) => attachment.id !== attachmentId),
          }
        : current,
    );
  }, []);

  const copyFeedback = useCallback(async () => {
    if (!selectedPath) {
      return;
    }
    /*
     * CDXC:DocsRootAdditive 2026-08-10:
     * This markdown is read by a human and by the agent it is pasted to, so it
     * names the file the way the tree does rather than by routing address.
     */
    const output = formatManageAnnotationsAsMarkdown(preview?.displayPath ?? selectedPath, annotations);
    try {
      await writeTextToClipboard(output);
      setFeedbackCopyState("copied");
      window.setTimeout(() => setFeedbackCopyState("idle"), 1_600);
    } catch {
      setFeedbackCopyState("error");
    }
  }, [annotations, preview?.displayPath, selectedPath]);

  const clearAllAnnotations = useCallback(() => {
    if (annotations.length === 0) {
      resetClearAnnotationsConfirm();
      return;
    }
    if (!clearAnnotationsConfirming) {
      setClearAnnotationsConfirming(true);
      if (clearAnnotationsTimerRef.current !== undefined) {
        window.clearTimeout(clearAnnotationsTimerRef.current);
      }
      clearAnnotationsTimerRef.current = window.setTimeout(() => {
        clearAnnotationsTimerRef.current = undefined;
        setClearAnnotationsConfirming(false);
      }, 3_000);
      return;
    }
    resetClearAnnotationsConfirm();
    setAnnotationsDropdownOpen(false);
    onAnnotationsChange(() => []);
  }, [annotations.length, clearAnnotationsConfirming, onAnnotationsChange, resetClearAnnotationsConfirm]);

  const openCommentForSelection = useCallback(() => {
    if (!selection) {
      return;
    }
    openCommentDraft(selection.text, selection.anchor);
  }, [openCommentDraft, selection]);

  const openGlobalComment = useCallback(
    (anchor: ManageSelectionAnchor) => {
      openCommentDraft("", anchor);
    },
    [openCommentDraft],
  );

  useEffect(() => {
    if (!selection || commentDraft) {
      return;
    }
    const activeSelection = selection;
    function handleAnnotationShortcut(event: KeyboardEvent) {
      if (event.isComposing || event.metaKey || event.ctrlKey || event.altKey || isEditableEventTarget(event.target)) {
        return;
      }
      const key = event.key.toLocaleLowerCase();
      if (key === "escape") {
        event.preventDefault();
        setSelection(undefined);
        return;
      }
      if (key === "backspace" || key === "d" || key === "delete") {
        event.preventDefault();
        addSelectedRedline();
        return;
      }
      if (key === "c") {
        event.preventDefault();
        openCommentForSelection();
        return;
      }
      if (/^[1-3]$/u.test(key)) {
        event.preventDefault();
        const label = MANAGE_QUICK_LABELS[Number(key) - 1];
        if (label) {
          addQuickLabel(label);
        }
        return;
      }
      if (event.key.length === 1) {
        event.preventDefault();
        openCommentDraft(activeSelection.text, activeSelection.anchor, event.key);
      }
    }
    window.addEventListener("keydown", handleAnnotationShortcut);
    return () => window.removeEventListener("keydown", handleAnnotationShortcut);
  }, [addQuickLabel, addSelectedRedline, commentDraft, openCommentDraft, openCommentForSelection, selection]);

  const removeAnnotation = useCallback(
    (annotationId: string) => {
      onAnnotationsChange((current) => current.filter((annotation) => annotation.id !== annotationId));
    },
    [onAnnotationsChange],
  );

  const removePreviewAnnotation = useCallback(
    (annotationId: string) => {
      removeAnnotation(annotationId);
      setAnnotationPreview(undefined);
    },
    [removeAnnotation],
  );

  if (previewState === "loading") {
    return <ManagePreviewMessage icon={<IconRefresh aria-hidden="true" size={20} />} title="Loading file" />;
  }
  if (error) {
    return (
      <ManagePreviewMessage
        icon={<IconAlertTriangle aria-hidden="true" size={21} />}
        title={error}
      />
    );
  }
  if (!selectedPath || !preview) {
    return <ManagePreviewMessage icon={<IconFileText aria-hidden="true" size={21} />} title="Select a file" />;
  }

  const language = languageLabelForPath(preview.path);
  const isMarkdown = isMarkdownPath(preview.path);
  const isDrawing = isExcalidrawPath(preview.path);
  const isHtml = isHtmlPath(preview.path);
  const usesCompactArtifactHeader = isMarkdown || isDrawing || isHtml;
  /*
   * CDXC:DocsRootAdditive 2026-08-09:
   * Show the file the way the tree names it. `preview.path` is a routing
   * address that starts with the reserved mount segment for anything under a
   * configured Docs directory, which is not a name any human asked for.
   */
  const previewDisplayPath = preview.displayPath ?? preview.path;
  const previewTitle = usesCompactArtifactHeader ? previewDisplayPath : preview.name;
  const annotationPersistenceTitle = annotationPersistenceLabel(annotationPersistenceState);

  return (
    <div
      className="manage-preview-content"
      data-compact-header={String(usesCompactArtifactHeader)}
      data-kind={isMarkdown ? "markdown" : isDrawing ? "drawing" : isHtml ? "html" : "text"}
    >
      <header className="manage-preview-header">
        <div className="manage-preview-title">
          {isDrawing ? (
            <IconEdit aria-hidden="true" size={17} stroke={1.85} />
          ) : (
            <IconFileText aria-hidden="true" size={17} stroke={1.85} />
          )}
          <span>{previewTitle}</span>
        </div>
        <div className="manage-preview-meta">
          <span>{language}</span>
          {preview.size !== undefined ? <span>{formatFileSize(preview.size)}</span> : null}
          {isDirty ? <span>Edited</span> : saveState === "saved" ? <span>Saved</span> : null}
        </div>
        {isMarkdown ? (
          <div className="manage-preview-header-actions">
            <ManageTooltipButton
              aria-label="Add global comment"
              onClick={(event) =>
                openGlobalComment(
                  selectionAnchorFromRect(event.currentTarget.getBoundingClientRect()) ?? defaultManageSelectionAnchor(),
                )
              }
              tooltip="Add global comment"
              type="button"
            >
              <IconMessagePlus aria-hidden="true" size={14} />
              <span>Comment</span>
            </ManageTooltipButton>
            <ManageTooltipButton
              aria-label="Copy feedback"
              disabled={annotations.length === 0}
              onClick={() => void copyFeedback()}
              tooltip="Copy feedback"
              type="button"
            >
              {feedbackCopyState === "copied" ? (
                <IconCheck aria-hidden="true" size={14} />
              ) : (
                <IconCopy aria-hidden="true" size={14} />
              )}
              <span>{feedbackCopyState === "copied" ? "Copied" : "Copy"}</span>
            </ManageTooltipButton>
            <ManageTooltipButton
              aria-label="Clear all annotations"
              className="manage-clear-annotations-button"
              data-confirming={String(clearAnnotationsConfirming)}
              disabled={annotations.length === 0}
              onClick={clearAllAnnotations}
              tooltip="Clear All Annotations"
              type="button"
            >
              {/*
                CDXC:DocsAnnotationToolbar 2026-06-30-04:55:
                The Markdown feedback toolbar's Clear action should use an X icon instead of a trash can because it clears review annotations rather than deleting a file.
              */}
              <IconX aria-hidden="true" size={14} />
              <span>{clearAnnotationsConfirming ? "Confirm" : "Clear"}</span>
            </ManageTooltipButton>
            <div className="manage-annotation-dropdown-shell" ref={annotationsDropdownRef}>
              <ManageTooltipButton
                aria-controls="manage-markdown-annotation-dropdown"
                aria-expanded={annotationsDropdownOpen}
                aria-haspopup="dialog"
                aria-label="Show annotations"
                className="manage-annotation-dropdown-trigger"
                onClick={() => setAnnotationsDropdownOpen((current) => !current)}
                tooltip={`Annotations (${annotations.length}) · ${annotationPersistenceTitle}`}
                type="button"
              >
                <IconMessages aria-hidden="true" size={14} />
                <span className="manage-count-badge">{annotations.length}</span>
              </ManageTooltipButton>
              {annotationsDropdownOpen ? (
                <ManageAnnotationDropdown annotations={annotations} onRemoveAnnotation={removeAnnotation} />
              ) : null}
            </div>
            <ManageTooltipButton
              aria-label={hasExternalChanges ? "Reload file with new changes" : "Reload file"}
              className="manage-file-reload-button"
              data-changes-available={String(hasExternalChanges)}
              onClick={onReload}
              tooltip={hasExternalChanges ? "Reload to show new changes" : "Reload file"}
              type="button"
            >
              <IconRefresh aria-hidden="true" size={14} />
              {hasExternalChanges ? <span aria-hidden="true" className="manage-file-change-indicator" /> : null}
            </ManageTooltipButton>
          </div>
        ) : isHtml ? (
          <div className="manage-preview-header-actions">
            <ManageTooltipButton
              aria-label="Toggle annotations"
              aria-pressed={htmlAnnotationEnabled}
              className="manage-annotation-toggle"
              onClick={() => setHtmlAnnotationEnabled((current) => !current)}
              tooltip={htmlAnnotationEnabled ? "Disable annotations" : "Enable annotations"}
              type="button"
            >
              <IconMessagePlus aria-hidden="true" size={14} />
              <span>Annotate</span>
            </ManageTooltipButton>
            <ManageTooltipButton
              aria-label="Reload HTML file"
              className="manage-file-reload-button"
              onClick={onReload}
              tooltip="Reload HTML file"
              type="button"
            >
              <IconRefresh aria-hidden="true" size={14} />
            </ManageTooltipButton>
          </div>
        ) : null}
      </header>
      {!usesCompactArtifactHeader ? (
        <div className="manage-preview-path">{previewDisplayPath}</div>
      ) : null}
      {preview.kind === "unsupported" ? (
        <ManagePreviewMessage
          icon={<IconAlertTriangle aria-hidden="true" size={21} />}
          title={preview.error ?? "Preview unavailable"}
        />
      ) : isDrawing ? (
        <ManageExcalidrawEditor
          content={draftContent}
          fileName={preview.name}
          key={preview.path}
          onChange={onDraftContentChange}
        />
      ) : isHtml ? (
        <ManageHtmlRenderViewer
          annotationsEnabled={htmlAnnotationEnabled}
          content={draftContent}
          documentKey={preview.path}
          onOpenDocument={onOpenDocument}
        />
      ) : isMarkdown ? (
        <>
          <ManageMarkdownReviewViewer
            annotations={annotations}
            content={draftContent}
            documentKey={preview.path}
            gitBaseline={preview.gitBaseline}
            onContentChange={onDraftContentChange}
            onAnnotationPreviewChange={setAnnotationPreview}
            onSelectionClear={clearSelectedText}
            onSelectionCapture={captureSelectedText}
            onSelectionToolbarModeChange={setSelectionToolbarMode}
            selection={selection}
            selectionToolbarMode={selectionToolbarMode}
          />
          {selection && selectionToolbarMode === "annotations" ? (
            <ManageAnnotationToolbar
              anchor={selection.anchor}
              onComment={openCommentForSelection}
              onDismiss={() => {
                setSelectionToolbarMode("annotations");
                setSelection(undefined);
              }}
              onFormatting={() => setSelectionToolbarMode("formatting")}
              onQuickLabel={addQuickLabel}
            />
          ) : null}
          {commentDraft ? (
            <ManageCommentPopover
              draft={commentDraft}
              onAddAttachmentFiles={addAttachmentFiles}
              onCancel={() => setCommentDraft(undefined)}
              onDraftNoteChange={updateCommentDraftNote}
              onRemoveDraftAttachment={removeDraftAttachment}
              onSubmit={submitCommentDraft}
            />
          ) : null}
          {annotationPreview && !selection && !commentDraft ? (
            <ManageAnnotationPreviewCard onRemoveAnnotation={removePreviewAnnotation} preview={annotationPreview} />
          ) : null}
        </>
      ) : (
        <ManageTextEditor
          content={draftContent}
          language={language}
          onChange={onDraftContentChange}
        />
      )}
    </div>
  );
}

function ManageHtmlRenderViewer({
  annotationsEnabled,
  content,
  documentKey,
  onOpenDocument,
}: {
  annotationsEnabled: boolean;
  content: string;
  documentKey: string;
  onOpenDocument: (path: string) => void;
}) {
  const resourceBaseUrl = manageHtmlResourceBaseUrl(documentKey);
  const renderedHtml = useMemo(
    () =>
      buildManageHtmlDocument(content, {
        injectAgentation: annotationsEnabled,
        resourceBaseUrl,
      }),
    [annotationsEnabled, content, resourceBaseUrl],
  );

  /*
   * CDXC:ManageHtmlAgentation 2026-08-08:
   * A feature named in `allow` without an explicit allowlist defaults to
   * `'src'`, which resolves against the frame's `src` URL. This frame renders
   * from `srcdoc` and has no `src`, so bare feature names matched no origin
   * and disabled clipboard and fullscreen in the rendered document instead of
   * granting them, leaving Agentation's copy button unable to write to the
   * clipboard. Name `'self'` explicitly: the srcdoc document is same-origin
   * with Manage, so it resolves and the grant is real.
   *
   * `clipboard-read` is denied rather than omitted. Omitting it inherits this
   * surface's permissive policy, and a programmatic `clipboard.readText()`
   * then hangs forever because Chromium wants a permission prompt that Alloy
   * cannot show. Denying it turns that into an immediate NotAllowedError.
   * User-initiated paste is unaffected: Cmd+V and the `paste` event carry
   * their data through `clipboardData`, which this policy does not gate.
   */
  return (
    <iframe
      allow="clipboard-read 'none'; clipboard-write 'self'; fullscreen 'self'"
      aria-label="Rendered HTML document"
      className="manage-html-render-view"
      data-document-key={documentKey}
      onLoad={(event) => {
        /*
         * CDXC:ManageHtmlDocumentNavigation 2026-08-06:
         * The synthetic folder base that makes sibling assets work also changes
         * fragment-link resolution inside srcdoc. Keep fragments owned by the
         * rendered document, and hand sibling HTML files back to Docs so its
         * selected path, header, and preview remain synchronized.
         */
        const renderedDocument = event.currentTarget.contentDocument;
        if (!renderedDocument) {
          return;
        }
        renderedDocument.addEventListener("click", (clickEvent) => {
          const mouseEvent = clickEvent as MouseEvent;
          if (
            clickEvent.defaultPrevented ||
            mouseEvent.button !== 0 ||
            mouseEvent.altKey ||
            mouseEvent.ctrlKey ||
            mouseEvent.metaKey ||
            mouseEvent.shiftKey
          ) {
            return;
          }
          const eventTarget = clickEvent.target as {
            closest?: (selector: string) => Element | null;
          } | null;
          const anchor = eventTarget?.closest?.("a[href]") as HTMLAnchorElement | null;
          const href = anchor?.getAttribute("href")?.trim();
          if (
            !anchor ||
            !href ||
            anchor.hasAttribute("download") ||
            (anchor.target && anchor.target !== "_self")
          ) {
            return;
          }
          if (href.startsWith("#")) {
            const targetId = decodeManageHtmlFragment(href);
            const target = targetId
              ? renderedDocument.getElementById(targetId)
              : renderedDocument.documentElement;
            if (target) {
              clickEvent.preventDefault();
              target.scrollIntoView({ behavior: "smooth", block: "start" });
            }
            return;
          }
          const linkedDocumentPath = manageHtmlLinkedDocumentPath(href, resourceBaseUrl);
          if (!linkedDocumentPath || linkedDocumentPath === documentKey) {
            return;
          }
          clickEvent.preventDefault();
          onOpenDocument(linkedDocumentPath);
        }, true);
      }}
      sandbox="allow-downloads allow-forms allow-modals allow-popups allow-popups-to-escape-sandbox allow-presentation allow-same-origin allow-scripts"
      srcDoc={renderedHtml}
      title={documentKey}
    />
  );
}

function ManageTextEditor({
  content,
  language,
  onChange,
}: {
  content: string;
  language: string;
  onChange: (content: string) => void;
}) {
  return (
    <textarea
      aria-label={`${language} editor`}
      className="manage-text-editor"
      onChange={(event) => onChange(event.currentTarget.value)}
      spellCheck={false}
      value={content}
    />
  );
}

function ManageMarkdownReviewViewer({
  annotations,
  content,
  documentKey,
  gitBaseline,
  onAnnotationPreviewChange,
  onContentChange,
  onSelectionClear,
  onSelectionCapture,
  onSelectionToolbarModeChange,
  selection,
  selectionToolbarMode,
}: {
  annotations: ManageAnnotation[];
  content: string;
  documentKey: string;
  gitBaseline?: ManageGitBaseline;
  onAnnotationPreviewChange: (preview: ManageAnnotationPreview | undefined) => void;
  onContentChange: (content: string) => void;
  onSelectionClear: () => void;
  onSelectionCapture: (selection: ManageCapturedSelection) => void;
  onSelectionToolbarModeChange: (mode: ManageSelectionToolbarMode) => void;
  selection?: ManageCapturedSelection;
  selectionToolbarMode: ManageSelectionToolbarMode;
}) {
  const editorHostRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<ManageMeoEditor | null>(null);
  const latestContentRef = useRef(content);
  const annotationsRef = useRef(annotations);
  const [contentMaxWidthEnabled, setContentMaxWidthEnabled] = useState(false);
  const [currentMode, setCurrentMode] = useState<ManageMeoMode>("live");
  const [findCaseSensitive, setFindCaseSensitive] = useState(false);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState("");
  const [findReplacement, setFindReplacement] = useState("");
  const [findStatus, setFindStatus] = useState("");
  const [findStatusIsError, setFindStatusIsError] = useState(false);
  const [findWholeWord, setFindWholeWord] = useState(false);
  const [gitGutterVisible, setGitGutterVisible] = useState(true);
  const [lineNumbersVisible, setLineNumbersVisible] = useState(true);
  const [meoSelectionState, setMeoSelectionState] = useState<ManageMeoSelectionState>({ visible: false });
  const onAnnotationPreviewChangeRef = useRef(onAnnotationPreviewChange);
  const onContentChangeRef = useRef(onContentChange);
  const onSelectionClearRef = useRef(onSelectionClear);
  const onSelectionCaptureRef = useRef(onSelectionCapture);

  useEffect(() => {
    annotationsRef.current = annotations;
  }, [annotations]);

  useEffect(() => {
    onAnnotationPreviewChangeRef.current = onAnnotationPreviewChange;
  }, [onAnnotationPreviewChange]);

  useEffect(() => {
    onContentChangeRef.current = onContentChange;
  }, [onContentChange]);

  useEffect(() => {
    onSelectionClearRef.current = onSelectionClear;
  }, [onSelectionClear]);

  useEffect(() => {
    onSelectionCaptureRef.current = onSelectionCapture;
  }, [onSelectionCapture]);

  const applyMeoFormat = useCallback((action: string, level?: number | { cols?: number; rows?: number }) => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    editor.insertFormat(action, level);
    editor.focus();
  }, []);

  const applyMeoMode = useCallback((mode: ManageMeoMode) => {
    const editor = editorRef.current;
    setCurrentMode(mode);
    editor?.setMode?.(mode);
    editor?.refreshLayout?.();
    editor?.focus();
  }, []);

  const toggleMeoLineNumbers = useCallback(() => {
    setLineNumbersVisible((current) => {
      const next = !current;
      editorRef.current?.setLineNumbers?.(next);
      editorRef.current?.refreshLayout?.();
      return next;
    });
  }, []);

  const toggleMeoGitGutter = useCallback(() => {
    setGitGutterVisible((current) => {
      const next = !current;
      editorRef.current?.setGitGutterVisible?.(next);
      editorRef.current?.refreshLayout?.();
      return next;
    });
  }, []);

  const toggleMeoContentMaxWidth = useCallback(() => {
    setContentMaxWidthEnabled((current) => {
      const next = !current;
      window.requestAnimationFrame(() => editorRef.current?.refreshLayout?.());
      return next;
    });
  }, []);

  const findOptions = useMemo(
    () => ({
      caseSensitive: findCaseSensitive,
      wholeWord: findWholeWord,
    }),
    [findCaseSensitive, findWholeWord],
  );

  const setFindStatusText = useCallback((text: string, isError = false) => {
    setFindStatus(text);
    setFindStatusIsError(isError);
  }, []);

  const updateFindStatusSummary = useCallback(() => {
    const editor = editorRef.current;
    if (!editor || !findOpen) {
      return;
    }
    editor.setSearchQuery?.(findQuery, findOptions);
    if (!findQuery) {
      setFindStatusText("");
      return;
    }
    const total = editor.countMatches?.(findQuery, findOptions) ?? 0;
    if (total === 0) {
      setFindStatusText("No matches", true);
      return;
    }
    setFindStatusText(`${total} matches`);
  }, [findOpen, findOptions, findQuery, setFindStatusText]);

  const runFind = useCallback(
    (backward = false) => {
      const editor = editorRef.current;
      if (!editor) {
        return;
      }
      if (!findQuery) {
        setFindStatusText("Enter text", true);
        return;
      }
      const result = backward
        ? editor.findPrevious?.(findQuery, findOptions)
        : editor.findNext?.(findQuery, findOptions);
      if (!result?.found) {
        setFindStatusText("No matches", true);
        return;
      }
      setFindStatusText(`${result.current}/${result.total}`);
    },
    [findOptions, findQuery, setFindStatusText],
  );

  const replaceCurrentFindMatch = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    if (!findQuery) {
      setFindStatusText("Enter text", true);
      return;
    }
    const result = editor.replaceCurrent?.(findQuery, findReplacement, findOptions);
    if (!result?.replaced) {
      if (result?.found) {
        setFindStatusText(`${result.current}/${result.total}`);
      } else {
        setFindStatusText("No matches", true);
      }
      return;
    }
    if (result.found) {
      setFindStatusText(`Replaced - ${result.current}/${result.total}`);
      return;
    }
    setFindStatusText(result.total ? `Replaced - ${result.total} remaining` : "Replaced");
  }, [findOptions, findQuery, findReplacement, setFindStatusText]);

  const replaceAllFindMatches = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    if (!findQuery) {
      setFindStatusText("Enter text", true);
      return;
    }
    const result = editor.replaceAll?.(findQuery, findReplacement, findOptions);
    if (!result?.replaced) {
      setFindStatusText("No matches", true);
      return;
    }
    setFindStatusText(`Replaced ${result.replaced} matches`);
  }, [findOptions, findQuery, findReplacement, setFindStatusText]);

  const closeFind = useCallback(() => {
    setFindOpen(false);
    setFindQuery("");
    setFindReplacement("");
    setFindStatusText("");
    editorRef.current?.setSearchQuery?.("", findOptions);
    editorRef.current?.focus();
  }, [findOptions, setFindStatusText]);

  useEffect(() => {
    if (!findOpen) {
      editorRef.current?.setSearchQuery?.("", findOptions);
      return;
    }
    updateFindStatusSummary();
  }, [findOpen, findOptions, findQuery, updateFindStatusSummary]);

  useEffect(() => {
    setMeoSelectionState({ visible: false });
    setFindOpen(false);
    setFindQuery("");
    setFindReplacement("");
    setFindStatusText("");
  }, [documentKey, setFindStatusText]);

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || content === latestContentRef.current) {
      return;
    }
    latestContentRef.current = content;
    editor.setText(content);
    editor.refreshLayout?.();
  }, [content]);

  useEffect(() => {
    editorRef.current?.setGitBaseline?.(gitBaseline ?? null);
  }, [documentKey, gitBaseline]);

  useEffect(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    editor.view.dispatch({
      effects: manageMeoAnnotationEffect.of(createManageMeoAnnotationDecorations(editor.getText(), annotations)),
    });
    syncManageMeoAnnotationReviewState(
      editor.view,
      annotations,
      onSelectionCaptureRef.current,
      onSelectionClearRef.current,
      onAnnotationPreviewChangeRef.current,
    );
  }, [annotations, content]);

  useEffect(() => {
    const host = editorHostRef.current;
    if (!host) {
      return;
    }
    latestContentRef.current = content;
    host.replaceChildren();
    applyManageMeoTheme();
    let mountedEditor: ManageMeoEditor | null = null;
    const editor = createMeoEditor({
      externalExtensions: [
        manageMeoAnnotationField,
        EditorView.updateListener.of((update) => {
          if (!update.selectionSet && !update.docChanged && !update.viewportChanged) {
            return;
          }
          syncManageMeoAnnotationReviewState(
            update.view,
            annotationsRef.current,
            onSelectionCaptureRef.current,
            onSelectionClearRef.current,
            onAnnotationPreviewChangeRef.current,
          );
        }),
      ] satisfies Extension[],
      initialGitGutter: gitGutterVisible,
      initialLineNumbers: lineNumbersVisible,
      initialMode: currentMode,
      initialVimKeybindings: [],
      parent: host,
      text: content,
      onSelectionChange: (state: ManageMeoSelectionState) => {
        setMeoSelectionState(state?.visible ? state : { visible: false });
      },
      onApplyChanges: (nextContent: string) => {
        latestContentRef.current = nextContent;
        onContentChangeRef.current(nextContent);
        mountedEditor?.view.dispatch({
          effects: manageMeoAnnotationEffect.of(createManageMeoAnnotationDecorations(nextContent, annotationsRef.current)),
        });
      },
      onOpenLink: (href: string) => {
        const safeHref = sanitizeManageHref(href);
        if (safeHref) {
          window.open(safeHref, "_blank", "noopener,noreferrer");
        }
      },
    }) as ManageMeoEditor;
    mountedEditor = editor;
    editorRef.current = editor;
    editor.setGitBaseline?.(gitBaseline ?? null);
    editor.view.dispatch({
      effects: manageMeoAnnotationEffect.of(createManageMeoAnnotationDecorations(content, annotationsRef.current)),
    });
    syncManageMeoAnnotationReviewState(
      editor.view,
      annotationsRef.current,
      onSelectionCaptureRef.current,
      onSelectionClearRef.current,
      onAnnotationPreviewChangeRef.current,
    );
    window.requestAnimationFrame(() => editor.refreshLayout?.());
    return () => {
      editor.destroy();
      if (editorRef.current === editor) {
        editorRef.current = null;
      }
    };
  }, [documentKey]);

  return (
    <div className="manage-markdown-review manage-markdown-meo-review">
      <section className="manage-markdown-review-main">
        <div
          className={`manage-meo-markdown-editor editor-root${contentMaxWidthEnabled ? " meo-content-max-width-enabled" : ""}`}
          style={{ "--meo-content-max-width": contentMaxWidthEnabled ? MANAGE_MEO_CONTENT_MAX_WIDTH : "100%" } as CSSProperties}
        >
          <ManageMeoTopToolbar
            contentMaxWidthEnabled={contentMaxWidthEnabled}
            currentMode={currentMode}
            findCaseSensitive={findCaseSensitive}
            findOpen={findOpen}
            findQuery={findQuery}
            findReplacement={findReplacement}
            findStatus={findStatus}
            findStatusIsError={findStatusIsError}
            findWholeWord={findWholeWord}
            gitGutterVisible={gitGutterVisible}
            lineNumbersVisible={lineNumbersVisible}
            onCloseFind={closeFind}
            onFindCaseSensitiveChange={setFindCaseSensitive}
            onFindOpenChange={setFindOpen}
            onFindQueryChange={setFindQuery}
            onFindReplacementChange={setFindReplacement}
            onFindWholeWordChange={setFindWholeWord}
            onFormat={applyMeoFormat}
            onModeChange={applyMeoMode}
            onReplaceAll={replaceAllFindMatches}
            onReplaceCurrent={replaceCurrentFindMatch}
            onRunFind={runFind}
            onToggleContentMaxWidth={toggleMeoContentMaxWidth}
            onToggleGitGutter={toggleMeoGitGutter}
            onToggleLineNumbers={toggleMeoLineNumbers}
          />
          <div className="editor-wrapper" data-outline-position="right">
            <div className="editor-host" ref={editorHostRef} />
          </div>
          {selectionToolbarMode === "formatting" && selection ? (
            <ManageMeoSelectionFormatToolbar
              anchor={selection.anchor}
              onAnnotate={() => onSelectionToolbarModeChange("annotations")}
              onFormat={applyMeoFormat}
              selectionState={meoSelectionState}
            />
          ) : null}
        </div>
      </section>
    </div>
  );
}

function ManageMeoTopToolbar({
  contentMaxWidthEnabled,
  currentMode,
  findCaseSensitive,
  findOpen,
  findQuery,
  findReplacement,
  findStatus,
  findStatusIsError,
  findWholeWord,
  gitGutterVisible,
  lineNumbersVisible,
  onCloseFind,
  onFindCaseSensitiveChange,
  onFindOpenChange,
  onFindQueryChange,
  onFindReplacementChange,
  onFindWholeWordChange,
  onFormat,
  onModeChange,
  onReplaceAll,
  onReplaceCurrent,
  onRunFind,
  onToggleContentMaxWidth,
  onToggleGitGutter,
  onToggleLineNumbers,
}: {
  contentMaxWidthEnabled: boolean;
  currentMode: ManageMeoMode;
  findCaseSensitive: boolean;
  findOpen: boolean;
  findQuery: string;
  findReplacement: string;
  findStatus: string;
  findStatusIsError: boolean;
  findWholeWord: boolean;
  gitGutterVisible: boolean;
  lineNumbersVisible: boolean;
  onCloseFind: () => void;
  onFindCaseSensitiveChange: (enabled: boolean) => void;
  onFindOpenChange: (open: boolean) => void;
  onFindQueryChange: (query: string) => void;
  onFindReplacementChange: (replacement: string) => void;
  onFindWholeWordChange: (enabled: boolean) => void;
  onFormat: (action: string, level?: number | { cols?: number; rows?: number }) => void;
  onModeChange: (mode: ManageMeoMode) => void;
  onReplaceAll: () => void;
  onReplaceCurrent: () => void;
  onRunFind: (backward?: boolean) => void;
  onToggleContentMaxWidth: () => void;
  onToggleGitGutter: () => void;
  onToggleLineNumbers: () => void;
}) {
  const [tableSize, setTableSize] = useState({ cols: 1, rows: 1 });
  const findInputRef = useRef<HTMLInputElement | null>(null);
  const fullToolbarWidthRef = useRef(0);
  const toolbarRef = useRef<HTMLDivElement | null>(null);
  const [hideOptionalControls, setHideOptionalControls] = useState(false);
  const headingIcons = [
    MeoHeading1Icon,
    MeoHeading2Icon,
    MeoHeading3Icon,
    MeoHeading4Icon,
    MeoHeading5Icon,
    MeoHeading6Icon,
  ];

  const runFindFromKeyboard = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter") {
      return;
    }
    event.preventDefault();
    onRunFind(event.shiftKey);
  };

  const runReplaceFromKeyboard = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key !== "Enter") {
      return;
    }
    event.preventDefault();
    onReplaceCurrent();
  };

  useEffect(() => {
    if (!findOpen) {
      return;
    }
    findInputRef.current?.focus();
    findInputRef.current?.select();
  }, [findOpen]);

  useLayoutEffect(() => {
    const toolbar = toolbarRef.current;
    if (!toolbar) {
      return undefined;
    }
    /*
     * CDXC:ManageMarkdownLayout 2026-06-30-13:45:
     * The three secondary right-side Markdown toolbar buttons should stay visible until the rendered toolbar actually overflows. Measure the full toolbar while those buttons are visible, then restore them only after the available width can fit that measured full row again.
     */
    let animationFrame: number | undefined;
    const measureToolbar = () => {
      animationFrame = undefined;
      const availableWidth = toolbar.clientWidth;
      if (availableWidth <= 0) {
        return;
      }
      if (!hideOptionalControls) {
        const toolbarStyle = window.getComputedStyle(toolbar);
        const toolbarGap = Number.parseFloat(toolbarStyle.columnGap || toolbarStyle.gap || "0") || 0;
        const horizontalPadding =
          (Number.parseFloat(toolbarStyle.paddingLeft) || 0) + (Number.parseFloat(toolbarStyle.paddingRight) || 0);
        const formatGroup = toolbar.querySelector(":scope > .format-group");
        const rightGroup = toolbar.querySelector(":scope > .right-group");
        const modeGroup = toolbar.querySelector(":scope > .mode-group");
        const requiredWidth =
          horizontalPadding +
          (formatGroup instanceof HTMLElement ? formatGroup.scrollWidth : 0) +
          (rightGroup instanceof HTMLElement ? rightGroup.getBoundingClientRect().width : 0) +
          (modeGroup instanceof HTMLElement ? modeGroup.getBoundingClientRect().width : 0) +
          toolbarGap * 2;
        fullToolbarWidthRef.current = requiredWidth;
        setHideOptionalControls(requiredWidth > availableWidth + 1);
        return;
      }
      const fullToolbarWidth = fullToolbarWidthRef.current;
      if (fullToolbarWidth > 0 && availableWidth >= fullToolbarWidth + 6) {
        setHideOptionalControls(false);
      }
    };
    const scheduleMeasure = () => {
      if (animationFrame !== undefined) {
        window.cancelAnimationFrame(animationFrame);
      }
      animationFrame = window.requestAnimationFrame(measureToolbar);
    };
    scheduleMeasure();
    const resizeObserver = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(scheduleMeasure);
    if (resizeObserver) {
      resizeObserver.observe(toolbar);
    } else {
      window.addEventListener("resize", scheduleMeasure);
    }
    return () => {
      if (animationFrame !== undefined) {
        window.cancelAnimationFrame(animationFrame);
      }
      resizeObserver?.disconnect();
      if (!resizeObserver) {
        window.removeEventListener("resize", scheduleMeasure);
      }
    };
  }, [currentMode, hideOptionalControls]);

  return (
    <div
      aria-label="Editor toolbar"
      className="mode-toolbar"
      data-optional-controls-hidden={String(hideOptionalControls)}
      ref={toolbarRef}
      role="toolbar"
    >
      <div aria-label="Formatting" className="format-group" role="group">
        <div className="heading-wrapper">
          <ManageTooltipButton
            className="format-button"
            data-action="heading"
            onClick={() => onFormat("heading", 1)}
            tooltip="Heading"
            type="button"
          >
            <MeoHeadingIcon aria-hidden="true" size={18} />
          </ManageTooltipButton>
          <div className="heading-dropdown-wrapper">
            <div aria-label="Heading levels" className="heading-dropdown" role="menu">
              {headingIcons.map((HeadingIcon, index) => {
                const level = index + 1;
                return (
                  <ManageTooltipButton
                    className="heading-dropdown-option"
                    data-level={level}
                    key={level}
                    onClick={() => onFormat("heading", level)}
                    tooltip={`Heading ${level}`}
                    type="button"
                  >
                    <HeadingIcon aria-hidden="true" size={18} />
                  </ManageTooltipButton>
                );
              })}
            </div>
          </div>
        </div>
        <ManageTooltipButton className="format-button" data-action="bulletList" onClick={() => onFormat("bulletList")} tooltip="Bullet List" type="button">
          <MeoListIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="numberedList" onClick={() => onFormat("numberedList")} tooltip="Numbered List" type="button">
          <MeoListOrderedIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="task" onClick={() => onFormat("task")} tooltip="Task" type="button">
          <MeoListTodoIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <div className="format-separator" role="separator" />
        <div className="table-wrapper">
          <ManageTooltipButton className="format-button" data-action="table" onClick={() => onFormat("table", tableSize)} tooltip="Table" type="button">
            <MeoTable2Icon aria-hidden="true" size={18} />
          </ManageTooltipButton>
          <div className="table-dropdown-wrapper">
            <div className="table-dropdown">
              <div className="table-grid">
                {Array.from({ length: 25 }, (_, index) => {
                  const row = Math.floor(index / 5) + 1;
                  const col = (index % 5) + 1;
                  const isHighlighted = col <= tableSize.cols && row <= tableSize.rows;
                  return (
                    <button
                      aria-label={`${col} by ${row} table`}
                      className={`table-grid-cell${isHighlighted ? " is-highlighted" : ""}`}
                      data-col={col}
                      data-row={row}
                      key={`${col}-${row}`}
                      onClick={() => onFormat("table", { cols: col, rows: row })}
                      onMouseEnter={() => setTableSize({ cols: col, rows: row })}
                      type="button"
                    />
                  );
                })}
              </div>
              <div className="table-size-label">{tableSize.cols} x {tableSize.rows}</div>
            </div>
          </div>
        </div>
        <ManageTooltipButton className="format-button" data-action="codeBlock" onClick={() => onFormat("codeBlock")} tooltip="Code Block" type="button">
          <MeoCodeIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="link" onClick={() => onFormat("link")} tooltip="Link" type="button">
          <MeoLinkIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="wikiLink" onClick={() => onFormat("wikiLink")} tooltip="Wiki Link" type="button">
          <MeoBracketsIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="image" onClick={() => onFormat("image")} tooltip="Image" type="button">
          <MeoImageIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="quote" onClick={() => onFormat("quote")} tooltip="Quote" type="button">
          <MeoQuoteIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton className="format-button" data-action="hr" onClick={() => onFormat("hr")} tooltip="Horizontal Rule" type="button">
          <MeoMinusIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
      </div>
      <div className="right-group">
        <ManageTooltipButton
          aria-pressed={findOpen}
          className={`format-button toggle-button${findOpen ? " is-active" : ""}`}
          data-action="find"
          onClick={() => {
            if (findOpen) {
              onCloseFind();
              return;
            }
            onFindOpenChange(true);
          }}
          tooltip="Find and Replace"
          type="button"
        >
          <MeoSearchIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          aria-pressed={contentMaxWidthEnabled}
          className={`format-button toggle-button manage-toolbar-optional-button${contentMaxWidthEnabled ? " is-active" : ""}`}
          data-action="contentMaxWidth"
          hidden={hideOptionalControls}
          onClick={onToggleContentMaxWidth}
          tooltip={contentMaxWidthEnabled ? "Use Full Content Width" : "Constrain Content Width"}
          type="button"
        >
          <MeoPanelLeftRightDashedIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          aria-pressed={lineNumbersVisible}
          className={`format-button toggle-button manage-toolbar-optional-button${lineNumbersVisible ? " is-active" : ""}`}
          data-action="lineNumbers"
          hidden={hideOptionalControls}
          onClick={onToggleLineNumbers}
          tooltip={lineNumbersVisible ? "Hide Line Numbers" : "Show Line Numbers"}
          type="button"
        >
          <MeoHashIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
        <ManageTooltipButton
          aria-pressed={gitGutterVisible}
          className={`format-button toggle-button manage-toolbar-optional-button${gitGutterVisible ? " is-active" : ""}`}
          data-action="gitChangesGutter"
          hidden={hideOptionalControls}
          onClick={onToggleGitGutter}
          tooltip={gitGutterVisible ? "Hide Git Changes" : "Show Git Changes"}
          type="button"
        >
          <MeoGitCompareIcon aria-hidden="true" size={18} />
        </ManageTooltipButton>
      </div>
      <div aria-label="Markdown mode" className="mode-group" role="group">
        <ManageTooltipButton
          aria-label={`Markdown mode: ${currentMode === "live" ? "Live" : "Source"}. Switch to ${
            currentMode === "live" ? "Source" : "Live"
          }.`}
          aria-pressed={currentMode === "source"}
          className="mode-button manage-mode-toggle is-active"
          data-mode={currentMode}
          onClick={() => onModeChange(currentMode === "live" ? "source" : "live")}
          tooltip={currentMode === "live" ? "Switch to Source" : "Switch to Live"}
          type="button"
        >
          {currentMode === "live" ? "Live" : "Source"}
        </ManageTooltipButton>
      </div>
      <div aria-label="Find and replace" className={`find-panel${findOpen ? " is-visible" : ""}`} role="search">
        <div className="find-row">
          <div className="find-input-wrap">
            <input
              aria-label="Find"
              className="find-input"
              onChange={(event) => onFindQueryChange(event.currentTarget.value)}
              onKeyDown={runFindFromKeyboard}
              placeholder="Find"
              ref={findInputRef}
              type="text"
              value={findQuery}
            />
            <span className={`find-status${findStatusIsError ? " is-error" : ""}`}>{findStatus}</span>
          </div>
          <ManageTooltipButton
            aria-label="Whole Word"
            aria-pressed={findWholeWord}
            className={`format-button toggle-button find-option-button${findWholeWord ? " is-active" : ""}`}
            onClick={() => onFindWholeWordChange(!findWholeWord)}
            tooltip="Whole Word"
            type="button"
          >
            <MeoWholeWordIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton
            aria-label="Case Sensitive"
            aria-pressed={findCaseSensitive}
            className={`format-button toggle-button find-option-button${findCaseSensitive ? " is-active" : ""}`}
            onClick={() => onFindCaseSensitiveChange(!findCaseSensitive)}
            tooltip="Case Sensitive"
            type="button"
          >
            <MeoCaseSensitiveIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton className="format-button" onClick={() => onRunFind(true)} tooltip="Previous Match" type="button">
            <MeoChevronUpIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton className="format-button" onClick={() => onRunFind(false)} tooltip="Next Match" type="button">
            <MeoChevronDownIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
        </div>
        <div className="find-row">
          <input
            aria-label="Replace"
            className="find-input"
            onChange={(event) => onFindReplacementChange(event.currentTarget.value)}
            onKeyDown={runReplaceFromKeyboard}
            placeholder="Replace"
            type="text"
            value={findReplacement}
          />
          <ManageTooltipButton className="format-button" onClick={onReplaceCurrent} tooltip="Replace Current Match" type="button">
            <MeoReplaceIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
          <ManageTooltipButton className="format-button" onClick={onReplaceAll} tooltip="Replace All Matches" type="button">
            <MeoReplaceAllIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
          <span aria-hidden="true" className="find-button-spacer" />
          <ManageTooltipButton aria-label="Close Find" className="format-button find-close-button" onClick={onCloseFind} tooltip="Close Find" type="button">
            <MeoXIcon aria-hidden="true" size={16} />
          </ManageTooltipButton>
        </div>
      </div>
    </div>
  );
}

function ManageMeoSelectionFormatToolbar({
  anchor,
  onAnnotate,
  onFormat,
  selectionState,
}: {
  anchor: ManageSelectionAnchor;
  onAnnotate: () => void;
  onFormat: (action: string, level?: number | { cols?: number; rows?: number }) => void;
  selectionState: ManageMeoSelectionState;
}) {
  const position = meoSelectionToolbarPosition(selectionState, anchor);
  const formatAction = (action: string) => {
    onFormat(action);
  };
  return createPortal(
    <div
      aria-label="Inline markdown formatting"
      className={`selection-inline-menu is-visible${position.isBelow ? " is-below" : ""}`}
      onPointerDown={(event) => event.preventDefault()}
      role="toolbar"
      style={{ left: position.left, top: position.top }}
    >
      <ManageTooltipButton
        aria-label="Annotations"
        className="selection-inline-button manage-selection-inline-mode-button"
        onClick={onAnnotate}
        tooltip="Annotations"
        type="button"
      >
        <IconMessagePlus aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Bold" className="selection-inline-button" data-action="bold" onClick={() => formatAction("bold")} tooltip="Bold" type="button">
        <MeoBoldIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Italic" className="selection-inline-button" data-action="italic" onClick={() => formatAction("italic")} tooltip="Italic" type="button">
        <MeoItalicIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Lineover" className="selection-inline-button" data-action="lineover" onClick={() => formatAction("lineover")} tooltip="Lineover" type="button">
        <MeoStrikethroughIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Inline Code" className="selection-inline-button" data-action="inlineCode" onClick={() => formatAction("inlineCode")} tooltip="Inline Code" type="button">
        <MeoTerminalIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Link" className="selection-inline-button" data-action="link" onClick={() => formatAction("link")} tooltip="Link" type="button">
        <MeoLinkIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Wiki Link" className="selection-inline-button" data-action="wikiLink" onClick={() => formatAction("wikiLink")} tooltip="Wiki Link" type="button">
        <MeoBracketsIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <ManageTooltipButton aria-label="Kbd" className="selection-inline-button" data-action="kbd" onClick={() => formatAction("kbd")} tooltip="Kbd" type="button">
        <MeoKeyboardIcon aria-hidden="true" size={16} />
      </ManageTooltipButton>
      <div aria-label="Suggested replacements" className="selection-inline-suggestions" hidden role="group" />
    </div>,
    document.body,
  );
}

function ManageAnnotationToolbar({
  anchor,
  onComment,
  onDismiss,
  onFormatting,
  onQuickLabel,
}: {
  anchor: ManageSelectionAnchor;
  onComment: () => void;
  onDismiss: () => void;
  onFormatting: () => void;
  onQuickLabel: (label: ManageQuickLabel) => void;
}) {
  return createPortal(
    <div
      className="manage-markdown-selection-toolbar"
      style={{
        left: clampManageSelectionToolbarLeft(anchor.left),
        top: Math.max(8, anchor.top - 46),
      }}
    >
      <AppTooltip content="Comment" side="top">
        <button
          aria-label="Comment"
          onClick={onComment}
          style={manageToolbarActionStyle(MANAGE_COMMENT_ANNOTATION_COLOR)}
          type="button"
        >
          <IconMessagePlus aria-hidden="true" size={15} />
        </button>
      </AppTooltip>
      <AppTooltip content="Formatting" side="top">
        <button
          aria-label="Formatting"
          onClick={onFormatting}
          style={manageToolbarActionStyle(MANAGE_MEO_HEADING_COLOR)}
          type="button"
        >
          <MeoBoldIcon aria-hidden="true" size={15} />
        </button>
      </AppTooltip>
      {MANAGE_QUICK_LABELS.map((label) => (
        <AppTooltip content={label.text} key={label.id} side="top">
          <button
            aria-label={label.text}
            onClick={() => onQuickLabel(label)}
            style={manageToolbarActionStyle(label.color)}
            type="button"
          >
            {renderManageQuickLabelIcon(label.id)}
          </button>
        </AppTooltip>
      ))}
      <AppTooltip content="Dismiss" side="top">
        <button
          aria-label="Dismiss"
          onClick={onDismiss}
          style={manageToolbarActionStyle(MANAGE_DISMISS_TOOLBAR_COLOR)}
          type="button"
        >
          <IconX aria-hidden="true" size={15} />
        </button>
      </AppTooltip>
    </div>,
    document.body,
  );
}

function ManageAnnotationPreviewCard({
  onRemoveAnnotation,
  preview,
}: {
  onRemoveAnnotation: (annotationId: string) => void;
  preview: ManageAnnotationPreview;
}) {
  const annotation = preview.annotation;
  const note = annotationPreviewText(annotation);
  return createPortal(
    <aside
      className="manage-annotation-preview-card"
      data-label-id={annotation.labelId}
      data-type={annotation.type}
      style={{
        ...annotationPreviewCardStyle(preview.anchor),
        "--manage-annotation-color": manageAnnotationColor(annotation),
      } as CSSProperties}
    >
      <header>
        <span>{annotationTypeLabel(annotation)}</span>
        {annotation.attachments.length > 0 ? (
          <span>
            {annotation.attachments.length} {annotation.attachments.length === 1 ? "image" : "images"}
          </span>
        ) : null}
      </header>
      <ManageTooltipButton
        aria-label="Remove annotation"
        className="manage-annotation-preview-remove-button manage-icon-button"
        onClick={(event: ReactMouseEvent<HTMLButtonElement>) => {
          event.stopPropagation();
          onRemoveAnnotation(annotation.id);
        }}
        onPointerDown={(event: ReactPointerEvent<HTMLButtonElement>) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        tooltip="Remove annotation"
        type="button"
      >
        <IconX aria-hidden="true" size={14} />
      </ManageTooltipButton>
      <p>{note}</p>
    </aside>,
    document.body,
  );
}

function ManageCommentPopover({
  draft,
  onAddAttachmentFiles,
  onCancel,
  onDraftNoteChange,
  onRemoveDraftAttachment,
  onSubmit,
}: {
  draft: ManageCommentDraft;
  onAddAttachmentFiles: (files: FileList | File[]) => void;
  onCancel: () => void;
  onDraftNoteChange: (note: string) => void;
  onRemoveDraftAttachment: (attachmentId: string) => void;
  onSubmit: () => void;
}) {
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const canSubmit = Boolean(draft.note.trim()) || draft.attachments.length > 0;
  return createPortal(
    <div className="manage-comment-popover" style={commentPopoverStyle(draft.anchor)}>
      <ManageTooltipButton
        aria-label="Close comment composer"
        className="manage-comment-popover-close manage-icon-button"
        onClick={onCancel}
        tooltip="Close"
        type="button"
      >
        <IconX aria-hidden="true" size={14} />
      </ManageTooltipButton>
      <textarea
        aria-label="Annotation note"
        autoFocus
        onChange={(event) => onDraftNoteChange(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
            return;
          }
          if ((event.metaKey || event.ctrlKey) && event.key === "Enter" && canSubmit) {
            event.preventDefault();
            onSubmit();
          }
        }}
        placeholder={draft.quote ? "Add a comment" : "Add a global comment"}
        value={draft.note}
      />
      {draft.attachments.length > 0 ? (
        <div className="manage-attachment-strip">
          {draft.attachments.map((attachment) => (
            <figure className="manage-attachment-chip" key={attachment.id}>
              <img alt="" src={attachment.dataUrl} />
              <figcaption>{attachment.name}</figcaption>
              <button
                aria-label={`Remove ${attachment.name}`}
                onClick={() => onRemoveDraftAttachment(attachment.id)}
                type="button"
              >
                <IconX aria-hidden="true" size={12} />
              </button>
            </figure>
          ))}
        </div>
      ) : null}
      {draft.attachmentError ? <div className="manage-attachment-error">{draft.attachmentError}</div> : null}
      <div className="manage-comment-popover-actions">
        {/*
         * CDXC:ManageAnnotationComposer 2026-06-28-08:31:
         * The Image action in the Markdown annotation comment composer is hidden because the current picker does not open from this surface. Keep the button source commented so the picker flow can be restored when it is fixed instead of deleting the intended UI.
         *
         * <button
         *   className="manage-comment-popover-image-button"
         *   onClick={() => attachmentInputRef.current?.click()}
         *   type="button"
         * >
         *   <IconPhoto aria-hidden="true" size={14} />
         *   Image
         * </button>
         */}
        <button
          className="manage-comment-popover-submit"
          disabled={!canSubmit}
          onClick={onSubmit}
          type="button"
        >
          <IconMessagePlus aria-hidden="true" size={14} />
          Submit
        </button>
      </div>
      <input
        accept="image/*"
        aria-label="Annotation image attachments"
        className="manage-hidden-file-input"
        multiple
        onChange={(event) => {
          if (event.currentTarget.files) {
            onAddAttachmentFiles(event.currentTarget.files);
          }
          event.currentTarget.value = "";
        }}
        ref={attachmentInputRef}
        type="file"
      />
    </div>,
    document.body,
  );
}

function ManageAnnotationDropdown({
  annotations,
  onRemoveAnnotation,
}: {
  annotations: ManageAnnotation[];
  onRemoveAnnotation: (annotationId: string) => void;
}) {
  return (
    <div
      aria-label="Annotations"
      className="manage-annotation-dropdown"
      id="manage-markdown-annotation-dropdown"
      role="dialog"
    >
      <header>
        <span>Annotations</span>
      </header>
      <div className="manage-annotation-dropdown-list">
        {annotations.length === 0 ? <div className="manage-annotation-empty">No annotations</div> : null}
        {annotations.map((annotation) => {
          const note = annotationDisplayNote(annotation);
          return (
            <article
              className="manage-annotation-card"
              data-label-id={annotation.labelId}
              data-type={annotation.type}
              key={annotation.id}
              style={{ "--manage-annotation-color": manageAnnotationColor(annotation) } as CSSProperties}
            >
              <div className="manage-annotation-card-header">
                <span>{annotationTypeLabel(annotation)}</span>
                <ManageTooltipButton
                  aria-label="Remove annotation"
                  className="manage-annotation-remove-button manage-icon-button"
                  onClick={() => onRemoveAnnotation(annotation.id)}
                  tooltip="Remove annotation"
                  type="button"
                >
                  <IconX aria-hidden="true" size={14} />
                </ManageTooltipButton>
              </div>
              {annotation.scope === "selection" ? <blockquote>{annotation.quote}</blockquote> : null}
              {note ? <p>{note}</p> : null}
              {annotation.attachments.length > 0 ? (
                <div className="manage-annotation-attachments">
                  {annotation.attachments.map((attachment) => (
                    <a href={attachment.dataUrl} key={attachment.id} rel="noreferrer" target="_blank">
                      <img alt="" src={attachment.dataUrl} />
                      <span>{attachment.name}</span>
                    </a>
                  ))}
                </div>
              ) : null}
            </article>
          );
        })}
      </div>
    </div>
  );
}

function ManageMarkdownBlockRenderer({
  annotations,
  block,
  orderedIndex,
}: {
  annotations: ManageAnnotation[];
  block: ManageMarkdownBlock;
  orderedIndex?: number;
}) {
  switch (block.type) {
    case "heading": {
      const HeadingTag = `h${Math.min(Math.max(block.level ?? 1, 1), 6)}` as "h1" | "h2" | "h3" | "h4" | "h5" | "h6";
      return (
        <HeadingTag data-block-id={block.id} data-block-type="heading">
          {renderManageInlineMarkdown(block.content, annotations)}
        </HeadingTag>
      );
    }
    case "blockquote": {
      if (block.alertKind) {
        return (
          <div className="manage-md-alert" data-kind={block.alertKind} data-block-id={block.id}>
            <div className="manage-md-alert-title">{block.alertKind}</div>
            {block.content.split(/\n\n+/u).map((paragraph, index) => (
              <p key={index}>{renderManageInlineMarkdown(paragraph, annotations)}</p>
            ))}
          </div>
        );
      }
      return (
        <blockquote data-block-id={block.id}>
          {block.content.split(/\n\n+/u).map((paragraph, index) => (
            <p key={index}>{renderManageInlineMarkdown(paragraph, annotations)}</p>
          ))}
        </blockquote>
      );
    }
    case "list-item":
      return (
        <div
          className="manage-md-list-item"
          data-block-id={block.id}
          style={{ "--manage-md-list-level": block.level ?? 0 } as CSSProperties}
        >
          <span className="manage-md-list-marker">
            {block.checked !== undefined ? (
              <input checked={block.checked} readOnly tabIndex={-1} type="checkbox" />
            ) : block.ordered ? (
              `${orderedIndex ?? block.orderedStart ?? 1}.`
            ) : (
              "*"
            )}
          </span>
          <span className={block.checked ? "manage-md-list-text is-checked" : "manage-md-list-text"}>
            {renderManageInlineMarkdown(block.content, annotations)}
          </span>
        </div>
      );
    case "code":
      return <ManageMarkdownCodeBlock block={block} />;
    case "table":
      return <ManageMarkdownTable block={block} annotations={annotations} />;
    case "hr":
      return <hr data-block-id={block.id} />;
    case "html":
      return <ManageMarkdownHtmlBlock block={block} />;
    case "directive":
      return (
        <div className="manage-md-directive" data-kind={block.directiveKind ?? "note"} data-block-id={block.id}>
          {block.content.split(/\n\n+/u).map((paragraph, index) => (
            <p key={index}>{renderManageInlineMarkdown(paragraph, annotations)}</p>
          ))}
        </div>
      );
    case "paragraph":
    default:
      return (
        <p data-block-id={block.id}>
          {renderManageInlineMarkdown(block.content, annotations)}
        </p>
      );
  }
}

function ManageMarkdownCodeBlock({ block }: { block: ManageMarkdownBlock }) {
  const [copied, setCopied] = useState(false);
  const copyCode = useCallback(async () => {
    try {
      await writeTextToClipboard(block.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1_600);
    } catch {
      setCopied(false);
    }
  }, [block.content]);
  return (
    <div className="manage-md-code-block" data-block-id={block.id}>
      <button aria-label="Copy code" onClick={() => void copyCode()} type="button">
        {copied ? <IconCheck aria-hidden="true" size={14} /> : <IconCopy aria-hidden="true" size={14} />}
      </button>
      <pre>
        <code className={block.language ? `language-${block.language}` : undefined}>{block.content}</code>
      </pre>
    </div>
  );
}

function ManageMarkdownTable({
  annotations,
  block,
}: {
  annotations: ManageAnnotation[];
  block: ManageMarkdownBlock;
}) {
  const { headers, rows } = parseManageMarkdownTableContent(block.content);
  return (
    <div className="manage-md-table-wrap" data-block-id={block.id}>
      <table>
        <thead>
          <tr>
            {headers.map((header, index) => (
              <th key={index}>{renderManageInlineMarkdown(header, annotations)}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {row.map((cell, cellIndex) => (
                <td key={cellIndex}>{renderManageInlineMarkdown(cell, annotations)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ManageMarkdownHtmlBlock({ block }: { block: ManageMarkdownBlock }) {
  const sanitized = useMemo(() => sanitizeManageBlockHtml(block.content), [block.content]);
  return (
    <div
      className="manage-md-html-block"
      data-block-id={block.id}
      data-block-type="html"
      dangerouslySetInnerHTML={{ __html: sanitized }}
    />
  );
}

function ManageExcalidrawEditor({
  content,
  fileName,
  onChange,
}: {
  content: string;
  fileName: string;
  onChange: (content: string) => void;
}) {
  const apiRef = useRef<ExcalidrawImperativeAPI | null>(null);
  const hasAcceptedInitialSceneRef = useRef(false);
  const previousSceneSignatureRef = useRef("");
  const lastSerializedRef = useRef(content);
  const [parseError, setParseError] = useState<string>();
  const parsed = useMemo(() => parseExcalidrawFile(content), [content]);

  useEffect(() => {
    if (content !== lastSerializedRef.current) {
      lastSerializedRef.current = content;
      hasAcceptedInitialSceneRef.current = false;
      previousSceneSignatureRef.current = "";
    }
  }, [content]);

  if (parsed.ok === false) {
    return (
      <div className="manage-drawing-source">
        <ManagePreviewMessage
          icon={<IconAlertTriangle aria-hidden="true" size={21} />}
          title={parseError ?? parsed.error}
        />
        <textarea
          aria-label={`${fileName} source`}
          className="manage-text-editor"
          onChange={(event) => onChange(event.currentTarget.value)}
          spellCheck={false}
          value={content}
        />
      </div>
    );
  }

  const data = parsed.data;
  const drawingElements = data.elements ?? [];
  return (
    <div className="manage-drawing-editor" onKeyDownCapture={handleManageExcalidrawKeyDown}>
      {parseError ? (
        <div className="manage-drawing-error">
          <IconAlertTriangle aria-hidden="true" size={15} />
          <span>{parseError}</span>
        </div>
      ) : null}
      <Excalidraw
        excalidrawAPI={(api) => {
          apiRef.current = api;
        }}
        initialData={{
          appState: {
            collaborators: new Map(),
            viewBackgroundColor: MANAGE_EXCALIDRAW_CANVAS_BACKGROUND,
            ...data.appState,
            theme: MANAGE_EXCALIDRAW_CANVAS_THEME,
          },
          elements: drawingElements,
          files: data.files ?? {},
        }}
        onChange={(elements, appState, files) => {
          const api = apiRef.current;
          const filesForSave = files ?? api?.getFiles() ?? {};
          const nextSignature = createExcalidrawSceneSignature(elements, appState, filesForSave);
          const nextContent = serializeExcalidrawFile(data, elements, appState, filesForSave);
          /*
           * CDXC:ManageDrawings 2026-06-20-06:14:
           * Excalidraw can emit a normalized scene while hydrating initialData. Accept that as the canvas baseline instead of marking the file dirty before the user edits the drawing.
           *
           * CDXC:ManageDrawings 2026-06-20-06:35:
           * The drawing editor should compare element versions, file ids, and persisted view state before saving. Excalidraw may call onChange repeatedly with equivalent scene data, so duplicate callbacks must not churn draft content or dirty state.
           */
          if (!hasAcceptedInitialSceneRef.current) {
            hasAcceptedInitialSceneRef.current = true;
            previousSceneSignatureRef.current = nextSignature;
            lastSerializedRef.current = nextContent;
            return;
          }
          if (nextSignature === previousSceneSignatureRef.current) {
            return;
          }
          if (nextContent === lastSerializedRef.current) {
            return;
          }
          previousSceneSignatureRef.current = nextSignature;
          lastSerializedRef.current = nextContent;
          setParseError(undefined);
          onChange(nextContent);
        }}
        theme={MANAGE_EXCALIDRAW_CANVAS_THEME}
      />
    </div>
  );
}

function handleManageExcalidrawKeyDown(event: ReactKeyboardEvent<HTMLDivElement>): void {
  /*
   * CDXC:ManageDrawingRedoHotkey 2026-08-08:
   * Excalidraw intentionally binds Ctrl+Y only on Windows, but Docs promises
   * Command+Y as redo on macOS. Invoke the mounted editor's own stable redo
   * action button so the upstream history remains the sole owner of the
   * operation and autosave observes the normal scene change.
   */
  if (
    !event.nativeEvent.isComposing &&
    event.metaKey &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.shiftKey &&
    event.key.toLocaleLowerCase() === "y"
  ) {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget
      .querySelector<HTMLButtonElement>('[data-testid="button-redo"]:not(:disabled)')
      ?.click();
    return;
  }
  suppressManageExcalidrawToolKeyBeep(event);
}

function suppressManageExcalidrawToolKeyBeep(event: ReactKeyboardEvent<HTMLDivElement>): void {
  /*
   * CDXC:ManageDrawings 2026-06-28-05:12:
   * In macOS WKWebView, Excalidraw's unmodified 1-4 tool shortcuts can still reach AppKit as unhandled keyDown events and play the failure beep. Prevent the native default on the Manage wrapper while allowing propagation to Excalidraw, and skip editable targets so text editing can still type numbers.
   */
  if (
    event.nativeEvent.isComposing ||
    event.metaKey ||
    event.ctrlKey ||
    event.altKey ||
    event.shiftKey ||
    isEditableEventTarget(event.target)
  ) {
    return;
  }
  if (/^[1-4]$/u.test(event.key)) {
    event.preventDefault();
  }
}

function ManageEmptyState({ icon, text }: { icon: ReactNode; text: string }) {
  return (
    <div className="manage-empty">
      {icon}
      <span>{text}</span>
    </div>
  );
}

function ManagePreviewMessage({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <div className="manage-preview-message">
      {icon}
      <span>{title}</span>
    </div>
  );
}

function requestManageFiles(
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

function manageFileMetadataSignature(file: Pick<ManageFilePreview, "modifiedAt" | "path" | "size">): string {
  return `${file.path}\u0000${file.modifiedAt ?? ""}\u0000${file.size ?? ""}`;
}

function createUniqueArtifactPath(
  entries: ManageFileEntry[],
  kind: ManageArtifactKind,
  directoryPath = MANAGE_DOCS_ROOT_PATH,
): string {
  const occupiedPaths = new Set(entries.map((entry) => entry.path.toLocaleLowerCase()));
  const { extension, stem } = artifactNameParts(kind);
  for (let index = 1; index < 10_000; index += 1) {
    const suffix = index === 1 ? "" : `-${index}`;
    const path = `${directoryPath}/${stem}${suffix}.${extension}`;
    if (!occupiedPaths.has(path.toLocaleLowerCase())) {
      return path;
    }
  }
  return `${directoryPath}/${stem}-${Date.now()}.${extension}`;
}

function createUniqueFolderPath(
  entries: ManageFileEntry[],
  directoryPath = MANAGE_DOCS_ROOT_PATH,
): string {
  const occupiedPaths = new Set(entries.map((entry) => entry.path.toLocaleLowerCase()));
  for (let index = 1; index < 10_000; index += 1) {
    const suffix = index === 1 ? "" : `-${index}`;
    const path = `${directoryPath}/folder${suffix}`;
    if (!occupiedPaths.has(path.toLocaleLowerCase())) {
      return path;
    }
  }
  return `${directoryPath}/folder-${Date.now()}`;
}

function createDuplicateManageFilePath(entries: ManageFileEntry[], path: string): string {
  const occupiedPaths = new Set(entries.map((entry) => entry.path.toLocaleLowerCase()));
  const parentPath = parentManagePath(path);
  const fileName = basenameManagePath(path);
  const extensionIndex = fileName.lastIndexOf(".");
  const hasExtension = extensionIndex > 0 && extensionIndex < fileName.length;
  const stem = hasExtension ? fileName.slice(0, extensionIndex) : fileName;
  const extension = hasExtension ? fileName.slice(extensionIndex) : "";
  for (let index = 1; index < 10_000; index += 1) {
    const candidateName = `${stem} (${index})${extension}`;
    const candidatePath = parentPath ? `${parentPath}/${candidateName}` : candidateName;
    if (!occupiedPaths.has(candidatePath.toLocaleLowerCase())) {
      return candidatePath;
    }
  }
  const fallbackName = `${stem} (${Date.now()})${extension}`;
  return parentPath ? `${parentPath}/${fallbackName}` : fallbackName;
}

function orderManageEntriesForTree(entries: readonly ManageFileEntry[]): ManageFileEntry[] {
  const childrenByParentPath = new Map<string, ManageFileEntry[]>();
  for (const entry of entries) {
    const parentPath = parentManagePath(entry.path);
    const siblings = childrenByParentPath.get(parentPath);
    if (siblings) {
      siblings.push(entry);
    } else {
      childrenByParentPath.set(parentPath, [entry]);
    }
  }

  const orderedEntries: ManageFileEntry[] = [];
  const visitedPaths = new Set<string>();
  const appendChildren = (parentPath: string) => {
    for (const child of childrenByParentPath.get(parentPath) ?? []) {
      if (visitedPaths.has(child.path)) {
        continue;
      }
      visitedPaths.add(child.path);
      orderedEntries.push(child);
      if (child.kind === "directory") {
        appendChildren(child.path);
      }
    }
  };

  appendChildren("");
  for (const entry of entries) {
    if (!visitedPaths.has(entry.path)) {
      orderedEntries.push(entry);
    }
  }
  return orderedEntries;
}

function canOpenManageEntryContextMenu(entry: ManageFileEntry): boolean {
  return entry.kind === "file" || entry.kind === "directory";
}

function canRenameOrDeleteManageEntry(entry: ManageFileEntry): boolean {
  return !(entry.kind === "directory" && entry.depth === 0);
}

function canCreateManageEntryChildren(entry: ManageFileEntry): boolean {
  return entry.kind === "directory";
}

function artifactNameParts(kind: ManageArtifactKind): { extension: string; stem: string } {
  switch (kind) {
    case "excalidraw":
      return { extension: "excalidraw", stem: "drawing" };
    case "html":
      return { extension: "html", stem: "page" };
    case "markdown":
      return { extension: "md", stem: "note" };
  }
}

function validateManageRenameFileName(name: string): string | undefined {
  if (!name) {
    return "Enter a file name.";
  }
  if (name === "." || name === "..") {
    return "Use a normal file name.";
  }
  if (name.includes("/") || name.includes("\\") || name.includes("\0")) {
    return "File names cannot contain path separators.";
  }
  return undefined;
}

function renameManageFilePath(path: string, nextName: string): string {
  const separatorIndex = path.lastIndexOf("/");
  if (separatorIndex === -1) {
    return nextName;
  }
  return `${path.slice(0, separatorIndex + 1)}${nextName}`;
}

function parentManagePath(path: string): string {
  const separatorIndex = path.lastIndexOf("/");
  return separatorIndex === -1 ? "" : path.slice(0, separatorIndex);
}

function basenameManagePath(path: string): string {
  const separatorIndex = path.lastIndexOf("/");
  return separatorIndex === -1 ? path : path.slice(separatorIndex + 1);
}

function isManageDescendantPath(path: string, ancestorPath: string): boolean {
  return path.startsWith(`${ancestorPath}/`);
}

function createInitialCollapsedManageDirectoryPaths(entries: ManageFileEntry[]): Set<string> {
  const parentPaths = new Set<string>();
  for (const entry of entries) {
    const parentPath = parentManagePath(entry.path);
    if (parentPath) {
      parentPaths.add(parentPath);
    }
  }
  return new Set(
    entries
      .filter((entry) => entry.kind === "directory" && parentPaths.has(entry.path))
      .map((entry) => entry.path),
  );
}

function hasCollapsedManageAncestor(path: string, collapsedDirectoryPaths: Set<string>): boolean {
  for (const collapsedPath of collapsedDirectoryPaths) {
    if (isManageDescendantPath(path, collapsedPath)) {
      return true;
    }
  }
  return false;
}

/**
 * CDXC:DocsSidebar 2026-06-30-21:39:
 * Docs file search must keep each matching row's existing parent folders visible so nested matches retain folder context, while nonmatching siblings stay hidden and the user's collapsed-folder state remains unchanged outside search mode.
 */
function filterManageEntriesForSearch(
  treeOrderedEntries: readonly ManageFileEntry[],
  normalizedQuery: string,
): ManageFileEntry[] {
  const entryPaths = new Set(treeOrderedEntries.map((entry) => entry.path));
  const visiblePaths = new Set<string>();

  for (const entry of treeOrderedEntries) {
    if (!entry.path.toLocaleLowerCase().includes(normalizedQuery)) {
      continue;
    }
    visiblePaths.add(entry.path);

    let parentPath = parentManagePath(entry.path);
    while (parentPath) {
      if (entryPaths.has(parentPath)) {
        visiblePaths.add(parentPath);
      }
      parentPath = parentManagePath(parentPath);
    }
  }

  return treeOrderedEntries.filter((entry) => visiblePaths.has(entry.path));
}

function moveManagePathToDirectory(path: string, targetDirectoryPath: string): string | undefined {
  const fileName = basenameManagePath(path);
  if (!fileName || targetDirectoryPath.length === 0) {
    return undefined;
  }
  return `${targetDirectoryPath}/${fileName}`;
}

function dropDirectoryPathForManageEntry(entry: ManageFileEntry): string {
  return entry.kind === "directory" ? entry.path : parentManagePath(entry.path) || MANAGE_DOCS_ROOT_PATH;
}

function canMoveManageEntryToDirectory(
  entry: ManageFileEntry,
  targetDirectoryPath: string,
  entries: readonly ManageFileEntry[],
): boolean {
  if (targetDirectoryPath !== MANAGE_DOCS_ROOT_PATH) {
    const targetEntry = entries.find((candidate) => candidate.path === targetDirectoryPath);
    if (targetEntry?.kind !== "directory") {
      return false;
    }
  }
  if (entry.path === targetDirectoryPath || parentManagePath(entry.path) === targetDirectoryPath) {
    return false;
  }
  if (entry.kind === "directory" && isManageDescendantPath(targetDirectoryPath, entry.path)) {
    return false;
  }
  const nextPath = moveManagePathToDirectory(entry.path, targetDirectoryPath);
  if (!nextPath || nextPath === entry.path) {
    return false;
  }
  return !entries.some(
    (candidate) =>
      candidate.path !== entry.path &&
      candidate.path.toLocaleLowerCase() === nextPath.toLocaleLowerCase(),
  );
}

function remapManagePathByMove(path: string, sourcePath: string, destinationPath: string): string | undefined {
  if (path === sourcePath) {
    return destinationPath;
  }
  if (isManageDescendantPath(path, sourcePath)) {
    return `${destinationPath}${path.slice(sourcePath.length)}`;
  }
  return undefined;
}

function remapManageAnnotationPathsForMove(
  annotationsByPath: Record<string, ManageAnnotation[]>,
  sourcePath: string,
  destinationPath: string,
): Record<string, ManageAnnotation[]> {
  let changed = false;
  const next: Record<string, ManageAnnotation[]> = {};
  for (const [path, annotations] of Object.entries(annotationsByPath)) {
    const movedPath = remapManagePathByMove(path, sourcePath, destinationPath);
    if (movedPath) {
      changed = true;
      next[movedPath] = [...(next[movedPath] ?? []), ...annotations];
    } else {
      next[path] = [...(next[path] ?? []), ...annotations];
    }
  }
  return changed ? next : annotationsByPath;
}

function remapManagePathSetForMove(
  paths: Set<string>,
  sourcePath: string,
  destinationPath: string,
): Set<string> {
  const next = new Set<string>();
  for (const path of paths) {
    next.add(remapManagePathByMove(path, sourcePath, destinationPath) ?? path);
  }
  return next;
}

function removeManageAnnotationPathsForDeletedEntry(
  annotationsByPath: Record<string, ManageAnnotation[]>,
  deletedPath: string,
): Record<string, ManageAnnotation[]> {
  let changed = false;
  const next: Record<string, ManageAnnotation[]> = {};
  for (const [path, annotations] of Object.entries(annotationsByPath)) {
    if (path === deletedPath || isManageDescendantPath(path, deletedPath)) {
      changed = true;
      continue;
    }
    next[path] = [...annotations];
  }
  return changed ? next : annotationsByPath;
}

function removeManagePathSetForDeletedEntry(paths: Set<string>, deletedPath: string): Set<string> {
  let changed = false;
  const next = new Set<string>();
  for (const path of paths) {
    if (path === deletedPath || isManageDescendantPath(path, deletedPath)) {
      changed = true;
      continue;
    }
    next.add(path);
  }
  return changed ? next : paths;
}

function createInitialArtifactContent(kind: ManageArtifactKind): string {
  switch (kind) {
    case "excalidraw":
      return `${JSON.stringify(createEmptyExcalidrawFile(), null, 2)}\n`;
    case "html":
      return createDefaultHtmlDocument();
    case "markdown":
      return "# Untitled\n\n";
  }
}

function createDefaultHtmlDocument(): string {
  /*
   * CDXC:ManageDefaultHtml 2026-06-28-07:17:
   * The default HTML document is user-facing onboarding copy, not a blank placeholder. It should teach users to ask an agent for a polished explanatory HTML page, then review and annotate exact rendered sections with Agentation.
   * Keep the document self-contained with inline dark-mode styles and no scripts so it remains portable, while Manage now preserves author styles for real HTML rendering.
   *
   * CDXC:ManageHtmlAgentation 2026-06-28-07:58:
   * The starter copy should describe Agentation as an idle bottom-left control on open. Users explicitly start feedback mode from Agentation only when they are ready to annotate.
   */
  return [
    "<!doctype html>",
    '<html lang="en">',
    "<head>",
    '  <meta charset="utf-8">',
    '  <meta name="viewport" content="width=device-width, initial-scale=1">',
    '  <meta name="color-scheme" content="dark">',
    "  <title>Ask an agent for an HTML explainer</title>",
    "  <style>",
    "    :root { color-scheme: dark; background: #0e0e0e; }",
    "    * { box-sizing: border-box; }",
    "    html { background: #0e0e0e; min-width: 0; }",
    "    body { margin: 0; min-width: 0; overflow-x: hidden; background: #0e0e0e; color: #c8cdd5; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"SF Pro Text\", \"Segoe UI\", sans-serif; }",
    "    main { min-height: 100vh; width: 100%; background: #0e0e0e; padding: 42px 30px 52px; }",
    "    .docs-shell { width: min(100%, 980px); margin: 0 auto; display: grid; gap: 18px; }",
    "    .docs-hero { background: #151515; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 8px; padding: 30px; box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28); }",
    "    .docs-eyebrow { margin: 0 0 12px; color: #95d7f6; font-size: 12px; font-weight: 760; letter-spacing: 0; text-transform: uppercase; }",
    "    h1 { margin: 0; color: #f3f4f6; font-size: 46px; line-height: 1.02; letter-spacing: 0; max-width: 780px; }",
    "    .docs-lede { margin: 18px 0 0; color: #a6adb6; font-size: 17px; line-height: 1.65; max-width: 760px; }",
    "    .docs-card-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }",
    "    .docs-card { min-width: 0; background: #181818; border: 1px solid rgba(255, 255, 255, 0.11); border-radius: 8px; padding: 18px; }",
    "    .docs-card-kicker { margin: 0 0 10px; color: #95d7f6; font-size: 12px; font-weight: 760; text-transform: uppercase; }",
    "    .docs-card h2, .docs-prompt h2 { margin: 0 0 8px; color: #f3f4f6; font-size: 20px; line-height: 1.2; letter-spacing: 0; }",
    "    .docs-card p { margin: 0; color: #a6adb6; font-size: 14px; line-height: 1.55; }",
    "    .docs-card p + p { margin-top: 10px; }",
    "    .docs-card strong { color: #f3f4f6; }",
    "    .docs-prompt { background: #101112; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 8px; padding: 22px; }",
    "    .docs-prompt pre { margin: 0; overflow-x: auto; white-space: pre-wrap; background: #222426; border: 1px solid rgba(255, 255, 255, 0.10); border-radius: 8px; color: #e5e7eb; font: 13px/1.65 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, \"Liberation Mono\", monospace; padding: 16px; }",
    "    @media (max-width: 760px) { main { padding: 30px 18px 42px; } .docs-hero { padding: 24px; } h1 { font-size: 38px; } .docs-card-grid { grid-template-columns: 1fr; } }",
    "    @media (max-width: 520px) { main { padding: 24px 14px 36px; } .docs-hero, .docs-card, .docs-prompt { padding: 16px; } h1 { font-size: 32px; } .docs-lede { font-size: 15px; } }",
    "  </style>",
    "</head>",
    "<body>",
    '  <main aria-labelledby="docs-title">',
    '    <section class="docs-shell">',
    '      <header class="docs-hero">',
    '        <p class="docs-eyebrow">Ghostex Docs</p>',
    '        <h1 id="docs-title">Ask your agent for an HTML explainer</h1>',
    '        <p class="docs-lede">Use this starter as a prompt target. Ask an agent to replace it with a focused HTML document that explains a feature, workflow, bug, decision, or research topic in a way your team can scan and discuss.</p>',
    "      </header>",
    '      <section class="docs-card-grid">',
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">1. Ask</p>',
    "          <h2>Tell your agent what to explain</h2>",
    "          <p>Name the topic, audience, and level of detail. Ask for sections, examples, diagrams, tables, or callouts when they help.</p>",
    "        </article>",
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">2. Review</p>',
    "          <h2>Open the rendered document</h2>",
    "          <p>Read it in Docs like a real page. Check whether the structure, labels, and examples make the explanation clear.</p>",
    "        </article>",
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">3. Annotate</p>',
    "          <h2>Use Agentation for feedback</h2>",
    "          <p>Use the bottom-left Agentation control when you are ready. Point at the exact paragraph, diagram, or layout issue, then leave notes your agent can act on.</p>",
    "        </article>",
    '        <article class="docs-card">',
    '          <p class="docs-card-kicker">4. Refine</p>',
    "          <h2>Make feedback actionable</h2>",
    "          <p><strong>Good requests are specific.</strong> Ask for the job the document should do: onboard a teammate, explain a tradeoff, compare options, summarize an incident, or teach a workflow.</p>",
    "          <p><strong>Good annotations are precise.</strong> Mark the part that is confusing, missing, too dense, or visually off, then ask your agent to revise this HTML file.</p>",
    "        </article>",
    "      </section>",
    '      <section class="docs-prompt">',
    "        <h2>Prompt to try</h2>",
    "        <pre>Create an HTML document in docs/ that explains &lt;topic&gt; for &lt;audience&gt;. Make it dark, polished, and easy to scan. Use document-owned styles, clear sections, practical examples, and a small diagram or table if it helps. Keep it self-contained so I can annotate it in Ghostex Docs with Agentation.</pre>",
    "      </section>",
    "    </section>",
    "  </main>",
    "</body>",
    "</html>",
    "",
  ].join("\n");
}

function formatFileSize(size: number): string {
  if (size < 1024) {
    return `${size} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = size / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

function languageLabelForPath(path: string): string {
  const extension = path.split(".").pop()?.toLocaleLowerCase();
  if (!extension || extension === path) {
    return "Text";
  }
  const labels: Record<string, string> = {
    css: "CSS",
    excalidraw: "Excalidraw",
    go: "Go",
    h: "C/C++",
    html: "HTML",
    js: "JavaScript",
    json: "JSON",
    jsx: "React",
    md: "Markdown",
    mjs: "JavaScript",
    py: "Python",
    rs: "Rust",
    sh: "Shell",
    swift: "Swift",
    ts: "TypeScript",
    tsx: "React",
    txt: "Text",
    yaml: "YAML",
    yml: "YAML",
    zig: "Zig",
  };
  return labels[extension] ?? extension.toLocaleUpperCase();
}

function fileIconForPath(path: string) {
  if (isMarkdownPath(path)) {
    return IconMarkdown;
  }
  if (isHtmlPath(path)) {
    return IconFileTypeHtml;
  }
  if (isExcalidrawPath(path)) {
    return IconEdit;
  }
  return IconFile;
}

function isMarkdownPath(path: string): boolean {
  return /\.(md|markdown|mdown|mkdn)$/iu.test(path);
}

function isExcalidrawPath(path: string): boolean {
  return /\.excalidraw$/iu.test(path);
}

function shouldAutosaveManageFile(path: string): boolean {
  return isMarkdownPath(path) || isExcalidrawPath(path);
}

function isHtmlPath(path: string): boolean {
  return /\.html?$/iu.test(path);
}

function annotationPersistenceLabel(state: "idle" | "loading" | "ready" | "saving" | "saved" | "error"): string {
  switch (state) {
    case "error":
      return "Not saved";
    case "loading":
      return "Loading";
    case "saved":
      return "Saved";
    case "saving":
      return "Saving";
    case "idle":
    case "ready":
      return "Local";
  }
}

function annotationTypeLabel(annotation: ManageAnnotation): string {
  if (annotation.type === "redline") {
    return "Redline";
  }
  if (annotation.labelId) {
    return quickLabelText(annotation.labelId);
  }
  return annotation.scope === "global" ? "Global comment" : "Comment";
}

function annotationDisplayNote(annotation: ManageAnnotation): string {
  const note = annotation.note.trim();
  if (!note) {
    return "";
  }
  return annotation.labelId && note === quickLabelText(annotation.labelId) ? "" : note;
}

function quickLabelText(labelId: ManageQuickLabelId): string {
  return MANAGE_QUICK_LABELS.find((label) => label.id === labelId)?.text ?? labelId;
}

function quickLabelColor(labelId: ManageQuickLabelId | undefined): string {
  return MANAGE_QUICK_LABELS.find((label) => label.id === labelId)?.color ?? MANAGE_COMMENT_ANNOTATION_COLOR;
}

function manageAnnotationColor(annotation: Pick<ManageAnnotation, "labelId" | "type">): string {
  return annotation.type === "redline" ? MANAGE_REDLINE_ANNOTATION_COLOR : quickLabelColor(annotation.labelId);
}

function manageToolbarActionStyle(color: string): CSSProperties {
  return { "--manage-toolbar-action-color": color } as CSSProperties;
}

function clampManageSelectionToolbarLeft(left: number): number {
  const halfToolbarWidth = Math.min(
    MANAGE_SELECTION_TOOLBAR_WIDTH_ESTIMATE / 2,
    Math.max(0, window.innerWidth / 2 - MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN),
  );
  const minLeft = MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN + halfToolbarWidth;
  const maxLeft = Math.max(minLeft, window.innerWidth - MANAGE_SELECTION_TOOLBAR_EDGE_MARGIN - halfToolbarWidth);
  return Math.min(Math.max(left, minLeft), maxLeft);
}

function meoSelectionToolbarPosition(
  selectionState: ManageMeoSelectionState,
  fallbackAnchor: ManageSelectionAnchor,
): { isBelow: boolean; left: number; top: number } {
  const margin = 8;
  const estimatedWidth = 236;
  const estimatedHeight = 34;
  const anchorX =
    typeof selectionState.anchorX === "number" && Number.isFinite(selectionState.anchorX)
      ? selectionState.anchorX
      : fallbackAnchor.left;
  const anchorY =
    typeof selectionState.anchorY === "number" && Number.isFinite(selectionState.anchorY)
      ? selectionState.anchorY
      : fallbackAnchor.top;
  const anchorBottomY =
    typeof selectionState.anchorBottomY === "number" && Number.isFinite(selectionState.anchorBottomY)
      ? selectionState.anchorBottomY
      : fallbackAnchor.top;
  const rawLeft = selectionState.align === "start" ? anchorX : anchorX - estimatedWidth / 2;
  const maxLeft = Math.max(margin, window.innerWidth - estimatedWidth - margin);
  const toolbarBottom =
    (document.querySelector(".manage-meo-markdown-editor .mode-toolbar") as HTMLElement | null)?.getBoundingClientRect().bottom ??
    0;
  const aboveTop = anchorY - margin - estimatedHeight;
  const isBelow = aboveTop < toolbarBottom + margin;
  return {
    isBelow,
    left: Math.min(maxLeft, Math.max(margin, rawLeft)),
    top: Math.max(margin, isBelow ? anchorBottomY + margin : anchorY - margin),
  };
}

function renderManageQuickLabelIcon(labelId: ManageQuickLabelId): ReactNode {
  switch (labelId) {
    case "clarify":
      return <IconHelpCircle aria-hidden="true" size={15} />;
    case "needs-tests":
      return <IconTestPipe aria-hidden="true" size={15} />;
    case "looks-good":
      return <IconCircleCheck aria-hidden="true" size={15} />;
  }
}

function normalizeAnnotationQuote(text: string): string {
  return text.replace(/\s+/g, " ").trim().slice(0, MANAGE_SELECTION_MAX_LENGTH);
}

function selectionAnchorFromRect(rect: DOMRect | undefined): ManageSelectionAnchor | undefined {
  if (!rect || rect.width === 0 || rect.height === 0) {
    return undefined;
  }
  const left = Math.min(Math.max(rect.left + rect.width / 2, 12), window.innerWidth - 12);
  const top = Math.min(Math.max(rect.top, 12), window.innerHeight - 12);
  return { left, top };
}

function defaultManageSelectionAnchor(): ManageSelectionAnchor {
  return {
    left: Math.min(Math.max(window.innerWidth / 2, 12), window.innerWidth - 12),
    top: Math.min(Math.max(72, 12), window.innerHeight - 12),
  };
}

function applyManageMeoTheme(): void {
  const rootStyle = document.documentElement.style;
  rootStyle.setProperty(
    "--vscode-editor-font-family",
    'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  );
  rootStyle.setProperty("--vscode-editor-font-size", "14px");
  rootStyle.setProperty("--vscode-editor-font-weight", "450");
  rootStyle.setProperty("--vscode-editor-background", "#101112");
  rootStyle.setProperty("--vscode-editor-foreground", "#e5e7eb");
  rootStyle.setProperty("--vscode-sideBar-background", "#17191c");
  rootStyle.setProperty("--vscode-panel-border", "rgba(255, 255, 255, 0.10)");
  rootStyle.setProperty("--vscode-editor-selectionBackground", "rgba(125, 211, 252, 0.28)");
  rootStyle.setProperty("--vscode-editorWidget-background", "#17191c");
  applyMeoThemeSettings(MANAGE_MEO_THEME);
}

function createManageMeoAnnotationDecorations(
  text: string,
  annotations: readonly ManageAnnotation[],
): ManageMeoAnnotationDecoration[] {
  return collectManageAnnotationRanges(text, annotations).map((range) => ({
    from: range.from,
    labelId: range.annotation.labelId,
    to: range.to,
    type: range.annotation.type,
  }));
}

function buildManageMeoAnnotationDecorations(decorations: readonly ManageMeoAnnotationDecoration[]): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const orderedDecorations = decorations
    .filter((decoration) => decoration.from >= 0 && decoration.to > decoration.from)
    .sort((left, right) => left.from - right.from || left.to - right.to);
  for (const decoration of orderedDecorations) {
    builder.add(
      decoration.from,
      decoration.to,
      Decoration.mark({
        attributes: {
          "data-type": decoration.type,
          ...(decoration.labelId ? { "data-label-id": decoration.labelId } : {}),
          style: `--manage-annotation-color: ${manageAnnotationColor(decoration)};`,
        },
        class: `annotation-highlight manage-annotation-highlight ${decoration.type === "redline" ? "deletion" : "comment"}`,
      }),
    );
  }
  return builder.finish();
}

function collectManageAnnotationRanges(
  text: string,
  annotations: readonly ManageAnnotation[],
): ManageResolvedAnnotationRange[] {
  const ranges: ManageResolvedAnnotationRange[] = [];
  for (const annotation of annotations) {
    if (annotation.scope !== "selection") {
      continue;
    }
    for (const match of findManageAnnotationTextMatches(text, annotation.quote)) {
      ranges.push({
        annotation,
        from: match.from,
        labelId: annotation.labelId,
        to: match.to,
        type: annotation.type,
      });
    }
  }
  return ranges;
}

function findManageAnnotationTextMatches(text: string, quote: string): Array<{ from: number; to: number }> {
  const normalizedQuote = normalizeAnnotationQuote(quote);
  if (!normalizedQuote) {
    return [];
  }
  const normalizedText = buildManageNormalizedTextIndex(text);
  const matches: Array<{ from: number; to: number }> = [];
  let fromIndex = 0;
  while (fromIndex < normalizedText.text.length) {
    const matchIndex = normalizedText.text.indexOf(normalizedQuote, fromIndex);
    if (matchIndex < 0) {
      break;
    }
    const start = normalizedText.positions[matchIndex];
    const end = normalizedText.positions[matchIndex + normalizedQuote.length - 1];
    if (typeof start === "number" && typeof end === "number" && end >= start) {
      matches.push({ from: start, to: end + 1 });
    }
    fromIndex = matchIndex + normalizedQuote.length;
  }
  return matches;
}

function buildManageNormalizedTextIndex(text: string): { positions: number[]; text: string } {
  const positions: number[] = [];
  let normalized = "";
  let previousWasWhitespace = true;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index] ?? "";
    if (/\s/u.test(character)) {
      if (!previousWasWhitespace) {
        normalized += " ";
        positions.push(index);
        previousWasWhitespace = true;
      }
      continue;
    }
    normalized += character;
    positions.push(index);
    previousWasWhitespace = false;
  }
  if (normalized.endsWith(" ")) {
    normalized = normalized.slice(0, -1);
    positions.pop();
  }
  return { positions, text: normalized };
}

function syncManageMeoAnnotationReviewState(
  view: EditorView,
  annotations: readonly ManageAnnotation[],
  onSelectionCapture: (selection: ManageCapturedSelection) => void,
  onSelectionClear: () => void,
  onAnnotationPreviewChange: (preview: ManageAnnotationPreview | undefined) => void,
): void {
  const selection = view.state.selection.main;
  const documentLength = view.state.doc.length;
  if (!selection.empty) {
    const from = Math.max(0, Math.min(Math.floor(Math.min(selection.from, selection.to)), documentLength));
    const to = Math.max(from, Math.min(Math.floor(Math.max(selection.from, selection.to)), documentLength));
    const text = view.state.doc.sliceString(from, to);
    if (!normalizeAnnotationQuote(text)) {
      onSelectionClear();
      onAnnotationPreviewChange(undefined);
      return;
    }
    onAnnotationPreviewChange(undefined);
    onSelectionCapture({
      anchor: manageEditorRangeAnchor(view, from, to) ?? defaultManageSelectionAnchor(),
      text,
    });
    return;
  }

  onSelectionClear();
  const caretPosition = Math.max(0, Math.min(Math.floor(selection.from), documentLength));
  const activeRange = findManageAnnotationRangeAtPosition(view.state.doc.toString(), annotations, caretPosition);
  if (!activeRange) {
    onAnnotationPreviewChange(undefined);
    return;
  }
  onAnnotationPreviewChange({
    anchor: manageEditorRangeAnchor(view, activeRange.from, activeRange.to) ?? defaultManageSelectionAnchor(),
    annotation: activeRange.annotation,
  });
}

function findManageAnnotationRangeAtPosition(
  text: string,
  annotations: readonly ManageAnnotation[],
  position: number,
): ManageResolvedAnnotationRange | undefined {
  return collectManageAnnotationRanges(text, annotations)
    .filter((range) => position >= range.from && position < range.to)
    .sort((left, right) => left.to - left.from - (right.to - right.from) || left.from - right.from)[0];
}

function manageEditorRangeAnchor(view: EditorView, from: number, to: number): ManageSelectionAnchor | undefined {
  const documentLength = view.state.doc.length;
  const rangeFrom = Math.max(0, Math.min(Math.floor(from), documentLength));
  const rangeTo = Math.max(rangeFrom, Math.min(Math.floor(to), documentLength));
  if (rangeTo <= rangeFrom) {
    return undefined;
  }
  const rects = manageEditorRangeRects(view, rangeFrom, rangeTo);
  if (rects.length > 0) {
    const left = Math.min(...rects.map((rect) => rect.left));
    const right = Math.max(...rects.map((rect) => rect.right));
    const top = Math.min(...rects.map((rect) => rect.top));
    return {
      left: Math.min(Math.max((left + right) / 2, 12), window.innerWidth - 12),
      top: Math.min(Math.max(top, 12), window.innerHeight - 12),
    };
  }
  const coords = view.coordsAtPos(rangeFrom);
  if (!coords) {
    return undefined;
  }
  return {
    left: Math.min(Math.max((coords.left + coords.right) / 2, 12), window.innerWidth - 12),
    top: Math.min(Math.max(coords.top, 12), window.innerHeight - 12),
  };
}

function manageEditorRangeRects(view: EditorView, from: number, to: number): DOMRect[] {
  try {
    const start = view.domAtPos(from);
    const end = view.domAtPos(to);
    const range = document.createRange();
    range.setStart(start.node, start.offset);
    range.setEnd(end.node, end.offset);
    const rects = Array.from(range.getClientRects()).filter((rect) => rect.width > 0 && rect.height > 0);
    range.detach();
    return rects;
  } catch {
    return [];
  }
}

function annotationPreviewText(annotation: ManageAnnotation): string {
  const note = annotationDisplayNote(annotation);
  if (note) {
    return truncateManageAnnotationPreviewText(note);
  }
  if (annotation.labelId) {
    return quickLabelText(annotation.labelId);
  }
  if (annotation.type === "redline") {
    return "Marked for deletion";
  }
  return truncateManageAnnotationPreviewText(annotation.quote);
}

function truncateManageAnnotationPreviewText(text: string): string {
  const normalized = normalizeAnnotationQuote(text);
  return normalized.length > 150 ? `${normalized.slice(0, 147)}...` : normalized;
}

function annotationPreviewCardStyle(anchor: ManageSelectionAnchor): CSSProperties {
  const width = Math.min(320, Math.max(240, window.innerWidth - 24));
  const halfWidth = width / 2;
  return {
    left: Math.min(Math.max(anchor.left, 12 + halfWidth), window.innerWidth - 12 - halfWidth),
    top: Math.max(12, anchor.top - 96),
    width,
  };
}

function commentPopoverStyle(anchor: ManageSelectionAnchor): CSSProperties {
  const width = Math.min(360, Math.max(280, window.innerWidth - 24));
  const left = Math.min(Math.max(anchor.left - width / 2, 12), window.innerWidth - width - 12);
  const maxTop = Math.max(12, window.innerHeight - 260);
  const top = Math.min(Math.max(anchor.top + 12, 12), maxTop);
  return {
    left,
    top,
    width,
  };
}

function readStoredManageSidebarSide(): ManageSidebarSide {
  return window.localStorage.getItem(MANAGE_SIDEBAR_SIDE_STORAGE_KEY) === "left" ? "left" : "right";
}

function readStoredManageSidebarWidth(): number {
  const parsedWidth = Number(window.localStorage.getItem(MANAGE_SIDEBAR_WIDTH_STORAGE_KEY));
  return clampManageSidebarWidth(
    Number.isFinite(parsedWidth) && parsedWidth > 0 ? parsedWidth : MANAGE_SIDEBAR_DEFAULT_WIDTH,
    window.innerWidth,
  );
}

function clampManageSidebarWidth(width: number, containerWidth: number): number {
  const maxForContainer = Math.max(
    MANAGE_SIDEBAR_MIN_WIDTH,
    Math.min(MANAGE_SIDEBAR_MAX_WIDTH, Math.floor(containerWidth * 0.46)),
  );
  return Math.min(Math.max(Math.round(width), MANAGE_SIDEBAR_MIN_WIDTH), maxForContainer);
}

const MANAGE_MARKDOWN_HTML_BLOCK_TAGS = new Set([
  "article",
  "aside",
  "blockquote",
  "details",
  "div",
  "figure",
  "footer",
  "form",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "header",
  "hr",
  "main",
  "nav",
  "ol",
  "p",
  "pre",
  "section",
  "table",
  "ul",
]);

function parseManageMarkdownToBlocks(markdown: string): ManageMarkdownBlock[] {
  const body = extractManageMarkdownBody(markdown);
  const lines = body.split("\n");
  const blocks: ManageMarkdownBlock[] = [];
  let index = 0;
  let order = 0;

  const pushBlock = (
    type: ManageMarkdownBlock["type"],
    content: string,
    startLine: number,
    extra: Partial<ManageMarkdownBlock> = {},
  ) => {
    blocks.push({
      content,
      id: `manage-md-block-${order}-${startLine}`,
      order,
      startLine,
      type,
      ...extra,
    });
    order += 1;
  };

  while (index < lines.length) {
    const line = lines[index] ?? "";
    const startLine = index + 1;
    if (!line.trim()) {
      index += 1;
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/u);
    if (heading) {
      pushBlock("heading", heading[2] ?? "", startLine, { level: heading[1]?.length ?? 1 });
      index += 1;
      continue;
    }

    if (/^\s{0,3}(?:([-*_])(?:\s*\1){2,})\s*$/u.test(line)) {
      pushBlock("hr", "", startLine);
      index += 1;
      continue;
    }

    const directive = line.match(/^\s*:::\s*([A-Za-z][\w-]*)\s*$/u);
    if (directive) {
      const contentLines: string[] = [];
      index += 1;
      while (index < lines.length && !/^\s*:::\s*$/u.test(lines[index] ?? "")) {
        contentLines.push(lines[index] ?? "");
        index += 1;
      }
      if (index < lines.length) {
        index += 1;
      }
      pushBlock("directive", contentLines.join("\n").trim(), startLine, {
        directiveKind: directive[1]?.toLocaleLowerCase(),
      });
      continue;
    }

    const fence = line.match(/^\s{0,3}(`{3,}|~{3,})(.*)$/u);
    if (fence) {
      const marker = fence[1] ?? "```";
      const markerChar = marker[0] ?? "`";
      const markerLength = marker.length;
      const language = (fence[2] ?? "").trim().split(/\s+/u)[0] ?? "";
      const contentLines: string[] = [];
      index += 1;
      while (index < lines.length) {
        const close = (lines[index] ?? "").match(/^\s{0,3}(`{3,}|~{3,})\s*$/u);
        if (close && close[1]?.[0] === markerChar && close[1].length >= markerLength) {
          index += 1;
          break;
        }
        contentLines.push(lines[index] ?? "");
        index += 1;
      }
      pushBlock("code", contentLines.join("\n"), startLine, { language });
      continue;
    }

    if (/^\s{0,3}>\s?/u.test(line)) {
      const quoteLines: string[] = [];
      while (index < lines.length && /^\s{0,3}>\s?/u.test(lines[index] ?? "")) {
        quoteLines.push((lines[index] ?? "").replace(/^\s{0,3}>\s?/u, ""));
        index += 1;
      }
      const alert = quoteLines[0]?.trim().match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*$/iu);
      if (alert) {
        pushBlock("blockquote", quoteLines.slice(1).join("\n").trim(), startLine, {
          alertKind: alert[1]?.toLocaleLowerCase() as ManageMarkdownAlertKind,
        });
      } else {
        pushBlock("blockquote", quoteLines.join("\n").trim(), startLine);
      }
      continue;
    }

    if (isManageMarkdownTableStart(lines, index)) {
      const tableLines = [line, lines[index + 1] ?? ""];
      index += 2;
      while (index < lines.length && lineHasUnescapedPipe(lines[index] ?? "")) {
        tableLines.push(lines[index] ?? "");
        index += 1;
      }
      pushBlock("table", tableLines.join("\n"), startLine);
      continue;
    }

    const list = line.match(/^(\s*)([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?(.*)$/u);
    if (list) {
      const marker = list[2] ?? "-";
      const checkbox = list[3];
      const contentLines = [list[4] ?? ""];
      const indentLength = expandManageMarkdownIndent(list[1] ?? "").length;
      index += 1;
      while (index < lines.length) {
        const nextLine = lines[index] ?? "";
        if (!nextLine.trim() || isManageMarkdownBlockStart(lines, index)) {
          break;
        }
        if (expandManageMarkdownIndent(nextLine).length > indentLength) {
          contentLines.push(nextLine.trim());
          index += 1;
          continue;
        }
        break;
      }
      const orderedStartMatch = marker.match(/^(\d+)/u);
      pushBlock("list-item", contentLines.join("\n").trim(), startLine, {
        checked: checkbox ? /\[[xX]\]/u.test(checkbox) : undefined,
        level: Math.floor(indentLength / 2),
        ordered: Boolean(orderedStartMatch),
        orderedStart: orderedStartMatch ? Number(orderedStartMatch[1]) : undefined,
      });
      continue;
    }

    const htmlTag = line.match(/^\s{0,3}<([A-Za-z][\w-]*)(?:\s|>|\/>)/u)?.[1]?.toLocaleLowerCase();
    if (htmlTag && MANAGE_MARKDOWN_HTML_BLOCK_TAGS.has(htmlTag)) {
      const htmlLines = [line];
      index += 1;
      if (!line.includes(`</${htmlTag}>`) && !/\/>\s*$/u.test(line)) {
        while (index < lines.length) {
          const nextLine = lines[index] ?? "";
          if (!nextLine.trim()) {
            break;
          }
          htmlLines.push(nextLine);
          index += 1;
          if (nextLine.includes(`</${htmlTag}>`)) {
            break;
          }
        }
      }
      pushBlock("html", htmlLines.join("\n"), startLine);
      continue;
    }

    const paragraphLines = [line.trim()];
    index += 1;
    while (index < lines.length && lines[index]?.trim() && !isManageMarkdownBlockStart(lines, index)) {
      paragraphLines.push((lines[index] ?? "").trim());
      index += 1;
    }
    pushBlock("paragraph", paragraphLines.join(" "), startLine);
  }

  return blocks;
}

function extractManageMarkdownBody(markdown: string): string {
  const normalized = markdown.replace(/\r\n?/gu, "\n");
  const frontmatter = normalized.match(/^---[ \t]*\n[\s\S]*?\n---[ \t]*(?:\n|$)/u);
  return frontmatter ? normalized.slice(frontmatter[0].length) : normalized;
}

function isManageMarkdownBlockStart(lines: string[], index: number): boolean {
  const line = lines[index] ?? "";
  if (!line.trim()) {
    return false;
  }
  return (
    /^(#{1,6})\s+/u.test(line) ||
    /^\s{0,3}(?:([-*_])(?:\s*\1){2,})\s*$/u.test(line) ||
    /^\s*:::\s*([A-Za-z][\w-]*)\s*$/u.test(line) ||
    /^\s{0,3}(`{3,}|~{3,})/u.test(line) ||
    /^\s{0,3}>\s?/u.test(line) ||
    /^(\s*)([-*+]|\d+[.)])\s+/u.test(line) ||
    isManageMarkdownTableStart(lines, index) ||
    Boolean(line.match(/^\s{0,3}<([A-Za-z][\w-]*)(?:\s|>|\/>)/u)?.[1])
  );
}

function isManageMarkdownTableStart(lines: string[], index: number): boolean {
  const line = lines[index] ?? "";
  const divider = lines[index + 1] ?? "";
  return lineHasUnescapedPipe(line) && isManageMarkdownTableDivider(divider);
}

function isManageMarkdownTableDivider(line: string): boolean {
  return /^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/u.test(line);
}

function lineHasUnescapedPipe(line: string): boolean {
  return /(^|[^\\])\|/u.test(line);
}

function expandManageMarkdownIndent(value: string): string {
  return value.replace(/\t/gu, "    ");
}

function computeManageOrderedListIndices(blocks: ManageMarkdownBlock[]): Map<string, number> {
  const indices = new Map<string, number>();
  const counters = new Map<number, number>();
  for (const block of blocks) {
    if (block.type !== "list-item") {
      counters.clear();
      continue;
    }
    const level = block.level ?? 0;
    for (const counterLevel of Array.from(counters.keys())) {
      if (counterLevel > level) {
        counters.delete(counterLevel);
      }
    }
    if (!block.ordered) {
      counters.delete(level);
      continue;
    }
    const nextIndex = counters.has(level) ? (counters.get(level) ?? 0) + 1 : block.orderedStart ?? 1;
    counters.set(level, nextIndex);
    indices.set(block.id, nextIndex);
  }
  return indices;
}

function parseManageMarkdownTableContent(content: string): { headers: string[]; rows: string[][] } {
  const lines = content.split("\n").filter((line) => line.trim());
  const parseRow = (line: string): string[] =>
    line
      .replace(/^\s*\|/u, "")
      .replace(/\|\s*$/u, "")
      .split(/(?<!\\)\|/u)
      .map((cell) => cell.trim().replace(/\\\|/gu, "|"));
  const headers = lines[0] ? parseRow(lines[0]) : [];
  const rows = lines.slice(2).map(parseRow);
  return { headers, rows };
}

function renderManageInlineMarkdown(text: string, annotations: ManageAnnotation[]): ReactNode {
  return renderManageAnnotatedInline(
    text,
    annotations.filter((annotation) => annotation.scope === "selection" && Boolean(annotation.quote)),
  );
}

function renderManageAnnotatedInline(text: string, annotations: ManageAnnotation[]): ReactNode {
  const annotation = annotations.find((candidate) => text.includes(candidate.quote));
  if (!annotation) {
    return renderManageInlineTokens(text);
  }
  const index = text.indexOf(annotation.quote);
  const before = text.slice(0, index);
  const match = text.slice(index, index + annotation.quote.length);
  const after = text.slice(index + annotation.quote.length);
  const remaining = annotations.filter((candidate) => candidate.id !== annotation.id);
  return (
    <>
      {renderManageAnnotatedInline(before, remaining)}
      <mark
        className={`annotation-highlight manage-annotation-highlight ${annotation.type === "redline" ? "deletion" : "comment"}`}
        data-label-id={annotation.labelId}
        data-type={annotation.type}
        style={{ "--manage-annotation-color": manageAnnotationColor(annotation) } as CSSProperties}
      >
        {renderManageInlineTokens(match)}
      </mark>
      {renderManageAnnotatedInline(after, remaining)}
    </>
  );
}

function renderManageInlineTokens(text: string): ReactNode {
  const nodes: ReactNode[] = [];
  let index = 0;
  while (index < text.length) {
    if (text.startsWith("`", index)) {
      const end = text.indexOf("`", index + 1);
      if (end > index) {
        nodes.push(
          <code className="manage-md-inline-code" key={`code-${index}`}>
            {text.slice(index + 1, end)}
          </code>,
        );
        index = end + 1;
        continue;
      }
    }
    if (text.startsWith("![", index)) {
      const image = parseManageMarkdownImageToken(text, index);
      if (image) {
        nodes.push(image.node);
        index = image.nextIndex;
        continue;
      }
    }
    if (text.startsWith("[", index)) {
      const link = parseManageMarkdownLinkToken(text, index);
      if (link) {
        nodes.push(link.node);
        index = link.nextIndex;
        continue;
      }
    }
    const strongMarker = text.startsWith("**", index) ? "**" : text.startsWith("__", index) ? "__" : "";
    if (strongMarker) {
      const end = text.indexOf(strongMarker, index + 2);
      if (end > index + 2) {
        nodes.push(<strong key={`strong-${index}`}>{renderManageInlineTokens(text.slice(index + 2, end))}</strong>);
        index = end + 2;
        continue;
      }
    }
    if (text.startsWith("~~", index)) {
      const end = text.indexOf("~~", index + 2);
      if (end > index + 2) {
        nodes.push(<del key={`del-${index}`}>{renderManageInlineTokens(text.slice(index + 2, end))}</del>);
        index = end + 2;
        continue;
      }
    }
    const emphasisMarker = text[index] === "*" || text[index] === "_" ? text[index] : "";
    if (emphasisMarker && !text.startsWith(`${emphasisMarker}${emphasisMarker}`, index)) {
      const end = text.indexOf(emphasisMarker, index + 1);
      if (end > index + 1) {
        nodes.push(<em key={`em-${index}`}>{renderManageInlineTokens(text.slice(index + 1, end))}</em>);
        index = end + 1;
        continue;
      }
    }

    const nextSpecial = findNextManageInlineSpecial(text, index + 1);
    nodes.push(...renderManagePlainInlineText(text.slice(index, nextSpecial), `text-${index}`));
    index = nextSpecial;
  }
  return nodes;
}

function parseManageMarkdownLinkToken(text: string, index: number): { nextIndex: number; node: ReactNode } | undefined {
  const labelEnd = text.indexOf("]", index + 1);
  if (labelEnd <= index + 1 || text[labelEnd + 1] !== "(") {
    return undefined;
  }
  const hrefEnd = text.indexOf(")", labelEnd + 2);
  if (hrefEnd <= labelEnd + 2) {
    return undefined;
  }
  const href = sanitizeManageHref(text.slice(labelEnd + 2, hrefEnd).trim());
  const label = text.slice(index + 1, labelEnd);
  if (!href) {
    return {
      nextIndex: hrefEnd + 1,
      node: <span key={`link-${index}`}>{renderManageInlineTokens(label)}</span>,
    };
  }
  return {
    nextIndex: hrefEnd + 1,
    node: (
      <a href={href} key={`link-${index}`} rel="noreferrer" target={href.startsWith("#") ? undefined : "_blank"}>
        {renderManageInlineTokens(label)}
      </a>
    ),
  };
}

function parseManageMarkdownImageToken(text: string, index: number): { nextIndex: number; node: ReactNode } | undefined {
  const altEnd = text.indexOf("]", index + 2);
  if (altEnd <= index + 2 || text[altEnd + 1] !== "(") {
    return undefined;
  }
  const srcEnd = text.indexOf(")", altEnd + 2);
  if (srcEnd <= altEnd + 2) {
    return undefined;
  }
  const alt = text.slice(index + 2, altEnd);
  const src = sanitizeManageImageSrc(text.slice(altEnd + 2, srcEnd).trim());
  return {
    nextIndex: srcEnd + 1,
    node: src ? <img alt={alt} className="manage-md-inline-image" key={`image-${index}`} src={src} /> : alt,
  };
}

function renderManagePlainInlineText(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const urlPattern = /(https?:\/\/[^\s<)]+)/giu;
  let lastIndex = 0;
  for (const match of text.matchAll(urlPattern)) {
    const url = match[0];
    const index = match.index ?? 0;
    if (index > lastIndex) {
      nodes.push(text.slice(lastIndex, index));
    }
    nodes.push(
      <a href={url} key={`${keyPrefix}-url-${index}`} rel="noreferrer" target="_blank">
        {url}
      </a>,
    );
    lastIndex = index + url.length;
  }
  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

function findNextManageInlineSpecial(text: string, start: number): number {
  const candidates = ["`", "![", "[", "**", "__", "~~", "*", "_"]
    .map((marker) => text.indexOf(marker, start))
    .filter((candidate) => candidate >= 0);
  return candidates.length > 0 ? Math.min(...candidates) : text.length;
}

function sanitizeManageHref(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed || /^(?:javascript|data|vbscript|file):/iu.test(trimmed)) {
    return undefined;
  }
  return trimmed;
}

function sanitizeManageImageSrc(value: string): string | undefined {
  const trimmed = value.trim();
  if (!trimmed || /^(?:javascript|vbscript|file):/iu.test(trimmed)) {
    return undefined;
  }
  if (/^data:/iu.test(trimmed) && !/^data:image\//iu.test(trimmed)) {
    return undefined;
  }
  return trimmed;
}

function sanitizeManageBlockHtml(html: string): string {
  const template = document.createElement("template");
  template.innerHTML = html;
  template.content.querySelectorAll("script, style, iframe, object, embed, link, meta").forEach((element) => {
    element.remove();
  });
  template.content.querySelectorAll("*").forEach((element) => {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLocaleLowerCase();
      if (name.startsWith("on") || name === "style") {
        element.removeAttribute(attribute.name);
        continue;
      }
      if ((name === "href" || name === "src") && !sanitizeManageHref(attribute.value)) {
        element.removeAttribute(attribute.name);
      }
    }
    if (element instanceof HTMLAnchorElement && element.href && !element.href.startsWith("#")) {
      element.target = "_blank";
      element.rel = "noreferrer";
    }
  });
  return template.innerHTML;
}

function buildManageHtmlDocument(
  html: string,
  options: { injectAgentation?: boolean; resourceBaseUrl?: string } = {},
): string {
  /*
   * CDXC:ManageHtmlRendering 2026-07-01-18:12:
   * Docs HTML files should behave like real interactive browser documents. Parse only to append Ghostex-owned viewer chrome and the optional Agentation bootstrap; do not remove authored scripts, inline handlers, JavaScript URLs, frames, form targets, srcdoc content, or base tags.
   */
  const documentValue = new DOMParser().parseFromString(html, "text/html");
  injectManageHtmlResourceBase(documentValue, options.resourceBaseUrl);
  injectManageHtmlViewerChromeStyles(documentValue);
  if (options.injectAgentation) {
    injectManageAgentationScript(documentValue);
  }
  return `${serializeManageDocumentType(documentValue)}\n${documentValue.documentElement.outerHTML}`;
}

function manageHtmlResourceBaseUrl(documentPath: string): string | undefined {
  const configuredBaseUrl = (window as ManageWebKitWindow).ghostexGpui?.manageDocsResourceBaseUrl;
  if (!configuredBaseUrl) {
    return undefined;
  }
  let baseUrl: URL;
  try {
    baseUrl = new URL(configuredBaseUrl);
  } catch {
    return undefined;
  }
  if (
    baseUrl.protocol !== "https:" ||
    baseUrl.hostname !== "ghostex-docs.invalid" ||
    baseUrl.pathname !== "/"
  ) {
    return undefined;
  }
  const components = documentPath.split("/");
  if (
    components.length < 2 ||
    components.some((component) => !component || component === "." || component === "..")
  ) {
    return undefined;
  }
  const parentPath = components.slice(0, -1).map(encodeURIComponent).join("/");
  return new URL(`${parentPath}/`, baseUrl).toString();
}

function decodeManageHtmlFragment(href: string): string | undefined {
  try {
    return decodeURIComponent(href.slice(1));
  } catch {
    return undefined;
  }
}

function manageHtmlLinkedDocumentPath(
  href: string,
  resourceBaseUrl: string | undefined,
): string | undefined {
  if (!resourceBaseUrl) {
    return undefined;
  }
  let baseUrl: URL;
  let linkedUrl: URL;
  try {
    baseUrl = new URL(resourceBaseUrl);
    linkedUrl = new URL(href, baseUrl);
  } catch {
    return undefined;
  }
  if (linkedUrl.origin !== baseUrl.origin) {
    return undefined;
  }
  const encodedComponents = linkedUrl.pathname.split("/").filter(Boolean);
  if (encodedComponents.length === 0) {
    return undefined;
  }
  let components: string[];
  try {
    components = encodedComponents.map(decodeURIComponent);
  } catch {
    return undefined;
  }
  if (
    components.some(
      (component) =>
        !component || component === "." || component === ".." || component.includes("\\"),
    )
  ) {
    return undefined;
  }
  const path = components.join("/");
  return isHtmlPath(path) ? path : undefined;
}

function injectManageHtmlResourceBase(
  documentValue: Document,
  resourceBaseUrl: string | undefined,
): void {
  if (!resourceBaseUrl) {
    return;
  }
  const authoredBase = documentValue.querySelector("base[href]");
  if (authoredBase) {
    const href = authoredBase.getAttribute("href");
    if (href) {
      try {
        authoredBase.setAttribute("href", new URL(href, resourceBaseUrl).toString());
      } catch {
        // Leave malformed authored base URLs unchanged so the browser reports them normally.
      }
    }
    return;
  }
  const base = documentValue.createElement("base");
  base.setAttribute("data-ghostex-manage-resource-base", "true");
  base.href = resourceBaseUrl;
  documentValue.head.prepend(base);
}

function injectManageHtmlViewerChromeStyles(documentValue: Document): void {
  /*
   * CDXC:ManageHtmlRendering 2026-06-30-04:57:
   * The rendered artifact document owns its page CSS, but Docs owns the embedded scrollbar chrome. Append the style after author CSS so the iframe never shows wide default scrollbars or an opaque track/corner behind them.
   *
   * CDXC:ManageHtmlRendering 2026-06-30-11:58:
   * Use document tagging plus WebKit scrollbar pseudo-elements for exact 4px embedded scrollbars. Standards `scrollbar-width: thin` is intentionally avoided because it produced a wider rendered scrollbar than the Docs requirement.
   */
  documentValue.documentElement.setAttribute("data-ghostex-manage-html-viewer", "true");
  const style = documentValue.createElement("style");
  style.setAttribute("data-ghostex-manage-html-chrome", "true");
  style.textContent = `
html[data-ghostex-manage-html-viewer],
html[data-ghostex-manage-html-viewer] body,
html[data-ghostex-manage-html-viewer] * {
  scrollbar-color: auto !important;
  scrollbar-width: auto !important;
}

html[data-ghostex-manage-html-viewer]::-webkit-scrollbar,
html[data-ghostex-manage-html-viewer] body::-webkit-scrollbar,
html[data-ghostex-manage-html-viewer] *::-webkit-scrollbar {
  background: transparent !important;
  height: 4px !important;
  width: 4px !important;
}

html[data-ghostex-manage-html-viewer]::-webkit-scrollbar-track,
html[data-ghostex-manage-html-viewer] body::-webkit-scrollbar-track,
html[data-ghostex-manage-html-viewer] *::-webkit-scrollbar-track,
html[data-ghostex-manage-html-viewer]::-webkit-scrollbar-track-piece,
html[data-ghostex-manage-html-viewer] body::-webkit-scrollbar-track-piece,
html[data-ghostex-manage-html-viewer] *::-webkit-scrollbar-track-piece {
  background: transparent !important;
  border: 0 !important;
  box-shadow: none !important;
}

html[data-ghostex-manage-html-viewer]::-webkit-scrollbar-thumb,
html[data-ghostex-manage-html-viewer] body::-webkit-scrollbar-thumb,
html[data-ghostex-manage-html-viewer] *::-webkit-scrollbar-thumb {
  background-color: #3e444c !important;
  border: 0 !important;
  border-radius: 999px !important;
}

html[data-ghostex-manage-html-viewer]::-webkit-scrollbar-button,
html[data-ghostex-manage-html-viewer] body::-webkit-scrollbar-button,
html[data-ghostex-manage-html-viewer] *::-webkit-scrollbar-button,
html[data-ghostex-manage-html-viewer]::-webkit-scrollbar-corner,
html[data-ghostex-manage-html-viewer] body::-webkit-scrollbar-corner,
html[data-ghostex-manage-html-viewer] *::-webkit-scrollbar-corner {
  background: transparent !important;
  border: 0 !important;
}
`.trim();
  (documentValue.head || documentValue.documentElement).appendChild(style);
}

function injectManageAgentationScript(documentValue: Document): void {
  const script = documentValue.createElement("script");
  script.type = "module";
  script.textContent = buildManageAgentationBootstrapScript();
  (documentValue.body || documentValue.documentElement).appendChild(script);
}

function buildManageAgentationBootstrapScript(): string {
  return `
const rootId = "ghostex-agentation-root";
const directionStyleId = "ghostex-agentation-direction-style";
document.getElementById(rootId)?.remove();
document.getElementById(directionStyleId)?.remove();
// Agentation portals its visible UI into document.body, outside rootEl. Give
// that portal an explicit writing-mode boundary so authored RTL page styles
// cannot reverse Agentation's own controls.
const directionStyle = document.createElement("style");
directionStyle.id = directionStyleId;
directionStyle.textContent = "[data-agentation-root][data-agentation-theme] { direction: ltr !important; text-align: left !important; }";
(document.head || document.documentElement).appendChild(directionStyle);
const rootEl = document.createElement("div");
rootEl.id = rootId;
rootEl.setAttribute("data-agentation-html-root", "true");
rootEl.setAttribute("data-agentation-root", "true");
(document.body || document.documentElement).appendChild(rootEl);
Promise.all([
  import(${JSON.stringify(MANAGE_AGENTATION_REACT_URL)}),
  import(${JSON.stringify(MANAGE_AGENTATION_REACT_DOM_CLIENT_URL)}),
  import(${JSON.stringify(MANAGE_AGENTATION_PACKAGE_URL)})
]).then(([reactModule, reactDomClientModule, agentationModule]) => {
  const React = reactModule.default ?? reactModule;
  const ReactDOMClient = reactDomClientModule;
  const Agentation = agentationModule.Agentation;
  if (!React?.createElement || !ReactDOMClient?.createRoot || !Agentation) {
    throw new Error("Agentation modules did not expose the expected React mounting API.");
  }
  const root = ReactDOMClient.createRoot(rootEl);
  globalThis.__GHOSTEX_AGENTATION__ = { container: rootEl, root };
  root.render(React.createElement(Agentation));
}).catch((error) => {
  console.warn("[Ghostex Docs Agentation] page injection failed", {
    message: error instanceof Error ? error.message : String(error)
  });
  rootEl.remove();
  directionStyle.remove();
});
`.trim();
}

function serializeManageDocumentType(documentValue: Document): string {
  const doctype = documentValue.doctype;
  if (!doctype) {
    return "<!doctype html>";
  }
  const publicId = doctype.publicId ? ` PUBLIC "${doctype.publicId}"` : "";
  const systemId = doctype.systemId ? `${publicId ? "" : " SYSTEM"} "${doctype.systemId}"` : "";
  return `<!doctype ${doctype.name}${publicId}${systemId}>`;
}

function normalizeAttachmentName(name: string): string {
  const trimmed = name.trim().replace(/\s+/g, "-");
  return trimmed ? trimmed.slice(0, 80) : "image";
}

function parseManageAnnotationStore(content: string): Record<string, ManageAnnotation[]> {
  if (!content.trim()) {
    return {};
  }
  try {
    const value = JSON.parse(content) as unknown;
    if (!isRecord(value)) {
      return {};
    }
    const annotationsValue = value.annotationsByPath;
    if (!isRecord(annotationsValue)) {
      return {};
    }
    const normalized: Record<string, ManageAnnotation[]> = {};
    for (const [path, annotations] of Object.entries(annotationsValue)) {
      const normalizedPath = normalizeStoredAnnotationPath(path);
      if (!normalizedPath || !Array.isArray(annotations)) {
        continue;
      }
      const normalizedAnnotations = annotations
        .map((annotation) => normalizeStoredAnnotation(annotation))
        .filter((annotation): annotation is ManageAnnotation => Boolean(annotation));
      if (normalizedAnnotations.length > 0) {
        normalized[normalizedPath] = normalizedAnnotations;
      }
    }
    return normalized;
  } catch {
    return {};
  }
}

function serializeManageAnnotationStore(annotationsByPath: Record<string, ManageAnnotation[]>): string {
  const store: ManageAnnotationStore = {
    annotationsByPath,
    updatedAt: new Date().toISOString(),
    version: MANAGE_ANNOTATION_SCHEMA_VERSION,
  };
  return `${JSON.stringify(store, null, 2)}\n`;
}

function stableManageAnnotationStoreKey(annotationsByPath: Record<string, ManageAnnotation[]>): string {
  return JSON.stringify(annotationsByPath);
}

function normalizeStoredAnnotationPath(path: string): string {
  const trimmed = path.trim();
  if (!trimmed || trimmed.startsWith("/") || trimmed.includes("\0")) {
    return "";
  }
  const components = trimmed.split("/").filter(Boolean);
  if (components.includes(".") || components.includes("..")) {
    return "";
  }
  return components.join("/");
}

function normalizeStoredAnnotation(value: unknown): ManageAnnotation | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const type = value.type === "redline" ? "redline" : value.type === "comment" ? "comment" : undefined;
  if (!type) {
    return undefined;
  }
  const quote = typeof value.quote === "string" ? normalizeAnnotationQuote(value.quote) : "";
  const note = typeof value.note === "string" ? value.note.slice(0, 4_000) : "";
  const attachments = Array.isArray(value.attachments)
    ? value.attachments
        .map((attachment) => normalizeStoredAttachment(attachment))
        .filter((attachment): attachment is ManageAnnotationImage => Boolean(attachment))
        .slice(0, MANAGE_ANNOTATION_MAX_IMAGES)
    : [];
  if (type === "redline" && !quote) {
    return undefined;
  }
  if (type === "comment" && !quote && !note.trim() && attachments.length === 0) {
    return undefined;
  }
  const labelId = normalizeQuickLabelId(value.labelId);
  return {
    attachments,
    createdAt: typeof value.createdAt === "string" ? value.createdAt : new Date().toISOString(),
    id: typeof value.id === "string" && value.id.trim() ? value.id : `manage-annotation-${Date.now()}`,
    ...(labelId ? { labelId } : {}),
    note,
    quote,
    scope: quote ? "selection" : "global",
    type,
  };
}

function normalizeStoredAttachment(value: unknown): ManageAnnotationImage | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const dataUrl = typeof value.dataUrl === "string" ? value.dataUrl : "";
  const mimeType = typeof value.mimeType === "string" ? value.mimeType : "";
  const name = typeof value.name === "string" ? normalizeAttachmentName(value.name) : "image";
  const size = typeof value.size === "number" && Number.isFinite(value.size) ? Math.max(0, value.size) : 0;
  if (!dataUrl.startsWith("data:image/") || !mimeType.startsWith("image/") || size > MANAGE_ANNOTATION_IMAGE_MAX_BYTES) {
    return undefined;
  }
  return {
    dataUrl,
    id: typeof value.id === "string" && value.id.trim() ? value.id : `manage-annotation-image-${Date.now()}`,
    mimeType,
    name,
    size,
  };
}

function normalizeQuickLabelId(value: unknown): ManageQuickLabelId | undefined {
  return MANAGE_QUICK_LABELS.some((label) => label.id === value) ? (value as ManageQuickLabelId) : undefined;
}

function formatManageAnnotationsAsMarkdown(path: string, annotations: ManageAnnotation[]): string {
  if (annotations.length === 0) {
    return `# Docs Markdown Feedback\n\nFile: \`${path}\`\n\nNo annotations.\n`;
  }
  const lines = ["# Docs Markdown Feedback", "", `File: \`${path}\``, ""];
  const redlines = annotations.filter((annotation) => annotation.type === "redline");
  const comments = annotations.filter((annotation) => annotation.type === "comment");
  if (redlines.length > 0) {
    lines.push("## Redlines", "");
    for (const annotation of redlines) {
      lines.push(`- Delete: ${formatMarkdownQuote(annotation.quote)}`);
      appendAnnotationDetails(lines, annotation);
    }
    lines.push("");
  }
  if (comments.length > 0) {
    lines.push("## Comments", "");
    for (const annotation of comments) {
      const prefix = annotation.scope === "global" ? "Global" : `On ${formatMarkdownQuote(annotation.quote)}`;
      const body = annotation.note.trim() || (annotation.labelId ? quickLabelText(annotation.labelId) : "(attachment only)");
      lines.push(`- ${prefix}: ${body}`);
      appendAnnotationDetails(lines, annotation);
    }
  }
  return `${lines.join("\n").trimEnd()}\n`;
}

function appendAnnotationDetails(lines: string[], annotation: ManageAnnotation): void {
  if (annotation.labelId) {
    lines.push(`  - Label: ${quickLabelText(annotation.labelId)}`);
  }
  if (annotation.type === "redline" && annotation.note.trim()) {
    lines.push(`  - Note: ${annotation.note.trim()}`);
  }
  if (annotation.attachments.length > 0) {
    lines.push("  - Attachments:");
    for (const attachment of annotation.attachments) {
      lines.push(`    - ${attachment.name}: ${attachment.dataUrl}`);
    }
  }
}

function formatMarkdownQuote(text: string): string {
  return `"${text.replace(/"/gu, '\\"')}"`;
}

async function writeTextToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.cssText = "position:fixed;left:-9999px;top:-9999px";
    document.body.append(textarea);
    textarea.select();
    const didCopy = document.execCommand("copy");
    textarea.remove();
    if (!didCopy) {
      throw new Error("Clipboard copy failed.");
    }
  }
}

function isEditableEventTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }
  if (target.matches("input, textarea, select, [contenteditable='true']")) {
    return true;
  }
  return Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
}

function parseExcalidrawFile(content: string): { data: ExcalidrawFileData; ok: true } | { error: string; ok: false } {
  const trimmed = content.trim();
  if (!trimmed) {
    return {
      data: createEmptyExcalidrawFile(),
      ok: true,
    };
  }
  try {
    const value = JSON.parse(trimmed) as unknown;
    if (!isRecord(value)) {
      return { error: "Drawing JSON must be an object.", ok: false };
    }
    if (value.type !== "excalidraw" && !Array.isArray(value.elements)) {
      return { error: "Drawing JSON is missing scene elements.", ok: false };
    }
    return {
      data: {
        appState: isRecord(value.appState) ? value.appState : {},
        elements: Array.isArray(value.elements) ? (value.elements as ExcalidrawElement[]) : [],
        files: isRecord(value.files) ? (value.files as BinaryFiles) : {},
        source: typeof value.source === "string" ? value.source : "https://excalidraw.com",
        type: "excalidraw",
        version: typeof value.version === "number" ? value.version : 2,
      },
      ok: true,
    };
  } catch (parseError) {
    return {
      error: parseError instanceof Error ? parseError.message : "Drawing JSON is invalid.",
      ok: false,
    };
  }
}

function createEmptyExcalidrawFile(): ExcalidrawFileData {
  return {
    appState: {
      theme: MANAGE_EXCALIDRAW_CANVAS_THEME,
      viewBackgroundColor: MANAGE_EXCALIDRAW_CANVAS_BACKGROUND,
    },
    elements: [],
    files: {},
    source: "https://excalidraw.com",
    type: "excalidraw",
    version: 2,
  };
}

function serializeExcalidrawFile(
  previousData: ExcalidrawFileData,
  elements: readonly ExcalidrawElement[],
  appState: AppState,
  files: BinaryFiles,
): string {
  const savedAppState: Record<string, unknown> = {
    ...(previousData.appState ?? {}),
    scrollX: appState.scrollX,
    scrollY: appState.scrollY,
    theme: appState.theme,
    viewBackgroundColor: appState.viewBackgroundColor,
    zoom: normalizeExcalidrawZoom(appState.zoom),
  };
  delete savedAppState.collaborators;
  return JSON.stringify(
    {
      appState: savedAppState,
      elements,
      files,
      source: previousData.source ?? "https://excalidraw.com",
      type: "excalidraw",
      version: previousData.version ?? 2,
    },
    null,
    2,
  );
}

function createExcalidrawSceneSignature(
  elements: readonly ExcalidrawElement[],
  appState: AppState,
  files: BinaryFiles,
): string {
  return JSON.stringify({
    appState: {
      scrollX: appState.scrollX,
      scrollY: appState.scrollY,
      viewBackgroundColor: appState.viewBackgroundColor,
      zoom: normalizeExcalidrawZoom(appState.zoom),
    },
    elements: elements.map((element) => ({
      id: element.id,
      isDeleted: element.isDeleted,
      version: element.version,
      versionNonce: element.versionNonce,
    })),
    files: Object.keys(files).sort(),
  });
}

function normalizeExcalidrawZoom(zoom: AppState["zoom"]): number {
  if (typeof zoom === "object" && zoom !== null && "value" in zoom && typeof zoom.value === "number") {
    return zoom.value;
  }
  return typeof zoom === "number" ? zoom : 1;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

const styleElement = document.createElement("style");
styleElement.textContent = `
  :root {
    color-scheme: dark;
    --manage-bg: #0e0e0e;
    --manage-panel: #0e0e0e;
    --manage-panel-strong: #181818;
    --manage-panel-raised: #202020;
    --manage-border: color-mix(in srgb, #ffffff 11%, transparent);
    --manage-border-strong: rgba(255, 255, 255, 0.12);
    --manage-text: #c8cdd5;
    --manage-muted: #a6adb6;
    --manage-subtle: #747b85;
    --manage-accent: #95d7f6;
    --manage-accent-muted: rgba(255, 255, 255, 0.055);
    --manage-row-surface: #202020;
    --manage-green: #86efac;
    --manage-red: #fda4af;
    --manage-yellow: #fde68a;
    background: var(--manage-bg);
  }

  * {
    box-sizing: border-box;
  }

  html,
  body,
  #root {
    background: var(--manage-bg);
    height: 100%;
    margin: 0;
    overflow: hidden;
    width: 100%;
  }

  /*
   * CDXC:DocsRootLayout 2026-08-18:
   * Manage pulls in the shared sidebar theme for its tooltip/app tokens, and
   * that stylesheet also carries the sidebar app's own shell layout
   * ('#root { display: grid; grid-template-rows: auto minmax(0, 1fr) }', a
   * titlebar row plus a content row). Manage has no titlebar row: it renders a
   * single '.manage-shell' child that must own the whole document. Under the
   * sidebar grid that child lands in the content-sized 'auto' row, so Docs
   * stopped at its content height and left the rest of the pane empty. Manage
   * declares its own root layout so the document root stays a plain full-height
   * block box regardless of which shared theme sheets are loaded.
   */
  #root {
    display: block;
  }

  body {
    color: var(--manage-text);
    font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  .manage-shell {
    background: var(--manage-bg);
    display: grid;
    grid-template-columns: var(--manage-sidebar-width, 292px) 5px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    position: relative;
    width: 100%;
  }

  .manage-shell[data-sidebar-side="right"] {
    grid-template-columns: minmax(0, 1fr) 5px var(--manage-sidebar-width, 292px);
  }

  .manage-shell[data-sidebar-hidden="true"] {
    grid-template-columns: minmax(0, 1fr);
  }

  .manage-shell[data-sidebar-hidden="true"] .manage-preview {
    grid-column: 1;
    grid-row: 1;
  }

  /*
   * CDXC:DocsSidebar 2026-06-30-13:45:
   * When the Manage page is below 690px wide, Docs should use a floating sidebar above the full-width preview instead of squeezing the preview into a second grid column. Keep the sidebar side preference for which edge the floating panel opens from, and let outside clicks hide it.
   *
   * CDXC:DocsSidebar 2026-06-30-21:52:
   * The floating Docs sidebar must paint above the copied Meo Markdown toolbar, whose z-index is 500, so the file tree covers the entire editor chrome instead of starting visually below the toolbar. Cast the floating shadow from the sidebar edge that overlaps the Markdown editor so the panel reads as a raised sheet.
   */
  .manage-shell[data-sidebar-floating="true"],
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] {
    grid-template-columns: minmax(0, 1fr);
  }

  .manage-shell[data-sidebar-floating="true"] .manage-preview {
    grid-column: 1;
    grid-row: 1;
  }

  .manage-sidebar {
    background: var(--manage-panel);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    grid-column: 1;
    grid-row: 1;
    min-height: 0;
    min-width: 0;
    padding: 0 0 7px;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar {
    grid-column: 3;
    grid-row: 1;
  }

  .manage-shell[data-sidebar-floating="true"] .manage-sidebar {
    border-right: 1px solid var(--manage-border);
    bottom: 0;
    box-shadow:
      16px 0 36px rgba(0, 0, 0, 0.42),
      4px 0 14px rgba(0, 0, 0, 0.26);
    grid-column: 1;
    grid-row: 1;
    left: 0;
    max-width: calc(100% - 34px);
    position: absolute;
    top: 0;
    width: min(var(--manage-sidebar-width, 292px), calc(100% - 34px));
    z-index: 650;
  }

  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-sidebar {
    border-left: 1px solid var(--manage-border);
    border-right: 0;
    box-shadow:
      -16px 0 36px rgba(0, 0, 0, 0.42),
      -4px 0 14px rgba(0, 0, 0, 0.26);
    left: auto;
    right: 0;
  }

  .manage-sidebar-resizer {
    background: var(--manage-bg);
    cursor: ew-resize;
    grid-column: 2;
    grid-row: 1;
    min-width: 5px;
    outline: none;
    position: relative;
    touch-action: none;
  }

  .manage-sidebar-resizer::before {
    background: #212121;
    content: "";
    bottom: 0;
    position: absolute;
    right: 0;
    top: 0;
    width: 1px;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-resizer::before {
    left: 0;
    right: auto;
  }

  .manage-sidebar-resizer::after {
    background: #ffffff;
    bottom: 0;
    content: "";
    opacity: 0;
    position: absolute;
    right: 0;
    top: 0;
    transition: opacity 180ms ease-out 50ms;
    width: 3px;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-resizer::after {
    left: 0;
    right: auto;
  }

  .manage-sidebar-resizer:hover::after,
  .manage-sidebar-resizer:focus-visible::after {
    opacity: 1;
  }

  .manage-shell[data-sidebar-floating="true"] .manage-sidebar-resizer {
    display: none;
  }

  .manage-preview {
    grid-column: 3;
    grid-row: 1;
  }

  .manage-shell[data-sidebar-side="right"] .manage-preview {
    grid-column: 1;
    grid-row: 1;
  }

  .manage-sidebar-header {
    align-items: center;
    border-bottom: 1px solid var(--manage-border);
    box-sizing: border-box;
    display: flex;
    gap: 8px;
    height: 35px;
    justify-content: flex-end;
    max-height: 35px;
    min-height: 35px;
    overflow: visible;
    padding: 0;
  }

  .manage-sidebar-header[data-root-drop-target="true"] {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
  }

  .manage-sidebar-actions {
    align-items: center;
    align-self: stretch;
    display: inline-flex;
    flex: 0 0 auto;
    gap: 0;
    height: 100%;
    position: relative;
  }

  .manage-icon-button {
    align-items: center;
    background: transparent;
    border: 0;
    color: color-mix(in srgb, var(--manage-text) 88%, var(--manage-subtle) 12%);
    display: inline-flex;
    height: 26px;
    justify-content: center;
    padding: 0;
    width: 26px;
  }

  .manage-icon-button:hover,
  .manage-icon-button:focus-visible {
    background: color-mix(in srgb, var(--manage-text) 10%, transparent);
    color: var(--manage-text);
    outline: none;
  }

  .manage-icon-button:disabled {
    color: var(--manage-subtle);
  }

  .manage-sidebar-header .manage-icon-button,
  .manage-sidebar-restore-button {
    background: transparent;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.84);
    height: 35px;
    max-height: 35px;
    min-height: 35px;
    padding: 0;
    width: 38px;
  }

  .manage-sidebar-header .manage-icon-button {
    border-left: 1px solid #252525;
    width: 42px;
  }

  /*
   * CDXC:DocsSidebar 2026-06-30-04:55:
   * Docs sidebar header actions should now use the same 42px width, including the rightmost action, so the top control strip stays evenly spaced.
   */
  .manage-sidebar-header .manage-icon-button:last-child {
    width: 42px;
  }

  .manage-sidebar-restore-button {
    border-right: 1px solid #252525;
  }

  .manage-sidebar-header .manage-icon-button:not(:disabled):hover,
  .manage-sidebar-header .manage-icon-button:not(:disabled):focus-visible,
  .manage-sidebar-header .manage-icon-button[aria-expanded="true"],
  .manage-sidebar-restore-button:not(:disabled):hover,
  .manage-sidebar-restore-button:not(:disabled):focus-visible {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.96);
    outline: none;
  }

  .manage-sidebar-header .manage-icon-button:disabled {
    background: transparent;
    color: rgba(255, 255, 255, 0.34);
    cursor: default;
  }

  .manage-sidebar-header .manage-icon-button svg,
  .manage-sidebar-restore-button svg {
    height: 16px;
    width: 16px;
  }

  .manage-sidebar-tree-toggle svg {
    height: 14px;
    transform: rotate(90deg);
    width: 14px;
  }

  .manage-sidebar-menu {
    backdrop-filter: blur(18px);
    background: #0e0e0e;
    border: 1px solid #595959;
    border-radius: 4px;
    box-shadow:
      0 18px 42px rgba(0, 0, 0, 0.38),
      0 4px 12px rgba(0, 0, 0, 0.28),
      inset 0 1px 0 rgba(255, 255, 255, 0.08);
    display: grid;
    gap: 3px;
    min-width: 190px;
    padding: 6px;
    position: absolute;
    right: 6px;
    top: calc(100% + 7px);
    z-index: 30;
  }

  .manage-create-menu {
    min-width: 182px;
  }

  .manage-sidebar-menu-item {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 3px;
    color: rgba(244, 244, 245, 0.88);
    display: flex;
    font-size: 12.5px;
    font-weight: 620;
    gap: 9px;
    line-height: 16px;
    min-height: 34px;
    padding: 8px 10px 8px 9px;
    position: relative;
    text-align: left;
    white-space: nowrap;
    width: 100%;
    z-index: 1;
  }

  .manage-sidebar-menu-item svg {
    color: rgba(244, 244, 245, 0.72);
    flex: 0 0 auto;
    height: 15px;
    width: 15px;
  }

  .manage-sidebar-menu-item:hover,
  .manage-sidebar-menu-item:focus-visible {
    background: rgba(255, 255, 255, 0.105);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.045);
    color: rgba(250, 250, 250, 0.98);
    outline: none;
  }

  .manage-sidebar-menu-item:hover svg,
  .manage-sidebar-menu-item:focus-visible svg {
    color: rgba(250, 250, 250, 0.92);
  }

  .manage-sidebar-menu-item:disabled {
    color: var(--manage-subtle);
    cursor: not-allowed;
  }

  .manage-sidebar-menu-item:disabled svg {
    color: color-mix(in srgb, var(--manage-subtle) 72%, transparent);
  }

  .manage-sidebar-menu-item:disabled:hover {
    background: transparent;
    box-shadow: none;
  }

  .manage-sidebar-restore-button {
    left: 0;
    position: absolute;
    top: 0;
    z-index: 5;
  }

  .manage-shell[data-sidebar-side="right"] .manage-sidebar-restore-button {
    border-left: 1px solid #252525;
    border-right: 0;
    left: auto;
    right: 0;
  }

  .manage-shell[data-sidebar-hidden="true"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"] .manage-preview-header {
    padding-left: 51px;
  }

  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-header {
    padding-left: 16px;
    padding-right: 51px;
  }

  /*
   * CDXC:DocsAnnotationToolbar 2026-06-30-22:58:
   * Markdown Docs can collapse header action labels at narrow widths, making
   * the annotations/comments button the last visible header action before the
   * right-side restore control. Reserve only the restore button's real width so
   * the comments button does not leave an empty gutter to its right.
   *
   * CDXC:DocsAnnotationToolbar 2026-06-30-23:52:
   * Floating sidebars hide and show above the same preview grid, so header
   * action geometry must not depend on whether the floating sidebar is currently
   * visible. Apply the same right-edge reservation in floating mode to prevent
   * the Markdown toolbar buttons from shifting during hide/show.
   */
  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-preview-header,
  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="html"] .manage-preview-header,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="html"] .manage-preview-header {
    padding-right: 38px;
  }

  .manage-search {
    align-items: center;
    background: transparent;
    border: 0;
    box-sizing: border-box;
    display: flex;
    gap: 11px;
    height: 34px;
    margin: 0 0 4px;
    padding: 7px 10px;
    width: 100%;
  }

  /*
   * CDXC:DocsSidebarSearch 2026-06-30-11:11:
   * Docs file search needs an inline X button that appears only while text is present; clicking it or pressing Escape clears the filter and keeps keyboard focus in the search field.
   */
  .manage-search:focus-within {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
  }

  .manage-search > svg {
    color: var(--manage-text);
    flex: 0 0 auto;
    pointer-events: none;
  }

  .manage-search input {
    background: transparent;
    border: 0;
    color: var(--manage-text);
    flex: 1 1 auto;
    font-size: 15.55px;
    font-weight: 300;
    line-height: 18px;
    min-width: 0;
    outline: 0;
    padding: 0;
    width: 100%;
  }

  .manage-search input::placeholder {
    color: color-mix(in srgb, var(--manage-text) 52%, transparent);
  }

  .manage-search-clear-button {
    align-items: center;
    background: transparent;
    border: 0;
    color: color-mix(in srgb, var(--manage-text) 58%, transparent);
    display: inline-flex;
    flex: 0 0 auto;
    height: 20px;
    justify-content: center;
    margin-right: -3px;
    padding: 0;
    width: 20px;
  }

  .manage-search-clear-button:hover,
  .manage-search-clear-button:focus-visible {
    color: var(--manage-text);
    outline: none;
  }

  .manage-file-list {
    min-height: 0;
    overflow: auto;
    padding: 0 0 10px;
    position: relative;
    scrollbar-color: transparent transparent;
    scrollbar-width: thin;
  }

  .manage-file-list:hover,
  .manage-file-list:focus-within {
    scrollbar-color: rgba(255, 255, 255, 0.38) transparent;
  }

  .manage-file-list::-webkit-scrollbar {
    height: 2px;
    width: 2px;
  }

  .manage-file-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .manage-file-list::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .manage-file-list:hover::-webkit-scrollbar-thumb,
  .manage-file-list:focus-within::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.38);
  }

  .manage-file-list::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.54);
  }

  .manage-file-list::before {
    background: #c8cdd5;
    box-shadow:
      0 0 0 1px rgba(200, 205, 213, 0.22),
      0 0 14px rgba(200, 205, 213, 0.24);
    content: "";
    height: 3px;
    left: 12px;
    opacity: 0;
    pointer-events: none;
    position: absolute;
    right: 12px;
    top: 0;
    transition: opacity 120ms ease;
    z-index: 3;
  }

  .manage-file-list[data-root-drop-target="true"]::before {
    opacity: 1;
  }

  /*
   * CDXC:DocsSidebar 2026-06-30-03:20:
   * The Docs file tree should sit 5px closer to the sidebar's left edge while the Search field keeps its current padding and icon alignment.
   */
  .manage-file-row {
    --depth: 0;
    align-items: center;
    background: transparent;
    border: 0;
    box-sizing: border-box;
    color: var(--manage-muted);
    display: grid;
    gap: 9px;
    grid-template-columns: 14px 16px minmax(0, 1fr) auto;
    min-height: 29px;
    padding: 4px 7px 4px calc(9px + (var(--depth) * 18px));
    position: relative;
    text-align: left;
    width: 100%;
  }

  .manage-file-row:hover,
  .manage-file-row:focus-visible {
    background: color-mix(in srgb, var(--manage-text) 8%, transparent);
    color: var(--manage-text);
    outline: none;
  }

  .manage-file-row[data-kind="directory"] {
    color: var(--manage-muted);
    font-weight: 300;
  }

  .manage-file-row[data-kind="directory"][data-active-descendant="true"] {
    color: #ffffff;
  }

  .manage-file-row[data-selected="true"] {
    background: color-mix(in srgb, var(--manage-row-surface) 72%, transparent);
    color: #ffffff;
  }

  .manage-file-row[data-context-menu-open="true"] {
    background: var(--manage-row-surface);
    color: var(--manage-text);
  }

  .manage-file-row[data-dragging="true"] {
    opacity: 0.18;
  }

  .manage-file-row[data-drop-target="true"] {
    background: var(--manage-row-surface);
    color: var(--manage-text);
  }

  .manage-file-disclosure {
    align-items: center;
    color: var(--manage-subtle);
    display: inline-flex;
    height: 14px;
    justify-content: center;
    width: 14px;
  }

  .manage-file-disclosure[data-visible="false"] {
    opacity: 0;
  }

  .manage-file-disclosure svg {
    transition: transform 120ms ease;
  }

  .manage-file-row[aria-expanded="true"] .manage-file-disclosure svg {
    transform: rotate(90deg);
  }

  .manage-file-row[data-active-descendant="true"] .manage-file-disclosure {
    color: currentColor;
  }

  .manage-file-icon {
    color: currentColor;
  }

  .manage-file-name {
    font-size: 15.55px;
    font-weight: 300;
    line-height: 18px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-file-badges {
    align-items: center;
    display: flex;
    gap: 5px;
    min-width: 0;
  }

  .manage-count-badge {
    align-items: center;
    background: rgba(253, 230, 138, 0.14);
    border: 1px solid rgba(253, 230, 138, 0.32);
    border-radius: 4px;
    color: var(--manage-yellow);
    display: inline-flex;
    font-size: 10px;
    font-weight: 750;
    height: 17px;
    justify-content: center;
    min-width: 17px;
    padding: 0 5px;
  }

  /*
   * CDXC:ManageFileActions 2026-08-08:
   * Docs uses the shared sidebar context-menu stylesheet and class contract.
   * Keep only Docs-specific tokens and nested-row layout here so its menu
   * surface, spacing, square corners, hover, dividers, and danger rows cannot
   * drift from the GPUI sidebar menu again.
   */
  .sidebar-context-menu-backdrop,
  .manage-rename-backdrop {
    background: transparent;
    border: 0;
    cursor: default;
    inset: 0;
    margin: 0;
    padding: 0;
    position: fixed;
    z-index: 60;
  }

  .manage-file-context-menu {
    --app-border: var(--manage-border);
    --app-card: var(--manage-panel);
    --app-context-menu-hover-background: var(--manage-row-surface);
    color: var(--manage-text);
    font-size: 12px;
    font-weight: 400;
  }

  .manage-file-context-menu-item {
    line-height: 16px;
  }

  .manage-file-context-menu-item svg {
    color: currentColor;
  }

  .manage-file-context-menu-nested {
    display: grid;
    gap: 2px;
  }

  .manage-file-context-menu-subitem {
    padding-left: 28px;
  }

  .manage-file-context-menu-spacer {
    flex: 1 1 auto;
    min-width: 10px;
  }

  .manage-file-context-menu-item .manage-file-context-menu-chevron {
    height: 13px;
    transform: rotate(0deg);
    transition: transform 120ms ease;
    width: 13px;
  }

  .manage-file-context-menu-item .manage-file-context-menu-chevron[data-open="true"] {
    transform: rotate(90deg);
  }

  .manage-file-context-menu-item:disabled {
    cursor: wait;
    opacity: 0.42;
  }

  .manage-file-context-menu-item-danger[data-confirming="true"] {
    background: color-mix(in srgb, #ff7b72 18%, transparent);
  }

  .manage-rename-backdrop {
    background: rgba(0, 0, 0, 0.34);
    z-index: 70;
  }

  .manage-rename-dialog {
    background: color-mix(in srgb, var(--manage-panel-raised) 94%, #000 6%);
    border: 1px solid var(--manage-border-strong);
    box-shadow:
      0 20px 52px rgba(0, 0, 0, 0.46),
      0 0 0 1px rgba(255, 255, 255, 0.04);
    color: var(--manage-text);
    display: grid;
    gap: 10px;
    left: 50%;
    max-width: calc(100vw - 32px);
    padding: 12px;
    position: fixed;
    top: 18%;
    transform: translateX(-50%);
    width: min(360px, calc(100vw - 32px));
    z-index: 71;
  }

  .manage-rename-header {
    align-items: center;
    display: flex;
    gap: 10px;
    min-width: 0;
  }

  .manage-rename-header span {
    flex: 1 1 auto;
    font-size: 13px;
    font-weight: 700;
    min-width: 0;
  }

  .manage-rename-close {
    height: 26px;
    width: 26px;
  }

  .manage-rename-input {
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid var(--manage-border);
    color: var(--manage-text);
    font-size: 13px;
    height: 34px;
    min-width: 0;
    outline: 0;
    padding: 0 9px;
    width: 100%;
  }

  .manage-rename-input:focus {
    border-color: rgba(125, 211, 252, 0.58);
  }

  .manage-rename-error {
    color: var(--manage-red);
    font-size: 12px;
    line-height: 1.35;
  }

  .manage-rename-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .manage-rename-secondary,
  .manage-rename-primary {
    align-items: center;
    border: 1px solid var(--manage-border);
    display: inline-flex;
    font-size: 12px;
    font-weight: 680;
    height: 30px;
    justify-content: center;
    min-width: 76px;
    padding: 0 11px;
  }

  .manage-rename-secondary {
    background: rgba(255, 255, 255, 0.04);
    color: rgba(248, 250, 252, 0.78);
  }

  .manage-rename-primary {
    background: rgba(125, 211, 252, 0.16);
    border-color: rgba(125, 211, 252, 0.42);
    color: var(--manage-text);
  }

  .manage-rename-secondary:hover,
  .manage-rename-secondary:focus-visible,
  .manage-rename-primary:hover,
  .manage-rename-primary:focus-visible {
    background: rgba(255, 255, 255, 0.08);
    color: var(--manage-text);
    outline: none;
  }

  .manage-rename-secondary:disabled,
  .manage-rename-primary:disabled {
    color: var(--manage-subtle);
    cursor: wait;
  }

  .manage-empty {
    align-items: center;
    color: var(--manage-subtle);
    display: flex;
    font-size: 12px;
    gap: 8px;
    justify-content: center;
    min-height: 72px;
    padding: 14px;
  }

  .manage-preview {
    background: var(--manage-bg);
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .manage-preview-content {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .manage-preview-content[data-compact-header="true"] {
    grid-template-rows: auto minmax(0, 1fr);
  }

  .manage-preview-header {
    align-items: center;
    border-bottom: 1px solid var(--manage-border);
    box-sizing: border-box;
    display: flex;
    gap: 8px;
    height: 35px;
    max-height: 35px;
    min-height: 35px;
    overflow: visible;
    padding: 0 0 0 13px;
  }

  .manage-preview-content[data-kind="drawing"] .manage-preview-header {
    padding-right: 13px;
  }

  .manage-preview-title {
    /*
     * CDXC:DocsHeader 2026-07-01-00:11:
     * Long project-relative Docs filenames should truncate before they can
     * displace metadata or header action buttons. Use a zero flex basis and
     * hidden overflow so the title yields width first while keeping the file
     * icon anchored at the left edge.
     */
    align-items: center;
    display: flex;
    flex: 1 1 0;
    font-size: 12px;
    font-weight: 680;
    gap: 7px;
    line-height: 35px;
    min-width: 0;
    overflow: hidden;
  }

  .manage-preview-title svg {
    flex: 0 0 auto;
    height: 15px;
    width: 15px;
  }

  .manage-preview-title span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-preview-meta {
    align-items: center;
    color: var(--manage-subtle);
    display: flex;
    flex: 0 0 auto;
    font-size: 10.5px;
    font-weight: 650;
    gap: 9px;
    line-height: 35px;
    min-width: max-content;
  }

  .manage-preview-header-actions {
    align-items: stretch;
    align-self: stretch;
    display: inline-flex;
    flex: 0 0 auto;
    gap: 0;
    height: 100%;
    min-width: 0;
  }

  .manage-annotation-dropdown-shell {
    display: inline-flex;
    margin-right: 7px;
    position: relative;
  }

  .manage-preview-content[data-kind="markdown"] .manage-annotation-dropdown-shell {
    margin-right: 0;
  }

  /*
   * CDXC:DocsAnnotationToolbar 2026-06-30-22:58:
   * When the right-side Docs sidebar is hidden, the restore button already owns the titlebar edge spacing. Remove the annotation dropdown shell's extra right margin so no empty strip appears between the comments/count button and the restore control.
   */
  .manage-shell[data-sidebar-hidden="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-annotation-dropdown-shell,
  .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] .manage-preview-content[data-kind="markdown"] .manage-annotation-dropdown-shell {
    margin-right: 0;
  }

  .manage-preview-path {
    border-bottom: 1px solid rgba(255, 255, 255, 0.055);
    color: var(--manage-subtle);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 11px;
    overflow: hidden;
    padding: 8px 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-text-editor {
    background: var(--manage-bg);
    border: 0;
    color: rgba(248, 250, 252, 0.88);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 12px;
    height: 100%;
    line-height: 1.55;
    margin: 0;
    min-height: 0;
    outline: 0;
    overflow: auto;
    padding: 16px 18px 28px;
    resize: none;
    tab-size: 2;
    white-space: pre;
    width: 100%;
  }

  /*
   * CDXC:ManageHtmlRendering 2026-06-29-17:25:
   * Rendered HTML Docs should give the artifact an isolated browser-like viewport. Do not apply Ghostex typography, padding, link colors, or dark background to the iframe because the HTML document's own CSS must decide how the page looks.
   *
   * CDXC:ManageHtmlRendering 2026-06-30-04:41:
   * The iframe element itself should not paint a white scrollbar gutter around dark HTML documents. Keep it transparent over the Manage background while the loaded document still owns its actual page background.
   */
  .manage-html-render-view {
    background: transparent;
    border: 0;
    color-scheme: dark;
    display: block;
    height: 100%;
    min-height: 0;
    min-width: 0;
    width: 100%;
  }

  .manage-markdown-review {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    width: 100%;
  }

  .manage-markdown-meo-review {
    background: var(--manage-bg);
  }

  .manage-markdown-review-main {
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    width: 100%;
  }

  .manage-preview-header-actions button,
  .manage-comment-popover-actions button,
  .manage-markdown-selection-toolbar button {
    align-items: center;
    display: inline-flex;
    font-size: 11px;
    font-weight: 750;
    gap: 5px;
    justify-content: center;
    min-width: 0;
  }

  .manage-preview-header-actions button,
  .manage-comment-popover-actions button {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--manage-border);
    color: var(--manage-muted);
    height: 28px;
    padding: 0 8px;
  }

  .manage-preview-header-actions button {
    background: transparent;
    border: 0;
    border-left: 1px solid #252525;
    border-radius: 0;
    box-shadow: none;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.84);
    font-size: 10.5px;
    font-weight: 650;
    height: 35px;
    line-height: 35px;
    max-height: 35px;
    min-height: 35px;
    min-width: 38px;
    padding: 0 10px;
  }

  .manage-preview-header-actions button:not(:disabled):hover,
  .manage-preview-header-actions button:not(:disabled):focus-visible,
  .manage-comment-popover-actions button:not(:disabled):hover,
  .manage-comment-popover-actions button:not(:disabled):focus-visible {
    background: rgba(125, 211, 252, 0.12);
    border-color: rgba(125, 211, 252, 0.32);
    color: var(--manage-text);
    outline: none;
  }

  .manage-preview-header-actions button:not(:disabled):hover,
  .manage-preview-header-actions button:not(:disabled):focus-visible,
  .manage-preview-header-actions button[aria-expanded="true"],
  .manage-preview-header-actions .manage-annotation-toggle[aria-pressed="true"] {
    background: rgba(255, 255, 255, 0.08);
    border-color: #252525;
    color: rgba(255, 255, 255, 0.96);
    outline: none;
  }

  .manage-preview-header-actions button:disabled,
  .manage-comment-popover-actions button:disabled {
    color: var(--manage-subtle);
  }

  .manage-preview-header-actions button:disabled {
    background: transparent;
    color: rgba(255, 255, 255, 0.34);
    cursor: default;
  }

  .manage-preview-header-actions button:disabled:hover {
    background: transparent;
    color: rgba(255, 255, 255, 0.34);
  }

  .manage-preview-header-actions .manage-annotation-toggle[aria-pressed="true"] {
    border-left-color: #252525;
  }

  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"] {
    background: rgba(244, 63, 94, 0.13);
    border-color: rgba(244, 63, 94, 0.34);
    color: #fda4af;
  }

  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"]:not(:disabled):hover,
  .manage-preview-header-actions .manage-clear-annotations-button[data-confirming="true"]:not(:disabled):focus-visible {
    background: rgba(244, 63, 94, 0.18);
    border-color: rgba(244, 63, 94, 0.46);
    color: #fecdd3;
  }

  .manage-preview-header-actions .manage-annotation-dropdown-trigger {
    padding: 0 9px;
  }

  .manage-preview-header-actions .manage-count-badge {
    height: 17px;
    min-width: 17px;
    padding: 0 4px;
  }

  .manage-preview-header-actions button svg {
    height: 16px;
    width: 16px;
  }

  .manage-preview-header-actions .manage-file-reload-button {
    padding: 0 10px;
    position: relative;
  }

  .manage-file-change-indicator {
    background: #fbbf24;
    border: 1px solid #0e0e0e;
    border-radius: 999px;
    box-shadow: 0 0 0 1px rgba(251, 191, 36, 0.18);
    height: 7px;
    pointer-events: none;
    position: absolute;
    right: 6px;
    top: 6px;
    width: 7px;
  }

  .manage-meo-markdown-editor {
    background: #101112;
    box-sizing: border-box;
    color: rgba(248, 250, 252, 0.9);
    inline-size: 100%;
    max-inline-size: 100%;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  /*
   * CDXC:ManageMarkdownLayout 2026-06-30-13:45:
   * The embedded Meo editor must keep both its toolbar and CodeMirror surface owned by the Manage preview column after heading formatting changes remeasure live Markdown content.
   * Keep Meo's single-row toolbar layout, measure before hiding the three secondary right-side utility buttons, and use one Live/Source toggle button instead of a two-option segmented control.
   */
  .manage-meo-markdown-editor .mode-toolbar {
    box-sizing: border-box;
    display: flex;
    flex: 0 0 auto;
    gap: 8px;
    inline-size: 100%;
    max-inline-size: 100%;
    min-width: 0;
    overflow: visible;
  }

  .manage-meo-markdown-editor .format-group {
    /*
     * CDXC:ManageMarkdownLayout 2026-07-01-00:11:
     * The left formatting group must not push the persistent right-side toolbar
     * controls outside narrow Docs panes. Let it shrink from zero-basis and
     * clip lower-priority formatting buttons before search, display toggles, or
     * the Live/Source mode control leave the visible toolbar.
     */
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
  }

  .manage-meo-markdown-editor .right-group,
  .manage-meo-markdown-editor .mode-group {
    flex: 0 0 auto;
  }

  .manage-meo-markdown-editor .right-group {
    margin-left: auto;
    margin-right: 0;
    min-width: 0;
  }

  .manage-meo-markdown-editor .mode-group {
    background: rgba(255, 255, 255, 0.025);
    border-color: rgba(255, 255, 255, 0.16);
    border-radius: 9px;
    gap: 2px;
  }

  .manage-meo-markdown-editor .mode-button {
    color: var(--manage-muted);
    min-width: 64px;
  }

  .manage-meo-markdown-editor .manage-mode-toggle {
    min-width: 76px;
  }

  .manage-meo-markdown-editor .mode-button[aria-selected="true"],
  .manage-meo-markdown-editor .mode-button.is-active {
    background: rgba(125, 211, 252, 0.18);
    box-shadow: inset 0 0 0 1px rgba(125, 211, 252, 0.34);
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .mode-button[aria-selected="false"]:hover,
  .manage-meo-markdown-editor .mode-button:not(.is-active):hover {
    background: rgba(255, 255, 255, 0.07);
    color: var(--manage-text);
  }

  .manage-meo-markdown-editor .table-grid-cell {
    appearance: none;
    padding: 0;
  }

  .manage-selection-inline-mode-button {
    color: ${MANAGE_MEO_HEADING_COLOR};
  }

  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block):not(.meo-mermaid-block) .meo-md-inline-code,
  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block):not(.meo-mermaid-block) .meo-md-inline-code * {
    background: ${MANAGE_MEO_CODE_BLOCK_BACKGROUND} !important;
    color: ${MANAGE_MEO_CODE_COLOR} !important;
    -webkit-text-fill-color: ${MANAGE_MEO_CODE_COLOR} !important;
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block),
  .manage-meo-markdown-editor .cm-line.meo-md-alert:is(.meo-md-code-block, .meo-src-code-block) {
    background: ${MANAGE_MEO_CODE_BLOCK_BACKGROUND} !important;
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) {
    --manage-code-block-top-border: transparent;
    --manage-code-block-bottom-border: transparent;
    box-shadow:
      inset 1px 0 0 var(--manage-border-strong),
      inset -1px 0 0 var(--manage-border-strong),
      inset 0 1px 0 var(--manage-code-block-top-border),
      inset 0 -1px 0 var(--manage-code-block-bottom-border);
  }

  .manage-meo-markdown-editor .cm-line:not(.meo-md-code-block):not(.meo-src-code-block) + .cm-line:is(.meo-md-code-block, .meo-src-code-block),
  .manage-meo-markdown-editor .cm-content > .cm-line:is(.meo-md-code-block, .meo-src-code-block):first-child {
    --manage-code-block-top-border: var(--manage-border-strong);
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block):has(+ .cm-line:not(.meo-md-code-block):not(.meo-src-code-block)),
  .manage-meo-markdown-editor .cm-content > .cm-line:is(.meo-md-code-block, .meo-src-code-block):last-child {
    --manage-code-block-bottom-border: var(--manage-border-strong);
  }

  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="#fde68a" i],
  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="rgb(253, 230, 138)" i],
  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="#c084fc" i],
  .manage-meo-markdown-editor .cm-line:is(.meo-md-code-block, .meo-src-code-block) [style*="rgb(192, 132, 252)" i] {
    color: ${MANAGE_MEO_VARIABLE_COLOR} !important;
    -webkit-text-fill-color: ${MANAGE_MEO_VARIABLE_COLOR} !important;
  }

  .manage-meo-markdown-editor .editor-wrapper,
  .manage-meo-markdown-editor .editor-host,
  .manage-meo-markdown-editor .cm-editor,
  .manage-meo-markdown-editor .cm-scroller,
  .manage-meo-markdown-editor .cm-content,
  .manage-meo-markdown-editor .cm-line {
    box-sizing: border-box;
    inline-size: 100%;
    max-inline-size: 100%;
    min-height: 0;
    min-width: 0;
  }

  .manage-meo-markdown-editor .cm-editor {
    background: #101112;
    height: 100%;
  }

  .manage-meo-markdown-editor .cm-scroller {
    scrollbar-color: rgba(148, 163, 184, 0.35) transparent;
  }

  .manage-meo-markdown-editor .cm-gutters {
    max-width: 47px;
    min-width: 47px;
  }

  .manage-meo-markdown-editor .cm-gutter.meo-md-fold-gutter {
    max-width: 16px;
    min-width: 16px;
    width: 16px;
  }

  .manage-meo-markdown-editor .cm-gutter.cm-lineNumbers,
  .manage-meo-markdown-editor .cm-lineNumbers .cm-gutterElement {
    max-width: 28px;
    min-width: 28px;
    width: 28px;
  }

  .manage-meo-markdown-editor .cm-lineNumbers .cm-gutterElement {
    align-items: flex-start;
    padding: 0 4px 0 0;
  }

  .manage-meo-markdown-editor .cm-content {
    margin-left: 0;
    margin-right: 0;
    padding-right: 12px;
  }

  .manage-markdown-document {
    color: rgba(248, 250, 252, 0.9);
    font-size: 15px;
    line-height: 1.625;
    min-height: 0;
    overflow: auto;
    padding: 24px 32px 48px;
  }

  .manage-markdown-document > :first-child {
    margin-top: 0;
  }

  .manage-markdown-document h1,
  .manage-markdown-document h2,
  .manage-markdown-document h3,
  .manage-markdown-document h4,
  .manage-markdown-document h5,
  .manage-markdown-document h6 {
    color: ${MANAGE_MEO_HEADING_COLOR};
    letter-spacing: 0;
    line-height: 1.22;
  }

  .manage-markdown-document h1 {
    font-size: 24px;
    font-weight: 750;
    margin: 24px 0 16px;
  }

  .manage-markdown-document h2 {
    font-size: 20px;
    font-weight: 700;
    margin: 32px 0 12px;
  }

  .manage-markdown-document h3 {
    font-size: 16px;
    font-weight: 700;
    margin: 24px 0 8px;
  }

  .manage-markdown-document h4,
  .manage-markdown-document h5,
  .manage-markdown-document h6 {
    font-size: 15px;
    font-weight: 700;
    margin: 18px 0 8px;
  }

  .manage-markdown-document p {
    margin: 0 0 16px;
  }

  .manage-markdown-document a {
    color: var(--manage-accent);
    text-decoration: none;
  }

  .manage-markdown-document a:hover,
  .manage-markdown-document a:focus-visible {
    text-decoration: underline;
  }

  .manage-markdown-document blockquote {
    border-left: 2px solid rgba(125, 211, 252, 0.48);
    color: var(--manage-muted);
    font-style: italic;
    margin: 16px 0;
    padding-left: 16px;
  }

  .manage-markdown-document blockquote p:last-child,
  .manage-md-alert p:last-child,
  .manage-md-directive p:last-child {
    margin-bottom: 0;
  }

  .manage-md-empty {
    color: var(--manage-subtle);
  }

  .manage-md-inline-code {
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    color: rgba(248, 250, 252, 0.92);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    font-size: 0.9em;
    padding: 1px 4px;
  }

  .manage-md-inline-image {
    border: 1px solid var(--manage-border);
    display: block;
    margin: 12px 0;
    max-width: 100%;
  }

  .manage-md-list-item {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    margin: 6px 0 6px calc(var(--manage-md-list-level, 0) * 20px);
  }

  .manage-md-list-marker {
    color: var(--manage-muted);
    flex: 0 0 22px;
    font-size: 13px;
    line-height: 1.625;
    text-align: right;
  }

  .manage-md-list-marker input {
    height: 13px;
    margin: 4px 0 0;
    width: 13px;
  }

  .manage-md-list-text {
    color: rgba(248, 250, 252, 0.9);
    min-width: 0;
  }

  .manage-md-list-text.is-checked {
    color: var(--manage-muted);
    text-decoration: line-through;
  }

  .manage-md-code-block {
    margin: 20px 0;
    position: relative;
  }

  .manage-md-code-block button {
    align-items: center;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid var(--manage-border);
    color: var(--manage-muted);
    display: inline-flex;
    height: 28px;
    justify-content: center;
    opacity: 0;
    padding: 0;
    position: absolute;
    right: 8px;
    top: 8px;
    transition: opacity 120ms ease;
    width: 28px;
  }

  .manage-md-code-block:hover button,
  .manage-md-code-block button:focus-visible {
    opacity: 1;
  }

  .manage-md-code-block pre {
    background: rgba(255, 255, 255, 0.045);
    border: 1px solid rgba(255, 255, 255, 0.09);
    border-radius: 8px;
    color: rgba(248, 250, 252, 0.88);
    font-size: 13px;
    line-height: 1.6;
    margin: 0;
    overflow-x: auto;
    padding: 16px;
  }

  .manage-md-code-block code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  }

  .manage-md-table-wrap {
    margin: 16px 0;
    overflow-x: auto;
  }

  .manage-md-table-wrap table {
    border-collapse: collapse;
    min-width: 100%;
  }

  .manage-md-table-wrap th,
  .manage-md-table-wrap td {
    border-bottom: 1px solid var(--manage-border);
    font-size: 14px;
    padding: 8px 12px;
    text-align: left;
    vertical-align: top;
  }

  .manage-md-table-wrap th {
    background: rgba(255, 255, 255, 0.045);
    color: rgba(248, 250, 252, 0.9);
    font-weight: 700;
  }

  .manage-md-table-wrap td {
    color: rgba(248, 250, 252, 0.8);
  }

  .manage-md-alert,
  .manage-md-directive {
    border: 1px solid rgba(125, 211, 252, 0.26);
    border-left: 3px solid rgba(125, 211, 252, 0.72);
    margin: 16px 0;
    padding: 12px 14px;
  }

  .manage-md-alert-title {
    color: var(--manage-accent);
    font-size: 11px;
    font-weight: 780;
    margin-bottom: 6px;
    text-transform: uppercase;
  }

  .manage-md-alert[data-kind="warning"],
  .manage-md-alert[data-kind="caution"] {
    border-color: rgba(253, 230, 138, 0.3);
    border-left-color: rgba(253, 230, 138, 0.72);
  }

  .manage-md-html-block {
    color: rgba(248, 250, 252, 0.9);
    font-size: 15px;
    line-height: 1.625;
    margin: 16px 0;
  }

  .annotation-highlight,
  .manage-annotation-highlight {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
    background: color-mix(in srgb, var(--manage-annotation-color) 28%, transparent);
    color: inherit;
    padding: 0 2px;
  }

  .annotation-highlight.comment,
  .manage-annotation-highlight[data-type="comment"] {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
  }

  .annotation-highlight[data-label-id="clarify"],
  .manage-annotation-highlight[data-label-id="clarify"] {
    --manage-annotation-color: ${quickLabelColor("clarify")};
  }

  .annotation-highlight[data-label-id="needs-tests"],
  .manage-annotation-highlight[data-label-id="needs-tests"] {
    --manage-annotation-color: ${quickLabelColor("needs-tests")};
  }

  .annotation-highlight[data-label-id="looks-good"],
  .manage-annotation-highlight[data-label-id="looks-good"] {
    --manage-annotation-color: ${quickLabelColor("looks-good")};
  }

  .annotation-highlight.deletion,
  .manage-annotation-highlight[data-type="redline"] {
    --manage-annotation-color: ${MANAGE_REDLINE_ANNOTATION_COLOR};
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--manage-annotation-color) 82%, transparent);
    text-decoration-thickness: 2px;
  }

  .manage-annotation-dropdown {
    background: color-mix(in srgb, var(--manage-panel-raised) 94%, #000 6%);
    border: 1px solid var(--manage-border-strong);
    border-radius: 5px;
    box-shadow: 0 18px 52px rgba(0, 0, 0, 0.36);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    max-height: min(520px, calc(100vh - 76px));
    min-height: 0;
    overflow: hidden;
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    width: min(360px, calc(100vw - 28px));
    z-index: 700;
  }

  .manage-annotation-dropdown header {
    align-items: center;
    border-bottom: 1px solid var(--manage-border);
    color: var(--manage-muted);
    display: flex;
    font-size: 12px;
    font-weight: 750;
    justify-content: space-between;
    min-height: 40px;
    padding: 0 12px;
  }

  .manage-annotation-dropdown-list {
    align-content: start;
    display: grid;
    gap: 8px;
    grid-auto-rows: max-content;
    min-height: 0;
    overflow: auto;
    padding: 10px;
  }

  .manage-attachment-strip {
    display: grid;
    gap: 6px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .manage-attachment-chip {
    align-items: center;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--manage-border);
    border-radius: 6px;
    display: grid;
    gap: 6px;
    grid-template-columns: 34px minmax(0, 1fr) 20px;
    margin: 0;
    min-width: 0;
    padding: 5px;
  }

  .manage-attachment-chip img,
  .manage-annotation-attachments img {
    background: rgba(255, 255, 255, 0.06);
    border-radius: 4px;
    height: 34px;
    object-fit: cover;
    width: 34px;
  }

  .manage-attachment-chip figcaption,
  .manage-annotation-attachments span {
    color: var(--manage-muted);
    font-size: 10px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-attachment-chip button {
    align-items: center;
    background: transparent;
    border: 0;
    color: var(--manage-muted);
    display: inline-flex;
    height: 20px;
    justify-content: center;
    padding: 0;
    width: 20px;
  }

  .manage-attachment-error {
    color: var(--manage-red);
    font-size: 11px;
    line-height: 1.35;
  }

  .manage-annotation-empty {
    color: var(--manage-subtle);
    font-size: 12px;
    padding: 12px 2px;
  }

  .manage-annotation-card {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
    align-self: start;
    background: color-mix(in srgb, var(--manage-panel) 96%, var(--manage-annotation-color) 4%);
    border: 1px solid color-mix(in srgb, var(--manage-annotation-color) 24%, var(--manage-border));
    border-radius: 4px;
    display: grid;
    gap: 7px;
    height: max-content;
    min-width: 0;
    padding: 9px 33px 9px 9px;
    position: relative;
  }

  .manage-annotation-card[data-type="redline"] {
    border-color: color-mix(in srgb, var(--manage-annotation-color) 28%, var(--manage-border));
  }

  .manage-annotation-card-header {
    align-items: center;
    color: var(--manage-muted);
    display: flex;
    font-size: 11px;
    font-weight: 760;
    justify-content: space-between;
  }

  .manage-annotation-card-header span {
    color: color-mix(in srgb, var(--manage-annotation-color) 72%, var(--manage-text));
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .manage-preview-header-actions .manage-annotation-remove-button,
  .manage-annotation-remove-button {
    background: transparent;
    border: 0;
    border-left: 0;
    border-radius: 3px;
    box-shadow: none;
    color: color-mix(in srgb, var(--manage-annotation-color) 48%, var(--manage-muted));
    height: 22px;
    padding: 0;
    position: absolute;
    right: 7px;
    top: 7px;
    transition: background 120ms ease, color 120ms ease;
    width: 22px;
  }

  .manage-preview-header-actions .manage-annotation-remove-button:not(:disabled):hover,
  .manage-preview-header-actions .manage-annotation-remove-button:not(:disabled):focus-visible,
  .manage-annotation-remove-button:hover,
  .manage-annotation-remove-button:focus-visible {
    background: transparent;
    border: 0;
    border-left: 0;
    color: color-mix(in srgb, var(--manage-annotation-color) 70%, var(--manage-text));
  }

  .manage-annotation-card blockquote {
    border-left: 2px solid color-mix(in srgb, var(--manage-annotation-color) 62%, transparent);
    color: rgba(248, 250, 252, 0.86);
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    max-height: 96px;
    overflow: auto;
    padding-left: 8px;
    scrollbar-color: transparent transparent;
    scrollbar-width: thin;
  }

  .manage-annotation-card blockquote::-webkit-scrollbar {
    height: 2px;
    width: 2px;
  }

  .manage-annotation-card blockquote::-webkit-scrollbar-track,
  .manage-annotation-card blockquote::-webkit-scrollbar-track-piece,
  .manage-annotation-card blockquote::-webkit-scrollbar-corner {
    background: transparent;
  }

  .manage-annotation-card blockquote::-webkit-scrollbar-thumb {
    background: transparent;
  }

  .manage-annotation-card:hover blockquote,
  .manage-annotation-card:focus-within blockquote {
    scrollbar-color: color-mix(in srgb, var(--manage-annotation-color) 58%, transparent) transparent;
  }

  .manage-annotation-card:hover blockquote::-webkit-scrollbar-thumb,
  .manage-annotation-card:focus-within blockquote::-webkit-scrollbar-thumb {
    background: color-mix(in srgb, var(--manage-annotation-color) 58%, transparent);
  }

  .manage-annotation-card[data-type="redline"] blockquote {
    text-decoration: line-through;
    text-decoration-color: color-mix(in srgb, var(--manage-annotation-color) 82%, transparent);
    text-decoration-thickness: 2px;
  }

  .manage-annotation-card p {
    color: color-mix(in srgb, var(--manage-text) 72%, var(--manage-muted));
    font-size: 12px;
    line-height: 1.45;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .manage-annotation-attachments {
    display: grid;
    gap: 6px;
  }

  .manage-annotation-attachments a {
    align-items: center;
    color: inherit;
    display: grid;
    gap: 6px;
    grid-template-columns: 34px minmax(0, 1fr);
    text-decoration: none;
  }

  .manage-markdown-selection-toolbar {
    align-items: center;
    background: var(--manage-panel-raised);
    border: 1px solid var(--manage-border-strong);
    border-radius: 8px;
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.34);
    display: flex;
    gap: 5px;
    max-width: calc(100vw - 36px);
    overflow: visible;
    padding: 5px;
    position: fixed;
    transform: translateX(-50%);
    z-index: 10;
  }

  .manage-markdown-selection-toolbar button {
    --manage-toolbar-action-color: var(--manage-muted);
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 6px;
    color: var(--manage-toolbar-action-color);
    display: inline-flex;
    height: 30px;
    justify-content: center;
    padding: 0;
    position: relative;
    width: 30px;
  }

  .manage-markdown-selection-toolbar button svg {
    color: currentColor;
  }

  .manage-markdown-selection-toolbar button:hover,
  .manage-markdown-selection-toolbar button:focus-visible {
    background: color-mix(in srgb, var(--manage-toolbar-action-color) 16%, transparent);
    color: var(--manage-toolbar-action-color);
    outline: none;
  }

  .manage-annotation-preview-card {
    --manage-annotation-color: ${MANAGE_COMMENT_ANNOTATION_COLOR};
    background: color-mix(in srgb, var(--manage-panel-raised) 96%, var(--manage-annotation-color) 4%);
    border: 1px solid color-mix(in srgb, var(--manage-annotation-color) 28%, var(--manage-border-strong));
    border-radius: 8px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.42);
    color: var(--manage-text);
    display: grid;
    gap: 6px;
    max-width: calc(100vw - 24px);
    padding: 10px 36px 10px 12px;
    pointer-events: none;
    position: fixed;
    z-index: 39;
  }

  .manage-annotation-preview-card header {
    align-items: center;
    display: flex;
    font-size: 10px;
    font-weight: 760;
    justify-content: space-between;
    letter-spacing: 0;
    line-height: 1.1;
    text-transform: uppercase;
  }

  .manage-annotation-preview-card header span:first-child {
    color: color-mix(in srgb, var(--manage-annotation-color) 76%, var(--manage-text));
  }

  .manage-annotation-preview-card header span:last-child {
    color: var(--manage-muted);
    font-weight: 680;
    text-transform: none;
  }

  .manage-annotation-preview-remove-button {
    background: transparent;
    border: 0;
    box-shadow: none;
    color: color-mix(in srgb, var(--manage-annotation-color) 48%, var(--manage-muted));
    pointer-events: auto;
    position: absolute;
    right: 7px;
    top: 7px;
    transition: background 120ms ease, color 120ms ease;
  }

  .manage-annotation-preview-remove-button:hover,
  .manage-annotation-preview-remove-button:focus-visible {
    background: transparent;
    color: color-mix(in srgb, var(--manage-annotation-color) 70%, var(--manage-text));
  }

  .manage-annotation-preview-card p {
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    color: color-mix(in srgb, var(--manage-text) 88%, var(--manage-muted));
    display: -webkit-box;
    font-size: 12px;
    line-height: 1.4;
    margin: 0;
    overflow: hidden;
  }

  .manage-comment-popover {
    background: color-mix(in srgb, var(--manage-panel-raised) 76%, #000 24%);
    border: 1px solid color-mix(in srgb, var(--manage-border-strong) 74%, #000 26%);
    border-radius: 10px;
    box-shadow: 0 20px 54px rgba(0, 0, 0, 0.44);
    display: grid;
    gap: 10px;
    max-height: calc(100vh - 24px);
    overflow: auto;
    padding: 34px 12px 12px;
    position: fixed;
    z-index: 710;
  }

  .manage-comment-popover-close {
    color: var(--manage-muted);
    height: 24px;
    position: absolute;
    right: 8px;
    top: 8px;
    width: 24px;
  }

  .manage-comment-popover-close:hover,
  .manage-comment-popover-close:focus-visible {
    background: rgba(255, 255, 255, 0.075);
    color: var(--manage-text);
    outline: none;
  }

  .manage-comment-popover textarea {
    background: color-mix(in srgb, var(--manage-panel) 72%, #000 28%);
    border: 1px solid var(--manage-border-strong);
    border-radius: 8px;
    color: var(--manage-text);
    font-size: 12px;
    height: 116px;
    line-height: 1.45;
    outline: 0;
    padding: 10px;
    resize: vertical;
  }

  .manage-comment-popover textarea:focus {
    border-color: rgba(125, 211, 252, 0.46);
    box-shadow: 0 0 0 1px rgba(125, 211, 252, 0.16);
  }

  .manage-comment-popover-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  .manage-comment-popover-actions button {
    border-radius: 7px;
    color: var(--manage-text);
    height: 30px;
    padding: 0 11px;
  }

  .manage-comment-popover-actions .manage-comment-popover-image-button {
    background: rgba(255, 255, 255, 0.055);
    border-color: var(--manage-border-strong);
  }

  .manage-comment-popover-actions .manage-comment-popover-submit {
    background: rgba(34, 197, 94, 0.18);
    border-color: rgba(74, 222, 128, 0.48);
    color: #bbf7d0;
  }

  .manage-comment-popover-actions .manage-comment-popover-submit:not(:disabled):hover,
  .manage-comment-popover-actions .manage-comment-popover-submit:not(:disabled):focus-visible {
    background: rgba(34, 197, 94, 0.26);
    border-color: rgba(74, 222, 128, 0.66);
    color: #dcfce7;
  }

  .manage-comment-popover-actions .manage-comment-popover-submit:disabled {
    background: rgba(34, 197, 94, 0.08);
    border-color: rgba(74, 222, 128, 0.2);
    color: rgba(187, 247, 208, 0.42);
  }

  .manage-hidden-file-input {
    display: none;
  }

  .manage-drawing-editor {
    background: #101112;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    min-height: 0;
    position: relative;
  }

  .manage-drawing-editor .excalidraw {
    min-height: 0;
  }

  .manage-drawing-error {
    align-items: center;
    background: rgba(253, 164, 175, 0.12);
    border: 1px solid rgba(253, 164, 175, 0.3);
    color: var(--manage-red);
    display: flex;
    font-size: 12px;
    gap: 7px;
    left: 12px;
    max-width: calc(100% - 24px);
    padding: 7px 9px;
    position: absolute;
    top: 12px;
    z-index: 3;
  }

  .manage-drawing-source {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
  }

  .manage-preview-message {
    align-items: center;
    color: var(--manage-muted);
    display: flex;
    gap: 10px;
    height: 100%;
    justify-content: center;
    min-height: 140px;
    padding: 24px;
  }

  .manage-preview-message span {
    font-size: 13px;
    font-weight: 650;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  @media (max-width: 960px) {
    .manage-preview-header {
      align-items: center;
      flex-direction: row;
      gap: 8px;
      height: 35px;
      max-height: 35px;
      min-height: 35px;
      padding: 0 0 0 13px;
    }

    .manage-preview-meta {
      align-self: auto;
    }

    .manage-preview-content[data-compact-header="true"] .manage-preview-header {
      align-items: center;
      flex-direction: row;
      gap: 8px;
      height: 35px;
      max-height: 35px;
      min-height: 35px;
      padding: 0 0 0 13px;
    }

    .manage-preview-content[data-compact-header="true"] .manage-preview-meta {
      align-self: auto;
    }

    .manage-preview-content[data-kind="markdown"] .manage-preview-header-actions button span:not(.manage-count-badge):not(.manage-file-change-indicator) {
      display: none;
    }

    .manage-markdown-review {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (max-width: 760px) {
    .manage-shell:not([data-sidebar-hidden="true"]):not([data-sidebar-floating="true"]) {
      grid-template-columns: minmax(190px, 42%) 5px minmax(0, 1fr);
    }

    .manage-shell:not([data-sidebar-hidden="true"]):not([data-sidebar-floating="true"])[data-sidebar-side="right"] {
      grid-template-columns: minmax(0, 1fr) 5px minmax(190px, 42%);
    }

    .manage-shell[data-sidebar-hidden="true"],
    .manage-shell[data-sidebar-floating="true"],
    .manage-shell[data-sidebar-floating="true"][data-sidebar-side="right"] {
      grid-template-columns: minmax(0, 1fr);
    }

    .manage-preview-path,
    .manage-text-editor,
    .manage-markdown-document {
      padding-left: 14px;
      padding-right: 14px;
    }
  }
`;
document.head.append(styleElement);

createRoot(document.getElementById("root")!).render(
  <TooltipProvider>
    <ManageApp />
  </TooltipProvider>,
);
