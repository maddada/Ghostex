import type { CSSProperties } from 'react';
import { IconArrowUpRight, IconPuzzle, IconTrash } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { cn } from '@/packages/components/utils';
import type { GhostexExtensionCatalogEntry, GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';

/** Same chrome gray as titlebar extension glyphs (`normalize_extension_titlebar_svg`). */
const EXTENSION_ICON_COLOR = '#b9b9b9';

function extensionIconMaskStyle(src: string): CSSProperties {
  const maskImage = `url(${JSON.stringify(src)})`;
  return {
    backgroundColor: EXTENSION_ICON_COLOR,
    maskImage,
    maskPosition: 'center',
    maskRepeat: 'no-repeat',
    maskSize: 'contain',
    WebkitMaskImage: maskImage,
    WebkitMaskPosition: 'center',
    WebkitMaskRepeat: 'no-repeat',
    WebkitMaskSize: 'contain',
  };
}

export function ExtensionIcon({ className, src, title }: { className?: string; src?: string; title: string }) {
  const iconClassName = cn(
    'extensions-icon flex size-9 shrink-0 items-center justify-center p-1.5 text-[#b9b9b9]',
    className
  );
  return src ? (
    <span aria-label={`${title} icon`} className={iconClassName} role='img'>
      <span aria-hidden='true' className='size-full' style={extensionIconMaskStyle(src)} />
    </span>
  ) : (
    <span aria-label={`${title} icon`} className={iconClassName} role='img'>
      <IconPuzzle aria-hidden='true' className='size-4' />
    </span>
  );
}

function placementLabel(extension: GhostexInstalledExtension): string {
  if (extension.manifest.kind === 'terminal-pane') {
    return extension.state.terminalPlacement === 'tab' ? 'New terminal tab' : 'Terminal split';
  }
  const placement = extension.state.placement ?? extension.manifest.defaultPlacement;
  if (placement === 'chat-bar') return 'Chat bar';
  return placement[0].toUpperCase() + placement.slice(1);
}

export function InstalledExtensionCard({
  extension,
  iconUrl,
  onDetails,
  onRemove,
  onSetChatBarAutoOpen,
  onSetEnabled,
  pending,
}: {
  extension: GhostexInstalledExtension;
  iconUrl?: string;
  onDetails: () => void;
  onRemove: () => void;
  onSetChatBarAutoOpen: (autoOpen: boolean) => void;
  onSetEnabled: (enabled: boolean) => void;
  pending?: boolean;
}) {
  const supportsChatBar = extension.manifest.placements?.includes('chat-bar') === true;
  return (
    <div
      className='extensions-row group/row flex min-h-20 items-center gap-3 px-3 py-2.5 transition-colors'
      data-extension-id={extension.id}
    >
      <span
        aria-hidden='true'
        className={cn('size-1.5 shrink-0 rounded-full', extension.state.enabled ? 'bg-emerald-400/80' : 'bg-white/20')}
      />
      <ExtensionIcon src={iconUrl} title={extension.manifest.title} />
      <div className='min-w-0 flex-1'>
        <div className='flex min-w-0 items-baseline gap-2'>
          <span className='truncate text-sm font-normal text-foreground'>{extension.manifest.title}</span>
          <span className='shrink-0 text-xs font-normal text-muted-foreground'>
            {extension.state.enabled ? 'Enabled' : 'Disabled'}
          </span>
        </div>
        <p className='mt-0.5 truncate text-[13px] font-normal text-foreground/75'>{extension.manifest.description}</p>
        <p className='mt-0.5 truncate text-xs font-normal text-muted-foreground'>
          {[
            `Version ${extension.state.version}`,
            placementLabel(extension),
            supportsChatBar && extension.state.chatBarAutoOpen ? 'Opens automatically in sessions' : undefined,
          ]
            .filter(Boolean)
            .join(' · ')}
        </p>
      </div>
      <div className='flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover/row:opacity-100 group-focus-within/row:opacity-100'>
        {supportsChatBar ? (
          <div className='mr-1 flex items-center gap-2 text-xs font-normal text-muted-foreground'>
            Auto-open
            <Switch
              aria-label={`${extension.state.chatBarAutoOpen ? 'Disable' : 'Enable'} automatic opening for ${extension.manifest.title}`}
              checked={extension.state.chatBarAutoOpen}
              disabled={pending}
              onCheckedChange={onSetChatBarAutoOpen}
              size='sm'
            />
          </div>
        ) : null}
        <Button className='font-normal' disabled={pending} onClick={onDetails} size='sm' type='button' variant='ghost'>
          Details
        </Button>
        <Button disabled={pending} onClick={onRemove} size='icon-sm' type='button' variant='ghost'>
          <IconTrash />
          <span className='sr-only'>Remove</span>
        </Button>
      </div>
      <div className='ml-1 flex shrink-0 items-center gap-2'>
        <span className='text-xs font-normal text-muted-foreground'>{extension.state.enabled ? 'On' : 'Off'}</span>
        <Switch
          aria-label={`${extension.state.enabled ? 'Disable' : 'Enable'} ${extension.manifest.title}`}
          checked={extension.state.enabled}
          disabled={pending}
          onCheckedChange={onSetEnabled}
          size='sm'
        />
      </div>
    </div>
  );
}

export function StoreExtensionCard({
  entry,
  iconUrl,
  installedVersion,
  onDetails,
}: {
  entry: GhostexExtensionCatalogEntry;
  iconUrl?: string;
  installedVersion?: string;
  onDetails: () => void;
}) {
  const metadata = [entry.author, `Version ${entry.version}`, ...entry.categories.slice(0, 2)];
  return (
    <div
      className='extensions-row group/row flex min-h-20 items-center gap-3 px-3 py-2.5 transition-colors'
      data-extension-id={entry.name}
    >
      <span
        aria-hidden='true'
        className={cn('size-1.5 shrink-0 rounded-full', installedVersion ? 'bg-emerald-400/80' : 'bg-white/20')}
      />
      <ExtensionIcon src={iconUrl} title={entry.title} />
      <div className='min-w-0 flex-1'>
        <div className='flex min-w-0 items-baseline gap-2'>
          <span className='truncate text-sm font-normal text-foreground'>{entry.title}</span>
          {installedVersion ? (
            <span className='shrink-0 text-xs font-normal text-muted-foreground'>Installed</span>
          ) : null}
        </div>
        <p className='mt-0.5 truncate text-[13px] font-normal text-foreground/75'>{entry.description}</p>
        <p className='mt-0.5 truncate text-xs font-normal text-muted-foreground'>{metadata.join(' · ')}</p>
      </div>
      <div className='flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover/row:opacity-100 group-focus-within/row:opacity-100'>
        <Button className='font-normal' onClick={onDetails} size='sm' type='button' variant='ghost'>
          Details
          <IconArrowUpRight data-icon='inline-end' />
        </Button>
      </div>
      {installedVersion ? (
        <span className='ml-1 shrink-0 text-xs font-normal text-muted-foreground'>
          {installedVersion === entry.version ? 'Up to date' : `Installed ${installedVersion}`}
        </span>
      ) : null}
    </div>
  );
}
