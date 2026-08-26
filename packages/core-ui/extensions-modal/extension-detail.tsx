import { IconArrowLeft, IconPin, IconTrash } from '@tabler/icons-react';
import type { ReactNode } from 'react';
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
import { ExtensionGroup, ExtensionSectionLabel } from './extension-surface';
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
    <header className='flex items-start gap-3'>
      <Button aria-label='Back to extensions' onClick={onBack} size='icon-sm' type='button' variant='ghost'>
        <IconArrowLeft />
      </Button>
      <ExtensionIcon className='extensions-icon-lg size-11 p-2' src={iconUrl} title={title} />
      <div className='min-w-0 flex-1'>
        <h2 className='truncate text-lg font-normal text-foreground'>{title}</h2>
        <p className='mt-1 max-w-3xl text-[13px] font-normal leading-relaxed text-muted-foreground'>{description}</p>
      </div>
    </header>
  );
}

function DetailRow({ children, label }: { children: ReactNode; label: ReactNode }) {
  return (
    <div className='flex min-h-11 items-center justify-between gap-4 px-4 py-2.5'>
      <span className='shrink-0 text-sm font-normal text-foreground/90'>{label}</span>
      <div className='flex min-w-0 items-center gap-2 text-sm font-normal text-muted-foreground'>{children}</div>
    </div>
  );
}

