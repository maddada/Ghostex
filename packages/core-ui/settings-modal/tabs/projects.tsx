import { useEffect, useId, useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Card, CardContent } from '@/packages/components/ui/card';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/packages/components/ui/command';
import { Popover, PopoverContent, PopoverTrigger } from '@/packages/components/ui/popover';
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/packages/components/ui/empty';
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/packages/components/ui/field';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/packages/components/ui/tooltip';
import { IconChevronDown, IconFolderOpen } from '@tabler/icons-react';
import { type SidebarProjectSettingsItem } from '../../../shared/session-grid-contract';
import { type ghostexSettings } from '../../../shared/ghostex-settings';
import { type WebviewApi } from '../../webview-api';
import { SettingsInput, SettingsTextarea } from '../fields';
import { SettingsTabSearch, hasVisibleSettingsSearchResult, shouldShowSettingsSection } from '../search';

/*
 * CDXC:GlobalProjectDefaults 2026-08-02:
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
  const [selectedProjectId, setSelectedProjectId] = useState(projects[0]?.projectId ?? '');
  const [isProjectSelectorOpen, setIsProjectSelectorOpen] = useState(false);
  const [projectSelectorQuery, setProjectSelectorQuery] = useState('');
  const selectedProject = projects.find((project) => project.projectId === selectedProjectId) ?? projects[0];
  const [command, setCommand] = useState(selectedProject?.worktreeCommand ?? '');
  const [beadsDisplayKey, setBeadsDisplayKey] = useState(selectedProject?.beadsDisplayKey ?? '');
  const [beadsDirectory, setBeadsDirectory] = useState(selectedProject?.beadsDirectory ?? '');
  const [docsDirectory, setDocsDirectory] = useState(selectedProject?.docsDirectory ?? '');
  /*
   * CDXC:GlobalProjectDefaults 2026-08-02:
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
          <Card className='settings-project-command-card'>
            <CardContent className='flex flex-col gap-4 p-4'>
              {/*
              CDXC:DocsSidebar 2026-06-30-11:42:
              Docs folder scanning is a global Projects setting, not selected-project metadata. Keep it above the project selector and accept comma-separated project-relative folder names so entries like "plans, my documents, folders/folder name" scan matching folders under each project root.
              Give this card an explicit Docs title so users coming from the Docs sidebar shortcut know the folder list controls Docs file discovery.

              CDXC:DocsRootAdditive 2026-08-09:
              This list is project-relative again. A Docs directory adds its own whole tree beside these folders instead of being narrowed by them, so the copy must not imply the two interact.
            */}
              <div className='settings-management-header-text'>
                <h3 className='settings-management-heading'>Docs</h3>
                <p className='settings-management-description'>
                  Docs scans docs, artifacts, ai, and tmp by default, including those folders one level down. Add more
                  project-relative folders here.
                </p>
              </div>
              <FieldGroup>
                <Field>
                  <FieldLabel>Docs folders</FieldLabel>
                  <SettingsInput
                    aria-label='Docs folders'
                    onChange={(event) => onManageAdditionalDocsFoldersChange(event.currentTarget.value)}
                    placeholder='plans, my documents, folders/folder name'
                    value={settings.manageAdditionalDocsFolders}
                  />
                  <FieldDescription>
                    Comma-separated project-relative folders to scan recursively in Docs. Spaces around folder names are
                    ignored. Leave blank to scan docs/, artifacts/, ai/, and tmp/ at the project root and one folder
                    down, plus root Markdown, HTML, and Excalidraw files. A Docs directory set below adds its whole tree
                    on top of this.
                  </FieldDescription>
                </Field>
              </FieldGroup>
            </CardContent>
          </Card>
        ) : null}
        {shouldShowSettingsSection(search.sections.globalDefaults) ? (
          <Card className='settings-project-command-card'>
            <CardContent className='flex flex-col gap-4 p-4'>
              {/*
              CDXC:GlobalProjectDefaults 2026-08-02:
              Global Defaults sits above the project selector because it configures every project at once. Each field mirrors the per-project field of the same name below; a project keeps winning whenever its own value is non-empty, so filling nothing in here leaves every project resolving exactly as it did before.
            */}
              <div className='settings-management-header-text'>
                <h3 className='settings-management-heading'>Global Defaults</h3>
                <p className='settings-management-description'>
                  Applied to every project that does not set its own value below.
                </p>
              </div>
              <FieldGroup>
                <Field>
                  <FieldLabel>Worktree command</FieldLabel>
                  <SettingsTextarea
                    aria-label='Global worktree command'
                    className='settings-project-command-textarea'
                    onChange={(event) => onGlobalWorktreeCommandChange(event.currentTarget.value)}
                    placeholder='bun install'
                    value={settings.globalWorktreeCommand}
                  />
                  <FieldDescription>
                    Runs in every new worktree folder unless the project sets its own command.
                  </FieldDescription>
                </Field>
              </FieldGroup>
              <FieldGroup>
                <Field>
                  <FieldLabel>Ticket key</FieldLabel>
                  <SettingsInput
                    aria-label='Global ticket key'
                    maxLength={3}
                    onChange={(event) =>
                      onGlobalBeadsDisplayKeyChange(event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ''))
                    }
                    placeholder='ZMX'
                    value={settings.globalBeadsDisplayKey}
                  />
                  <FieldDescription>
                    Ticket prefix for every project board unless the project sets its own key.
                  </FieldDescription>
                </Field>
              </FieldGroup>
              <FieldGroup>
                <Field>
                  <FieldLabel>Beads directory</FieldLabel>
                  <SettingsInput
                    aria-label='Global Beads directory'
                    onChange={(event) => onGlobalBeadsDirectoryChange(event.currentTarget.value)}
                    placeholder='/Users/you/code/my-repo'
                    value={settings.globalBeadsDirectory}
                  />
                  <FieldDescription>
                    Absolute path every Project board reads its Beads workspace (.beads) from unless the project sets
                    its own directory. Leave blank to keep using each project root.
                  </FieldDescription>
                </Field>
              </FieldGroup>
              {/*
              CDXC:DocsRootDirectory 2026-08-09:
              Docs can show any absolute folder, not only the project's own repo
              folder, so a notes vault is browsable from every project.

              CDXC:DocsRootAdditive 2026-08-09:
              That folder is ADDED to the project's own docs, never swapped in
              for them, and the Docs folders list above stays project-relative.
              Say so here, because "Docs directory" reads like a replacement.
              A project that sets its own `docsDirectory` overrides this value,
              with the same additive meaning.
            */}
              <FieldGroup>
                <Field>
                  <FieldLabel>Docs directory</FieldLabel>
                  <SettingsInput
                    aria-label='Global Docs directory'
                    onChange={(event) => onGlobalDocsDirectoryChange(event.currentTarget.value)}
                    placeholder='/Users/you/Documents/vault'
                    value={settings.globalDocsDirectory}
                  />
                  <FieldDescription>
                    Extra folder every project's Docs surface shows unless the project sets its own. It is added
                    alongside that project's own README, CLAUDE.md, docs/ and Docs folders — it never replaces them —
                    and appears as one top-level folder named after itself. Leave blank to add nothing.
                  </FieldDescription>
                </Field>
              </FieldGroup>
            </CardContent>
          </Card>
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
             * CDXC:ProjectSettings 2026-06-14-17:29:
             * The Projects settings tab should not render every project as a visible
             * button list. Keep one project selector at the top, open a searchable
             * dropdown of project paths on click, and bind the settings editor below
             * to the selected project.
             *
             * CDXC:ProjectSettings 2026-06-19-12:11:
             * The Projects settings page edits selected-project metadata only.
             * Do not expose project deletion from this page; removing the standalone
             * trash row keeps destructive project management out of this settings flow.
             */}
            <div className='projects-settings-selector'>
              <span className='projects-settings-selector-label' id={projectSelectorLabelId}>
                Project
              </span>
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
                <PopoverContent
                  align='start'
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
                </PopoverContent>
              </Popover>
            </div>
            <Card className='settings-project-command-card'>
              <CardContent className='flex flex-col gap-4 p-4'>
                {/*
              CDXC:ProjectSettings 2026-06-15-03:21:
              Worktree command is the primary Projects-page setup control, so it should be the first editable project field after selecting a project. Ticket key and Beads directory stay below because they configure board metadata.
            */}
                <FieldGroup>
                  <Field>
                    <FieldLabel>
                      Worktree command
                      {isWorktreeCommandInherited ? <InheritedSettingBadge /> : null}
                    </FieldLabel>
                    <SettingsTextarea
                      aria-label='Worktree command'
                      className='settings-project-command-textarea'
                      onChange={(event) => setCommand(event.currentTarget.value)}
                      placeholder={inheritedPlaceholder(command, settings.globalWorktreeCommand, 'bun install')}
                      value={command}
                    />
                    <FieldDescription>
                      Runs in the new worktree folder before the project is added (Useful for .envs/installing
                      dependencies/etc.)
                    </FieldDescription>
                  </Field>
                </FieldGroup>
                <div className='settings-management-actions'>
                  <Button onClick={() => setCommand('')} type='button' variant='outline'>
                    Clear
                  </Button>
                  <Button onClick={saveCommand} type='button'>
                    Save Command
                  </Button>
                </div>
                {/*
              CDXC:ProjectBoard 2026-05-23-14:35:
              Projects settings owns the three-letter ticket key shown on the board (for example ZMX-12) while Beads keeps hash ids internally.
            */}
                <FieldGroup>
                  <Field>
                    <FieldLabel>
                      Ticket key
                      {isBeadsDisplayKeyInherited ? <InheritedSettingBadge /> : null}
                    </FieldLabel>
                    <SettingsInput
                      aria-label='Ticket key'
                      maxLength={3}
                      onChange={(event) =>
                        setBeadsDisplayKey(event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ''))
                      }
                      placeholder={inheritedPlaceholder(beadsDisplayKey, settings.globalBeadsDisplayKey, 'ZMX')}
                      value={beadsDisplayKey}
                    />
                    <FieldDescription>
                      Three-letter prefix used for Linear-style ticket numbers on the Project board.
                    </FieldDescription>
                  </Field>
                </FieldGroup>
                <div className='settings-management-actions'>
                  <Button onClick={() => setBeadsDisplayKey('')} type='button' variant='outline'>
                    Clear
                  </Button>
                  <Button onClick={saveBeadsDisplayKey} type='button'>
                    Save Ticket Key
                  </Button>
                </div>
                {/*
              CDXC:ProjectBoard 2026-06-13:
              Projects settings owns the directory the Project board launches its Beads workspace from. Leave blank to use the project root; otherwise the board reads `.beads` from this absolute path.
            */}
                <FieldGroup>
                  <Field>
                    <FieldLabel>
                      Beads directory
                      {isBeadsDirectoryInherited ? <InheritedSettingBadge /> : null}
                    </FieldLabel>
                    <SettingsInput
                      aria-label='Beads directory'
                      onChange={(event) => setBeadsDirectory(event.currentTarget.value)}
                      placeholder={inheritedPlaceholder(
                        beadsDirectory,
                        settings.globalBeadsDirectory,
                        '/Users/you/code/my-repo'
                      )}
                      value={beadsDirectory}
                    />
                    <FieldDescription>
                      Path to this project's Beads workspace (.beads). Leave blank to use the Global Default or project
                      root.
                    </FieldDescription>
                  </Field>
                </FieldGroup>
                <div className='settings-management-actions'>
                  <Button onClick={() => setBeadsDirectory('')} type='button' variant='outline'>
                    Clear
                  </Button>
                  <Button onClick={saveBeadsDirectory} type='button'>
                    Save Beads Directory
                  </Button>
                </div>
                {/*
              CDXC:DocsRootDirectory 2026-08-09:
              Projects settings owns this project's `docsDirectory`: the extra
              folder its Docs surface shows. Leave blank to use the Global
              Default.

              CDXC:DocsRootAdditive 2026-08-09:
              This project's own docs list either way, so `docsDirectory` only
              ever adds a tree beside them — it never replaces them.
            */}
                <FieldGroup>
                  <Field>
                    <FieldLabel>
                      Docs directory
                      {isDocsDirectoryInherited ? <InheritedSettingBadge /> : null}
                    </FieldLabel>
                    <SettingsInput
                      aria-label='Docs directory'
                      onChange={(event) => setDocsDirectory(event.currentTarget.value)}
                      placeholder={inheritedPlaceholder(
                        docsDirectory,
                        settings.globalDocsDirectory,
                        '/Users/you/Documents/vault'
                      )}
                      value={docsDirectory}
                    />
                    <FieldDescription>
                      Extra folder this project's Docs surface shows, in addition to the project's own docs. Leave blank
                      to use the Global Default.
                    </FieldDescription>
                  </Field>
                </FieldGroup>
                <div className='settings-management-actions'>
                  <Button onClick={() => setDocsDirectory('')} type='button' variant='outline'>
                    Clear
                  </Button>
                  <Button onClick={saveDocsDirectory} type='button'>
                    Save Docs Directory
                  </Button>
                </div>
              </CardContent>
            </Card>
          </>
        )}
      </div>
    </div>
  );
}
