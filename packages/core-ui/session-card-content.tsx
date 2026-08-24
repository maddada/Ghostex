import {
  IconClock,
  IconLoader2,
  IconMessageCircle,
  IconPin,
  IconTerminal2,
  IconWorld,
  IconX,
} from '@tabler/icons-react';
import {
  cloneElement,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type FocusEventHandler,
  type MouseEventHandler,
  type ReactElement,
  type ReactNode,
  type RefObject,
} from 'react';
import { createPortal } from 'react-dom';
import { TOOLTIP_MOTION_CLASS_NAME } from '../components/ui/tooltip-config';
import { cn } from '@/packages/components/utils';
import { DEFAULT_TERMINAL_SESSION_TITLE, type SidebarSessionItem } from '../shared/session-grid-contract';
import { getSidebarAgentNameByIcon, type SidebarAgentIcon } from '../shared/sidebar-agents';
import { AGENT_LOGOS, COLORED_AGENT_LOGOS } from './agent-logos';
import {
  getEffectiveSessionTag,
  getSidebarSessionTagLabel,
  SessionTagIcon,
  type SidebarSessionTag,
} from './session-tag-ui';
import {
  AppTooltip,
  areSidebarTooltipsSuppressed,
  SIDEBAR_TOOLTIP_DISMISS_EVENT,
  SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT,
} from './app-tooltip';
import { formatRelativeTime } from './relative-time';
import { TOOLTIP_DELAY_MS } from './tooltip-delay';
import { useRelativeTimeTick } from './use-relative-time-tick';

const SESSION_TOOLTIP_VIEWPORT_MARGIN_PX = 8;
const SESSION_TOOLTIP_TRIGGER_OFFSET_PX = 8;
const CLOSE_AFTER_DONE_ARMED_REMAINING_LABEL = '03:00';

const AGENT_SECONDARY_LABELS: Record<SidebarAgentIcon, readonly string[]> = {
  'amp-cli': ['amp', 'amp cli'],
  'antigravity-cli': ['agy', 'antigravity', 'antigravity cli'],
  browser: ['browser'],
  claude: ['claude', 'claude code'],
  codebuddy: ['codebuddy', 'code buddy'],
  'cursor-cli': ['cursor', 'cursor agent', 'cursor cli', 'cursor-agent'],
  codex: ['codex', 'codex cli', 'openai codex'],
  copilot: ['copilot', 'github copilot'],
  'factory-droid': ['droid', 'factory droid'],
  gemini: ['gemini'],
  'grok-build': ['grok', 'grok build'],
  'hermes-agent': ['hermes', 'hermes agent'],
  kiro: ['kiro', 'kiro cli', 'kiro-cli'],
  omp: ['omp'],
  opencode: ['open code', 'opencode'],
  pi: ['pi', 'π'],
  qoder: ['qoder', 'qodercli'],
  'rovo-dev': ['rovo', 'rovo dev', 'rovodev'],
};

let activeOverflowTooltipId: symbol | undefined;
let activeOverflowTooltipClose: (() => void) | undefined;
const TERMINAL_TITLE_MARKER = '∗';
const UNSYNCED_TITLE_LABEL = '(Unsynced title)';
const GHOST_PLACEHOLDER_TITLE_PATTERN = /^👻(?:\s+Terminal Session)?$/u;
const FILESYSTEM_PATH_TOOLTIP_PATTERN =
  /(?:^|\s)(?:~\/|\/(?:Applications|Library|System|Users|Volumes|etc|home|opt|private|tmp|usr|var)\/|[A-Za-z]:[\\/]|file:\/\/)/u;

type SessionTooltipStateInput = Partial<
  Pick<
    SidebarSessionItem,
    | 'isLive'
    | 'isRunning'
    | 'isSleeping'
    | 'lifecycleState'
    | 'nativePaneState'
    | 'providerSessionState'
    | 'sessionPersistenceProvider'
  >
>;

export type SessionCardContentProps = {
  aliasHeadingRef?: RefObject<HTMLDivElement | null>;
  hideHeaderAgentIcon?: boolean;
  onDelayedSendClick?: () => void;
  onClose?: () => void;
  session: SidebarSessionItem;
  showDebugSessionNumbers: boolean;
  showCloseButton: boolean;
  showLastActiveTime?: boolean;
  showLastInteractionTime?: boolean;
  trailingPrefix?: ReactNode;
};

