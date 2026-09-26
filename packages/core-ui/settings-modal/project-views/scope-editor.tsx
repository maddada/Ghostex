import { IconArrowBackUp, IconCheck, IconCornerDownRight, IconInfoCircle, IconX } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import type { ProjectViewProject, ProjectViewSpace } from '@/packages/shared/ghostex-settings/project-views';
import {
  DEFAULT_GHOSTEX_VIEW_SCOPE,
  parseViewScopeSpaceKey,
  viewScopeSpaceKey,
  type GhostexViewScope,
  type GhostexViewScopeState,
} from '@/packages/shared/ghostex-settings/view-scopes';
import { ScopeMultiSelect, type ScopeOption } from './scope-multi-select';
import './scope-editor.css';

/**
 * CDXC:Extensions 2026-09-24 DECISION:
 * User (ruling 3A, 2026-09-20): a view's scope is a Default of shown or hidden plus per-project and per-space
 * overrides. User (2026-09-24): replace the grids of toggle cards with a multi-select dropdown and "make the UX
 * for this part great". The editor now reads as one rule: "Show <view> [Everywhere | Only in selected places]",
 * then "Except in" / "Show in" with the picked spaces and projects (the overrides that differ from the
 * Default), then, once a space is picked, "But keep in" / "But not in" for projects that override their space
 * (project overrides equal to the Default). A plain sentence under it states the result.
 *
 * Settings is given the sidebar's projects and spaces but not which project sits in which space (the sidebar
 * runtime resolves that live), so the "But keep in" list offers every project rather than only those inside
 * the picked spaces.
 * SEE-ALSO: packages/core-ui/settings-modal/project-views/editor.tsx renders the custom-view picker.
 */
export type ViewScopeEditorState = { draft: GhostexViewScope; key: string; title: string };

const flipState = (state: GhostexViewScopeState): GhostexViewScopeState => (state === 'shown' ? 'hidden' : 'shown');
const spaceOptionId = (spaceKey: string) => `space:${spaceKey}`;
const projectOptionId = (projectId: string) => `project:${projectId}`;

/** Space overrides equal to the Default are never shown here; they are only kept until the mode flips. */
function scopeSelection(scope: GhostexViewScope) {
  const opposite = flipState(scope.default);
  return {
    keep: Object.entries(scope.projects)
      .filter(([, state]) => state === scope.default)
      .map(([projectId]) => projectOptionId(projectId)),
    targets: [
      ...Object.entries(scope.spaces)
        .filter(([, state]) => state === opposite)
        .map(([spaceKey]) => spaceOptionId(spaceKey)),
      ...Object.entries(scope.projects)
        .filter(([, state]) => state === opposite)
        .map(([projectId]) => projectOptionId(projectId)),
    ],
  };
}

/** Write one pick (or un-pick) into the scope: a target differs from the Default, a keep matches it. */
function withSelection(
  scope: GhostexViewScope,
  optionId: string,
  state: GhostexViewScopeState | 'inherit'
): GhostexViewScope {
  const isSpace = optionId.startsWith('space:');
  const key = optionId.slice(isSpace ? 'space:'.length : 'project:'.length);
  const overrides = { ...(isSpace ? scope.spaces : scope.projects) };
  if (state === 'inherit') delete overrides[key];
  else overrides[key] = state;
  return isSpace ? { ...scope, spaces: overrides } : { ...scope, projects: overrides };
}

/** Switching the mode keeps the same picks and flips what they mean, so "hidden in X" becomes "only in X". */
function withFlippedDefault(scope: GhostexViewScope): GhostexViewScope {
  const flip = (overrides: Record<string, GhostexViewScopeState>, keepMatching: boolean) =>
    Object.fromEntries(
      Object.entries(overrides).flatMap(([key, state]) =>
        keepMatching || state !== scope.default ? [[key, flipState(state)]] : []
      )
    );
  return { default: flipState(scope.default), projects: flip(scope.projects, true), spaces: flip(scope.spaces, false) };
}

