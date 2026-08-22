import { trimPromptEditorTrailingSpaces } from "../../../shared/prompt-editor-text";

type GhostexEditorConfigureMessage = {
  type: "configure";
  initialText?: unknown;
  cursorOffset?: unknown;
  language?: unknown;
  filePath?: unknown;
  title?: unknown;
};

type GhostexEditorHostMessage =
  | { type: "ready" }
  | { type: "configured" }
  | { type: "draftUpdate"; text: string; cursorOffset: number }
  | { type: "cursorUpdate"; cursorOffset: number }
  | { type: "saveAndClose"; text: string; cursorOffset: number }
  | { type: "save"; text: string; cursorOffset: number }
  | { type: "cancel"; text: string; cursorOffset: number }
  | {
      type: "pasteImage";
      requestId: string;
      /*
       * The WKWebView host reads NSPasteboard directly, so the macOS message
       * carries only the requestId. The wry hosts (Linux/Windows) have no
       * native pasteboard reader and receive the DOM file as base64.
       */
      base64Data?: string;
      suggestedName?: string;
    }
  | { type: "loadImagePreview"; requestId: string; path: string };

type ImagePasteResult = {
  type: "imagePasteResult";
  requestId: string;
  path?: string;
  error?: string;
};

type ImagePreviewResult = {
  type: "imagePreviewResult";
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

type MonacoEditor = {
  createContextKey<T>(key: string, defaultValue: T): { set(value: T): void };
  dispose(): void;
  executeEdits(
    source: string,
    edits: Array<{ range: unknown; text: string; forceMoveMarkers?: boolean }>,
  ): boolean;
  focus(): void;
  getModel(): MonacoModel | null;
  getPosition(): unknown;
  getSelection(): unknown;
  getValue(): string;
  onDidChangeModelContent(handler: () => void): { dispose(): void };
  onDidChangeCursorPosition(handler: () => void): { dispose(): void };
  pushUndoStop(): boolean;
  revealPositionInCenterIfOutsideViewport(position: unknown): void;
  setModel(model: MonacoModel | null): void;
  setPosition(position: unknown): void;
  updateOptions(options: Record<string, unknown>): void;
};

type MonacoPosition = {
  column: number;
  lineNumber: number;
};

type MonacoModel = {
  dispose(): void;
  getLanguageId(): string;
  getOffsetAt(position: unknown): number;
  getPositionAt(offset: number): MonacoPosition;
  getValue(): string;
  getValueLength(): number;
};

type MonacoApi = {
  Uri: {
    file(path: string): unknown;
    parse(value: string): unknown;
  };
  editor: {
    create(element: HTMLElement, options: Record<string, unknown>): MonacoEditor;
    createModel(value: string, language?: string, uri?: unknown): MonacoModel;
    setModelLanguage(model: MonacoModel, language: string): void;
  };
};

type MonacoAmdRequire = {
  (dependencies: string[], callback: () => void, errorback?: (error: unknown) => void): void;
  config(options: Record<string, unknown>): void;
};

declare global {
  interface Window {
    MonacoEnvironment?: {
      getWorkerUrl(_workerId: string, _label: string): string;
    };
    ipc?: {
      postMessage(message: string): void;
    };
    webkit?: {
      messageHandlers?: {
        ghostexEditorHost?: {
          postMessage(message: GhostexEditorHostMessage): void;
        };
      };
    };
  }

  const monaco: MonacoApi;
  const require: MonacoAmdRequire;
}

const pendingImagePasteRequests = new Map<string, (result: ImagePasteResult) => void>();

let editorInstance: MonacoEditor | null = null;
let draftUpdateTimer: ReturnType<typeof window.setTimeout> | null = null;
let cursorUpdateTimer: ReturnType<typeof window.setTimeout> | null = null;
let pendingConfigureMessage: GhostexEditorConfigureMessage | null = null;

let imagePreviews: ImagePreview[] = [];
let openImagePreview: ImagePreview | null = null;
const imagePreviewDataUrls = new Map<string, string>();
const failedImagePreviewPaths = new Set<string>();
const pendingImagePreviewPaths = new Set<string>();
const pendingImagePreviewRequests = new Map<string, string>();

window.MonacoEnvironment = {
  getWorkerUrl() {
    return "./monaco/vs/base/worker/workerMain.js";
  },
};

require.config({
  paths: {
    vs: "./monaco/vs",
  },
});

window.addEventListener("ghostex-editor-host-message", (event) => {
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

require(
  ["vs/editor/editor.main"],
  () => {
    getRequiredElement("editor-hint").textContent = editorShortcutHint();
    const editorElement = getRequiredElement("editor");
    const saveButton = getRequiredElement("save-button") as HTMLButtonElement;
    const cancelButton = getRequiredElement("cancel-button") as HTMLButtonElement;
    const model = createModelForConfig({
      filePath: "",
      initialText: "",
      language: "markdown",
    });

    editorInstance = monaco.editor.create(editorElement, {
      acceptSuggestionOnEnter: "off",
      automaticLayout: true,
      cursorBlinking: "smooth",
      fontFamily:
        "JetBrains Mono, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
      fontLigatures: true,
      fontSize: 14,
      lineNumbersMinChars: 3,
      minimap: {
        enabled: false,
      },
      model,
      occurrencesHighlight: "off",
      copyWithSyntaxHighlighting: false,
      padding: {
        bottom: 48,
        top: 12,
      },
      parameterHints: {
        enabled: false,
      },
      quickSuggestions: false,
      renderLineHighlight: "none",
      scrollBeyondLastLine: false,
      scrollbar: {
        horizontalScrollbarSize: 7,
        verticalScrollbarSize: 7,
      },
      selectionHighlight: false,
      snippetSuggestions: "none",
      suggestOnTriggerCharacters: false,
      tabCompletion: "off",
      theme: "vs-dark",
      wordBasedSuggestions: "off",
      wordWrap: "on",
    });

    editorInstance.onDidChangeModelContent(() => {
      scheduleDraftUpdate();
      updateImagePreviews();
    });
    editorInstance.onDidChangeCursorPosition(() => {
      scheduleCursorUpdate();
    });

    document.addEventListener("keydown", handleDocumentKeyDown, true);
    /*
     * The paste hook must live on window capture: Monaco's clipboard stack
     * halts capture descent below document, so a listener on the editor
     * container (or anywhere deeper) never fires and image pastes get
     * silently dropped by the textarea default handling.
     */
    window.addEventListener("paste", handlePaste, true);
    saveButton.addEventListener("click", () => {
      saveAndClose();
    });
    cancelButton.addEventListener("click", () => {
      cancel();
    });
    installImagePreviewPopupHandlers();

    postToHost({ type: "ready" });
    if (pendingConfigureMessage) {
      const configureMessage = pendingConfigureMessage;
      pendingConfigureMessage = null;
      applyConfigureMessage(configureMessage);
    } else {
      editorInstance.focus();
    }
  },
  (error) => {
    console.error("Failed to load Monaco editor", error);
  },
);

function normalizeConfigureMessage(rawMessage: GhostexEditorConfigureMessage) {
  const filePath =
    typeof rawMessage.filePath === "string" && rawMessage.filePath.length > 0
      ? rawMessage.filePath
      : "";
  return {
    filePath,
    initialText: typeof rawMessage.initialText === "string" ? rawMessage.initialText : "",
    cursorOffset: normalizeCursorOffset(rawMessage.cursorOffset),
    language:
      typeof rawMessage.language === "string" && rawMessage.language.length > 0
        ? rawMessage.language
        : null,
  };
}

function createModelForConfig(config: {
  filePath: string;
  initialText: string;
  language: string | null;
}): MonacoModel {
  const language = config.language || undefined;
  const model = monaco.editor.createModel(config.initialText, language, uriForFilePath(config.filePath));
  if (config.language) {
    monaco.editor.setModelLanguage(model, config.language);
  } else if (model.getLanguageId() === "plaintext") {
    monaco.editor.setModelLanguage(model, "markdown");
  }
  return model;
}

function applyConfigureMessage(message: GhostexEditorConfigureMessage): boolean {
  if (!editorInstance) {
    return false;
  }

  clearDraftUpdateTimer();
  clearCursorUpdateTimer();
  resetImagePreviewState();

  const config = normalizeConfigureMessage(message);
  const previousModel = editorInstance.getModel();
  if (previousModel) {
    editorInstance.setModel(null);
    previousModel.dispose();
  }

  const model = createModelForConfig(config);
  editorInstance.setModel(model);
  editorInstance.updateOptions({
    wordWrap: model.getLanguageId() === "markdown" ? "on" : "off",
  });
  moveCaretToOffset(model, config.cursorOffset ?? model.getValueLength());
  editorInstance.focus();
  updateImagePreviews();
  postToHost({ type: "configured" });

  return true;
}

/*
 * Cmd+S/Ctrl+S also save (see isSaveShortcut); the hint intentionally shows
 * only one shortcut per modifier to stay short.
 */
function editorShortcutHint(): string {
  const platform = navigator.platform || navigator.userAgent;
  return /mac/iu.test(platform)
    ? "F1 for commands - CMD + S or CTRL + G to Save"
    : "F1 for commands - CTRL + S or CTRL + G to Save";
}

function getRequiredElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`Missing required element #${id}`);
  }
  return element;
}

