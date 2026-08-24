/**
 * Static Storybook mockups exploring how to make the sidebar's five grouping
 * levels visually distinct:
 *   1. Sections & remote machines (Quick / Projects / Kubuntu / …)
 *   2. Project groups (Personal / GX / ShortPoint)
 *   3. Projects (Ghostex / maddada-com)
 *   4. Row kinds ("Browser tabs" / "Sessions")
 *   5. Browser tabs & agent session rows
 *
 * Purely presentational with fake data — no app wiring. See
 * hierarchy-mockups.css for the three design directions.
 */
import type { CSSProperties } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import './hierarchy-mockups.css';
import {
  AgentIcon,
  Chevron,
  DiffStats,
  MOCK_SECTIONS,
  StatusTrailing,
  type MockProject,
  type MockRow,
  type MockSection,
} from './hierarchy-mockup-data';

function rowClass(row: MockRow): string {
  const classes = ['hxm-row'];
  if (row.status === 'selected') classes.push('hxm-selected');
  if (row.status === 'idle') classes.push('hxm-idle');
  return classes.join(' ');
}

function Row({ row, browserColor }: { row: MockRow; browserColor?: string }) {
  return (
    <div className={rowClass(row)}>
      <AgentIcon row={row} browserColor={browserColor} />
      <span className='hxm-row-title'>{row.title}</span>
      <StatusTrailing status={row.status} />
    </div>
  );
}

function Titlebar() {
  return (
    <div className='hxm-titlebar'>
      <span>Search</span>
      <span className='hxm-titlebar-burger'>☰</span>
    </div>
  );
}

/* ------------------------- variant A: typographic ladder ------------------------- */

function ProjectA({ project }: { project: MockProject }) {
  return (
    <div>
      <div className='hxm-a-project-head'>
        <span className='hxm-a-project-name'>{project.name}</span>
        <DiffStats additions={project.additions} deletions={project.deletions} />
      </div>
      {project.browserTabs.length > 0 && (
        <>
          <div className='hxm-a-kind-label'>Browser tabs</div>
          {project.browserTabs.map((row) => (
            <Row key={row.title} row={row} />
          ))}
        </>
      )}
      {project.sessions.length > 0 && (
        <>
          <div className='hxm-a-kind-label'>Sessions</div>
          {project.sessions.map((row) => (
            <Row key={row.title} row={row} />
          ))}
        </>
      )}
    </div>
  );
}

function SectionA({ section }: { section: MockSection }) {
  const offline = section.machineStatus === 'disconnected';
  return (
    <div className='hxm-a-section'>
      <div className={`hxm-a-section-head${offline ? ' hxm-offline-head' : ''}`}>
        {section.kind === 'machine' && <span className={`hxm-machine-dot${offline ? ' hxm-offline' : ''}`} />}
        <span>{section.name}</span>
        {section.kind === 'machine' && <span className='hxm-a-machine-kind'>remote</span>}
      </div>
      {section.quickRows?.map((row) => (
        <Row key={row.title} row={row} />
      ))}
      {section.groups?.map((group) => (
        <div key={group.name}>
          <div className='hxm-a-group-head'>
            <Chevron collapsed={group.collapsed} />
            <span className='hxm-a-group-name'>{group.name}</span>
            {group.runningCount ? <span className='hxm-count-badge'>{group.runningCount}</span> : null}
          </div>
          {!group.collapsed && group.projects.map((project) => <ProjectA key={project.name} project={project} />)}
        </div>
      ))}
    </div>
  );
}

function MockupA() {
  return (
    <div className='hxm-shell hxm-a'>
      <Titlebar />
      {MOCK_SECTIONS.map((section) => (
        <SectionA key={section.name} section={section} />
      ))}
    </div>
  );
}

/* --------------------------- variant B: layered panels --------------------------- */

function ProjectB({ project }: { project: MockProject }) {
  return (
    <div className='hxm-b-project-card'>
      <div className='hxm-b-project-head'>
        <span className='hxm-b-project-name'>{project.name}</span>
        <DiffStats additions={project.additions} deletions={project.deletions} />
      </div>
      {project.browserTabs.length > 0 && (
        <>
          <div className='hxm-b-kind-label'>Browser tabs</div>
          {project.browserTabs.map((row) => (
            <Row key={row.title} row={row} />
          ))}
        </>
      )}
      {project.sessions.length > 0 && (
        <>
          <div className='hxm-b-kind-label'>Sessions</div>
          {project.sessions.map((row) => (
            <Row key={row.title} row={row} />
          ))}
        </>
      )}
    </div>
  );
}

function SectionB({ section }: { section: MockSection }) {
  const offline = section.machineStatus === 'disconnected';
  return (
    <div>
      {section.kind === 'machine' ? (
        <div className={`hxm-b-machine-head${offline ? ' hxm-offline-head' : ''}`}>
          <span className={`hxm-machine-dot${offline ? ' hxm-offline' : ''}`} />
          <span>{section.name}</span>
          <span className='hxm-b-machine-kind'>remote</span>
        </div>
      ) : (
        <div className='hxm-b-section-label'>{section.name}</div>
      )}
      {section.quickRows && (
        <div className='hxm-b-quick-rows'>
          {section.quickRows.map((row) => (
            <Row key={row.title} row={row} />
          ))}
        </div>
      )}
      {section.groups?.map((group) => (
        <div key={group.name} className='hxm-b-group-panel'>
          <div className='hxm-b-group-head'>
            <Chevron collapsed={group.collapsed} />
            <span className='hxm-b-group-name'>{group.name}</span>
            {group.runningCount ? <span className='hxm-count-badge'>{group.runningCount}</span> : null}
          </div>
          {!group.collapsed && group.projects.map((project) => <ProjectB key={project.name} project={project} />)}
        </div>
      ))}
    </div>
  );
}

