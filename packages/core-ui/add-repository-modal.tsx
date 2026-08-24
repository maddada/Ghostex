import { IconFolderOpen, IconInfoCircle } from '@tabler/icons-react';
import { useEffect, useId, useRef, useState, type CSSProperties, type FormEvent } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/packages/components/ui/dialog';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { Input } from '@/packages/components/ui/input';
import type { GxserverRepositoryClonePreviewResult } from '../shared/gxserver-protocol';
import { isRepositoryCloneBranchNameInputValid, parseRepositoryCloneInput } from '../shared/repository-clone';
import { AppTooltip, TooltipProvider } from './app-tooltip';
import { postAppModalHostMessage } from './app-modal-host-bridge';
import { canSubmitAddRepositoryClone } from './add-repository-modal-state';
import { RemoteProjectPickerModal } from './remote-project-picker/remote-project-picker-modal';
import type {
  RemoteFilesystemBrowseInput,
  RemoteFilesystemBrowseResult,
} from './remote-project-picker/remote-filesystem';

const ADD_REPOSITORY_LAST_LOCATION_STORAGE_KEY = 'ghostex.addRepository.lastLocation';
const ADD_REPOSITORY_OPTION_HELP_TOOLTIP_STYLE = {
  maxWidth: 'min(230px, 90vw)',
} satisfies CSSProperties;

type AddRepositoryCloneRequest = {
  branchName: string;
  cloneMainOnly: boolean;
  folderPath: string;
  newFolderName: string;
  repositoryInput: string;
  requestId: string;
  shallowClone: boolean;
};

type AddRepositoryClonePreviewRequest = {
  folderPath: string;
  newFolderName: string;
  repositoryInput: string;
  requestId: string;
};

export type AddRepositoryModalProps = {
  isOpen: boolean;
  onCancel: () => void;
  onClone: (request: AddRepositoryCloneRequest) => void;
  onCloneSuccess: () => void;
  onRemoteBrowse?: (input: RemoteFilesystemBrowseInput) => Promise<RemoteFilesystemBrowseResult | null>;
  onPreview: (request: AddRepositoryClonePreviewRequest) => void;
  remoteMachineId?: string;
  remoteMachineName?: string;
};

type RepositoryFolderPickedMessage = {
  path?: unknown;
  type: 'repositoryFolderPicked';
};

type RepositoryCloneResultMessage = {
  error?: unknown;
  ok: boolean;
  projectPath?: unknown;
  requestId: string;
  type: 'repositoryCloneResult';
};

type RepositoryClonePreviewResultMessage = {
  error?: unknown;
  ok: boolean;
  preview?: unknown;
  requestId: string;
  type: 'repositoryClonePreviewResult';
};

/**
 * CDXC:AddRepository 2026-05-29-11:45:
 * Projects needs a full-window Clone Repository dialog next to Add Project. Keep
 * the visual shell aligned with Rename Session, accept flexible repository
 * paste formats, and remember the last clone parent folder across the app.
 *
 * CDXC:AddRepository 2026-06-01-10:33:
 * Submitting Clone & Add closes the modal immediately. Long-running clone
 * progress, cancellation, and final errors live in toasts so the modal does not
 * block the workspace while Git runs.
 */
