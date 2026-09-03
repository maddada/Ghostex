import { useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Command } from '@/packages/components/ui/command';
import { IconCodeDots, IconFolderOpen, IconPencil, IconPlus, IconTrash } from '@tabler/icons-react';
import { type ghostexSettings } from '../../../shared/ghostex-settings';
import {
  BUILT_IN_WORKSPACE_OPEN_TARGETS,
  CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX,
  createWorkspaceOpenTargetSlug,
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetHiddenIds,
  type CustomWorkspaceOpenTarget,
} from '../../../shared/workspace-open-targets';
import { EditorBrandIcon, getEditorBrandIconId } from '../../brand-icons';
import { SettingSwitch, SettingsInput, SettingsNativeScrollArea, SettingsSection, SettingsTextarea } from '../fields';
import {
  SettingsTabSearch,
  hasVisibleSettingsSearchResult,
  shouldShowSetting,
  shouldShowSettingsSection,
} from '../search';

export type SettingsOpenTargetEditorState = {
  draft: {
    argsText: string;
    command: string;
    label: string;
  };
  id?: string;
};

export function OpenTargetsSettingsTab({
  onChange,
  search,
  searchEmptyState,
  settings,
}: {
  onChange: (settings: ghostexSettings) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
}) {
  const [editorState, setEditorState] = useState<SettingsOpenTargetEditorState>();
  const hiddenIds = new Set(settings.workspaceOpenTargetHiddenIds);
  /**
   * CDXC:Titlebar 2026-05-11-02:03
   * Settings shows installed built-ins as toggleable and unavailable built-ins
   * as disabled rows. Turning an installed target off writes only hidden ids,
   * so the startup scan can refresh availability without undoing that choice.
   */
  const availableBuiltInIds = new Set(settings.workspaceOpenTargetAvailability.availableTargetIds);

  const updateHiddenTarget = (targetId: string, isVisible: boolean) => {
    const nextHiddenIds = new Set(settings.workspaceOpenTargetHiddenIds);
    if (isVisible) {
      nextHiddenIds.delete(targetId);
    } else {
      nextHiddenIds.add(targetId);
    }
    onChange({
      ...settings,
      workspaceOpenTargetHiddenIds: normalizeWorkspaceOpenTargetHiddenIds([...nextHiddenIds]),
    });
  };

  const saveCustomTarget = () => {
    if (!editorState) {
      return;
    }
    const label = editorState.draft.label.trim();
    const command = editorState.draft.command.trim();
    if (!label || !command) {
      return;
    }
    const nextTarget: CustomWorkspaceOpenTarget = {
      args: editorState.draft.argsText
        .split('\n')
        .map((arg) => arg.trim())
        .filter(Boolean),
      command,
      id:
        editorState.id ??
        `${CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX}${createWorkspaceOpenTargetSlug(label)}-${Date.now().toString(36)}`,
      label,
    };
    const existingTargets = settings.customWorkspaceOpenTargets.filter((target) => target.id !== editorState.id);
    onChange({
      ...settings,
      customWorkspaceOpenTargets: normalizeCustomWorkspaceOpenTargets([...existingTargets, nextTarget]),
    });
    setEditorState(undefined);
  };

  const removeCustomTarget = (targetId: string) => {
    onChange({
      ...settings,
      customWorkspaceOpenTargets: settings.customWorkspaceOpenTargets.filter((target) => target.id !== targetId),
    });
  };

  return (
    <SettingsNativeScrollArea className='h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab) ? searchEmptyState : null}
        {shouldShowSettingsSection(search.sections.openIn) ? (
          <SettingsSection title='Open In'>
            {/* CDXC:Titlebar 2026-05-11-00:22
              Users need a Settings tab opened from the titlebar dropdown to
              show or hide IDE targets and add custom project-open commands.

              CDXC:Titlebar 2026-05-16-23:24
              Settings must show the same Open In editor icons as the titlebar
              dropdown so users can scan Cursor, VS Code variants, Zed,
              Antigravity, VSCodium, and JetBrains-family targets by brand. */}
            <div className='flex flex-col gap-2'>
              {BUILT_IN_WORKSPACE_OPEN_TARGETS.filter((target) =>
                shouldShowSetting(search.sections.openIn, `builtin:${target.id}`)
              ).map((target) => {
                const isAvailable = target.id === 'finder' || availableBuiltInIds.has(target.id);
                return (
                  <div
                    className='flex items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2'
                    key={target.id}
                  >
                    <div className='flex min-w-0 flex-1 items-center gap-3'>
                      <OpenTargetSettingsIcon targetId={target.id} />
                      <div className='min-w-0'>
                        <div className='truncate text-sm font-medium'>{target.label}</div>
                        <div className='truncate text-xs text-muted-foreground'>
                          {isAvailable
                            ? target.id === 'finder'
                              ? 'Built-in'
                              : (target.commands?.join(', ') ?? 'macOS')
                            : 'Not installed'}
                        </div>
                      </div>
                    </div>
                    <SettingSwitch
                      checked={isAvailable && !hiddenIds.has(target.id)}
                      disabled={!isAvailable}
                      disabledReason={`Install ${target.label} to enable this option.`}
                      onCheckedChange={(checked) => updateHiddenTarget(target.id, checked)}
                    />
                  </div>
                );
              })}
            </div>
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.customOpenTargets) ? (
          <SettingsSection title='Custom Open Targets'>
            <div className='flex flex-col gap-2'>
              {settings.customWorkspaceOpenTargets.map((target) => (
                <div
                  className='flex items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2'
                  key={target.id}
                >
                  <div className='min-w-0'>
                    <div className='truncate text-sm font-medium'>{target.label}</div>
                    <div className='truncate text-xs text-muted-foreground'>
                      {[target.command, ...target.args].join(' ')}
                    </div>
                  </div>
                  <div className='flex shrink-0 items-center gap-1'>
                    <Button
                      onClick={() =>
                        setEditorState({
                          draft: {
                            argsText: target.args.join('\n'),
                            command: target.command,
                            label: target.label,
                          },
                          id: target.id,
                        })
                      }
                      size='icon-xs'
                      type='button'
                      variant='ghost'
                    >
                      <IconPencil aria-hidden='true' size={14} />
                      <span className='sr-only'>Edit</span>
                    </Button>
                    <Button onClick={() => removeCustomTarget(target.id)} size='icon-xs' type='button' variant='ghost'>
                      <IconTrash aria-hidden='true' size={14} />
                      <span className='sr-only'>Remove</span>
                    </Button>
                  </div>
                </div>
              ))}
              {editorState ? (
                <div className='flex flex-col gap-3 rounded-none border border-border/70 bg-card/40 p-3'>
                  <SettingsInput
                    aria-label='Open target name'
                    onChange={(event) =>
                      setEditorState({
                        ...editorState,
                        draft: { ...editorState.draft, label: event.currentTarget.value },
                      })
                    }
                    placeholder='Name'
                    value={editorState.draft.label}
                  />
                  <SettingsInput
                    aria-label='Open target command'
                    onChange={(event) =>
                      setEditorState({
                        ...editorState,
                        draft: { ...editorState.draft, command: event.currentTarget.value },
                      })
                    }
                    placeholder='Command'
                    value={editorState.draft.command}
                  />
                  <SettingsTextarea
                    aria-label='Open target arguments'
                    onChange={(event) =>
                      setEditorState({
                        ...editorState,
                        draft: { ...editorState.draft, argsText: event.currentTarget.value },
                      })
                    }
                    placeholder='Optional arguments, one per line'
                    value={editorState.draft.argsText}
                  />
                  <div className='flex justify-end gap-2'>
                    <Button onClick={() => setEditorState(undefined)} type='button' variant='ghost'>
                      Cancel
                    </Button>
                    <Button onClick={saveCustomTarget} type='button'>
                      Save
                    </Button>
                  </div>
                </div>
              ) : (
                <Button
                  className='w-fit'
                  onClick={() => setEditorState({ draft: { argsText: '', command: '', label: '' } })}
                  type='button'
                  variant='outline'
                >
                  <IconPlus aria-hidden='true' size={16} />
                  Add target
                </Button>
              )}
            </div>
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

export function OpenTargetSettingsIcon({ targetId }: { targetId: string }) {
  if (targetId === 'finder') {
    return <IconFolderOpen aria-hidden='true' className='settings-open-target-icon text-muted-foreground' />;
  }
  const icon = getEditorBrandIconId(targetId);
  if (icon) {
    return <EditorBrandIcon className='settings-open-target-icon' icon={icon} />;
  }
  return <IconCodeDots aria-hidden='true' className='settings-open-target-icon text-muted-foreground' />;
}