function uriForFilePath(filePath: string): unknown {
  if (filePath.length > 0) {
    return monaco.Uri.file(filePath);
  }
  return monaco.Uri.parse("inmemory://ghostex-editor/draft.md");
}

function getCurrentText(): string {
  return editorInstance?.getValue() ?? "";
}

function getCurrentCursorOffset(): number {
  const model = editorInstance?.getModel();
  const position = editorInstance?.getPosition();
  if (!model || !position) {
    return getCurrentText().length;
  }
  const offset = model.getOffsetAt(position);
  return clampOffset(offset, model.getValueLength());
}

function saveAndClose(): void {
  postToHost({
    type: "saveAndClose",
    text: getCurrentText(),
    cursorOffset: getCurrentCursorOffset(),
  });
}

function cancel(): void {
  postToHost({
    type: "cancel",
    text: getCurrentText(),
    cursorOffset: getCurrentCursorOffset(),
  });
}

function handleDocumentKeyDown(event: KeyboardEvent): void {
  const key = event.key.toLowerCase();
  if (
    key === "escape" &&
    openImagePreview &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.metaKey &&
    !event.shiftKey
  ) {
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
  if (key === "s") {
    return event.metaKey || event.ctrlKey;
  }
  return key === "g" && event.ctrlKey && !event.metaKey;
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
      type: "draftUpdate",
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
    postToHost({ type: "cursorUpdate", cursorOffset: getCurrentCursorOffset() });
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
  const webKitHost = window.webkit?.messageHandlers?.ghostexEditorHost;
  if (webKitHost) {
    webKitHost.postMessage(message);
    return;
  }

  window.ipc?.postMessage(JSON.stringify(message));
}

function handlePaste(event: ClipboardEvent): void {
  if (!editorInstance || event.defaultPrevented) {
    return;
  }

  if (!hasImagePastePayload(event)) {
    /*
     * Pasting plain text must not accumulate invisible trailing whitespace
     * from shell capture or clipboard content: strip spaces/tabs at line ends
     * and only take over the paste when that changes the text.
     */
    const pastedText = event.clipboardData?.getData("text/plain") ?? "";
    const trimmedText = trimPromptEditorTrailingSpaces(pastedText);
    if (!pastedText || trimmedText === pastedText) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    insertTextAtCursor(trimmedText);
    return;
  }

  if (window.webkit?.messageHandlers?.ghostexEditorHost) {
    // macOS host: native resolves the clipboard image (file or bitmap) itself.
    event.preventDefault();
    event.stopPropagation();
    requestImagePaste(createRequestId(), undefined, undefined)
      .then(handleImagePasteResult)
      .catch((error) => {
        console.error("Image paste failed", error);
      });
    return;
  }

  const imageFile = firstImageClipboardItem(event.clipboardData)?.getAsFile();
  if (!imageFile) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();

  const requestId = createRequestId();
  const suggestedName = suggestedImageName(imageFile, requestId);

  readFileAsDataUrl(imageFile)
    .then((dataUrl) => {
      const base64Data = dataUrl.split(",", 2)[1] ?? "";
      return requestImagePaste(requestId, base64Data, suggestedName);
    })
    .then(handleImagePasteResult)
    .catch((error) => {
      console.error("Image paste failed", error);
    });
}

function handleImagePasteResult(result: ImagePasteResult): void {
  if (result.path) {
    insertImageMarkdown(result.path.trim());
  } else if (result.error) {
    console.error("Image paste failed", result.error);
  }
}

function insertImageMarkdown(imagePath: string): void {
  if (!editorInstance || !imagePath) {
    return;
  }
  insertTextAtCursor(
    `[Image #${getNextPromptEditorImageIndex(editorInstance.getValue())}](${imagePath})`,
  );
}

function getNextPromptEditorImageIndex(text: string): number {
  const imageLabelPattern = /\[Image #(\d+)\]\(/g;
  let highestIndex = 0;
  for (const match of text.matchAll(imageLabelPattern)) {
    const index = Number.parseInt(match[1] ?? "", 10);
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
    const rawPath = match[1]?.trim() ?? "";
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
    normalizedPath.startsWith("~/.ghostex/i/") ||
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
    postToHost({ type: "loadImagePreview", requestId, path: preview.path });
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
      console.error("Image preview load failed", requestedPath, result.error);
    }
  }
  renderImagePreviewStrip();
}

function renderImagePreviewStrip(): void {
  const strip = getRequiredElement("image-strip");
  strip.replaceChildren();
  if (imagePreviews.length === 0) {
    strip.hidden = true;
    return;
  }
  strip.hidden = false;

  for (const preview of imagePreviews) {
    const thumb = document.createElement("div");
    thumb.className = "editor-image-thumb";
    thumb.title = preview.path;

    /*
     * The entire visible image thumbnail should open the preview. Keep
     * removal as a separate small button above this full-area button so only
     * the explicit remove control is excluded.
     */
    const openButton = document.createElement("button");
    openButton.type = "button";
    openButton.className = "editor-image-open";
    openButton.setAttribute("aria-label", `Open image preview ${preview.path}`);
    const dataUrl = imagePreviewDataUrls.get(preview.path);
    if (dataUrl) {
      const image = document.createElement("img");
      image.alt = "";
      image.src = dataUrl;
      openButton.appendChild(image);
    } else {
      const placeholder = document.createElement("span");
      placeholder.setAttribute("aria-hidden", "true");
      openButton.appendChild(placeholder);
    }
    openButton.addEventListener("click", () => {
      openImagePreviewPopup(preview);
    });
    thumb.appendChild(openButton);

    const removeButton = document.createElement("button");
    removeButton.type = "button";
    removeButton.className = "editor-image-remove";
    removeButton.setAttribute("aria-label", `Remove image ${preview.path}`);
    removeButton.textContent = "✕";
    removeButton.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      removeImagePreview(preview);
    });
    thumb.appendChild(removeButton);

    strip.appendChild(thumb);
  }
}

