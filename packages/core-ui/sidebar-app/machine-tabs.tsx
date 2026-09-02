import { IconCloud, IconDeviceDesktop, IconEyeOff, IconLoader2, IconSettings } from '@tabler/icons-react';
import { useState, type MouseEvent as ReactMouseEvent } from 'react';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { SidebarContextMenuPortal } from '../sidebar-context-menu-portal';
import { SidebarFixedTooltipButton } from '../sidebar-fixed-tooltip-button';
import type { WebviewApi } from '../webview-api';
import type { SidebarSectionSessionSummary } from './types';

/*
 * CDXC:SidebarMachineTabs 2026-08-28:
 * The machine strip is the sidebar's top-level switch: one tab for the local
 * machine plus one per saved remote machine, and the body below renders only the
 * selected machine's projects. It uses the app's single segmented single-select
 * control (see AGENTS.md) rather than a hand-rolled button row, with stretched,
 * truncating segments because machine names are user-supplied and the sidebar is
 * narrow.
 *
 * Each tab keeps the attention signal its old section header showed while
 * collapsed: working and attention counts remain visible so an unselected
 * machine still reports that something needs the user. Awake idle sessions do
 * not produce a tab count.
 *
 * CDXC:SidebarProjectMenu 2026-09-02:
 * The remote machine header is gone, so its connection control lives here: a
 * remote tab's cloud glyph is the control. It spins while connecting or
 * installing, turns red on failure with the host's sanitized reason as the
 * tab tooltip, and clicking the glyph (not the tab) retries the connection
 * whenever the machine is not connected. The tooltip is the tab's own so it
 * survives the narrow, truncating layout.
 *
 * CDXC:SidebarMachineTabMenu 2026-09-02:
 * Right-clicking a remote tab opens a small menu: Hide Machine flips that
 * machine's "Show in sidebar" setting off (the same switch the Remote settings
 * tab exposes), and Configure Machines opens that settings tab. The Local tab
 * has no menu because it cannot be hidden.
 */

export type SidebarMachineTabConnectionState = 'busy' | 'connected' | 'disconnected' | 'failed';

export type SidebarMachineTabItem = {
  /**
   * Remote machines only: what the tab tooltip says about the connection —
   * the busy step, the failure reason, or a Connect prompt while disconnected.
   */
  connectionLabel?: string;
  /** Undefined for the local machine, which has no connection lifecycle. */
  connectionState?: SidebarMachineTabConnectionState;
  id: string;
  label: string;
  /** Remote machines only: retries the connection; absent while connected or busy. */
  onConnect?: () => void;
  sessionSummary?: SidebarSectionSessionSummary;
};

function SidebarMachineTabConnectionGlyph({ item }: { item: SidebarMachineTabItem }) {
  if (item.connectionState === undefined) {
    return <IconDeviceDesktop aria-hidden='true' stroke={1.8} />;
  }
  if (item.connectionState === 'busy') {
    return (
      <span aria-busy='true' className='reference-sidebar-machine-tab-connection' data-connection='busy'>
        <IconLoader2 aria-hidden='true' stroke={1.8} />
      </span>
    );
  }
  if (!item.onConnect) {
    return <IconCloud aria-hidden='true' stroke={1.8} />;
  }
  const retry = (event: ReactMouseEvent<HTMLSpanElement>) => {
    // The glyph sits inside the tab's toggle; retrying must not also flip tabs.
    event.stopPropagation();
    item.onConnect?.();
  };
  return (
    <span
      aria-label={item.connectionState === 'failed' ? 'Retry connection' : 'Connect'}
      className='reference-sidebar-machine-tab-connection'
      data-connection={item.connectionState}
      onClick={retry}
      role='button'
    >
      <IconCloud aria-hidden='true' stroke={1.8} />
    </span>
  );
}