export function AddRepositoryModal({
  isOpen,
  onCancel,
  onClone,
  onCloneSuccess,
  onRemoteBrowse,
  onPreview,
  remoteMachineId,
  remoteMachineName,
}: AddRepositoryModalProps) {
  const repositoryId = useId();
  const folderId = useId();
  const newFolderId = useId();
  const branchId = useId();
  const cloneMainOnlyId = useId();
  const shallowCloneId = useId();
  const repositoryRef = useRef<HTMLInputElement>(null);
  const activeRequestIdRef = useRef<string | undefined>(undefined);
  const previewRequestIdRef = useRef<string | undefined>(undefined);
  const [cloneMainOnly, setCloneMainOnly] = useState(false);
  const [repositoryInput, setRepositoryInput] = useState('');
  const [folderPath, setFolderPath] = useState(readLastRepositoryLocation);
  const [newFolderName, setNewFolderName] = useState('');
  const [branchName, setBranchName] = useState('');
  const [clonePreview, setClonePreview] = useState<GxserverRepositoryClonePreviewResult | undefined>(undefined);
  const [previewErrorMessage, setPreviewErrorMessage] = useState<string | undefined>(undefined);
  const [shallowClone, setShallowClone] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | undefined>(undefined);
  const [isCloning, setIsCloning] = useState(false);
  const [isRemoteDestinationPickerOpen, setIsRemoteDestinationPickerOpen] = useState(false);
  const isRemoteClone =
    typeof remoteMachineId === 'string' &&
    remoteMachineId.trim().length > 0 &&
    typeof remoteMachineName === 'string' &&
    remoteMachineName.trim().length > 0 &&
    typeof onRemoteBrowse === 'function';

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setRepositoryInput('');
    setCloneMainOnly(false);
    setShallowClone(false);
    setNewFolderName('');
    setBranchName('');
    setClonePreview(undefined);
    setPreviewErrorMessage(undefined);
    setErrorMessage(undefined);
    setIsCloning(false);
    setIsRemoteDestinationPickerOpen(false);
    activeRequestIdRef.current = undefined;
    previewRequestIdRef.current = undefined;
    setFolderPath(isRemoteClone ? '~/' : readLastRepositoryLocation());

    const animationFrame = window.requestAnimationFrame(() => {
      repositoryRef.current?.focus();
    });
    return () => {
      window.cancelAnimationFrame(animationFrame);
    };
  }, [isOpen, isRemoteClone]);

  useEffect(() => {
    if (!isOpen || isCloning) {
      return;
    }
    const normalizedRepositoryInput = repositoryInput.trim();
    const normalizedFolderPath = folderPath.trim();
    if (!normalizedRepositoryInput || !normalizedFolderPath) {
      setClonePreview(undefined);
      setPreviewErrorMessage(undefined);
      return;
    }
    const requestId = `repository-clone-preview-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    previewRequestIdRef.current = requestId;
    const timeout = window.setTimeout(() => {
      onPreview({
        folderPath: normalizedFolderPath,
        newFolderName,
        repositoryInput: normalizedRepositoryInput,
        requestId,
      });
    }, 220);
    return () => {
      window.clearTimeout(timeout);
    };
  }, [folderPath, isCloning, isOpen, newFolderName, onPreview, repositoryInput]);

  useEffect(() => {
    const handleModalHostMessage = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (!message || typeof message !== 'object' || !('type' in message)) {
        return;
      }
      if (isRepositoryFolderPickedMessage(message)) {
        const nextPath = typeof message.path === 'string' ? message.path.trim() : '';
        if (nextPath) {
          rememberLastRepositoryLocation(nextPath);
          setFolderPath(nextPath);
        }
        return;
      }
      if (isRepositoryClonePreviewResultMessage(message)) {
        if (message.requestId !== previewRequestIdRef.current) {
          return;
        }
        if (!message.ok) {
          setClonePreview(undefined);
          setPreviewErrorMessage(
            typeof message.error === 'string' && message.error.trim()
              ? message.error.trim()
              : 'Repository clone preview failed.'
          );
          return;
        }
        const preview = isRepositoryClonePreview(message.preview) ? message.preview : undefined;
        setClonePreview(preview);
        setPreviewErrorMessage(undefined);
        return;
      }
      if (!isRepositoryCloneResultMessage(message)) {
        return;
      }
      if (message.requestId !== activeRequestIdRef.current) {
        return;
      }
      activeRequestIdRef.current = undefined;
      setIsCloning(false);
      if (message.ok) {
        onCloneSuccess();
        return;
      }
      setErrorMessage(
        typeof message.error === 'string' && message.error.trim() ? message.error.trim() : 'Repository clone failed.'
      );
    };

    window.addEventListener('ghostex-app-modal-host-message', handleModalHostMessage);
    return () => {
      window.removeEventListener('ghostex-app-modal-host-message', handleModalHostMessage);
    };
  }, [onCloneSuccess]);

  if (!isOpen) {
    return null;
  }

  const hasInvalidRepositoryInput = repositoryInput.trim().length > 0 && Boolean(previewErrorMessage);
  const hasInvalidBranchName = branchName.trim().length > 0 && !isRepositoryCloneBranchNameInputValid(branchName);
  const destinationWarning = clonePreview?.destinationExists ? clonePreview.warning : undefined;
  const canClone = canSubmitAddRepositoryClone({
    branchName,
    clonePreview,
    folderPath,
    isCloning,
    newFolderName,
    previewErrorMessage,
    repositoryInput,
  });

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setErrorMessage(undefined);
    const normalizedRepositoryInput = repositoryInput.trim();
    const normalizedFolderPath = folderPath.trim();
    if (!parseRepositoryCloneInput(normalizedRepositoryInput)) {
      setErrorMessage('Enter a Git repository to clone.');
      return;
    }
    if (!isRepositoryCloneBranchNameInputValid(branchName)) {
      setErrorMessage('Enter a valid Git branch name.');
      return;
    }
    if (!normalizedFolderPath) {
      setErrorMessage('Choose a folder location.');
      return;
    }
    if (clonePreview?.destinationExists) {
      setErrorMessage(clonePreview.warning ?? 'Choose a new folder name before cloning.');
      return;
    }
    if (previewErrorMessage) {
      setErrorMessage(previewErrorMessage);
      return;
    }

    const requestId = `repository-clone-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
    if (!isRemoteClone) {
      rememberLastRepositoryLocation(normalizedFolderPath);
    }
    activeRequestIdRef.current = requestId;
    setIsCloning(true);
    onClone({
      branchName: branchName.trim(),
      cloneMainOnly,
      folderPath: normalizedFolderPath,
      newFolderName: newFolderName.trim(),
      repositoryInput: normalizedRepositoryInput,
      requestId,
      shallowClone,
    });
  };

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !isCloning) {
          onCancel();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className='command-config-modal-shadcn session-rename-modal-shadcn add-repository-modal-shadcn font-sans'
        showCloseButton={false}
      >
        <form className='session-rename-form add-repository-form' onSubmit={submit}>
          <DialogHeader>
            <DialogTitle className='text-xl'>Clone Repository</DialogTitle>
            <DialogDescription>
              {isRemoteClone
                ? `Clone a Git repository on ${remoteMachineName} and add it as a remote project.`
                : 'Clone a Git repository and add it as a project.'}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup className='session-rename-field-group'>
            <Field data-invalid={hasInvalidRepositoryInput || undefined}>
              <FieldLabel htmlFor={repositoryId}>Repository</FieldLabel>
              <Input
                aria-invalid={hasInvalidRepositoryInput || undefined}
                autoFocus
                className='h-10 px-3 text-sm md:text-sm'
                disabled={isCloning}
                id={repositoryId}
                onChange={(event) => {
                  setRepositoryInput(event.currentTarget.value);
                  setNewFolderName('');
                  setBranchName('');
                  setClonePreview(undefined);
                  setPreviewErrorMessage(undefined);
                  setErrorMessage(undefined);
                }}
                placeholder='maddada/zehn'
                ref={repositoryRef}
                value={repositoryInput}
              />
              <FieldDescription>
                {clonePreview?.cloneUrl ?? 'Paste a GitHub shorthand, HTTPS URL, or SSH URL.'}
              </FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor={folderId}>Folder location</FieldLabel>
              <div className='add-repository-folder-row'>
                <Input
                  className='h-10 px-3 text-sm md:text-sm'
                  disabled={isCloning}
                  id={folderId}
                  onChange={(event) => {
                    setFolderPath(event.currentTarget.value);
                    setClonePreview(undefined);
                    setPreviewErrorMessage(undefined);
                    setErrorMessage(undefined);
                  }}
                  value={folderPath}
                />
                <Button
                  disabled={isCloning}
                  onClick={() => {
                    if (isRemoteClone) {
                      setIsRemoteDestinationPickerOpen(true);
                      return;
                    }
                    postAppModalHostMessage(
                      { initialPath: folderPath.trim(), type: 'pickRepositoryFolder' },
                      'AppModals:pickRepositoryFolder'
                    );
                  }}
                  type='button'
                  variant='secondary'
                >
                  <IconFolderOpen aria-hidden='true' data-icon='inline-start' />
                  Choose
                </Button>
              </div>
              {isRemoteClone ? (
                <FieldDescription>Folder path on {remoteMachineName}; Choose browses that machine.</FieldDescription>
              ) : null}
            </Field>
            <Field data-invalid={destinationWarning ? true : undefined}>
              {/*
              CDXC:AddRepository 2026-06-01-11:18:
              The modal needs an explicit editable new-folder name because gxserver now blocks clone start when the resolved destination already exists. Keep the warning tied to server preview results so all clients enforce the same destination rule.

              CDXC:AddRepository 2026-06-07-16:01:
              The field must remain empty by default so users can clone with the repository name as the destination folder. Only typed text should override gxserver's repo-name default.
              */}
              <FieldLabel htmlFor={newFolderId}>New folder</FieldLabel>
              <Input
                aria-invalid={destinationWarning ? true : undefined}
                className='h-10 px-3 text-sm md:text-sm'
                disabled={isCloning}
                id={newFolderId}
                onChange={(event) => {
                  setNewFolderName(event.currentTarget.value);
                  setClonePreview(undefined);
                  setPreviewErrorMessage(undefined);
                  setErrorMessage(undefined);
                }}
                placeholder="Leave empty to use the Repo's name as the folder name"
                value={newFolderName}
              />
              <FieldDescription>
                {clonePreview?.destinationPath ?? 'The repository will be cloned into this folder.'}
              </FieldDescription>
            </Field>
            <Field data-invalid={hasInvalidBranchName ? true : undefined}>
              {/*
              CDXC:AddRepository 2026-06-07-16:06:
              Branch input is optional. Empty keeps Git on the repository default branch, usually main or master, while typed text asks gxserver to clone and check out that branch by name.
              */}
              <FieldLabel htmlFor={branchId}>Branch</FieldLabel>
              <Input
                aria-invalid={hasInvalidBranchName ? true : undefined}
                className='h-10 px-3 text-sm md:text-sm'
                disabled={isCloning}
                id={branchId}
                onChange={(event) => {
                  setBranchName(event.currentTarget.value);
                  setErrorMessage(undefined);
                }}
                placeholder='Leave empty to clone main/master'
                value={branchName}
              />
              <FieldDescription>
                {hasInvalidBranchName
                  ? 'Enter a valid Git branch name.'
                  : 'Type a branch name only when you do not want the repository default.'}
              </FieldDescription>
            </Field>
            <TooltipProvider delayDuration={300}>
              <div className='add-repository-options-row'>
                {/*
                CDXC:AddRepository 2026-06-01-10:28:
                The Clone Repository modal needs explicit unchecked clone-scope options for reference-only repositories. Keep the option help adjacent to each checkbox so users understand main-only and shallow clones are for repos they want to inspect, not repos they expect to work on heavily.

                CDXC:AddRepository 2026-06-02-20:12:
                Clone option help tooltips must wrap within a 230px maximum width so explanatory copy stays readable and does not span across the modal.

                CDXC:AddRepository 2026-06-07-16:06:
                Branch-only clone scope now follows the Branch field. If Branch is empty, Git clones only the repository default branch; if Branch is typed, Git clones only that branch.
                */}
                <label className='add-repository-option' htmlFor={cloneMainOnlyId}>
                  <input
                    checked={cloneMainOnly}
                    className='add-repository-option-checkbox'
                    disabled={isCloning}
                    id={cloneMainOnlyId}
                    onChange={(event) => setCloneMainOnly(event.currentTarget.checked)}
                    type='checkbox'
                  />
                  <span className='add-repository-option-label'>Clone branch only</span>
                  <AppTooltip
                    content='Use for repos you mostly want as references. This fetches only the selected branch, or the default branch when Branch is empty, so avoid it for repos you plan to work on heavily across branches.'
                    contentStyle={ADD_REPOSITORY_OPTION_HELP_TOOLTIP_STYLE}
                  >
                    <span
                      aria-label='Clone branch only help'
                      className='add-repository-option-info'
                      role='img'
                      tabIndex={0}
                    >
                      <IconInfoCircle aria-hidden='true' size={14} stroke={2.2} />
                    </span>
                  </AppTooltip>
                </label>
                <label className='add-repository-option' htmlFor={shallowCloneId}>
                  <input
                    checked={shallowClone}
                    className='add-repository-option-checkbox'
                    disabled={isCloning}
                    id={shallowCloneId}
                    onChange={(event) => setShallowClone(event.currentTarget.checked)}
                    type='checkbox'
                  />
                  <span className='add-repository-option-label'>Shallow clone</span>
                  <AppTooltip
                    content='Use for repos you mostly want as references. This fetches only the latest history depth, so avoid it for repos you plan to work on heavily with blame, bisect, or older commits.'
                    contentStyle={ADD_REPOSITORY_OPTION_HELP_TOOLTIP_STYLE}
                  >
                    <span
                      aria-label='Shallow clone help'
                      className='add-repository-option-info'
                      role='img'
                      tabIndex={0}
                    >
                      <IconInfoCircle aria-hidden='true' size={14} stroke={2.2} />
                    </span>
                  </AppTooltip>
                </label>
              </div>
            </TooltipProvider>
            {destinationWarning ? (
              <div className='add-repository-warning' role='alert'>
                {destinationWarning}
              </div>
            ) : null}
            {errorMessage || previewErrorMessage ? (
              <div className='add-repository-error' role='alert'>
                {errorMessage ?? previewErrorMessage}
              </div>
            ) : null}
          </FieldGroup>
          <DialogFooter>
            <Button disabled={isCloning} onClick={onCancel} type='button' variant='outline'>
              Cancel
            </Button>
            <Button disabled={!canClone} type='submit'>
              {isCloning ? 'Cloning...' : 'Clone & Add'}
            </Button>
          </DialogFooter>
        </form>
        {isRemoteClone ? (
          /*
           * CDXC:RemoteClone 2026-06-02-23:53:
           * Remote Clone Repository uses the shared path-aware browse picker
           * for destination selection. The local macOS folder picker stays
           * local-only; remote browsing is machine-scoped through gxserver.
           */
          <RemoteProjectPickerModal
            actionLabel='Select'
            description={`Choose a clone destination folder on ${remoteMachineName}`}
            initialQuery={folderPath.trim() || '~/'}
            isOpen={isRemoteDestinationPickerOpen}
            machineName={remoteMachineName ?? 'Remote'}
            onAddProject={(path) => {
              setFolderPath(path);
              setClonePreview(undefined);
              setPreviewErrorMessage(undefined);
              setErrorMessage(undefined);
            }}
            onBrowse={onRemoteBrowse}
            onClose={() => setIsRemoteDestinationPickerOpen(false)}
            pendingLabel='Selecting'
            title='Select clone destination'
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function readLastRepositoryLocation(): string {
  const storedLocation = localStorage.getItem(ADD_REPOSITORY_LAST_LOCATION_STORAGE_KEY)?.trim();
  if (storedLocation) {
    return storedLocation;
  }
  const nativeBootstrap = (window as { __ghostex_NATIVE_HOST__?: { cwd?: string; homeDir?: string } })
    .__ghostex_NATIVE_HOST__;
  return nativeBootstrap?.cwd?.trim() || nativeBootstrap?.homeDir?.trim() || '';
}

function rememberLastRepositoryLocation(path: string): void {
  localStorage.setItem(ADD_REPOSITORY_LAST_LOCATION_STORAGE_KEY, path);
}

function isRepositoryFolderPickedMessage(message: object): message is RepositoryFolderPickedMessage {
  return 'type' in message && message.type === 'repositoryFolderPicked';
}

function isRepositoryCloneResultMessage(message: object): message is RepositoryCloneResultMessage {
  return (
    'type' in message &&
    message.type === 'repositoryCloneResult' &&
    'requestId' in message &&
    typeof message.requestId === 'string' &&
    'ok' in message &&
    typeof message.ok === 'boolean'
  );
}

function isRepositoryClonePreviewResultMessage(message: object): message is RepositoryClonePreviewResultMessage {
  return (
    'type' in message &&
    message.type === 'repositoryClonePreviewResult' &&
    'requestId' in message &&
    typeof message.requestId === 'string' &&
    'ok' in message &&
    typeof message.ok === 'boolean'
  );
}

function isRepositoryClonePreview(value: unknown): value is GxserverRepositoryClonePreviewResult {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    typeof (value as GxserverRepositoryClonePreviewResult).cloneUrl === 'string' &&
    typeof (value as GxserverRepositoryClonePreviewResult).destinationFolderName === 'string' &&
    typeof (value as GxserverRepositoryClonePreviewResult).destinationPath === 'string' &&
    typeof (value as GxserverRepositoryClonePreviewResult).parentPath === 'string' &&
    typeof (value as GxserverRepositoryClonePreviewResult).repositoryName === 'string' &&
    typeof (value as GxserverRepositoryClonePreviewResult).destinationExists === 'boolean'
  );
}
