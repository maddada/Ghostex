import type { CSSProperties, ReactNode } from 'react';
import { IconArrowUpRight, IconPencil, IconPuzzle, IconTrash } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { cn } from '@/packages/components/utils';
import type { GhostexExtensionCatalogEntry, GhostexInstalledExtension } from '@/packages/shared/ghostex-extensions';
import './extension-grid.css';

/** Same chrome gray as titlebar extension glyphs (`normalize_extension_titlebar_svg`). */
const EXTENSION_ICON_COLOR = '#b9b9b9';

function extensionIconMaskStyle(src: string): CSSProperties {
  const maskImage = `url(${JSON.stringify(src)})`;
  return {
    backgroundColor: `var(--extension-icon-color, ${EXTENSION_ICON_COLOR})`,
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

/**
 * CDXC:Extensions 2026-09-24 DECISION:
 * User: every extension on the Settings Extensions page (built-in, installed, Store and the user's own views)
 * is a card in a grid, three to a row, instead of a list row. One card shape serves all four so they read as
 * one family: icon and the on/off switch (or Install) on top, title, description, an optional scope label,
 * and a footer with the type or author on the left and the row actions, which appear on hover or focus.
 * The switch is the state, so the old green status dot is gone; a card that is off dims its icon.
 */
export function ExtensionGridCard({
  actions,
  children,
  className,
  control,
  dataAttributes,
  description,
  editing,
  enabled = true,
  icon,
  leading,
  meta,
  scopeSummary,
  title,
}: {
  actions?: ReactNode;
  /** Extra content under the description, such as a secondary switch. */
  children?: ReactNode;
  className?: string;
  control?: ReactNode;
  dataAttributes?: Record<`data-${string}`, string>;
  description: ReactNode;
  editing?: boolean;
  enabled?: boolean;
  icon: ReactNode;
  leading?: ReactNode;
  meta?: ReactNode;
  scopeSummary?: string;
  title: ReactNode;
}) {
  return (
    <div
      className={cn('extension-grid-card', className)}
      data-editing={editing ? 'true' : undefined}
      data-enabled={enabled ? 'true' : 'false'}
      {...dataAttributes}
    >
      <div className='extension-grid-card-head'>
        {leading}
        <span className='extension-grid-card-icon'>{icon}</span>
        <span className='extension-grid-card-control'>{control}</span>
      </div>
      <div className='extension-grid-card-title'>{title}</div>
      <p className='extension-grid-card-description'>{description}</p>
      {children}
      {scopeSummary ? <span className='extension-grid-card-scope'>{scopeSummary}</span> : null}
      <div className='extension-grid-card-foot'>
        <span className='extension-grid-card-meta'>{meta}</span>
        {actions ? <span className='extension-grid-card-actions'>{actions}</span> : null}
      </div>
    </div>
  );
}

/**
 * Lays cards out three to a row. An inline editor passed after a card spans the whole row and, because the
 * grid packs densely, lands directly under the row that holds that card while the cards after it fill the
 * row first. That keeps the editor readable at any column count without measuring rows in script.
 */
export function ExtensionCardGrid({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className='extension-card-grid-container'>
      <div className={cn('extension-card-grid', className)}>{children}</div>
    </div>
  );
}

export function ExtensionCardGridWide({ children }: { children: ReactNode }) {
  return children ? <div className='extension-card-grid-wide'>{children}</div> : null;
}

export function InstalledExtensionCard({
  editing,
  extension,
  iconUrl,
  onDetails,
  onEditScope,
  onRemove,
  onSetChatBarAutoOpen,
  onSetEnabled,
  pending,
  scopeSummary,
}: {
  editing?: boolean;
  extension: GhostexInstalledExtension;
  iconUrl?: string;
  onDetails: () => void;
  /**
   * CDXC:Extensions 2026-09-18 DECISION:
   * User: an installed extension gets the same Edit button as a built-in view, so it can be limited to
   * selected projects or spaces. Absent on hosts that do not own the `viewScopes` setting.
   */
  onEditScope?: () => void;
  onRemove: () => void;
  onSetChatBarAutoOpen: (autoOpen: boolean) => void;
  onSetEnabled: (enabled: boolean) => void;
  pending?: boolean;
  scopeSummary?: string;
}) {
  const supportsChatBar = extension.manifest.placements?.includes('chat-bar') === true;
  const title = extension.manifest.title;
  return (
    <ExtensionGridCard
      actions={
        <>
          <Button
            className='font-normal'
            disabled={pending}
            onClick={onDetails}
            size='xs'
            type='button'
            variant='ghost'
          >
            Details
          </Button>
          {onEditScope ? (
            <Button
              aria-label={`Choose where ${title} is shown`}
              disabled={pending}
              onClick={onEditScope}
              size='icon-xs'
              type='button'
              variant='ghost'
            >
              <IconPencil />
            </Button>
          ) : null}
          <Button
            aria-label={`Remove ${title}`}
            disabled={pending}
            onClick={onRemove}
            size='icon-xs'
            type='button'
            variant='ghost'
          >
            <IconTrash />
          </Button>
        </>
      }
      control={
        /* CDXC:Settings 2026-09-09 DECISION: User: never show On or Off text beside a toggle in Settings. The switch itself is the state. */
        <Switch
          aria-label={`${extension.state.enabled ? 'Disable' : 'Enable'} ${title}`}
          checked={extension.state.enabled}
          disabled={pending}
          onCheckedChange={onSetEnabled}
          size='sm'
        />
      }
      dataAttributes={{ 'data-extension-id': extension.id }}
      description={extension.manifest.description}
      editing={editing}
      enabled={extension.state.enabled}
      icon={<ExtensionIcon src={iconUrl} title={title} />}
      meta={[extension.manifest.author, `v${extension.state.version}`, placementLabel(extension)].join(' · ')}
      scopeSummary={scopeSummary}
      title={title}
    >
      {supportsChatBar ? (
        <label className='extension-grid-card-option'>
          Open automatically in sessions
          <Switch
            aria-label={`${extension.state.chatBarAutoOpen ? 'Disable' : 'Enable'} automatic opening for ${title}`}
            checked={extension.state.chatBarAutoOpen}
            disabled={pending}
            onCheckedChange={onSetChatBarAutoOpen}
            size='sm'
          />
        </label>
      ) : null}
    </ExtensionGridCard>
  );
}

export function StoreExtensionCard({
  entry,
  iconUrl,
  installedVersion,
  installing,
  onDetails,
  onInstall,
}: {
  entry: GhostexExtensionCatalogEntry;
  iconUrl?: string;
  installedVersion?: string;
  installing?: boolean;
  onDetails: () => void;
  /** Starts the install consent flow; absent where the host cannot install from the list. */
  onInstall?: () => void;
}) {
  return (
    <ExtensionGridCard
      actions={
        <Button className='font-normal' onClick={onDetails} size='xs' type='button' variant='ghost'>
          Details
          <IconArrowUpRight data-icon='inline-end' />
        </Button>
      }
      control={
        installedVersion ? (
          <span className='extension-grid-card-status'>
            {installedVersion === entry.version ? 'Up to date' : `Installed v${installedVersion}`}
          </span>
        ) : onInstall ? (
          <Button
            className='font-normal'
            disabled={installing}
            onClick={onInstall}
            size='xs'
            type='button'
            variant='outline'
          >
            {installing ? 'Installing…' : 'Install'}
          </Button>
        ) : null
      }
      dataAttributes={{ 'data-extension-id': entry.name }}
      description={entry.description}
      icon={<ExtensionIcon src={iconUrl} title={entry.title} />}
      meta={[entry.author, `v${entry.version}`, ...entry.categories.slice(0, 1)].join(' · ')}
      title={entry.title}
    />
  );
}
