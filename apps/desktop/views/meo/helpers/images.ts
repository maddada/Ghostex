// @ts-nocheck
import { WidgetType } from '@codemirror/view';

const IMAGE_EXT_RE = /\.(?:avif|bmp|gif|ico|jpe?g|png|svg|tiff?|webp)(?:$|[?#])/i;

let imageSrcResolver: (url: string) => string | Promise<string | null | undefined> | null | undefined = (url) => url;
let vscodeApi: any = null;

const imageSrcCache = new Map<string, string>();
const pendingImageResolvers = new Map<string, ((value: string) => void)[]>();
const imageRequestById = new Map<string, string>();
let imageRequestCounter = 0;

let imageSaveRequestCounter = 0;
const pendingImageSaveRequests = new Map<
  string,
  {
    resolve: (value: { success: boolean; path?: string; error?: string }) => void;
  }
>();

const imageExtensionByMime: Record<string, string> = {
  'image/avif': 'avif',
  'image/bmp': 'bmp',
  'image/gif': 'gif',
  'image/icon': 'ico',
  'image/jpeg': 'jpg',
  'image/jpg': 'jpg',
  'image/png': 'png',
  'image/svg+xml': 'svg',
  'image/tiff': 'tiff',
  'image/webp': 'webp',
  'image/x-icon': 'ico',
};

export function initializeImageHandling(vscode: any): void {
  vscodeApi = vscode;
}

const isImmediateImageSrc = (url: string): boolean =>
  /^(?:https?:|data:|blob:|vscode-webview-resource:|vscode-resource:)/i.test(url);

const requestImageSrcResolution = (url: string): Promise<string> =>
  new Promise((resolve) => {
    const waiting = pendingImageResolvers.get(url);
    if (waiting) {
      waiting.push(resolve);
      return;
    }

    pendingImageResolvers.set(url, [resolve]);
    const requestId = `img-${imageRequestCounter++}`;
    imageRequestById.set(requestId, url);
    vscodeApi?.postMessage({ type: 'resolveImageSrc', requestId, url });
  });

export const settleImageSrcRequest = (requestId: string, resolvedUrl: string | undefined): void => {
  const rawUrl = imageRequestById.get(requestId);
  if (typeof rawUrl !== 'string') {
    return;
  }

  imageRequestById.delete(requestId);
  const finalUrl = resolvedUrl || rawUrl;
  imageSrcCache.set(rawUrl, finalUrl);
  const waiters = pendingImageResolvers.get(rawUrl) ?? [];
  pendingImageResolvers.delete(rawUrl);
  for (const resolve of waiters) {
    resolve(finalUrl);
  }
};

export const resolveImageSrc = (rawUrl: string | null | undefined): string | Promise<string> => {
  const url = (rawUrl ?? '').trim();
  if (!url || isImmediateImageSrc(url)) {
    return url;
  }
  const cached = imageSrcCache.get(url);
  if (typeof cached === 'string') {
    return cached;
  }
  return requestImageSrcResolution(url);
};

export const parseDataUrlMimeType = (dataUrl: string): string => {
  const match = /^data:([^;,]+)[;,]/i.exec(dataUrl);
  return match?.[1]?.toLowerCase() ?? '';
};

const fallbackImageExtensionFromMimeType = (mimeType: string): string => {
  const normalized = mimeType.trim().toLowerCase();
  if (!normalized.startsWith('image/')) {
    return '';
  }

  const subtype = normalized
    .slice('image/'.length)
    .replace(/\+xml$/, '')
    .replace(/^x-/, '');
  const sanitized = subtype.replace(/[^a-z0-9.+-]/g, '');
  return sanitized || '';
};

export const imageExtensionFromMimeType = (mimeType: string): string =>
  imageExtensionByMime[mimeType.trim().toLowerCase()] ?? fallbackImageExtensionFromMimeType(mimeType);

export const handleSavedImagePath = (message: {
  requestId: string;
  success?: boolean;
  path?: string;
  error?: string;
}): void => {
  const pending = pendingImageSaveRequests.get(message.requestId);
  if (pending) {
    pendingImageSaveRequests.delete(message.requestId);
    if (message.success && message.path) {
      pending.resolve({ success: true, path: message.path });
    } else {
      pending.resolve({ success: false, error: message.error ?? 'Failed to save image' });
    }
  }
};

export interface ImagePasteContext {
  lineNumber: number;
  lineOffset: number;
}

export const handleImagePaste = async (
  event: ClipboardEvent,
  editor: any,
  context: ImagePasteContext
): Promise<boolean> => {
  const clipboardItems = event.clipboardData?.items;
  if (!clipboardItems) {
    return false;
  }

  for (const item of clipboardItems) {
    if (!item.type.startsWith('image/')) {
      continue;
    }

    event.preventDefault();
    event.stopPropagation();

    const blob = item.getAsFile();
    if (!blob) {
      continue;
    }

    const imageData = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ''));
      reader.onerror = () => reject(reader.error ?? new Error('Failed to read pasted image'));
      reader.readAsDataURL(blob);
    });

    if (!imageData) {
      return true;
    }

    const requestId = `img-save-${imageSaveRequestCounter++}`;
    const timestamp = Date.now();
    const dataUrlMimeType = parseDataUrlMimeType(imageData);
    const extension = imageExtensionFromMimeType(dataUrlMimeType) || imageExtensionFromMimeType(item.type) || 'png';
    const fileName = `${timestamp}.${extension}`;

    const promise = new Promise<{ success: boolean; path?: string; error?: string }>((resolve) => {
      pendingImageSaveRequests.set(requestId, { resolve });
    });

    vscodeApi?.postMessage({
      type: 'saveImageFromClipboard',
      requestId,
      imageData,
      fileName,
    });

    try {
      const result = await promise;
      if (result.success && result.path) {
        const imageMarkdown = `![${fileName}](${result.path})`;
        const currentState = editor.view.state;
        const targetLineNumber = Math.min(context.lineNumber, currentState.doc.lines);
        const targetLine = currentState.doc.line(targetLineNumber);
        const insertAt = Math.min(targetLine.to, targetLine.from + context.lineOffset);
        editor.view.dispatch({
          changes: { from: insertAt, to: insertAt, insert: imageMarkdown },
          selection: { anchor: insertAt + imageMarkdown.length },
        });
        editor.focus();
      }
    } catch {
      // Ignore errors - image paste failed silently
    }

    return true;
  }

  return false;
};

