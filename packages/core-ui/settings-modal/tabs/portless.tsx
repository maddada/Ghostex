import { useId } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { ToggleGroup, ToggleGroupItem } from '@/packages/components/ui/toggle-group';
import { IconCircleX, IconDownload, IconInfoCircle, IconRefresh, IconTools, IconTrash } from '@tabler/icons-react';
import { type SidebarPortlessState, type SidebarProjectSettingsItem } from '../../../shared/session-grid-contract';
import { type PortlessProtocol, type ghostexSettings } from '../../../shared/ghostex-settings';
import {
  type NativePortlessAdminAction,
  type NativePortlessAdminInstallAction,
} from '../../../shared/native-ghostty-host-protocol';
import { SettingButton } from '../fields';

export type PortlessSettingsDomainSummary = {
  domains: readonly {
    hostname: string;
    liveRoutes: readonly {
      kind: 'primary' | 'additional';
      port: number;
    }[];
  }[];
  kind: 'project' | 'worktree';
  projectId: string;
  title: string;
};

export const PORTLESS_PROTOCOL_OPTIONS: readonly { label: string; value: PortlessProtocol }[] = [
  { label: 'HTTPS', value: 'https' },
  { label: 'HTTP', value: 'http' },
];

export const PORTLESS_SETTINGS_RECOMMENDED_ADMIN_ACTIONS: readonly NativePortlessAdminInstallAction[] = [
  'install',
  'reconfigure',
  'retry',
];

export const PORTLESS_SETTINGS_ADMIN_ACTION_LABELS: Record<NativePortlessAdminAction, string> = {
  install: 'Install',
  reconfigure: 'Reconfigure',
  remove: 'Remove background proxy',
  retry: 'Retry',
};

/*
 * CDXC:PortlessSettingsDisabled 2026-07-25:
 * Preserve the complete Portless Settings implementation for a later return,
 * but do not expose its controls while the app integration is disabled.
 */
export const PORTLESS_SETTINGS_VISIBLE = false;

