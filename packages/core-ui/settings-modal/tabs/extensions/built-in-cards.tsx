import { Fragment, type ReactNode } from 'react';
import {
  IconBolt,
  IconBug,
  IconChartBar,
  IconCloud,
  IconCodeDots,
  IconDatabase,
  IconDeviceDesktop,
  IconFileText,
  IconFolderOpen,
  IconGitCommit,
  IconBell,
  IconHelpCircle,
  IconInfoCircle,
  IconPalette,
  IconPencil,
  IconPlayerPlay,
  IconPuzzle,
  IconRefresh,
  IconTerminal2,
  IconWorld,
  type Icon as TablerIcon,
} from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { Switch } from '@/packages/components/ui/switch';
import { cn } from '@/packages/components/utils';
import {
  ExtensionCardGrid,
  ExtensionCardGridWide,
  ExtensionCardGroup,
  ExtensionGridCard,
  extensionFilterMatches,
  type ExtensionFilter,
  type ExtensionFilterSubject,
} from '../../../extensions-modal';
import {
  GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES,
  GHOSTEX_OFFICIAL_EXTENSIONS,
  isOfficialExtensionEnabled,
  type GhostexOfficialExtension,
  type GhostexOfficialExtensionId,
  type GhostexOfficialExtensionSettingsKey,
} from '@/packages/shared/ghostex-official-extensions';
import type { SidebarPluginSettingsItem } from '@/packages/shared/session-grid-contract';
import type { ghostexSettings } from '@/packages/shared/ghostex-settings';
import { officialViewScopeKey } from '@/packages/shared/ghostex-settings/view-scopes';
import { SettingButton } from '../../fields';

const OFFICIAL_EXTENSION_ICONS: Record<GhostexOfficialExtensionId, TablerIcon> = {
  linear: IconWorld,
  jira: IconWorld,
  github: IconGitCommit,
  sentry: IconBug,
  figma: IconPalette,
  vercel: IconCloud,
  supabase: IconDatabase,
  'github-actions': IconPlayerPlay,
  posthog: IconChartBar,
  'custom-website': IconWorld,
  automate: IconBolt,
  browser: IconWorld,
  code: IconCodeDots,
  devServers: IconWorld,
  docs: IconFileText,
  extensionsButton: IconPuzzle,
  gitActions: IconGitCommit,
  help: IconHelpCircle,
  kanban: IconPlayerPlay,
  notifications: IconBell,
  openIn: IconFolderOpen,
  quickActions: IconPlayerPlay,
  resources: IconDeviceDesktop,
  terminal: IconTerminal2,
  storybook: IconCodeDots,
  tips: IconInfoCircle,
};

/** Official entries whose runtime component the app can install or reinstall. */
const OFFICIAL_EXTENSION_RUNTIME_IDS: Partial<Record<GhostexOfficialExtensionId, SidebarPluginSettingsItem['id']>> = {
  code: 'code',
};

const SHARED_RUNTIME_LABEL = 'Shared runtime';
const CEF_TITLE = 'Chromium runtime (CEF)';
const CEF_DESCRIPTION =
  'Chromium Embedded Framework powers Ghostex web surfaces and stays on because the app requires it.';

/** Every category label a built-in card can carry, in page order, for the page's category filter. */
export const BUILT_IN_CATEGORY_LABELS = [
  ...GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES.map((category) => category.label),
  SHARED_RUNTIME_LABEL,
];

function categoryOf(extension: GhostexOfficialExtension) {
  return GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES.find((category) => category.id === extension.category)!;
}

export function builtInFilterSubject(extension: GhostexOfficialExtension): ExtensionFilterSubject {
  const category = categoryOf(extension);
  return {
    categories: [category.label],
    searchText: [extension.description],
    source: 'built-in',
    title: extension.title,
    types: [extension.placement === 'view' ? 'view' : category.id === 'header-buttons' ? 'header-button' : 'menu-item'],
  };
}

const CEF_FILTER_SUBJECT: ExtensionFilterSubject = {
  categories: [SHARED_RUNTIME_LABEL],
  searchText: [CEF_DESCRIPTION],
  source: 'built-in',
  title: CEF_TITLE,
  types: [],
};

