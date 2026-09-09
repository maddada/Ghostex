import { useId, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import type { GhostexCustomView } from '@/packages/shared/ghostex-settings/custom-views';
import { normalizeCustomViewUrl } from '@/packages/shared/ghostex-settings/custom-views';
import type { ProjectViewBinding } from '@/packages/shared/ghostex-settings/project-views';
import { SettingRow, SettingsSection, TextField } from '../fields';

export function ProjectViewSettings({
  projectId,
  parentProjectId,
  views,
  onChange,
}: {
  projectId: string;
  parentProjectId?: string;
  views: GhostexCustomView[];
  onChange: (views: GhostexCustomView[]) => void;
}) {
  return (
    <>
      {views
        .filter((view) => view.source)
        .map((view) => (
          <ProjectViewBindingEditor
            key={`${projectId}:${view.id}`}
            projectId={projectId}
            parentProjectId={parentProjectId}
            view={view}
            onSave={(binding, selected) =>
              onChange(
                views.map((candidate) =>
                  candidate.id !== view.id
                    ? candidate
                    : {
                        ...candidate,
                        projectBindings: { ...candidate.projectBindings, [projectId]: binding },
                        projectIds: selected
                          ? [...new Set([...(candidate.projectIds ?? []), projectId])]
                          : candidate.projectIds?.filter((id) => id !== projectId),
                      }
                )
              )
            }
          />
        ))}
    </>
  );
}
function ProjectViewBindingEditor({
  projectId,
  parentProjectId,
  view,
  onSave,
}: {
  projectId: string;
  parentProjectId?: string;
  view: GhostexCustomView;
  onSave: (binding: ProjectViewBinding, selected: boolean) => void;
}) {
  const id = useId();
  const [binding, setBinding] = useState<ProjectViewBinding>(view.projectBindings?.[projectId] ?? {});
  const [selected, setSelected] = useState(view.projectIds?.includes(projectId) ?? false);
  const [error, setError] = useState('');
  const parent = parentProjectId ? view.projectBindings?.[parentProjectId] : undefined;
  const inherited = parent?.inherit !== false ? parent : undefined;
  const source = view.source!;
  const update = (patch: Partial<ProjectViewBinding>) => {
    setBinding((current) => ({ ...current, ...patch }));
    setError('');
  };
  const save = () => {
    if (
      (binding.url && !normalizeCustomViewUrl(binding.url)) ||
      (binding.repositoryUrl && !normalizeCustomViewUrl(binding.repositoryUrl))
    ) {
      setError('Enter a complete HTTP or HTTPS URL.');
      return;
    }
    onSave(binding, selected);
  };
  return (
    <SettingsSection
      title={`${view.name} view`}
      description='Settings for this project. View definitions and templates are managed in Extensions.'
    >
      {view.availability === 'selected' ? (
        <SettingRow htmlFor={`${id}-available`} label='Available in this project'>
          <Switch id={`${id}-available`} checked={selected} onCheckedChange={setSelected} />
        </SettingRow>
      ) : null}
      {source.kind === 'website' ? (
        source.destination === 'project' || source.destination === 'fixed' ? (
          <TextField
            label={view.name === 'Linear' ? 'Linear URL' : 'Project URL'}
            value={binding.url ?? ''}
            onChange={(url) => update({ url })}
            placeholder={inherited?.url || view.url || 'https://example.com'}
            description='Paste the exact project, workspace, or filtered page to open.'
          />
        ) : (
          <TextField
            label='Repository URL override'
            value={binding.repositoryUrl ?? ''}
            onChange={(repositoryUrl) => update({ repositoryUrl })}
            placeholder={inherited?.repositoryUrl || 'Use the detected GitHub repository'}
          />
        )
      ) : (
        <>
          <TextField
            label='Command override'
            value={binding.command ?? ''}
            onChange={(command) => update({ command })}
            placeholder={
              inherited?.command || (source.discovery === 'storybook' ? 'Detect the Storybook script' : source.command)
            }
          />
          <TextField
            label='Working directory override'
            value={binding.cwd ?? ''}
            onChange={(cwd) => update({ cwd })}
            placeholder={inherited?.cwd || source.cwd}
            description='Relative to the checkout. Set this to select one package in a monorepo.'
          />
          {source.kind === 'dev-server' ? (
            <TextField
              label='Ready URL override'
              value={binding.readinessUrl ?? ''}
              onChange={(readinessUrl) => update({ readinessUrl })}
              placeholder={inherited?.readinessUrl || source.readinessUrl || 'Detected from the Storybook script'}
              description='Use the port from this project’s command, or {port} in both fields.'
            />
          ) : null}
          {source.kind === 'dev-server' ? (
            <SettingRow
              htmlFor={`${id}-startup`}
              label='Start when project opens'
              description='Otherwise, starts when you open this view.'
            >
              <Switch
                id={`${id}-startup`}
                checked={binding.startOnProjectOpen ?? inherited?.startOnProjectOpen ?? false}
                onCheckedChange={(startOnProjectOpen) => update({ startOnProjectOpen })}
              />
            </SettingRow>
          ) : null}
        </>
      )}
      <SettingRow
        htmlFor={`${id}-inherit`}
        label='Use for worktrees'
        description='Inherit these values; commands still run inside each worktree.'
      >
        <Switch
          id={`${id}-inherit`}
          checked={binding.inherit !== false}
          onCheckedChange={(inherit) => update({ inherit })}
        />
      </SettingRow>
      {error ? (
        <p className='text-sm text-destructive' role='alert'>
          {error}
        </p>
      ) : null}
      <div className='settings-management-actions'>
        <Button
          type='button'
          variant='outline'
          onClick={() => {
            setBinding({});
            setError('');
          }}
        >
          Clear
        </Button>
        <Button type='button' onClick={save}>
          Save view settings
        </Button>
      </div>
    </SettingsSection>
  );
}
