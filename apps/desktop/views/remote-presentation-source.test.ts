import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';

const sortableSessionCardSource = readFileSync(
  new URL('../../../packages/core-ui/sortable-session-card.tsx', import.meta.url),
  'utf8'
);
const sessionGroupSectionSource = readFileSync(
  new URL('../../../packages/core-ui/session-group-section.tsx', import.meta.url),
  'utf8'
);
const sessionCardsCssSource = readFileSync(
  new URL('../../../packages/core-ui/styles/session-cards.css', import.meta.url),
  'utf8'
);
const sidebarStoreSource = readFileSync(new URL('../../../packages/core-ui/sidebar-store.ts', import.meta.url), 'utf8');

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex).toBeGreaterThanOrEqual(0);
  expect(endIndex).toBeGreaterThan(startIndex);
  return source
    .slice(startIndex, endIndex)
    .replace(/\s+/g, ' ')
    .replace(/([([])\s+/g, '$1')
    .replace(/,?\s+([)\]])/g, '$1');
}

describe('remote presentation sidebar source', () => {
  test('marks remote cards and neutral lifecycle dots without duplicating running state', () => {
    /*
     * CDXC:RemotePresentation 2026-06-30-00:11:
     * Remote sessions need the same card and group marker in both render paths
     * so their lifecycle chrome stays scoped to remote rows without forking the
     * shared gxserver sidebar projection.
     *
     * CDXC:RemotePresentation 2026-06-30-11:15:
     * Remote running-idle rows use the bright running title treatment instead of a
     * redundant dot. Keep always-on neutral dots for remote sleeping/done states
     * while working, attention, and error continue to own their existing dots.
     */
    expect(sortableSessionCardSource).toContain('const isRemoteSession = Boolean(sessionGroup?.remoteMachineContext);');
    expect(sortableSessionCardSource).toContain('alwaysShowStateTooltip: isRemoteSession');
    expect(sortableSessionCardSource).toContain('data-remote-session={String(isRemoteSession)}');
    expect(sessionGroupSectionSource).toContain('data-remote-session={String(Boolean(group.remoteMachineContext))}');
    expect(sessionCardsCssSource).toContain(
      ".session-frame[data-remote-session='true'][data-lifecycle-state='sleeping']"
    );
    expect(sessionCardsCssSource).toContain(
      ".session-status-dot-anchored[data-remote-session='true'][data-lifecycle-state='sleeping']"
    );
    expect(sessionCardsCssSource).not.toContain("[data-remote-session='true'][data-lifecycle-state='running']");
    expect(sessionCardsCssSource).toContain("[data-remote-session='true'][data-lifecycle-state='sleeping']");
    expect(sessionCardsCssSource).toContain("[data-remote-session='true'][data-lifecycle-state='done']");
  });

  test('keeps remote rows on the shared context menu with explicit parity affordances', () => {
    /*
     * CDXC:RemoteContextMenu 2026-06-30-15:22:
     * Remote session rows should keep using the shared session-card context menu
     * while making parity affordances intentional: basic metadata actions,
     * remote lifecycle actions, timers, fork, full reload, and below-scoped bulk
     * actions should be visible from the same normalized row shape.
     */
    expect(sortableSessionCardSource).toContain('const isRemoteSession = Boolean(sessionGroup?.remoteMachineContext);');
    expect(sortableSessionCardSource).not.toContain('RemoteSessionContextMenu');
    const menuActionsSource = sourceBetween(
      sortableSessionCardSource,
      'const primaryActions: SessionContextMenuAction[] = [];',
      'const destructiveActions: SessionContextMenuAction[] = [];'
    );
    for (const label of [
      'Rename',
      'Tag as',
      'Copy details',
      'Delayed Send',
      'Close After Done',
      'Fork',
      'Full reload',
      'Sleep below',
      'Close below',
    ]) {
      expect(menuActionsSource).toContain(`label: '${label}'`);
    }
    expect(menuActionsSource).toContain("label: session.isPinned ? 'Unpin' : 'Pin'");
    expect(menuActionsSource).toContain("label: lifecycleState === 'sleeping' ? 'Wake' : 'Sleep'");
    expect(menuActionsSource).toContain("label: 'Advanced'");
    expect(menuActionsSource).toContain("label: 'Note'");
    expect(menuActionsSource).not.toContain('Pop Out Pane');
    expect(sortableSessionCardSource).toContain('supportsFork(session)');
    expect(sortableSessionCardSource).toContain('supportsFullReloadMenuAction(session, isRemoteSession)');
  });
});
