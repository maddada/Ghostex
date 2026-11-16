import { useEffect, useMemo, useRef } from 'react';
import { installAppScrollbars } from '@/packages/components/ui/app-scrollbars';
import scrollbarStyles from '@/packages/components/ui/app-scrollbars.css?inline';
import { ManageWebKitWindow } from '../types';
import agentationBundle from '../../../vendor/agentation/agentation.generated.js?raw';

/**
 * Annotate's Agentation: Ghostex's fork with screenshot capture, built into one script by `cargo xtask build-agentation` (see CDXC:Browser 2026-11-07 in apps/desktop/src/app/helpers/browser.rs). It replaced agentation@3.1.2 from esm.sh.
 */
export const MANAGE_AGENTATION_BUNDLE = agentationBundle;

/** What a screenshot reference in Send's markdown points at until the app has saved the picture (`ANNOTATE_SCREENSHOT_REFERENCE_PREFIX` in helpers/browser.rs). */
export const MANAGE_ANNOTATION_SCREENSHOT_PREFIX = 'ghostex-annotation-screenshot:';

/** Annotate's Send in the rendered document: its markdown and screenshots, delivered by Files (the document asks by message). */
export type ManageAnnotationSend = (markdown: string, screenshots: { id: string; png: string }[]) => Promise<void>;

/** How Annotate starts in the rendered document (`annotateLoad`), and what Files keeps for its reloads. */
export type ManageAnnotateBoot = {
  on: boolean;
  theme: 'dark' | 'light';
  settings?: Record<string, unknown>;
  notes?: unknown[];
  /** The Annotate hotkey: the sealed page has the keyboard while it is focused, so it listens itself. */
  hotkey?: { key: string; code: string; ctrl: boolean; alt: boolean; shift: boolean; meta: boolean } | null;
};

/** A command from the Files header for the rendered document's Agentation (`ghostex-manage-annotate-command`). */
export type ManageAnnotateCommand = { command: string; value?: unknown };

/** The event the app dispatches into this page with a header command (app/native_docs/annotate.rs). */
export const MANAGE_ANNOTATE_COMMAND_EVENT = 'ghostex-manage-annotate-command';