export function SessionCardContent({
  aliasHeadingRef,
  hideHeaderAgentIcon = false,
  onDelayedSendClick,
  onClose,
  session,
  showCloseButton,
  showDebugSessionNumbers,
  showLastActiveTime = true,
  showLastInteractionTime = false,
  trailingPrefix,
}: SessionCardContentProps) {
  const isGeneratingFirstPromptTitle = session.isGeneratingFirstPromptTitle === true;
  const { headingText } = getSessionCardTitleTooltip({
    session,
    showDebugSessionNumbers,
  });
  const timerTrailingLabel = getSessionCardTimerTrailingLabel(session);
  const hasLastInteractionTime =
    timerTrailingLabel === undefined && showLastActiveTime && Boolean(session.lastInteractionAt);
  const showHeaderLoadingSpinner = session.isReloading === true || isGeneratingFirstPromptTitle;
  const showTerminalSessionIcon = !hideHeaderAgentIcon && shouldShowTerminalSessionIcon(session);
  const shouldAllowFullWidthTitle =
    timerTrailingLabel === undefined && !showLastActiveTime && !showLastInteractionTime && !trailingPrefix;
  /**
   * CDXC:DelayedSend 2026-05-30-08:33:
   * Active Delayed Send timers should show exactly one sidebar clock, in the
   * leading session identity slot. Do not promote the timer into the
   * right-side header agent slot; that duplicates the clock beside Last Active
   * and makes the row look like two separate timers.
   */
  const hasHeaderAgentIcon =
    !hideHeaderAgentIcon &&
    timerTrailingLabel === undefined &&
    !shouldAllowFullWidthTitle &&
    (Boolean(session.agentIcon) || showTerminalSessionIcon || showHeaderLoadingSpinner);
  /*
  CDXC:SidebarRelativeTime 2026-06-07-06:27:
  Session-card Last Active labels must keep aging from the client clock after the row is rendered. Pass the relative-time tick into the formatter so React Compiler cannot cache the first label, such as a newly created session's 0s, until gxserver publishes an unrelated row update.
  */
  const relativeTimeTick = useRelativeTimeTick(hasLastInteractionTime);
  const lastInteractionLabel =
    hasLastInteractionTime && session.lastInteractionAt
      ? formatRelativeTime(session.lastInteractionAt, {
          allowJustNow: false,
          nowMs: relativeTimeTick,
        }).value
      : undefined;
  /**
   * CDXC:SessionTimers 2026-06-16-01:48:
   * Delayed Send and Close After Done countdowns use the same trailing slot as
   * Last Active. Timer labels must stay visible even when Last Active is hidden;
   * Close After Done shows 03:00 while armed and switches to the live native
   * countdown after the session is actually Done/non-working.
   */
  const trailingTimeLabel = timerTrailingLabel ?? lastInteractionLabel;
  /**
   * CDXC:SidebarSessions 2026-04-28-05:18
   * Active session cards keep the icon slot as the default display and reveal
   * Last Active only on hover. Previous-session rows can request time as their
   * fixed trailing detail.
   *
   * CDXC:SidebarSessions 2026-05-07-14:57
   * Agentless terminal sessions use the terminal glyph as the default icon
   * slot, so new plain terminals have visible card identity before detection
   * assigns a real agent icon.
   *
   * CDXC:SidebarSessions 2026-05-08-11:01
   * Last Active uses one fixed visual color in session cards. Elapsed time can
   * change the text label, but must not recolor the timestamp by age.
   *
   * CDXC:SidebarSessions 2026-05-15-08:57
   * Users can hide active session-card Last Active timestamps from Settings.
   * Gate only this timestamp label; trailing prefixes such as project metadata
   * and separate project-header git diff stats remain outside this visibility
   * control.
   *
   * CDXC:SidebarSessions 2026-05-15-09:22
   * When Last Active is hidden for active session cards, the title owns the
   * full card width. Do not keep the header agent icon's trailing column in
   * that mode; the leading floating icon still carries session identity.
   */
  const defaultTrailingDisplay = timerTrailingLabel || (showLastInteractionTime && trailingTimeLabel) ? 'time' : 'icon';
  const shouldKeepLoadingIconVisible = showHeaderLoadingSpinner && hasHeaderAgentIcon;
  const hoverTrailingDisplay = shouldKeepLoadingIconVisible
    ? 'icon'
    : defaultTrailingDisplay === 'icon'
      ? trailingTimeLabel
        ? 'time'
        : 'icon'
      : hasHeaderAgentIcon
        ? 'icon'
        : 'time';
  /**
   * CDXC:SidebarSessions 2026-05-09-16:55
   * Session rows expose close as hover chrome for project and chat cards. The
   * button renders in the header layer so it can outrank Last Active and agent
   * indicators without reserving a permanent title slot.
   *
   * CDXC:SidebarSessions 2026-05-09-18:09
   * Close belongs in the same trailing slot as Last Active and header icons so
   * it aligns to the established right-side title affordance and can hide those
   * competing indicators as a single hover state.
   */
  const canCloseFromCard = showCloseButton && Boolean(onClose) && timerTrailingLabel === undefined;
  const hasSessionHeadTrailing =
    Boolean(trailingPrefix) || Boolean(trailingTimeLabel) || hasHeaderAgentIcon || canCloseFromCard;

  return (
    <>
      <div className='session-head' data-title-full-width={String(shouldAllowFullWidthTitle)}>
        {/**
         * CDXC:PreviousSessions 2026-05-09-17:44
         * Previous Sessions rows use this shared sidebar title row but must not
         * show the agent icon in the trailing slot. Their trailing slot is
         * reserved for Last Active, matching the confirmed modal layout.
         */}
        <div className='session-alias-heading' ref={aliasHeadingRef}>
          {headingText}
        </div>
        {hasSessionHeadTrailing ? (
          <div
            className='session-head-trailing'
            data-default-trailing-display={defaultTrailingDisplay}
            data-hover-trailing-display={hoverTrailingDisplay}
            data-timer-trailing={String(timerTrailingLabel !== undefined)}
          >
            {trailingPrefix}
            {trailingTimeLabel ? <div className='session-last-interaction-time'>{trailingTimeLabel}</div> : null}
            {hasHeaderAgentIcon ? (
              <SessionHeaderAgentIcon
                agentIcon={session.agentIcon}
                faviconDataUrl={session.faviconDataUrl}
                isGeneratingFirstPromptTitle={session.isGeneratingFirstPromptTitle}
                isReloading={session.isReloading}
                sessionPersistenceName={session.sessionPersistenceName}
                sessionPersistenceProvider={session.sessionPersistenceProvider}
                showTerminalIcon={showTerminalSessionIcon}
              />
            ) : null}
            {canCloseFromCard ? (
              <button
                aria-label='Close session'
                className='session-card-close-button'
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  onClose?.();
                }}
                type='button'
              >
                <IconX aria-hidden='true' size={14} stroke={1.8} />
              </button>
            ) : null}
          </div>
        ) : null}
      </div>
      {isGeneratingFirstPromptTitle ? (
        <div className='session-title-generation-overlay' role='status' aria-label='Generating title'>
          {/**
           * CDXC:SessionTitleLoading 2026-05-08-09:07
           * First-prompt title generation should look like the real sidebar
           * title text with a subtle blue state color. The label owns the
           * progress cue through looping dots, so it must not render the extra
           * left-side spinner that made the row typography feel mismatched.
           */}
          <span className='session-title-generation-overlay-label'>
            Generating title
            <span className='session-title-generation-overlay-dots' aria-hidden='true'>
              <span>.</span>
              <span>.</span>
              <span>.</span>
            </span>
          </span>
        </div>
      ) : null}
    </>
  );
}

function getSessionCardTimerTrailingLabel(
  session: Pick<
    SidebarSessionItem,
    | 'closeAfterDone'
    | 'closeAfterDoneDeadlineAt'
    | 'closeAfterDoneRemainingLabel'
    | 'delayedSendDeadlineAt'
    | 'delayedSendRemainingLabel'
  >
): string | undefined {
  if (session.delayedSendRemainingLabel) {
    /*
     * Send-when-finished triggers do not have a countdown while their agent
     * scope is still working. Keep that state on the Delayed Send icon and in
     * its tooltip instead of rendering prose in the session button's compact
     * trailing-time slot.
     */
    return isDelayedSendWaitingLabel(session.delayedSendRemainingLabel) ? undefined : session.delayedSendRemainingLabel;
  }
  if (session.delayedSendDeadlineAt) {
    return formatSessionTimerDeadlineCountdown(session.delayedSendDeadlineAt);
  }
  if (session.closeAfterDoneRemainingLabel) {
    return session.closeAfterDoneRemainingLabel;
  }
  if (session.closeAfterDoneDeadlineAt) {
    return formatSessionTimerDeadlineCountdown(session.closeAfterDoneDeadlineAt);
  }
  return session.closeAfterDone === true ? CLOSE_AFTER_DONE_ARMED_REMAINING_LABEL : undefined;
}

function isDelayedSendWaitingLabel(remainingLabel: string): boolean {
  return remainingLabel === 'Waiting for agent' || remainingLabel === 'Waiting for agents';
}

function getDelayedSendTooltipText(remainingLabel?: string): string {
  if (remainingLabel === 'Waiting for agent') {
    return 'Delayed Send: the prompt will be sent when the agent finishes working';
  }
  if (remainingLabel === 'Waiting for agents') {
    return 'Delayed Send: the prompt will be sent when all agents finish working';
  }
  if (remainingLabel) {
    return `Delayed Send: the prompt will be sent in ${remainingLabel}`;
  }
  return 'Delayed Send is scheduled';
}

function formatSessionTimerDeadlineCountdown(deadlineAt: string): string | undefined {
  const deadlineMs = Date.parse(deadlineAt);
  return Number.isNaN(deadlineMs) ? undefined : formatSessionTimerCountdown(deadlineMs - Date.now());
}

function formatSessionTimerCountdown(delayMs: number): string {
  const totalSeconds = Math.max(0, Math.ceil(delayMs / 1_000));
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  const paddedMinutes = String(minutes).padStart(2, '0');
  const paddedSeconds = String(seconds).padStart(2, '0');
  if (hours > 0) {
    return `${String(hours).padStart(2, '0')}:${paddedMinutes}:${paddedSeconds}`;
  }
  return `${paddedMinutes}:${paddedSeconds}`;
}

