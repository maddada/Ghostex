import { useMemo } from 'react';
import { ManageWebKitWindow } from '../types';
import { isHtmlPath } from '../file-tree-utils';

export const MANAGE_AGENTATION_VERSION = '3.0.2';
export const MANAGE_AGENTATION_REACT_VERSION = '18.2.0';
export const MANAGE_AGENTATION_PACKAGE_URL = `https://esm.sh/agentation@${MANAGE_AGENTATION_VERSION}?bundle&deps=react@${MANAGE_AGENTATION_REACT_VERSION},react-dom@${MANAGE_AGENTATION_REACT_VERSION}`;
export const MANAGE_AGENTATION_REACT_URL = `https://esm.sh/react@${MANAGE_AGENTATION_REACT_VERSION}`;
export const MANAGE_AGENTATION_REACT_DOM_CLIENT_URL = `https://esm.sh/react-dom@${MANAGE_AGENTATION_REACT_VERSION}/client?deps=react@${MANAGE_AGENTATION_REACT_VERSION}`;

export function ManageHtmlRenderViewer({
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
    [annotationsEnabled, content, resourceBaseUrl]
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
      aria-label='Rendered HTML document'
      className='manage-html-render-view'
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
  options: { injectAgentation?: boolean; resourceBaseUrl?: string } = {}
): string {
  /*
   * CDXC:ManageHtmlRendering 2026-07-01-18:12:
   * Docs HTML files should behave like real interactive browser documents. Parse only to append Ghostex-owned viewer chrome and the optional Agentation bootstrap; do not remove authored scripts, inline handlers, JavaScript URLs, frames, form targets, srcdoc content, or base tags.
   */
  const documentValue = new DOMParser().parseFromString(html, 'text/html');
  injectManageHtmlResourceBase(documentValue, options.resourceBaseUrl);
  injectManageHtmlViewerChromeStyles(documentValue);
  if (options.injectAgentation) {
    injectManageAgentationScript(documentValue);
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
  return isHtmlPath(path) ? path : undefined;
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
   * CDXC:ManageHtmlRendering 2026-06-30-04:57:
   * The rendered artifact document owns its page CSS, but Docs owns the embedded scrollbar chrome. Append the style after author CSS so the iframe never shows wide default scrollbars or an opaque track/corner behind them.
   *
   * CDXC:ManageHtmlRendering 2026-06-30-11:58:
   * Use document tagging plus WebKit scrollbar pseudo-elements for exact 4px embedded scrollbars. Standards `scrollbar-width: thin` is intentionally avoided because it produced a wider rendered scrollbar than the Docs requirement.
   */
  documentValue.documentElement.setAttribute('data-ghostex-manage-html-viewer', 'true');
  const style = documentValue.createElement('style');
  style.setAttribute('data-ghostex-manage-html-chrome', 'true');
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

export function injectManageAgentationScript(documentValue: Document): void {
  const script = documentValue.createElement('script');
  script.type = 'module';
  script.textContent = buildManageAgentationBootstrapScript();
  (documentValue.body || documentValue.documentElement).appendChild(script);
}

export function buildManageAgentationBootstrapScript(): string {
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

export function serializeManageDocumentType(documentValue: Document): string {
  const doctype = documentValue.doctype;
  if (!doctype) {
    return '<!doctype html>';
  }
  const publicId = doctype.publicId ? ` PUBLIC "${doctype.publicId}"` : '';
  const systemId = doctype.systemId ? `${publicId ? '' : ' SYSTEM'} "${doctype.systemId}"` : '';
  return `<!doctype ${doctype.name}${publicId}${systemId}>`;
}
