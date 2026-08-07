import { IconCopy, IconFolder, IconFolderOpen, IconRotateClockwise, IconTrash } from '@tabler/icons-react';
import { createPortal } from 'react-dom';
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import type { ExtensionToSidebarMessage, SidebarRecentProject } from '../shared/session-grid-contract';
import { resolveWorkspaceProjectIconDataUrl } from '../shared/workspace-project-appearance';
import { AppTooltip, TooltipProvider } from './app-tooltip';
import { SidebarCommandIconGlyph } from './sidebar-command-icon';
import { SidebarContextMenuPortal } from './sidebar-context-menu-portal';
import { QuickAccessSearchInput } from './quick-access-search-input';
import { QuickAccessHeader } from './quick-access-tabs';
import { TOOLTIP_DELAY_MS } from './tooltip-delay';
import type { WebviewApi } from './webview-api';

const RECENT_PROJECTS_INITIAL_LOAD_TIMEOUT_MS = 2_000;

type RecentProjectContextMenuPosition = {
  projectId: string;
  x: number;
  y: number;
};

type RecentProjectsDayGroup = {
  dayLabel: string;
  projects: SidebarRecentProject[];
};

export type RecentProjectsModalProps = {
  isOpen: boolean;
  machineId?: string;
  machineName?: string;
  onClose: () => void;
  onInitialLoadReady?: () => void;
  vscode: WebviewApi;
};

export type RecentProjectRowProps = {
  isContextMenuOpen: boolean;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>, projectId: string) => void;
  onRestore: (projectId: string) => void;
  project: SidebarRecentProject;
};

function parseRecentProjectClosedAt(value: string | undefined): number {
  if (!value) {
    return 0;
  }
  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function sortRecentProjectsByClosedAt(projects: readonly SidebarRecentProject[]): SidebarRecentProject[] {
  return [...projects].sort(
    (left, right) =>
      parseRecentProjectClosedAt(right.recentClosedAt) - parseRecentProjectClosedAt(left.recentClosedAt) ||
      left.title.localeCompare(right.title)
  );
}

function groupRecentProjectsByDay(projects: readonly SidebarRecentProject[]): RecentProjectsDayGroup[] {
  /*
   * CDXC:RecentProjects 2026-07-17-04:12:
   * Recent Projects groups by the day each project was closed, using the same
   * day-label format as the Reopen a Session modal. Rows are sorted most
   * recently closed first, so the newest day group leads and rows inside each
   * group run recent to oldest. Projects without a usable closed timestamp
   * collect in a trailing "Earlier" group instead of fabricating a date.
   */
  const formatter = new Intl.DateTimeFormat(undefined, {
    day: 'numeric',
    month: 'long',
    weekday: 'long',
    year: 'numeric',
  });
  const projectsByDay = new Map<string, SidebarRecentProject[]>();
  for (const project of sortRecentProjectsByClosedAt(projects)) {
    const timestamp = parseRecentProjectClosedAt(project.recentClosedAt);
    const dayLabel = timestamp === 0 ? 'Earlier' : formatter.format(new Date(timestamp));
    const grouped = projectsByDay.get(dayLabel);
    if (grouped) {
      grouped.push(project);
      continue;
    }
    projectsByDay.set(dayLabel, [project]);
  }
  return [...projectsByDay.entries()].map(([dayLabel, dayProjects]) => ({
    dayLabel,
    projects: dayProjects,
  }));
}

function filterRecentProjects(projects: readonly SidebarRecentProject[], query: string): SidebarRecentProject[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) {
    return [...projects];
  }
  const queryTerms = normalizedQuery.split(/\s+/u);
  return projects.filter((project) => {
    const searchableText = [project.title, project.path].join('\n').toLocaleLowerCase();
    return queryTerms.every((term) => searchableText.includes(term));
  });
}

function RecentProjectIcon({ project }: { project: SidebarRecentProject }) {
  const iconDataUrl = resolveWorkspaceProjectIconDataUrl(project);
  if (iconDataUrl) {
    return <img alt='' className='recent-projects-row-icon-image' draggable={false} src={iconDataUrl} />;
  }
  if (project.icon?.kind === 'tabler') {
    return <SidebarCommandIconGlyph color={project.icon.color} icon={project.icon.icon} size={16} stroke={1.8} />;
  }
  return <IconFolder aria-hidden='true' size={16} stroke={1.8} />;
}