export function getSessionCardTitleTooltip({
  alwaysShowTitleTooltip = false,
  alwaysShowStateTooltip = false,
  session,
  showDebugSessionNumbers,
  showSessionDetails = false,
}: {
  alwaysShowTitleTooltip?: boolean;
  alwaysShowStateTooltip?: boolean;
  session: Pick<
    SidebarSessionItem,
    | 'activityLabel'
    | 'agentIcon'
    | 'agentSessionId'
    | 'alias'
    | 'closeAfterDone'
    | 'closeAfterDoneRemainingLabel'
    | 'delayedSendRemainingLabel'
    | 'detail'
    | 'displayTitle'
    | 'displayTitleTooltip'
    | 'firstUserMessage'
    | 'isFavorite'
    | 'kind'
    | 'isPrimaryTitleTerminalTitle'
    | 'primaryTitle'
    | 'sessionKind'
    | 'sessionTag'
    | 'sessionRoutingId'
    | 'sessionPersistenceName'
    | 'sessionPersistenceProvider'
    | 'sessionNumber'
    | 'terminalTitle'
  > &
    SessionTooltipStateInput & {
      projectName?: string;
      projectPath?: string;
    };
  showDebugSessionNumbers: boolean;
  showSessionDetails?: boolean;
}): {
  headingText: string;
  tooltip?: string;
  tooltipWhen: 'always' | 'overflow';
} {
  const headingText = formatSessionHeadingText({
    agentIcon: session.agentIcon,
    displayTitle: session.displayTitle,
    displayTitleTooltip: session.displayTitleTooltip,
    includeUnsyncedTitleLabel: false,
    kind: session.kind,
    isPrimaryTitleTerminalTitle: session.isPrimaryTitleTerminalTitle,
    primaryTitle: session.primaryTitle,
    sessionKind: session.sessionKind,
    terminalTitle: session.terminalTitle,
    alias: session.alias,
  });
  const tooltipHeadingText = formatSessionHeadingText({
    agentIcon: session.agentIcon,
    displayTitle: session.displayTitle,
    displayTitleTooltip: session.displayTitleTooltip,
    includeUnsyncedTitleLabel: true,
    kind: session.kind,
    isPrimaryTitleTerminalTitle: session.isPrimaryTitleTerminalTitle,
    primaryTitle: session.primaryTitle,
    sessionKind: session.sessionKind,
    terminalTitle: session.terminalTitle,
    alias: session.alias,
  });
  const fullTooltipHeadingText = getFullSessionTooltipHeadingText({
    firstUserMessage: session.firstUserMessage,
    headingText: formatSessionTagTooltipHeadingText(session, tooltipHeadingText),
  });
  /**
   * CDXC:PreviousSessions 2026-05-08-16:07
   * Previous-session search cards need scannable restore context in their
   * title tooltip: archived agent, source project, and persistence provider
   * must be visible without exposing extra columns in the compact result row.
   *
   * CDXC:SessionTooltips 2026-05-31-06:25:
   * macOS gxserver session-card tooltips should show the full routed session id
   * such as S7k-P3a91-G8v20 when available. The legacy two-digit display id is
   * only a visual row shortcut and should not replace the routed identity.
   *
   * CDXC:SessionTooltips 2026-06-14-16:26:
   * Session identifiers and provider names are Debugging Mode-only tooltip
   * metadata, and filesystem paths should not appear in session-card tooltips.
   * Keep safe semantic details visible while suppressing routed ids, captured
   * agent ids, provider session names, and project paths by default.
   *
   * CDXC:SessionTooltips 2026-06-14-16:56:
   * Session status lines expose provider/surface internals, so show `State: ...`
   * only while Debugging Mode is enabled.
   *
   * CDXC:RemotePresentation 2026-06-30-00:11:
   * Remote sidebar rows need their title tooltip to expose the terminal state
   * without enabling Debugging Mode. Keep IDs and provider names debug-only,
   * but allow callers to opt into the non-private state label for remote
   * sessions.
   */
  const sessionIdTooltipValue = session.sessionRoutingId?.trim() || session.sessionNumber?.trim();
  const sessionIdTooltip =
    showDebugSessionNumbers && sessionIdTooltipValue ? `ID: ${sessionIdTooltipValue}` : undefined;
  const agentSessionIdTooltip = getCapturedAgentSessionIdTooltipText(session, showDebugSessionNumbers);
  const tooltipMetadata = [
    /*
     * CDXC:DelayedSend 2026-05-21-12:21:
     * Session-row hover tooltips must surface an active Delayed Send countdown
     * directly below the title, even when the user is not hovering the clock
     * icon itself, so pending Enter timing is visible from the normal card hover.
     */
    session.delayedSendRemainingLabel ? getDelayedSendTooltipText(session.delayedSendRemainingLabel) : undefined,
    /*
     * CDXC:CloseAfterDone 2026-06-15-21:00:
     * Close After Done uses the same leading clock slot as Delayed Send, but
     * the card tooltip must still expose whether it is merely armed or actively
     * counting down after the session has stayed Done.
     */
    session.closeAfterDoneRemainingLabel
      ? `Close After Done in ${session.closeAfterDoneRemainingLabel}`
      : session.closeAfterDone
        ? 'Close After Done armed'
        : undefined,
    getSessionStateTooltipText(session, showDebugSessionNumbers || alwaysShowStateTooltip),
    getSessionTooltipSecondaryText(session),
    ...(showSessionDetails ? getSessionDetailsTooltipLines(session) : []),
    agentSessionIdTooltip,
    sessionIdTooltip ? undefined : getSessionPersistenceTooltipText(session, showDebugSessionNumbers),
  ]
    .filter((value): value is string => Boolean(value))
    .join('\n');
  const titleTooltip = buildSessionTitleTooltip({
    headingText: fullTooltipHeadingText,
    secondaryText: tooltipMetadata,
    sessionIdTooltip,
  });
  const titleTooltipOptions = getSessionTitleTooltipOptions({
    alwaysShowTitleTooltip,
    headingText,
    titleTooltip,
  });

  return {
    headingText,
    ...titleTooltipOptions,
  };
}

function getCapturedAgentSessionIdTooltipText(
  session: Pick<SidebarSessionItem, 'agentSessionId'>,
  showDebugSessionNumbers: boolean
): string | undefined {
  if (!showDebugSessionNumbers) {
    return undefined;
  }
  const agentSessionId = session.agentSessionId?.trim();
  return agentSessionId || undefined;
}

function formatSessionTagTooltipHeadingText(
  session: Pick<SidebarSessionItem, 'isFavorite' | 'sessionTag'>,
  headingText: string
): string {
  const sessionTag = getEffectiveSessionTag(session);
  const label = getSidebarSessionTagLabel(sessionTag);
  if (!label) {
    return headingText;
  }

  /**
   * CDXC:SessionTags 2026-06-05-12:30:
   * Session-card hover tooltips prefix the title with the active tag, for
   * example `[Todo]`, without changing the visible row title.
   */
  return `[${label}] ${headingText}`;
}

function getFullSessionTooltipHeadingText({
  firstUserMessage,
  headingText,
}: {
  firstUserMessage?: string;
  headingText: string;
}): string {
  /**
   * CDXC:SessionTooltips 2026-05-15-15:57:
   * Active and Previous session-card tooltips must show the full human title line when the visible session title has already been shortened with an ellipsis. First-prompt auto titles can preserve only the shortened card label, so use the saved first user message as the full tooltip heading only when it clearly starts with the displayed truncated prefix.
   */
  const normalizedFirstUserMessage = firstUserMessage?.trim().replace(/\s+/g, ' ');
  if (!normalizedFirstUserMessage) {
    return headingText;
  }

  const unsyncedLabelSuffix = ` ${UNSYNCED_TITLE_LABEL}`;
  const headingWithoutUnsyncedLabel = headingText.endsWith(unsyncedLabelSuffix)
    ? headingText.slice(0, -unsyncedLabelSuffix.length)
    : headingText;
  const normalizedHeading = headingWithoutUnsyncedLabel.trim();
  const truncatedPrefix = normalizedHeading.replace(/(?:\.\.\.|…)$/u, '').trim();
  if (
    truncatedPrefix.length > 0 &&
    truncatedPrefix.length < normalizedFirstUserMessage.length &&
    truncatedPrefix !== normalizedHeading &&
    normalizedFirstUserMessage.toLowerCase().startsWith(truncatedPrefix.toLowerCase())
  ) {
    const fullHeading = normalizedHeading.startsWith(TERMINAL_TITLE_MARKER)
      ? `${TERMINAL_TITLE_MARKER} ${normalizedFirstUserMessage}`
      : normalizedFirstUserMessage;
    return headingText.endsWith(unsyncedLabelSuffix) ? `${fullHeading} ${UNSYNCED_TITLE_LABEL}` : fullHeading;
  }

  return headingText;
}