/** How many built-in cards the filter (and the Settings search) leaves showing, and how many exist. */
export function builtInCounts(filter: ExtensionFilter, showOfficial: (key: string) => boolean) {
  const matching = GHOSTEX_OFFICIAL_EXTENSIONS.filter(
    (extension) => showOfficial(extension.id) && extensionFilterMatches(filter, builtInFilterSubject(extension))
  ).length;
  const cef = showOfficial('cef') && extensionFilterMatches(filter, CEF_FILTER_SUBJECT) ? 1 : 0;
  return { shown: matching + cef, total: GHOSTEX_OFFICIAL_EXTENSIONS.length + 1 };
}

/** The editing controls a card needs from the page, which owns the `viewScopes` setting. */
export type ViewScopeControls = {
  /** The card's scope label, or undefined while the view is available everywhere. */
  describe: (key: string) => string | undefined;
  edit: (key: string, title: string) => void;
  editingKey: string | undefined;
  /** The inline editor for this card, or null when another card (or none) is being edited. */
  renderEditor: (key: string) => ReactNode;
};

/**
 * CDXC:Extensions 2026-09-24 DECISION:
 * User: the built-in extensions are grouped by category, each category a labelled grid of cards. The
 * Refresh action (component status for the Code editor and CEF) stays on the Built-in section header.
 */
export function BuiltInExtensionGroups({
  filter,
  onReinstallPlugin,
  onToggle,
  scopeControls,
  settings,
  showOfficial,
  statusById,
}: {
  filter: ExtensionFilter;
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem['id']) => void;
  onToggle: (key: GhostexOfficialExtensionSettingsKey, hidden: boolean) => void;
  scopeControls: ViewScopeControls;
  settings: ghostexSettings;
  showOfficial: (key: string) => boolean;
  statusById: ReadonlyMap<SidebarPluginSettingsItem['id'], SidebarPluginSettingsItem>;
}) {
  const cef = statusById.get('cef');
  const showCef = showOfficial('cef') && extensionFilterMatches(filter, CEF_FILTER_SUBJECT);
  return (
    <div className='flex flex-col'>
      {GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES.map((category) => {
        const visible = GHOSTEX_OFFICIAL_EXTENSIONS.filter(
          (extension) =>
            extension.category === category.id &&
            showOfficial(extension.id) &&
            extensionFilterMatches(filter, builtInFilterSubject(extension))
        );
        if (!visible.length) return null;
        return (
          <ExtensionCardGroup
            count={visible.length}
            dataCategory={category.id}
            key={category.id}
            label={category.label}
          >
            <ExtensionCardGrid>
              {visible.map((extension) => {
                const runtimeId = OFFICIAL_EXTENSION_RUNTIME_IDS[extension.id];
                const runtime = runtimeId ? statusById.get(runtimeId) : undefined;
                const scopeKey = officialViewScopeKey(extension.id);
                // An app-wide entry has no project to be narrowed to, so it shows the switch alone.
                const scoped = extension.appWide !== true;
                return (
                  <Fragment key={extension.id}>
                    <BuiltInExtensionCard
                      description={extension.description}
                      editing={scoped && scopeControls.editingKey === scopeKey}
                      enabled={isOfficialExtensionEnabled(settings, extension)}
                      icon={OFFICIAL_EXTENSION_ICONS[extension.id]}
                      id={extension.id}
                      meta={extension.appWide ? `${category.typeLabel} · Every project` : category.typeLabel}
                      onEditScope={scoped ? () => scopeControls.edit(scopeKey, extension.title) : undefined}
                      onEnabledChange={(enabled) => onToggle(extension.settingsKey, !enabled)}
                      onReinstall={runtimeId && onReinstallPlugin ? () => onReinstallPlugin(runtimeId) : undefined}
                      reinstallAvailable={Boolean(onReinstallPlugin && runtime?.canReinstall)}
                      runtime={runtime}
                      scopeSummary={scoped ? scopeControls.describe(scopeKey) : undefined}
                      title={extension.title}
                    />
                    {scoped ? (
                      <ExtensionCardGridWide>{scopeControls.renderEditor(scopeKey)}</ExtensionCardGridWide>
                    ) : null}
                  </Fragment>
                );
              })}
            </ExtensionCardGrid>
          </ExtensionCardGroup>
        );
      })}
      {showCef ? (
        <ExtensionCardGroup count={1} dataCategory='runtime' label={SHARED_RUNTIME_LABEL}>
          <ExtensionCardGrid>
            <BuiltInExtensionCard
              description={CEF_DESCRIPTION}
              icon={IconDeviceDesktop}
              id='cef'
              onReinstall={onReinstallPlugin ? () => onReinstallPlugin('cef') : undefined}
              reinstallAvailable={Boolean(onReinstallPlugin && cef?.canReinstall)}
              runtime={cef}
              title={CEF_TITLE}
            />
          </ExtensionCardGrid>
        </ExtensionCardGroup>
      ) : null}
    </div>
  );
}

