import type { ComponentType } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { IconBriefcase, IconDots, IconFlask, IconHome, IconRocket, IconStack2 } from '@tabler/icons-react';
import { SpaceSessionStatusCounts } from './space-filter-row';
import './space-session-status-designs.css';

type SpaceExample = {
  attentionCount: number;
  icon: ComponentType<{ 'aria-hidden'?: boolean | 'true'; className?: string; size?: number; stroke?: number }>;
  name: string;
  selected?: boolean;
  workingCount: number;
};

const baseSpaces: SpaceExample[] = [
  { attentionCount: 2, icon: IconBriefcase, name: 'Work', selected: true, workingCount: 1 },
  { attentionCount: 1, icon: IconHome, name: 'Personal', workingCount: 2 },
  { attentionCount: 0, icon: IconFlask, name: 'Research', workingCount: 3 },
  { attentionCount: 2, icon: IconRocket, name: 'Launch', workingCount: 0 },
  { attentionCount: 0, icon: IconStack2, name: 'Archive', workingCount: 0 },
];

function SpaceButton({ space }: { space: SpaceExample }) {
  const Icon = space.icon;
  const showStatus = space.workingCount > 0 || space.attentionCount > 0;
  const statusLabel = [
    space.workingCount > 0 ? `${space.workingCount} working` : '',
    space.attentionCount > 0 ? `${space.attentionCount} need attention` : '',
  ]
    .filter(Boolean)
    .join(', ');

  return (
    <span className='ssid-space-item'>
      <button
        aria-label={`${space.name}${space.selected ? ', selected' : showStatus ? `, ${statusLabel}` : ''}`}
        aria-pressed={space.selected === true}
        className='ssid-space-button'
        data-has-status={String(showStatus)}
        type='button'
      >
        <Icon aria-hidden='true' className='ssid-space-icon' size={16} stroke={1.8} />
        <SpaceSessionStatusCounts summary={space} />
      </button>
      <span className='ssid-space-name'>{space.name}</span>
    </span>
  );
}

function SpaceStrip({ spaces }: { spaces: readonly SpaceExample[] }) {
  return (
    <div className='sidebar-reference-layout' data-reference-sidebar='true' style={{ display: 'contents' }}>
      <div className='ssid-space-strip' aria-label='Example Space switcher'>
        {spaces.map((space) => (
          <SpaceButton key={space.name} space={space} />
        ))}
        <span className='ssid-more-space' aria-label='More Spaces'>
          <IconDots aria-hidden='true' size={16} stroke={2} />
        </span>
      </div>
    </div>
  );
}

function SidebarPreview({
  attentionCount,
  narrow = false,
  workingCount,
}: {
  attentionCount: number;
  narrow?: boolean;
  workingCount: number;
}) {
  const spaces = baseSpaces.map((space, index) => (index === 1 ? { ...space, attentionCount, workingCount } : space));

  return (
    <section className='ssid-preview-column' data-narrow={String(narrow)}>
      <p className='ssid-preview-label'>{narrow ? 'Narrow sidebar' : 'Standard sidebar'}</p>
      <div className='ssid-sidebar-preview'>
        <SpaceStrip spaces={spaces} />
        <div className='ssid-project-placeholder' aria-hidden='true'>
          <span>
            <i /> Ghostex
          </span>
          <span>
            <i /> Extensions
          </span>
          <span>
            <i /> gxserver
          </span>
        </div>
      </div>
      <p className='ssid-preview-explanation'>
        Personal: {workingCount} working, {attentionCount} attention. Research: 3 working. Launch: 2 attention.
      </p>
    </section>
  );
}

function CenteredNumbersStudy({
  attentionCount = 1,
  workingCount = 2,
}: {
  attentionCount?: number;
  workingCount?: number;
}) {
  return (
    <main className='ssid-study'>
      <div className='ssid-study-content'>
        <header className='ssid-study-header'>
          <div>
            <p className='ssid-eyebrow'>GHOSTEX / SPACES / OFF-SCREEN ACTIVITY</p>
            <h1>Colored counts centered inside Space icons</h1>
            <p>
              Every Space keeps its usual icon and layers its aggregate counts into the icon's exact center, including
              the active Space. Working is amber, attention is light blue, and the JetBrains Mono ExtraBold values use a
              small gap.
            </p>
          </div>
          <div className='ssid-legend' aria-label='Status legend'>
            <span className='ssid-legend-working'>
              2 <small>Working</small>
            </span>
            <span className='ssid-legend-attention'>
              1 <small>Needs attention</small>
            </span>
          </div>
        </header>

        <div className='ssid-preview-grid'>
          <SidebarPreview attentionCount={attentionCount} workingCount={workingCount} />
          <SidebarPreview attentionCount={attentionCount} narrow workingCount={workingCount} />
        </div>

        <footer className='ssid-footnote'>
          The selected Work Space keeps its counts visible along with every inactive Space. The Personal Space uses the
          Storybook count controls, and every icon stays fully visible.
        </footer>
      </div>
    </main>
  );
}

const meta = {
  title: 'Sidebar/Space session status designs',
  component: CenteredNumbersStudy,
  parameters: { layout: 'fullscreen' },
  args: { attentionCount: 1, workingCount: 2 },
  argTypes: {
    attentionCount: { control: { max: 9, min: 0, step: 1, type: 'range' } },
    workingCount: { control: { max: 9, min: 0, step: 1, type: 'range' } },
  },
} satisfies Meta<typeof CenteredNumbersStudy>;

export default meta;
type Story = StoryObj<typeof meta>;

export const CompareAll: Story = { name: 'Centered numbers' };
