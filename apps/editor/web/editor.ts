import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { SessionChatLexicalInput } from '@/packages/core-ui/chat/session-chat-lexical-input';
import type { ComposerEditorControls } from '@/packages/core-ui/chat/session-chat-lexical/commands';
import { trimPromptEditorTrailingSpaces } from '@/packages/shared/prompt-editor-text';

type GhostexEditorConfigureMessage = {
  type: 'configure';
  initialText?: unknown;
  cursorOffset?: unknown;
  language?: unknown;
  filePath?: unknown;
  title?: unknown;
};

type GhostexEditorHostMessage =
  | { type: 'ready' }
  | { type: 'configured' }
  | { type: 'draftUpdate'; text: string; cursorOffset: number }
  | { type: 'cursorUpdate'; cursorOffset: number }
  | { type: 'saveAndClose'; text: string; cursorOffset: number }
  | { type: 'save'; text: string; cursorOffset: number }
  | { type: 'cancel'; text: string; cursorOffset: number }
  | {
      type: 'pasteImage';
      requestId: string;
      /*
       * The WKWebView host reads NSPasteboard directly, so the macOS message
       * carries only the requestId. The wry hosts (Linux/Windows) have no
       * native pasteboard reader and receive the DOM file as base64.
       */
      base64Data?: string;
      suggestedName?: string;
    }
  | { type: 'loadImagePreview'; requestId: string; path: string };

type ImagePasteResult = {
  type: 'imagePasteResult';
  requestId: string;
  path?: string;
  error?: string;
};

type ImagePreviewResult = {
  type: 'imagePreviewResult';
  requestId: string;
  path: string;
  dataUrl?: string;
  error?: string;
};

type ImagePreview = {
  endOffset: number;
  id: string;
  markdown: string;
  path: string;
  startOffset: number;
};

const hostWindow = window as Window & {
  ipc?: { postMessage(message: string): void };
  webkit?: {
    messageHandlers?: {
      ghostexEditorHost?: { postMessage(message: GhostexEditorHostMessage): void };
    };
  };
};

const pendingImagePasteRequests = new Map<string, (result: ImagePasteResult) => void>();

let editorInstance: ComposerEditorControls | null = null;
let currentCursorOffset = 0;
let editorGeneration = 0;
const editorRoot = createRoot(getRequiredElement('editor'));
let draftUpdateTimer: number | null = null;
let cursorUpdateTimer: number | null = null;
let pendingConfigureMessage: GhostexEditorConfigureMessage | null = null;

let imagePreviews: ImagePreview[] = [];
let openImagePreview: ImagePreview | null = null;
const imagePreviewDataUrls = new Map<string, string>();
const failedImagePreviewPaths = new Set<string>();
const pendingImagePreviewPaths = new Set<string>();
const pendingImagePreviewRequests = new Map<string, string>();

window.addEventListener('ghostex-editor-host-message', (event) => {
  const detail = (event as CustomEvent<unknown>).detail;
  if (isConfigureMessage(detail)) {
    if (!applyConfigureMessage(detail)) {
      pendingConfigureMessage = detail;
    }
    return;
  }

  if (isImagePreviewResult(detail)) {
    handleImagePreviewResult(detail);
    return;
  }

  if (!isImagePasteResult(detail)) {
    return;
  }

  const resolve = pendingImagePasteRequests.get(detail.requestId);
  if (!resolve) {
    return;
  }
  pendingImagePasteRequests.delete(detail.requestId);
  resolve(detail);
});

/**
 * CDXC:PromptEditor 2026-09-11 DECISION:
 * User: use the same editor as the chat composer instead of Monaco to reduce the standalone prompt editor's memory.
 * Mount only the shared Lexical input, including its commands and find/replace panels.
 */
function mountEditor(config?: GhostexEditorConfigureMessage): void {
  const initialValue = typeof config?.initialText === 'string' ? config.initialText.replace(/\r\n?/g, '\n') : '';
  const cursorOffset = normalizeCursorOffset(config?.cursorOffset) ?? initialValue.length;
  editorRoot.render(
    createElement(SessionChatLexicalInput, {
      key: ++editorGeneration,
      initialValue,
      placeholder: 'Write a prompt…',
      fillHeight: true,
      theme: 'dark',
      onKeyDown: () => {},
      onPasteData: handlePasteData,
      onChange: () => {
        scheduleDraftUpdate();
        updateImagePreviews();
      },
      onCaretChange: (caret: number) => {
        currentCursorOffset = caret;
        scheduleCursorUpdate();
      },
      registerApi: (api: ComposerEditorControls | null) => {
        editorInstance = api;
        if (!api) return;
        currentCursorOffset = clampOffset(cursorOffset, initialValue.length);
        api.setSelection(currentCursorOffset);
        updateImagePreviews();
        if (config) {
          postToHost({ type: 'configured' });
        } else {
          postToHost({ type: 'ready' });
          if (pendingConfigureMessage) {
            const pending = pendingConfigureMessage;
            pendingConfigureMessage = null;
            applyConfigureMessage(pending);
          }
        }
      },
    })
  );
}

