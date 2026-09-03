import type { Meta, StoryObj } from '@storybook/react-vite';
import { DelayedSendModal, type DelayedSendModalProps } from './delayed-send-modal';

/**
 * CDXC:AppModal 2026-08-24:
 * The Session Automations dialog (opened from the "Delayed Actions" menu item)
 * is a one-shot native fit-height modal in the desktop app, so it has no
 * in-app inspection surface. These stories mount it against the Codex-style
 * #0e0e0e page color so the restyled surfaces, 32px controls, and accent
 * status text can be reviewed without launching the app.
 */
const baseProps: DelayedSendModalProps = {
  agentIcon: 'codex',
  closeAfterDoneActive: false,
  isOpen: true,
  onCancel: () => undefined,
  onCancelTimer: () => undefined,
  onConfirm: () => undefined,
  onToggleCloseAfterDone: () => undefined,
  sessionTitle: 'Restyle Session Automations',
  supportsSendWhenAgentStops: true,
  supportsSendWhenAllProjectSessionsStop: true,
};

function DelayedSendModalStory(props: Partial<DelayedSendModalProps>) {
  return (
    <div
      style={{
        background: '#0e0e0e',
        height: '100vh',
        width: '100vw',
      }}
    >
      <DelayedSendModal {...baseProps} {...props} />
    </div>
  );
}

const meta = {
  title: 'Modals/App Host/Delayed Actions',
  parameters: {
    layout: 'fullscreen',
  },
  render: () => <DelayedSendModalStory />,
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * An armed timer: the Send Enter summary switches to the accent status color
 * and the duration fields are prefilled from the remaining deadline.
 */
export const ActiveTimer: Story = {
  render: () => (
    <DelayedSendModalStory
      closeAfterDoneActive
      delayedSendDeadlineAt={new Date(Date.now() + 95 * 60 * 1000).toISOString()}
      delayedSendRemainingLabel='1h 35m'
    />
  ),
};

/**
 * A status trigger replaces the duration grid with its explanation copy, which
 * is the other height the one-shot native window has to measure.
 */
export const AgentFinishesTrigger: Story = {
  render: () => <DelayedSendModalStory delayedSendRemainingLabel={undefined} sendWhenAgentStopsActive />,
};
