import { Excalidraw } from "@excalidraw/excalidraw";
import { type ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { IconAlertTriangle } from "@tabler/icons-react";
import {
  type KeyboardEvent as ReactKeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { MANAGE_EXCALIDRAW_CANVAS_BACKGROUND, MANAGE_EXCALIDRAW_CANVAS_THEME } from "../constants";
import { ManagePreviewMessage, isEditableEventTarget } from "./preview-shared";
import { createExcalidrawSceneSignature, parseExcalidrawFile, serializeExcalidrawFile } from "../excalidraw-io";
import "@excalidraw/excalidraw/index.css";

export function ManageExcalidrawEditor({
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

export function handleManageExcalidrawKeyDown(event: ReactKeyboardEvent<HTMLDivElement>): void {
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

export function suppressManageExcalidrawToolKeyBeep(event: ReactKeyboardEvent<HTMLDivElement>): void {
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
