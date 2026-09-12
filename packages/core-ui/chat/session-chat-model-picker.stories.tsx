import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import catalogJson from '@/agent-model-catalog.json';
import { parseAgentModelCatalog } from '@/packages/shared/agent-model-catalog';
import { createModelPickerRequest } from './session-chat-model-picker-request';
import {
  SessionChatModelPicker,
  type ModelPickerProvider,
  type ModelPickerSelection,
} from './session-chat-model-picker';

const catalog = parseAgentModelCatalog(catalogJson)!;

function PickerPreview({ provider, model, effort }: { provider: ModelPickerProvider; model: string; effort: string }) {
  const [container, setContainer] = useState<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(true);
  const [selection, setSelection] = useState<ModelPickerSelection>({ model, effort });
  const request = createModelPickerRequest(catalog, provider, selection.model, selection.effort)!;

  return (
    <div
      ref={setContainer}
      className='ghostex-session-chat-scope'
      style={{ position: 'relative', height: '100vh', background: '#08090c' }}
    >
      <button type='button' onClick={() => setOpen(true)}>
        Open model picker
      </button>
      {container && open && (
        <SessionChatModelPicker
          request={request}
          container={container}
          onSave={setSelection}
          onClose={() => setOpen(false)}
        />
      )}
    </div>
  );
}

const meta = {
  title: 'Chat/Model Picker',
  component: PickerPreview,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof PickerPreview>;

export default meta;
type Story = StoryObj<typeof meta>;

// The preview imports the full app stylesheet, including its generic disabled-button opacity.
export const DisabledEfforts: Story = {
  args: { provider: 'cursor', model: 'claude-fable-5-1', effort: 'low' },
};

export const AllEffortsDisabled: Story = {
  args: { provider: 'claude', model: 'haiku', effort: '' },
};

export const DisabledUltra: Story = {
  args: { provider: 'codex', model: 'gpt-5.6-luna', effort: 'high' },
};