export function ManageHtmlRenderViewer({
  annotate,
  content,
  documentKey,
  onAnnotateNotes,
  onAnnotateHotkey,
  onAnnotateState,
  onCopyAnnotation,
  onOpenDocument,
  onSendAnnotation,
  pageName,
}: {
  /** How Annotate starts; Agentation is loaded into every HTML file, idle until Annotate is on. */
  annotate: ManageAnnotateBoot;
  content: string;
  documentKey: string;
  /** The notes after each change, so Files keeps them across reloads and restarts. */
  onAnnotateNotes: (notes: unknown[]) => void;
  /** What the header's Annotate buttons show. */
  onAnnotateState: (state: Record<string, unknown>) => void;
  /** Annotate's Copy: the app writes the clipboard (a click in the header leaves this page unfocused). */
  onCopyAnnotation: (markdown: string) => void;
  /** The Annotate hotkey, pressed while the page had the keyboard. */
  onAnnotateHotkey: () => void;
  onOpenDocument: (path: string) => void;
  /** Annotate's Send: delivers the notes and their screenshots to the agent (see CDXC:Browser 2026-11-07 in app/page_annotations.rs). */
  onSendAnnotation: ManageAnnotationSend;
  /** What Annotate's copied feedback calls this page (see `manageHtmlPageName`). */
  pageName?: string;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const resourceBaseUrl = manageHtmlResourceBaseUrl(documentKey);
  // A reload (the file changed on disk) starts Agentation as Annotate is now, with the latest notes.
  const boot = useRef(annotate);
  boot.current = { ...boot.current, ...annotate, notes: boot.current.notes ?? annotate.notes };
  const renderedHtml = useMemo(
    () =>
      buildManageHtmlDocument(content, {
        annotate: boot.current,
        pageName,
        resourceBaseUrl,
      }),
    [content, pageName, resourceBaseUrl]
  );

  /*
   * CDXC:Docs 2026-11-14 DECISION:
   * The rendered file runs in its own sealed (opaque) origin, so its script cannot reach this page, the Files bridge or the app. It talks to Files only through `postMessage`, and only these messages are accepted, from this frame's own window: open another HTML file beside it, and Annotate's Send, Copy, notes and state for this file. Everything else is ignored.
   */
  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      const frameWindow = frame.current?.contentWindow;
      if (!frameWindow || event.source !== frameWindow) {
        return;
      }
      const message = parseManageHtmlFrameMessage(event.data);
      if (!message) {
        return;
      }
      switch (message.type) {
        case MANAGE_HTML_FRAME_OPEN_DOCUMENT: {
          const linkedDocumentPath = manageHtmlLinkedDocumentPath(message.href, resourceBaseUrl);
          if (linkedDocumentPath && linkedDocumentPath !== documentKey) {
            onOpenDocument(linkedDocumentPath);
          }
          return;
        }
        case MANAGE_HTML_FRAME_ANNOTATE_STATE:
          onAnnotateState(message.state);
          return;
        case MANAGE_HTML_FRAME_ANNOTATE_NOTES:
          boot.current = { ...boot.current, notes: message.notes };
          onAnnotateNotes(message.notes);
          return;
        case MANAGE_HTML_FRAME_ANNOTATE_COPY:
          onCopyAnnotation(message.markdown);
          return;
        case MANAGE_HTML_FRAME_ANNOTATE_HOTKEY:
          onAnnotateHotkey();
          return;
        case MANAGE_HTML_FRAME_SEND_ANNOTATION: {
          const reply = (error?: string) =>
            frameWindow.postMessage(
              { type: MANAGE_HTML_FRAME_SEND_ANNOTATION_RESULT, id: message.id, ...(error ? { error } : {}) },
              '*'
            );
          onSendAnnotation(message.markdown, message.screenshots).then(
            () => reply(),
            (reason: unknown) => reply(reason instanceof Error ? reason.message : String(reason))
          );
        }
      }
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [
    documentKey,
    onAnnotateHotkey,
    onAnnotateNotes,
    onAnnotateState,
    onCopyAnnotation,
    onOpenDocument,
    onSendAnnotation,
    resourceBaseUrl,
  ]);

  // The header's buttons reach Agentation through the app, this page and the frame, in that order.
  useEffect(() => {
    const onCommand = (event: Event) => {
      const detail = (event as CustomEvent<ManageAnnotateCommand>).detail;
      if (!detail || typeof detail.command !== 'string') {
        return;
      }
      if (detail.command === 'setActive') {
        boot.current = { ...boot.current, on: detail.value === true };
      } else if (detail.command === 'theme' && (detail.value === 'dark' || detail.value === 'light')) {
        boot.current = { ...boot.current, theme: detail.value };
      }
      frame.current?.contentWindow?.postMessage(
        { type: MANAGE_HTML_FRAME_ANNOTATE_COMMAND, command: detail.command, value: detail.value },
        '*'
      );
    };
    window.addEventListener(MANAGE_ANNOTATE_COMMAND_EVENT, onCommand);
    return () => window.removeEventListener(MANAGE_ANNOTATE_COMMAND_EVENT, onCommand);
  }, []);

  /*
   * CDXC:Docs 2026-08-08:
   * `clipboard-read` is denied rather than omitted. Omitting it inherits this
   * surface's permissive policy, and a programmatic `clipboard.readText()`
   * then hangs forever because Chromium wants a permission prompt that Alloy
   * cannot show. Denying it turns that into an immediate NotAllowedError.
   * User-initiated paste is unaffected: Cmd+V and the `paste` event carry
   * their data through `clipboardData`, which this policy does not gate.
   *
   * CDXC:Docs 2026-11-14 WHY:
   * Clipboard writes and fullscreen are granted with `*`: the document has an opaque origin now, which `'self'` (this page's origin) no longer matches, and a page's own copy buttons write to the clipboard. No `allow-same-origin`, and no `allow-popups-to-escape-sandbox`: see the decision above.
   */
  return (
    <iframe
      allow="clipboard-read 'none'; clipboard-write *; fullscreen *"
      aria-label='Rendered HTML document'
      className='manage-html-render-view'
      data-document-key={documentKey}
      ref={frame}
      sandbox='allow-downloads allow-forms allow-modals allow-popups allow-presentation allow-scripts'
      srcDoc={renderedHtml}
      title={documentKey}
    />
  );
}

