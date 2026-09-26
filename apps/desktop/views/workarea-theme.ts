import { useSyncExternalStore } from 'react';

import { getSidebarTitlebarMenuBackgroundForChrome } from '@/packages/shared/ghostex-settings';

export type WorkareaTheme = 'light' | 'dark';
const THEME_EVENT = 'ghostex-workarea-theme-changed';

/** CDXC:Theming 2026-09-13 DECISION:
 * User: Docs, Kanban and Automate follow the app's light theme.
 * The native host sends the resolved appearance at startup and on changes so these pages do not depend on the terminal or Browser content theme.
 */
export function applyWorkareaTheme(theme: WorkareaTheme, colors?: WorkareaThemeColors): void {
  document.documentElement.dataset.workareaTheme = theme;
  document.documentElement.style.colorScheme = theme;
  document.documentElement.classList.toggle('dark', theme === 'dark');
  document.body.dataset.sidebarTheme = theme === 'light' ? 'plain-light' : 'plain-dark';
  /**
   * CDXC:Theming 2026-09-22 DECISION:
   * User: the theme colours Docs, Kanban and Automate too. The host sends the resolved chrome colour
   * and the content colour with the appearance; the page background reads --app-background and the
   * chrome-like rows (Docs file list, search row, toolbars) read --app-chrome-background.
   */
  const chrome = normalizeHex(colors?.chrome);
  const content = normalizeHex(colors?.content);
  if (chrome) document.documentElement.style.setProperty('--app-chrome-background', chrome);
  else document.documentElement.style.removeProperty('--app-chrome-background');
  /**
   * CDXC:Theming 2026-09-23 SEE-ALSO:
   * Menus and popovers take the sidebar's tinted menu colour, the same one the desktop's own menus
   * use (`titlebar_popup_menu_background`, and `ChatAppearance::menu_surface` for the chat).
   */
  if (chrome)
    document.documentElement.style.setProperty(
      '--app-menu-background',
      getSidebarTitlebarMenuBackgroundForChrome(chrome)
    );
  else document.documentElement.style.removeProperty('--app-menu-background');
  if (content) document.documentElement.style.setProperty('--app-background', content);
  else document.documentElement.style.removeProperty('--app-background');
  if (colors?.glass === true) document.documentElement.dataset.windowGlass = 'true';
  else delete document.documentElement.dataset.windowGlass;
}

/** `glass`: the page is a solid card on the desktop window's glass. */
export type WorkareaThemeColors = { chrome?: string; content?: string; glass?: boolean };

function normalizeHex(value: unknown): string | undefined {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/iu.test(value) ? value.toLowerCase() : undefined;
}

export function getWorkareaTheme(): WorkareaTheme {
  return document.documentElement.dataset.workareaTheme === 'light' ? 'light' : 'dark';
}

export function installWorkareaTheme(): void {
  const target = window as Window & {
    ghostexGpui?: {
      workareaTheme?: WorkareaTheme;
      workareaChrome?: string;
      workareaContent?: string;
      workareaGlass?: boolean;
    };
  };
  const initial = target.ghostexGpui?.workareaTheme ?? new URLSearchParams(location.search).get('appTheme');
  applyWorkareaTheme(initial === 'light' ? 'light' : 'dark', {
    chrome: target.ghostexGpui?.workareaChrome,
    content: target.ghostexGpui?.workareaContent,
    glass: target.ghostexGpui?.workareaGlass,
  });
  window.addEventListener(THEME_EVENT, (event) => {
    const detail = (event as CustomEvent<WorkareaTheme | ({ theme: WorkareaTheme } & WorkareaThemeColors)>).detail;
    const theme = typeof detail === 'string' ? detail : detail?.theme;
    if (theme !== 'light' && theme !== 'dark') return;
    applyWorkareaTheme(theme, typeof detail === 'string' ? undefined : detail);
  });
}

/**
 * CDXC:Theming 2026-09-26 SEE-ALSO:
 * The app-modal host and Search by Prompt pages keep their own theming, so they take only the glass flag from the
 * host's theme event and publish it as `data-window-glass`; packages/core-ui/styles/modals-glass.css lightens them.
 */
export function installWindowGlassFlag(): void {
  const apply = (glass: boolean | undefined) => {
    if (glass === true) document.documentElement.dataset.windowGlass = 'true';
    else delete document.documentElement.dataset.windowGlass;
  };
  apply(
    (window as Window & { ghostexGpui?: { workareaGlass?: boolean } }).ghostexGpui?.workareaGlass ??
      new URLSearchParams(location.search).get('windowGlass') === '1'
  );
  window.addEventListener(THEME_EVENT, (event) => {
    const detail = (event as CustomEvent<WorkareaTheme | WorkareaThemeColors>).detail;
    if (typeof detail === 'object' && detail !== null) apply(detail.glass);
  });
}

function subscribe(listener: () => void): () => void {
  window.addEventListener(THEME_EVENT, listener);
  return () => window.removeEventListener(THEME_EVENT, listener);
}

export function useWorkareaTheme(): WorkareaTheme {
  return useSyncExternalStore(subscribe, getWorkareaTheme, () => 'dark');
}