function installImagePreviewPopupHandlers(): void {
  const popup = getRequiredElement("image-popup");
  const popupImage = getRequiredElement("image-popup-image");
  const closeButton = getRequiredElement("image-popup-close");
  /*
   * The dimmed image-preview backdrop is part of the preview dismissal
   * target. Close on direct backdrop pointer-down while keeping the image
   * itself clickable as its own dismissal affordance.
   */
  popup.addEventListener("pointerdown", (event) => {
    if (event.target === popup) {
      closeImagePreviewPopup();
    }
  });
  popupImage.addEventListener("click", () => {
    closeImagePreviewPopup();
  });
  closeButton.addEventListener("click", () => {
    closeImagePreviewPopup();
  });
}

function openImagePreviewPopup(preview: ImagePreview): void {
  const dataUrl = imagePreviewDataUrls.get(preview.path);
  if (!dataUrl) {
    return;
  }
  openImagePreview = preview;
  const popupImage = getRequiredElement("image-popup-image") as HTMLImageElement;
  popupImage.src = dataUrl;
  getRequiredElement("image-popup").hidden = false;
}

function closeImagePreviewPopup(): void {
  openImagePreview = null;
  const popup = document.getElementById("image-popup");
  if (popup) {
    popup.hidden = true;
  }
}

