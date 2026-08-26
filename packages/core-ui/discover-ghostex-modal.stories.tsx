import type { Meta, StoryObj } from '@storybook/react-vite';
import { DiscoverGhostexModal } from './discover-ghostex-modal';

const meta = {
  title: 'Modals/Onboarding/Highlighted Features',
  parameters: {
    layout: 'fullscreen',
  },
  render: () => (
    <div className='first-launch-setup-story-frame'>
      <DiscoverGhostexModal isOpen onClose={() => undefined} theme='dark-blue' />
    </div>
  ),
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {};
