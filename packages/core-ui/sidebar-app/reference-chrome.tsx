import {
  IconArrowLeft,
  IconArrowRight,
  IconArrowsDiagonal2,
  IconArrowsDiagonalMinimize,
  IconBolt,
  IconCaretRightFilled,
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconClock,
  IconCloud,
  IconCoffee,
  IconDeviceMobile,
  IconEdit,
  IconFileSearch,
  IconFilter2,
  IconFolders,
  IconHistoryToggle,
  IconKeyboard,
  IconLayoutSidebar,
  IconLoader2,
  IconMenu2,
  IconMoon,
  IconPlugConnected,
  IconPlus,
  IconRobotFace,
  IconSearch,
  IconSettings,
  IconSquareMinus,
  IconTerminal2,
  IconUsersGroup,
  IconWorld,
  type TablerIcon,
} from "@tabler/icons-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { Button } from "@/packages/components/ui/button";
import type { SidebarActiveSessionsSortMode } from "../../shared/session-grid-contract";
import {
  KEEP_AWAKE_DURATION_OPTIONS,
  type KeepAwakeDurationMinutes,
  type SidebarV2Layout,
  type SidebarVersion,
  type ghostexSettings,
} from "../../shared/ghostex-settings";
import {
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from "../../shared/ghostex-hotkeys";
import type { SidebarAgentButton } from "../../shared/sidebar-agents";
import { PET_CONTROLS_VISIBLE } from "../../shared/pets";
import {
  getSidebarSessionTagListItemFilter,
  normalizeSidebarSessionTagListItems,
  type SidebarSessionTagListItem,
} from "../../shared/session-tags";
import { dismissSidebarTooltips } from "../app-tooltip";
import { AgentMenuChatIndicator } from "../agent-menu-chat-indicator";
import { formatSidebarHotkeyLabel } from "../hotkey-label";
import { ProjectAgentLauncherIcon } from "../project-agent-launcher-icon";
import {
  registerSidebarContextMenuDismissHandler,
  SidebarContextMenuPortal,
} from "../sidebar-context-menu-portal";
import { SidebarFixedTooltipButton } from "../sidebar-fixed-tooltip-button";
import {
  getSidebarSessionTagLabel,
  SessionTagIcon,
  type SidebarSessionTagFilter,
} from "../session-tag-ui";
import type {
  HeaderSortMenuPosition,
  RemoteMachineHeaderConnectionControl,
  ReferenceSidebarSectionId,
  SidebarKeepAwakeRuntimeState,
  SidebarSectionSessionSummary,
} from "./types";

export const REFERENCE_SECTION_AGENT_MENU_WIDTH_PX = 220;
export type SidebarReferencePrimaryMenuKind = "keepAwake" | "settings";

export function SidebarReferenceTopChrome({
  keepAwakeRuntime,
  onOpenAgentsHub,
  onOpenAutomations,
  onOpenDiscord,
  onOpenHotkeys,
  onOpenMobile,
  onOpenPowerSettings,
  onOpenPreviousSessions,
  onRunKeepAwake,
  onSearchPreviousSessionsByPrompt,
  onSearch,
  onStopKeepAwake,
  onTogglePetOverlay,
  settings,
  showKeepAwakeButton,
}: {
  keepAwakeRuntime?: SidebarKeepAwakeRuntimeState;
  onOpenAgentsHub: () => void;
  onOpenAutomations: () => void;
  onOpenDiscord: () => void;
  onOpenHotkeys: () => void;
  onOpenMobile: () => void;
  onOpenPowerSettings: () => void;
  onOpenPreviousSessions: () => void;
  onRunKeepAwake: (durationMinutes: KeepAwakeDurationMinutes) => void;
  onSearchPreviousSessionsByPrompt: () => void;
  onSearch: () => void;
  onStopKeepAwake: () => void;
  onTogglePetOverlay: () => void;
  settings: ghostexSettings;
  showKeepAwakeButton: boolean;
}) {
  const topControlRowRef = useRef<HTMLDivElement>(null);
  const [ openMenu, setOpenMenu ] = useState<SidebarReferencePrimaryMenuKind>();
  const settingsMenuHotkeys = normalizeghostexHotkeySettings(settings.hotkeys);

  useEffect(() => {
    if (!openMenu) {
      return undefined;
    }

    const handleOutsidePointerDown = (event: PointerEvent) => {
      if (isNode(event.target) && topControlRowRef.current?.contains(event.target)) {
        return;
      }
      setOpenMenu(undefined);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpenMenu(undefined);
      }
    };
    const handleWindowBlur = () => {
      setOpenMenu(undefined);
    };
    const unregisterNativeDismiss = registerSidebarContextMenuDismissHandler(() => {
      setOpenMenu(undefined);
    });

    document.addEventListener("pointerdown", handleOutsidePointerDown, true);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("blur", handleWindowBlur);
    return () => {
      unregisterNativeDismiss();
      document.removeEventListener("pointerdown", handleOutsidePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("blur", handleWindowBlur);
    };
  }, [openMenu]);

  const toggleMoreMenu = () => {
    dismissSidebarTooltips();
    setOpenMenu((current) => (current ? undefined : "settings"));
  };

  const closeMenuAndRun = (action: () => void) => {
    dismissSidebarTooltips();
    setOpenMenu(undefined);
    action();
  };

  /**
   * CDXC:SidebarReference 2026-05-08-09:11
   * Combined mode should visually match the provided app sidebar: native-style
   * window dots, disabled back/forward chrome, and primary sidebar navigation.
   *
   * CDXC:TitlebarActions 2026-05-11-02:46
   * Actions moved out of the sidebar header into the native titlebar beside
   * Open In. Keep this top chrome focused on navigation/search so the action
   * menu has one home and one split-button UX.
   *
   * CDXC:AgentsHub 2026-05-12-09:59
   * Agents Hub should remain the first primary sidebar destination so agent
   * configuration content is reached before secondary reference surfaces.
   *
   * CDXC:Mobile 2026-06-16-00:45:
   * The primary sidebar needs a Mobile entry near other reference/setup
   * navigation. It should launch through the same fixed browser-chat path as
   * Plugins so mobile setup docs open outside the active code project.
   *
   * CDXC:Mobile 2026-06-16-01:23:
   * Mobile should open the Ghostex download page, not the GitHub README anchor,
   * because the product site now owns mobile download routing.
   *
   * CDXC:Automations 2026-06-29-15:55:
   * Automations should sit above Mobile in the primary sidebar and open the
   * gxserver-backed Automation page instead of the old coming-soon toast.
   *
   * CDXC:Automations 2026-06-30-11:05:
   * Sidebar Automations opens the Quick-level all-project page. Project-specific automation access moved to the titlebar Automate view so the sidebar shortcut does not hijack the active project's Kanban/Project surface.
   *
   * CDXC:Automations 2026-06-30-12:51:
   * The sidebar shortcut tooltip should use the full page name, Automations Overview, so users can distinguish it from the per-project Automate titlebar view.
   *
   * CDXC:SidebarReference 2026-06-16-01:23:
   * Plugins should no longer consume a primary sidebar row.
   *
   * CDXC:Plugins 2026-06-16-01:29:
   * Hide the Plugins sidebar affordance for now instead of keeping it as an
   * Agents Hub secondary action.
   *
   * CDXC:ExperimentalFeatures 2026-06-28-07:41:
   * Agents Hub is no longer gated by Enable Experimental Features. Keep it
   * visible as the first primary sidebar destination even when experimental
   * features are disabled.
   *
   * CDXC:SidebarReference 2026-06-28-15:04:
   * Agents Hub, Automations, and Mobile should be icon-only shortcuts sharing one full-width row at the top of the sidebar, with hover tooltips providing their labels. Search remains a separate full-width row below them.
   *
   * CDXC:SidebarTopChrome 2026-06-29-01:43:
   * Settings and Keep Awake moved out of the macOS titlebar into the same full-width sidebar shortcut row. They remain icon-only with hover tooltips, and normal clicks open local sidebar dropdowns instead of native titlebar child-window menus.
   *
   * CDXC:SidebarTopChrome 2026-06-29-03:39:
   * The overflow menu trigger should present itself as "More" in the sidebar
   * tooltip while the dropdown still contains the Settings destination.
   *
   * CDXC:SidebarTopChrome 2026-07-04-17:26:
   * The visible top chrome is now Search plus More. Agents Hub, Automations,
   * Mobile, Keep Awake, Search by Prompt, and Previous Sessions all live under
   * More so the sidebar only spends one row on primary navigation.
   *
   * CDXC:SidebarFooter 2026-08-07:
   * Settings now has one icon-only home in the fixed sidebar footer. Keep it
   * out of More so the same destination is not repeated in two places.
   */
  return (
    <header className="reference-sidebar-top">
      <div aria-hidden="true" className="reference-sidebar-window-row">
        <span className="reference-sidebar-window-dot" data-window-dot="close" />
        <span className="reference-sidebar-window-dot" data-window-dot="minimize" />
        <span className="reference-sidebar-window-dot" data-window-dot="zoom" />
        <IconLayoutSidebar className="reference-sidebar-window-icon" size={16} stroke={1.9} />
        <IconArrowLeft className="reference-sidebar-window-icon" size={17} stroke={1.9} />
        <IconArrowRight className="reference-sidebar-window-icon" size={17} stroke={1.9} />
      </div>
      <nav aria-label="Sidebar primary navigation" className="reference-sidebar-primary-nav">
        <div
          aria-label="Sidebar search and menu"
          className="reference-sidebar-search-more-row"
          ref={topControlRowRef}
          role="group"
        >
          <SidebarReferenceSearchNavItem
            onSearch={onSearch}
            shortcut={formatSidebarMenuHotkeyLabel(settingsMenuHotkeys.openSessionSearchPalette)}
          />
          <div className="reference-sidebar-primary-menu-cell">
            <SidebarReferenceShortcutButton
              ariaExpanded={Boolean(openMenu)}
              ariaHaspopup="menu"
              icon={IconMenu2}
              label="More"
              menuOpen={Boolean(openMenu)}
              onClick={toggleMoreMenu}
            />
            {openMenu === "settings" ? (
              <SidebarReferenceSettingsDropdown
                keepAwakeRuntime={keepAwakeRuntime}
                hotkeys={settingsMenuHotkeys}
                onOpenAgentsHub={() => closeMenuAndRun(onOpenAgentsHub)}
                onOpenAutomations={() => closeMenuAndRun(onOpenAutomations)}
                onOpenDiscord={() => closeMenuAndRun(onOpenDiscord)}
                onOpenHotkeys={() => closeMenuAndRun(onOpenHotkeys)}
                onOpenKeepAwakeMenu={() => {
                  dismissSidebarTooltips();
                  setOpenMenu("keepAwake");
                }}
                onOpenMobile={() => closeMenuAndRun(onOpenMobile)}
                onOpenPreviousSessions={() => closeMenuAndRun(onOpenPreviousSessions)}
                onSearchPreviousSessionsByPrompt={() =>
                  closeMenuAndRun(onSearchPreviousSessionsByPrompt)
                }
                onTogglePetOverlay={() => closeMenuAndRun(onTogglePetOverlay)}
                showKeepAwakeButton={showKeepAwakeButton}
              />
            ) : null}
            {openMenu === "keepAwake" ? (
              <SidebarReferenceKeepAwakeDropdown
                activeDuration={keepAwakeRuntime?.durationMinutes}
                isRunning={Boolean(keepAwakeRuntime)}
                onBack={() => {
                  dismissSidebarTooltips();
                  setOpenMenu("settings");
                }}
                onOpenPowerSettings={() => closeMenuAndRun(onOpenPowerSettings)}
                onStartKeepAwake={(durationMinutes) =>
                  closeMenuAndRun(() => onRunKeepAwake(durationMinutes))
                }
                onStopKeepAwake={() => closeMenuAndRun(onStopKeepAwake)}
              />
            ) : null}
          </div>
        </div>
      </nav>
    </header>
  );
}

export function SidebarReferenceSearchNavItem({
  onSearch,
  shortcut,
}: {
  onSearch: () => void;
  shortcut?: string;
}) {
  return (
    <div className="reference-sidebar-search-slot" data-active="false">
      <div className="reference-sidebar-nav-item">
        <Button
          className="reference-sidebar-nav-button"
          onClick={onSearch}
          size="sm"
          type="button"
          variant="ghost"
        >
          <IconSearch
            aria-hidden="true"
            className="reference-sidebar-nav-icon reference-sidebar-search-icon"
            size={15}
            stroke={1.8}
          />
          <span className="reference-sidebar-nav-label">Search</span>
          {shortcut ? <kbd className="reference-sidebar-nav-shortcut">{shortcut}</kbd> : null}
        </Button>
      </div>
    </div>
  );
}

export function SidebarReferenceNavButton({
  icon: Icon,
  iconOnly = false,
  label,
  onClick,
}: {
  icon: TablerIcon;
  iconOnly?: boolean;
  label: string;
  onClick: () => void;
}) {
  const className = iconOnly
    ? "reference-sidebar-nav-button reference-sidebar-nav-icon-button reference-sidebar-hover-action-tooltip"
    : "reference-sidebar-nav-button";

  if (iconOnly) {
    return (
      <SidebarFixedTooltipButton
        aria-label={label}
        className={className}
        onClick={onClick}
        tooltip={label}
        type="button"
      >
        <Icon
          aria-hidden="true"
          className="reference-sidebar-nav-icon"
          data-icon="inline-start"
          size={15}
          stroke={1.9}
        />
      </SidebarFixedTooltipButton>
    );
  }

  return (
    <Button
      className={className}
      onClick={onClick}
      size="sm"
      type="button"
      variant="ghost"
    >
      <Icon
        aria-hidden="true"
        className="reference-sidebar-nav-icon"
        data-icon="inline-start"
        size={15}
        stroke={1.9}
      />
      <span className="reference-sidebar-nav-label">{label}</span>
    </Button>
  );
}

export function SidebarReferenceShortcutButton({
  active = false,
  ariaExpanded,
  ariaHaspopup,
  icon: Icon,
  label,
  menuOpen = false,
  onClick,
  stableBackground = false,
}: {
  active?: boolean;
  ariaExpanded?: boolean;
  ariaHaspopup?: "menu";
  icon: TablerIcon;
  label: string;
  menuOpen?: boolean;
  onClick: () => void;
  stableBackground?: boolean;
}) {
  return (
    <SidebarFixedTooltipButton
      aria-expanded={ariaExpanded}
      aria-haspopup={ariaHaspopup}
      aria-label={label}
      className="reference-sidebar-nav-button reference-sidebar-nav-icon-button reference-sidebar-hover-action-tooltip"
      data-active={String(active)}
      data-state={menuOpen ? "open" : undefined}
      data-stable-background={stableBackground ? "true" : undefined}
      onClick={onClick}
      tooltip={label}
      type="button"
    >
      <Icon
        aria-hidden="true"
        className="reference-sidebar-nav-icon"
        data-icon="inline-start"
        size={15}
        stroke={1.9}
      />
    </SidebarFixedTooltipButton>
  );
}

export function SidebarReferenceSettingsDropdown({
  keepAwakeRuntime,
  hotkeys,
  onOpenAgentsHub,
  onOpenAutomations,
  onOpenDiscord,
  onOpenHotkeys,
  onOpenMobile,
  onOpenKeepAwakeMenu,
  onOpenPreviousSessions,
  onSearchPreviousSessionsByPrompt,
  onTogglePetOverlay,
  showKeepAwakeButton,
}: {
  keepAwakeRuntime?: SidebarKeepAwakeRuntimeState;
  hotkeys: ghostexHotkeySettings;
  onOpenAgentsHub: () => void;
  onOpenAutomations: () => void;
  onOpenDiscord: () => void;
  onOpenHotkeys: () => void;
  onOpenMobile: () => void;
  onOpenKeepAwakeMenu: () => void;
  onOpenPreviousSessions: () => void;
  onSearchPreviousSessionsByPrompt: () => void;
  onTogglePetOverlay: () => void;
  showKeepAwakeButton: boolean;
}) {
  return (
    <div className="reference-sidebar-primary-dropdown" role="menu">
      <SidebarReferencePrimaryMenuItem
        icon={IconHistoryToggle}
        label="Sessions"
        onSelect={onOpenPreviousSessions}
        shortcut={formatSidebarMenuHotkeyLabel(hotkeys.openSessionSearchPalette)}
      />
      <SidebarReferencePrimaryMenuItem
        icon={IconFileSearch}
        label="Search by Prompt"
        onSelect={onSearchPreviousSessionsByPrompt}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconUsersGroup}
        label="Agents Hub"
        onSelect={onOpenAgentsHub}
      />
      <SidebarReferencePrimaryMenuItem
        icon={IconClock}
        label="Automations Overview"
        onSelect={onOpenAutomations}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconDeviceMobile}
        label="Mobile"
        onSelect={onOpenMobile}
      />
      {PET_CONTROLS_VISIBLE ? (
        <SidebarReferencePrimaryMenuItem
          icon={IconRobotFace}
          label="Wake Pet"
          onSelect={onTogglePetOverlay}
        />
      ) : null}
      {showKeepAwakeButton ? (
        <>
          <SidebarReferencePrimaryMenuItem
            icon={keepAwakeRuntime ? IconCoffee : IconMoon}
            label="Keep awake"
            onSelect={onOpenKeepAwakeMenu}
            trailingIcon={IconChevronRight}
          />
        </>
      ) : null}
      <SidebarReferencePrimaryMenuItem
        icon={IconUsersGroup}
        label="Join Discord"
        onSelect={onOpenDiscord}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconKeyboard}
        label="Hotkeys"
        onSelect={onOpenHotkeys}
        shortcut={formatSidebarMenuHotkeyLabel(hotkeys.openHotkeys)}
      />
    </div>
  );
}