function BuiltInExtensionCard({
  description,
  editing,
  enabled,
  icon: Icon,
  id,
  meta,
  onEditScope,
  onEnabledChange,
  onReinstall,
  reinstallAvailable,
  runtime,
  scopeSummary,
  title,
}: {
  description: string;
  editing?: boolean;
  enabled?: boolean;
  icon: TablerIcon;
  id: string;
  meta?: string;
  onEditScope?: () => void;
  onEnabledChange?: (enabled: boolean) => void;
  onReinstall?: () => void;
  reinstallAvailable?: boolean;
  runtime?: SidebarPluginSettingsItem;
  scopeSummary?: string;
  title: string;
}) {
  const busy = runtime !== undefined && !['installed', 'notInstalled', 'failed'].includes(runtime.status);
  const actionLabel = runtime?.status === 'notInstalled' ? 'Install' : 'Reinstall';
  const runtimeMeta = [
    runtime?.statusLabel,
    runtime?.version ? `v${runtime.version}` : undefined,
    runtime?.errorMessage,
  ]
    .filter(Boolean)
    .join(' · ');
  return (
    <ExtensionGridCard
      actions={
        onReinstall || onEditScope ? (
          <>
            {onReinstall ? (
              <SettingButton
                className='font-normal'
                disabled={busy || !reinstallAvailable}
                disabledReason={
                  busy
                    ? `${title} is being installed.`
                    : 'This build does not provide a reinstallable remote component.'
                }
                onClick={onReinstall}
                size='xs'
                type='button'
                variant='ghost'
              >
                <IconRefresh aria-hidden='true' className={cn(busy && 'animate-spin')} data-icon='inline-start' />
                {actionLabel}
              </SettingButton>
            ) : null}
            {onEditScope ? (
              <Button
                aria-label={`Choose where ${title} is shown`}
                onClick={onEditScope}
                size='icon-xs'
                type='button'
                variant='ghost'
              >
                <IconPencil aria-hidden='true' />
              </Button>
            ) : null}
          </>
        ) : undefined
      }
      control={
        onEnabledChange && enabled !== undefined ? (
          <Switch
            aria-label={`${enabled ? 'Disable' : 'Enable'} ${title}`}
            checked={enabled}
            onCheckedChange={onEnabledChange}
            size='sm'
          />
        ) : (
          /* CDXC:Settings 2026-09-09 DECISION: User: no On or Off text beside toggles anywhere in Settings. A view that cannot be turned off shows a locked-on switch instead of an "Always on" caption. */
          <Switch aria-label={`${title} is always on`} checked disabled size='sm' />
        )
      }
      dataAttributes={{ 'data-extension-id': id }}
      description={description}
      editing={editing}
      enabled={enabled !== false}
      icon={
        <span
          aria-hidden='true'
          className='extensions-icon flex size-9 shrink-0 items-center justify-center p-1.5 text-[#b9b9b9]'
        >
          <Icon className='size-4' />
        </span>
      }
      meta={runtimeMeta || meta}
      scopeSummary={scopeSummary}
      title={title}
    />
  );
}
