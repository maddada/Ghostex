/**
 * Fake-but-realistic data + tiny shared primitives for the sidebar hierarchy
 * redesign mockups. Purely presentational: no app imports, no runtime wiring.
 */
import type { ReactNode } from 'react';

export type MockRowKind = 'browser' | 'session';
export type MockAgent = 'claude' | 'openai' | 'codex' | 'terminal';
export type MockStatus = 'running' | 'selected' | 'idle' | 'done' | 'none';

export interface MockRow {
  kind: MockRowKind;
  agent?: MockAgent;
  title: string;
  status?: MockStatus;
}

export interface MockProject {
  name: string;
  additions?: number;
  deletions?: number;
  browserTabs: MockRow[];
  sessions: MockRow[];
}

export interface MockGroup {
  name: string;
  collapsed?: boolean;
  runningCount?: number;
  projects: MockProject[];
}

export interface MockSection {
  kind: 'quick' | 'projects' | 'machine';
  name: string;
  machineStatus?: 'connected' | 'disconnected';
  quickRows?: MockRow[];
  groups?: MockGroup[];
}

export const MOCK_SECTIONS: MockSection[] = [
  {
    kind: 'quick',
    name: 'Quick',
    quickRows: [
      { kind: 'session', agent: 'openai', title: 'Refactor pricing notes', status: 'none' },
      { kind: 'session', agent: 'claude', title: 'Trip brainstorm', status: 'none' },
    ],
  },
  {
    kind: 'projects',
    name: 'Projects',
    groups: [
      { name: 'Personal', collapsed: true, runningCount: 1, projects: [] },
      {
        name: 'GX',
        projects: [
          {
            name: 'Ghostex',
            additions: 139,
            deletions: 10,
            browserTabs: [
              { kind: 'browser', title: 'Google' },
              { kind: 'browser', title: 'localhost:6006 — Storybook' },
            ],
            sessions: [
              { kind: 'session', agent: 'claude', title: 'sidebar-hierarchy-redesign', status: 'running' },
              { kind: 'session', agent: 'openai', title: 'GPUI Remote Connection Errors', status: 'running' },
              { kind: 'session', agent: 'openai', title: 'macOS update button missing', status: 'selected' },
              { kind: 'session', agent: 'codex', title: 'GPUI Tab Input Bug', status: 'none' },
              { kind: 'session', agent: 'openai', title: 'Ghostex Mobile Handover', status: 'idle' },
            ],
          },
          {
            name: 'maddada-com',
            browserTabs: [],
            sessions: [{ kind: 'session', agent: 'claude', title: 'Deploy landing page', status: 'none' }],
          },
        ],
      },
      { name: 'ShortPoint', collapsed: true, projects: [] },
    ],
  },
  {
    kind: 'machine',
    name: 'Kubuntu',
    machineStatus: 'connected',
    groups: [
      {
        name: 'GX',
        projects: [
          {
            name: 'Ghostex',
            additions: 181,
            deletions: 9,
            browserTabs: [],
            sessions: [
              { kind: 'session', agent: 'terminal', title: 'madda@kubuntu-vm: ~/dev/Ghostex', status: 'none' },
              { kind: 'session', agent: 'claude', title: 'Remote build verification', status: 'running' },
            ],
          },
        ],
      },
    ],
  },
  {
    kind: 'machine',
    name: 'hetzner-01',
    machineStatus: 'disconnected',
    groups: [],
  },
];

/* ---------- tiny inline icons (self-contained, no asset deps) ---------- */

export function GlobeIcon({ color = '#6da2e8' }: { color?: string }) {
  return (
    <svg className='hxm-icon' viewBox='0 0 16 16' aria-hidden>
      <circle cx='8' cy='8' r='6.1' fill='none' stroke={color} strokeWidth='1.2' />
      <ellipse cx='8' cy='8' rx='2.7' ry='6.1' fill='none' stroke={color} strokeWidth='1' />
      <line x1='1.9' y1='8' x2='14.1' y2='8' stroke={color} strokeWidth='1' />
    </svg>
  );
}

