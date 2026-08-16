// Floating top-right [surface switch][Agent Actions] cluster, shared by the
// chat view (primary button = "Terminal View") and hosts that overlay it on a
// terminal surface (primary button = "Chat View"). Extracted from
// session-chat-view.tsx so the web terminal layer can mount the exact same
// strip the chat view shows.
//
// The cluster clones the gpui terminal overlay's action strip
// (gpui/src/terminal_element.rs terminal_overlay_button): flush 28.125px
// square buttons sharing 1px #2a2a2a divider borders, #101010 background,
// #343434 hover, 14px #a6a6a6 icons, 0 10px 22px rgba(0,0,0,0.32) shadow,
// inset 13px from the pane's top-right corner. Icons are the same Tabler
// glyphs the gpui assets use; Sleep's moon is a custom filled asset copied
// verbatim from gpui/assets/titlebar/moon.svg.

import {
  IconClockCheck,
  IconDots,
  IconGitBranch,
  IconMessage,
  IconMessageCode,
  IconPaperclip,
  IconPencil,
  IconRefresh,
  IconStack,
  IconStackPush,
  IconTerminal2,
  type Icon as TablerIcon,
} from "@tabler/icons-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { cn } from "../../lib/utils";
import { AppTooltip } from "../app-tooltip";

export interface SessionChatHostAction {
  /** Host-defined action id, passed back verbatim to onAction. */
  id: string;
  label: string;
  /** Formatted effective shortcut shown beside the label in the tooltip. */
  shortcut?: string;
  /**
   * When set, clicking swaps the cluster to an inline text field (e.g.
   * Rename); onAction receives the submitted value as its second argument.
   */
  input?: { initialValue?: string; placeholder?: string };
}

/**
 * Host-injected top-right cluster shown over the chat: a "Terminal View"
 * switch-back button plus an optional "Agent Actions" button that expands
 * into the host's per-session action row. Hosts whose own chrome already
 * offers these (e.g. the mobile app's native header) simply omit the prop.
 */
export interface SessionChatHostActions {
  onSwitchToTerminal: () => void;
  /** Formatted shortcut for switching between Terminal View and Chat View. */
  switchViewShortcut?: string;
  /** Optional plain switch reserved for opening an agent-owned model picker. */
  onSwitchToTerminalForAgentPicker?: () => void;
  /** Expanded Agent Actions row; omit to hide the Agent Actions button. */
  actions?: readonly SessionChatHostAction[];
  onAction?: (id: string, value?: string) => void;
  /** Direct Agent Actions handler (e.g. opens a host menu) instead of expanding a row. */
  onAgentActions?: () => void;
}

const HOST_ACTION_ICONS: Record<string, TablerIcon> = {
  attachPath: IconPaperclip,
  delayedActions: IconClockCheck,
  fork: IconGitBranch,
  fullReload: IconRefresh,
  promptEditor: IconMessageCode,
  rename: IconPencil,
  stashPrompt: IconStackPush,
  stashedPrompts: IconStack,
};

function SleepMoonIcon() {
  return (
    <svg aria-hidden="true" className="size-3.5" fill="currentColor" viewBox="0 0 32 32">
      <path
        d="M30.4422 21.7576L30.4116 21.7051C30.2498 21.4554 29.954 21.3157 29.6478 21.3697C29.5454 21.3877 29.4525 21.4254 29.3705 21.4785L29.375 21.4756C28.2165 22.2303 26.8137 22.7975 25.2833 23.0673C19.1647 24.1462 13.3295 20.0604 12.2506 13.9418C11.4414 9.3526 13.5372 4.9234 17.2172 2.5401L17.2852 2.4997L17.4776 2.3754C17.72 2.2129 17.8546 1.9221 17.8014 1.6207C17.7363 1.2514 17.4105 0.9931 17.0476 1.0022L17.0435 1.0019C16.3825 1.0139 15.6299 1.0877 14.8745 1.2209C6.8533 2.6353 1.4972 10.2846 2.9116 18.3058C4.3259 26.3271 11.9752 31.6832 19.9965 30.2688C24.6723 29.4443 28.4435 26.5007 30.4942 22.5994L30.5129 22.5615C30.5867 22.4216 30.616 22.254 30.586 22.0836C30.5639 21.9585 30.5128 21.8467 30.4404 21.7529L30.443 21.7564Z"
        transform="rotate(-10 16 16)"
      />
    </svg>
  );
}

function hostActionIcon(id: string): ReactNode {
  if (id === "sleep") {
    return <SleepMoonIcon />;
  }
  const Icon = HOST_ACTION_ICONS[id];
  return Icon ? <Icon aria-hidden="true" size={14} stroke={2} /> : null;
}

