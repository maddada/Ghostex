/*
 * CDXC:Extensions 2026-08-30:
 * Settings has one Extensions page for everything a user can turn on or off:
 * the features Ghostex ships itself ("Official") and the audited third-party
 * extensions gxserver installs. The built-in features have no manifest and no
 * registry entry — they are plain settings keys — so this descriptor list is
 * what lets the Extensions page render them beside real extensions without
 * inventing an `origin`/`builtin` field on the gxserver wire contract.
 *
 * Every settings key here is inverted: the settings schema stores "hidden",
 * while the Extensions page shows "enabled". An entry is enabled when its key
 * is `false`; missing values use the settings defaults.
 */
import type { ghostexSettings } from './ghostex-settings';
import {
  PROJECT_WEBSITE_PROVIDERS,
  type ProjectWebsiteId,
  type ProjectWebsiteHiddenKey,
} from './ghostex-settings/project-websites';

type BooleanGhostexSettingsKey = {
  [Key in keyof ghostexSettings]-?: boolean extends ghostexSettings[Key] ? Key : never;
}[keyof ghostexSettings];

/**
 * The boolean visibility keys the Official entries own. `Extract` against the
 * settings type means a renamed or non-boolean key stops compiling here instead
 * of silently rendering a switch that writes nothing.
 */
export type GhostexOfficialExtensionSettingsKey = Extract<
  BooleanGhostexSettingsKey,
  | 'automateViewTabHidden'
  | 'browserViewTabHidden'
  | 'codeViewTabHidden'
  | 'devServersTitlebarButtonHidden'
  | 'docsViewTabHidden'
  | 'extensionsTitlebarButtonHidden'
  | 'gitActionsTitlebarButtonHidden'
  | 'helpTitlebarButtonHidden'
  | 'kanbanViewTabHidden'
  | 'notificationsTitlebarButtonHidden'
  | 'openInTitlebarButtonHidden'
  | 'quickActionsTitlebarButtonHidden'
  | 'resourcesTitlebarButtonHidden'
  | 'storybookViewTabHidden'
  | ProjectWebsiteHiddenKey
  | 'terminalViewTabHidden'
  | 'tipsAndTricksTitlebarButtonHidden'
>;

/** Where an official entry appears in the app once it is enabled. */
export type GhostexOfficialExtensionPlacement = 'view' | 'titlebar-button';

/**
 * CDXC:Extensions 2026-09-24 DECISION:
 * User: the built-in extensions are grouped by category on the Settings Extensions page, replacing the two
 * "Views" / "Buttons and menus" buckets. The user rejected the "⋯ menu panels" label as ugly; that group is
 * "Menus and panels". The CEF runtime is not a descriptor, so the page adds its "Shared runtime" group itself.
 */
export type GhostexOfficialExtensionCategory =
  'project-websites' | 'code-and-files' | 'planning' | 'header-buttons' | 'menus-and-panels';

export const GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES: readonly {
  id: GhostexOfficialExtensionCategory;
  label: string;
  /** The type shown on each card's footer and offered by the page's type filter. */
  typeLabel: string;
}[] = [
  { id: 'project-websites', label: 'Project websites', typeLabel: 'View' },
  { id: 'code-and-files', label: 'Code and files', typeLabel: 'View' },
  { id: 'planning', label: 'Planning and automation', typeLabel: 'View' },
  { id: 'header-buttons', label: 'Header buttons', typeLabel: 'Header button' },
  { id: 'menus-and-panels', label: 'Menus and panels', typeLabel: 'Menu item' },
];

export type GhostexOfficialExtensionId =
  | ProjectWebsiteId
  | 'automate'
  | 'browser'
  | 'code'
  | 'devServers'
  | 'docs'
  | 'extensionsButton'
  | 'gitActions'
  | 'help'
  | 'kanban'
  | 'notifications'
  | 'openIn'
  | 'quickActions'
  | 'resources'
  | 'storybook'
  | 'terminal'
  | 'tips';

export type GhostexOfficialExtension = {
  /**
   * CDXC:Titlebar 2026-09-21 DECISION:
   * User: Ask Ghostex, Tips & Tricks and Resources open as overlays from the ⋯ menu again (no longer view tabs), and stay app-wide: available in every project regardless of scope. An app-wide entry has an on/off switch and nothing else: no "where it appears" editor, because there is no project it could be narrowed to.
   */
  appWide?: true;
  category: GhostexOfficialExtensionCategory;
  description: string;
  id: GhostexOfficialExtensionId;
  placement: GhostexOfficialExtensionPlacement;
  settingsKey: GhostexOfficialExtensionSettingsKey;
  title: string;
};