export function setImageSrcResolver(
  resolver: (url: string) => string | Promise<string | null | undefined> | null | undefined
): void {
  imageSrcResolver = typeof resolver === 'function' ? resolver : (url) => url;
}

export function isImageUrl(url: string | null | undefined): boolean {
  if (!url) {
    return false;
  }
  return IMAGE_EXT_RE.test(url);
}

export class ImageWidget extends WidgetType {
  url: string;
  altText: string;
  linkUrl: string;

  constructor(url: string | null | undefined, altText: string | null | undefined, linkUrl: string | null | undefined) {
    super();
    this.url = url?.trim() ?? '';
    this.altText = altText ?? '';
    this.linkUrl = linkUrl?.trim() ?? '';
  }

  eq(other: ImageWidget): boolean {
    return (
      other instanceof ImageWidget &&
      other.url === this.url &&
      other.altText === this.altText &&
      other.linkUrl === this.linkUrl
    );
  }

  toDOM(): HTMLElement {
    const container = document.createElement('div');
    container.className = 'meo-md-image';

    if (!this.url) {
      this.renderFallback(container);
      return container;
    }

    const img = document.createElement('img');
    img.className = 'meo-md-image-img';
    img.alt = this.altText;
    img.loading = 'lazy';

    let loadingPlaceholder: HTMLElement | undefined;
    const hideLoadingPlaceholder = () => {
      if (loadingPlaceholder && container.contains(loadingPlaceholder)) {
        container.removeChild(loadingPlaceholder);
      }
      img.classList.add('meo-md-image-loaded');
    };

    const resolveAndSetSrc = (src: string) => {
      img.src = src;
      if (img.complete) {
        hideLoadingPlaceholder();
      }
    };

    const fail = () => {
      hideLoadingPlaceholder();
      this.renderFallback(container);
    };

    img.addEventListener('load', hideLoadingPlaceholder);
    img.addEventListener('error', fail);

    loadingPlaceholder = document.createElement('div');
    loadingPlaceholder.className = 'meo-md-image-loading';
    loadingPlaceholder.textContent = 'Loading image...';

    container.appendChild(loadingPlaceholder);
    container.appendChild(img);
    this.setImageSource(resolveAndSetSrc, fail);

    if (this.linkUrl) {
      container.classList.add('meo-md-image-linked');
      container.setAttribute('data-meo-link-href', this.linkUrl);
    }

    return container;
  }

  renderFallback(container: HTMLElement): void {
    container.classList.add('meo-md-image-fallback');
    const fallback = document.createElement('code');
    fallback.className = 'meo-md-image-fallback-text';
    fallback.textContent = `![${this.altText}](${this.url})`;
    container.replaceChildren(fallback);
  }

  setImageSource(onSrc: (src: string) => void, onFail: () => void): void {
    const resolved = imageSrcResolver(this.url);
    if (isPromiseLike(resolved)) {
      resolved
        .then((value) => {
          if (!value || !document.contains(document.body)) {
            onFail();
            return;
          }
          onSrc(value);
        })
        .catch(onFail);
      return;
    }

    if (!resolved) {
      onFail();
      return;
    }

    onSrc(resolved);
  }

  ignoreEvent(event: Event): boolean {
    if (event.type.startsWith('pointer') || event.type.startsWith('mouse')) {
      return false;
    }
    return true;
  }
}

function isPromiseLike(value: unknown): value is Promise<unknown> {
  return Boolean(value) && typeof (value as any).then === 'function';
}

function findChildNode(node: any, name: string): any {
  for (let child = node.node.firstChild; child; child = child.nextSibling) {
    if (child.name === name) {
      return child;
    }
  }
  return null;
}

export function getImageData(state: any, node: any): { url: string; altText: string; linkUrl: string } {
  const urlNode = findChildNode(node, 'URL');
  const url = urlNode ? state.doc.sliceString(urlNode.from, urlNode.to).trim() : '';

  let altText = '';
  const imageText = state.doc.sliceString(node.from, node.to);
  const altMatch = /!\[([^\]]*)\]/.exec(imageText);
  if (altMatch) {
    altText = altMatch[1];
  }

  let linkUrl = '';
  const parentNode = node.node.parent;
  if (parentNode && parentNode.name === 'Link') {
    const linkUrlNode = findChildNode(parentNode, 'URL');
    if (linkUrlNode) {
      linkUrl = state.doc.sliceString(linkUrlNode.from, linkUrlNode.to).trim();
    }
  }

  return { url, altText, linkUrl };
}
