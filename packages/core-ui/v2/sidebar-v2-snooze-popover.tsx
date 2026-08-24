import { useMemo } from 'react';
import { resolveSidebarV2SnoozePresets, type SidebarV2SnoozePreset } from '../../shared/sidebar-v2-snooze';
import { SidebarContextMenuPortal } from '../sidebar-context-menu-portal';
import type { WebviewApi } from '../webview-api';

/*
 * CDXC:SidebarV2Lifecycle 2026-07-29:
 * The snooze preset popover.
 *
 * Two rules carried over from that port:
 * - Presets resolve when the popover OPENS, not when the row mounts. "In 1
 *   hour" has to mean an hour from the click; a list computed at mount would
 *   drift by however long the sidebar has been open.
 * - Each row pairs a relative label with an absolute time column ("Tomorrow" /
 *   "9:00 AM") so the user commits to a real wake time, not to a phrase.
 *
 * It rides `SidebarContextMenuPortal` rather than an in-row absolute element:
 * the sidebar list is a scroll container with clipping, and the portal already
 * owns viewport clamping, Escape/click-away dismissal, and the native
 * menu-opened/closed notifications the app depends on.
 */

export type SidebarV2SnoozePopoverPosition = {
  clientX: number;
  clientY: number;
};

export type SidebarV2SnoozePopoverProps = {
  /** Rendering clock the presets resolve against; re-resolved on each tick. */
  nowMs: number;
  onDismiss: () => void;
  onSelectPreset: (preset: SidebarV2SnoozePreset) => void;
  position: SidebarV2SnoozePopoverPosition;
  vscode: WebviewApi;
};

export function SidebarV2SnoozePopover({
  nowMs,
  onDismiss,
  onSelectPreset,
  position,
  vscode,
}: SidebarV2SnoozePopoverProps) {
  /*
   * The popover only mounts while open, so the first resolve is the open. The
   * memo is keyed on `nowMs`, so the shared 30s clock DOES re-resolve the list
   * while the popover stays open — the memo saves the work between ticks, not
   * across them. That is the behavior we want: relative labels never change
   * ("In 1 hour" stays "In 1 hour"), the absolute time column tracks the clock,
   * and the wake time the user commits is at most one tick (30s) old and always
   * strictly in the future, which is what gxserver's snooze guard requires. A
   * list frozen at open time would eventually offer a past wake time and be
   * rejected. The only visible churn is "This evening" dropping off once it is
   * within an hour of now, which is exactly when it stops being a useful choice.
   */
  const presets = useMemo(() => resolveSidebarV2SnoozePresets(nowMs), [nowMs]);

  return (
    <SidebarContextMenuPortal
      menuClassName='session-context-menu sidebar-v2-snooze-popover'
      menuStyle={{ left: `${position.clientX}px`, top: `${position.clientY}px` }}
      onDismiss={onDismiss}
      vscode={vscode}
    >
      <div className='session-context-menu-section'>
        {presets.map((preset) => (
          <button
            className='session-context-menu-item sidebar-v2-snooze-preset'
            data-snooze-preset={preset.id}
            key={preset.id}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onDismiss();
              onSelectPreset(preset);
            }}
            role='menuitem'
            type='button'
          >
            <span className='sidebar-v2-snooze-preset-label'>{preset.label}</span>
            <span className='sidebar-v2-snooze-preset-when'>{preset.whenLabel}</span>
          </button>
        ))}
      </div>
    </SidebarContextMenuPortal>
  );
}