function MockupB() {
  return (
    <div className='hxm-shell hxm-b'>
      <Titlebar />
      {MOCK_SECTIONS.map((section) => (
        <SectionB key={section.name} section={section} />
      ))}
    </div>
  );
}

/* ---------------------------- variant C: accent rails ---------------------------- */

const MACHINE_ACCENTS: Record<string, { accent: string; soft: string }> = {
  Quick: { accent: '#9aa3b0', soft: 'rgba(154, 163, 176, 0.35)' },
  Projects: { accent: '#6da2e8', soft: 'rgba(109, 162, 232, 0.4)' },
  Kubuntu: { accent: '#e5a158', soft: 'rgba(229, 161, 88, 0.45)' },
  'hetzner-01': { accent: '#8f7ee7', soft: 'rgba(143, 126, 231, 0.3)' },
};

function ProjectC({ project }: { project: MockProject }) {
  return (
    <div>
      <div className='hxm-c-project-head'>
        <span className='hxm-c-project-name'>{project.name}</span>
        <DiffStats additions={project.additions} deletions={project.deletions} />
      </div>
      {project.browserTabs.length > 0 && (
        <>
          <div className='hxm-c-kind-chip hxm-c-chip-browser'>Browser tabs</div>
          {project.browserTabs.map((row) => (
            <Row key={row.title} row={row} browserColor='#6da2e8' />
          ))}
        </>
      )}
      {project.sessions.length > 0 && (
        <>
          <div className='hxm-c-kind-chip hxm-c-chip-sessions'>Sessions</div>
          {project.sessions.map((row) => (
            <Row key={row.title} row={row} />
          ))}
        </>
      )}
    </div>
  );
}

function SectionC({ section }: { section: MockSection }) {
  const offline = section.machineStatus === 'disconnected';
  const accents = MACHINE_ACCENTS[section.name] ?? MACHINE_ACCENTS.Quick;
  const noRail = section.kind === 'quick' || (section.groups?.length ?? 0) === 0;
  return (
    <div
      className={`hxm-c-section${noRail ? ' hxm-c-no-rail' : ''}${offline ? ' hxm-c-offline' : ''}`}
      style={{ '--hxm-accent': accents.accent, '--hxm-accent-soft': accents.soft } as CSSProperties}
    >
      <div className='hxm-c-section-head'>
        {section.kind === 'machine' && <span className={`hxm-machine-dot${offline ? ' hxm-offline' : ''}`} />}
        <span>{section.name}</span>
      </div>
      {section.quickRows?.map((row) => (
        <Row key={row.title} row={row} />
      ))}
      {section.groups?.map((group) => (
        <div key={group.name}>
          <div className='hxm-c-group-head'>
            <Chevron collapsed={group.collapsed} />
            <span className='hxm-c-group-name'>{group.name}</span>
            {group.runningCount ? <span className='hxm-count-badge'>{group.runningCount}</span> : null}
          </div>
          {!group.collapsed && group.projects.map((project) => <ProjectC key={project.name} project={project} />)}
        </div>
      ))}
    </div>
  );
}

function MockupC() {
  return (
    <div className='hxm-shell hxm-c'>
      <Titlebar />
      {MOCK_SECTIONS.map((section) => (
        <SectionC key={section.name} section={section} />
      ))}
    </div>
  );
}

/* ----------------------------------- stories ----------------------------------- */

const meta = {
  title: 'Sidebar/Hierarchy Mockups',
  parameters: {
    layout: 'fullscreen',
  },
} satisfies Meta;

export default meta;

type Story = StoryObj<typeof meta>;

export const ATypographicLadder: Story = {
  name: 'A — Typographic Ladder',
  render: () => (
    <div className='hxm-backdrop'>
      <div>
        <p className='hxm-variant-title'>A — Typographic ladder</p>
        <MockupA />
      </div>
    </div>
  ),
};

export const BLayeredPanels: Story = {
  name: 'B — Layered Panels',
  render: () => (
    <div className='hxm-backdrop'>
      <div>
        <p className='hxm-variant-title'>B — Layered panels</p>
        <MockupB />
      </div>
    </div>
  ),
};

export const CAccentRails: Story = {
  name: 'C — Accent Rails',
  render: () => (
    <div className='hxm-backdrop'>
      <div>
        <p className='hxm-variant-title'>C — Accent rails</p>
        <MockupC />
      </div>
    </div>
  ),
};

export const SideBySide: Story = {
  name: 'All Three Side By Side',
  render: () => (
    <div className='hxm-backdrop'>
      <div>
        <p className='hxm-variant-title'>A — Typographic ladder</p>
        <MockupA />
      </div>
      <div>
        <p className='hxm-variant-title'>B — Layered panels</p>
        <MockupB />
      </div>
      <div>
        <p className='hxm-variant-title'>C — Accent rails</p>
        <MockupC />
      </div>
    </div>
  ),
};