function removeImagePreview(preview: ImagePreview): void {
  const model = editorInstance?.getModel();
  if (!editorInstance || !model) {
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
  if (currentText[startOffset - 1] === "\n" && currentText[endOffset] === "\n") {
    endOffset += 1;
  } else if (currentText[endOffset] === "\n") {
    endOffset += 1;
  } else if (currentText[startOffset - 1] === "\n") {
    startOffset -= 1;
  }
  const startPosition = model.getPositionAt(startOffset);
  const endPosition = model.getPositionAt(endOffset);
  editorInstance.pushUndoStop();
  editorInstance.executeEdits("ghostex-image-preview-remove", [
    {
      forceMoveMarkers: true,
      range: {
        endColumn: endPosition.column,
        endLineNumber: endPosition.lineNumber,
        startColumn: startPosition.column,
        startLineNumber: startPosition.lineNumber,
      },
      text: "",
    },
  ]);
  editorInstance.pushUndoStop();
  editorInstance.focus();
  if (openImagePreview?.id === preview.id) {
    closeImagePreviewPopup();
  }
}

function hasImagePastePayload(event: ClipboardEvent): boolean {
  const clipboardData = event.clipboardData;
  if (!clipboardData) {
    return false;
  }

  const files = Array.from(clipboardData.files);
  if (
    files.some((file) => {
      const type = file.type.toLowerCase();
      return (
        type.startsWith("image/") ||
        /\.(avif|gif|heic|heif|jpe?g|png|svg|tiff?|webp)$/iu.test(file.name)
      );
    })
  ) {
    return true;
  }

  const items = Array.from(clipboardData.items);
  if (items.some((item) => item.kind === "file" && item.type.toLowerCase().startsWith("image/"))) {
    return true;
  }

  const types = Array.from(clipboardData.types).map((type) => type.toLowerCase());
  if (
    types.some(
      (type) =>
        type === "files" ||
        type === "public.file-url" ||
        type.startsWith("image/") ||
        type.startsWith("public.image"),
    )
  ) {
    return true;
  }

  return (
    types.includes("text/uri-list") &&
    clipboardData.getData("text/uri-list").trim().toLowerCase().startsWith("file:")
  );
}

function firstImageClipboardItem(dataTransfer: DataTransfer | null): DataTransferItem | null {
  if (!dataTransfer) {
    return null;
  }
  for (const item of dataTransfer.items) {
    if (item.kind === "file" && item.type.startsWith("image/")) {
      return item;
    }
  }
  return null;
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => {
      if (typeof reader.result === "string") {
        resolve(reader.result);
      } else {
        reject(new Error("Image data was not readable"));
      }
    });
    reader.addEventListener("error", () => {
      reject(reader.error ?? new Error("Image data read failed"));
    });
    reader.readAsDataURL(file);
  });
}