export function SidebarReferenceFooter({
  commandPaletteHotkey,
  onOpenQuickAccess,
  onOpenSettings,
}: {
  commandPaletteHotkey?: string;
  onOpenQuickAccess: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <footer className="reference-sidebar-footer">
      <div className="reference-sidebar-search-slot reference-sidebar-footer-quick-access-slot">
        <div className="reference-sidebar-nav-item">
          <Button
            aria-label="Commands"
            className="reference-sidebar-nav-button reference-sidebar-footer-quick-access-button"
            onClick={onOpenQuickAccess}
            size="sm"
            type="button"
            variant="ghost"
          >
            <IconBolt
              aria-hidden="true"
              className="reference-sidebar-nav-icon reference-sidebar-search-icon"
              size={15}
              stroke={1.8}
            />
            <span className="reference-sidebar-nav-label">Commands</span>
            {commandPaletteHotkey ? (
              <kbd className="reference-sidebar-nav-shortcut">{commandPaletteHotkey}</kbd>
            ) : null}
          </Button>
        </div>
      </div>
      <div className="reference-sidebar-primary-menu-cell">
        <SidebarReferenceShortcutButton
          icon={IconSettings}
          label="Settings"
          onClick={onOpenSettings}
        />
      </div>
    </footer>
  );
}