function applyConfigureMessage(message: GhostexEditorConfigureMessage): boolean {
  if (!editorInstance) return false;
  clearDraftUpdateTimer();
  clearCursorUpdateTimer();
  resetImagePreviewState();
  pendingImagePasteRequests.clear();
  // A new editing session must not inherit the previous file's undo history.
  mountEditor(message);
  return true;
}

function editorShortcutHint(): string {
  return /mac/iu.test(navigator.platform || navigator.userAgent)
    ? 'F1 for commands - CMD + S or CTRL + G to Save'
    : 'F1 for commands - CTRL + S or CTRL + G to Save';
}

getRequiredElement('editor-hint').textContent = editorShortcutHint();
getRequiredElement('save-button').addEventListener('click', saveAndClose);
getRequiredElement('cancel-button').addEventListener('click', cancel);
document.addEventListener('keydown', handleDocumentKeyDown, true);
installImagePreviewPopupHandlers();
mountEditor();
window.addEventListener(
  'pagehide',
  () => {
    clearDraftUpdateTimer();
    clearCursorUpdateTimer();
    resetImagePreviewState();
    pendingImagePasteRequests.clear();
    editorRoot.unmount();
  },
  { once: true }
);

function getRequiredElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`Missing required element #${id}`);
  }
  return element;
}

function getCurrentText(): string {
  return editorInstance?.getValue() ?? '';
}

function getCurrentCursorOffset(): number {
  return clampOffset(currentCursorOffset, getCurrentText().length);
}

function saveAndClose(): void {
  postToHost({
    type: 'saveAndClose',
    text: getCurrentText(),
    cursorOffset: getCurrentCursorOffset(),
  });
}

function cancel(): void {
  postToHost({
    type: 'cancel',
    text: getCurrentText(),
    cursorOffset: getCurrentCursorOffset(),
  });
}

function handleDocumentKeyDown(event: KeyboardEvent): void {
  if (event.isComposing || event.keyCode === 229) return;
  const key = event.key.toLowerCase();
  if (key === 'escape' && openImagePreview && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey) {
    stopShortcutEvent(event);
    closeImagePreviewPopup();
    return;
  }
  if (isSaveShortcut(event, key)) {
    stopShortcutEvent(event);
    saveAndClose();
  }
}

function isSaveShortcut(event: KeyboardEvent, key: string): boolean {
  if (event.altKey || event.shiftKey) {
    return false;
  }
  if (key === 's') {
    return event.metaKey || event.ctrlKey;
  }
  return key === 'g' && event.ctrlKey && !event.metaKey;
}

function stopShortcutEvent(event: KeyboardEvent): void {
  event.preventDefault();
  event.stopPropagation();
}

function scheduleDraftUpdate(): void {
  if (draftUpdateTimer !== null) {
    window.clearTimeout(draftUpdateTimer);
  }
  draftUpdateTimer = window.setTimeout(() => {
    draftUpdateTimer = null;
    postToHost({
      type: 'draftUpdate',
      text: getCurrentText(),
      cursorOffset: getCurrentCursorOffset(),
    });
  }, 300);
}

function scheduleCursorUpdate(): void {
  if (cursorUpdateTimer !== null) {
    window.clearTimeout(cursorUpdateTimer);
  }
  cursorUpdateTimer = window.setTimeout(() => {
    cursorUpdateTimer = null;
    postToHost({ type: 'cursorUpdate', cursorOffset: getCurrentCursorOffset() });
  }, 120);
}

function clearDraftUpdateTimer(): void {
  if (draftUpdateTimer === null) {
    return;
  }
  window.clearTimeout(draftUpdateTimer);
  draftUpdateTimer = null;
}

function clearCursorUpdateTimer(): void {
  if (cursorUpdateTimer === null) {
    return;
  }
  window.clearTimeout(cursorUpdateTimer);
  cursorUpdateTimer = null;
}

