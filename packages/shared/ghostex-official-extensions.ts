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
 * is `false` or unset.
 */
import type { ghostexSettings } from './ghostex-settings';

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
  | 'docsViewTabHidden'
  | 'extensionsTitlebarButtonHidden'
  | 'gitActionsTitlebarButtonHidden'
  | 'kanbanViewTabHidden'
  | 'openInTitlebarButtonHidden'
  | 'quickActionsTitlebarButtonHidden'
  | 'resourcesTitlebarButtonHidden'
  | 'tipsAndTricksTitlebarButtonHidden'
>;

/** Where an official entry appears in the app once it is enabled. */
export type GhostexOfficialExtensionPlacement = 'view' | 'titlebar-button';

export type GhostexOfficialExtensionId =
  | 'automate'
  | 'browser'
  | 'code'
  | 'docs'
  | 'extensionsButton'
  | 'gitActions'
  | 'kanban'
  | 'openIn'
  | 'quickActions'
  | 'resources'
  | 'tips';

export type GhostexOfficialExtension = {
  description: string;
  id: GhostexOfficialExtensionId;
  placement: GhostexOfficialExtensionPlacement;
  settingsKey: GhostexOfficialExtensionSettingsKey;
  title: string;
};

export const GHOSTEX_OFFICIAL_EXTENSIONS: readonly GhostexOfficialExtension[] = [
  {
    description:
      'Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex.',
    id: 'code',
    placement: 'view',
    settingsKey: 'codeViewTabHidden',
    title: 'Code editor',
  },
  {
    description:
      'Open websites alongside your project and keep useful pages organized without leaving Ghostex.',
    id: 'browser',
    placement: 'view',
    settingsKey: 'browserViewTabHidden',
    title: 'Browser',
  },
  {
    description: 'Plan upcoming work and track task progress at a glance.',
    id: 'kanban',
    placement: 'view',
    settingsKey: 'kanbanViewTabHidden',
    title: 'Beads Kanban',
  },
  {
    description: 'Turn repeatable project routines into simple workflows you can run whenever you need them.',
    id: 'automate',
    placement: 'view',
    settingsKey: 'automateViewTabHidden',
    title: 'Automate',
  },
  {
    description: 'Browse your project’s notes, plans, and reference files together in one focused reading space.',
    id: 'docs',
    placement: 'view',
    settingsKey: 'docsViewTabHidden',
    title: 'Docs',
  },
  {
    description: 'Title bar button that opens short tips for getting more out of Ghostex.',
    id: 'tips',
    placement: 'titlebar-button',
    settingsKey: 'tipsAndTricksTitlebarButtonHidden',
    title: 'Tips & Tricks',
  },
  {
    description: 'Title bar button that opens Ghostex docs, guides, and community links.',
    id: 'resources',
    placement: 'titlebar-button',
    settingsKey: 'resourcesTitlebarButtonHidden',
    title: 'Resources',
  },
  {
    description: 'Title bar button for commit, branch, and worktree helpers on the active project.',
    id: 'gitActions',
    placement: 'titlebar-button',
    settingsKey: 'gitActionsTitlebarButtonHidden',
    title: 'Git actions',
  },
  {
    description: 'Title bar button that runs your saved terminal and browser actions in one click.',
    id: 'quickActions',
    placement: 'titlebar-button',
    settingsKey: 'quickActionsTitlebarButtonHidden',
    title: 'Quick Actions',
  },
  {
    description: 'Title bar button that opens the active project in another app.',
    id: 'openIn',
    placement: 'titlebar-button',
    settingsKey: 'openInTitlebarButtonHidden',
    title: 'Open In',
  },
  {
    description: 'Title bar button that opens this Extensions page.',
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