/** What the rendered document may ask of Files (see CDXC:Docs 2026-11-14 in `ManageHtmlRenderViewer`). */
export const MANAGE_HTML_FRAME_OPEN_DOCUMENT = 'ghostex-open-document';
export const MANAGE_HTML_FRAME_SEND_ANNOTATION = 'ghostex-send-annotation';
export const MANAGE_HTML_FRAME_SEND_ANNOTATION_RESULT = 'ghostex-send-annotation-result';
export const MANAGE_HTML_FRAME_ANNOTATE_STATE = 'ghostex-annotate-state';
export const MANAGE_HTML_FRAME_ANNOTATE_NOTES = 'ghostex-annotate-notes';
export const MANAGE_HTML_FRAME_ANNOTATE_COPY = 'ghostex-annotate-copy';
export const MANAGE_HTML_FRAME_ANNOTATE_HOTKEY = 'ghostex-annotate-hotkey';
/** Files to the rendered document: a header command for its Agentation. */
export const MANAGE_HTML_FRAME_ANNOTATE_COMMAND = 'ghostex-annotate-command';
const MANAGE_HTML_FRAME_MAX_SCREENSHOTS = 50;
const MANAGE_HTML_FRAME_MAX_NOTES = 500;

export type ManageHtmlFrameMessage =
  | { type: typeof MANAGE_HTML_FRAME_OPEN_DOCUMENT; href: string }
  | {
      type: typeof MANAGE_HTML_FRAME_SEND_ANNOTATION;
      id: string;
      markdown: string;
      screenshots: { id: string; png: string }[];
    }
  | { type: typeof MANAGE_HTML_FRAME_ANNOTATE_STATE; state: Record<string, unknown> }
  | { type: typeof MANAGE_HTML_FRAME_ANNOTATE_NOTES; notes: unknown[] }
  | { type: typeof MANAGE_HTML_FRAME_ANNOTATE_COPY; markdown: string }
  | { type: typeof MANAGE_HTML_FRAME_ANNOTATE_HOTKEY };

const MANAGE_ANNOTATE_STATE_FLAGS = [
  'active',
  'frozen',
  'layout',
  'marksVisible',
  'canCopy',
  'canSend',
  'canClear',
] as const;
const MANAGE_ANNOTATE_SEND_STATES = ['idle', 'sending', 'sent', 'failed'];

/** One of the messages the rendered document may send, checked field by field; `undefined` for anything else. */
export function parseManageHtmlFrameMessage(data: unknown): ManageHtmlFrameMessage | undefined {
  if (!data || typeof data !== 'object') {
    return undefined;
  }
  const message = data as Record<string, unknown>;
  if (message.type === MANAGE_HTML_FRAME_OPEN_DOCUMENT) {
    return typeof message.href === 'string' && message.href.length <= 4096
      ? { type: MANAGE_HTML_FRAME_OPEN_DOCUMENT, href: message.href }
      : undefined;
  }
  if (message.type === MANAGE_HTML_FRAME_ANNOTATE_STATE) {
    const state = message.state as Record<string, unknown> | null;
    if (!state || typeof state !== 'object') {
      return undefined;
    }
    const checked: Record<string, unknown> = {};
    for (const flag of MANAGE_ANNOTATE_STATE_FLAGS) {
      checked[flag] = state[flag] === true;
    }
    checked.notes =
      typeof state.notes === 'number' && Number.isFinite(state.notes) ? Math.max(0, Math.floor(state.notes)) : 0;
    checked.sendState = MANAGE_ANNOTATE_SEND_STATES.includes(state.sendState as string) ? state.sendState : 'idle';
    return { type: MANAGE_HTML_FRAME_ANNOTATE_STATE, state: checked };
  }
  if (message.type === MANAGE_HTML_FRAME_ANNOTATE_NOTES) {
    return Array.isArray(message.notes) && message.notes.length <= MANAGE_HTML_FRAME_MAX_NOTES
      ? { type: MANAGE_HTML_FRAME_ANNOTATE_NOTES, notes: message.notes }
      : undefined;
  }
  if (message.type === MANAGE_HTML_FRAME_ANNOTATE_HOTKEY) {
    return { type: MANAGE_HTML_FRAME_ANNOTATE_HOTKEY };
  }
  if (message.type === MANAGE_HTML_FRAME_ANNOTATE_COPY) {
    return typeof message.markdown === 'string'
      ? { type: MANAGE_HTML_FRAME_ANNOTATE_COPY, markdown: message.markdown }
      : undefined;
  }
  if (message.type !== MANAGE_HTML_FRAME_SEND_ANNOTATION) {
    return undefined;
  }
  const { id, markdown, screenshots } = message;
  if (
    typeof id !== 'string' ||
    id.length > 200 ||
    typeof markdown !== 'string' ||
    !Array.isArray(screenshots) ||
    screenshots.length > MANAGE_HTML_FRAME_MAX_SCREENSHOTS
  ) {
    return undefined;
  }
  const checked: { id: string; png: string }[] = [];
  for (const screenshot of screenshots as unknown[]) {
    const entry = screenshot as Record<string, unknown> | null;
    if (!entry || typeof entry.id !== 'string' || typeof entry.png !== 'string') {
      return undefined;
    }
    checked.push({ id: entry.id, png: entry.png });
  }
  return { type: MANAGE_HTML_FRAME_SEND_ANNOTATION, id, markdown, screenshots: checked };
}

