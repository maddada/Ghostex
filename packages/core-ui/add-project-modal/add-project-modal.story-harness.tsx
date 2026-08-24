/*
 * CDXC:AddProject 2026-07-30:
 * Shared Storybook harness for the add-project dialog. Both the visual stories
 * and the play-function stories mount the dialog through this component so the
 * mocked callbacks, the close bookkeeping, and the `data-add-project-story-*`
 * hooks stay identical between them.
 */

import { useMemo, useState } from 'react';
import { AddProjectModal } from './add-project-modal';
import {
  createAddProjectStoryMocks,
  type AddProjectStoryMockOptions,
  type AddProjectStoryMocks,
} from './add-project-modal-mocks';
import type { AddProjectAddResult, AddProjectProviderId } from './types';

let latestStoryMocks: AddProjectStoryMocks | null = null;

export function getAddProjectStoryMocks(): AddProjectStoryMocks {
  if (!latestStoryMocks) {
    throw new Error('The add-project story harness has not mounted yet');
  }
  return latestStoryMocks;
}

export function findAddProjectStoryCall(name: string): unknown {
  const call = getAddProjectStoryMocks().calls.find((entry) => entry.name === name);
  if (!call) {
    throw new Error(`No ${name} call was made`);
  }
  return call.payload;
}

export interface AddProjectStoryHarnessProps {
  readonly activeProjectCwd?: string | null;
  readonly cloneJobPollIntervalMs?: number;
  readonly initialMachineId?: string;
  readonly mockOptions?: AddProjectStoryMockOptions;
  readonly slowOperationNoticeMs?: number;
}

export function AddProjectStoryHarness({
  activeProjectCwd = null,
  cloneJobPollIntervalMs = 10,
  initialMachineId,
  mockOptions,
  slowOperationNoticeMs = 8000,
}: AddProjectStoryHarnessProps) {
  const mocks = useMemo(() => {
    const created = createAddProjectStoryMocks(mockOptions);
    latestStoryMocks = created;
    return created;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [isOpen, setIsOpen] = useState(true);
  const [addedProjects, setAddedProjects] = useState<readonly AddProjectAddResult[]>([]);
  const [settingsProvider, setSettingsProvider] = useState<AddProjectProviderId | null>(null);

  return (
    <div
      className='flex h-screen w-screen flex-col gap-2 bg-background p-4 text-sm text-muted-foreground'
      data-add-project-story-added={addedProjects.length}
      data-add-project-story-open={isOpen ? 'true' : 'false'}
      data-add-project-story-settings-provider={settingsProvider ?? ''}
    >
      <p>Add-project dialog harness.</p>
      {addedProjects.map((project) => (
        <p data-add-project-story-added-path={project.path} key={project.path}>
          Added {project.path} on {project.machineId}
        </p>
      ))}
      {isOpen ? null : (
        <button
          className='w-fit border border-border px-2 py-1 text-foreground'
          data-add-project-story-reopen=''
          onClick={() => setIsOpen(true)}
          type='button'
        >
          Reopen
        </button>
      )}
      <AddProjectModal
        {...mocks}
        activeProjectCwd={activeProjectCwd}
        cloneJobPollIntervalMs={cloneJobPollIntervalMs}
        initialMachineId={initialMachineId}
        isOpen={isOpen}
        onClose={() => setIsOpen(false)}
        onOpenSourceControlSettings={(provider) => setSettingsProvider(provider)}
        onProjectAdded={(result) => setAddedProjects((current) => [...current, result])}
        slowOperationNoticeMs={slowOperationNoticeMs}
      />
    </div>
  );
}
