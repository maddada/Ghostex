import { IconX } from '@tabler/icons-react';
import { useRef } from 'react';
import {
  getSidebarSessionLifecycleState,
  type SidebarPreviousSessionItem,
  type SidebarSessionItem,
} from '../shared/session-grid-contract';
import {
  getSessionCardTitleTooltip,
  OverflowTooltipText,
  SessionCardContent,
  SessionFloatingAgentIcon,
  shouldShowTerminalSessionIcon,
} from './session-card-content';
import { getSessionHistoryCardTitle } from './session-history-card-title';
import { getEffectiveSessionTag } from './session-tag-ui';

export type SessionHistoryCardProps = {
  displayTimestamp?: string;
  fileSizeBytes?: number | null;
  isSearchSelected?: boolean;
  onDelete?: () => void;
  onPointerMove?: () => void;
  onRestore: () => void;
  projectLabel?: string;
  quickAccessSessionKey?: string;
  session: SidebarPreviousSessionItem | SidebarSessionItem;
  showDebugSessionNumbers: boolean;
};

export function SessionHistoryCard({
  displayTimestamp,
  fileSizeBytes,
  isSearchSelected = false,
  onDelete,
  onPointerMove,
  onRestore,
  projectLabel: suppliedProjectLabel,
  quickAccessSessionKey,
  session,
  showDebugSessionNumbers,
}: SessionHistoryCardProps) {
  const aliasHeadingRef = useRef<HTMLDivElement>(null);
  const isClosedSession = 'historyId' in session;
  const lifecycleState = isClosedSession ? 'closed' : getSidebarSessionLifecycleState(session);
  const canActivate = !isClosedSession || session.isRestorable;
  const displayTitle = getSessionHistoryCardTitle(session);
  const titleDisplaySession =
    session.displayTitle?.trim() || session.primaryTitle?.trim() || !session.terminalTitle?.trim()
      ? session
      : {
          ...session,
          primaryTitle: session.terminalTitle,
          terminalTitle: undefined,
        };
  const displaySession = displayTimestamp
    ? { ...titleDisplaySession, lastInteractionAt: displayTimestamp }
    : titleDisplaySession;
  const sessionTitleTooltip = getSessionCardTitleTooltip({
    alwaysShowTitleTooltip: true,
    session: displaySession,
    showDebugSessionNumbers,
    showSessionDetails: true,
  });
  const projectLabel = suppliedProjectLabel ?? (isClosedSession ? getSessionHistoryProjectLabel(session) : undefined);
  const effectiveSessionTag = getEffectiveSessionTag(session);
  const showTerminalSessionIcon = shouldShowTerminalSessionIcon(session);
  const hasSessionCardIcon =
    session.isPinned === true ||
    Boolean(effectiveSessionTag) ||
    Boolean(session.agentIcon) ||
    showTerminalSessionIcon ||
    session.isReloading === true;
  /**
   * CDXC:PreviousSessions 2026-05-13-16:11:
   * Previous Sessions rows place project metadata and transcript size on the
   * right before Last Active, so the title column stays dedicated to the
   * session title while useful context remains visible during scanning.
   *
   * CDXC:PreviousSessions 2026-06-09-09:41:
   * Tagged Previous Sessions rows must advertise the same leading identity
   * state as live sidebar rows. The tag glyph is visible at rest, and hover or
   * keyboard focus reveals the session's agent/terminal icon in that same
   * slot.
   */

  return (
    <OverflowTooltipText
      text={sessionTitleTooltip.headingText}
      textRef={aliasHeadingRef}
      tooltip={sessionTitleTooltip.tooltip}
      tooltipWhen={sessionTitleTooltip.tooltipWhen}
    >
      <div
        className='session-frame session-history-frame'
        data-focused='false'
        data-has-agent-icon={String(hasSessionCardIcon)}
        data-has-project-label={String(Boolean(projectLabel))}
        data-pinned={String(session.isPinned === true)}
        data-running={String(!isClosedSession)}
        data-session-lifecycle={lifecycleState}
        data-restorable={String(canActivate)}
        data-tagged={String(Boolean(effectiveSessionTag))}
        data-visible='false'
      >
        {/**
         * CDXC:PreviousSessions 2026-05-09-17:44
         * History rows are archived restore entries. Render the leading icon
         * as identity only, and never let stale live-session visible/focused
         * state make previous-session cards look like active UI rows.
         *
         * CDXC:PreviousSessions 2026-05-11-09:04
         * Sidebar search and the modal must show every previous-session button
         * with the same row chrome; active/live highlights are misleading here
         * because these rows restore history instead of representing open UI.
         */}
        <article
          aria-disabled={!canActivate}
          aria-pressed='false'
          aria-label={canActivate ? `${isClosedSession ? 'Restore' : 'Focus'} ${displayTitle}` : displayTitle}
          className='session session-history-card'
          data-has-agent-icon={String(hasSessionCardIcon)}
          data-dragging='false'
          data-focused='false'
          data-pinned={String(session.isPinned === true)}
          data-running={String(!isClosedSession)}
          data-session-lifecycle={lifecycleState}
          data-search-selected={String(isSearchSelected)}
          data-sidebar-history-id={isClosedSession ? session.historyId : undefined}
          data-quick-access-session-key={quickAccessSessionKey}
          data-restorable={String(canActivate)}
          data-tagged={String(Boolean(effectiveSessionTag))}
          data-visible='false'
          onAuxClick={(event) => {
            if (!onDelete || event.button !== 1) {
              return;
            }

            event.preventDefault();
            event.stopPropagation();
            onDelete();
          }}
          onClick={() => {
            if (!canActivate) {
              return;
            }

            onRestore();
          }}
          onKeyDown={(event) => {
            if (!canActivate || (event.key !== 'Enter' && event.key !== ' ')) {
              return;
            }

            event.preventDefault();
            onRestore();
          }}
          onMouseDown={(event) => {
            if (!onDelete || event.button !== 1) {
              return;
            }

            event.preventDefault();
          }}
          onPointerMove={onPointerMove}
          role={canActivate ? 'button' : undefined}
          tabIndex={canActivate ? 0 : -1}
        >
          {/**
           * CDXC:PreviousSessions 2026-06-05-14:21:
           * Inline Previous Sessions search rows must match project-session row
           * icon placement on both macOS and Electron. Keep the floating
           * identity glyph inside the clickable session button so absolute
           * positioning uses the same containing block and cannot overlap the
           * title text.
           */}
          <SessionFloatingAgentIcon
            agentIcon={session.agentIcon}
            faviconDataUrl={session.faviconDataUrl}
            isFavorite={session.isFavorite}
            sessionTag={session.sessionTag}
            sessionPersistenceName={session.sessionPersistenceName}
            sessionPersistenceProvider={session.sessionPersistenceProvider}
            showTerminalIcon={showTerminalSessionIcon}
          />
          {onDelete ? (
            <button
              aria-label={`Delete ${displayTitle} from session history`}
              className='previous-session-delete-button'
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onDelete();
              }}
              type='button'
            >
              <IconX aria-hidden='true' size={14} stroke={1.9} />
            </button>
          ) : null}
          <SessionCardContent
            aliasHeadingRef={aliasHeadingRef}
            hideHeaderAgentIcon={true}
            session={displaySession}
            showDebugSessionNumbers={showDebugSessionNumbers}
            showCloseButton={false}
            showLastInteractionTime={true}
            trailingPrefix={
              <>
                {projectLabel ? (
                  <div className='session-history-project-label' aria-hidden='true'>
                    {projectLabel}
                  </div>
                ) : (
                  <div aria-hidden='true' className='session-history-project-label' />
                )}
                <div
                  aria-label={
                    typeof fileSizeBytes === 'number'
                      ? `Transcript file size ${fileSizeBytes.toLocaleString()} bytes`
                      : fileSizeBytes === null
                        ? 'Transcript file unavailable'
                        : 'Loading transcript file size'
                  }
                  className='session-history-file-size'
                  data-loading={String(fileSizeBytes === undefined)}
                  title={typeof fileSizeBytes === 'number' ? `${fileSizeBytes.toLocaleString()} bytes` : undefined}
                >
                  {formatSessionFileSize(fileSizeBytes)}
                </div>
              </>
            }
          />
        </article>
      </div>
    </OverflowTooltipText>
  );
}

function formatSessionFileSize(sizeBytes: number | null | undefined): string {
  if (sizeBytes === undefined) {
    return '…';
  }
  if (sizeBytes === null) {
    return '-';
  }
  if (sizeBytes < 1_024) {
    return `${sizeBytes} B`;
  }
  const units = ['KB', 'MB', 'GB'];
  let value = sizeBytes / 1_024;
  let unitIndex = 0;
  while (value >= 1_024 && unitIndex < units.length - 1) {
    value /= 1_024;
    unitIndex += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unitIndex]}`;
}

function getSessionHistoryProjectLabel(session: SidebarPreviousSessionItem): string | undefined {
  const projectName = session.projectName?.trim();
  if (projectName) {
    return projectName;
  }

  const projectPath = session.projectPath?.trim();
  if (!projectPath) {
    return undefined;
  }

  const pathParts = projectPath.split(/[\\/]/u).filter(Boolean);
  return pathParts[pathParts.length - 1] ?? projectPath;
}