export function SidebarReferenceKeepAwakeDropdown({
  activeDuration,
  isRunning,
  onBack,
  onOpenPowerSettings,
  onStartKeepAwake,
  onStopKeepAwake,
}: {
  activeDuration?: KeepAwakeDurationMinutes;
  isRunning: boolean;
  onBack: () => void;
  onOpenPowerSettings: () => void;
  onStartKeepAwake: (durationMinutes: KeepAwakeDurationMinutes) => void;
  onStopKeepAwake: () => void;
}) {
  return (
    <div className="reference-sidebar-primary-dropdown" role="menu">
      <SidebarReferencePrimaryMenuItem
        icon={IconArrowLeft}
        label="More"
        onSelect={onBack}
      />
      <SidebarReferencePrimaryMenuSeparator />
      <div className="reference-sidebar-primary-menu-label">Keep awake period</div>
      {KEEP_AWAKE_DURATION_OPTIONS.map((option) => (
        <SidebarReferencePrimaryMenuItem
          active={activeDuration === option.value}
          icon={IconCoffee}
          key={option.value}
          label={getSidebarKeepAwakeMenuLabel(option.label)}
          onSelect={() => onStartKeepAwake(option.value)}
        />
      ))}
      {isRunning ? (
        <SidebarReferencePrimaryMenuItem
          icon={IconSquareMinus}
          label="Don't keep awake"
          onSelect={onStopKeepAwake}
        />
      ) : null}
      <SidebarReferencePrimaryMenuSeparator />
      <SidebarReferencePrimaryMenuItem
        icon={IconSettings}
        label="Power Settings"
        onSelect={onOpenPowerSettings}
      />
    </div>
  );
}