export function ClaudeIcon({ color = '#d97757' }: { color?: string }) {
  return (
    <svg className='hxm-icon' viewBox='0 0 16 16' aria-hidden>
      {Array.from({ length: 8 }).map((_, i) => (
        <line
          key={i}
          x1='8'
          y1='8'
          x2={8 + 5.6 * Math.cos((i * Math.PI) / 4)}
          y2={8 + 5.6 * Math.sin((i * Math.PI) / 4)}
          stroke={color}
          strokeWidth='1.5'
          strokeLinecap='round'
        />
      ))}
    </svg>
  );
}

export function OpenAiIcon({ color = '#b6bcc6' }: { color?: string }) {
  return (
    <svg className='hxm-icon' viewBox='0 0 16 16' aria-hidden>
      {Array.from({ length: 6 }).map((_, i) => (
        <path
          key={i}
          d='M 8 2.2 A 5.8 5.8 0 0 1 13.8 8'
          fill='none'
          stroke={color}
          strokeWidth='1.3'
          strokeLinecap='round'
          transform={`rotate(${i * 60} 8 8)`}
        />
      ))}
    </svg>
  );
}

export function CodexIcon({ color = '#8f7ee7' }: { color?: string }) {
  return (
    <svg className='hxm-icon' viewBox='0 0 16 16' aria-hidden>
      <rect x='3.4' y='2.6' width='3.4' height='10.8' rx='1.4' fill='none' stroke={color} strokeWidth='1.3' />
      <rect x='9.2' y='2.6' width='3.4' height='10.8' rx='1.4' fill='none' stroke={color} strokeWidth='1.3' />
    </svg>
  );
}

export function TerminalIcon({ color = '#9aa3af' }: { color?: string }) {
  return (
    <svg className='hxm-icon' viewBox='0 0 16 16' aria-hidden>
      <rect x='1.8' y='2.8' width='12.4' height='10.4' rx='2' fill='none' stroke={color} strokeWidth='1.2' />
      <path
        d='M 4.6 6.2 L 7 8.2 L 4.6 10.2'
        fill='none'
        stroke={color}
        strokeWidth='1.2'
        strokeLinecap='round'
        strokeLinejoin='round'
      />
    </svg>
  );
}

export function AgentIcon({ row, browserColor }: { row: MockRow; browserColor?: string }) {
  if (row.kind === 'browser') return <GlobeIcon color={browserColor} />;
  switch (row.agent) {
    case 'claude':
      return <ClaudeIcon />;
    case 'codex':
      return <CodexIcon />;
    case 'terminal':
      return <TerminalIcon />;
    default:
      return <OpenAiIcon />;
  }
}

export function StatusTrailing({ status }: { status?: MockStatus }): ReactNode {
  if (status === 'running') return <span className='hxm-spinner' aria-label='running' />;
  if (status === 'done') return <span className='hxm-done-dot' aria-label='done' />;
  return null;
}

export function DiffStats({ additions, deletions }: { additions?: number; deletions?: number }) {
  if (additions == null && deletions == null) return null;
  return (
    <span className='hxm-diff'>
      <span className='hxm-diff-add'>+{additions ?? 0}</span>
      <span className='hxm-diff-del'>-{deletions ?? 0}</span>
    </span>
  );
}

export function Chevron({ collapsed }: { collapsed?: boolean }) {
  return (
    <svg className={`hxm-chevron${collapsed ? ' hxm-chevron-collapsed' : ''}`} viewBox='0 0 16 16' aria-hidden>
      <path
        d='M 5 3.5 L 11 8 L 5 12.5'
        fill='none'
        stroke='currentColor'
        strokeWidth='1.6'
        strokeLinecap='round'
        strokeLinejoin='round'
      />
    </svg>
  );
}