function listNames(names: readonly string[]): string {
  if (names.length < 2) return names.join('');
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

function scopeSentence(
  title: string,
  scope: GhostexViewScope,
  selection: ReturnType<typeof scopeSelection>,
  labelFor: (id: string) => string
): string {
  const names = (ids: readonly string[]) => listNames(ids.map(labelFor));
  const keep = selection.keep.length ? names(selection.keep) : '';
  if (scope.default === 'shown') {
    if (!selection.targets.length) return `${title} shows in every project.`;
    return `${title} shows everywhere except ${names(selection.targets)}${keep ? `, but stays in ${keep}` : ''}.`;
  }
  if (!selection.targets.length) return `${title} is hidden everywhere. Pick where it should show.`;
  return `${title} shows only in ${names(selection.targets)}${keep ? `, but not in ${keep}` : ''}.`;
}

export function ViewScopeEditor({
  editor,
  onCancel,
  onChange,
  onSave,
  projects,
  spaces,
}: {
  editor: ViewScopeEditorState;
  onCancel: () => void;
  /**
   * CDXC:Settings 2026-09-18 WHY:
   * Takes an updater, not a value. Each pick writes one key of the same draft, so a value-based callback
   * reads `editor` from the render that mounted the control: two picks landing in one React batch make the
   * second overwrite the first, and the one the user made first silently springs back.
   */
  onChange: (apply: (current: ViewScopeEditorState) => ViewScopeEditorState) => void;
  onSave: () => void;
  projects: readonly ProjectViewProject[];
  spaces: readonly ProjectViewSpace[];
}) {
  const scope = editor.draft;
  const everywhere = scope.default === 'shown';
  const selection = scopeSelection(scope);
  /*
   * A saved override can outlive the space or project it names: a space deleted in the sidebar, a project
   * closed, a remote machine disconnected. Keep the stored entry listed and removable rather than dropping it
   * silently, so saving an unrelated edit cannot quietly widen or narrow the scope.
   */
  const spaceOptions: ScopeOption[] = [
    ...spaces.map((space) => ({
      id: spaceOptionId(viewScopeSpaceKey(space)),
      kind: 'space' as const,
      label: space.name,
    })),
    ...Object.keys(scope.spaces).flatMap((key) =>
      spaces.some((space) => viewScopeSpaceKey(space) === key) || !parseViewScopeSpaceKey(key)
        ? []
        : [
            {
              hint: 'No longer in the sidebar',
              id: spaceOptionId(key),
              kind: 'space' as const,
              label: 'Unavailable space',
            },
          ]
    ),
  ];
  const projectOptions: ScopeOption[] = [
    ...projects.map((project) => ({
      hint: project.path || undefined,
      id: projectOptionId(project.projectId),
      kind: 'project' as const,
      label: project.name,
    })),
    ...Object.keys(scope.projects).flatMap((projectId) =>
      projects.some((project) => project.projectId === projectId)
        ? []
        : [
            {
              hint: 'No longer in the sidebar',
              id: projectOptionId(projectId),
              kind: 'project' as const,
              label: 'Unavailable project',
            },
          ]
    ),
  ];
  const allOptions = [...spaceOptions, ...projectOptions];
  const labelFor = (id: string) => {
    const option = allOptions.find((candidate) => candidate.id === id);
    if (!option) return id;
    return option.kind === 'space' ? `the ${option.label} space` : option.label;
  };
  const toggleTarget = (id: string) =>
    onChange((current) => ({
      ...current,
      draft: withSelection(
        current.draft,
        id,
        scopeSelection(current.draft).targets.includes(id) ? 'inherit' : flipState(current.draft.default)
      ),
    }));
  const toggleKeep = (id: string) =>
    onChange((current) => ({
      ...current,
      draft: withSelection(
        current.draft,
        id,
        scopeSelection(current.draft).keep.includes(id) ? 'inherit' : current.draft.default
      ),
    }));
  const clear = (ids: readonly string[]) =>
    onChange((current) => ({
      ...current,
      draft: ids.reduce((draft, id) => withSelection(draft, id, 'inherit'), current.draft),
    }));
  const showKeepRow = selection.targets.some((id) => id.startsWith('space:')) || selection.keep.length > 0;
  const isDefault =
    scope.default === 'shown' && !Object.keys(scope.projects).length && !Object.keys(scope.spaces).length;

  return (
    <div className='view-scope-editor'>
      <div className='view-scope-editor-head'>
        <span className='view-scope-editor-title'>Where {editor.title} is shown</span>
        <Button
          className='font-normal'
          disabled={isDefault}
          onClick={() => onChange((current) => ({ ...current, draft: { ...DEFAULT_GHOSTEX_VIEW_SCOPE } }))}
          size='xs'
          type='button'
          variant='ghost'
        >
          <IconArrowBackUp data-icon='inline-start' />
          Reset
        </Button>
      </div>
      <div className='view-scope-rule'>
        <span className='view-scope-rule-label'>Show {editor.title}</span>
        <SegmentedControl
          aria-label={`Where ${editor.title} is shown`}
          onValueChange={(next) =>
            onChange((current) =>
              (next === 'shown') === (current.draft.default === 'shown')
                ? current
                : { ...current, draft: withFlippedDefault(current.draft) }
            )
          }
          size='sm'
          value={scope.default}
        >
          <SegmentedControlItem value='shown'>Everywhere</SegmentedControlItem>
          <SegmentedControlItem value='hidden'>Only in selected places</SegmentedControlItem>
        </SegmentedControl>
      </div>
      <div className='view-scope-rule'>
        <span className='view-scope-rule-label'>{everywhere ? 'Except in' : 'Show in'}</span>
        {allOptions.length ? (
          <ScopeMultiSelect
            ariaLabel={everywhere ? `Hide ${editor.title} in` : `Show ${editor.title} in`}
            onClear={() => clear(selection.targets)}
            onToggle={toggleTarget}
            options={allOptions}
            placeholder={everywhere ? 'Choose projects or spaces to hide it in' : 'Choose projects or spaces'}
            searchPlaceholder='Search projects and spaces'
            selected={selection.targets}
          />
        ) : (
          <span className='view-scope-empty'>No projects or spaces in the sidebar yet.</span>
        )}
      </div>
      {showKeepRow ? (
        <div className='view-scope-rule view-scope-rule-keep'>
          <span className='view-scope-rule-label'>
            <IconCornerDownRight aria-hidden='true' />
            {everywhere ? 'But keep in' : 'But not in'}
          </span>
          <ScopeMultiSelect
            ariaLabel={everywhere ? `Keep ${editor.title} in` : `Also hide ${editor.title} in`}
            onClear={() => clear(selection.keep)}
            onToggle={toggleKeep}
            options={projectOptions.filter(
              (option) => !selection.targets.includes(option.id) || selection.keep.includes(option.id)
            )}
            placeholder={everywhere ? 'Optional: projects that still show it' : 'Optional: projects that still hide it'}
            searchPlaceholder='Search projects'
            selected={selection.keep}
          />
        </div>
      ) : null}
      <div className='view-scope-editor-foot'>
        <span className='view-scope-summary'>
          <IconInfoCircle aria-hidden='true' />
          {scopeSentence(editor.title, scope, selection, labelFor)}
        </span>
        <div className='view-scope-editor-actions'>
          <Button onClick={onCancel} type='button' variant='outline'>
            <IconX data-icon='inline-start' />
            Cancel
          </Button>
          <Button onClick={onSave} type='button'>
            <IconCheck data-icon='inline-start' />
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}
