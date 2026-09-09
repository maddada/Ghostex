import { isRecord } from './primitives';

export type ProjectViewSource = {
  kind: 'website' | 'dev-server' | 'report';
  destination: 'fixed' | 'project' | 'github-issues' | 'github-actions' | 'github-pulls';
  discovery: 'command' | 'storybook';
  command: string;
  cwd: string;
  readinessUrl: string;
  reportDirectory: string;
  entry: string;
  timeoutSeconds: number;
};
export type ProjectViewBinding = {
  url?: string;
  repositoryUrl?: string;
  command?: string;
  cwd?: string;
  readinessUrl?: string;
  startOnProjectOpen?: boolean;
  inherit?: boolean;
};
export type ProjectViewSpace = { sectionKey: string; spaceId: string; name: string };
export type ProjectViewOptions = {
  source?: ProjectViewSource;
  availability?: 'all' | 'matching' | 'selected' | 'spaces';
  spaceRefs?: Pick<ProjectViewSpace, 'sectionKey' | 'spaceId'>[];
  projectIds?: string[];
  projectBindings?: Record<string, ProjectViewBinding>;
  templateId?: string;
};
export type ProjectViewTemplate = ProjectViewOptions & { id: string; name: string; url: string };

export const DEFAULT_PROJECT_VIEW_SOURCE: ProjectViewSource = {
  kind: 'website',
  destination: 'fixed',
  discovery: 'command',
  command: '',
  cwd: '.',
  readinessUrl: '',
  reportDirectory: '',
  entry: 'index.html',
  timeoutSeconds: 60,
};
const text = (value: unknown, max = 8192) => (typeof value === 'string' ? value.trim().slice(0, max) : '');
export function normalizeProjectViewOptions(value: Record<string, unknown>): ProjectViewOptions {
  if (!isRecord(value.source)) return {};
  const raw = value.source;
  const source: ProjectViewSource = {
    kind: raw.kind === 'dev-server' || raw.kind === 'report' ? raw.kind : 'website',
    destination: ['project', 'github-issues', 'github-actions', 'github-pulls'].includes(String(raw.destination))
      ? (raw.destination as ProjectViewSource['destination'])
      : 'fixed',
    discovery: raw.discovery === 'storybook' ? 'storybook' : 'command',
    command: text(raw.command),
    cwd: text(raw.cwd) || '.',
    readinessUrl: text(raw.readinessUrl),
    reportDirectory: text(raw.reportDirectory),
    entry: text(raw.entry) || 'index.html',
    timeoutSeconds:
      typeof raw.timeoutSeconds === 'number' && Number.isFinite(raw.timeoutSeconds)
        ? Math.min(600, Math.max(5, Math.round(raw.timeoutSeconds)))
        : 60,
  };
  const projectBindings: Record<string, ProjectViewBinding> = {};
  if (isRecord(value.projectBindings))
    for (const [id, binding] of Object.entries(value.projectBindings)) {
      if (!isRecord(binding) || ['__proto__', 'constructor', 'prototype'].includes(id)) continue;
      projectBindings[id] = {
        url: text(binding.url),
        repositoryUrl: text(binding.repositoryUrl),
        command: text(binding.command),
        cwd: text(binding.cwd),
        readinessUrl: text(binding.readinessUrl),
        ...(typeof binding.startOnProjectOpen === 'boolean' ? { startOnProjectOpen: binding.startOnProjectOpen } : {}),
        inherit: binding.inherit !== false,
      };
    }
  return {
    source,
    availability:
      value.availability === 'selected' || value.availability === 'all' || value.availability === 'spaces'
        ? value.availability
        : 'matching',
    projectIds: Array.isArray(value.projectIds)
      ? [...new Set(value.projectIds.filter((id): id is string => typeof id === 'string'))]
      : [],
    spaceRefs: Array.isArray(value.spaceRefs)
      ? value.spaceRefs.flatMap((entry) => {
          if (!isRecord(entry)) return [];
          const sectionKey = text(entry.sectionKey, 256),
            spaceId = text(entry.spaceId, 256);
          return sectionKey && spaceId ? [{ sectionKey, spaceId }] : [];
        })
      : [],
    projectBindings,
    templateId: text(value.templateId, 128) || undefined,
  };
}

/**
 * CDXC:Extensions 2026-09-09 DECISION:
 * User approved Website, Dev server, and HTML report primitives with reusable templates and per-project values, including a Linear URL and detected Storybook command.
 * Existing fixed URL views keep their identity and ordering; templates create editable copies rather than live links.
 */
export const BUILTIN_PROJECT_VIEW_TEMPLATES: readonly ProjectViewTemplate[] = [
  {
    id: 'github-issues',
    name: 'GitHub Issues',
    url: '',
    availability: 'matching',
    source: { ...DEFAULT_PROJECT_VIEW_SOURCE, destination: 'github-issues' },
  },
  {
    id: 'github-actions',
    name: 'GitHub Actions',
    url: '',
    availability: 'matching',
    source: { ...DEFAULT_PROJECT_VIEW_SOURCE, destination: 'github-actions' },
  },
  {
    id: 'storybook',
    name: 'Storybook',
    url: '',
    availability: 'matching',
    source: { ...DEFAULT_PROJECT_VIEW_SOURCE, kind: 'dev-server', discovery: 'storybook' },
  },
  {
    id: 'linear',
    name: 'Linear',
    url: '',
    availability: 'matching',
    source: { ...DEFAULT_PROJECT_VIEW_SOURCE, destination: 'project' },
  },
];
export function normalizeProjectViewTemplates(value: unknown): ProjectViewTemplate[] {
  if (!Array.isArray(value)) return [];
  const ids = new Set<string>();
  return value.flatMap((entry) => {
    if (!isRecord(entry)) return [];
    const id = text(entry.id, 128),
      name = text(entry.name, 128);
    if (!id || !name || ids.has(id)) return [];
    ids.add(id);
    const options = normalizeProjectViewOptions(entry);
    return [
      {
        ...options,
        id,
        name,
        url: '',
        projectBindings: {},
        projectIds: [],
        spaceRefs: [],
        availability: 'matching' as const,
      },
    ];
  });
}
export function projectViewDescription(view: { url: string } & ProjectViewOptions): string {
  const source = view.source;
  if (!source) return view.url;
  if (source.kind === 'report') return `HTML report · ${source.reportDirectory || 'Choose an output directory'}`;
  if (source.kind === 'dev-server')
    return source.discovery === 'storybook'
      ? 'Dev server · Detect project Storybook script'
      : `Dev server · ${source.command || 'Set a project command'}`;
  if (source.destination === 'project') return 'Website · URL supplied by each project';
  if (source.destination.startsWith('github-')) return `Website · Current repository ${source.destination.slice(7)}`;
  return view.url || 'Website · Set a URL';
}

/** Space ids are scoped to their owning computer, just like the sidebar's space picker. */
export function projectViewSpaceOptions(state: unknown, sectionKey: string, computerName?: string): ProjectViewSpace[] {
  if (!isRecord(state) || !isRecord(state.spaces) || !Array.isArray(state.order)) return [];
  const spaces = state.spaces;
  return state.order.flatMap((id) => {
    if (typeof id !== 'string' || !isRecord(spaces[id])) return [];
    const name = text(spaces[id].name, 256);
    return name ? [{ sectionKey, spaceId: id, name: computerName ? `${name} (${computerName})` : name }] : [];
  });
}