export function buildManageHtmlDocument(
  html: string,
  options: { annotate?: ManageAnnotateBoot; pageName?: string; resourceBaseUrl?: string } = {}
): string {
  /*
   * CDXC:Docs 2026-07-01-18:12:
   * Docs HTML files should behave like real interactive browser documents. Parse only to append Ghostex-owned viewer chrome and the Agentation bootstrap; do not remove authored scripts, inline handlers, JavaScript URLs, frames, form targets, srcdoc content, or base tags.
   */
  const documentValue = new DOMParser().parseFromString(html, 'text/html');
  injectManageHtmlResourceBase(documentValue, options.resourceBaseUrl);
  injectManageHtmlFrameHelper(documentValue);
  injectManageHtmlViewerChromeStyles(documentValue);
  if (options.annotate) {
    injectManageAgentationScript(documentValue, options.pageName, options.annotate);
  }
  return `${serializeManageDocumentType(documentValue)}\n${documentValue.documentElement.outerHTML}`;
}

/** The Files resource origin a rendered document loads its own folder's files from (`MANAGE_DOCS_RESOURCE_BASE_URL` in cef/shell/request_handling.rs). */
export const MANAGE_DOCS_RESOURCE_ORIGIN = 'https://ghostex-docs.invalid';

/**
 * Public library CDNs a rendered document may still load scripts, styles, fonts and pictures from. Keep in step with
 * `MANAGE_HTML_PAGE_LIBRARY_HOSTS` in apps/desktop/src/app/helpers/manage_docs_resources.rs, which the app enforces.
 */
export const MANAGE_HTML_LIBRARY_HOSTS = [
  'cdn.jsdelivr.net',
  'unpkg.com',
  'cdnjs.cloudflare.com',
  'fonts.googleapis.com',
  'fonts.gstatic.com',
  'cdn.tailwindcss.com',
  'd3js.org',
  'cdn.plot.ly',
  'code.jquery.com',
] as const;

/**
 * CDXC:Docs 2026-11-14 DECISION:
 * User: an HTML page may read plain files in its own folder and below, read only, never dot-files or dot-folders, with network access blocked for the page so nothing it reads can leave. The app enforces it for every frame of the page (cef/shell/request_handling.rs); this policy is the same rule inside the document, and also covers WebSocket connections. Only the Files resource origin and a few public library CDNs, which never show a page's requests to anyone the page could name, are reachable.
 */
export function manageHtmlContentSecurityPolicy(): string {
  const libraries = MANAGE_HTML_LIBRARY_HOSTS.map((host) => `https://${host}`).join(' ');
  const files = MANAGE_DOCS_RESOURCE_ORIGIN;
  return [
    "default-src 'none'",
    `script-src ${files} ${libraries} 'unsafe-inline' 'unsafe-eval' blob: data:`,
    `style-src ${files} ${libraries} 'unsafe-inline' blob: data:`,
    `img-src ${files} ${libraries} blob: data:`,
    `font-src ${files} ${libraries} blob: data:`,
    `media-src ${files} blob: data:`,
    `connect-src ${files} ${libraries} blob: data:`,
    'worker-src blob:',
    `frame-src ${files} blob:`,
    "object-src 'none'",
    "form-action 'none'",
    "manifest-src 'none'",
  ].join('; ');
}

/**
 * CDXC:Docs 2026-11-14 WHY:
 * Files used to reach into the rendered document from outside (its scrollbars, its `--foreground`, fragment links and links to other HTML files, Annotate's Send). The document has its own sealed origin now, so that work runs inside it: this script goes first in the document, before any authored script. Pages that keep settings in `localStorage` or `sessionStorage` get an in-memory copy, because a sealed origin has no storage and reading it throws; their settings last until the page reloads.
 */