export const GHOSTEX_OFFICIAL_EXTENSIONS: readonly GhostexOfficialExtension[] = [
  ...PROJECT_WEBSITE_PROVIDERS.map((provider): GhostexOfficialExtension => ({
    id: provider.id,
    title: provider.title,
    description: provider.description,
    settingsKey: provider.hiddenSettingsKey,
    placement: 'view',
    category: 'project-websites',
  })),
  {
    description:
      'Build, browse, and annotate your project’s components without a persistent development server. Appears only in projects with Storybook.',
    category: 'code-and-files',
    id: 'storybook',
    placement: 'view',
    settingsKey: 'storybookViewTabHidden',
    title: 'Storybook',
  },
  {
    description:
      'Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex.',
    category: 'code-and-files',
    id: 'code',
    placement: 'view',
    settingsKey: 'codeViewTabHidden',
    title: 'Code editor',
  },
  {
    description: 'Open websites alongside your project and keep useful pages organized without leaving Ghostex.',
    category: 'code-and-files',
    id: 'browser',
    placement: 'view',
    settingsKey: 'browserViewTabHidden',
    title: 'Browser',
  },
  {
    description: 'Plan upcoming work and track task progress at a glance.',
    category: 'planning',
    id: 'kanban',
    placement: 'view',
    settingsKey: 'kanbanViewTabHidden',
    title: 'Kanban',
  },
  {
    description: 'Turn repeatable project routines into simple workflows you can run whenever you need them.',
    category: 'planning',
    id: 'automate',
    placement: 'view',
    settingsKey: 'automateViewTabHidden',
    title: 'Automate',
  },
  {
    description: 'Browse your project’s notes, plans, and reference files together in one focused reading space.',
    category: 'code-and-files',
    id: 'docs',
    placement: 'view',
    settingsKey: 'docsViewTabHidden',
    title: 'Docs',
  },
  {
    description:
      'A command terminal beside your sessions, with its own tabs and splits, that works like the Commands pane but lives in the view panel.',
    category: 'code-and-files',
    id: 'terminal',
    placement: 'view',
    settingsKey: 'terminalViewTabHidden',
    title: 'Terminal',
  },
  {
    appWide: true,
    description: 'A panel of short tips for getting more out of Ghostex, opened from the ⋯ menu.',
    category: 'menus-and-panels',
    id: 'tips',
    placement: 'titlebar-button',
    settingsKey: 'tipsAndTricksTitlebarButtonHidden',
    title: 'Tips & Tricks',
  },
  {
    description: "A bell in the sidebar's top row that lists what your agents finished or need from you.",
    category: 'header-buttons',
    id: 'notifications',
    placement: 'titlebar-button',
    settingsKey: 'notificationsTitlebarButtonHidden',
    title: 'Notifications',
  },
  {
    appWide: true,
    description:
      'A ⋯ menu entry with sample questions that start a Ghostex Help chat: an agent explains the app or changes settings for you.',
    category: 'menus-and-panels',
    id: 'help',
    placement: 'titlebar-button',
    settingsKey: 'helpTitlebarButtonHidden',
    title: 'Ghostex Help',
  },
  {
    description:
      'A ⋯ menu panel listing development servers running on this computer. A new Browser tab shows the same list.',
    category: 'menus-and-panels',
    id: 'devServers',
    placement: 'titlebar-button',
    settingsKey: 'devServersTitlebarButtonHidden',
    title: 'Dev servers',
  },
  {
    appWide: true,
    description:
      'A ⋯ menu panel listing what Ghostex is running right now, with the CPU and memory each part is using.',
    category: 'menus-and-panels',
    id: 'resources',
    placement: 'titlebar-button',
    settingsKey: 'resourcesTitlebarButtonHidden',
    title: 'Resources',
  },
  {
    description: 'A work area header button for commit, branch, and worktree helpers on the active project.',
    category: 'header-buttons',
    id: 'gitActions',
    placement: 'titlebar-button',
    settingsKey: 'gitActionsTitlebarButtonHidden',
    title: 'Git actions',
  },
  {
    description: 'A work area header button that runs your saved terminal and browser actions in one click.',
    category: 'header-buttons',
    id: 'quickActions',
    placement: 'titlebar-button',
    settingsKey: 'quickActionsTitlebarButtonHidden',
    title: 'Quick Actions',
  },
  {
    description: 'A work area header button that opens the active project in another app.',
    category: 'header-buttons',
    id: 'openIn',
    placement: 'titlebar-button',
    settingsKey: 'openInTitlebarButtonHidden',
    title: 'Open In',
  },
  {
    description: 'An entry in the work area header’s ⋯ menu that opens this Extensions page.',
    category: 'menus-and-panels',
    id: 'extensionsButton',
    placement: 'titlebar-button',
    settingsKey: 'extensionsTitlebarButtonHidden',
    title: 'Extensions',
  },
];

/** An official entry is on when its inverted "hidden" settings key is not true. */
export function isOfficialExtensionEnabled(
  settings: ghostexSettings,
  extension: Pick<GhostexOfficialExtension, 'settingsKey'>
): boolean {
  return settings[extension.settingsKey] !== true;
}
