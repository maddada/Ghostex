import { ProjectViewSettings } from '../project-views/project-settings';
import { useEffect, useId, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/packages/components/ui/command';
import { SearchableDropdownContent } from '@/packages/components/ui/searchable-dropdown';
import { Popover, PopoverTrigger } from '@/packages/components/ui/popover';
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/packages/components/ui/empty';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/packages/components/ui/tooltip';
import { IconChevronDown, IconFolderOpen } from '@tabler/icons-react';
import { type SidebarProjectSettingsItem } from '../../../shared/session-grid-contract';
import { type ghostexSettings } from '../../../shared/ghostex-settings';
import { type WebviewApi } from '../../webview-api';
import { SettingRow, SettingsInput, SettingsListItem, SettingsSection, SettingsTextarea } from '../fields';
import { SettingsTabSearch, hasVisibleSettingsSearchResult, shouldShowSettingsSection } from '../search';

/*
 * CDXC:Projects 2026-08-02:
 * A project field is "inherited" only while the project's own value is empty and
 * a Global Default exists to take its place. The badge marks that state next to
 * the field name, and the caller shows the inherited value as the input's
 * placeholder so the effective value is visible without leaving the page.
 */
export function InheritedSettingBadge() {
  return (
    <Tooltip>
      <TooltipTrigger render={<span className='settings-inherited-badge'>Inherited</span>} />
      <TooltipContent sideOffset={6}>Using the Global Default set above</TooltipContent>
    </Tooltip>
  );
}

export function inheritedPlaceholder(projectValue: string, globalValue: string, fallback: string): string {
  return projectValue.trim().length === 0 && globalValue.trim().length > 0 ? globalValue : fallback;
}

export function ProjectsSettingsPanel({
  onCustomViewsChange,
  onGlobalBeadsDirectoryChange,
  onGlobalBeadsDisplayKeyChange,
  onGlobalDocsDirectoryChange,
  onGlobalWorktreeCommandChange,
  onManageAdditionalDocsFoldersChange,
  projects,
  search,
  searchEmptyState,
  settings,
  vscode,
}: {
  onCustomViewsChange?: (views: ghostexSettings['customViews']) => void;
  onGlobalBeadsDirectoryChange: (value: string) => void;
  onGlobalBeadsDisplayKeyChange: (value: string) => void;
  onGlobalDocsDirectoryChange: (value: string) => void;
  onGlobalWorktreeCommandChange: (value: string) => void;
  onManageAdditionalDocsFoldersChange: (value: string) => void;
  projects: SidebarProjectSettingsItem[];
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
  vscode?: WebviewApi;
}) {
  const projectSelectorLabelId = useId();
  const projectSelectorValueId = useId();
  const docsFoldersId = useId();
  const globalWorktreeCommandId = useId();
  const globalBeadsDisplayKeyId = useId();
  const globalBeadsDirectoryId = useId();
  const globalDocsDirectoryId = useId();
  const worktreeCommandId = useId();
  const beadsDisplayKeyId = useId();
  const beadsDirectoryId = useId();
  const docsDirectoryId = useId();
  const [selectedProjectId, setSelectedProjectId] = useState(projects[0]?.projectId ?? '');
  const [isProjectSelectorOpen, setIsProjectSelectorOpen] = useState(false);
  const [projectSelectorQuery, setProjectSelectorQuery] = useState('');
  const selectedProject = projects.find((project) => project.projectId === selectedProjectId) ?? projects[0];
  const [command, setCommand] = useState(selectedProject?.worktreeCommand ?? '');
  const [beadsDisplayKey, setBeadsDisplayKey] = useState(selectedProject?.beadsDisplayKey ?? '');
  const [beadsDirectory, setBeadsDirectory] = useState(selectedProject?.beadsDirectory ?? '');
  const [docsDirectory, setDocsDirectory] = useState(selectedProject?.docsDirectory ?? '');
  /*
   * CDXC:Projects 2026-08-02:
   * Track inheritance against the live draft text rather than the saved project
   * value so the badge disappears the moment the user starts typing an override
   * and returns when they clear the field again.
   */
  const isWorktreeCommandInherited = command.trim().length === 0 && settings.globalWorktreeCommand.trim().length > 0;
  const isBeadsDisplayKeyInherited =
    beadsDisplayKey.trim().length === 0 && settings.globalBeadsDisplayKey.trim().length > 0;
  const isBeadsDirectoryInherited =
    beadsDirectory.trim().length === 0 && settings.globalBeadsDirectory.trim().length > 0;
  const isDocsDirectoryInherited = docsDirectory.trim().length === 0 && settings.globalDocsDirectory.trim().length > 0;

  useEffect(() => {
    if (!projects.some((project) => project.projectId === selectedProjectId)) {
      setSelectedProjectId(projects[0]?.projectId ?? '');
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    setCommand(selectedProject?.worktreeCommand ?? '');
    setBeadsDisplayKey(selectedProject?.beadsDisplayKey ?? '');
    setBeadsDirectory(selectedProject?.beadsDirectory ?? '');
    setDocsDirectory(selectedProject?.docsDirectory ?? '');
  }, [
    selectedProject?.beadsDirectory,
    selectedProject?.beadsDisplayKey,
    selectedProject?.docsDirectory,
    selectedProject?.projectId,
    selectedProject?.worktreeCommand,
  ]);

  useEffect(() => {
    if (!isProjectSelectorOpen) {
      return undefined;
    }
    const frame = window.requestAnimationFrame(() => {
      document
        .querySelector<HTMLInputElement>(".projects-settings-selector-popover [data-slot='command-input']")
        ?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isProjectSelectorOpen]);

  const selectProject = (projectId: string) => {
    setSelectedProjectId(projectId);
    setIsProjectSelectorOpen(false);
    setProjectSelectorQuery('');
  };

  const saveCommand = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      command,
      projectId: selectedProject.projectId,
      type: 'setProjectWorktreeCommand',
    });
  };

  const saveBeadsDisplayKey = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      displayKey: beadsDisplayKey,
      projectId: selectedProject.projectId,
      type: 'setProjectBeadsDisplayKey',
    });
  };

  const saveBeadsDirectory = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      directory: beadsDirectory,
      projectId: selectedProject.projectId,
      type: 'setProjectBeadsDirectory',
    });
  };

  const saveDocsDirectory = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      directory: docsDirectory,
      projectId: selectedProject.projectId,
      type: 'setProjectDocsDirectory',
    });
  };

  return (
    <div className='settings-tab-scroll'>
      {/*
       * CDXC:RemotePairing 2026-09-03:
       * The Portless global panel no longer renders here (it was already hidden
       * behind PORTLESS_SETTINGS_VISIBLE); tabs/portless.tsx and the
       * portlessSetup modal kind remain for a separate removal.
       *
       * CDXC:Worktrees 2026-05-18-23:07:
       * Main projects can store a setup command that runs inside every new worktree before the selected agent receives the first prompt. Keep worktree projects out of this list because they inherit from their parent project.
       */}
      <div className='projects-settings-layout'>
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
        {shouldShowSettingsSection(search.sections.docs) ? (
          <SettingsSection
            description='Docs scans docs, artifacts, ai, and tmp by default, including those folders one level down. Add more project-relative folders here.'
            title='Docs'
          >
            {/*
              CDXC:Docs 2026-06-30-11:42:
              Docs folder scanning is a global Projects setting, not selected-project metadata. Keep it above the project selector and accept comma-separated project-relative folder names so entries like "plans, my documents, folders/folder name" scan matching folders under each project root.
              Give this card an explicit Docs title so users coming from the Docs sidebar shortcut know the folder list controls Docs file discovery.

              CDXC:Docs 2026-08-09:
              This list is project-relative again. A Docs directory adds its own whole tree beside these folders instead of being narrowed by them, so the copy must not imply the two interact.
            */}
            <SettingRow
              description='Comma-separated project-relative folders to scan recursively in Docs. Spaces around folder names are ignored. Leave blank to scan docs/, artifacts/, ai/, and tmp/ at the project root and one folder down, plus root Markdown, HTML, and Excalidraw files. A Docs directory set below adds its whole tree on top of this.'
              htmlFor={docsFoldersId}
              label='Docs folders'
              wide
            >
              <SettingsInput
                aria-label='Docs folders'
                id={docsFoldersId}
                onChange={(event) => onManageAdditionalDocsFoldersChange(event.currentTarget.value)}
                placeholder='plans, my documents, folders/folder name'
                value={settings.manageAdditionalDocsFolders}
              />
            </SettingRow>
          </SettingsSection>
        ) : null}
        {shouldShowSettingsSection(search.sections.globalDefaults) ? (
          <SettingsSection
            description='Applied to every project that does not set its own value below.'
            title='Global Defaults'
          >
            {/*
              CDXC:Projects 2026-08-02:
              Global Defaults sits above the project selector because it configures every project at once. Each field mirrors the per-project field of the same name below; a project keeps winning whenever its own value is non-empty, so filling nothing in here leaves every project resolving exactly as it did before.
            */}
            <SettingRow
              description='Runs in every new worktree folder unless the project sets its own command.'
              htmlFor={globalWorktreeCommandId}
              label='Worktree command'
              wide
            >
              <SettingsTextarea
                aria-label='Global worktree command'
                className='settings-project-command-textarea'
                id={globalWorktreeCommandId}
                onChange={(event) => onGlobalWorktreeCommandChange(event.currentTarget.value)}
                placeholder='bun install'
                value={settings.globalWorktreeCommand}
              />
            </SettingRow>
            <SettingRow
              description='Ticket prefix for every project board unless the project sets its own key.'
              htmlFor={globalBeadsDisplayKeyId}
              label='Ticket key'
            >
              <SettingsInput
                aria-label='Global ticket key'
                className='settings-control-lane'
                id={globalBeadsDisplayKeyId}
                maxLength={3}
                onChange={(event) =>
                  onGlobalBeadsDisplayKeyChange(event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ''))
                }
                placeholder='ZMX'
                value={settings.globalBeadsDisplayKey}
              />
            </SettingRow>
            <SettingRow
              description='Absolute path every Project board reads its Beads workspace (.beads) from unless the project sets its own directory. Leave blank to keep using each project root.'
              htmlFor={globalBeadsDirectoryId}
              label='Beads directory'
              wide
            >
              <SettingsInput
                aria-label='Global Beads directory'
                id={globalBeadsDirectoryId}
                onChange={(event) => onGlobalBeadsDirectoryChange(event.currentTarget.value)}
                placeholder='/Users/you/code/my-repo'
                value={settings.globalBeadsDirectory}
              />
            </SettingRow>
            {/*
              CDXC:Docs 2026-08-09:
              Docs can show any absolute folder, not only the project's own repo
              folder, so a notes vault is browsable from every project.

              CDXC:Docs 2026-08-09:
              That folder is ADDED to the project's own docs, never swapped in
              for them, and the Docs folders list above stays project-relative.
              Say so here, because "Docs directory" reads like a replacement.
              A project that sets its own `docsDirectory` overrides this value,
              with the same additive meaning.
            */}
            <SettingRow
              description="Extra folder every project's Docs surface shows unless the project sets its own. It is added alongside that project's own README, CLAUDE.md, docs/ and Docs folders; it never replaces them. It appears as one top-level folder named after itself. Leave blank to add nothing."
              htmlFor={globalDocsDirectoryId}
              label='Docs directory'
              wide
            >
              <SettingsInput
                aria-label='Global Docs directory'
                id={globalDocsDirectoryId}
                onChange={(event) => onGlobalDocsDirectoryChange(event.currentTarget.value)}
                placeholder='/Users/you/Documents/vault'
                value={settings.globalDocsDirectory}
              />
            </SettingRow>
          </SettingsSection>
        ) : null}
        {!shouldShowSettingsSection(search.sections.projectSettings) ? null : projects.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>No projects</EmptyTitle>
              <EmptyDescription>Main projects will appear here.</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <>
            {/*
             * CDXC:Projects 2026-06-14-17:29:
             * The Projects settings tab should not render every project as a visible
             * button list. Keep one project selector at the top, open a searchable
             * dropdown of project paths on click, and bind the settings editor below
             * to the selected project.
             *
             * CDXC:Projects 2026-06-19-12:11:
             * The Projects settings page edits selected-project metadata only.
             * Do not expose project deletion from this page; removing the standalone
             * trash row keeps destructive project management out of this settings flow.
             */}
            <SettingsSection title='Project'>
              <SettingsListItem title={<span id={projectSelectorLabelId}>Project</span>}>
                <Popover
                  onOpenChange={(open) => {
                    setIsProjectSelectorOpen(open);
                    if (!open) {
                      setProjectSelectorQuery('');
                    }
                  }}
                  open={isProjectSelectorOpen}
                >
                  <PopoverTrigger
                    render={
                      <Button
                        aria-expanded={isProjectSelectorOpen}
                        aria-labelledby={`${projectSelectorLabelId} ${projectSelectorValueId}`}
                        className='projects-settings-selector-trigger'
                        type='button'
                        variant='outline'
                      />
                    }
                  >
                    <span className='projects-settings-selector-icon' aria-hidden='true'>
                      <IconFolderOpen aria-hidden='true' />
                    </span>
                    <span className='projects-settings-selector-copy' id={projectSelectorValueId}>
                      <span className='projects-settings-selector-name'>{selectedProject?.name}</span>
                      <span className='projects-settings-selector-path'>{selectedProject?.path}</span>
                    </span>
                    <IconChevronDown aria-hidden='true' data-icon='inline-end' />
                  </PopoverTrigger>
                  <SearchableDropdownContent
                    align='end'
                    className='projects-settings-selector-popover'
                    onOpenAutoFocus={(event) => event.preventDefault()}
                    sideOffset={8}
                  >
                    <Command className='projects-settings-selector-command'>
                      <CommandInput
                        aria-label='Search projects'
                        className='projects-settings-selector-search pl-3'
                        clearLabel='Clear project search'
                        onValueChange={setProjectSelectorQuery}
                        placeholder='Search project paths'
                        spellCheck={false}
                        value={projectSelectorQuery}
                      />
                      <CommandList className='projects-settings-selector-list scroll-mask-y'>
                        <CommandEmpty>No matching projects</CommandEmpty>
                        <CommandGroup heading='Projects'>
                          {projects.map((project) => (
                            <CommandItem
                              className='projects-settings-selector-option'
                              data-checked={selectedProject?.projectId === project.projectId}
                              key={project.projectId}
                              onSelect={() => selectProject(project.projectId)}
                              value={`${project.name} ${project.path}`}
                            >
                              <IconFolderOpen aria-hidden='true' />
                              <span className='projects-settings-selector-option-copy'>
                                <span className='projects-settings-selector-option-name'>{project.name}</span>
                                <span className='projects-settings-selector-option-path'>{project.path}</span>
                              </span>
                            </CommandItem>
                          ))}
                        </CommandGroup>
                      </CommandList>
                    </Command>
                  </SearchableDropdownContent>
                </Popover>
              </SettingsListItem>
              {/*
                CDXC:Projects 2026-06-15-03:21:
                Worktree command is the primary Projects-page setup control, so it should be the first editable project field after selecting a project. Ticket key and Beads directory stay below because they configure board metadata.
              */}
              <SettingRow
                description='Runs in the new worktree folder before the project is added (Useful for .envs/installing dependencies/etc.)'
                htmlFor={worktreeCommandId}
                label='Worktree command'
                labelAddon={isWorktreeCommandInherited ? <InheritedSettingBadge /> : null}
                wide
              >
                <div className='flex flex-col gap-3'>
                  <SettingsTextarea
                    aria-label='Worktree command'
                    className='settings-project-command-textarea'
                    id={worktreeCommandId}
                    onChange={(event) => setCommand(event.currentTarget.value)}
                    placeholder={inheritedPlaceholder(command, settings.globalWorktreeCommand, 'bun install')}
                    value={command}
                  />
                  <div className='settings-management-actions'>
                    <Button onClick={() => setCommand('')} type='button' variant='outline'>
                      Clear
                    </Button>
                    <Button onClick={saveCommand} type='button'>
                      Save Command
                    </Button>
                  </div>
                </div>
              </SettingRow>
              {/*
                CDXC:ProjectBoard 2026-05-23-14:35:
                Projects settings owns the three-letter ticket key shown on the board (for example ZMX-12) while Beads keeps hash ids internally.
              */}
              <SettingRow
                description='Three-letter prefix used for Linear-style ticket numbers on the Project board.'
                htmlFor={beadsDisplayKeyId}
                label='Ticket key'
                labelAddon={isBeadsDisplayKeyInherited ? <InheritedSettingBadge /> : null}
              >
                <div className='flex items-center gap-2'>
                  <SettingsInput
                    aria-label='Ticket key'
                    className='w-24'
                    id={beadsDisplayKeyId}
                    maxLength={3}
                    onChange={(event) =>
                      setBeadsDisplayKey(event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ''))
                    }
                    placeholder={inheritedPlaceholder(beadsDisplayKey, settings.globalBeadsDisplayKey, 'ZMX')}
                    value={beadsDisplayKey}
                  />
                  <Button onClick={() => setBeadsDisplayKey('')} type='button' variant='outline'>
                    Clear
                  </Button>
                  <Button onClick={saveBeadsDisplayKey} type='button'>
                    Save Ticket Key
                  </Button>
                </div>
              </SettingRow>
              {/*
                CDXC:ProjectBoard 2026-06-13:
                Projects settings owns the directory the Project board launches its Beads workspace from. Leave blank to use the project root; otherwise the board reads `.beads` from this absolute path.
              */}
              <SettingRow
                description="Path to this project's Beads workspace (.beads). Leave blank to use the Global Default or project root."
                htmlFor={beadsDirectoryId}
                label='Beads directory'
                labelAddon={isBeadsDirectoryInherited ? <InheritedSettingBadge /> : null}
                wide
              >
                <div className='flex flex-col gap-3'>
                  <SettingsInput
                    aria-label='Beads directory'
                    id={beadsDirectoryId}
                    onChange={(event) => setBeadsDirectory(event.currentTarget.value)}
                    placeholder={inheritedPlaceholder(
                      beadsDirectory,
                      settings.globalBeadsDirectory,
                      '/Users/you/code/my-repo'
                    )}
                    value={beadsDirectory}
                  />
                  <div className='settings-management-actions'>
                    <Button onClick={() => setBeadsDirectory('')} type='button' variant='outline'>
                      Clear
                    </Button>
                    <Button onClick={saveBeadsDirectory} type='button'>
                      Save Beads Directory
                    </Button>
                  </div>
                </div>
              </SettingRow>
              {/*
                CDXC:Docs 2026-08-09:
                Projects settings owns this project's `docsDirectory`: the extra
                folder its Docs surface shows. Leave blank to use the Global
                Default.

                CDXC:Docs 2026-08-09:
                This project's own docs list either way, so `docsDirectory` only
                ever adds a tree beside them — it never replaces them.
              */}
              <SettingRow
                description="Extra folder this project's Docs surface shows, in addition to the project's own docs. Leave blank to use the Global Default."
                htmlFor={docsDirectoryId}
                label='Docs directory'
                labelAddon={isDocsDirectoryInherited ? <InheritedSettingBadge /> : null}
                wide
              >
                <div className='flex flex-col gap-3'>
                  <SettingsInput
                    aria-label='Docs directory'
                    id={docsDirectoryId}
                    onChange={(event) => setDocsDirectory(event.currentTarget.value)}
                    placeholder={inheritedPlaceholder(
                      docsDirectory,
                      settings.globalDocsDirectory,
                      '/Users/you/Documents/vault'
                    )}
                    value={docsDirectory}
                  />
                  <div className='settings-management-actions'>
                    <Button onClick={() => setDocsDirectory('')} type='button' variant='outline'>
                      Clear
                    </Button>
                    <Button onClick={saveDocsDirectory} type='button'>
                      Save Docs Directory
                    </Button>
                  </div>
                </div>
              </SettingRow>
            </SettingsSection>
            {selectedProject && onCustomViewsChange ? (
              <ProjectViewSettings
                projectId={selectedProject.projectId}
                parentProjectId={selectedProject.worktreeParentProjectId}
                views={settings.customViews}
                onChange={onCustomViewsChange}
              />
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}
