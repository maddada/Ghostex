import { IconArrowLeft, IconPin, IconTrash } from '@tabler/icons-react';
import { useEffect, useMemo, useState } from 'react';
import { Button } from '@/packages/components/ui/button';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { Switch } from '@/packages/components/ui/switch';
import type {
  GhostexExtensionCatalogEntry,
  GhostexExtensionPermission,
  GhostexExtensionPreference,
  GhostexExtensionPreferenceValue,
  GhostexExtensionStatePatch,
  GhostexInstalledExtension,
} from '@/packages/shared/ghostex-extensions';
import { SessionChatMarkdown } from '@/packages/core-ui/chat/session-chat-markdown';
import { ExtensionIcon } from './extension-card';
import { missingRequiredPreferences, PreferencesForm } from './preferences-form';

function permissionLabel(permission: GhostexExtensionPermission): string {
  return {
    cli: 'Ghostex CLI',
    clipboard: 'Clipboard',
    exec: 'System commands',
    network: 'Network access',
    ssh: 'SSH access',
  }[permission];
}

function numericVersionParts(version: string): number[] {
  return version
    .split('-', 1)[0]
    .split('.')
    .map((part) => Number.parseInt(part, 10))
    .map((part) => (Number.isFinite(part) ? part : 0));
}

export function isVersionNewer(candidate: string, current: string): boolean {
  const candidateParts = numericVersionParts(candidate);
  const currentParts = numericVersionParts(current);
  const length = Math.max(candidateParts.length, currentParts.length);
  for (let index = 0; index < length; index += 1) {
    const difference = (candidateParts[index] ?? 0) - (currentParts[index] ?? 0);
    if (difference !== 0) return difference > 0;
  }
  return false;
}

function DetailHeader({
  description,
  iconUrl,
  onBack,
  title,
}: {
  description: string;
  iconUrl?: string;
  onBack: () => void;
  title: string;
}) {
  return (
    <header className='flex items-start gap-4 border-b border-border/60 pb-5'>
      <Button aria-label='Back to extensions' onClick={onBack} size='icon' type='button' variant='ghost'>
        <IconArrowLeft />
      </Button>
      <ExtensionIcon src={iconUrl} title={title} />
      <div className='min-w-0 flex-1'>
        <h2 className='truncate text-xl font-normal text-foreground'>{title}</h2>
        <p className='mt-1 max-w-3xl text-sm leading-6 text-muted-foreground'>{description}</p>
      </div>
    </header>
  );
}

function PermissionsList({ permissions }: { permissions: readonly GhostexExtensionPermission[] }) {
  return (
    <section aria-labelledby='extension-permissions-heading' className='flex flex-col gap-3'>
      <h3 className='text-sm font-medium text-foreground' id='extension-permissions-heading'>
        Permissions
      </h3>
      {permissions.length ? (
        <ul className='flex flex-wrap gap-2'>
          {permissions.map((permission) => (
            <li
              className='border border-border/70 bg-card/40 px-2.5 py-1 text-xs text-muted-foreground'
              key={permission}
            >
              {permissionLabel(permission)}
            </li>
          ))}
        </ul>
      ) : (
        <p className='text-sm text-muted-foreground'>No additional permissions requested.</p>
      )}
    </section>
  );
}

