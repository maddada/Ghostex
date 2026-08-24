import { expect, waitFor } from 'storybook/test';
import type { SidebarToExtensionMessage } from '../../shared/session-grid-contract';
import { getSidebarStoryMessages } from '../sidebar-story-harness';

/*
 * CDXC:SidebarV2 2026-07-29:
 * V2 needs its own readiness helper: the shared one waits for
 * `[data-sidebar-group-id]`, which is a V1 project section and never renders
 * while the Inbox sidebar is active. Waiting on the V2 root instead keeps V1's
 * helper untouched.
 */
export async function waitForSidebarV2(storyRoot: ParentNode): Promise<HTMLElement> {
  let root: HTMLElement | undefined;
  await waitFor(
    () => {
      const stack = storyRoot.querySelector('.stack');
      const v2Root = storyRoot.querySelector('[data-sidebar-version="v2"]');
      expect(stack).toBeTruthy();
      expect(stack).toHaveAttribute('data-dimmed', 'false');
      expect(v2Root).toBeTruthy();
      root = v2Root as HTMLElement;
      return expect(v2Root).toBeTruthy();
    },
    { timeout: 20_000 }
  );
  if (!root) {
    throw new Error('Sidebar V2 root never rendered');
  }
  return root;
}

export async function findSidebarV2Row(storyRoot: ParentNode, sessionId: string): Promise<HTMLElement> {
  let row: HTMLElement | undefined;
  await waitFor(() => {
    const element = storyRoot.querySelector(`.sidebar-v2-row[data-session-id="${sessionId}"]`);
    expect(element).toBeTruthy();
    row = element as HTMLElement;
    return expect(element).toBeTruthy();
  });
  if (!row) {
    throw new Error(`Sidebar V2 row ${sessionId} never rendered`);
  }
  return row;
}

/**
 * Settings writes travel as `updateSettingsPatch` with a nested `patch` object,
 * which the shared `expectMessage` helper cannot match because it compares
 * values by reference. Assert on the nested key directly.
 */
export async function expectSettingsPatch(key: string, value: unknown): Promise<void> {
  await waitFor(() => {
    const matched = getSidebarStoryMessages().some((message: SidebarToExtensionMessage) => {
      if (message.type !== 'updateSettingsPatch') {
        return false;
      }
      const patch = message.patch as Record<string, unknown> | undefined;
      return patch?.[key] === value;
    });
    return expect(matched).toBe(true);
  });
}

/**
 * CDXC:SidebarV2LogicalProjects 2026-07-29:
 * `sidebarProjectGroupingOverrides` is a RECORD, so neither the shared message
 * matcher nor `expectSettingsPatch` above can see inside it. This asserts on
 * the one entry a click was supposed to write, and on the source, because a
 * grouping write must never be attributed to the sidebar-version source (only
 * that source is allowed to change the sidebar the user is looking at).
 */
export async function expectProjectGroupingOverridePatch(expected: Readonly<Record<string, string>>): Promise<void> {
  await waitFor(() => {
    const matched = getSidebarStoryMessages().some((message: SidebarToExtensionMessage) => {
      if (message.type !== 'updateSettingsPatch' || message.source !== 'sidebar:projectGrouping') {
        return false;
      }
      const overrides = (message.patch as Record<string, unknown> | undefined)?.['sidebarProjectGroupingOverrides'] as
        Record<string, string> | undefined;
      if (!overrides) {
        return false;
      }
      return Object.entries(expected).every(([key, mode]) => overrides[key] === mode);
    });
    return expect(matched).toBe(true);
  });
}
