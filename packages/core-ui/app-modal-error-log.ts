type AppModalErrorLogPayload = {
  area: string;
  message: string;
  name?: string;
  stack?: string;
  type: 'logError';
};

declare global {
  interface Window {
    webkit?: {
      messageHandlers?: {
        ghostexAppModalHost?: {
          postMessage: (message: unknown) => void;
        };
        ghostexNativeHost?: {
          postMessage: (message: unknown) => void;
        };
        ghostexNativeHostDiagnostics?: {
          postMessage: (message: unknown) => void;
        };
      };
    };
  }
}

/**
 * CDXC:AppModals 2026-04-27-14:25
 * Modal-host errors must be persisted even when debugging mode is disabled.
 * Send every captured exception to the native host so it can append a timestamped
 * area-tagged line under the resolved Ghostex logs directory for post-failure diagnosis.
 */
export function logAppModalError(area: string, error: unknown): void {
  const payload: AppModalErrorLogPayload = {
    area,
    message: describeErrorMessage(error),
    /*
     * CDXC:GPUISettingsModalDiagnostics 2026-06-27-17:25:
     * GPUI persists app-modal errors through a sanitized writer so blank Settings repros can identify the error class without storing raw messages or stacks that may contain paths, URLs, commands, tokens, or user content.
     */
    name: error instanceof Error ? error.name : undefined,
    stack: error instanceof Error ? error.stack : undefined,
    type: 'logError',
  };

  try {
    window.webkit?.messageHandlers?.ghostexAppModalHost?.postMessage(payload);
  } catch (loggingError) {
    console.error('[AppModals] failed to persist modal error', loggingError, payload);
  }
}

export function installAppModalGlobalErrorLogging(area: string): void {
  window.addEventListener('error', (event) => {
    logAppModalError(area, event.error ?? event.message);
  });
  window.addEventListener('unhandledrejection', (event) => {
    logAppModalError(area, event.reason);
  });
}

function describeErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