export function formatSessionHeadingText({
  agentIcon,
  alias,
  displayTitle,
  displayTitleTooltip,
  includeUnsyncedTitleLabel = false,
  kind,
  isPrimaryTitleTerminalTitle,
  primaryTitle,
  sessionKind,
  terminalTitle,
}: Pick<
  SidebarSessionItem,
  | 'agentIcon'
  | 'alias'
  | 'displayTitle'
  | 'displayTitleTooltip'
  | 'kind'
  | 'isPrimaryTitleTerminalTitle'
  | 'primaryTitle'
  | 'sessionKind'
  | 'terminalTitle'
> & {
  includeUnsyncedTitleLabel?: boolean;
}): string {
  const gxserverDisplayTitle = normalizeDisplayTitle(displayTitle);
  if (gxserverDisplayTitle) {
    /*
    CDXC:GxserverSessionTitles 2026-06-07-09:33:
    gxserver presentation rows are dumb-rendered title strings. When `displayTitle` is present, React must not compare titleSource, terminalTitle, or placeholder state locally; the server already applied the shared title rules and unsynced marker.
    */
    return includeUnsyncedTitleLabel
      ? (normalizeDisplayTitle(displayTitleTooltip) ?? gxserverDisplayTitle)
      : gxserverDisplayTitle;
  }

  const primaryHeadingTitle = normalizeSessionCardHeadingTitle(primaryTitle);
  const terminalHeadingTitle = normalizeSessionCardHeadingTitle(terminalTitle);
  const aliasHeadingTitle = normalizeSessionCardHeadingTitle(alias);
  const normalizedPrimaryTitle = primaryHeadingTitle.text;
  const normalizedTerminalTitle = terminalHeadingTitle.text;
  const baseHeadingTitle = normalizedPrimaryTitle ? primaryHeadingTitle : aliasHeadingTitle;
  const baseHeadingText = baseHeadingTitle.text || alias;
  const isBrowserSession = kind === 'browser' || sessionKind === 'browser';
  if (baseHeadingTitle.isGhostPlaceholder) {
    return formatNonPersistentSessionHeadingText(baseHeadingText, includeUnsyncedTitleLabel);
  }

  if (
    isBrowserSession ||
    isPrimaryTitleTerminalTitle ||
    !normalizedPrimaryTitle ||
    normalizedPrimaryTitle === normalizedTerminalTitle
  ) {
    return baseHeadingText;
  }

  return formatNonPersistentSessionHeadingText(baseHeadingText, includeUnsyncedTitleLabel);
}

function normalizeDisplayTitle(title: string | undefined): string | undefined {
  const normalizedTitle = title?.trim().replace(/\s+/g, ' ');
  return normalizedTitle || undefined;
}

function formatNonPersistentSessionHeadingText(headingText: string, includeUnsyncedTitleLabel: boolean): string {
  return includeUnsyncedTitleLabel
    ? `${TERMINAL_TITLE_MARKER} ${headingText} ${UNSYNCED_TITLE_LABEL}`
    : `${TERMINAL_TITLE_MARKER} ${headingText}`;
}

function normalizeSessionCardHeadingTitle(title: string | undefined): {
  isGhostPlaceholder: boolean;
  text?: string;
} {
  const normalizedTitle = title?.trim().replace(/\s+/g, ' ');
  if (!normalizedTitle) {
    return { isGhostPlaceholder: false };
  }

  /**
   * CDXC:SidebarSessions 2026-05-07-14:48
   * Ghost placeholder titles are UI-only session defaults, not meaningful
   * terminal titles. Sidebar cards must render them with the existing
   * non-persistent title marker as `∗ Terminal Session` instead of exposing
   * the ghost emoji as the card title.
   */
  if (GHOST_PLACEHOLDER_TITLE_PATTERN.test(normalizedTitle)) {
    return {
      isGhostPlaceholder: true,
      text: DEFAULT_TERMINAL_SESSION_TITLE,
    };
  }

  return {
    isGhostPlaceholder: false,
    text: normalizedTitle,
  };
}

export function buildSessionTitleTooltip({
  headingText,
  secondaryText,
  sessionIdTooltip,
}: {
  headingText: string;
  secondaryText?: string;
  sessionIdTooltip?: string;
}): string {
  /**
   * CDXC:Tooltips 2026-05-07-18:16
   * Session title tooltips can wrap inside the narrow sidebar, so separate each
   * logical metadata row with a blank line. Splitting metadata blocks first keeps
   * authored line breaks visible while making row boundaries readable after
   * wrapping.
   */
  const uniqueLines = [headingText, secondaryText, sessionIdTooltip].reduce<string[]>((lines, block) => {
    const normalizedBlockLines =
      block
        ?.split(/\r?\n/u)
        .map((line) => line.trim())
        .filter(Boolean) ?? [];

    return normalizedBlockLines.reduce<string[]>((nextLines, normalizedLine) => {
      if (nextLines.includes(normalizedLine)) {
        return nextLines;
      }

      return [...nextLines, normalizedLine];
    }, lines);
  }, []);

  return uniqueLines.join('\n\n');
}

export function getSessionTooltipSecondaryText(
  session: Pick<SidebarSessionItem, 'activityLabel' | 'agentIcon' | 'detail' | 'terminalTitle'>
): string | undefined {
  const detail = stripAgentTooltipText(session.detail, session.agentIcon);
  if (detail && !containsFilesystemPath(detail)) {
    return detail;
  }

  const terminalHeadingTitle = normalizeSessionCardHeadingTitle(session.terminalTitle);
  const terminalTitle = terminalHeadingTitle.isGhostPlaceholder
    ? undefined
    : stripAgentTooltipText(terminalHeadingTitle.text, session.agentIcon);
  if (terminalTitle && !containsFilesystemPath(terminalTitle)) {
    return terminalTitle;
  }

  return session.activityLabel?.trim() || undefined;
}

function containsFilesystemPath(value: string): boolean {
  return FILESYSTEM_PATH_TOOLTIP_PATTERN.test(value);
}

function getSessionStateTooltipText(session: SessionTooltipStateInput, showStateTooltip: boolean): string | undefined {
  if (!showStateTooltip) {
    return undefined;
  }

  const label = getSessionStateTooltipLabel(session);
  return label ? `State: ${label}` : undefined;
}