export function SidebarReferencePrimaryMenuItem({
  active = false,
  icon: Icon,
  label,
  onSelect,
  shortcut,
  trailingIcon: TrailingIcon,
}: {
  active?: boolean;
  icon: TablerIcon;
  label: string;
  onSelect: () => void;
  shortcut?: string;
  trailingIcon?: TablerIcon;
}) {
  return (
    <button
      className="reference-sidebar-primary-menu-item"
      onClick={onSelect}
      role="menuitem"
      type="button"
    >
      <Icon aria-hidden="true" className="reference-sidebar-primary-menu-icon" size={16} stroke={1.8} />
      <span className="reference-sidebar-primary-menu-label-text">{label}</span>
      {shortcut ? (
        <span className="reference-sidebar-primary-menu-shortcut">{shortcut}</span>
      ) : null}
      {TrailingIcon ? (
        <TrailingIcon
          aria-hidden="true"
          className="reference-sidebar-primary-menu-trailing-icon"
          size={15}
          stroke={1.8}
        />
      ) : null}
      {active ? (
        <IconCheck aria-hidden="true" className="reference-sidebar-primary-menu-check" size={15} stroke={1.8} />
      ) : null}
    </button>
  );
}

export function SidebarReferencePrimaryMenuSeparator() {
  return <div className="reference-sidebar-primary-menu-separator" role="separator" />;
}

export function getSidebarKeepAwakeMenuLabel(label: string): string {
  return label === "Until turned off" ? label : `For ${label.toLowerCase()}`;
}

export function formatSidebarMenuHotkeyLabel(hotkey: string | undefined): string | undefined {
  return hotkey ? formatSidebarHotkeyLabel(hotkey) : undefined;
}

export function isNode(value: EventTarget | null): value is Node {
  return value instanceof Node;
}

