import { IconAlertTriangle, IconFolderSearch } from '@tabler/icons-react';
import { useEffect } from 'react';

export type MissingProjectFolderModalProps = {
  isOpen: boolean;
  onCancel: () => void;
  onLocate: () => void;
  onRemove: () => void;
  projectName: string;
  projectPath: string;
};

export function MissingProjectFolderModal({
  isOpen,
  onCancel,
  onLocate,
  onRemove,
  projectName,
  projectPath,
}: MissingProjectFolderModalProps) {
  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onCancel();
      }
    };
    document.addEventListener('keydown', handleKeyDown, true);
    return () => document.removeEventListener('keydown', handleKeyDown, true);
  }, [isOpen, onCancel]);

  if (!isOpen) {
    return null;
  }

  return (
    <div className='confirm-modal-root scroll-mask-y' role='presentation'>
      <button className='confirm-modal-backdrop' onClick={onCancel} type='button' />
      <div
        aria-describedby='missing-project-folder-description'
        aria-labelledby='missing-project-folder-title'
        aria-modal='true'
        className='confirm-modal missing-project-folder-modal scroll-mask-y'
        role='dialog'
      >
        <div className='missing-project-folder-heading'>
          <IconAlertTriangle aria-hidden='true' size={22} stroke={1.8} />
          <div>
            <div className='confirm-modal-title' id='missing-project-folder-title'>
              Project folder can’t be found
            </div>
            <div className='confirm-modal-description' id='missing-project-folder-description'>
              Ghostex can’t start terminals or agents for {projectName} because its folder is no longer available.
            </div>
          </div>
        </div>
        <code className='missing-project-folder-path'>{projectPath}</code>
        <div className='missing-project-folder-note'>
          Locating the moved folder keeps this project’s sessions, groups, actions, and settings.
        </div>
        <div className='confirm-modal-actions missing-project-folder-actions'>
          <button className='secondary confirm-modal-button' onClick={onCancel} type='button'>
            Cancel
          </button>
          <button className='secondary danger confirm-modal-button' onClick={onRemove} type='button'>
            Remove from Ghostex
          </button>
          <button className='primary confirm-modal-button' onClick={onLocate} type='button'>
            <IconFolderSearch aria-hidden='true' size={15} stroke={1.9} />
            Locate Folder…
          </button>
        </div>
      </div>
    </div>
  );
}