function getSessionStateTooltipLabel(session: SessionTooltipStateInput): string | undefined {
  /*
   * CDXC:SessionTooltips 2026-06-13-23:24:
   * Session hover tooltips need one short lifecycle line that combines zmx
   * provider liveness with the app's loaded surface state. "Active, not loaded"
   * is the user-facing wording for a live provider session whose native pane is
   * not mounted yet.
   */
  const hasStateSignal =
    session.isLive !== undefined ||
    session.isRunning !== undefined ||
    session.isSleeping !== undefined ||
    session.lifecycleState !== undefined ||
    session.nativePaneState !== undefined ||
    session.providerSessionState !== undefined;
  if (!hasStateSignal) {
    return undefined;
  }

  const hasLoadedSurface = session.nativePaneState === 'mounted' || session.nativePaneState === 'mounting';
  if (hasLoadedSurface) {
    return 'Active in app';
  }

  if (session.providerSessionState === 'exists') {
    return 'Active, not loaded';
  }

  if (session.isSleeping === true || session.lifecycleState === 'sleeping') {
    return 'Sleeping';
  }

  if (session.providerSessionState === 'unknown' || session.lifecycleState === 'error') {
    return 'Unknown';
  }

  if (session.isLive === true || session.isRunning === true || session.lifecycleState === 'running') {
    return 'Active in app';
  }

  if (session.providerSessionState === 'missing' && session.sessionPersistenceProvider) {
    return 'Not started';
  }

  if (session.lifecycleState === 'done' || session.isRunning === false || session.isLive === false) {
    return 'Done';
  }

  return undefined;
}

export function getSessionTitleTooltipOptions({
  alwaysShowTitleTooltip,
  headingText,
  titleTooltip,
}: {
  alwaysShowTitleTooltip: boolean;
  headingText: string;
  titleTooltip: string;
}): {
  tooltip?: string;
  tooltipWhen: 'always' | 'overflow';
} {
  const hasTooltipMetadata = titleTooltip !== headingText;
  if (alwaysShowTitleTooltip || hasTooltipMetadata) {
    return {
      tooltip: titleTooltip,
      tooltipWhen: 'always',
    };
  }

  return {
    tooltip: undefined,
    tooltipWhen: 'overflow',
  };
}

type SessionAgentIconProps = {
  agentIcon: SidebarSessionItem['agentIcon'];
  closeAfterDone?: boolean;
  closeAfterDoneDeadlineAt?: string;
  closeAfterDoneRemainingLabel?: string;
  delayedSendDeadlineAt?: string;
  delayedSendRemainingLabel?: string;
  faviconDataUrl?: string;
  isFavorite?: boolean;
  isPinned?: boolean;
  sessionTag?: SidebarSessionTag;
  isGeneratingFirstPromptTitle?: boolean;
  isReloading?: boolean;
  /**
   * CDXC:SessionChatPromptQueue 2026-08-21:
   * How many Ghostex-owned chat prompts are waiting for this session. Rendered
   * as a decoration over the leading icon, never in place of it.
   */
  queuedPromptCount?: number;
  /**
   * CDXC:SessionChatPromptQueue 2026-08-21-b:
   * How many of those rows failed to deliver. Non-zero paints the badge red.
   */
  queuedPromptFailedCount?: number;
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: SidebarSessionItem['sessionPersistenceProvider'];
  showTerminalIcon?: boolean;
};

type SessionAgentLogoStyle = CSSProperties & {
  '--session-agent-logo': string;
  '--session-agent-logo-colored': string;
};

type SessionAgentIconDecorationProps = SessionAgentIconProps & {
  className: string;
  loadingClassName: string;
  tablerClassName: string;
};

function SessionAgentIconDecoration({
  agentIcon,
  className,
  faviconDataUrl,
  isGeneratingFirstPromptTitle = false,
  isReloading = false,
  loadingClassName,
  showTerminalIcon = false,
  tablerClassName,
}: SessionAgentIconDecorationProps) {
  if (isReloading || isGeneratingFirstPromptTitle) {
    return <IconLoader2 aria-hidden='true' className={loadingClassName} size={14} stroke={1.8} />;
  }

  if (agentIcon === 'browser') {
    if (faviconDataUrl) {
      /**
       * CDXC:BrowserPanes 2026-05-03-11:28
       * Browser-pane cards identify the loaded tab with the page favicon when
       * available. Keep a Tabler world glyph as the fallback so cards still
       * have a stable browser affordance before favicon discovery or for pages
       * without icons.
       *
       * CDXC:SidebarBrowserIcon 2026-05-07-19:44
       * Browser affordances in the sidebar use the Tabler world glyph so
       * browser sessions share the same globe cue as browser groups.
       */
      return (
        <img
          alt=''
          aria-hidden='true'
          className={tablerClassName}
          data-agent-icon='browser'
          data-icon-variant='favicon'
          src={faviconDataUrl}
        />
      );
    }
    return (
      <IconWorld aria-hidden='true' className={tablerClassName} data-agent-icon='browser' size={14} stroke={1.8} />
    );
  }

  if (showTerminalIcon && !agentIcon) {
    /**
     * CDXC:SidebarSessions 2026-05-07-14:57
     * Plain terminal sessions still need a visible card identity before an
     * agent is detected. Render the Tabler terminal glyph as a white
     * non-agent icon instead of leaving the Agent Icon slot blank.
     */
    return (
      <IconTerminal2 aria-hidden='true' className={tablerClassName} data-agent-icon='terminal' size={14} stroke={1.8} />
    );
  }

  if (!agentIcon) {
    return null;
  }

  const agentLogoStyle: SessionAgentLogoStyle = {
    /*
     * CDXC:SidebarSessionAgentIcons 2026-06-29-23:58:
     * Session cards need both render assets at the element boundary: masks for
     * monochrome mode and image backgrounds for the colored Settings toggle.
     * Favorite state must not feed this style, so favorite rows keep the same
     * agent logo colors as non-favorite rows.
     */
    '--session-agent-logo': `url("${AGENT_LOGOS[agentIcon]}")`,
    '--session-agent-logo-colored': `url("${COLORED_AGENT_LOGOS[agentIcon]}")`,
  };

  return <span aria-hidden='true' className={className} data-agent-icon={agentIcon} style={agentLogoStyle} />;
}

