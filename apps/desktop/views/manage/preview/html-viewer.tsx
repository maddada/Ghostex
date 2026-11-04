import { useEffect, useMemo, useRef } from 'react';
import { installAppScrollbars } from '@/packages/components/ui/app-scrollbars';
import scrollbarStyles from '@/packages/components/ui/app-scrollbars.css?inline';
import { ManageWebKitWindow } from '../types';

export const MANAGE_AGENTATION_VERSION = '3.1.2';
export const MANAGE_AGENTATION_REACT_VERSION = '18.2.0';
export const MANAGE_AGENTATION_PACKAGE_URL = `https://esm.sh/agentation@${MANAGE_AGENTATION_VERSION}?bundle&deps=react@${MANAGE_AGENTATION_REACT_VERSION},react-dom@${MANAGE_AGENTATION_REACT_VERSION}`;
export const MANAGE_AGENTATION_REACT_URL = `https://esm.sh/react@${MANAGE_AGENTATION_REACT_VERSION}`;
export const MANAGE_AGENTATION_REACT_DOM_CLIENT_URL = `https://esm.sh/react-dom@${MANAGE_AGENTATION_REACT_VERSION}/client?deps=react@${MANAGE_AGENTATION_REACT_VERSION}`;

export function ManageHtmlRenderViewer({
  annotationsEnabled,
  content,
  documentKey,
  onOpenDocument,
  pageName,
}: {
  annotationsEnabled: boolean;
  content: string;
  documentKey: string;
  onOpenDocument: (path: string) => void;
  /** What Annotate's copied feedback calls this page (see `manageHtmlPageName`). */
  pageName?: string;
}) {
  const scrollbars = useRef<(() => void) | undefined>(undefined);
  useEffect(() => () => scrollbars.current?.(), []);
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
   * CDXC:Docs 2026-08-08:
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
      aria-label='Rendered HTML document'
      className='manage-html-render-view'
      data-document-key={documentKey}
      onLoad={(event) => {
        /*
         * CDXC:Docs 2026-08-06:
         * The synthetic folder base that makes sibling assets work also changes
         * fragment-link resolution inside srcdoc. Keep fragments owned by the
         * rendered document, and hand sibling HTML files back to Docs so its
         * selected path, header, and preview remain synchronized.
         */
        const renderedDocument = event.currentTarget.contentDocument;
        if (!renderedDocument) {
          return;
        }
        renderedDocument.documentElement.style.setProperty(
          '--foreground',
          renderedDocument.defaultView!.getComputedStyle(renderedDocument.body).color
        );
        scrollbars.current?.();
        scrollbars.current = installAppScrollbars(renderedDocument.defaultView as Window & typeof globalThis);
        renderedDocument.addEventListener(
          'click',
          (clickEvent) => {
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
            const anchor = eventTarget?.closest?.('a[href]') as HTMLAnchorElement | null;
            const href = anchor?.getAttribute('href')?.trim();
            if (!anchor || !href || anchor.hasAttribute('download') || (anchor.target && anchor.target !== '_self')) {
              return;
            }
            if (href.startsWith('#')) {
              const targetId = decodeManageHtmlFragment(href);
              const target = targetId ? renderedDocument.getElementById(targetId) : renderedDocument.documentElement;
              if (target) {
                clickEvent.preventDefault();
                target.scrollIntoView({ behavior: 'smooth', block: 'start' });
              }
              return;
            }
            const linkedDocumentPath = manageHtmlLinkedDocumentPath(href, resourceBaseUrl);
            if (!linkedDocumentPath || linkedDocumentPath === documentKey) {
              return;
            }
            clickEvent.preventDefault();
            onOpenDocument(linkedDocumentPath);
          },
          true
        );
      }}
      sandbox='allow-downloads allow-forms allow-modals allow-popups allow-popups-to-escape-sandbox allow-presentation allow-same-origin allow-scripts'
      srcDoc={renderedHtml}
      title={documentKey}
    />
  );
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
  injectManageHtmlViewerChromeStyles(documentValue);
  if (options.injectAgentation) {
    injectManageAgentationScript(documentValue, options.pageName);
  }
  return `${serializeManageDocumentType(documentValue)}\n${documentValue.documentElement.outerHTML}`;
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
  script.textContent = buildManageAgentationBootstrapScript(pageName);
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
    localStorage.setItem(settingsKey, JSON.stringify(settings));
  }
  const root = ReactDOMClient.createRoot(rootEl);
  globalThis.__GHOSTEX_AGENTATION__ = { container: rootEl, root };
  const pageName = ${JSON.stringify(pageName ?? '').replace(/</gu, '\\u003c')};
  if (pageName && navigator.clipboard?.writeText) {
    ${MANAGE_PAGE_FEEDBACK_HEADER_SOURCE}
    const clipboard = navigator.clipboard;
    const writeText = clipboard.writeText.bind(clipboard);
    clipboard.writeText = (text) => writeText(renamePageFeedbackHeader(text, location.pathname, pageName));
  }
  root.render(React.createElement(Agentation));
  startManageAgentationFeedbackMode();
}).catch((error) => {
  console.warn("[Ghostex Docs Agentation] page injection failed", {
    message: error instanceof Error ? error.message : String(error)
  });
  rootEl.remove();
  directionStyle.remove();
});
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