export function InstalledExtensionDetail({
  catalogEntry,
  extension,
  iconUrl,
  onBack,
  onSetState,
  onUninstall,
  onUpdate,
  pending,
}: {
  catalogEntry?: GhostexExtensionCatalogEntry;
  extension: GhostexInstalledExtension;
  iconUrl?: string;
  onBack: () => void;
  onSetState: (patch: GhostexExtensionStatePatch) => Promise<void>;
  onUninstall: () => Promise<void>;
  onUpdate: () => Promise<void>;
  pending?: boolean;
}) {
  const definitions = useMemo(() => extension.manifest.preferences ?? [], [extension.manifest.preferences]);
  const [preferences, setPreferences] = useState<Record<string, GhostexExtensionPreferenceValue>>(() =>
    preferenceValues(definitions, extension.state.preferences)
  );
  useEffect(
    () => setPreferences(preferenceValues(definitions, extension.state.preferences)),
    [definitions, extension.id, extension.state.preferences]
  );
  const missing = missingRequiredPreferences(definitions, preferences);
  const updateAvailable = Boolean(catalogEntry && isVersionNewer(catalogEntry.version, extension.state.version));
  const webManifest = extension.manifest.kind === 'terminal-pane' ? undefined : extension.manifest;

  return (
    <div className='flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-6'>
      <DetailHeader
        description={extension.manifest.description}
        iconUrl={iconUrl}
        onBack={onBack}
        title={extension.manifest.title}
      />
      <div className='grid gap-7 lg:grid-cols-[minmax(0,1fr)_280px]'>
        <div className='flex min-w-0 flex-col gap-7'>
          {webManifest ? (
            <section aria-labelledby='extension-placement-heading' className='flex flex-col gap-3'>
              <div>
                <h3 className='text-sm font-medium text-foreground' id='extension-placement-heading'>
                  Placement
                </h3>
                <p className='mt-1 text-xs text-muted-foreground'>Choose where this extension opens.</p>
              </div>
              <SegmentedControl
                onValueChange={(placement) =>
                  void onSetState({ placement: placement as (typeof webManifest.placements)[number] })
                }
                value={extension.state.placement ?? webManifest.defaultPlacement}
              >
                {webManifest.placements.map((placement) => (
                  <SegmentedControlItem key={placement} value={placement}>
                    {placement === 'chat-bar' ? 'Chat bar' : placement[0].toUpperCase() + placement.slice(1)}
                  </SegmentedControlItem>
                ))}
              </SegmentedControl>
            </section>
          ) : (
            <section aria-labelledby='extension-terminal-placement-heading' className='flex flex-col gap-3'>
              <div>
                <h3 className='text-sm font-medium text-foreground' id='extension-terminal-placement-heading'>
                  Terminal placement
                </h3>
                <p className='mt-1 text-xs text-muted-foreground'>Choose how its terminal pane opens.</p>
              </div>
              <SegmentedControl
                onValueChange={(terminalPlacement) =>
                  void onSetState({ terminalPlacement: terminalPlacement as 'splitRight' | 'tab' })
                }
                value={extension.state.terminalPlacement}
              >
                <SegmentedControlItem value='splitRight'>Split right</SegmentedControlItem>
                <SegmentedControlItem value='tab'>New tab</SegmentedControlItem>
              </SegmentedControl>
            </section>
          )}
          {definitions.length ? (
            <section aria-labelledby='extension-preferences-heading' className='flex flex-col gap-4'>
              <div>
                <h3 className='text-sm font-medium text-foreground' id='extension-preferences-heading'>
                  Preferences
                </h3>
                <p className='mt-1 text-xs text-muted-foreground'>
                  Required preferences must be completed before first use.
                </p>
              </div>
              <PreferencesForm definitions={definitions} onChange={setPreferences} values={preferences} />
              <div>
                <Button
                  disabled={pending || missing.length > 0}
                  onClick={() => void onSetState({ preferences })}
                  type='button'
                >
                  Save preferences
                </Button>
              </div>
            </section>
          ) : null}
          <PermissionsList permissions={extension.state.grantedPermissions} />
        </div>
        <aside className='flex flex-col gap-5 border-l border-border/60 pl-6'>
          <div className='flex items-center justify-between gap-4'>
            <div>
              <div className='text-sm text-foreground'>Enabled</div>
              <div className='text-xs text-muted-foreground'>Available from its configured placement.</div>
            </div>
            <Switch
              aria-label={`${extension.state.enabled ? 'Disable' : 'Enable'} ${extension.manifest.title}`}
              checked={extension.state.enabled}
              disabled={pending}
              onCheckedChange={(enabled) => void onSetState({ enabled })}
            />
          </div>
          <div className='flex items-center justify-between gap-4'>
            <div>
              <div className='flex items-center gap-1.5 text-sm text-foreground'>
                <IconPin aria-hidden='true' />
                Pinned
              </div>
              <div className='text-xs text-muted-foreground'>Show its icon in the titlebar.</div>
            </div>
            <Switch
              aria-label={`${extension.state.pinned ? 'Unpin' : 'Pin'} ${extension.manifest.title}`}
              checked={extension.state.pinned}
              disabled={pending}
              onCheckedChange={(pinned) => void onSetState({ pinned })}
            />
          </div>
          <div className='border-t border-border/60 pt-5'>
            <div className='text-xs text-muted-foreground'>Installed version</div>
            <div className='mt-1 text-sm text-foreground'>{extension.state.version}</div>
            {updateAvailable ? (
              <Button className='mt-3 w-full' disabled={pending} onClick={() => void onUpdate()} type='button'>
                Update to {catalogEntry?.version}
              </Button>
            ) : null}
          </div>
          <Button disabled={pending} onClick={() => void onUninstall()} type='button' variant='destructive'>
            <IconTrash data-icon='inline-start' />
            Uninstall
          </Button>
        </aside>
      </div>
    </div>
  );
}