export function SessionFloatingAgentIcon({
  agentIcon,
  closeAfterDone,
  closeAfterDoneDeadlineAt,
  closeAfterDoneRemainingLabel,
  delayedSendDeadlineAt,
  delayedSendRemainingLabel,
  faviconDataUrl,
  isFavorite = false,
  isPinned = false,
  onCloseAfterDoneClick,
  onDelayedSendClick,
  onPinnedClick,
  queuedPromptCount,
  queuedPromptFailedCount,
  sessionTag,
  sessionPersistenceName,
  sessionPersistenceProvider,
  showTerminalIcon = false,
}: SessionAgentIconProps & {
  onCloseAfterDoneClick?: () => void;
  onDelayedSendClick?: () => void;
  onPinnedClick?: (pinned: boolean) => void;
}) {
  const effectiveSessionTag = getEffectiveSessionTag({ isFavorite, sessionTag });
  /*
  CDXC:SessionChatPromptQueue 2026-08-21:
  The queued-prompt badge is a decoration on the leading icon slot, not a
  competitor for it: whatever owns the slot (agent logo, Delayed Send clock,
  Close After Done clock) keeps rendering underneath. It is a SIBLING of that
  icon, never a wrapper around it, and it carries its own anchor element in the
  timer branches, which do not render the shared one. Wrapping the icon would
  create a second flow box in the row — the exact mistake that once pushed the
  Delayed Send clock above the session card.
  */
  const queuedPromptBadge = (
    <SessionQueuedPromptBadge count={queuedPromptCount} failedCount={queuedPromptFailedCount} />
  );
  const hasActiveDelayedSend = Boolean(delayedSendRemainingLabel || delayedSendDeadlineAt);
  const hasActiveCloseAfterDone = Boolean(closeAfterDone || closeAfterDoneRemainingLabel || closeAfterDoneDeadlineAt);
  const isCloseAfterDoneCountingDown = Boolean(closeAfterDoneRemainingLabel || closeAfterDoneDeadlineAt);

  if (hasActiveDelayedSend) {
    /*
    CDXC:DelayedSend 2026-06-06-05:29:
    An active Delayed Send timer always owns the leading session icon slot, even when the session is tagged, pinned, or has a visible agent icon. The deadline alone is enough to show the yellow clock so a missing countdown label cannot hide the active timer state.
    */
    return (
      <>
        <span aria-hidden='true' className='session-floating-icon-anchor' />
        <DelayedSendSidebarIcon
          className='session-floating-agent-tabler-icon session-delayed-send-agent-icon'
          onClick={onDelayedSendClick}
          remainingLabel={delayedSendRemainingLabel}
        />
        {queuedPromptBadge}
      </>
    );
  }

  if (hasActiveCloseAfterDone) {
    /*
    CDXC:CloseAfterDone 2026-06-15-21:00:
    Close After Done uses Delayed Send's leading clock affordance with a pastel
    red color. Keep it below Delayed Send in precedence so a pending Enter key
    remains the dominant active timer, then fade the red clock only while the
    session is Done and the close countdown is active.
    */
    return (
      <>
        <span aria-hidden='true' className='session-floating-icon-anchor' />
        <CloseAfterDoneSidebarIcon
          className={`session-floating-agent-tabler-icon session-close-after-done-agent-icon${
            isCloseAfterDoneCountingDown ? ' session-close-after-done-agent-icon-countdown' : ''
          }`}
          onClick={onCloseAfterDoneClick}
          remainingLabel={closeAfterDoneRemainingLabel}
        />
        {queuedPromptBadge}
      </>
    );
  }

  return (
    <>
      <span aria-hidden='true' className='session-floating-icon-anchor' />
      {onPinnedClick ? <SessionPinnedFloatingButton isPinned={isPinned} onPinnedClick={onPinnedClick} /> : null}
      {effectiveSessionTag ? <SessionTagSidebarIcon sessionTag={effectiveSessionTag} /> : null}
      <SessionAgentIconDecoration
        agentIcon={agentIcon}
        className='session-floating-agent-icon'
        faviconDataUrl={faviconDataUrl}
        isFavorite={isFavorite}
        loadingClassName='session-floating-reloading-icon'
        showTerminalIcon={showTerminalIcon}
        tablerClassName='session-floating-agent-tabler-icon'
      />
      <SessionPersistenceProviderBadge
        sessionPersistenceName={sessionPersistenceName}
        sessionPersistenceProvider={sessionPersistenceProvider}
        slot='floating'
      />
      {queuedPromptBadge}
    </>
  );
}

/*
CDXC:SessionChatPromptQueue 2026-08-21:
An absolutely positioned, pointer-transparent circle carrying the number of
prompts waiting for this session. It renders nothing at all below one, so a
drained queue leaves no empty dot behind, and counts above 99 collapse to "99+"
rather than widening the badge into the session title.
*/
const SESSION_QUEUED_PROMPT_BADGE_MAX_COUNT = 99;

function SessionQueuedPromptBadge({ count, failedCount }: { count?: number; failedCount?: number }) {
  if (typeof count !== 'number' || !Number.isFinite(count) || count < 1) {
    return null;
  }

  const roundedCount = Math.floor(count);
  /*
  CDXC:SessionChatPromptQueue 2026-08-21-b:
  A row that failed to deliver holds the whole queue until the user retries or
  deletes it, so the badge switches from the yellow "waiting" colour to the
  sidebar's own error red. The colour is the ONLY thing that changes: the box is
  driven entirely by `--session-queued-prompt-badge-*`, so a red badge and a
  yellow badge of the same digit count are the same geometry and cannot reflow
  the icon slot.
  */
  const hasFailed = typeof failedCount === 'number' && Number.isFinite(failedCount) && failedCount > 0;

  return (
    <span
      aria-hidden='true'
      className='session-queued-prompt-badge'
      data-queued-prompt-count={String(roundedCount)}
      data-queued-prompt-failed={hasFailed ? 'true' : undefined}
    >
      {roundedCount > SESSION_QUEUED_PROMPT_BADGE_MAX_COUNT
        ? `${SESSION_QUEUED_PROMPT_BADGE_MAX_COUNT}+`
        : String(roundedCount)}
    </span>
  );
}

function SessionPinnedFloatingButton({
  isPinned,
  onPinnedClick,
}: {
  isPinned: boolean;
  onPinnedClick: (pinned: boolean) => void;
}) {
  const pointerGestureRef = useRef<{ didMove: boolean; pointerId: number; startX: number; startY: number } | undefined>(
    undefined
  );

  return (
    <button
      aria-label={isPinned ? 'Unpin session' : 'Pin session'}
      aria-pressed={isPinned}
      className='session-pinned-floating-button'
      data-pinned={String(isPinned)}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        if (pointerGestureRef.current?.didMove === true) {
          pointerGestureRef.current = undefined;
          return;
        }
        pointerGestureRef.current = undefined;
        onPinnedClick(!isPinned);
      }}
      onPointerCancel={() => {
        pointerGestureRef.current = undefined;
      }}
      onPointerDown={(event) => {
        /*
         * CDXC:PinnedSessions 2026-06-30-11:33:
         * Clicking the pin icon toggles pin state, but pressing and moving it is a pinned-session reorder gesture. Track pointer travel at the button boundary so a completed drag cannot also fire the button click and unpin the session.
         */
        pointerGestureRef.current = {
          didMove: false,
          pointerId: event.pointerId,
          startX: event.clientX,
          startY: event.clientY,
        };
        try {
          event.currentTarget.setPointerCapture(event.pointerId);
        } catch {
          // Pointer capture can fail if the browser has already ended this pointer stream.
        }
      }}
      onPointerMove={(event) => {
        const gesture = pointerGestureRef.current;
        if (!gesture || gesture.pointerId !== event.pointerId || gesture.didMove) {
          return;
        }

        const distanceX = Math.abs(event.clientX - gesture.startX);
        const distanceY = Math.abs(event.clientY - gesture.startY);
        gesture.didMove = distanceX > 3 || distanceY > 3;
      }}
      tabIndex={isPinned ? 0 : -1}
      type='button'
    >
      <IconPin aria-hidden='true' size={15} stroke={1.9} />
    </button>
  );
}

function SessionTagSidebarIcon({ sessionTag }: { sessionTag: SidebarSessionTag }) {
  /**
   * CDXC:SessionTags 2026-06-05-12:30:
   * A tagged session shows its tag glyph in the same leading identity slot used
   * by agent icons. Delayed Send owns higher precedence; otherwise the tag is
   * visible at rest and hover/focus can reveal the hidden agent identity.
   */
  return (
    <SessionTagIcon
      className='session-floating-agent-tabler-icon session-tag-agent-icon'
      fillFavorite
      size={15}
      stroke={1.9}
      tag={sessionTag}
    />
  );
}

function SessionHeaderAgentIcon({
  agentIcon,
  faviconDataUrl,
  isGeneratingFirstPromptTitle = false,
  isReloading = false,
  sessionPersistenceName,
  sessionPersistenceProvider,
  showTerminalIcon = false,
}: SessionAgentIconProps) {
  return (
    <>
      <SessionAgentIconDecoration
        agentIcon={agentIcon}
        className='session-header-agent-icon'
        faviconDataUrl={faviconDataUrl}
        isGeneratingFirstPromptTitle={isGeneratingFirstPromptTitle}
        isReloading={isReloading}
        loadingClassName='session-header-reloading-icon'
        showTerminalIcon={showTerminalIcon}
        tablerClassName='session-header-agent-tabler-icon'
      />
      <SessionPersistenceProviderBadge
        sessionPersistenceName={sessionPersistenceName}
        sessionPersistenceProvider={sessionPersistenceProvider}
        slot='header'
      />
    </>
  );
}