export function PortlessGlobalSettingsPanel({
  domainSummaries,
  onAdminAction,
  onEnabledChange,
  onProtocolChange,
  portless,
  settings,
}: {
  domainSummaries: readonly PortlessSettingsDomainSummary[];
  onAdminAction: (action: NativePortlessAdminAction) => void;
  onEnabledChange: (checked: boolean) => void;
  onProtocolChange: (protocol: PortlessProtocol) => void;
  portless?: SidebarPortlessState;
  settings: ghostexSettings;
}) {
  const portlessToggleId = useId();
  const portlessProtocolLabelId = useId();
  const status = getPortlessSettingsStatus(portless, settings);
  const recommendedAction = getPortlessRecommendedSettingsAdminAction(portless);
  const showRemoveAction = portless?.health.setupOwnership === 'ghostex';
  const removeAvailability = portless?.nativeAdmin.actions.remove;

  return (
    <section className='settings-modal-section settings-projects-global-settings'>
      <div className='settings-projects-global-header'>
        <div className='settings-management-header-text'>
          {/*
            CDXC:PortlessSettings 2026-06-30-11:42:
            Projects global settings should title the Portless card as Portless and briefly define it, because the controls manage Ghostex's local-domain proxy rather than generic project metadata.
          */}
          <h3 className='settings-management-heading'>Portless</h3>
          <p className='settings-management-description'>
            Portless gives projects and worktrees stable local domains for dev servers through Ghostex's background
            proxy.
          </p>
        </div>
        <span className='settings-portless-status-badge' data-status={status.tone}>
          {status.label}
        </span>
      </div>
      <div className='settings-projects-global-body'>
        <div className='settings-portless-control-row'>
          <div className='settings-management-main'>
            <label className='settings-management-title' htmlFor={portlessToggleId}>
              Portless
            </label>
            <span className='settings-management-detail'>
              Create stable local domains for running project and worktree dev servers.
            </span>
          </div>
          <Switch checked={settings.portlessEnabled} id={portlessToggleId} onCheckedChange={onEnabledChange} />
        </div>
        <div className='settings-portless-control-row'>
          <div className='settings-management-main'>
            <span className='settings-management-title' id={portlessProtocolLabelId}>
              Protocol
            </span>
            <span className='settings-management-detail'>
              Choose the standard local web port the background proxy should use.
            </span>
          </div>
          <ToggleGroup
            aria-labelledby={portlessProtocolLabelId}
            className='settings-portless-protocol-toggle'
            onValueChange={(value) => {
              const [protocol] = value as PortlessProtocol[];
              if (protocol) {
                onProtocolChange(protocol);
              }
            }}
            value={[settings.portlessProtocol]}
            variant='outline'
          >
            {PORTLESS_PROTOCOL_OPTIONS.map((option) => (
              <ToggleGroupItem key={option.value} value={option.value}>
                {option.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
        <div className='settings-portless-status-row'>
          <IconInfoCircle aria-hidden='true' />
          <div className='settings-management-main'>
            <span className='settings-management-title'>Setup status</span>
            <span className='settings-management-detail'>{status.detail}</span>
          </div>
        </div>
        <div className='settings-portless-actions' aria-label='Portless actions'>
          {recommendedAction ? (
            <PortlessSettingsAdminActionButton
              action={recommendedAction}
              availability={portless?.nativeAdmin.actions[recommendedAction]}
              onAdminAction={onAdminAction}
            />
          ) : null}
          {settings.portlessEnabled ? (
            <Button onClick={() => onEnabledChange(false)} type='button' variant='outline'>
              <IconCircleX aria-hidden='true' />
              Disable
            </Button>
          ) : null}
          {showRemoveAction ? (
            <PortlessSettingsAdminActionButton
              action='remove'
              availability={removeAvailability}
              onAdminAction={onAdminAction}
            />
          ) : null}
        </div>
        <PortlessAssignedDomainsSummary
          domainSummaries={domainSummaries}
          routePreviewStatus={portless?.presentation?.routePreviewStatus}
          settings={settings}
        />
      </div>
    </section>
  );
}

export function PortlessSettingsAdminActionButton({
  action,
  availability,
  onAdminAction,
}: {
  action: NativePortlessAdminAction;
  availability?: SidebarPortlessState['nativeAdmin']['actions'][NativePortlessAdminAction];
  onAdminAction: (action: NativePortlessAdminAction) => void;
}) {
  const Icon =
    action === 'install'
      ? IconDownload
      : action === 'retry'
        ? IconRefresh
        : action === 'remove'
          ? IconTrash
          : IconTools;
  const disabled = availability?.available !== true;
  const disabledReason =
    availability?.unavailableReason === 'localMacOnly'
      ? 'This action is available only on the local Mac.'
      : availability?.unavailableReason === 'setupNotGhostexOwned'
        ? 'Ghostex can’t change a setup it doesn’t own.'
        : 'No setup change is needed right now.';
  return (
    <SettingButton
      disabled={disabled}
      disabledReason={disabledReason}
      onClick={() => onAdminAction(action)}
      type='button'
      variant={action === 'remove' ? 'outline' : 'default'}
    >
      <Icon aria-hidden='true' />
      {PORTLESS_SETTINGS_ADMIN_ACTION_LABELS[action]}
    </SettingButton>
  );
}

export function PortlessAssignedDomainsSummary({
  domainSummaries,
  routePreviewStatus,
  settings,
}: {
  domainSummaries: readonly PortlessSettingsDomainSummary[];
  routePreviewStatus?: NonNullable<SidebarPortlessState['presentation']>['routePreviewStatus'];
  settings: ghostexSettings;
}) {
  const emptyMessage = getPortlessAssignedDomainsEmptyMessage(routePreviewStatus, settings);
  return (
    <div className='settings-portless-domains'>
      <div className='settings-management-main'>
        <span className='settings-management-title'>Assigned domains</span>
        <span className='settings-management-detail'>Generated project and worktree domains are read-only.</span>
      </div>
      {domainSummaries.length > 0 ? (
        <ul aria-label='Assigned Portless domains' className='settings-portless-domain-list'>
          {domainSummaries.map((summary) => (
            <li className='settings-portless-domain-group' key={summary.projectId}>
              <div className='settings-portless-domain-group-header'>
                <span className='settings-portless-domain-group-title'>{summary.title}</span>
                <span className='settings-portless-domain-group-kind'>
                  {summary.kind === 'worktree' ? 'Worktree' : 'Project'}
                </span>
              </div>
              <div className='settings-portless-domain-hosts'>
                {summary.domains.map((domain) => (
                  <div className='settings-portless-domain-host' key={domain.hostname}>
                    <code className='settings-portless-domain-hostname'>{domain.hostname}</code>
                    <span className='settings-portless-domain-meta'>
                      {domain.liveRoutes.length > 0
                        ? domain.liveRoutes
                            .map(
                              (route) => `${route.kind === 'primary' ? 'Primary' : 'Additional'} - port ${route.port}`
                            )
                            .join(', ')
                        : 'Assigned'}
                    </span>
                  </div>
                ))}
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <div className='settings-portless-domain-empty'>{emptyMessage}</div>
      )}
    </div>
  );
}

export function getPortlessSettingsStatus(
  portless: SidebarPortlessState | undefined,
  settings: ghostexSettings
): { detail: string; label: string; tone: 'active' | 'disabled' | 'failed' | 'needsSetup' | 'unknown' } {
  if (!settings.portlessEnabled) {
    return {
      detail: 'Portless is off in Ghostex settings.',
      label: 'Disabled',
      tone: 'disabled',
    };
  }
  const health = portless?.health;
  if (!health) {
    return {
      detail: 'Gxserver has not reported Portless setup metadata yet.',
      label: 'Unknown',
      tone: 'unknown',
    };
  }
  if (health.setupStatus === 'active' && health.setupOwnership === 'ghostex') {
    return {
      detail: `Ghostex is managing the ${health.protocol.toUpperCase()} background proxy.`,
      label: 'Active',
      tone: 'active',
    };
  }
  if (health.setupStatus === 'failed') {
    return {
      detail: 'Ghostex could not verify the managed background proxy.',
      label: 'Failed',
      tone: 'failed',
    };
  }
  if (health.setupStatus === 'needed' && health.setupOwnership === 'standalone') {
    return {
      detail: 'A Portless service is installed, but Ghostex is not managing it.',
      label: 'Reconfigure',
      tone: 'needsSetup',
    };
  }
  if (health.setupStatus === 'needed') {
    return {
      detail: 'Install the Ghostex-managed background proxy to assign domains.',
      label: 'Setup needed',
      tone: 'needsSetup',
    };
  }
  if (health.setupStatus === 'disabled') {
    return {
      detail: 'Portless setup is disabled in the reported runtime state.',
      label: 'Disabled',
      tone: 'disabled',
    };
  }
  return {
    detail: 'Portless setup state is not available yet.',
    label: 'Unknown',
    tone: 'unknown',
  };
}

export function getPortlessRecommendedSettingsAdminAction(
  portless: SidebarPortlessState | undefined
): NativePortlessAdminInstallAction | undefined {
  return PORTLESS_SETTINGS_RECOMMENDED_ADMIN_ACTIONS.find((action) => {
    const nativeAvailability = portless?.nativeAdmin.actions[action];
    const healthRecommendation = portless?.health.actions[action];
    return nativeAvailability?.available === true || healthRecommendation?.recommended === true;
  });
}

export function getProjectPortlessDomainSummaries(
  projects: readonly SidebarProjectSettingsItem[],
  selectedProject: SidebarProjectSettingsItem | undefined,
  portless: SidebarPortlessState | undefined
): readonly PortlessSettingsDomainSummary[] {
  const assignedDomains = portless?.presentation?.assignedDomains ?? [];
  if (!selectedProject || assignedDomains.length === 0) {
    return [];
  }
  const projectsById = new Map(projects.map((project) => [project.projectId, project]));
  const includedProjectIds = new Set<string>([selectedProject.projectId]);
  if (!selectedProject.worktreeParentProjectId) {
    for (const project of projects) {
      if (project.worktreeParentProjectId === selectedProject.projectId) {
        includedProjectIds.add(project.projectId);
      }
    }
  }
  const liveRoutesByProjectAndHostname = new Map<
    string,
    PortlessSettingsDomainSummary['domains'][number]['liveRoutes']
  >();
  for (const preview of portless?.presentation?.routePreviews ?? []) {
    const key = `${preview.projectId}\0${preview.hostname}`;
    liveRoutesByProjectAndHostname.set(key, [
      ...(liveRoutesByProjectAndHostname.get(key) ?? []),
      {
        kind: preview.kind,
        port: preview.port,
      },
    ]);
  }
  const domainsByProjectId = new Map<string, PortlessSettingsDomainSummary['domains'][number][]>();
  for (const domain of assignedDomains) {
    if (!includedProjectIds.has(domain.projectId)) {
      continue;
    }
    const domains = domainsByProjectId.get(domain.projectId) ?? [];
    if (!domains.some((existingDomain) => existingDomain.hostname === domain.hostname)) {
      domains.push({
        hostname: domain.hostname,
        liveRoutes: liveRoutesByProjectAndHostname.get(`${domain.projectId}\0${domain.hostname}`) ?? [],
      });
    }
    domainsByProjectId.set(domain.projectId, domains);
  }
  return [...domainsByProjectId.entries()].map(([projectId, domains]) => {
    const project = projectsById.get(projectId);
    return {
      domains,
      kind: project?.worktreeParentProjectId ? 'worktree' : 'project',
      projectId,
      title: project?.name ?? 'Project',
    };
  });
}

export function getPortlessAssignedDomainsEmptyMessage(
  routePreviewStatus: NonNullable<SidebarPortlessState['presentation']>['routePreviewStatus'] | undefined,
  settings: ghostexSettings
): string {
  if (!settings.portlessEnabled || routePreviewStatus === 'disabled') {
    return 'No domains are assigned while Portless is disabled.';
  }
  if (routePreviewStatus === 'unavailable' || !routePreviewStatus) {
    return 'No assigned domain metadata is available yet.';
  }
  return 'No assigned domains are available for the selected project yet.';
}

export function createPortlessSettingsAdminRequestId(action: NativePortlessAdminAction): string {
  return `portless-settings-${action}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