function HostActionButton({
  children,
  label,
  shortcut,
  last = false,
  onClick,
}: {
  children: ReactNode;
  label: string;
  shortcut?: string;
  last?: boolean;
  onClick: () => void;
}) {
  // data-slot opts out of the unlayered legacy `button:where(:not([data-slot]))`
  // chrome in theme.css, which sets `background: transparent` and would beat the
  // layered Tailwind `bg-[#101010]` / `hover:bg-[#343434]` utilities.
  return (
    <AppTooltip content={shortcut ? `${label} (${shortcut})` : label}>
      <button
        aria-label={label}
        className={cn(
          "flex h-[28.125px] min-w-[28.125px] shrink-0 items-center justify-center border-y border-l border-[#2a2a2a] bg-[#101010] text-[#a6a6a6] transition-colors hover:bg-[#343434]",
          last && "border-r",
        )}
        data-slot="session-chat-host-action"
        onClick={onClick}
        type="button"
      >
        {children}
      </button>
    </AppTooltip>
  );
}

/**
 * `surface` picks the primary button: on the chat surface it reads
 * "Terminal View"; overlaid on a terminal it reads "Chat View". Both call
 * hostActions.onSwitchToTerminal — the host wires it to whichever switch
 * applies to its surface.
 */
export function SessionChatHostActionsCluster({
  hostActions,
  surface = "chat",
}: {
  hostActions: SessionChatHostActions;
  surface?: "chat" | "terminal";
}) {
  const [expanded, setExpanded] = useState(false);
  const [inputAction, setInputAction] = useState<SessionChatHostAction | null>(null);
  const [inputValue, setInputValue] = useState("");
  const promptsAction = hostActions.actions?.find((action) => action.id === "stashedPrompts");
  const menuActions = hostActions.actions?.filter((action) => action.id !== "stashedPrompts") ?? [];
  const hasAgentActions = hostActions.onAgentActions !== undefined || menuActions.length > 0;

  return (
    <div className="pointer-events-none absolute right-[13px] top-[13px] z-20 flex flex-col items-end">
      <div className="pointer-events-auto flex items-center shadow-[0_10px_22px_rgba(0,0,0,0.32)]">
        {inputAction ? (
          <input
            autoFocus
            className="h-[28.125px] w-64 border border-[#2a2a2a] bg-[#101010] px-2 text-xs text-foreground outline-none focus:border-ring"
            onBlur={() => setInputAction(null)}
            onChange={(event) => setInputValue(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                hostActions.onAction?.(inputAction.id, inputValue);
                setInputAction(null);
              } else if (event.key === "Escape") {
                event.preventDefault();
                setInputAction(null);
              }
            }}
            placeholder={inputAction.input?.placeholder ?? inputAction.label}
            value={inputValue}
          />
        ) : (
          <>
            <HostActionButton
              label={surface === "chat" ? "Terminal View" : "Chat View"}
              shortcut={hostActions.switchViewShortcut}
              last={promptsAction === undefined && !hasAgentActions}
              onClick={hostActions.onSwitchToTerminal}
            >
              {surface === "chat" ? (
                <IconTerminal2 aria-hidden="true" size={14} stroke={2} />
              ) : (
                <IconMessage aria-hidden="true" size={14} stroke={2} />
              )}
            </HostActionButton>
            {promptsAction ? (
              <HostActionButton
                label={promptsAction.label}
                shortcut={promptsAction.shortcut}
                last={!hasAgentActions}
                onClick={() => hostActions.onAction?.(promptsAction.id)}
              >
                {hostActionIcon(promptsAction.id)}
              </HostActionButton>
            ) : null}
            {hasAgentActions ? (
              <HostActionButton
                label="Agent Actions"
                last
                onClick={hostActions.onAgentActions ?? (() => setExpanded((value) => !value))}
              >
                <IconDots aria-hidden="true" size={14} stroke={2} />
              </HostActionButton>
            ) : null}
          </>
        )}
      </div>
      {/* The Agent Actions menu opens as its own bar 13px below the cluster. */}
      {expanded && !inputAction && menuActions.length ? (
        <div className="pointer-events-auto mt-[13px] flex items-center shadow-[0_10px_22px_rgba(0,0,0,0.32)]">
          {menuActions.map((action, index) => (
            <HostActionButton
              key={action.id}
              label={action.label}
              shortcut={action.shortcut}
              last={index === menuActions.length - 1}
              onClick={() => {
                if (action.input) {
                  setInputValue(action.input.initialValue ?? "");
                  setInputAction(action);
                } else {
                  hostActions.onAction?.(action.id);
                }
              }}
            >
              {hostActionIcon(action.id) ?? (
                <span className="whitespace-nowrap px-2 text-[11px]">{action.label}</span>
              )}
            </HostActionButton>
          ))}
        </div>
      ) : null}
    </div>
  );
}
