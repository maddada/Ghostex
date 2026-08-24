import type { Meta } from '@storybook/react-vite';
import type { JSX } from 'react';
import { useEffect, useState } from 'react';
import { SidebarStoryHarness } from './sidebar-story-harness';
import {
  createSidebarStoryMessage,
  type SidebarStoryArgs,
  type SidebarStoryCurrentSettings,
} from './sidebar-story-fixtures';

export const DEFAULT_SIDEBAR_STORY_ARGS: SidebarStoryArgs = {
  createSessionOnSidebarDoubleClick: false,
  debuggingMode: false,
  fixture: 'default',
  highlightedVisibleCount: 1,
  isFocusModeActive: false,
  renameSessionOnDoubleClick: false,
  showCloseButtonOnSessionCards: true,
  showSessionCloseContextMenuAction: false,
  showSessionCommandCopyActions: false,
  showSessionDetailsCopyAction: false,
  theme: 'dark-blue',
  viewMode: 'grid',
  visibleCount: 1,
};

export const SIDEBAR_STORY_ARG_TYPES: NonNullable<Meta<SidebarStoryArgs>['argTypes']> = {
  createSessionOnSidebarDoubleClick: {
    control: 'boolean',
  },
  debuggingMode: {
    control: 'boolean',
  },
  fixture: {
    control: 'select',
    options: [
      'agent-icon-render',
      'combined-header-alignment',
      'combined-recent-projects',
      'combined-sparse-reference',
      'command-indicator-active',
      'default',
      'sort-toggle-demo',
      'selector-states',
      'overflow-stress',
      'scroll-end-retention',
      'empty-groups',
      'sidebar-v2-empty',
      'sidebar-v2-gxserver-unavailable',
      'sidebar-v2-inbox',
      'sidebar-v2-monorepo',
      'sidebar-v2-multi-machine',
      'sidebar-v2-project-icons',
      'sidebar-v2-row-width',
      'three-groups-stress',
    ],
  },
  highlightedVisibleCount: {
    control: 'inline-radio',
    options: [1, 2, 3, 4, 6, 9],
  },
  isFocusModeActive: {
    control: 'boolean',
  },
  renameSessionOnDoubleClick: {
    control: 'boolean',
  },
  showCloseButtonOnSessionCards: {
    control: 'boolean',
  },
  showSessionCloseContextMenuAction: {
    control: 'boolean',
  },
  showSessionCommandCopyActions: {
    control: 'boolean',
  },
  showSessionDetailsCopyAction: {
    control: 'boolean',
  },
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Sidebar version and its Group by Project sub-mode are story controls so
   * the opt-in Inbox sidebar can be inspected without editing fixtures.
   */
  /*
   * CDXC:SidebarV2Lifecycle 2026-07-29:
   * Capability is a story control so the "old gxserver" degradation can be
   * inspected without a second fixture tree.
   */
  sidebarLifecycleCapabilities: {
    control: 'inline-radio',
    options: ['absent', 'settleAndSnooze', 'settleSnoozeAndGit', 'settleSnoozeGitAndWorktree'],
  },
  sidebarV2Layout: {
    control: 'inline-radio',
    options: ['flat', 'byProject'],
  },
  sidebarVersion: {
    control: 'inline-radio',
    options: ['v1', 'v2'],
  },
  theme: {
    control: 'select',
    options: [
      'dark-1',
      'dark-2',
      'plain-light',
      'dark-green',
      'dark-blue',
      'dark-red',
      'dark-pink',
      'dark-orange',
      'light-blue',
      'light-green',
      'light-pink',
      'light-orange',
    ],
  },
  viewMode: {
    control: 'inline-radio',
    options: ['horizontal', 'vertical', 'grid'],
  },
  visibleCount: {
    control: 'inline-radio',
    options: [1, 2, 3, 4, 6, 9],
  },
};

export const SIDEBAR_STORY_DECORATORS = [
  (Story: () => JSX.Element) => (
    <SidebarStoryFrame>
      <Story />
    </SidebarStoryFrame>
  ),
];

export function renderSidebarStory(args: SidebarStoryArgs) {
  return (
    <NativeSidebarStoryShell>
      <SidebarStoryHarnessWithCurrentSettings args={args} />
    </NativeSidebarStoryShell>
  );
}

export function renderCombinedSidebarStory(args: SidebarStoryArgs) {
  return (
    <NativeSidebarStoryShell>
      <SidebarStoryHarnessWithCurrentSettings args={args} />
    </NativeSidebarStoryShell>
  );
}

function NativeSidebarStoryShell({ children }: { children: JSX.Element }) {
  return (
    <div className='native-sidebar-shell' data-sidebar-mode='combined'>
      {/*
       * CDXC:StorybookSidebarReality 2026-05-26-22:52:
       * Sidebar stories must render under the same native shell as the app.
       * Otherwise Storybook can show different button widths, edge padding,
       * and project folder icon clipping than the real sidebar webview.
       */}
      <main className='native-sidebar-main'>{children}</main>
    </div>
  );
}

function SidebarStoryHarnessWithCurrentSettings({ args }: { args: SidebarStoryArgs }) {
  const currentSettings = useCurrentSidebarSettings();
  return <SidebarStoryHarness message={createSidebarStoryMessage(args, currentSettings)} />;
}

function SidebarStoryFrame({ children }: { children: JSX.Element }) {
  const currentSettings = useCurrentSidebarSettings();
  const sidebarWidth =
    typeof currentSettings?.sidebarWidth === 'number' && Number.isFinite(currentSettings.sidebarWidth)
      ? Math.max(220, Math.min(420, currentSettings.sidebarWidth))
      : 260;

  useEffect(() => {
    /*
     * CDXC:StorybookSidebarReality 2026-05-26-22:52:
     * Storybook uses its own body classes, but native sidebar CSS relies on
     * native-sidebar-body for the root viewport contract. Apply it while a
     * sidebar story is mounted so Storybook and the app share the same chrome.
     */
    document.body.classList.add('native-sidebar-body');

    return () => {
      document.body.classList.remove('native-sidebar-body');
    };
  }, []);

  return (
    <div
      style={{
        boxSizing: 'border-box',
        height: '100vh',
        overflow: 'hidden',
        width: `${sidebarWidth}px`,
      }}
    >
      {children}
    </div>
  );
}

function useCurrentSidebarSettings(): SidebarStoryCurrentSettings | undefined {
  const [settings, setSettings] = useState<SidebarStoryCurrentSettings | undefined>();

  useEffect(() => {
    let isMounted = true;
    void fetch('/__ghostex-current-sidebar-settings', { cache: 'no-store' })
      .then((response) => response.json())
      .then((payload: unknown) => {
        if (isMounted && payload && typeof payload === 'object' && !Array.isArray(payload)) {
          /**
           * CDXC:StorybookSettings 2026-05-08-16:45
           * Storybook must render sidebar scenarios with the same persisted
           * native settings snapshot as the running ghostex app. This keeps local
           * visual checks honest for width, combined-mode visibility, theme,
           * and session-card chrome preferences.
           */
          setSettings(payload as SidebarStoryCurrentSettings);
        }
      })
      .catch(() => {
        if (isMounted) {
          setSettings(undefined);
        }
      });

    return () => {
      isMounted = false;
    };
  }, []);

  return settings;
}