export function RecentProjectRow({ isContextMenuOpen, onContextMenu, onRestore, project }: RecentProjectRowProps) {
  return (
    <AppTooltip content={project.path}>
      <button
        aria-label={`Restore recent ${project.title}`}
        className='recent-projects-row'
        data-context-menu-open={String(isContextMenuOpen)}
        onClick={() => onRestore(project.projectId)}
        onContextMenu={(event) => onContextMenu(event, project.projectId)}
        type='button'
      >
        <span aria-hidden='true' className='recent-projects-row-icon'>
          <RecentProjectIcon project={project} />
        </span>
        <span className='recent-projects-row-title'>{project.title}</span>
        <span aria-label={`${project.sessionCount} preserved sessions`} className='recent-projects-session-count'>
          {project.sessionCount}
        </span>
      </button>
    </AppTooltip>
  );
}

export function RecentProjectsModal({
  isOpen,
  machineId,
  onClose,
  onInitialLoadReady,
  vscode,
}: RecentProjectsModalProps) {
  const [recentProjects, setRecentProjects] = useState<SidebarRecentProject[]>();
  const [hasInitialLoadResolved, setHasInitialLoadResolved] = useState(false);
  const [hasInitialLoadTimedOut, setHasInitialLoadTimedOut] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [contextMenuPosition, setContextMenuPosition] = useState<RecentProjectContextMenuPosition>();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const canShowModal = isOpen && (hasInitialLoadResolved || hasInitialLoadTimedOut);
  const filteredProjects = useMemo(
    () => filterRecentProjects(recentProjects ?? [], searchQuery),
    [recentProjects, searchQuery]
  );
  const groupedProjects = useMemo(() => groupRecentProjectsByDay(filteredProjects), [filteredProjects]);

  const requestRecentProjects = useCallback(() => {
    vscode.postMessage({ machineId, type: 'requestRecentProjects' });
  }, [machineId, vscode]);

  const restoreRecentProject = useCallback(
    (projectId: string) => {
      setContextMenuPosition(undefined);
      vscode.postMessage({ projectId, type: 'restoreRecentProject' });
      onClose();
    },
    [onClose, vscode]
  );

  useEffect(() => {
    if (!isOpen) {
      setRecentProjects(undefined);
      setHasInitialLoadResolved(false);
      setHasInitialLoadTimedOut(false);
      setSearchQuery('');
      setContextMenuPosition(undefined);
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen || hasInitialLoadResolved) {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      setHasInitialLoadTimedOut(true);
      onInitialLoadReady?.();
    }, RECENT_PROJECTS_INITIAL_LOAD_TIMEOUT_MS);
    return () => window.clearTimeout(timeoutId);
  }, [hasInitialLoadResolved, isOpen, onInitialLoadReady]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const handleMessage = (event: MessageEvent<ExtensionToSidebarMessage>) => {
      if (event.data.type !== 'recentProjectsResult' || event.data.machineId !== machineId) {
        return;
      }
      setRecentProjects(event.data.recentProjects);
      setHasInitialLoadResolved(true);
      onInitialLoadReady?.();
    };
    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [isOpen, machineId, onInitialLoadReady]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    requestRecentProjects();
  }, [isOpen, requestRecentProjects]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return;
      }
      if (contextMenuPosition) {
        event.preventDefault();
        event.stopPropagation();
        setContextMenuPosition(undefined);
        return;
      }
      onClose();
    };
    document.addEventListener('keydown', handleKeyDown, true);
    return () => document.removeEventListener('keydown', handleKeyDown, true);
  }, [contextMenuPosition, isOpen, onClose]);

  useEffect(() => {
    if (!canShowModal) {
      return;
    }
    const timeoutId = window.setTimeout(() => searchInputRef.current?.focus(), 0);
    return () => window.clearTimeout(timeoutId);
  }, [canShowModal]);

  if (!isOpen) {
    return null;
  }

  return createPortal(
    <TooltipProvider delayDuration={TOOLTIP_DELAY_MS}>
      <div className='confirm-modal-root scroll-mask-y' role='presentation'>
        <button className='confirm-modal-backdrop' onClick={onClose} type='button' />
        <div
          aria-label='Ghostex Quick Access'
          aria-modal='true'
          className='confirm-modal ghostex-settings-shadcn previous-sessions-modal quick-access-surface recent-projects-modal scroll-mask-y'
          role='dialog'
        >
          <QuickAccessHeader activeTab='recentProjects' />
          <div className='previous-sessions-toolbar'>
            <QuickAccessSearchInput
              ariaLabel='Search recent projects'
              clearLabel='Clear Recent Projects search'
              inputRef={searchInputRef}
              placeholder='Search projects...'
              query={searchQuery}
              setQuery={setSearchQuery}
            />
          </div>
          <div className='previous-sessions-modal-body recent-projects-modal-body scroll-mask-y'>
            {!hasInitialLoadResolved ? (
              <div className='group-empty-state previous-sessions-empty-state'>Loading recent projects…</div>
            ) : filteredProjects.length > 0 ? (
              groupedProjects.map((group) => (
                <section className='previous-sessions-day-group' key={group.dayLabel}>
                  <div className='previous-sessions-day-label'>{group.dayLabel}</div>
                  <div className='recent-projects-modal-list'>
                    {group.projects.map((project) => (
                      <RecentProjectRow
                        isContextMenuOpen={contextMenuPosition?.projectId === project.projectId}
                        key={project.projectId}
                        onContextMenu={(event, projectId) => {
                          event.preventDefault();
                          event.stopPropagation();
                          setContextMenuPosition({
                            projectId,
                            x: event.clientX,
                            y: event.clientY,
                          });
                        }}
                        onRestore={restoreRecentProject}
                        project={project}
                      />
                    ))}
                  </div>
                </section>
              ))
            ) : (
              <div className='group-empty-state previous-sessions-empty-state'>
                {searchQuery.trim() ? 'No recent projects match that search.' : 'No recent projects yet.'}
              </div>
            )}
          </div>
          {contextMenuPosition ? (
            <SidebarContextMenuPortal
              menuStyle={{ left: `${contextMenuPosition.x}px`, top: `${contextMenuPosition.y}px` }}
              onDismiss={() => setContextMenuPosition(undefined)}
              vscode={vscode}
            >
              <button
                className='session-context-menu-item'
                onClick={() => restoreRecentProject(contextMenuPosition.projectId)}
                role='menuitem'
                type='button'
              >
                <IconRotateClockwise aria-hidden='true' className='session-context-menu-icon' size={14} />
                Restore
              </button>
              <button
                className='session-context-menu-item'
                onClick={() => {
                  vscode.postMessage({
                    projectId: contextMenuPosition.projectId,
                    type: 'copyRecentProjectPath',
                  });
                  setContextMenuPosition(undefined);
                }}
                role='menuitem'
                type='button'
              >
                <IconCopy aria-hidden='true' className='session-context-menu-icon' size={14} />
                Copy Path
              </button>
              <button
                className='session-context-menu-item'
                onClick={() => {
                  vscode.postMessage({
                    projectId: contextMenuPosition.projectId,
                    type: 'openRecentProjectInFinder',
                  });
                  setContextMenuPosition(undefined);
                }}
                role='menuitem'
                type='button'
              >
                <IconFolderOpen aria-hidden='true' className='session-context-menu-icon' size={14} />
                Open in Finder
              </button>
              <div className='session-context-menu-divider' role='separator' />
              <button
                className='session-context-menu-item session-context-menu-item-danger'
                onClick={() => {
                  vscode.postMessage({
                    projectId: contextMenuPosition.projectId,
                    type: 'removeRecentProject',
                  });
                  setContextMenuPosition(undefined);
                  requestRecentProjects();
                }}
                role='menuitem'
                type='button'
              >
                <IconTrash aria-hidden='true' className='session-context-menu-icon' size={14} />
                Remove
              </button>
            </SidebarContextMenuPortal>
          ) : null}
        </div>
      </div>
    </TooltipProvider>,
    document.body
  );
}
