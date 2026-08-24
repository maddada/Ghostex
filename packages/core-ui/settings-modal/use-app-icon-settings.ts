/*
 * CDXC:SettingsModalSplit 2026-08-23:
 * The prop-driven App Icon confirm-before-persist effect and its wire-contract
 * commands, plus the adjacent native Background Image picker round trip. The
 * two effects keep their original relative order inside this hook.
 */
import { useEffect, type Dispatch, type RefObject, type SetStateAction } from 'react';
import { type SidebarAppIconStateMessage } from '../../shared/session-grid-contract';
import { type ghostexSettings } from '../../shared/ghostex-settings';
import { type WebviewApi } from '../webview-api';

export function useAppIconSettings({
  appIconPickerUnavailable,
  appIconState,
  draft,
  handledAppIconStateRef,
  isOpen,
  pendingAppIconSourceIdRef,
  pendingSettingsRef,
  setAppIconError,
  updateDraft,
  vscode,
}: {
  appIconPickerUnavailable: boolean;
  appIconState: SidebarAppIconStateMessage | undefined;
  draft: ghostexSettings;
  handledAppIconStateRef: RefObject<SidebarAppIconStateMessage | undefined>;
  isOpen: boolean;
  pendingAppIconSourceIdRef: RefObject<string | undefined>;
  pendingSettingsRef: RefObject<ghostexSettings | undefined>;
  setAppIconError: Dispatch<SetStateAction<string | undefined>>;
  updateDraft: <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => void;
  vscode: WebviewApi | undefined;
}) {
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * Confirm-before-persist is prop-driven: native relays appIconState into this
   * component through the modal-state plumbing (exactly like osIntegrationStatus),
   * so react to each new prop value. On an ok state, persist the in-flight
   * pending selection (falling back to native's selectedId) and clear any error;
   * on a failed state, drop the pending id and surface the error without writing
   * appIconSourceId.
   *
   * CDXC:SettingsPerformance 2026-06-29-00:40:
   * Process each native appIconState once inside this effect instead of updating
   * a closure ref during render, because SettingsModal needs React Compiler
   * coverage to reduce large settings-page rerenders during scroll navigation.
   */
  useEffect(() => {
    if (!appIconState) {
      return;
    }
    if (handledAppIconStateRef.current === appIconState) {
      return;
    }
    handledAppIconStateRef.current = appIconState;
    if (appIconState.ok) {
      setAppIconError(undefined);
      const pendingSourceId = pendingAppIconSourceIdRef.current;
      const confirmedSourceId = pendingSourceId !== undefined ? pendingSourceId : appIconState.selectedId;
      pendingAppIconSourceIdRef.current = undefined;
      const currentSettings = pendingSettingsRef.current ?? draft;
      if (currentSettings.appIconSourceId !== confirmedSourceId) {
        updateDraft('appIconSourceId', confirmedSourceId);
      }
      return;
    }
    pendingAppIconSourceIdRef.current = undefined;
    setAppIconError(
      typeof appIconState.error === 'string' && appIconState.error.trim()
        ? appIconState.error.trim()
        : 'Could not update the app icon.'
    );
  }, [appIconState, draft]);
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * Selecting, choosing a file, revealing the folder, and resetting all post the
   * exact wire-contract messages to native. The selection messages record the
   * pending source id and clear any prior error; the sidebar persists nothing
   * until the matching ok: true appIconState arrives.
   */
  const selectAppIcon = (sourceId: string) => {
    if (!vscode) {
      return;
    }
    pendingAppIconSourceIdRef.current = sourceId;
    setAppIconError(undefined);
    vscode.postMessage({ type: 'setAppIcon', sourceId });
  };
  const chooseAppIconFile = () => {
    if (!vscode) {
      return;
    }
    setAppIconError(undefined);
    vscode.postMessage({ type: 'pickAppIconFile' });
  };
  /**
   * CDXC:TerminalBackgroundImage 2026-08-01:
   * The Browse button next to Settings -> Terminal -> Background Image opens a
   * native file dialog host-side; the picked absolute path comes back as a
   * terminalBackgroundImageFilePicked host message and lands in the draft like
   * a typed path. Native pickers only exist in the desktop app, so web hosts
   * (which set appIconPickerUnavailable) render the plain text field instead.
   */
  const nativeFilePickerAvailable = Boolean(vscode) && !appIconPickerUnavailable;
  const chooseTerminalBackgroundImageFile = () => {
    if (!vscode) {
      return;
    }
    vscode.postMessage({ type: 'pickTerminalBackgroundImageFile' });
  };
  useEffect(() => {
    if (!isOpen || !nativeFilePickerAvailable) {
      return;
    }
    const handlePickedBackgroundImage = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (
        !message ||
        typeof message !== 'object' ||
        !('type' in message) ||
        message.type !== 'terminalBackgroundImageFilePicked'
      ) {
        return;
      }
      const path = 'path' in message && typeof message.path === 'string' ? message.path.trim() : '';
      if (!path) {
        return;
      }
      updateDraft('terminalBackgroundImage', path);
    };
    window.addEventListener('ghostex-app-modal-host-message', handlePickedBackgroundImage);
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', handlePickedBackgroundImage);
    };
  }, [isOpen, nativeFilePickerAvailable]);

  return {
    chooseAppIconFile,
    chooseTerminalBackgroundImageFile,
    nativeFilePickerAvailable,
    selectAppIcon,
  };
}