function PermissionsList({ permissions }: { permissions: readonly GhostexExtensionPermission[] }) {
  return (
    <section aria-labelledby='extension-permissions-heading' className='flex flex-col gap-2.5'>
      <ExtensionSectionLabel id='extension-permissions-heading'>Permissions</ExtensionSectionLabel>
      <ExtensionGroup>
        {permissions.length ? (
          <ul className='extensions-group-list divide-y'>
            {permissions.map((permission) => (
              <li className='flex min-h-10 items-center gap-3 px-4 py-2.5' key={permission}>
                <span aria-hidden='true' className='size-1.5 shrink-0 rounded-full bg-white/20' />
                <span className='text-sm font-normal text-muted-foreground'>{permissionLabel(permission)}</span>
              </li>
            ))}
          </ul>
        ) : (
          <div className='flex min-h-11 items-center px-4 py-2.5 text-sm font-normal text-muted-foreground'>
            No additional permissions requested.
          </div>
        )}
      </ExtensionGroup>
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
    <div className='vertical-scroll-fade-mask min-h-0 flex-1 overflow-y-auto [--edge-fade-distance:16px]'>
      <div className='mx-auto flex max-w-5xl flex-col gap-6 p-6'>
        <DetailHeader
          description={extension.manifest.description}
          iconUrl={iconUrl}
          onBack={onBack}
          title={extension.manifest.title}
        />
        <div className='grid gap-6 lg:grid-cols-[minmax(0,1fr)_300px]'>
          <div className='flex min-w-0 flex-col gap-6'>
            {webManifest ? (
              <section aria-labelledby='extension-placement-heading' className='flex flex-col gap-2.5'>
                <ExtensionSectionLabel id='extension-placement-heading'>Placement</ExtensionSectionLabel>
                <ExtensionGroup>
                  <div className='flex min-h-14 items-center justify-between gap-4 px-4 py-3'>
                    <div>
                      <div className='text-sm font-normal text-foreground/90'>Open location</div>
                      <p className='mt-0.5 text-xs font-normal text-muted-foreground'>
                        Choose where this extension opens.
                      </p>
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
                  </div>
                </ExtensionGroup>
              </section>
            ) : (
              <section aria-labelledby='extension-terminal-placement-heading' className='flex flex-col gap-2.5'>
                <ExtensionSectionLabel id='extension-terminal-placement-heading'>
                  Terminal placement
                </ExtensionSectionLabel>
                <ExtensionGroup>
                  <div className='flex min-h-14 items-center justify-between gap-4 px-4 py-3'>
                    <div>
                      <div className='text-sm font-normal text-foreground/90'>Open location</div>
                      <p className='mt-0.5 text-xs font-normal text-muted-foreground'>
                        Choose how its terminal pane opens.
                      </p>
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
                  </div>
                </ExtensionGroup>
              </section>
            )}
            {definitions.length ? (
              <section aria-labelledby='extension-preferences-heading' className='flex flex-col gap-2.5'>
                <ExtensionSectionLabel id='extension-preferences-heading'>Preferences</ExtensionSectionLabel>
                <ExtensionGroup className='divide-y-0 p-4'>
                  <p className='mb-4 text-xs font-normal text-muted-foreground'>
                    Required preferences must be completed before first use.
                  </p>
                  <PreferencesForm definitions={definitions} onChange={setPreferences} values={preferences} />
                  <Button
                    className='mt-4 font-normal'
                    disabled={pending || missing.length > 0}
                    onClick={() => void onSetState({ preferences })}
                    size='sm'
                    type='button'
                    variant='outline'
                  >
                    Save preferences
                  </Button>
                </ExtensionGroup>
              </section>
            ) : null}
            <PermissionsList permissions={extension.state.grantedPermissions} />
          </div>
          <aside className='flex flex-col gap-2.5'>
            <ExtensionSectionLabel>Status</ExtensionSectionLabel>
            <ExtensionGroup>
              <div className='flex min-h-14 items-center justify-between gap-4 px-4 py-3'>
                <div className='flex min-w-0 items-start gap-2.5'>
                  <span
                    aria-hidden='true'
                    className={`mt-1.5 size-1.5 shrink-0 rounded-full ${extension.state.enabled ? 'bg-emerald-400/80' : 'bg-white/20'}`}
                  />
                  <div>
                    <div className='text-sm font-normal text-foreground/90'>Enabled</div>
                    <div className='mt-0.5 text-xs font-normal text-muted-foreground'>
                      Available from its configured placement.
                    </div>
                  </div>
                </div>
                <Switch
                  aria-label={`${extension.state.enabled ? 'Disable' : 'Enable'} ${extension.manifest.title}`}
                  checked={extension.state.enabled}
                  disabled={pending}
                  onCheckedChange={(enabled) => void onSetState({ enabled })}
                  size='sm'
                />
              </div>
              <div className='flex min-h-14 items-center justify-between gap-4 px-4 py-3'>
                <div className='flex min-w-0 items-start gap-2.5'>
                  <IconPin aria-hidden='true' className='mt-0.5 size-4 shrink-0 text-muted-foreground' />
                  <div>
                    <div className='text-sm font-normal text-foreground/90'>Pinned</div>
                    <div className='mt-0.5 text-xs font-normal text-muted-foreground'>
                      Show its icon in the titlebar.
                    </div>
                  </div>
                </div>
                <Switch
                  aria-label={`${extension.state.pinned ? 'Unpin' : 'Pin'} ${extension.manifest.title}`}
                  checked={extension.state.pinned}
                  disabled={pending}
                  onCheckedChange={(pinned) => void onSetState({ pinned })}
                  size='sm'
                />
              </div>
              <DetailRow label='Installed version'>
                <span>{extension.state.version}</span>
              </DetailRow>
              {updateAvailable ? (
                <div className='p-3'>
                  <Button
                    className='w-full font-normal'
                    disabled={pending}
                    onClick={() => void onUpdate()}
                    size='sm'
                    type='button'
                    variant='outline'
                  >
                    Update to {catalogEntry?.version}
                  </Button>
                </div>
              ) : null}
            </ExtensionGroup>
            <Button
              className='mt-2 self-start font-normal'
              disabled={pending}
              onClick={() => void onUninstall()}
              size='sm'
              type='button'
              variant='destructive'
            >
              <IconTrash data-icon='inline-start' />
              Uninstall
            </Button>
          </aside>
        </div>
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
    <div className='vertical-scroll-fade-mask min-h-0 flex-1 overflow-y-auto [--edge-fade-distance:16px]'>
      <div className='mx-auto flex max-w-5xl flex-col gap-6 p-6'>
        <DetailHeader description={entry.description} iconUrl={iconUrl} onBack={onBack} title={entry.title} />
        <div className='grid gap-6 lg:grid-cols-[minmax(0,1fr)_300px]'>
          <div className='flex min-w-0 flex-col gap-6'>
            {screenshotUrls.length ? (
              <section
                aria-label={`${entry.title} screenshots`}
                className='extensions-group flex gap-3 overflow-x-auto p-3'
              >
                {screenshotUrls.map((url, index) => (
                  <img
                    alt={`${entry.title} screenshot ${index + 1}`}
                    className='extensions-screenshot h-44 w-auto shrink-0 object-contain'
                    key={url}
                    src={url}
                  />
                ))}
              </section>
            ) : null}
            <section aria-labelledby='extension-readme-heading' className='flex flex-col gap-2.5'>
              <ExtensionSectionLabel id='extension-readme-heading'>About {entry.title}</ExtensionSectionLabel>
              <ExtensionGroup className='divide-y-0 p-5'>
                {loadingContent ? (
                  <p className='text-sm font-normal text-muted-foreground'>Loading extension details…</p>
                ) : readmeMarkdown ? (
                  <SessionChatMarkdown markdown={readmeMarkdown} />
                ) : (
                  <p className='text-sm font-normal text-muted-foreground'>Extension README could not be loaded.</p>
                )}
              </ExtensionGroup>
            </section>
            {changelogMarkdown ? (
              <section aria-labelledby='extension-changelog-heading' className='flex flex-col gap-2.5'>
                <ExtensionSectionLabel id='extension-changelog-heading'>Changelog</ExtensionSectionLabel>
                <ExtensionGroup className='divide-y-0 p-5'>
                  <SessionChatMarkdown markdown={changelogMarkdown} />
                </ExtensionGroup>
              </section>
            ) : null}
          </div>
          <aside className='flex flex-col gap-6'>
            <section className='flex flex-col gap-2.5'>
              <ExtensionSectionLabel>Details</ExtensionSectionLabel>
              <ExtensionGroup>
                <DetailRow label='Version'>
                  <span>{entry.version}</span>
                </DetailRow>
                <DetailRow label='Author'>
                  <span className='truncate'>{entry.author}</span>
                </DetailRow>
              </ExtensionGroup>
            </section>
            <PermissionsList permissions={entry.permissions ?? []} />
            <Button
              className='self-start font-normal'
              disabled={Boolean(installedVersion && !updateAvailable)}
              onClick={onInstall}
              size='sm'
              type='button'
              variant='outline'
            >
              {actionLabel}
            </Button>
          </aside>
        </div>
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