export function buildManageHtmlFrameHelperScript(): string {
  return `
(() => {
  "use strict";
  const memoryStorage = () => {
    const values = new Map();
    return {
      get length() { return values.size; },
      key: (index) => Array.from(values.keys())[index] ?? null,
      getItem: (key) => (values.has(String(key)) ? values.get(String(key)) : null),
      setItem: (key, value) => { values.set(String(key), String(value)); },
      removeItem: (key) => { values.delete(String(key)); },
      clear: () => { values.clear(); },
    };
  };
  for (const name of ["localStorage", "sessionStorage"]) {
    let usable = false;
    try { usable = Boolean(window[name]); } catch { usable = false; }
    if (!usable) {
      try { Object.defineProperty(window, name, { configurable: true, enumerable: true, value: memoryStorage() }); } catch {}
    }
  }
  const filesOrigin = ${JSON.stringify(MANAGE_DOCS_RESOURCE_ORIGIN)};
  document.addEventListener("click", (event) => {
    if (event.defaultPrevented || event.button !== 0 || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const anchor = event.target && event.target.closest ? event.target.closest("a[href]") : null;
    const href = anchor ? (anchor.getAttribute("href") || "").trim() : "";
    if (!anchor || !href || anchor.hasAttribute("download") || (anchor.target && anchor.target !== "_self")) return;
    if (href.startsWith("#")) {
      let id;
      try { id = decodeURIComponent(href.slice(1)); } catch { return; }
      const target = id ? document.getElementById(id) : document.documentElement;
      if (target) {
        event.preventDefault();
        target.scrollIntoView({ behavior: "smooth", block: "start" });
      }
      return;
    }
    let url;
    try { url = new URL(href, document.baseURI); } catch { return; }
    if (url.origin === filesOrigin && /\\.html?$/i.test(url.pathname)) {
      event.preventDefault();
      parent.postMessage({ type: ${JSON.stringify(MANAGE_HTML_FRAME_OPEN_DOCUMENT)}, href: url.href }, "*");
    }
  }, true);
  const installAppScrollbars = ${installAppScrollbars.toString()};
  const decorate = () => {
    try {
      document.documentElement.style.setProperty("--foreground", getComputedStyle(document.body || document.documentElement).color);
      installAppScrollbars(window);
    } catch (error) {
      console.warn("[Ghostex Docs] viewer chrome failed", error instanceof Error ? error.message : String(error));
    }
  };
  if (document.readyState === "complete") decorate();
  else window.addEventListener("load", decorate, { once: true });
})();
`.trim();
}

/** Puts the content policy and the helper script first in the document's head, ahead of everything the file wrote. */
export function injectManageHtmlFrameHelper(documentValue: Document): void {
  const head = documentValue.head || documentValue.documentElement;
  const script = documentValue.createElement('script');
  script.setAttribute('data-ghostex-manage-html-helper', 'true');
  // The srcdoc is serialized HTML, so nothing in the script may close its tag.
  script.textContent = buildManageHtmlFrameHelperScript().replace(/<\/script/giu, '<\\/script');
  head.prepend(script);
  const policy = documentValue.createElement('meta');
  policy.setAttribute('http-equiv', 'Content-Security-Policy');
  policy.setAttribute('content', manageHtmlContentSecurityPolicy());
  head.prepend(policy);
}

export function manageHtmlResourceBaseUrl(documentPath: string): string | undefined {
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
  if (baseUrl.protocol !== 'https:' || baseUrl.hostname !== 'ghostex-docs.invalid' || baseUrl.pathname !== '/') {
    return undefined;
  }
  const components = documentPath.split('/');
  if (components.length < 2 || components.some((component) => !component || component === '.' || component === '..')) {
    return undefined;
  }
  const parentPath = components.slice(0, -1).map(encodeURIComponent).join('/');
  return new URL(`${parentPath}/`, baseUrl).toString();
}

export function decodeManageHtmlFragment(href: string): string | undefined {
  try {
    return decodeURIComponent(href.slice(1));
  } catch {
    return undefined;
  }
}

export function manageHtmlLinkedDocumentPath(href: string, resourceBaseUrl: string | undefined): string | undefined {
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
  const encodedComponents = linkedUrl.pathname.split('/').filter(Boolean);
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
    components.some((component) => !component || component === '.' || component === '..' || component.includes('\\'))
  ) {
    return undefined;
  }
  const path = components.join('/');
  return /\.html?$/iu.test(path) ? path : undefined;
}

export function injectManageHtmlResourceBase(documentValue: Document, resourceBaseUrl: string | undefined): void {
  if (!resourceBaseUrl) {
    return;
  }
  const authoredBase = documentValue.querySelector('base[href]');
  if (authoredBase) {
    const href = authoredBase.getAttribute('href');
    if (href) {
      try {
        authoredBase.setAttribute('href', new URL(href, resourceBaseUrl).toString());
      } catch {
        // Leave malformed authored base URLs unchanged so the browser reports them normally.
      }
    }
    return;
  }
  const base = documentValue.createElement('base');
  base.setAttribute('data-ghostex-manage-resource-base', 'true');
  base.href = resourceBaseUrl;
  documentValue.head.prepend(base);
}