function CloseAfterDoneSidebarIcon({
  className,
  onClick,
  remainingLabel,
}: {
  className: string;
  onClick?: () => void;
  remainingLabel?: string;
}) {
  const tooltip = remainingLabel ? `Close After Done in ${remainingLabel}` : 'Close After Done armed';
  return (
    <AppTooltip content={tooltip}>
      <button
        aria-label={tooltip}
        className={className}
        onClick={(event) => {
          event.stopPropagation();
          onClick?.();
        }}
        type='button'
      >
        <IconClock aria-hidden='true' size={16} stroke={1.9} />
      </button>
    </AppTooltip>
  );
}

function DelayedSendSidebarIcon({
  className,
  onClick,
  remainingLabel,
}: {
  className: string;
  onClick?: () => void;
  remainingLabel?: string;
}) {
  /**
   * CDXC:DelayedSend 2026-05-17-03:14
   * CDXC:DelayedSend 2026-05-21-12:21
   * Active Delayed Send timers replace the sidebar agent icon in the same DOM
   * slot and dimensions as the normal agent glyph, expose the remaining
   * hh:mm:ss/mm:ss countdown on hover, and reopen the modal so users can
   * change or cancel the pending Enter keypress. Render the clock element
   * directly in the leading agent-icon slot; a wrapper would become a separate
   * flow box and can push the clock above the session card.
   */
  const tooltip = getDelayedSendTooltipText(remainingLabel);
  return (
    <AppTooltip content={tooltip}>
      <button
        aria-label={tooltip}
        className={className}
        onClick={(event) => {
          event.stopPropagation();
          onClick?.();
        }}
        type='button'
      >
        <IconClock aria-hidden='true' size={16} stroke={1.9} />
      </button>
    </AppTooltip>
  );
}

function SessionPersistenceProviderBadge({
  sessionPersistenceName,
  sessionPersistenceProvider,
  slot,
}: {
  sessionPersistenceName?: string;
  sessionPersistenceProvider?: SidebarSessionItem['sessionPersistenceProvider'];
  slot: 'floating' | 'header';
}) {
  /**
   * CDXC:SessionPersistence 2026-05-15-15:32:
   * Persistence-backed sessions should keep the agent icon clean; do not render
   * tmux/zmx/zellij provider letters over floating or header icons even when
   * provider metadata is stored for attach commands and tooltips.
   */
  void sessionPersistenceName;
  void sessionPersistenceProvider;
  void slot;
  return null;
}

function getSessionDetailsTooltipLines(
  session: Pick<SidebarSessionItem, 'agentIcon' | 'sessionKind' | 'sessionPersistenceProvider'> & {
    projectName?: string;
    projectPath?: string;
  }
): string[] {
  const agentName = getSessionDetailsAgentName(session);
  const projectLabel = getSessionDetailsProjectLabel(session);
  const providerLabel = session.sessionPersistenceProvider ?? 'none';

  return [`Agent: ${agentName}`, `Project: ${projectLabel}`, `Provider: ${providerLabel}`];
}

function getSessionDetailsAgentName(session: Pick<SidebarSessionItem, 'agentIcon' | 'sessionKind'>): string {
  if (session.agentIcon) {
    return getSidebarAgentNameByIcon(session.agentIcon) ?? session.agentIcon;
  }

  if (session.sessionKind === 'browser') {
    return 'Browser';
  }

  return 'None';
}

function getSessionDetailsProjectLabel({ projectName }: { projectName?: string; projectPath?: string }): string {
  const normalizedProjectName = projectName?.trim();
  return normalizedProjectName || 'None';
}

function getSessionPersistenceTooltipText(
  session: Pick<SidebarSessionItem, 'sessionPersistenceName' | 'sessionPersistenceProvider'>,
  showDebugSessionNumbers: boolean
): string | undefined {
  if (!showDebugSessionNumbers) {
    return undefined;
  }
  if (!session.sessionPersistenceName || !session.sessionPersistenceProvider) {
    return undefined;
  }
  return `${session.sessionPersistenceProvider} session: ${session.sessionPersistenceName}`;
}

export function shouldShowTerminalSessionIcon(session: Pick<SidebarSessionItem, 'agentIcon' | 'sessionKind'>): boolean {
  return !session.agentIcon && (session.sessionKind === undefined || session.sessionKind === 'terminal');
}

function stripAgentTooltipText(
  value: string | undefined,
  agentIcon: SidebarSessionItem['agentIcon']
): string | undefined {
  const normalizedValue = value?.trim();
  if (!normalizedValue) {
    return undefined;
  }

  if (!agentIcon) {
    return normalizedValue;
  }

  const normalizedAgentLabels = Array.from(
    new Set([getSidebarAgentNameByIcon(agentIcon), ...AGENT_SECONDARY_LABELS[agentIcon]])
  )
    .filter((label): label is string => typeof label === 'string')
    .map((label) => label.trim())
    .filter((label) => label.length > 0)
    .sort((left, right) => right.length - left.length);
  const lowerValue = normalizedValue.toLowerCase();

  for (const label of normalizedAgentLabels) {
    const lowerLabel = label.toLowerCase();
    if (lowerValue === lowerLabel) {
      return undefined;
    }

    if (!lowerValue.startsWith(lowerLabel)) {
      continue;
    }

    const remainder = normalizedValue.slice(label.length).trimStart();
    if (!remainder) {
      return undefined;
    }

    const separatorMatch = remainder.match(/^([:/|-]+)\s*(.*)$/);
    if (separatorMatch) {
      const strippedValue = separatorMatch[2]?.trim();
      return strippedValue || undefined;
    }

    return normalizedValue;
  }

  return normalizedValue;
}

type OverflowTooltipTextProps = {
  children: ReactElement<{
    onBlur?: FocusEventHandler<HTMLElement>;
    onFocus?: FocusEventHandler<HTMLElement>;
    onMouseEnter?: MouseEventHandler<HTMLElement>;
    onMouseLeave?: MouseEventHandler<HTMLElement>;
  }>;
  delayMs?: number;
  textRef?: RefObject<HTMLDivElement | null>;
  text: string;
  tooltip?: string;
  tooltipWhen?: 'always' | 'overflow';
};

type SessionTooltipPosition = {
  left: number;
  top: number;
  width: number;
};

