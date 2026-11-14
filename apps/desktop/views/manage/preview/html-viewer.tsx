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

export function ManageHtmlRenderViewer({
  annotationsEnabled,
  content,
  documentKey,
  onOpenDocument,
  onSendAnnotation,
  pageName,
}: {
  annotationsEnabled: boolean;
  content: string;
  documentKey: string;
  onOpenDocument: (path: string) => void;
  /** Annotate's Send: delivers the notes and their screenshots to the agent (see CDXC:Browser 2026-11-07 in app/page_annotations.rs). */
  onSendAnnotation: ManageAnnotationSend;
  /** What Annotate's copied feedback calls this page (see `manageHtmlPageName`). */
  pageName?: string;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const resourceBaseUrl = manageHtmlResourceBaseUrl(documentKey);
  const renderedHtml = useMemo(
    () =>
      buildManageHtmlDocument(content, {
        injectAgentation: annotationsEnabled,
        pageName,
        resourceBaseUrl,
      }),
    [annotationsEnabled, content, pageName, resourceBaseUrl]
  );

  /*
   * CDXC:Docs 2026-11-14 DECISION:
   * The rendered file runs in its own sealed (opaque) origin, so its script cannot reach this page, the Files bridge or the app. It talks to Files only through `postMessage`, and only two messages are accepted, from this frame's own window: open another HTML file beside it, and Annotate's Send while Annotate is on for this file. Everything else is ignored.
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
      if (message.type === MANAGE_HTML_FRAME_OPEN_DOCUMENT) {
        const linkedDocumentPath = manageHtmlLinkedDocumentPath(message.href, resourceBaseUrl);
        if (linkedDocumentPath && linkedDocumentPath !== documentKey) {
          onOpenDocument(linkedDocumentPath);
        }
        return;
      }
      const reply = (error?: string) =>
        frameWindow.postMessage(
          { type: MANAGE_HTML_FRAME_SEND_ANNOTATION_RESULT, id: message.id, ...(error ? { error } : {}) },
          '*'
        );
      if (!annotationsEnabled) {
        reply('Annotate is off for this file.');
        return;
      }
      onSendAnnotation(message.markdown, message.screenshots).then(
        () => reply(),
        (reason: unknown) => reply(reason instanceof Error ? reason.message : String(reason))
      );
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [annotationsEnabled, documentKey, onOpenDocument, onSendAnnotation, resourceBaseUrl]);

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
   * Clipboard writes and fullscreen are granted with `*`: the document has an opaque origin now, which `'self'` (this page's origin) no longer matches, and Agentation's copy button writes to the clipboard. No `allow-same-origin`, and no `allow-popups-to-escape-sandbox`: see the decision above.
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
const MANAGE_HTML_FRAME_MAX_SCREENSHOTS = 50;

export type ManageHtmlFrameMessage =
  | { type: typeof MANAGE_HTML_FRAME_OPEN_DOCUMENT; href: string }
  | {
      type: typeof MANAGE_HTML_FRAME_SEND_ANNOTATION;
      id: string;
      markdown: string;
      screenshots: { id: string; png: string }[];
    };

/** One of the two messages the rendered document may send, checked field by field; `undefined` for anything else. */
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
  options: { injectAgentation?: boolean; pageName?: string; resourceBaseUrl?: string } = {}
): string {
  /*
   * CDXC:Docs 2026-07-01-18:12:
   * Docs HTML files should behave like real interactive browser documents. Parse only to append Ghostex-owned viewer chrome and the optional Agentation bootstrap; do not remove authored scripts, inline handlers, JavaScript URLs, frames, form targets, srcdoc content, or base tags.
   */
  const documentValue = new DOMParser().parseFromString(html, 'text/html');
  injectManageHtmlResourceBase(documentValue, options.resourceBaseUrl);
  injectManageHtmlFrameHelper(documentValue);
  injectManageHtmlViewerChromeStyles(documentValue);
  if (options.injectAgentation) {
    injectManageAgentationScript(documentValue, options.pageName);
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
 * Runs inside the rendered document. Agentation copies through `navigator.clipboard.writeText`, so the header it wrote from the srcdoc location (`## Page Feedback: srcdoc`) is swapped for the page's name on the way out. Only that exact leading header is touched, so text a page copies itself passes through unchanged.
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

export function injectManageAgentationScript(documentValue: Document, pageName?: string): void {
  const script = documentValue.createElement('script');
  script.type = 'module';
  // The srcdoc is serialized HTML, so nothing in the script may close its tag.
  script.textContent = `${MANAGE_AGENTATION_BUNDLE.replace(/<\/script/giu, '<\\/script')}\n;\n${buildManageAgentationBootstrapScript(pageName)}`;
  (documentValue.body || documentValue.documentElement).appendChild(script);
}

/**
 * CDXC:Docs 2026-10-23 DECISION: User (issue #280): turning on Annotate for an HTML file in Files goes straight into annotating mode and stays in it across the reloads Files does when the file changes on disk. agentation@3.1.2 mounts collapsed (`isActive` is component state that starts false, is never persisted, and has no prop or handle), so the bootstrap clicks Agentation's own launcher once it renders. It matches the launcher only while it reads `aria-expanded="false"` and `aria-label="Start feedback mode"` inside the open shadow root of its `<agentation-toolbar>` host (3.1.2 put the shortcut into the title, so the title is not matched, and a MutationObserver on the page cannot see into the shadow root, so it polls every 50ms for up to 5s), so it can never switch an active toolbar off; its Cmd/Ctrl+Shift+F shortcut was rejected because it toggles and the synthetic key would also reach the page's own listeners. Every srcdoc reload re-runs this bootstrap, which is what keeps it active.
 * SEE-ALSO: apps/desktop/src/app/helpers/browser.rs (`autoActivate`, the Browser toolbar's Annotate does the same).
 */
export function buildManageAgentationBootstrapScript(pageName?: string): string {
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
// Annotate means "annotate now": open Agentation's collapsed toolbar as soon
// as it renders (see CDXC:Docs 2026-10-23 in html-viewer.tsx).
const startManageAgentationFeedbackMode = () => {
  const startSelector = '[data-agentation-toolbar] button[aria-expanded="false"][aria-label="Start feedback mode"]';
  let attempts = 0;
  const tryStart = () => {
    const startButton = document.querySelector("agentation-toolbar")?.shadowRoot?.querySelector(startSelector);
    if (startButton) {
      startButton.click();
      return;
    }
    attempts += 1;
    if (attempts < 100) {
      setTimeout(tryStart, 50);
    }
  };
  tryStart();
};
try {
  // __ghostexAgentation comes from the bundled fork placed before this bootstrap.
  const lib = __ghostexAgentation;
  if (!lib?.createElement || !lib?.createRoot || !lib?.Agentation) {
    throw new Error("The bundled Agentation did not expose the expected mounting API.");
  }
  // Agentation reads its settings from this key once, on mount, and merges them
  // over its own defaults; it exposes no prop for them. Ghostex turns
  // "Clear on copy/send" on once per origin and keeps the stamp inside the
  // stored object, which Agentation preserves, so a later manual toggle stays
  // in force. Same default as the Browser toolbar injection in
  // apps/desktop/src/app/helpers/browser.rs.
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
  if (settings.ghostexDefaults !== 1) {
    settings.autoClearAfterCopy = true;
    settings.ghostexDefaults = 1;
    try {
      localStorage.setItem(settingsKey, JSON.stringify(settings));
    } catch {
      // A sealed document keeps no storage; the helper script's in-memory copy normally answers.
    }
  }
  const root = lib.createRoot(rootEl);
  globalThis.__GHOSTEX_AGENTATION__ = { container: rootEl, root };
  const pageName = ${JSON.stringify(pageName ?? '').replace(/</gu, '\\u003c')};
  ${MANAGE_PAGE_FEEDBACK_HEADER_SOURCE}
  if (pageName && navigator.clipboard?.writeText) {
    const clipboard = navigator.clipboard;
    const writeText = clipboard.writeText.bind(clipboard);
    clipboard.writeText = (text) => writeText(renamePageFeedbackHeader(text, location.pathname, pageName));
  }
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
    parent.postMessage({ type: ${JSON.stringify(MANAGE_HTML_FRAME_SEND_ANNOTATION)}, id, markdown, screenshots }, "*");
  });
  root.render(lib.createElement(lib.Agentation, {
    screenshotReference: (annotation, index) => "[Image #" + index + "](" + screenshotPrefix + annotation.id + ")",
    onSubmit: (output, annotations) => {
      const named = renamePageFeedbackHeader(output, location.pathname, pageName);
      const screenshots = annotations
        .filter((annotation) => annotation.screenshot && output.includes(screenshotPrefix + annotation.id + ")"))
        .map((annotation) => ({ id: String(annotation.id), png: annotation.screenshot }));
      return sendToFiles(named, screenshots);
    },
  }));
  startManageAgentationFeedbackMode();
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