export function injectManageHtmlViewerChromeStyles(documentValue: Document): void {
  /*
   * CDXC:Docs 2026-06-30-04:57:
   * The rendered artifact document owns its page CSS, but Docs owns the embedded scrollbar chrome. Append the style after author CSS so the iframe never shows wide default scrollbars or an opaque track/corner behind them.
   *
   * CDXC:Docs 2026-06-30-11:58:
   * The rendered document installs the same floating scrollbar controller as the app, using its own text color. Native scrollbar geometry would reserve a gutter.
   */
  documentValue.documentElement.setAttribute('data-ghostex-manage-html-viewer', 'true');
  const style = documentValue.createElement('style');
  style.setAttribute('data-ghostex-manage-html-chrome', 'true');
  style.textContent = scrollbarStyles;
  (documentValue.head || documentValue.documentElement).appendChild(style);
}

/**
 * CDXC:Docs 2026-10-27 WHY: Annotate's copied feedback in Files (issue #329) names the file, `## Page Feedback: docs/a.html`, instead of `srcdoc`. Files renders a document through a `srcDoc` iframe, whose location is `about:srcdoc`, and Agentation (3.1.2) builds that header from `window.location.pathname` at copy time with no prop to name the page (`appName` only adds an `**App:**` line), so the bootstrap renames it on the way to the clipboard. A file opened from outside the project (its path is absolute) is named by its file name alone, never an absolute or mount path. The Browser toolbar (`helpers/browser.rs`) and the phone's viewer load real URLs, so their headers already name the page.
 */
export function manageHtmlPageName(path: string, resourcePath: string): string {
  const outsideFile = resourcePath !== path || /^(?:[a-z]:)?[\\/]/iu.test(path);
  return outsideFile ? (path.split(/[\\/]/u).pop() ?? path) : path;
}

/**
 * Runs inside the rendered document. Agentation writes its Copy and Send headers from the srcdoc location (`## Page Feedback: srcdoc`), so the bootstrap swaps in the page's name before handing the text to Files. Only that exact leading header is touched.
 */
export const MANAGE_PAGE_FEEDBACK_HEADER_SOURCE = `
function renamePageFeedbackHeader(text, pathname, pageName) {
  const header = "## Page Feedback: " + pathname;
  if (!pageName || typeof text !== "string" || !text.startsWith(header)) return text;
  const rest = text.slice(header.length);
  if (rest !== "" && !/^[\\r\\n?#]/.test(rest)) return text;
  return "## Page Feedback: " + pageName + rest;
}
`.trim();

export function injectManageAgentationScript(
  documentValue: Document,
  pageName?: string,
  annotate?: ManageAnnotateBoot
): void {
  const script = documentValue.createElement('script');
  script.type = 'module';
  // The srcdoc is serialized HTML, so nothing in the script may close its tag.
  script.textContent = `${MANAGE_AGENTATION_BUNDLE.replace(/<\/script/giu, '<\\/script')}\n;\n${buildManageAgentationBootstrapScript(pageName, annotate)}`;
  (documentValue.body || documentValue.documentElement).appendChild(script);
}

/** JSON for inline script source: nothing in it may close the script tag. */
function inlineScriptJson(value: unknown): string {
  return JSON.stringify(value ?? null).replace(/</gu, '\\u003c');
}

/**
 * CDXC:Docs 2026-11-14 DECISION:
 * User (Q34 to Q47, after the options pages under docs/2026-11-14/files-annotate-toolbar/): "i want annotate to not show the floating toolbar ui on the page, i want that part to be controlled from our app basically so we dont have 2 toggle buttons", "when i click annotate we add agentation in ACTIVE mode so user doesn't need to click on it again". Annotate starts off for every HTML file and stays on per file across restarts; Agentation is loaded into every HTML file hidden and idle, so Annotate switches it on and off without reloading the page; its floating toolbar is never drawn (the fork's `toolbar: "hidden"`), and its buttons are the Files header's (Pause, Layout, Hide marks, Copy, Send, Clear, Settings), driven through the controller the fork hands over; the marks, the hover outline, note popups and Layout's palette (top right) stay on the page. Turning Annotate off keeps the notes and hides their marks, and the notes are kept per file across reloads and restarts. This supersedes the 2026-09-24 default ("on html pages i want agentation injected") on the desktop, where Annotate used to start on with the toolbar open, and replaces the 2026-10-23 start-up click (issue #280): the fork mounts active (`defaultActive`).
 * SEE-ALSO: apps/desktop/src/app/native_docs/annotate.rs (the header, the saved state), .dependencies/agentation (branch ghostex: `toolbar`, `defaultActive`, `theme`, `onReady`, `onStateChange`, `onAnnotationsChange`).
 */