function postToHost(message: GhostexEditorHostMessage): void {
  const webKitHost = hostWindow.webkit?.messageHandlers?.ghostexEditorHost;
  if (webKitHost) {
    webKitHost.postMessage(message);
    return;
  }

  hostWindow.ipc?.postMessage(JSON.stringify(message));
}

function handlePasteData(data: DataTransfer): boolean {
  if (!editorInstance) return false;
  const generation = editorGeneration;
  const insertResult = (result: ImagePasteResult) => {
    if (generation === editorGeneration) handleImagePasteResult(result);
  };
  if (!hasImagePastePayload(data)) {
    const pastedText = data.getData('text/plain');
    const trimmedText = trimPromptEditorTrailingSpaces(pastedText);
    if (trimmedText === pastedText) return false;
    insertTextAtCursor(trimmedText);
    return true;
  }
  if (hostWindow.webkit?.messageHandlers?.ghostexEditorHost) {
    requestImagePaste(createRequestId(), undefined, undefined).then(insertResult);
    return true;
  }
  const imageFile = firstImageClipboardItem(data)?.getAsFile();
  if (!imageFile) return false;
  const requestId = createRequestId();
  readFileAsDataUrl(imageFile)
    .then((dataUrl) =>
      requestImagePaste(requestId, dataUrl.split(',', 2)[1] ?? '', suggestedImageName(imageFile, requestId))
    )
    .then(insertResult)
    .catch((error) => console.error('Image paste failed', error));
  return true;
}

function handleImagePasteResult(result: ImagePasteResult): void {
  if (result.path) {
    insertImageMarkdown(result.path.trim());
  } else if (result.error) {
    console.error('Image paste failed', result.error);
  }
}

function insertImageMarkdown(imagePath: string): void {
  if (!editorInstance || !imagePath) {
    return;
  }
  insertTextAtCursor(`[Image #${getNextPromptEditorImageIndex(editorInstance.getValue())}](${imagePath})`);
}

function getNextPromptEditorImageIndex(text: string): number {
  const imageLabelPattern = /\[Image #(\d+)·?\]\(/g;
  let highestIndex = 0;
  for (const match of text.matchAll(imageLabelPattern)) {
    const index = Number.parseInt(match[1] ?? '', 10);
    if (Number.isFinite(index)) {
      highestIndex = Math.max(highestIndex, index);
    }
  }
  return highestIndex + 1;
}

function parsePromptEditorImagePreviews(text: string): ImagePreview[] {
  const markdownLinkPattern = /!?\[[^\]\n]*\]\(([^)\n]+)\)/g;
  const previews: ImagePreview[] = [];
  for (const match of text.matchAll(markdownLinkPattern)) {
    const markdown = match[0];
    const rawPath = match[1]?.trim() ?? '';
    const startOffset = match.index ?? 0;
    if (!isPromptEditorImagePath(rawPath)) {
      continue;
    }
    previews.push({
      endOffset: startOffset + markdown.length,
      id: `${startOffset}:${rawPath}:${markdown.length}`,
      markdown,
      path: rawPath,
      startOffset,
    });
  }
  return previews;
}