function requestImagePaste(
  requestId: string,
  base64Data: string | undefined,
  suggestedName: string | undefined,
): Promise<ImagePasteResult> {
  return new Promise((resolve) => {
    pendingImagePasteRequests.set(requestId, resolve);
    postToHost(
      base64Data === undefined
        ? { type: "pasteImage", requestId }
        : { type: "pasteImage", requestId, base64Data, suggestedName },
    );
  });
}

function insertTextAtCursor(text: string): void {
  if (!editorInstance) {
    return;
  }

  const selection = editorInstance.getSelection();
  if (!selection) {
    return;
  }

  editorInstance.pushUndoStop();
  editorInstance.executeEdits("ghostex-editor-image-paste", [
    {
      forceMoveMarkers: true,
      range: selection,
      text,
    },
  ]);
  const model = editorInstance.getModel();
  if (model) {
    const insertedPosition = positionAfterInsertedText(selection, text);
    editorInstance.setPosition(insertedPosition);
    editorInstance.revealPositionInCenterIfOutsideViewport(insertedPosition);
  }
  editorInstance.pushUndoStop();
  editorInstance.focus();
}

function positionAfterInsertedText(selection: unknown, text: string): unknown {
  const startLineNumber = readNumberProperty(selection, "startLineNumber", 1);
  const startColumn = readNumberProperty(selection, "startColumn", 1);
  const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  if (lines.length === 1) {
    return {
      column: startColumn + text.length,
      lineNumber: startLineNumber,
    };
  }
  return {
    column: lines[lines.length - 1].length + 1,
    lineNumber: startLineNumber + lines.length - 1,
  };
}

