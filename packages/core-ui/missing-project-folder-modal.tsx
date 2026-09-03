import { IconFolderSearch, IconTrash } from '@tabler/icons-react';
import { Card, CardContent } from '@/packages/components/ui/card';
import {
  AppModalButton,
  AppModalColumn,
  AppModalDescription,
  AppModalFooter,
  AppModalHeader,
  AppModalShell,
  AppModalStack,
  AppModalTitle,
} from './app-modal-shell';

export type MissingProjectFolderModalProps = {
  isOpen: boolean;
  onCancel: () => void;
  onLocate: () => void;
  onRemove: () => void;
  projectName: string;
  projectPath: string;
};

/**
 * CDXC:AppModal 2026-08-26:
 * Restyled onto AppModalShell. The `missing-project-folder-modal` class stays
 * on the shell root as a marker: apps/desktop/views/modal-host.tsx measures
 * that selector to fit the native child window's height.
 */
export function MissingProjectFolderModal({
  isOpen,
  onCancel,
  onLocate,
  onRemove,
  projectName,
  projectPath,
}: MissingProjectFolderModalProps) {
  return (
    <AppModalShell className='missing-project-folder-modal' isOpen={isOpen} onClose={onCancel} width={520}>
      <AppModalColumn>
        <AppModalHeader>
          <AppModalTitle>Project folder can’t be found</AppModalTitle>
          <AppModalDescription>
            Ghostex can’t start terminals or agents because {projectName}’s folder is missing.
          </AppModalDescription>
        </AppModalHeader>
        <AppModalStack>
          <Card size='sm'>
            <CardContent className='missing-project-folder-details'>
              <code className='missing-project-folder-path'>{projectPath}</code>
              <p className='missing-project-folder-note'>
                Locating the moved folder keeps this project’s sessions, groups, actions, and settings.
              </p>
            </CardContent>
          </Card>
        </AppModalStack>
        <AppModalFooter>
          <AppModalButton className='missing-project-folder-remove' onClick={onRemove} tone='danger' type='button'>
            <IconTrash aria-hidden='true' size={15} stroke={1.9} />
            Remove Project
          </AppModalButton>
          <AppModalButton className='missing-project-folder-locate' onClick={onLocate} type='button'>
            <IconFolderSearch aria-hidden='true' size={15} stroke={1.9} />
            Locate Folder…
          </AppModalButton>
        </AppModalFooter>
      </AppModalColumn>
    </AppModalShell>
  );
}