function isPromptEditorImagePath(path: string): boolean {
  const normalizedPath = path.split(/[?#]/u)[0].toLowerCase();
  return (
    normalizedPath.startsWith('~/.ghostex/i/') ||
    /\.(avif|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/iu.test(normalizedPath)
  );
}

function resetImagePreviewState(): void {
  imagePreviews = [];
  imagePreviewDataUrls.clear();
  failedImagePreviewPaths.clear();
  pendingImagePreviewPaths.clear();
  pendingImagePreviewRequests.clear();
  closeImagePreviewPopup();
  renderImagePreviewStrip();
}

function updateImagePreviews(): void {
  if (!editorInstance) {
    return;
  }
  imagePreviews = parsePromptEditorImagePreviews(editorInstance.getValue());
  if (openImagePreview && !imagePreviews.some((preview) => preview.id === openImagePreview?.id)) {
    closeImagePreviewPopup();
  }
  const paths = new Set(imagePreviews.map((preview) => preview.path));
  for (const path of imagePreviewDataUrls.keys()) {
    if (!paths.has(path)) imagePreviewDataUrls.delete(path);
  }
  for (const path of failedImagePreviewPaths) {
    if (!paths.has(path)) failedImagePreviewPaths.delete(path);
  }
  for (const [requestId, path] of pendingImagePreviewRequests) {
    if (!paths.has(path)) {
      pendingImagePreviewRequests.delete(requestId);
      pendingImagePreviewPaths.delete(path);
    }
  }
  requestMissingImagePreviews();
  renderImagePreviewStrip();
}

function requestMissingImagePreviews(): void {
  for (const preview of imagePreviews) {
    if (
      imagePreviewDataUrls.has(preview.path) ||
      failedImagePreviewPaths.has(preview.path) ||
      pendingImagePreviewPaths.has(preview.path)
    ) {
      continue;
    }
    const requestId = createRequestId();
    pendingImagePreviewPaths.add(preview.path);
    pendingImagePreviewRequests.set(requestId, preview.path);
    postToHost({ type: 'loadImagePreview', requestId, path: preview.path });
  }
}

function handleImagePreviewResult(result: ImagePreviewResult): void {
  const requestedPath = pendingImagePreviewRequests.get(result.requestId);
  if (!requestedPath) {
    return;
  }
  pendingImagePreviewRequests.delete(result.requestId);
  pendingImagePreviewPaths.delete(requestedPath);
  if (result.dataUrl) {
    imagePreviewDataUrls.set(requestedPath, result.dataUrl);
  } else {
    failedImagePreviewPaths.add(requestedPath);
    if (result.error) {
      console.error('Image preview load failed', requestedPath, result.error);
    }
  }
  renderImagePreviewStrip();
}

function renderImagePreviewStrip(): void {
  const strip = getRequiredElement('image-strip');
  strip.replaceChildren();
  if (imagePreviews.length === 0) {
    strip.hidden = true;
    return;
  }
  strip.hidden = false;

  for (const preview of imagePreviews) {
    const thumb = document.createElement('div');
    thumb.className = 'editor-image-thumb';
    thumb.title = preview.path;

    /*
     * The entire visible image thumbnail should open the preview. Keep
     * removal as a separate small button above this full-area button so only
     * the explicit remove control is excluded.
     */
    const openButton = document.createElement('button');
    openButton.type = 'button';
    openButton.className = 'editor-image-open';
    openButton.setAttribute('aria-label', `Open image preview ${preview.path}`);
    const dataUrl = imagePreviewDataUrls.get(preview.path);
    if (dataUrl) {
      const image = document.createElement('img');
      image.alt = '';
      image.loading = 'lazy';
      image.decoding = 'async';
      image.src = dataUrl;
      openButton.appendChild(image);
    } else {
      const placeholder = document.createElement('span');
      placeholder.setAttribute('aria-hidden', 'true');
      openButton.appendChild(placeholder);
    }
    openButton.addEventListener('click', () => {
      openImagePreviewPopup(preview);
    });
    thumb.appendChild(openButton);

    const removeButton = document.createElement('button');
    removeButton.type = 'button';
    removeButton.className = 'editor-image-remove';
    removeButton.setAttribute('aria-label', `Remove image ${preview.path}`);
    removeButton.textContent = '✕';
    removeButton.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      removeImagePreview(preview);
    });
    thumb.appendChild(removeButton);

    strip.appendChild(thumb);
  }
}

function installImagePreviewPopupHandlers(): void {
  const popup = getRequiredElement('image-popup');
  const popupImage = getRequiredElement('image-popup-image');
  const closeButton = getRequiredElement('image-popup-close');
  /*
   * The dimmed image-preview backdrop is part of the preview dismissal
   * target. Close on direct backdrop pointer-down while keeping the image
   * itself clickable as its own dismissal affordance.
   */
  popup.addEventListener('pointerdown', (event) => {
    if (event.target === popup) {
      closeImagePreviewPopup();
    }
  });
  popupImage.addEventListener('click', () => {
    closeImagePreviewPopup();
  });
  closeButton.addEventListener('click', () => {
    closeImagePreviewPopup();
  });
}

function openImagePreviewPopup(preview: ImagePreview): void {
  const dataUrl = imagePreviewDataUrls.get(preview.path);
  if (!dataUrl) {
    return;
  }
  openImagePreview = preview;
  const popupImage = getRequiredElement('image-popup-image') as HTMLImageElement;
  popupImage.src = dataUrl;
  getRequiredElement('image-popup').hidden = false;
}

function closeImagePreviewPopup(): void {
  openImagePreview = null;
  const popup = document.getElementById('image-popup');
  if (popup) {
    popup.hidden = true;
    document.getElementById('image-popup-image')?.removeAttribute('src');
  }
}

function removeImagePreview(preview: ImagePreview): void {
  if (!editorInstance) {
    return;
  }

  const currentText = editorInstance.getValue();
  let startOffset =
    currentText.slice(preview.startOffset, preview.endOffset) === preview.markdown
      ? preview.startOffset
      : currentText.indexOf(preview.markdown);
  if (startOffset < 0) {
    return;
  }
  let endOffset = startOffset + preview.markdown.length;
  if (currentText[startOffset - 1] === '\n' && currentText[endOffset] === '\n') {
    endOffset += 1;
  } else if (currentText[endOffset] === '\n') {
    endOffset += 1;
  } else if (currentText[startOffset - 1] === '\n') {
    startOffset -= 1;
  }
  editorInstance.setSelection(startOffset, endOffset);
  editorInstance.insertText('');
  editorInstance.focus();
  if (openImagePreview?.id === preview.id) {
    closeImagePreviewPopup();
  }
}

function hasImagePastePayload(clipboardData: DataTransfer): boolean {
  const files = Array.from(clipboardData.files);
  if (
    files.some((file) => {
      const type = file.type.toLowerCase();
      return type.startsWith('image/') || /\.(avif|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/iu.test(file.name);
    })
  ) {
    return true;
  }

  const items = Array.from(clipboardData.items);
  if (items.some((item) => item.kind === 'file' && item.type.toLowerCase().startsWith('image/'))) {
    return true;
  }

  const types = Array.from(clipboardData.types).map((type) => type.toLowerCase());
  if (
    types.some(
      (type) =>
        type === 'files' || type === 'public.file-url' || type.startsWith('image/') || type.startsWith('public.image')
    )
  ) {
    return true;
  }

  return (
    types.includes('text/uri-list') && clipboardData.getData('text/uri-list').trim().toLowerCase().startsWith('file:')
  );
}

function firstImageClipboardItem(dataTransfer: DataTransfer | null): DataTransferItem | null {
  if (!dataTransfer) {
    return null;
  }
  for (const item of dataTransfer.items) {
    if (item.kind === 'file' && item.type.startsWith('image/')) {
      return item;
    }
  }
  return null;
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('load', () => {
      if (typeof reader.result === 'string') {
        resolve(reader.result);
      } else {
        reject(new Error('Image data was not readable'));
      }
    });
    reader.addEventListener('error', () => {
      reject(reader.error ?? new Error('Image data read failed'));
    });
    reader.readAsDataURL(file);
  });
}