export function StoreExtensionDetail({
  changelogMarkdown,
  entry,
  iconUrl,
  installedVersion,
  loadingContent,
  onBack,
  onInstall,
  readmeMarkdown,
  screenshotUrls,
}: {
  changelogMarkdown?: string;
  entry: GhostexExtensionCatalogEntry;
  iconUrl?: string;
  installedVersion?: string;
  loadingContent?: boolean;
  onBack: () => void;
  onInstall: () => void;
  readmeMarkdown?: string;
  screenshotUrls: string[];
}) {
  const updateAvailable = Boolean(installedVersion && isVersionNewer(entry.version, installedVersion));
  const actionLabel = updateAvailable ? `Update to ${entry.version}` : installedVersion ? 'Installed' : 'Install';
  return (
    <div className='flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-6'>
      <DetailHeader description={entry.description} iconUrl={iconUrl} onBack={onBack} title={entry.title} />
      <div className='grid gap-7 lg:grid-cols-[minmax(0,1fr)_280px]'>
        <div className='flex min-w-0 flex-col gap-7'>
          {screenshotUrls.length ? (
            <section aria-label={`${entry.title} screenshots`} className='flex gap-3 overflow-x-auto pb-2'>
              {screenshotUrls.map((url, index) => (
                <img
                  alt={`${entry.title} screenshot ${index + 1}`}
                  className='h-44 w-auto shrink-0 border border-border/70 object-contain'
                  key={url}
                  src={url}
                />
              ))}
            </section>
          ) : null}
          <section aria-labelledby='extension-readme-heading' className='flex flex-col gap-3'>
            <h3 className='sr-only' id='extension-readme-heading'>
              About {entry.title}
            </h3>
            {loadingContent ? (
              <p className='text-sm text-muted-foreground'>Loading extension details…</p>
            ) : readmeMarkdown ? (
              <SessionChatMarkdown markdown={readmeMarkdown} />
            ) : (
              <p className='text-sm text-muted-foreground'>Extension README could not be loaded.</p>
            )}
          </section>
          {changelogMarkdown ? (
            <section aria-labelledby='extension-changelog-heading' className='flex flex-col gap-3'>
              <h3 className='text-sm font-medium text-foreground' id='extension-changelog-heading'>
                Changelog
              </h3>
              <SessionChatMarkdown markdown={changelogMarkdown} />
            </section>
          ) : null}
        </div>
        <aside className='flex flex-col gap-5 border-l border-border/60 pl-6'>
          <div>
            <div className='text-xs text-muted-foreground'>Version</div>
            <div className='mt-1 text-sm text-foreground'>{entry.version}</div>
          </div>
          <div>
            <div className='text-xs text-muted-foreground'>Author</div>
            <div className='mt-1 text-sm text-foreground'>{entry.author}</div>
          </div>
          <PermissionsList permissions={entry.permissions ?? []} />
          <Button disabled={Boolean(installedVersion && !updateAvailable)} onClick={onInstall} type='button'>
            {actionLabel}
          </Button>
        </aside>
      </div>
    </div>
  );
}

function preferenceValues(
  definitions: readonly GhostexExtensionPreference[],
  stored: Record<string, GhostexExtensionPreferenceValue>
): Record<string, GhostexExtensionPreferenceValue> {
  const defaults = Object.fromEntries(
    definitions
      .filter((definition) => definition.default !== undefined)
      .map((definition) => [definition.name, definition.default as GhostexExtensionPreferenceValue])
  ) as Record<string, GhostexExtensionPreferenceValue>;
  return { ...defaults, ...stored };
}