function readNumberProperty(value: unknown, key: string, fallback: number): number {
  if (!value || typeof value !== "object") {
    return fallback;
  }
  const record = value as Record<string, unknown>;
  return typeof record[key] === "number" ? record[key] : fallback;
}

function suggestedImageName(file: File, requestId: string): string {
  const extension = file.name.split(".").pop() || extensionForMimeType(file.type);
  return `pasted-image-${requestId}.${extension}`;
}

function extensionForMimeType(mimeType: string): string {
  if (mimeType === "image/jpeg") {
    return "jpg";
  }
  if (mimeType === "image/gif") {
    return "gif";
  }
  if (mimeType === "image/webp") {
    return "webp";
  }
  return "png";
}

function createRequestId(): string {
  if (globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function normalizeCursorOffset(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }
  return Math.max(0, Math.floor(value));
}

function moveCaretToOffset(model: MonacoModel, offset: number): void {
  if (!editorInstance) {
    return;
  }
  const position = model.getPositionAt(clampOffset(offset, model.getValueLength()));
  editorInstance.setPosition(position);
  editorInstance.revealPositionInCenterIfOutsideViewport(position);
}

function clampOffset(offset: number, length: number): number {
  if (!Number.isFinite(offset)) {
    return length;
  }
  return Math.min(Math.max(Math.floor(offset), 0), Math.max(length, 0));
}

function isConfigureMessage(value: unknown): value is GhostexEditorConfigureMessage {
  if (!value || typeof value !== "object") {
    return false;
  }
  return (value as Record<string, unknown>).type === "configure";
}

function isImagePasteResult(value: unknown): value is ImagePasteResult {
  if (!value || typeof value !== "object") {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    record.type === "imagePasteResult" &&
    typeof record.requestId === "string" &&
    (record.path === undefined || typeof record.path === "string") &&
    (record.error === undefined || typeof record.error === "string")
  );
}

function isImagePreviewResult(value: unknown): value is ImagePreviewResult {
  if (!value || typeof value !== "object") {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    record.type === "imagePreviewResult" &&
    typeof record.requestId === "string" &&
    typeof record.path === "string" &&
    (record.dataUrl === undefined || typeof record.dataUrl === "string") &&
    (record.error === undefined || typeof record.error === "string")
  );
}

export {};
