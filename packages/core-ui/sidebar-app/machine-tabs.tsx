import { IconCloud, IconDeviceDesktop } from '@tabler/icons-react';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
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
 * collapsed — the working/attention/awake counts — compressed to small dots so
 * an unselected machine still reports that something needs the user.
 */

export type SidebarMachineTabConnectionState = 'busy' | 'connected' | 'disconnected' | 'failed';

export type SidebarMachineTabItem = {
  /** Undefined for the local machine, which has no connection lifecycle. */
  connectionState?: SidebarMachineTabConnectionState;
  id: string;
  label: string;
  sessionSummary?: SidebarSectionSessionSummary;
};

function SidebarMachineTabStatus({ sessionSummary }: { sessionSummary?: SidebarSectionSessionSummary }) {
  if (!sessionSummary) {
    return null;
  }

  const hasActionStatus = sessionSummary.workingCount > 0 || sessionSummary.attentionCount > 0;
  if (!hasActionStatus && sessionSummary.awakeCount === 0) {
    return null;
  }

  return (
    <span
      aria-label={[
        sessionSummary.workingCount > 0 ? `${sessionSummary.workingCount} working` : '',
        sessionSummary.attentionCount > 0 ? `${sessionSummary.attentionCount} done` : '',
        !hasActionStatus && sessionSummary.awakeCount > 0
          ? `${sessionSummary.awakeCount} awake terminals and browsers`
          : '',
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
      {!hasActionStatus && sessionSummary.awakeCount > 0 ? (
        <span className='group-collapsed-status-count' data-activity='awake'>
          {sessionSummary.awakeCount}
        </span>
      ) : null}
    </span>
  );
}

export function SidebarMachineTabs({
  items,
  onSelectMachineTab,
  selectedMachineTabId,
}: {
  items: readonly SidebarMachineTabItem[];
  onSelectMachineTab: (machineTabId: string) => void;
  selectedMachineTabId: string;
}) {
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
        {items.map((item) => {
          const Icon = item.connectionState === undefined ? IconDeviceDesktop : IconCloud;
          return (
            <SegmentedControlItem
              className='reference-sidebar-machine-tab min-w-0 shrink gap-1 px-2'
              data-machine-connection={item.connectionState}
              key={item.id}
              title={item.label}
              value={item.id}
            >
              <Icon aria-hidden='true' stroke={1.8} />
              <span className='reference-sidebar-machine-tab-label'>{item.label}</span>
              <SidebarMachineTabStatus sessionSummary={item.sessionSummary} />
            </SegmentedControlItem>
          );
        })}
      </SegmentedControl>
    </div>
  );
}
