import { type AppState, type BinaryFiles } from '@excalidraw/excalidraw/types';
import { type ExcalidrawElement } from '@excalidraw/excalidraw/element/types';
import { ExcalidrawFileData, isRecord } from './types';
import { MANAGE_EXCALIDRAW_CANVAS_BACKGROUND, MANAGE_EXCALIDRAW_CANVAS_THEME } from './constants';

export function parseExcalidrawFile(
  content: string
): { data: ExcalidrawFileData; ok: true } | { error: string; ok: false } {
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
      return { error: 'Drawing JSON must be an object.', ok: false };
    }
    if (value.type !== 'excalidraw' && !Array.isArray(value.elements)) {
      return { error: 'Drawing JSON is missing scene elements.', ok: false };
    }
    return {
      data: {
        appState: isRecord(value.appState) ? value.appState : {},
        elements: Array.isArray(value.elements) ? (value.elements as ExcalidrawElement[]) : [],
        files: isRecord(value.files) ? (value.files as BinaryFiles) : {},
        source: typeof value.source === 'string' ? value.source : 'https://excalidraw.com',
        type: 'excalidraw',
        version: typeof value.version === 'number' ? value.version : 2,
      },
      ok: true,
    };
  } catch (parseError) {
    return {
      error: parseError instanceof Error ? parseError.message : 'Drawing JSON is invalid.',
      ok: false,
    };
  }
}

export function createEmptyExcalidrawFile(): ExcalidrawFileData {
  return {
    appState: {
      theme: MANAGE_EXCALIDRAW_CANVAS_THEME,
      viewBackgroundColor: MANAGE_EXCALIDRAW_CANVAS_BACKGROUND,
    },
    elements: [],
    files: {},
    source: 'https://excalidraw.com',
    type: 'excalidraw',
    version: 2,
  };
}

export function serializeExcalidrawFile(
  previousData: ExcalidrawFileData,
  elements: readonly ExcalidrawElement[],
  appState: AppState,
  files: BinaryFiles
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
      source: previousData.source ?? 'https://excalidraw.com',
      type: 'excalidraw',
      version: previousData.version ?? 2,
    },
    null,
    2
  );
}

export function createExcalidrawSceneSignature(
  elements: readonly ExcalidrawElement[],
  appState: AppState,
  files: BinaryFiles
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

export function normalizeExcalidrawZoom(zoom: AppState['zoom']): number {
  if (typeof zoom === 'object' && zoom !== null && 'value' in zoom && typeof zoom.value === 'number') {
    return zoom.value;
  }
  return typeof zoom === 'number' ? zoom : 1;
}