function SidebarMachineTabStatus({ sessionSummary }: { sessionSummary?: SidebarSectionSessionSummary }) {
  if (!sessionSummary) {
    return null;
  }

  const hasActionStatus = sessionSummary.workingCount > 0 || sessionSummary.attentionCount > 0;
  if (!hasActionStatus) {
    return null;
  }

  return (
    <span
      aria-label={[
        sessionSummary.workingCount > 0 ? `${sessionSummary.workingCount} working` : '',
        sessionSummary.attentionCount > 0 ? `${sessionSummary.attentionCount} done` : '',
      ]
        .filter(Boolean)
        .join(', ')}
      className='reference-sidebar-machine-tab-status'
    >
      {sessionSummary.workingCount > 0 ? (
        <span className='group-collapsed-status-count' data-activity='working'>
          {sessionSummary.workingCount}
        </span>
      ) : null}
      {sessionSummary.attentionCount > 0 ? (
        <span className='group-collapsed-status-count' data-activity='attention'>
          {sessionSummary.attentionCount}
        </span>
      ) : null}
    </span>
  );
}

type SidebarMachineTabMenuState = {
  machineId: string;
  x: number;
  y: number;
};

export function SidebarMachineTabs({
  items,
  onConfigureMachines,
  onHideMachine,
  onSelectMachineTab,
  selectedMachineTabId,
  vscode,
}: {
  items: readonly SidebarMachineTabItem[];
  onConfigureMachines: () => void;
  onHideMachine: (machineId: string) => void;
  onSelectMachineTab: (machineTabId: string) => void;
  selectedMachineTabId: string;
  vscode: WebviewApi;
}) {
  const [contextMenu, setContextMenu] = useState<SidebarMachineTabMenuState>();
  const dismissContextMenu = () => setContextMenu(undefined);
  return (
    <div className='reference-sidebar-machine-tabs'>
      <SegmentedControl
        aria-label='Machine'
        className='reference-sidebar-machine-tab-strip'
        onValueChange={onSelectMachineTab}
        size='sm'
        stretch={true}
        value={selectedMachineTabId}
      >
        {items.map((item) => (
          <SegmentedControlItem
            className='reference-sidebar-machine-tab min-w-0 shrink gap-1 px-2'
            data-machine-connection={item.connectionState}
            key={item.id}
            onContextMenu={
              item.connectionState === undefined
                ? undefined
                : (event) => {
                    event.preventDefault();
                    setContextMenu({ machineId: item.id, x: event.clientX, y: event.clientY });
                  }
            }
            render={
              <SidebarFixedTooltipButton
                tooltip={item.connectionLabel ? `${item.label} — ${item.connectionLabel}` : item.label}
                tooltipSide='bottom'
              />
            }
            value={item.id}
          >
            <SidebarMachineTabConnectionGlyph item={item} />
            <span className='reference-sidebar-machine-tab-label'>{item.label}</span>
            <SidebarMachineTabStatus sessionSummary={item.sessionSummary} />
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
      {contextMenu ? (
        <SidebarContextMenuPortal
          menuClassName='session-context-menu reference-sidebar-machine-tab-menu'
          menuStyle={{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px`, width: '200px' }}
          onDismiss={dismissContextMenu}
          vscode={vscode}
        >
          <button
            className='session-context-menu-item'
            onClick={() => {
              dismissContextMenu();
              onHideMachine(contextMenu.machineId);
            }}
            role='menuitem'
            type='button'
          >
            <IconEyeOff aria-hidden='true' className='session-context-menu-icon' size={14} stroke={1.9} />
            Hide Machine
          </button>
          <div className='session-context-menu-divider' role='separator' />
          <button
            className='session-context-menu-item'
            onClick={() => {
              dismissContextMenu();
              onConfigureMachines();
            }}
            role='menuitem'
            type='button'
          >
            <IconSettings aria-hidden='true' className='session-context-menu-icon' size={14} stroke={1.9} />
            Configure Machines
          </button>
        </SidebarContextMenuPortal>
      ) : null}
    </div>
  );
}
