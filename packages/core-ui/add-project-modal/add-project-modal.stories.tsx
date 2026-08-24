import type { Meta, StoryObj } from '@storybook/react-vite';
import { ADD_PROJECT_STORY_LOCAL_MACHINE, ADD_PROJECT_STORY_REMOTE_MACHINE } from './add-project-modal-mocks';
import { AddProjectStoryHarness } from './add-project-modal.story-harness';

/*
 * CDXC:AddProject 2026-07-30:
 * Standalone visual stories for the shared add-project dialog. Every server
 * round trip is a mocked callback over an in-memory fixture directory tree, so
 * these run with no gxserver, no gpui bridge, and no web runtime.
 */

const meta = {
  title: 'Modals/Add Project',
  component: AddProjectStoryHarness,
} satisfies Meta<typeof AddProjectStoryHarness>;

export default meta;

type Story = StoryObj<typeof meta>;

/** Single machine: the dialog opens straight on the Sources step. */
export const Sources: Story = {
  args: {},
};

/** Two machines: the machine step is shown first. */
export const MachineStep: Story = {
  args: {
    mockOptions: {
      machines: [ADD_PROJECT_STORY_LOCAL_MACHINE, ADD_PROJECT_STORY_REMOTE_MACHINE],
    },
  },
};

/** A remote entry point preselects its machine and skips the machine step. */
export const PreselectedRemoteMachine: Story = {
  args: {
    initialMachineId: ADD_PROJECT_STORY_REMOTE_MACHINE.machineId,
    mockOptions: {
      machines: [ADD_PROJECT_STORY_LOCAL_MACHINE, ADD_PROJECT_STORY_REMOTE_MACHINE],
    },
  },
};

/** Discovery failed entirely: every provider renders with the Setup Required treatment. */
export const NoProvidersReady: Story = {
  args: {
    mockOptions: { discoveryUnavailable: true },
  },
};

/** Slow machine: the pending call trips the "still working" notice quickly. */
export const SlowMachine: Story = {
  args: {
    mockOptions: { latencyMs: 1200 },
    slowOperationNoticeMs: 400,
  },
};

/** Every add fails: the inline error region is persistent, not a transient list line. */
export const AddAlwaysFails: Story = {
  args: {
    mockOptions: {
      addProjectError: 'Workspace root is not a directory: /Users/story/dev/notes.md',
    },
  },
};
