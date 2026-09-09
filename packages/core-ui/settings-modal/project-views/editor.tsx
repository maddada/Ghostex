import { useId } from 'react';
import { IconCheck, IconTemplate, IconX } from '@tabler/icons-react';
import { Checkbox } from '@/packages/components/ui/checkbox';
import { Button } from '@/packages/components/ui/button';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { Switch } from '@/packages/components/ui/switch';
import type { GhostexCustomView } from '@/packages/shared/ghostex-settings/custom-views';
import { DEFAULT_PROJECT_VIEW_SOURCE, type ProjectViewSource } from '@/packages/shared/ghostex-settings/project-views';
import { SettingRow, SettingsInput, SettingsTextarea, SelectField, TextField } from '../fields';

export type CustomViewEditorState = { draft: GhostexCustomView; id?: string; error?: string; notice?: string };

/**
 * CDXC:Settings 2026-09-09 DECISION:
 * User: reuse the new settings look and shared controls. Keep Cancel bordered like the other editor actions.
 */
export function CustomViewEditor({
  editor,
  spaces,
  onChange,
  onSave,
  onCancel,
  onSaveTemplate,
}: {
  editor: CustomViewEditorState;
  spaces: import('@/packages/shared/ghostex-settings/project-views').ProjectViewSpace[];
  onChange: (editor: CustomViewEditorState) => void;
  onSave: () => void;
  onCancel: () => void;
  onSaveTemplate: () => void;
}) {
  const id = useId();
  const view = editor.draft;
  const source = view.source ?? DEFAULT_PROJECT_VIEW_SOURCE;
  const spaceOptions = [
    ...spaces,
    ...(view.spaceRefs ?? [])
      .filter((ref) => !spaces.some((space) => space.sectionKey === ref.sectionKey && space.spaceId === ref.spaceId))
      .map((ref) => ({ ...ref, name: 'Unavailable space' })),
  ];
  const update = (patch: Partial<GhostexCustomView>) =>
    onChange({ ...editor, draft: { ...view, ...patch }, error: undefined });
  const updateSource = (patch: Partial<ProjectViewSource>) => update({ source: { ...source, ...patch } });
  return (
    <div className='settings-list-panel py-3'>
      <TextField label='Name' value={view.name} onChange={(name) => update({ name })} placeholder='Titlebar name' />
      <SettingRow label='Source' htmlFor={`${id}-source`}>
        <SegmentedControl
          id={`${id}-source`}
          value={source.kind}
          onValueChange={(kind) => updateSource({ kind: kind as ProjectViewSource['kind'] })}
        >
          <SegmentedControlItem value='website'>Website</SegmentedControlItem>
          <SegmentedControlItem value='dev-server'>Dev server</SegmentedControlItem>
          <SegmentedControlItem value='report'>HTML report</SegmentedControlItem>
        </SegmentedControl>
      </SettingRow>
      <SelectField
        description='Selected projects are enabled individually in Settings → Projects. Matching projects show the view when its source can be resolved.'
        label='Available in'
        value={view.availability ?? 'all'}
        onChange={(availability) => update({ source, availability: availability as GhostexCustomView['availability'] })}
        options={[
          { value: 'all', label: 'All projects' },
          { value: 'matching', label: 'Matching projects' },
          { value: 'selected', label: 'Selected projects' },
          { value: 'spaces', label: 'Selected spaces' },
        ]}
      />
      {view.availability === 'spaces' ? (
        <SettingRow
          label='Spaces'
          htmlFor={`${id}-spaces`}
          description='Show this view for any project in a selected space, including projects in its groups and their worktrees.'
          wide
        >
          <div id={`${id}-spaces`} className='flex flex-wrap gap-4'>
            {spaceOptions.length === 0 ? (
              <span className='text-sm text-muted-foreground'>
                No spaces available. Create a space in the sidebar first.
              </span>
            ) : null}
            {spaceOptions.map((space) => {
              const checked =
                view.spaceRefs?.some((ref) => ref.sectionKey === space.sectionKey && ref.spaceId === space.spaceId) ??
                false;
              return (
                <label key={`${space.sectionKey}:${space.spaceId}`} className='flex items-center gap-2 text-sm'>
                  <Checkbox
                    checked={checked}
                    onCheckedChange={(selected) =>
                      update({
                        spaceRefs: selected
                          ? [...(view.spaceRefs ?? []), { sectionKey: space.sectionKey, spaceId: space.spaceId }]
                          : view.spaceRefs?.filter(
                              (ref) => ref.sectionKey !== space.sectionKey || ref.spaceId !== space.spaceId
                            ),
                      })
                    }
                  />
                  {space.name}
                </label>
              );
            })}
          </div>
        </SettingRow>
      ) : null}
      {source.kind === 'website' ? (
        <>
          <SelectField
            description='Project URLs are supplied in Settings → Projects. GitHub destinations use the selected project’s repository, with an optional project override.'
            label='Destination'
            value={source.destination}
            onChange={(destination) => updateSource({ destination: destination as ProjectViewSource['destination'] })}
            options={[
              { value: 'fixed', label: 'Fixed URL' },
              { value: 'project', label: 'Project URL' },
              { value: 'github-issues', label: 'GitHub Issues' },
              { value: 'github-actions', label: 'GitHub Actions' },
              { value: 'github-pulls', label: 'GitHub Pull Requests' },
            ]}
          />
          {source.destination === 'fixed' ? (
            <TextField
              label='URL'
              value={view.url}
              onChange={(url) => update({ url })}
              placeholder='https://example.com'
            />
          ) : null}
        </>
      ) : (
        <>
          {source.kind === 'dev-server' ? (
            <SelectField
              description='Detects package scripts and their package manager without running them. Select a package or override the command in Projects settings when several Storybooks exist.'
              label='Command source'
              value={source.discovery}
              onChange={(discovery) => updateSource({ discovery: discovery as ProjectViewSource['discovery'] })}
              options={[
                { value: 'command', label: 'Configured command' },
                { value: 'storybook', label: 'Detect Storybook script' },
              ]}
            />
          ) : null}
          {source.kind === 'report' || source.discovery === 'command' ? (
            <>
              <SettingRow
                label='Command'
                description={
                  source.kind === 'report'
                    ? 'Optional. Generate the report before serving it.'
                    : 'Runs on the project’s computer when you open the view.'
                }
                htmlFor={`${id}-command`}
                wide
              >
                <SettingsTextarea
                  id={`${id}-command`}
                  value={source.command}
                  onChange={(e) => updateSource({ command: e.currentTarget.value })}
                  placeholder={source.kind === 'report' ? 'bun run coverage' : 'bun run dev --port {port}'}
                />
              </SettingRow>
              <TextField
                label='Working directory'
                value={source.cwd}
                onChange={(cwd) => updateSource({ cwd })}
                description='Relative to each checkout. Use . for the project root.'
              />
            </>
          ) : null}
          {source.kind === 'report' ? (
            <>
              <TextField
                label='Report directory'
                value={source.reportDirectory}
                onChange={(reportDirectory) => updateSource({ reportDirectory })}
                placeholder='coverage'
                description='Relative to the checkout. Assets are served from this directory.'
              />
              <TextField
                label='Entry page'
                value={source.entry}
                onChange={(entry) => updateSource({ entry })}
                placeholder='index.html'
              />
            </>
          ) : (
            <TextField
              label='Ready URL'
              value={source.readinessUrl}
              onChange={(readinessUrl) => updateSource({ readinessUrl })}
              placeholder={
                source.discovery === 'storybook' ? 'Detected from the Storybook script' : 'http://127.0.0.1:{port}/'
              }
              description='Use {port} in both command and URL for an allocated port. Storybook can detect its configured port.'
            />
          )}
          <SettingRow
            label='Startup timeout'
            description='Starts when you open the view. Project settings can enable startup when the project opens. Switching views keeps the process running.'
            htmlFor={`${id}-timeout`}
          >
            <SettingsInput
              id={`${id}-timeout`}
              className='settings-control-lane'
              type='number'
              min={5}
              max={600}
              value={source.timeoutSeconds}
              onChange={(e) => updateSource({ timeoutSeconds: Number(e.currentTarget.value) })}
            />
          </SettingRow>
        </>
      )}
      <SettingRow label='Enabled' htmlFor={`${id}-enabled`}>
        <Switch id={`${id}-enabled`} checked={view.enabled} onCheckedChange={(enabled) => update({ enabled })} />
      </SettingRow>
      {editor.notice ? (
        <p role='status' className='text-sm text-muted-foreground'>
          {editor.notice}
        </p>
      ) : null}
      {editor.error ? (
        <p role='alert' className='text-sm text-destructive'>
          {editor.error}
        </p>
      ) : null}
      <div className='settings-management-actions flex-wrap py-3'>
        <div className='mr-auto flex'>
          <Button onClick={onSaveTemplate} type='button' variant='outline'>
            <IconTemplate data-icon='inline-start' />
            Save as template
          </Button>
        </div>
        <Button onClick={onCancel} type='button' variant='outline'>
          <IconX data-icon='inline-start' />
          Cancel
        </Button>
        <Button onClick={onSave} type='button'>
          <IconCheck data-icon='inline-start' />
          {editor.id ? 'Save changes' : 'Add view'}
        </Button>
      </div>
    </div>
  );
}
