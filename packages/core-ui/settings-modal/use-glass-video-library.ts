/**
 * Settings -> Theme -> Transparency -> Video: the glass video library's state in the settings page.
 * Asks the app for the library when Settings opens with the Video source picked, and follows the
 * download progress and removal answers the app sends back.
 */
import { useCallback, useEffect, useState } from 'react';
import {
  parseGlassVideoLibraryListing,
  type GlassVideoDownload,
  type GlassVideoLibraryListing,
} from '../../shared/glass-video-library';
import { type WebviewApi } from '../webview-api';

export type GlassVideoLibraryControls = {
  listing?: GlassVideoLibraryListing;
  downloads: Readonly<Record<string, GlassVideoDownload>>;
  /** The last error per video (a failed download or removal), shown on its card. */
  errors: Readonly<Record<string, string>>;
  refresh: () => void;
  download: (id: string) => void;
  cancel: (id: string) => void;
  remove: (id: string) => void;
};

export function useGlassVideoLibrary({
  enabled,
  isOpen,
  vscode,
}: {
  /** Only while the Video source is picked and a native host can answer. */
  enabled: boolean;
  isOpen: boolean;
  vscode: WebviewApi | undefined;
}): GlassVideoLibraryControls {
  const [listing, setListing] = useState<GlassVideoLibraryListing>();
  const [downloads, setDownloads] = useState<Record<string, GlassVideoDownload>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});

  const post = useCallback(
    (message: Parameters<WebviewApi['postMessage']>[0]) => {
      vscode?.postMessage(message);
    },
    [vscode]
  );

  useEffect(() => {
    if (!isOpen || !enabled || !vscode) {
      return;
    }
    const handle = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (!message || typeof message !== 'object' || !('type' in message)) {
        return;
      }
      const record = message as Record<string, unknown>;
      const id = typeof record.id === 'string' ? record.id : '';
      if (record.type === 'glassVideoLibraryListed') {
        const parsed = parseGlassVideoLibraryListing(message);
        if (parsed) {
          setListing(parsed);
        }
      } else if (record.type === 'glassVideoDownloadProgress' && id) {
        const received = typeof record.received === 'number' ? record.received : 0;
        const total = typeof record.total === 'number' ? record.total : undefined;
        setDownloads((current) => ({ ...current, [id]: { received, total } }));
      } else if (record.type === 'glassVideoDownloadFinished' && id) {
        setDownloads((current) => {
          const { [id]: _finished, ...rest } = current;
          return rest;
        });
        const error = typeof record.error === 'string' ? record.error : '';
        setErrors((current) => {
          const { [id]: _previous, ...rest } = current;
          return record.ok === true || error === 'Cancelled' || !error ? rest : { ...rest, [id]: error };
        });
      } else if (record.type === 'glassVideoRemoveFailed' && id && typeof record.error === 'string') {
        const error = record.error;
        setErrors((current) => ({ ...current, [id]: error }));
      }
    };
    window.addEventListener('ghostex-app-modal-host-message', handle);
    post({ type: 'listGlassVideoLibrary' });
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', handle);
    };
  }, [enabled, isOpen, post, vscode]);

  return {
    listing,
    downloads,
    errors,
    refresh: () => post({ type: 'listGlassVideoLibrary' }),
    download: (id) => {
      setErrors((current) => {
        const { [id]: _previous, ...rest } = current;
        return rest;
      });
      setDownloads((current) => ({ ...current, [id]: { received: 0 } }));
      post({ id, type: 'downloadGlassVideo' });
    },
    cancel: (id) => post({ id, type: 'cancelGlassVideoDownload' }),
    remove: (id) => post({ id, type: 'removeGlassVideo' }),
  };
}