export function buildManageAgentationBootstrapScript(pageName?: string, annotate?: ManageAnnotateBoot): string {
  const boot = {
    on: annotate?.on === true,
    theme: annotate?.theme === 'light' ? 'light' : 'dark',
    settings: annotate?.settings ?? null,
    notes: Array.isArray(annotate?.notes) ? annotate.notes : null,
    hotkey: annotate?.hotkey ?? null,
  };
  return `
export const rootId = "ghostex-agentation-root";
export const directionStyleId = "ghostex-agentation-direction-style";
document.getElementById(rootId)?.remove();
document.getElementById(directionStyleId)?.remove();
// Agentation portals its visible UI into document.body, outside rootEl. Give
// that portal an explicit writing-mode boundary so authored RTL page styles
// cannot reverse Agentation's own controls.
export const directionStyle = document.createElement("style");
directionStyle.id = directionStyleId;
directionStyle.textContent = "[data-agentation-root][data-agentation-theme] { direction: ltr !important; text-align: left !important; }";
(document.head || document.documentElement).appendChild(directionStyle);
export const rootEl = document.createElement("div");
rootEl.id = rootId;
rootEl.setAttribute("data-agentation-html-root", "true");
rootEl.setAttribute("data-agentation-root", "true");
(document.body || document.documentElement).appendChild(rootEl);
const boot = ${inlineScriptJson(boot)};
try {
  // __ghostexAgentation comes from the bundled fork placed before this bootstrap.
  const lib = __ghostexAgentation;
  if (!lib?.createElement || !lib?.createRoot || !lib?.Agentation) {
    throw new Error("The bundled Agentation did not expose the expected mounting API.");
  }
  // Agentation reads its settings and this page's notes from storage once, on mount. The
  // sealed document's storage is an in-memory copy, so Files hands both in: the settings
  // Ghostex keeps for every HTML file, and the notes it kept for this one.
  const settingsKey = "feedback-toolbar-settings";
  let settings = null;
  try {
    settings = JSON.parse(localStorage.getItem(settingsKey) || "null");
  } catch {
    settings = null;
  }
  if (!settings || typeof settings !== "object") {
    settings = {};
  }
  // "Clear on copy/send" starts on, as in the Browser toolbar (apps/desktop/src/app/helpers/browser.rs).
  if (settings.ghostexDefaults !== 1) {
    settings.autoClearAfterCopy = true;
    settings.ghostexDefaults = 1;
  }
  if (boot.settings && typeof boot.settings === "object") {
    Object.assign(settings, boot.settings);
  }
  try {
    localStorage.setItem(settingsKey, JSON.stringify(settings));
    if (Array.isArray(boot.notes)) {
      localStorage.setItem("feedback-annotations-" + location.pathname, JSON.stringify(boot.notes));
    }
  } catch {
    // A sealed document keeps no storage; the helper script's in-memory copy normally answers.
  }
  const root = lib.createRoot(rootEl);
  globalThis.__GHOSTEX_AGENTATION__ = { container: rootEl, root };
  const pageName = ${inlineScriptJson(pageName ?? '')};
  ${MANAGE_PAGE_FEEDBACK_HEADER_SOURCE}
  const tell = (message) => parent.postMessage(message, "*");
  // Send hands the notes and their screenshots to Files, which delivers them to the agent;
  // screenshots are named in the markdown until the app has saved them. The document has its
  // own sealed origin, so it asks Files by message and waits for the answer.
  const screenshotPrefix = ${JSON.stringify(MANAGE_ANNOTATION_SCREENSHOT_PREFIX)};
  const sendToFiles = (markdown, screenshots) => new Promise((resolve, reject) => {
    const id = Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
    const finish = (error) => {
      window.removeEventListener("message", onAnswer);
      clearTimeout(timer);
      if (error) reject(new Error(error));
      else resolve();
    };
    const onAnswer = (event) => {
      const answer = event.data;
      if (event.source !== parent || !answer || answer.type !== ${JSON.stringify(MANAGE_HTML_FRAME_SEND_ANNOTATION_RESULT)} || answer.id !== id) return;
      finish(typeof answer.error === "string" ? answer.error : undefined);
    };
    const timer = setTimeout(() => finish("Files did not answer."), 30000);
    window.addEventListener("message", onAnswer);
    tell({ type: ${JSON.stringify(MANAGE_HTML_FRAME_SEND_ANNOTATION)}, id, markdown, screenshots });
  });
  // The Files header's buttons, through the controller the fork hands over once mounted.
  let controller = null;
  const waiting = [];
  const props = {
    toolbar: "hidden",
    defaultActive: boot.on,
    theme: boot.theme,
    copyToClipboard: false,
    screenshotReference: (annotation, index) => "[Image #" + index + "](" + screenshotPrefix + annotation.id + ")",
    // The app writes the clipboard: after a click in the header this page has no focus.
    onCopy: (output) => tell({ type: ${JSON.stringify(MANAGE_HTML_FRAME_ANNOTATE_COPY)}, markdown: renamePageFeedbackHeader(String(output ?? ""), location.pathname, pageName) }),
    onSubmit: (output, annotations) => {
      const named = renamePageFeedbackHeader(output, location.pathname, pageName);
      const screenshots = annotations
        .filter((annotation) => annotation.screenshot && output.includes(screenshotPrefix + annotation.id + ")"))
        .map((annotation) => ({ id: String(annotation.id), png: annotation.screenshot }));
      return sendToFiles(named, screenshots);
    },
    onStateChange: (state) => tell({ type: ${JSON.stringify(MANAGE_HTML_FRAME_ANNOTATE_STATE)}, state }),
    onAnnotationsChange: (notes) => tell({ type: ${JSON.stringify(MANAGE_HTML_FRAME_ANNOTATE_NOTES)}, notes }),
    onReady: (ready) => {
      controller = ready;
      waiting.splice(0).forEach(run);
    },
  };
  const render = () => root.render(lib.createElement(lib.Agentation, props));
  const run = (message) => {
    if (message.command === "theme") {
      if (message.value === "dark" || message.value === "light") {
        props.theme = message.value;
        render();
      }
      return;
    }
    if (!controller) {
      waiting.push(message);
      return;
    }
    switch (message.command) {
      case "setActive": controller.setActive(message.value === true); break;
      case "toggleFreeze": controller.toggleFreeze(); break;
      case "toggleLayout": controller.toggleLayout(); break;
      case "setMarksVisible": controller.setMarksVisible(message.value === true); break;
      case "copy": void controller.copy(); break;
      case "send": void controller.send(); break;
      case "clearAll": controller.clearAll(); break;
      case "updateSettings":
        if (message.value && typeof message.value === "object") controller.updateSettings(message.value);
        break;
    }
  };
  window.addEventListener("message", (event) => {
    const message = event.data;
    if (event.source !== parent || !message || message.type !== ${JSON.stringify(MANAGE_HTML_FRAME_ANNOTATE_COMMAND)} || typeof message.command !== "string") return;
    run(message);
  });
  // The Annotate hotkey (rebindable in Settings > Hotkeys): this page has the keyboard while it
  // is focused, so it hands the chord to Files itself, ahead of the page's own listeners.
  const hotkey = boot.hotkey;
  if (hotkey) {
    window.addEventListener("keydown", (event) => {
      const keyMatches = hotkey.code ? event.code === hotkey.code : String(event.key).toLowerCase() === hotkey.key;
      if (!keyMatches || event.repeat || event.ctrlKey !== hotkey.ctrl || event.altKey !== hotkey.alt
        || event.shiftKey !== hotkey.shift || event.metaKey !== hotkey.meta) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      tell({ type: ${JSON.stringify(MANAGE_HTML_FRAME_ANNOTATE_HOTKEY)} });
    }, true);
  }
  render();
} catch (error) {
  console.warn("[Ghostex Docs Agentation] page injection failed", {
    message: error instanceof Error ? error.message : String(error)
  });
  rootEl.remove();
  directionStyle.remove();
}
`.trim();
}

export function serializeManageDocumentType(documentValue: Document): string {
  const doctype = documentValue.doctype;
  if (!doctype) {
    return '<!doctype html>';
  }
  const publicId = doctype.publicId ? ` PUBLIC "${doctype.publicId}"` : '';
  const systemId = doctype.systemId ? `${publicId ? '' : ' SYSTEM'} "${doctype.systemId}"` : '';
  return `<!doctype ${doctype.name}${publicId}${systemId}>`;
}