export function OverflowTooltipText({
  children,
  delayMs = TOOLTIP_DELAY_MS,
  text,
  textRef,
  tooltip,
  tooltipWhen = 'overflow',
}: OverflowTooltipTextProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [tooltipPosition, setTooltipPosition] = useState<SessionTooltipPosition>();
  const openTimeoutIdRef = useRef<number | undefined>(undefined);
  const shellRef = useRef<HTMLDivElement>(null);
  const tooltipPopupRef = useRef<HTMLDivElement>(null);
  const tooltipIdRef = useRef(Symbol('overflowTooltip'));
  const tooltipContent = tooltip ?? text;

  const clearOpenTimeout = () => {
    if (openTimeoutIdRef.current === undefined) {
      return;
    }

    window.clearTimeout(openTimeoutIdRef.current);
    openTimeoutIdRef.current = undefined;
  };

  const closeTooltip = () => {
    clearOpenTimeout();
    if (activeOverflowTooltipId === tooltipIdRef.current) {
      activeOverflowTooltipId = undefined;
      activeOverflowTooltipClose = undefined;
    }
    setIsOpen(false);
    setTooltipPosition(undefined);
  };

  const hasOverflow = () => {
    const element = textRef?.current;
    if (!element) {
      return false;
    }

    if (element.scrollWidth > element.clientWidth) {
      return true;
    }

    return element.scrollHeight > element.clientHeight;
  };

  const openTooltip = () => {
    clearOpenTimeout();
    if (areSidebarTooltipsSuppressed()) {
      setIsOpen(false);
      return;
    }
    const shouldOpen = tooltipWhen === 'always' ? Boolean(tooltip ?? text) : hasOverflow();
    if (!shouldOpen) {
      setIsOpen(false);
      return;
    }

    openTimeoutIdRef.current = window.setTimeout(() => {
      if (areSidebarTooltipsSuppressed()) {
        closeTooltip();
        return;
      }
      if (activeOverflowTooltipId !== tooltipIdRef.current) {
        activeOverflowTooltipClose?.();
      }

      activeOverflowTooltipId = tooltipIdRef.current;
      activeOverflowTooltipClose = closeTooltip;
      setIsOpen(true);
      openTimeoutIdRef.current = undefined;
    }, delayMs);
  };

  useEffect(() => {
    const handleSidebarTooltipDismiss = () => closeTooltip();
    const handleSidebarTooltipSuppressionChanged = () => {
      if (areSidebarTooltipsSuppressed()) {
        closeTooltip();
      }
    };
    window.addEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleSidebarTooltipDismiss);
    window.addEventListener(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT, handleSidebarTooltipSuppressionChanged);
    return () => {
      window.removeEventListener(SIDEBAR_TOOLTIP_DISMISS_EVENT, handleSidebarTooltipDismiss);
      window.removeEventListener(SIDEBAR_TOOLTIP_SUPPRESSION_CHANGED_EVENT, handleSidebarTooltipSuppressionChanged);
      clearOpenTimeout();
      if (activeOverflowTooltipId === tooltipIdRef.current) {
        activeOverflowTooltipId = undefined;
        activeOverflowTooltipClose = undefined;
      }
    };
  }, []);

  useLayoutEffect(() => {
    if (!isOpen) {
      return undefined;
    }

    const getTriggerElement = () => textRef?.current ?? shellRef.current;

    const updateTooltipPosition = () => {
      const triggerElement = getTriggerElement();
      const tooltipElement = tooltipPopupRef.current;
      if (!triggerElement || !tooltipElement) {
        return;
      }

      const ownerElement = shellRef.current?.firstElementChild ?? shellRef.current ?? triggerElement;
      const triggerBounds = ownerElement.getBoundingClientRect();
      const tooltipBounds = tooltipElement.getBoundingClientRect();
      const left = triggerBounds.left;
      const width = triggerBounds.width;
      const belowTop = triggerBounds.bottom + SESSION_TOOLTIP_TRIGGER_OFFSET_PX;
      const preferredTop = belowTop;
      const top = Math.max(
        SESSION_TOOLTIP_VIEWPORT_MARGIN_PX,
        Math.min(preferredTop, window.innerHeight - SESSION_TOOLTIP_VIEWPORT_MARGIN_PX - tooltipBounds.height)
      );

      setTooltipPosition((previousPosition) => {
        if (previousPosition?.left === left && previousPosition.top === top && previousPosition.width === width) {
          return previousPosition;
        }

        return { left, top, width };
      });
    };

    updateTooltipPosition();
    window.addEventListener('resize', updateTooltipPosition);
    window.addEventListener('scroll', updateTooltipPosition, true);

    const resizeObserver =
      typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateTooltipPosition);
    const triggerElement = getTriggerElement();
    if (triggerElement) {
      resizeObserver?.observe(triggerElement);
    }
    if (tooltipPopupRef.current) {
      resizeObserver?.observe(tooltipPopupRef.current);
    }

    return () => {
      window.removeEventListener('resize', updateTooltipPosition);
      window.removeEventListener('scroll', updateTooltipPosition, true);
      resizeObserver?.disconnect();
    };
  }, [isOpen, textRef, tooltipContent]);

  const trigger = cloneElement(children, {
    onBlur: chainEventHandlers(children.props.onBlur, closeTooltip),
    onFocus: chainEventHandlers(children.props.onFocus, openTooltip),
    onMouseEnter: chainEventHandlers(children.props.onMouseEnter, openTooltip),
    onMouseLeave: chainEventHandlers(children.props.onMouseLeave, closeTooltip),
  });

  /*
   * CDXC:SessionTooltips 2026-05-20-11:05:
   * Session-card title tooltips must render below the row without overlapping
   * the trigger. Portaled Radix tooltips mis-anchor in the native sidebar
   * webview, so keep the label local to the card with a below-positioned popup.
   *
   * CDXC:SidebarTooltips 2026-05-25-07:16:
   * Local session-card tooltips must also close on the shared sidebar dismiss
   * event because app switching and fast exits can skip the trigger mouseleave
   * event that normally clears this local open state.
   *
   * CDXC:SessionTooltips 2026-05-26-22:29:
   * Session title tooltips should keep metadata and provider/session id rows at
   * their existing base weight while making only the first title row slightly
   * bolder, so the title scans as the primary label without making ids heavier.
   *
   * CDXC:SessionTooltips 2026-05-28-04:33:
   * Quick-session hover tooltips must paint above surrounding Projects content.
   * Keep the custom native-sidebar positioning behavior, but portal the
   * rendered tooltip to the document body and place it from the trigger rect so
   * section overflow and row stacking contexts cannot cover it.
   *
   * CDXC:SidebarTooltips 2026-05-30-06:36:
   * Sidebar tooltips should open below their trigger for a consistent scan path
   * across action buttons and session rows. Preserve viewport clamping, but do
   * not choose an above-trigger position just because the lower half is tighter.
   *
   * CDXC:SidebarTooltipAlignment 2026-07-24:
   * Session tooltips belong to the rendered session surface, not its inset
   * wrapper or title text. Align the popup's left edge with the direct child
   * trigger while retaining viewport clamping for unusually narrow windows.
   *
   * CDXC:SidebarTooltipSizing 2026-07-26:
   * The session tooltip border box must match the session button's measured
   * width as well as its left edge. Use the raw trigger rectangle for both
   * values so nested project sessions and edge-clipped rows are clipped by the
   * viewport identically.
   */
  return (
    <div className='session-local-tooltip-shell' ref={shellRef}>
      {trigger}
      {isOpen && tooltipContent
        ? createPortal(
            <div
              className={cn('session-local-tooltip-popup', TOOLTIP_MOTION_CLASS_NAME)}
              data-side='bottom'
              data-state='delayed-open'
              ref={tooltipPopupRef}
              role='tooltip'
              style={
                {
                  '--session-local-tooltip-left': tooltipPosition ? `${tooltipPosition.left}px` : '0px',
                  '--session-local-tooltip-top': tooltipPosition ? `${tooltipPosition.top}px` : '0px',
                  '--session-local-tooltip-width': tooltipPosition ? `${tooltipPosition.width}px` : 'max-content',
                } as CSSProperties
              }
            >
              {renderSessionLocalTooltipContent(tooltipContent)}
            </div>,
            document.body
          )
        : null}
    </div>
  );
}

function renderSessionLocalTooltipContent(content: string): ReactNode {
  const lines = content
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean);

  if (lines.length === 0) {
    return content;
  }

  return lines.map((line, index) => (
    <span
      className={index === 0 ? 'session-local-tooltip-title' : 'session-local-tooltip-meta'}
      key={`${index}-${line}`}
    >
      {line}
    </span>
  ));
}

function chainEventHandlers<Event>(
  originalHandler: ((event: Event) => void) | undefined,
  nextHandler: (event: Event) => void
): (event: Event) => void {
  return (event) => {
    originalHandler?.(event);
    nextHandler(event);
  };
}