function requestImagePaste(
  requestId: string,
  base64Data: string | undefined,
  suggestedName: string | undefined
): Promise<ImagePasteResult> {
  return new Promise((resolve) => {
    pendingImagePasteRequests.set(requestId, resolve);
    postToHost(
      base64Data === undefined
        ? { type: 'pasteImage', requestId }
        : { type: 'pasteImage', requestId, base64Data, suggestedName }
    );
  });
}

function insertTextAtCursor(text: string): void {
  editorInstance?.insertText(text);
}

function suggestedImageName(file: File, requestId: string): string {
  const extension = file.name.split('.').pop() || extensionForMimeType(file.type);
  return `pasted-image-${requestId}.${extension}`;
}

function extensionForMimeType(mimeType: string): string {
  if (mimeType === 'image/jpeg') {
    return 'jpg';
  }
  if (mimeType === 'image/gif') {
    return 'gif';
  }
  if (mimeType === 'image/webp') {
    return 'webp';
  }
  return 'png';
}

function createRequestId(): string {
  if (globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function normalizeCursorOffset(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return null;
  }
  return Math.max(0, Math.floor(value));
}

function clampOffset(offset: number, length: number): number {
  if (!Number.isFinite(offset)) {
    return length;
  }
  return Math.min(Math.max(Math.floor(offset), 0), Math.max(length, 0));
}

function isConfigureMessage(value: unknown): value is GhostexEditorConfigureMessage {
  if (!value || typeof value !== 'object') {
    return false;
  }
  return (value as Record<string, unknown>).type === 'configure';
}

function isImagePasteResult(value: unknown): value is ImagePasteResult {
  if (!value || typeof value !== 'object') {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    record.type === 'imagePasteResult' &&
    typeof record.requestId === 'string' &&
    (record.path === undefined || typeof record.path === 'string') &&
    (record.error === undefined || typeof record.error === 'string')
  );
}

function isImagePreviewResult(value: unknown): value is ImagePreviewResult {
  if (!value || typeof value !== 'object') {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    record.type === 'imagePreviewResult' &&
    typeof record.requestId === 'string' &&
    typeof record.path === 'string' &&
    (record.dataUrl === undefined || typeof record.dataUrl === 'string') &&
    (record.error === undefined || typeof record.error === 'string')
  );
}

export {};