export function SidebarReferenceSectionHeader({
  activeSessionsSortMode,
  actionsAlwaysVisible,
  agents = [],
  bulkActionLabel,
  collapsed,
  containsActiveSession = false,
  dragHandleRef,
  onAddProject,
  onBulkProjectToggle,
  onConfigureAgents,
  onCreateBrowserChat,
  onCreateChat,
  onEdit,
  onFilterChats,
  onRunAgent,
  onSetActiveSessionsSortMode,
  onSetSidebarV2Layout,
  onSetSidebarVersion,
  onToggleShowHidden,
  onToggleSessionTagFilter,
  onToggleCollapsed,
  primaryAgentId,
  remoteConnectionControl,
  sectionKey,
  selectedSessionTagFilters = [],
  sessionSummary,
  sessionTagListItems,
  sidebarV2Layout = "flat",
  sidebarVersion = "v1",
  title,
  showHidden = false,
  useColoredAgentIcons = false,
}: {
  activeSessionsSortMode?: SidebarActiveSessionsSortMode;
  actionsAlwaysVisible?: boolean;
  agents?: readonly SidebarAgentButton[];
  bulkActionLabel?: string;
  collapsed: boolean;
  containsActiveSession?: boolean;
  dragHandleRef?: (element: Element | null) => void;
  onAddProject?: () => void;
  onBulkProjectToggle?: () => void;
  onConfigureAgents?: () => void;
  onCreateBrowserChat?: () => void;
  onCreateChat?: () => void;
  onEdit?: () => void;
  onFilterChats?: () => void;
  onRunAgent?: (agent: SidebarAgentButton) => void;
  onSetActiveSessionsSortMode?: (sortMode: SidebarActiveSessionsSortMode) => void;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * The Sort & Filter menu is the in-sidebar entry point for the Inbox
   * sidebar. Both writers are optional so section headers rendered without a
   * settings pipeline (remote machine headers) simply omit the group.
   */
  onSetSidebarV2Layout?: (layout: SidebarV2Layout) => void;
  onSetSidebarVersion?: (sidebarVersion: SidebarVersion) => void;
  onToggleShowHidden?: () => void;
  onToggleSessionTagFilter?: (tag: SidebarSessionTagFilter) => void;
  onToggleCollapsed: () => void;
  primaryAgentId?: string;
  remoteConnectionControl?: RemoteMachineHeaderConnectionControl;
  sectionKey: ReferenceSidebarSectionId;
  selectedSessionTagFilters?: readonly SidebarSessionTagFilter[];
  sessionSummary?: SidebarSectionSessionSummary;
  sessionTagListItems?: readonly SidebarSessionTagListItem[];
  sidebarV2Layout?: SidebarV2Layout;
  sidebarVersion?: SidebarVersion;
  title: string;
  showHidden?: boolean;
  useColoredAgentIcons?: boolean;
}) {
  /**
   * CDXC:SidebarReference 2026-05-08-01:41
   * Reference-mode Chats and Projects are collapsible section headers. Chats
   * exposes browser-chat and new-chat controls on hover, while Projects expose
   * add-project and expand/collapse-all controls on hover so the compact
   * Codex.app-style list keeps management actions nearby. Add Project owns both
   * folder selection and repository cloning through its source picker.
   *
   * CDXC:SidebarReference 2026-05-08-02:21
   * The project bulk control is one stateful text button: "Collapse All" while
   * any project is expanded, then "Expand Previous" after it collapses the
   * previously expanded projects.
   *
   * CDXC:SidebarReference 2026-05-08-02:56
   * The bulk project button stays icon-only in the visible UI: use
   * IconArrowsDiagonal2 for Collapse All and IconArrowsDiagonalMinimize for
   * Expand Previous, while preserving the text labels for tooltips and
   * accessibility.
   *
   * CDXC:Tooltips 2026-05-20-10:05:
   * Quick and Projects section-header actions use the same local left-side
   * tooltip treatment as the reference-sidebar hover icons because portaled
   * Radix tooltips mis-anchor in the native sidebar webview. Quick exposes
   * filter, browser, terminal, and agent-picker actions beside the section label.
   *
   * CDXC:SidebarStickyHeaders 2026-05-20-09:55:
   * Section headers need a stable section key in the DOM so spacing can be
   * tuned for Projects and Quick independently without depending on visible
   * label text or adjacent markup shape.
   *
   * CDXC:ManualSessionSorting 2026-06-05-12:30:
   * Quick and Projects expose the same filter-shaped sort control in their
   * section headers. Last Active Sorting remains the default, while Manual
   * Sorting preserves the first visible last-active snapshot and later
   * user-defined row order.
   *
   * CDXC:QuickAgents 2026-06-08-18:25:
   * Quick exposes the same selected-agent split picker as project headers, with
   * Browser and Terminal as separate section-header actions to its left. Keep
   * the agent picker at the far right of the Quick header cluster so it aligns
   * with project-header agent placement. The main agent half launches the
   * selected provider and the chevron opens the shared agent list plus Configure.
   *
   * CDXC:RemoteMachines 2026-06-10-09:54:
   * Remote machine headers keep Edit in the hover action cluster so users can
   * jump to that machine's Settings -> Remote fields, while the always-visible
   * connection-state control remains beside the machine title.
   *
   * CDXC:SidebarSortFilter 2026-06-15-21:24:
   * The section-header filter icon should use the stable hover label "Sort & Filter" even when the accessible label continues to expose the current sort mode and selected tag-filter count.
   */
  const [ sortMenuPosition, setSortMenuPosition ] = useState<HeaderSortMenuPosition>();
  const [ agentMenuPosition, setAgentMenuPosition ] = useState<HeaderSortMenuPosition>();
  const BulkProjectIcon =
    bulkActionLabel === "Collapse All" ? IconArrowsDiagonalMinimize : IconArrowsDiagonal2;
  const SectionIcon =
    sectionKey === "remote"
      ? IconCloud
      : sectionKey === "projects" && title === "Projects"
        ? IconFolders
        : undefined;
  const remoteConnectionError =
    remoteConnectionControl?.kind === "error" ? remoteConnectionControl : undefined;
  const remoteConnectionBusy =
    remoteConnectionControl?.kind === "busy" ? remoteConnectionControl : undefined;
  const leadingRemoteConnectionControl = remoteConnectionError ?? remoteConnectionBusy;
  const trailingRemoteConnectionControl = leadingRemoteConnectionControl
    ? undefined
    : remoteConnectionControl;
  const primaryAgent = agents.find((agent) => agent.agentId === primaryAgentId) ?? agents[ 0 ];
  const primaryAgentLabel = primaryAgent?.name ?? "Agent";
  const primaryAgentIconColorMode = useColoredAgentIcons ? "brand" : "monochrome";
  const normalizedSessionTagListItems = useMemo(
    () => normalizeSidebarSessionTagListItems(sessionTagListItems),
    [ sessionTagListItems ],
  );
  const hasTagFilters = selectedSessionTagFilters.length > 0;
  const hasActions =
    onAddProject ||
    onBulkProjectToggle ||
    onConfigureAgents ||
    onCreateBrowserChat ||
    onCreateChat ||
    onEdit ||
    onFilterChats ||
    onRunAgent ||
    onSetActiveSessionsSortMode ||
    onToggleShowHidden ||
    onToggleSessionTagFilter;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Session sorting is a V1-only concept: the Inbox is position-stable by
   * construction and ignores the sort mode entirely. So while V2 is active the
   * whole sort radio group disappears from this menu, and the trigger's
   * accessible name states the active sidebar instead of advertising a sort
   * order that does nothing.
   */
  const isSidebarV2Active = sidebarVersion === "v2";
  const sortModeLabel =
    activeSessionsSortMode === "manual" ? "Manual Sorting" : "Last Active Sorting";
  const showSortModeOptions = onSetActiveSessionsSortMode !== undefined && !isSidebarV2Active;
  const filterModeLabel = isSidebarV2Active ? "Inbox sidebar" : sortModeLabel;
  const filterLabel = hasTagFilters
    ? `${filterModeLabel}, ${selectedSessionTagFilters.length} tag filter${selectedSessionTagFilters.length === 1 ? "" : "s"
    }`
    : filterModeLabel;
  const hasActionStatus =
    (sessionSummary?.workingCount ?? 0) > 0 ||
    (sessionSummary?.attentionCount ?? 0) > 0;
  const shouldShowCollapsedStatus =
    collapsed &&
    sessionSummary !== undefined &&
    (hasActionStatus || sessionSummary.awakeCount > 0);

  const openSortMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setAgentMenuPosition(undefined);
    setSortMenuPosition({
      left: bounds.left,
      top: bounds.bottom + 4,
    });
  };

  const openAgentMenu = (event: ReactMouseEvent<HTMLButtonElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setSortMenuPosition(undefined);
    setAgentMenuPosition({
      left: bounds.right - REFERENCE_SECTION_AGENT_MENU_WIDTH_PX,
      top: bounds.bottom + 6,
    });
  };

  const selectSortMode = (sortMode: SidebarActiveSessionsSortMode) => {
    setSortMenuPosition(undefined);
    onSetActiveSessionsSortMode?.(sortMode);
  };

  const selectSidebarVersion = (nextSidebarVersion: SidebarVersion) => {
    setSortMenuPosition(undefined);
    onSetSidebarVersion?.(nextSidebarVersion);
  };

  const selectSidebarV2Layout = (nextLayout: SidebarV2Layout) => {
    setSortMenuPosition(undefined);
    onSetSidebarV2Layout?.(nextLayout);
  };

  const runAgent = (agent: SidebarAgentButton | undefined) => {
    setAgentMenuPosition(undefined);
    if (!agent) {
      onConfigureAgents?.();
      return;
    }
    onRunAgent?.(agent);
  };

  return (
    <div
      className="reference-sidebar-section-row"
      data-actions-always-visible={String(actionsAlwaysVisible === true)}
      data-collapsed={String(collapsed)}
      data-contains-active-session={String(containsActiveSession)}
      data-has-remote-connection-control={String(
        trailingRemoteConnectionControl !== undefined,
      )}
      data-reference-section={sectionKey}
    >
      {remoteConnectionError ? (
        <SidebarFixedTooltipButton
          aria-label={remoteConnectionError.label}
          className="reference-remote-machine-error-cloud"
          onClick={remoteConnectionError.onClick}
          tooltip={remoteConnectionError.label}
          tooltipSide="top"
          type="button"
        >
          <IconCloud aria-hidden="true" size={15} stroke={1.8} />
        </SidebarFixedTooltipButton>
      ) : remoteConnectionBusy ? (
        <SidebarFixedTooltipButton
          aria-busy="true"
          aria-disabled="true"
          aria-label={remoteConnectionBusy.label}
          className="reference-remote-machine-busy-indicator"
          tooltip={remoteConnectionBusy.label}
          tooltipSide="top"
          type="button"
        >
          <IconLoader2 aria-hidden="true" size={13} stroke={1.8} />
        </SidebarFixedTooltipButton>
      ) : null}
      <button
        aria-expanded={!collapsed}
        className="reference-sidebar-section-heading"
        onClick={onToggleCollapsed}
        ref={dragHandleRef}
        type="button"
      >
        {SectionIcon && !leadingRemoteConnectionControl ? (
          <SectionIcon
            aria-hidden="true"
            className="reference-sidebar-section-icon"
            size={15}
            stroke={1.8}
          />
        ) : null}
        <span className="reference-sidebar-section-title">{title}</span>
        {remoteConnectionControl ? null : (
          <IconCaretRightFilled
            aria-hidden="true"
            className="reference-sidebar-section-chevron"
            size={13}
          />
        )}
      </button>
      {trailingRemoteConnectionControl ? (
        <SidebarFixedTooltipButton
          aria-label={trailingRemoteConnectionControl.label}
          className="reference-remote-machine-connection-control"
          data-kind={trailingRemoteConnectionControl.kind}
          onClick={trailingRemoteConnectionControl.onClick}
          tooltip={trailingRemoteConnectionControl.label}
          tooltipSide="top"
          type="button"
        >
          <IconPlugConnected aria-hidden="true" size={14} stroke={1.9} />
        </SidebarFixedTooltipButton>
      ) : null}
      {shouldShowCollapsedStatus && sessionSummary ? (
        <div
          aria-label={[
            sessionSummary.workingCount > 0
              ? `${sessionSummary.workingCount} working`
              : "",
            sessionSummary.attentionCount > 0
              ? `${sessionSummary.attentionCount} done`
              : "",
            !hasActionStatus && sessionSummary.awakeCount > 0
              ? `${sessionSummary.awakeCount} awake terminals and browsers`
              : "",
          ]
            .filter(Boolean)
            .join(", ")}
          className="group-collapsed-status-counts reference-sidebar-section-status-counts"
        >
          {sessionSummary.workingCount > 0 ? (
            <span className="group-collapsed-status-count" data-activity="working">
              {sessionSummary.workingCount}
            </span>
          ) : null}
          {sessionSummary.attentionCount > 0 ? (
            <span className="group-collapsed-status-count" data-activity="attention">
              {sessionSummary.attentionCount}
            </span>
          ) : null}
          {!hasActionStatus && sessionSummary.awakeCount > 0 ? (
            <span className="group-collapsed-status-count" data-activity="awake">
              {sessionSummary.awakeCount}
            </span>
          ) : null}
        </div>
      ) : null}
      {hasActions ? (
        <div className="reference-sidebar-section-actions">
          {onSetActiveSessionsSortMode || onToggleSessionTagFilter ? (
            <SidebarFixedTooltipButton
              aria-expanded={sortMenuPosition !== undefined}
              aria-haspopup="menu"
              aria-label={`Filter sessions: ${filterLabel}`}
              className="reference-sidebar-section-action reference-sidebar-section-sort-action reference-sidebar-hover-action-tooltip"
              data-selected={String(
                (activeSessionsSortMode === "manual" && !isSidebarV2Active) ||
                  hasTagFilters ||
                  showHidden,
              )}
              onClick={openSortMenu}
              tooltip="Sort & Filter"
              tooltipAlign="end"
              type="button"
            >
              <IconFilter2 aria-hidden="true" size={14} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onCreateBrowserChat ? (
            <SidebarFixedTooltipButton
              aria-label="Quick Browser Tab"
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onCreateBrowserChat}
              tooltip="Quick Browser Tab"
              tooltipAlign="end"
              type="button"
            >
              <IconWorld aria-hidden="true" size={15} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onCreateChat ? (
            <SidebarFixedTooltipButton
              aria-label="Quick Terminal"
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onCreateChat}
              tooltip="Quick Terminal"
              tooltipAlign="end"
              type="button"
            >
              <IconTerminal2 aria-hidden="true" size={14} stroke={2} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onRunAgent || onConfigureAgents ? (
            <div
              className="group-agent-split-button reference-sidebar-section-agent-picker"
              data-open={String(agentMenuPosition !== undefined)}
            >
              <SidebarFixedTooltipButton
                aria-label={`Create ${primaryAgentLabel}`}
                className="group-agent-main-button reference-sidebar-hover-action-tooltip"
                onClick={() => runAgent(primaryAgent)}
                tooltip={`Create ${primaryAgentLabel}`}
                tooltipAlign="end"
                type="button"
              >
                <ProjectAgentLauncherIcon
                  agent={primaryAgent}
                  colorMode={primaryAgentIconColorMode}
                />
              </SidebarFixedTooltipButton>
              <SidebarFixedTooltipButton
                aria-expanded={agentMenuPosition !== undefined}
                aria-haspopup="menu"
                aria-label="Select agent"
                className="group-agent-toggle-button reference-sidebar-hover-action-tooltip"
                data-open={String(agentMenuPosition !== undefined)}
                onClick={openAgentMenu}
                tooltip="Select Agent"
                tooltipAlign="end"
                type="button"
              >
                <IconChevronDown aria-hidden="true" size={13} stroke={2} />
              </SidebarFixedTooltipButton>
            </div>
          ) : null}
          {onBulkProjectToggle && bulkActionLabel ? (
            <SidebarFixedTooltipButton
              aria-label={bulkActionLabel}
              className="reference-sidebar-section-action reference-sidebar-section-bulk-project-action reference-sidebar-hover-action-tooltip"
              onClick={onBulkProjectToggle}
              tooltip={bulkActionLabel}
              tooltipAlign="end"
              type="button"
            >
              <BulkProjectIcon aria-hidden="true" size={14} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onEdit ? (
            <SidebarFixedTooltipButton
              aria-label={`Edit ${title}`}
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onEdit}
              tooltip="Edit"
              tooltipAlign="end"
              type="button"
            >
              <IconEdit aria-hidden="true" size={14} stroke={1.9} />
            </SidebarFixedTooltipButton>
          ) : null}
          {onAddProject ? (
            <SidebarFixedTooltipButton
              aria-label="Add project"
              className="reference-sidebar-section-action reference-sidebar-hover-action-tooltip"
              onClick={onAddProject}
              tooltip="Add project"
              tooltipAlign="end"
              type="button"
            >
              <IconPlus aria-hidden="true" size={14} stroke={2} />
            </SidebarFixedTooltipButton>
          ) : null}
        </div>
      ) : null}
      {sortMenuPosition ? (
        <SidebarContextMenuPortal
          menuClassName="session-context-menu reference-sidebar-sort-menu"
          menuStyle={{
            left: sortMenuPosition.left,
            top: sortMenuPosition.top,
          }}
          onDismiss={() => setSortMenuPosition(undefined)}
        >
          {onToggleShowHidden ? (
            <>
              <button
                aria-checked={showHidden}
                className="session-context-menu-item"
                onClick={onToggleShowHidden}
                role="menuitemcheckbox"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(showHidden)}
                  size={14}
                  stroke={2}
                />
                Show hidden
              </button>
              <div className="session-context-menu-divider" role="separator" />
            </>
          ) : null}
          {/*
            * CDXC:SidebarV2 2026-07-29:
            * The sidebar picker sits above the sort radios because it chooses
            * which sidebar renders at all. Manual Sorting is a V1-only concept,
            * so it disappears while the Inbox sidebar is active instead of
            * offering an order the inbox intentionally ignores.
            */}
          {onSetSidebarVersion ? (
            <>
              <button
                aria-checked={sidebarVersion !== "v2"}
                className="session-context-menu-item"
                onClick={() => selectSidebarVersion("v1")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(sidebarVersion !== "v2")}
                  size={14}
                  stroke={2}
                />
                Classic sidebar
              </button>
              <button
                aria-checked={sidebarVersion === "v2"}
                className="session-context-menu-item"
                onClick={() => selectSidebarVersion("v2")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(sidebarVersion === "v2")}
                  size={14}
                  stroke={2}
                />
                Inbox sidebar (New)
              </button>
              {sidebarVersion === "v2" && onSetSidebarV2Layout ? (
                <button
                  aria-checked={sidebarV2Layout === "byProject"}
                  className="session-context-menu-item"
                  onClick={() =>
                    selectSidebarV2Layout(sidebarV2Layout === "byProject" ? "flat" : "byProject")
                  }
                  role="menuitemcheckbox"
                  type="button"
                >
                  <IconCheck
                    aria-hidden="true"
                    className="session-context-menu-icon"
                    data-visible={String(sidebarV2Layout === "byProject")}
                    size={14}
                    stroke={2}
                  />
                  Group by Project
                </button>
              ) : null}
              {showSortModeOptions || onToggleSessionTagFilter ? (
                <div className="session-context-menu-divider" role="separator" />
              ) : null}
            </>
          ) : null}
          {showSortModeOptions ? (
            <>
              <button
                aria-checked={activeSessionsSortMode !== "manual"}
                className="session-context-menu-item"
                onClick={() => selectSortMode("lastActivity")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(activeSessionsSortMode !== "manual")}
                  size={14}
                  stroke={2}
                />
                Last Active Sorting
              </button>
              <button
                aria-checked={activeSessionsSortMode === "manual"}
                className="session-context-menu-item"
                onClick={() => selectSortMode("manual")}
                role="menuitemradio"
                type="button"
              >
                <IconCheck
                  aria-hidden="true"
                  className="session-context-menu-icon"
                  data-visible={String(activeSessionsSortMode === "manual")}
                  size={14}
                  stroke={2}
                />
                Manual Sorting
              </button>
            </>
          ) : null}
          {showSortModeOptions && onToggleSessionTagFilter ? (
            <div className="session-context-menu-divider" role="separator" />
          ) : null}
          {onToggleSessionTagFilter
            ? normalizedSessionTagListItems.map((item) => {
              if (!item.visible) {
                return null;
              }
              if (item.type === "separator") {
                return item.enabled ? (
                  <div className="session-context-menu-divider" key={item.id} role="separator" />
                ) : null;
              }

              const filter = getSidebarSessionTagListItemFilter(item);
              if (!filter) {
                return null;
              }
              const isSelected = selectedSessionTagFilters.includes(filter);
              return (
                <button
                  aria-checked={isSelected}
                  className="session-context-menu-item reference-sidebar-tag-filter-item"
                  data-selected={String(isSelected)}
                  disabled={!item.enabled}
                  key={item.id}
                  onClick={() => onToggleSessionTagFilter(filter)}
                  role="menuitemcheckbox"
                  type="button"
                >
                  <SessionTagIcon
                    className="session-context-menu-icon session-tag-colored-icon"
                    fillFavorite
                    size={14}
                    stroke={1.8}
                    tag={filter}
                  />
                  {getSidebarSessionTagLabel(filter)}
                  <IconCheck
                    aria-hidden="true"
                    className="session-context-menu-trailing-icon reference-sidebar-tag-filter-check"
                    data-visible={String(isSelected)}
                    size={14}
                    stroke={2}
                  />
                </button>
              );
            })
            : null}
        </SidebarContextMenuPortal>
      ) : null}
      {agentMenuPosition ? (
        <SidebarContextMenuPortal
          menuClassName="session-context-menu group-agent-menu reference-sidebar-agent-menu"
          menuStyle={{
            left: `${agentMenuPosition.left}px`,
            top: `${agentMenuPosition.top}px`,
            width: `${REFERENCE_SECTION_AGENT_MENU_WIDTH_PX}px`,
          }}
          onDismiss={() => setAgentMenuPosition(undefined)}
        >
          {agents.map((agent) => (
            <button
              aria-label={agent.name}
              aria-pressed={primaryAgent?.agentId === agent.agentId}
              className="session-context-menu-item group-control-menu-item group-agent-menu-item"
              data-selected={String(primaryAgent?.agentId === agent.agentId)}
              key={agent.agentId}
              onClick={() => runAgent(agent)}
              role="menuitem"
              type="button"
            >
              <ProjectAgentLauncherIcon agent={agent} colorMode="brand" />
              <span className="group-agent-menu-label">{agent.name}</span>
              <AgentMenuChatIndicator agent={agent} />
            </button>
          ))}
          {agents.length > 0 ? (
            <div className="session-context-menu-divider" role="separator" />
          ) : null}
          <button
            className="session-context-menu-item group-control-menu-item group-agent-menu-item"
            onClick={() => {
              setAgentMenuPosition(undefined);
              onConfigureAgents?.();
            }}
            role="menuitem"
            type="button"
          >
            <IconSettings aria-hidden="true" className="session-context-menu-icon" size={14} />
            <span className="group-agent-menu-label">Configure</span>
          </button>
        </SidebarContextMenuPortal>
      ) : null}
    </div>
  );
}

