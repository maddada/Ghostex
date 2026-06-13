import { DragDropProvider, type DragDropEventHandlers } from "@dnd-kit/react";
import { isSortableOperation, useSortable } from "@dnd-kit/react/sortable";
import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type RefObject,
  type UIEvent as ReactUIEvent,
} from "react";
import Fuse from "fuse.js";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldTitle,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { SidebarSessionSearchField } from "./sidebar-session-search-overlay";
import {
  IconAsterisk,
  IconAlertTriangle,
  IconChevronRight,
  IconCircleCheckFilled,
  IconCircleX,
  IconCodeDots,
  IconDeviceDesktop,
  IconDownload,
  IconFolderOpen,
  IconGripVertical,
  IconInfoCircle,
  IconPencil,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconDeviceFloppy,
  IconTerminal2,
  IconTools,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import { COMPLETION_SOUND_OPTIONS, type CompletionSoundSetting } from "../shared/completion-sound";
import { GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES } from "../shared/ghostty-config-actions";
import {
  resolveSidebarTheme,
  type SidebarAgentHookStatusMessage,
  type SidebarAgentHookStatusItem,
  type SidebarGhostexCliStatusMessage,
  type SidebarGhostexFolderStatsMessage,
  type SidebarOSIntegrationStatusMessage,
  type SidebarProjectSettingsItem,
  type SidebarTheme,
  type SidebarThemeVariant,
} from "../shared/session-grid-contract";
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  APP_SHOTS_HOTKEY_OPTIONS,
  BROWSER_FEEDBACK_TOOL_OPTIONS,
  DEFAULT_ghostex_SETTINGS,
  DEFAULT_EDITOR_COMMAND_OPTIONS,
  MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
  GHOSTTY_COPY_ON_SELECT_OPTIONS,
  GHOSTTY_SCROLLBAR_OPTIONS,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  type PromptEditorBackend,
  SESSION_PERSISTENCE_PROVIDER_OPTIONS,
  SESSION_STATUS_INDICATOR_SIZE_OPTIONS,
  SESSION_TITLE_GENERATION_AGENT_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_SIDE_OPTIONS,
  SIDEBAR_THEME_SETTING_OPTIONS,
  applySidebarSettingsPreset,
  getSessionTitleGenerationCommandPreview,
  getSidebarSettingsPresetId,
  MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MAX_SIDEBAR_DEFAULT_WIDTH_PX,
  MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MIN_SIDEBAR_DEFAULT_WIDTH_PX,
  normalizeghostexSettings,
  normalizeRemoteMachineSettings,
  type BrowserFeedbackTool,
  type AppShotsHotkey,
  type AutoSleepIdleMinutes,
  type DefaultEditorCommand,
  type GhosttyConfirmCloseSurface,
  type GhosttyCopyOnSelect,
  type GhosttyScrollbar,
  type KeepAwakeDurationMinutes,
  type RemoteMachineSettings,
  type SessionPersistenceProvider,
  type SessionStatusIndicatorSize,
  type SessionTitleGenerationAgent,
  type SidebarSettingsPresetId,
  type SidebarSide,
  type TerminalCursorStyle,
  type ghostexSettings,
} from "../shared/ghostex-settings";
import {
  BUILT_IN_WORKSPACE_OPEN_TARGETS,
  CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX,
  createWorkspaceOpenTargetSlug,
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetHiddenIds,
  type CustomWorkspaceOpenTarget,
} from "../shared/workspace-open-targets";
import {
  FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS,
  isFirstLaunchSetupMainSettingVisible,
  type FirstLaunchSetupMainSettingKey,
} from "../shared/first-launch-setup-settings";
import {
  AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS,
  supportsAgentAcceptAll,
  type AgentAcceptAllMode,
} from "../shared/sidebar-agent-accept-all";
import {
  DEFAULT_SIDEBAR_AGENTS,
  getDefaultSidebarAgentByIcon,
  type SidebarAgentButton,
  type SidebarAgentIcon,
} from "../shared/sidebar-agents";
import {
  DEFAULT_BROWSER_ACTION_URL,
  type SidebarActionType,
  type SidebarCommandButton,
} from "../shared/sidebar-commands";
import {
  DEFAULT_SIDEBAR_COMMAND_ICON,
  DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
  type SidebarCommandIcon,
} from "../shared/sidebar-command-icons";
import {
  DEFAULT_ghostex_HOTKEYS,
  GHOSTEX_HOTKEY_DEFINITIONS,
  normalizeHotkeyText,
  normalizeghostexHotkeySettings,
  type ghostexHotkeyActionId,
  type ghostexHotkeySettings,
} from "../shared/ghostex-hotkeys";
import { PET_OPTIONS, type PetId } from "../shared/pets";
import { AGENT_LOGO_COLORS, AGENT_LOGOS } from "./agent-logos";
import { EditorBrandIcon, getEditorBrandIconId } from "./brand-icons";
import { BundledAgentSkillsPanel } from "./bundled-agent-skills-panel";
import { HotkeyRecorderField } from "./hotkey-recorder-field";
import { PetAvatar } from "./pet-avatar";
import { CommandIconPicker } from "./command-icon-picker";
import { SidebarCommandIconGlyph } from "./sidebar-command-icon";
import { useSidebarStore } from "./sidebar-store";
import type { AgentConfigDraft } from "./agent-config-modal";
import type { CommandConfigDraft } from "./command-config-modal";
import type { WebviewApi } from "./webview-api";

const NUMERIC_SETTINGS_DEBOUNCE_MS = 180;
const GHOSTTY_THEME_UNMANAGED_VALUE = "__ghostex_ghostty_theme_unmanaged__";
const MODIFIED_SETTING_TOOLTIP = "Modified Setting.\n \nClick to Reset to Default";
const PASTE_PREVIEWABLE_IMAGES_DESCRIPTION =
  "Paste clipboard images as previewable Markdown links with Cmd+V or Ctrl+V. Hold Cmd over the linked path to preview it in the terminal, and see the same image preview in the Ctrl+G Rich Prompt Editor.";
const HOTKEY_SETTINGS_SECTIONS: readonly HotkeySettingsSectionDefinition[] = [
  {
    id: "general",
    ids: [
      "createSession",
      "openCommandPalette",
      "openSettings",
      "toggleSidebarCollapsed",
      "moveSidebar",
    ],
    title: "General",
  },
  {
    id: "paneActions",
    ids: [
      "openBrowserPane",
      "splitMore",
      "splitMoreDown",
      "rotatePanesClockwise",
      "mergeAllTabs",
      "renameActiveSession",
      "delayedSend",
      "forkSession",
      "reloadSession",
      "popOutPane",
    ],
    title: "Pane Actions",
  },
  {
    id: "navigation",
    ids: [
      "focusPreviousGroup",
      "focusNextGroup",
      "focusPreviousSession",
      "focusNextSession",
      "focusUp",
      "focusRight",
      "focusDown",
      "focusLeft",
    ],
    title: "Navigation",
  },
  {
    id: "groups",
    ids: ["focusGroup1", "focusGroup2", "focusGroup3", "focusGroup4", "focusGroup5"],
    title: "Groups",
  },
  {
    id: "sessionSlots",
    ids: [
      "focusSessionSlot1",
      "focusSessionSlot2",
      "focusSessionSlot3",
      "focusSessionSlot4",
      "focusSessionSlot5",
      "focusSessionSlot6",
      "focusSessionSlot7",
      "focusSessionSlot8",
      "focusSessionSlot9",
    ],
    title: "Session Slots",
  },
  {
    id: "actions",
    ids: [
      "runActionSlot1",
      "runActionSlot2",
      "runActionSlot3",
      "runActionSlot4",
      "runActionSlot5",
    ],
    title: "Actions",
  },
];

type SettingSearchDefinition = {
  key: string;
  options?: ReadonlyArray<{ label: string; value: string }>;
  subtitle?: string;
  title: string;
};

type SettingsSectionSearchResult = {
  isSearching: boolean;
  sectionMatches: boolean;
  visibleSettingKeys: Set<string>;
};

type SettingModificationProps = {
  isModified?: boolean;
  onResetToDefault?: () => void;
};

export type SettingsModalTab =
  | "settings"
  | "integrations"
  | "osIntegration"
  | "remote"
  | "projects"
  | "agents"
  | "actions"
  | "openTargets"
  | "hotkeys";

type MainSettingsSectionId =
  | "agents"
  | "sidebar"
  | "statusIndicators"
  | "sessionCards"
  | "workspace"
  | "terminal"
  | "terminalBehavior"
  | "terminalScrolling"
  | "browser"
  | "editor"
  | "autoSleep"
  | "power"
  | "sounds"
  | "storage";

export type MainSettingsInitialSectionId = MainSettingsSectionId;

const MAIN_SETTINGS_SECTION_SETTING_KEYS: Record<
  MainSettingsSectionId,
  readonly FirstLaunchSetupMainSettingKey[]
> = {
  agents: ["agentAcceptAllEnabled"],
  sidebar: [
    "sidebarSettingsPreset",
    "sidebarSide",
    "sidebarDefaultWidthPx",
    "projectSessionListCollapsedCount",
    "sidebarTheme",
    "sessionStatusIndicatorSize",
    "agentManagerZoomPercent",
    "createSessionOnSidebarDoubleClick",
    "renameSessionOnDoubleClick",
  ],
  /*
   * CDXC:BrowserSettings 2026-05-22-09:18:
   * Browser-related controls belong in one Browser section on the main
   * Settings tab: URL open target and browser-pane feedback tool selection.
   */
  browser: ["browserFeedbackTool"],
  /*
   * CDXC:StatusIndicators 2026-05-20-12:00:
   * Status Indicators groups desktop session badges and the optional sidebar pet
   * overlay because both surfaces communicate session state at a glance.
   */
  statusIndicators: [
    "hideFloatingSessionStatusIndicators",
    "hideMenuBarSessionStatusIndicators",
    "petOverlayEnabled",
    "selectedPetId",
  ],
  sessionCards: [
    "hideSessionAgentIconUntilHover",
    "hideBrowserFaviconUntilHover",
    "showCloseButtonOnSessionCards",
    "hideLastActiveTimeOnSessionCards",
    "showSessionCloseContextMenuAction",
    "showSessionCommandCopyActions",
    "showSessionDetailsCopyAction",
  ],
  workspace: [
    "workspaceActivePaneBorderColor",
    "workspaceBackgroundColor",
    "clickToWakeSleepingSessions",
    "commandsPanelDefaultHeightPx",
    "debuggingMode",
  ],
  terminal: [
    "ghosttySettingsActions",
    "terminalGhosttyTheme",
    "terminalFontFamily",
    "terminalFontSize",
    "terminalFontWeight",
    "terminalLineHeight",
    "terminalLetterSpacing",
    "terminalCursorStyle",
    "terminalCursorStyleBlink",
    "sessionPersistenceProvider",
    "showSessionIdInTerminalPanes",
    "promptEditorBackend",
    "customPromptEditorCommand",
  ],
  terminalBehavior: [
    "terminalScrollbackLimitMb",
    "terminalCopyOnSelect",
    "terminalConfirmCloseSurface",
    "terminalClipboardTrimTrailingSpaces",
    "terminalClipboardPasteProtection",
    "terminalPastePreviewableImages",
    "terminalMouseHideWhileTyping",
    "terminalScrollbar",
  ],
  terminalScrolling: [
    "terminalMouseScrollMultiplierPrecision",
    "terminalMouseScrollMultiplierDiscrete",
    "terminalScrollToBottomWhenTyping",
  ],
  editor: [
    "defaultEditorCommand",
    "customDefaultEditorCommand",
    "codeServerLinkVscodeUserConfig",
    "codeServerUseVscodeInsidersUserConfig",
    "hideProjectHeaderDiffStats",
    "showProjectEditorDiffFileCount",
    "showUntrackedProjectDiffWhenNoTrackedChanges",
  ],
  autoSleep: [
    "autoSleepCodeEditorEnabled",
    "autoSleepCodeEditorIdleMinutes",
    "autoSleepGitEditorEnabled",
    "autoSleepGitEditorIdleMinutes",
    "autoSleepProjectEditorEnabled",
    "autoSleepProjectEditorIdleMinutes",
    "autoSleepBrowserSessionsEnabled",
    "autoSleepBrowserIdleMinutes",
    "autoSleepAgentSessionsEnabled",
    "autoSleepAgentIdleMinutes",
    "autoSleepRequireAgentResumeCommand",
    "autoSleepFavoriteAgentSessions",
  ],
  power: [
    "hideKeepAwakeTitlebarControl",
    "keepAwakeDefaultDurationMinutes",
    "keepAwakeAllowDisplaySleep",
    "keepAwakePreventLidSleep",
    "keepAwakeActivateOnLaunch",
    "keepAwakeActivateOnExternalDisplay",
    "keepAwakeDeactivateBelowBatteryThreshold",
    "keepAwakeBatteryThresholdPercent",
    "keepAwakeDeactivateOnLowPowerMode",
    "keepAwakeDeactivateOnUserSwitch",
  ],
  sounds: [
    "completionBellEnabled",
    "completionSound",
    "showMacOSAttentionNotifications",
    "actionCompletionSound",
  ],
  storage: [],
};

type HotkeySettingsSectionId =
  | "general"
  | "paneActions"
  | "navigation"
  | "groups"
  | "sessionSlots"
  | "actions";

type HotkeySettingsSectionDefinition = {
  ids: readonly ghostexHotkeyActionId[];
  id: HotkeySettingsSectionId;
  title: string;
};

let rememberedSettingsModalTab: SettingsModalTab | undefined;
const rememberedSettingsModalScrollTopByTab: Partial<Record<SettingsModalTab, number>> = {};

function getInitialSettingsModalTab(initialTab: SettingsModalTab): SettingsModalTab {
  /**
   * CDXC:Settings 2026-05-11-09:06
   * Settings remembers the last selected tab for the current app session. A
   * non-default entry point such as Hotkeys still opens its requested tab, then
   * that tab becomes the remembered choice until the app restarts.
   */
  if (initialTab !== "settings") {
    return initialTab;
  }
  return rememberedSettingsModalTab ?? initialTab;
}

function isSearchableSettingsModalTab(tab: SettingsModalTab): tab is "settings" | "hotkeys" {
  return tab === "settings" || tab === "hotkeys";
}

function hasActiveHotkeyRecorder(): boolean {
  return Boolean(document.querySelector("[data-hotkey-recorder='true'][data-recording='true']"));
}

function isEditableSettingsModalEventTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  return Boolean(target.closest("input, textarea, select, [contenteditable='true']"));
}

function getActiveSettingsModalScrollViewport(dialogElement: HTMLElement | null): HTMLElement | null {
  return (
    dialogElement
      ?.querySelector<HTMLElement>("[role='tabpanel'][data-state='active']")
      ?.querySelector<HTMLElement>("[data-slot='scroll-area-viewport']") ?? null
  );
}

function getMainSettingsSectionRef(
  sectionId: MainSettingsSectionId,
  refs: Record<MainSettingsSectionId, RefObject<HTMLDivElement | null>>,
): RefObject<HTMLDivElement | null> {
  return refs[sectionId];
}

export type GhosttySettingsAction =
  | "applyRecommendedGhosttySettings"
  | "openGhosttyConfigFile"
  | "openGhosttySettingsDocs"
  | "resetGhosttySettingsToDefault";

export type SettingsModalPresentation = "default" | "firstLaunchSetup";

export type SettingsModalProps = {
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentHookStatusLoading?: boolean;
  firstLaunchSetupVisibleSettings?: ReadonlySet<FirstLaunchSetupMainSettingKey>;
  initialSection?: MainSettingsInitialSectionId;
  initialSearchQuery?: string;
  initialRemoteMachineId?: string;
  initialTab?: SettingsModalTab;
  isOpen: boolean;
  presentation?: SettingsModalPresentation;
  onChange: (settings: ghostexSettings) => void;
  onClose: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenMacOSNotificationSettings?: () => void;
  onOpenFirstLaunchSetup?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onOpenGhostexFolder?: () => void;
  onGhosttySettingsAction?: (action: GhosttySettingsAction) => void;
  onInstallAgentOrchestrationSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGte?: () => void;
  onInstallGhostexCli?: () => void;
  onPlayCompletionSound?: (sound: CompletionSoundSetting) => void;
  onRequestMacOSNotificationPermission?: () => void;
  onInstallAgentHooks?: () => void;
  onRequestAgentHookStatus?: () => void;
  onRequestGhostexCliStatus?: () => void;
  onRequestGhostexFolderStats?: () => void;
  onRequestOSIntegrationStatus?: () => void;
  onSetOSIntegrationDefaults?: (target: "editor" | "terminalLinks" | "scriptRunner" | "all") => void;
  onTestAgentTaskCompletion?: () => void;
  projects?: SidebarProjectSettingsItem[];
  settings?: ghostexSettings;
  theme?: SidebarTheme;
  vscode?: WebviewApi;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading?: boolean;
  ghostexFolderStats?: SidebarGhostexFolderStatsMessage;
  ghostexFolderStatsLoading?: boolean;
  osIntegrationStatus?: SidebarOSIntegrationStatusMessage;
  osIntegrationStatusLoading?: boolean;
};

export function SettingsModal({
  agentHookStatus,
  agentHookStatusLoading = false,
  firstLaunchSetupVisibleSettings,
  initialSection,
  initialSearchQuery,
  initialRemoteMachineId,
  initialTab = "settings",
  isOpen,
  onChange,
  onClose,
  presentation = "default",
  onOpenAccessibilityPreferences,
  onOpenMacOSNotificationSettings,
  onOpenFirstLaunchSetup,
  onOpenScreenRecordingPreferences,
  onOpenGhostexFolder,
  onGhosttySettingsAction,
  onInstallAgentOrchestrationSkill,
  onInstallBrowserControl,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallGenerateTitleSkill,
  onInstallGte,
  onInstallGhostexCli,
  onPlayCompletionSound,
  onRequestMacOSNotificationPermission,
  onInstallAgentHooks,
  onRequestAgentHookStatus,
  onRequestGhostexCliStatus,
  onRequestGhostexFolderStats,
  onRequestOSIntegrationStatus,
  onSetOSIntegrationDefaults,
  onTestAgentTaskCompletion,
  projects = [],
  settings,
  theme = "dark-blue",
  vscode,
  ghostexCliStatus,
  ghostexCliStatusLoading = false,
  ghostexFolderStats,
  ghostexFolderStatsLoading = false,
  osIntegrationStatus,
  osIntegrationStatusLoading = false,
}: SettingsModalProps) {
  const isFirstLaunchSetup = presentation === "firstLaunchSetup";
  const [draft, setDraft] = useState<ghostexSettings>(normalizeghostexSettings(settings));
  const [settingsSearchQuery, setSettingsSearchQuery] = useState("");
  const [hotkeysSearchQuery, setHotkeysSearchQuery] = useState("");
  const [activeTab, setActiveTabState] = useState<SettingsModalTab>(() =>
    getInitialSettingsModalTab(initialTab),
  );
  const dialogContentRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const pendingSettingsRef = useRef<ghostexSettings | undefined>(undefined);
  const pendingTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const autoSleepSectionRef = useRef<HTMLDivElement>(null);
  const browserSectionRef = useRef<HTMLDivElement>(null);
  const editorSectionRef = useRef<HTMLDivElement>(null);
  const ghosttyBehaviorSectionRef = useRef<HTMLDivElement>(null);
  const ghosttyScrollingSectionRef = useRef<HTMLDivElement>(null);
  const ghosttyTerminalSectionRef = useRef<HTMLDivElement>(null);
  const powerSectionRef = useRef<HTMLDivElement>(null);
  const statusIndicatorsSectionRef = useRef<HTMLDivElement>(null);
  const sessionCardsSectionRef = useRef<HTMLDivElement>(null);
  const agentsOnboardingSectionRef = useRef<HTMLDivElement>(null);
  const sidebarSectionRef = useRef<HTMLDivElement>(null);
  const soundsSectionRef = useRef<HTMLDivElement>(null);
  const storageSectionRef = useRef<HTMLDivElement>(null);
  const workspaceSectionRef = useRef<HTMLDivElement>(null);
  const hasRequestedStorageStatsRef = useRef(false);
  const modalTheme = resolveSidebarTheme(draft.sidebarTheme, getSidebarThemeVariant(theme));
  const isModalDarkTheme = getSidebarThemeVariant(modalTheme) === "dark";
  const rememberActiveScrollPosition = () => {
    const viewport = getActiveSettingsModalScrollViewport(dialogContentRef.current);
    if (viewport) {
      rememberedSettingsModalScrollTopByTab[activeTab] = viewport.scrollTop;
    }
  };
  const focusSearchInput = () => {
    if (isFirstLaunchSetup || !isSearchableSettingsModalTab(activeTab)) {
      return;
    }
    searchInputRef.current?.focus({ preventScroll: true });
  };
  const getActiveSearchQuery = () => {
    if (activeTab === "hotkeys") {
      return hotkeysSearchQuery;
    }
    return settingsSearchQuery;
  };
  const setActiveSearchQuery = (nextQuery: string) => {
    if (activeTab === "hotkeys") {
      setHotkeysSearchQuery(nextQuery);
      return;
    }
    setSettingsSearchQuery(nextQuery);
  };
  const handleSettingsModalScrollCapture = (event: ReactUIEvent<HTMLDivElement>) => {
    if (event.target instanceof HTMLElement && event.target.dataset.slot === "scroll-area-viewport") {
      rememberedSettingsModalScrollTopByTab[activeTab] = event.target.scrollTop;
    }
  };
  const handleSettingsModalKeyDownCapture = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (
      event.defaultPrevented ||
      event.nativeEvent.isComposing ||
      event.metaKey ||
      event.ctrlKey ||
      event.altKey ||
      isFirstLaunchSetup ||
      !isSearchableSettingsModalTab(activeTab) ||
      event.key.length !== 1 ||
      isEditableSettingsModalEventTarget(event.target)
    ) {
      return;
    }

    event.preventDefault();
    setActiveSearchQuery(`${getActiveSearchQuery()}${event.key}`);
    requestAnimationFrame(focusSearchInput);
  };
  const setActiveTab = (nextTab: SettingsModalTab) => {
    rememberActiveScrollPosition();
    rememberedSettingsModalTab = nextTab;
    setActiveTabState(nextTab);
  };

  const scrollSettingsSectionIntoView = (sectionRef: RefObject<HTMLDivElement | null>) => {
    sectionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const nextTab = getInitialSettingsModalTab(initialTab);
    rememberActiveScrollPosition();
    rememberedSettingsModalTab = nextTab;
    setActiveTabState(nextTab);
  }, [initialTab, isOpen]);

  useEffect(() => {
    if (!isOpen || isFirstLaunchSetup || !initialSearchQuery?.trim()) {
      return;
    }
    const nextQuery = initialSearchQuery.trim();
    const nextTab = getInitialSettingsModalTab(initialTab);
    /**
     * CDXC:SessionPersistence 2026-06-04-02:52:
     * Titlebar Tips notices can deep-link into Settings by opening a searchable
     * tab and pre-filling the search box with the setting label. Seed the
     * correct tab-specific query instead of typing through the DOM so repeated
     * opens land on the intended control without depending on focus timing.
     */
    if (nextTab === "hotkeys") {
      setHotkeysSearchQuery(nextQuery);
    } else if (nextTab === "settings") {
      setSettingsSearchQuery(nextQuery);
    }
    const animationFrame = requestAnimationFrame(() => {
      searchInputRef.current?.focus({ preventScroll: true });
      searchInputRef.current?.select();
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialSearchQuery, initialTab, isFirstLaunchSetup, isOpen]);

  useEffect(() => {
    if (!isOpen || activeTab !== "osIntegration" || osIntegrationStatus || osIntegrationStatusLoading) {
      return;
    }
    onRequestOSIntegrationStatus?.();
  }, [
    activeTab,
    isOpen,
    onRequestOSIntegrationStatus,
    osIntegrationStatus,
    osIntegrationStatusLoading,
  ]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    /**
     * CDXC:SettingsNavigation 2026-05-26-18:47:
     * During one app session, reopening Settings should return to the same tab
     * and scroll position the user left. Keep that state in module memory so it
     * survives modal remounts but resets naturally when the app restarts.
     *
     * CDXC:SettingsSearch 2026-05-26-18:47:
     * When a searchable Settings tab opens, ordinary typing should enter the
     * active tab's search box even if Radix focus starts on a tab, button, or
     * another non-text control. Text fields and recorders keep their own input.
     */
    const animationFrame = requestAnimationFrame(() => {
      const viewport = getActiveSettingsModalScrollViewport(dialogContentRef.current);
      if (viewport) {
        viewport.scrollTop = rememberedSettingsModalScrollTopByTab[activeTab] ?? 0;
      }
      focusSearchInput();
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, isFirstLaunchSetup, isOpen]);

  useEffect(() => {
    if (!isOpen || activeTab !== "agents" || agentHookStatus || agentHookStatusLoading) {
      return;
    }
    onRequestAgentHookStatus?.();
  }, [activeTab, agentHookStatus, agentHookStatusLoading, isOpen, onRequestAgentHookStatus]);

  useEffect(() => {
    if (!isOpen || activeTab !== "integrations") {
      return;
    }
    /**
     * CDXC:IntegrationsSetup 2026-05-27-04:17:
     * Settings -> Integrations is the single ongoing setup page for CLI,
     * Ghostex Browser Use, agent hooks, Ghostex Computer Use, and macOS permissions.
     * Request machine-local statuses only when the tab opens so Settings does
     * not run filesystem checks while the user is editing unrelated settings.
     *
     * CDXC:ComputerAgentControl 2026-05-27-06:58:
     * Settings should present the public skill names Ghostex Browser Use and
     * Ghostex Computer Use. Desktop Control is ready only when Cua Driver and
     * the `$ghostex-computer-use` skill are both installed.
     */
    if (!agentHookStatus && !agentHookStatusLoading) {
      onRequestAgentHookStatus?.();
    }
    if (!ghostexCliStatus && !ghostexCliStatusLoading) {
      onRequestGhostexCliStatus?.();
    }
  }, [
    activeTab,
    agentHookStatus,
    agentHookStatusLoading,
    ghostexCliStatus,
    ghostexCliStatusLoading,
    isOpen,
    onRequestAgentHookStatus,
    onRequestGhostexCliStatus,
  ]);

  /**
   * CDXC:SettingsSearch 2026-05-04-02:30
   * Settings search must be fuzzy and cover section titles, setting subtitles,
   * and selectable option text so users can find controls by the value they
   * want to choose, not only by the visible setting label.
   */
  const settingsSearch = {
    browser: getSettingsSectionSearch(settingsSearchQuery, "Browser", [
      {
        key: "browserFeedbackTool",
        options: BROWSER_FEEDBACK_TOOL_OPTIONS,
        subtitle: "Choose the feedback tool launched from browser pane menus.",
        title: "Feedback Tool",
      },
    ]),
    editor: getSettingsSectionSearch(settingsSearchQuery, "Editor", [
      {
        key: "defaultEditorCommand",
        options: DEFAULT_EDITOR_COMMAND_OPTIONS,
        subtitle: "Choose the command used when opening files in an external editor.",
        title: "Default editor command",
      },
      ...(draft.defaultEditorCommand === "other"
        ? [
            {
              key: "customDefaultEditorCommand",
              subtitle: "Write a custom editor command for the Other editor option.",
              title: "Custom editor command",
            },
          ]
        : []),
      {
        key: "codeServerLinkVscodeUserConfig",
        subtitle: "Use the VS Code settings from the local VS Code install.",
        title: "Use VS Code settings",
      },
      {
        key: "codeServerUseVscodeInsidersUserConfig",
        subtitle: "Use the VS Code Insiders user settings directory.",
        title: "Use VS Code Insiders settings",
      },
      {
        key: "hideProjectHeaderDiffStats",
        subtitle: "Hide +added/-removed line counts next to project headers.",
        title: "Hide project git stats",
      },
      {
        key: "showProjectEditorDiffFileCount",
        subtitle: "Show changed-file counts in project header git stats.",
        title: "Show editor file count",
      },
      {
        key: "showUntrackedProjectDiffWhenNoTrackedChanges",
        subtitle:
          "When tracked git diff is +0 -0, show untracked line counts in project headers (Starship-style prompts ignore untracked lines).",
        title: "Show untracked lines without tracked changes",
      },
    ]),
    autoSleep: getSettingsSectionSearch(settingsSearchQuery, "Auto Sleep", [
      {
        key: "autoSleepCodeEditorEnabled",
        subtitle: "Sleep inactive VS Code panes after the selected idle period.",
        title: "Sleep inactive VS Code panes",
      },
      {
        key: "autoSleepCodeEditorIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive VS Code panes sleep.",
        title: "VS Code idle time",
      },
      {
        key: "autoSleepGitEditorEnabled",
        subtitle: "Sleep inactive Git panes after the selected idle period.",
        title: "Sleep inactive Git panes",
      },
      {
        key: "autoSleepGitEditorIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive Git panes sleep.",
        title: "Git idle time",
      },
      {
        key: "autoSleepProjectEditorEnabled",
        subtitle: "Sleep inactive Project panes after the selected idle period.",
        title: "Sleep inactive Project panes",
      },
      {
        key: "autoSleepProjectEditorIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive Project panes sleep.",
        title: "Project idle time",
      },
      {
        key: "autoSleepBrowserSessionsEnabled",
        subtitle: "Sleep inactive browser panes after the selected idle period.",
        title: "Sleep inactive browser panes",
      },
      {
        key: "autoSleepBrowserIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before inactive browser panes sleep.",
        title: "Browser idle time",
      },
      {
        key: "autoSleepAgentSessionsEnabled",
        subtitle: "Sleep idle agent terminal sessions automatically.",
        title: "Sleep idle agent sessions",
      },
      {
        key: "autoSleepAgentIdleMinutes",
        options: AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Idle time before eligible agent terminals sleep.",
        title: "Agent idle time",
      },
      {
        key: "autoSleepRequireAgentResumeCommand",
        subtitle: "Only auto-sleep agent sessions Ghostex can wake with a resume command.",
        title: "Require resume command",
      },
      {
        key: "autoSleepFavoriteAgentSessions",
        subtitle: "Allow favorite agent sessions to auto-sleep.",
        title: "Include favorite agents",
      },
    ]),
    power: getSettingsSectionSearch(settingsSearchQuery, "Power", [
      {
        key: "hideKeepAwakeTitlebarControl",
        subtitle: "Hide the keep-awake control from the title bar.",
        title: "Hide title-bar keep-awake control",
      },
      {
        key: "keepAwakeDefaultDurationMinutes",
        options: KEEP_AWAKE_DURATION_OPTIONS.map((option) => ({
          label: option.label,
          value: String(option.value),
        })),
        subtitle: "Choose the duration used by the title-bar keep-awake button.",
        title: "Default keep-awake duration",
      },
      {
        key: "keepAwakeAllowDisplaySleep",
        subtitle: "Keep the Mac awake but allow the display to turn off.",
        title: "Allow display sleep",
      },
      {
        key: "keepAwakePreventLidSleep",
        subtitle:
          "Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session.",
        title: "Prevent lid-close sleep",
      },
      {
        key: "keepAwakeActivateOnLaunch",
        subtitle: "Start preventing sleep when Ghostex launches.",
        title: "Activate on launch",
      },
      {
        key: "keepAwakeActivateOnExternalDisplay",
        subtitle: "Start preventing sleep when an external display is connected.",
        title: "Activate on external display",
      },
      {
        key: "keepAwakeDeactivateBelowBatteryThreshold",
        subtitle: "Stop preventing sleep when battery capacity drops below the threshold.",
        title: "Deactivate below battery threshold",
      },
      {
        key: "keepAwakeBatteryThresholdPercent",
        subtitle: "Battery percentage used by the threshold rule.",
        title: "Battery threshold",
      },
      {
        key: "keepAwakeDeactivateOnLowPowerMode",
        subtitle: "Stop preventing sleep when macOS Low Power Mode is enabled.",
        title: "Deactivate in Low Power Mode",
      },
      {
        key: "keepAwakeDeactivateOnUserSwitch",
        subtitle: "Stop preventing sleep when this user session is no longer active.",
        title: "Deactivate on user switch",
      },
    ]),
    sessionCards: getSettingsSectionSearch(settingsSearchQuery, "Session Cards", [
      {
        key: "hideSessionAgentIconUntilHover",
        subtitle: "Hide session agent icons until a session row is hovered.",
        title: "Hide agent icon until hover",
      },
      {
        key: "hideBrowserFaviconUntilHover",
        subtitle: "Hide browser page favicons until a session row is hovered.",
        title: "Hide browser favicon until hover",
      },
      {
        key: "showCloseButtonOnSessionCards",
        subtitle: "Reveal the close control when hovering a card.",
        title: "Show close button on hover",
      },
      /*
       * CDXC:SidebarSessions 2026-05-15-19:46:
       * Settings must not expose the card-hotkey visibility row; session-card shortcut visibility is no longer configurable from the modal.
       */
      {
        key: "hideLastActiveTimeOnSessionCards",
        subtitle: "Hide Last Active timestamps from session-card title rows.",
        title: "Hide last active time",
      },
      {
        key: "showSessionCloseContextMenuAction",
        subtitle: "Show the Close item in session context menus.",
        title: "Show Close option in context menu",
      },
      {
        key: "showSessionCommandCopyActions",
        subtitle: "Show Copy resume and Copy attach command in session context menus.",
        title: "Show command copy actions",
      },
      {
        key: "showSessionDetailsCopyAction",
        subtitle: "Show Copy details in session context menus.",
        title: "Show Copy details option",
      },
    ]),
    statusIndicators: getSettingsSectionSearch(settingsSearchQuery, "Status Indicators", [
      /*
       * CDXC:StatusIndicators 2026-05-20-12:00:
       * hide* settings stay persisted as hide flags, but Settings presents them
       * as Show toggles so ON means the indicator surface is visible.
       */
      {
        key: "hideFloatingSessionStatusIndicators",
        subtitle: "Show the desktop floating session status badges.",
        title: "Show Floating Session Indicators",
      },
      {
        key: "hideMenuBarSessionStatusIndicators",
        subtitle: "Show the menu bar session status badges.",
        title: "Show Menu Bar Session Indicators",
      },
      {
        key: "petOverlayEnabled",
        subtitle: "Show the draggable animated pet in the native sidebar.",
        title: "Wake Pet",
      },
      {
        key: "selectedPetId",
        options: PET_OPTIONS.map((option) => ({ label: option.displayName, value: option.id })),
        subtitle: "Choose the pet sprite.",
        title: "Pet",
      },
    ]),
    sidebar: getSettingsSectionSearch(settingsSearchQuery, "Sidebar", [
      {
        key: "sidebarSettingsPreset",
        options: [
          ...SIDEBAR_SETTINGS_PRESETS.map((preset) => ({
            label: preset.label,
            value: preset.id,
          })),
          { label: "Custom", value: "custom" },
        ],
        subtitle: "Apply a sidebar UI preset or show Custom when controlled settings diverge.",
        title: "Preset",
      },
      {
        key: "sidebarSide",
        options: SIDEBAR_SIDE_OPTIONS,
        subtitle: "Choose which side of the screen holds the sidebar.",
        title: "Side",
      },
      {
        key: "sidebarDefaultWidthPx",
        subtitle: "Width restored when double-clicking the sidebar resize handle.",
        title: "Default Width",
      },
      {
        key: "projectSessionListCollapsedCount",
        subtitle: "Number of project sessions kept visible after Show less.",
        title: "Show Less Count",
      },
      {
        key: "sidebarTheme",
        options: SIDEBAR_THEME_SETTING_OPTIONS,
        subtitle: "Choose the sidebar color scheme.",
        title: "Theme",
      },
      {
        key: "sessionStatusIndicatorSize",
        options: SESSION_STATUS_INDICATOR_SIZE_OPTIONS,
        subtitle: "Scale the floating session status indicator.",
        title: "Floating Session Indicator Size",
      },
      {
        key: "agentManagerZoomPercent",
        subtitle: "Scale the agent manager UI.",
        title: "Agent Manager Zoom",
      },
      {
        key: "createSessionOnSidebarDoubleClick",
        subtitle: "Create a session from empty sidebar space.",
        title: "Double-click empty sidebar space to create a session",
      },
      {
        key: "renameSessionOnDoubleClick",
        subtitle: "Rename sessions directly from their cards.",
        title: "Double-click session cards to rename",
      },
    ]),
    sounds: getSettingsSectionSearch(settingsSearchQuery, "Sounds", [
      {
        key: "completionBellEnabled",
        subtitle: "Play a completion sound when work finishes.",
        title: "Enable completion bell",
      },
      {
        key: "completionSound",
        options: COMPLETION_SOUND_OPTIONS,
        subtitle: "Sound for terminal completions.",
        title: "Completion Sound",
      },
      {
        key: "showMacOSAttentionNotifications",
        subtitle: "Show a macOS banner when a session needs attention.",
        title: "macOS Attention Notifications",
      },
      {
        key: "attentionNotificationActions",
        subtitle: "Test the current completion alert settings or open macOS Notification Settings.",
        title: "Agent Completion Alert Test",
      },
      {
        key: "actionCompletionSound",
        options: COMPLETION_SOUND_OPTIONS,
        subtitle: "Sound for action completions.",
        title: "Action Completion Sound",
      },
    ]),
    storage: getSettingsSectionSearch(settingsSearchQuery, "Storage", [
      {
        key: "ghostexFolderStats",
        options: [
          { label: "Open Ghostex folder", value: "openGhostexFolder" },
          { label: "Folder sizes", value: "folderSizes" },
          { label: "Disk usage", value: "diskUsage" },
        ],
        subtitle: "Show ~/.ghostex folder sizes and open the Ghostex storage folder.",
        title: "Ghostex folder",
      },
    ]),
    terminal: getSettingsSectionSearch(settingsSearchQuery, "Terminal", [
      {
        key: "ghosttySettingsActions",
        options: [
          { label: "Apply recommended", value: "applyRecommendedGhosttySettings" },
          { label: "Open Ghostty config", value: "openGhosttyConfigFile" },
          { label: "Open Ghostty docs", value: "openGhosttySettingsDocs" },
          { label: "Reset Ghostty defaults", value: "resetGhosttySettingsToDefault" },
        ],
        subtitle:
          "Recommended Ghostty settings, Ghostty config file, Ghostty docs, and Ghostty defaults.",
        title: "Ghostty settings actions",
      },
      {
        key: "terminalGhosttyTheme",
        options: GHOSTTY_THEME_SETTING_OPTIONS,
        subtitle: "Choose a bundled Ghostty theme or leave the config unmanaged.",
        title: "Theme",
      },
      {
        key: "terminalFontFamily",
        subtitle: "Type a Ghostty font-family name.",
        title: "Font Family",
      },
      {
        key: "terminalFontSize",
        subtitle: "Set terminal text size.",
        title: "Font Size",
      },
      {
        key: "terminalFontWeight",
        subtitle: "Set terminal text weight.",
        title: "Font Weight",
      },
      {
        key: "terminalLineHeight",
        subtitle: "Adjust terminal row height.",
        title: "Line Height",
      },
      {
        key: "terminalLetterSpacing",
        subtitle: "Adjust spacing between glyphs.",
        title: "Letter Spacing",
      },
      {
        key: "terminalCursorStyle",
        options: [
          { label: "Line", value: "bar" },
          { label: "Block", value: "block" },
          { label: "Underline", value: "underline" },
        ],
        subtitle: "Choose the cursor shape.",
        title: "Cursor Style",
      },
      {
        key: "terminalCursorStyleBlink",
        subtitle: "Blink the terminal cursor.",
        title: "Cursor blink",
      },
      {
        key: "sessionPersistenceProvider",
        options: SESSION_PERSISTENCE_PROVIDER_OPTIONS,
        subtitle:
          "Choose whether new terminal and agent sessions should use zmx persistence.",
        title: "Session Persistence",
      },
      ...(draft.sessionPersistenceProvider === "off"
        ? []
        : [
            {
              key: "showSessionIdInTerminalPanes",
              subtitle: "Show the provider session id in the top-right corner of terminal panes.",
              title: "Show session id in terminal panes",
            },
          ]),
      {
        key: "promptEditorBackend",
        options: [...PROMPT_EDITOR_BACKEND_OPTIONS, { label: "Install gte", value: "installGte" }],
        subtitle:
          "Choose which editor Ctrl+G uses when a terminal prompt asks for $EDITOR.",
        title: "Ctrl+G prompt editor",
      },
      ...(draft.promptEditorBackend === "custom"
        ? [
            {
              key: "customPromptEditorCommand",
              subtitle: "Write a custom $EDITOR command for Ctrl+G prompt editing.",
              title: "Custom Ctrl+G editor command",
            },
          ]
        : []),
    ]),
    terminalBehavior: getSettingsSectionSearch(settingsSearchQuery, "Terminal Behavior", [
      {
        key: "terminalScrollbackLimitMb",
        subtitle: "Set scrollback memory per terminal surface.",
        title: "Scrollback limit",
      },
      {
        key: "terminalCopyOnSelect",
        options: GHOSTTY_COPY_ON_SELECT_OPTIONS,
        subtitle: "Copy selected terminal text automatically.",
        title: "Copy on select",
      },
      {
        key: "terminalConfirmCloseSurface",
        options: GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
        subtitle: "Confirm before closing terminal surfaces.",
        title: "Confirm close",
      },
      {
        key: "terminalClipboardTrimTrailingSpaces",
        subtitle: "Trim trailing whitespace when copying terminal text.",
        title: "Trim trailing spaces on copy",
      },
      {
        key: "terminalClipboardPasteProtection",
        subtitle: "Ask before pasting text Ghostty considers unsafe.",
        title: "Paste protection",
      },
      {
        key: "terminalPastePreviewableImages",
        subtitle: PASTE_PREVIEWABLE_IMAGES_DESCRIPTION,
        title: "Paste previewable images",
      },
      {
        key: "terminalMouseHideWhileTyping",
        subtitle: "Hide the pointer while typing in the terminal.",
        title: "Hide mouse while typing",
      },
      {
        key: "terminalScrollbar",
        options: GHOSTTY_SCROLLBAR_OPTIONS,
        subtitle: "Control whether Ghostty shows its native scrollback scrollbar.",
        title: "Scrollbar",
      },
    ]),
    terminalScrolling: getSettingsSectionSearch(settingsSearchQuery, "Terminal Scrolling", [
      {
        key: "terminalMouseScrollMultiplierPrecision",
        subtitle: "Trackpads and high-resolution scroll wheels. Ghostty default is 1.",
        title: "Precision scroll multiplier",
      },
      {
        key: "terminalMouseScrollMultiplierDiscrete",
        subtitle: "Traditional notched mouse wheels. Ghostty default is 3.",
        title: "Discrete scroll multiplier",
      },
      {
        key: "terminalScrollToBottomWhenTyping",
        subtitle: "Keep the prompt visible while typing.",
        title: "Scroll to bottom when typing",
      },
    ]),
    workspace: getSettingsSectionSearch(settingsSearchQuery, "Workspace", [
      {
        key: "workspaceActivePaneBorderColor",
        subtitle: "CSS color for the focused pane border.",
        title: "Active Pane Border",
      },
      {
        key: "workspaceBackgroundColor",
        subtitle: "Color shown behind terminal panes.",
        title: "Terminal Background",
      },
      {
        key: "clickToWakeSleepingSessions",
        subtitle: "Select sleeping pane tabs without waking them until the empty pane is clicked.",
        title: "Click to Wake Sleeping Panes",
      },
      {
        key: "commandsPanelDefaultHeightPx",
        subtitle: "Height used when opening the command pane and when double-clicking its top resize rail.",
        title: "Command Pane Default Height",
      },
      /*
      CDXC:DiagnosticsSettings 2026-06-06-07:09:
      Debugging Mode is both diagnostic disk logging and debug UI exposure. The setting label and searchable subtitle must warn that detailed app logs can affect performance so users disable it after a repro.
      */
      {
        key: "debuggingMode",
        subtitle: "Show debugging controls and write detailed app diagnostics to disk. This can affect performance, so turn it off when you do not need it.",
        title: "Debug logging and UI",
      },
    ]),
  };
  const mainSettingsSectionNavigation: Array<{
    id: MainSettingsSectionId;
    ref: RefObject<HTMLDivElement | null>;
    searchResult: SettingsSectionSearchResult;
    title: string;
  }> = [
    { id: "sidebar", ref: sidebarSectionRef, searchResult: settingsSearch.sidebar, title: "Sidebar" },
    {
      id: "statusIndicators",
      ref: statusIndicatorsSectionRef,
      searchResult: settingsSearch.statusIndicators,
      title: "Status Indicators",
    },
    { id: "browser", ref: browserSectionRef, searchResult: settingsSearch.browser, title: "Browser" },
    {
      id: "sessionCards",
      ref: sessionCardsSectionRef,
      searchResult: settingsSearch.sessionCards,
      title: "Session Cards",
    },
    {
      id: "workspace",
      ref: workspaceSectionRef,
      searchResult: settingsSearch.workspace,
      title: "Workspace",
    },
    /*
     * CDXC:SettingsNavigation 2026-06-12-04:13:
     * Ghostty terminal controls belong on the main Settings page so one search query can find app settings and terminal settings together.
     */
    { id: "terminal", ref: ghosttyTerminalSectionRef, searchResult: settingsSearch.terminal, title: "Terminal" },
    {
      id: "terminalBehavior",
      ref: ghosttyBehaviorSectionRef,
      searchResult: settingsSearch.terminalBehavior,
      title: "Terminal Behavior",
    },
    {
      id: "terminalScrolling",
      ref: ghosttyScrollingSectionRef,
      searchResult: settingsSearch.terminalScrolling,
      title: "Terminal Scrolling",
    },
    { id: "editor", ref: editorSectionRef, searchResult: settingsSearch.editor, title: "Editor" },
    {
      id: "autoSleep",
      ref: autoSleepSectionRef,
      searchResult: settingsSearch.autoSleep,
      title: "Auto Sleep",
    },
    { id: "power", ref: powerSectionRef, searchResult: settingsSearch.power, title: "Power" },
    { id: "sounds", ref: soundsSectionRef, searchResult: settingsSearch.sounds, title: "Sounds" },
    { id: "storage", ref: storageSectionRef, searchResult: settingsSearch.storage, title: "Storage" },
  ];
  const hasVisibleMainSettings = mainSettingsSectionNavigation.some((section) =>
    hasVisibleSettingsSearchResult(section.searchResult),
  );
  const visibleFirstLaunchMainSettings =
    firstLaunchSetupVisibleSettings ?? FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS;
  const mainSettingVisible = (
    sectionResult: SettingsSectionSearchResult,
    settingKey: string,
  ) => {
    if (isFirstLaunchSetup) {
      return isFirstLaunchSetupMainSettingVisible(
        settingKey as FirstLaunchSetupMainSettingKey,
        visibleFirstLaunchMainSettings,
      );
    }
    return shouldShowSetting(sectionResult, settingKey);
  };
  const mainSectionVisible = (
    sectionId: MainSettingsSectionId,
    sectionResult: SettingsSectionSearchResult,
  ) => {
    if (isFirstLaunchSetup) {
      return MAIN_SETTINGS_SECTION_SETTING_KEYS[sectionId].some((settingKey) =>
        isFirstLaunchSetupMainSettingVisible(settingKey, visibleFirstLaunchMainSettings),
      );
    }
    return shouldShowSettingsSection(sectionResult);
  };

  useEffect(() => {
    if (!isOpen || activeTab !== "settings" || initialSection === undefined) {
      return;
    }
    /**
     * CDXC:SettingsNavigation 2026-05-27-07:32:
     * Titlebar entry points such as Power Settings should land on the matching
     * Settings section, not only open the modal at the previously remembered
     * scroll position.
     */
    const targetSectionRef = getMainSettingsSectionRef(initialSection, {
      autoSleep: autoSleepSectionRef,
      browser: browserSectionRef,
      editor: editorSectionRef,
      power: powerSectionRef,
      sessionCards: sessionCardsSectionRef,
      sidebar: sidebarSectionRef,
      sounds: soundsSectionRef,
	      statusIndicators: statusIndicatorsSectionRef,
	      storage: storageSectionRef,
	      terminal: ghosttyTerminalSectionRef,
	      terminalBehavior: ghosttyBehaviorSectionRef,
	      terminalScrolling: ghosttyScrollingSectionRef,
	      workspace: workspaceSectionRef,
	      agents: agentsOnboardingSectionRef,
	    });
    const animationFrame = requestAnimationFrame(() => {
      targetSectionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, initialSection, isOpen]);
  useEffect(() => {
    if (!isOpen) {
      hasRequestedStorageStatsRef.current = false;
      return;
    }
    if (isFirstLaunchSetup) {
      setActiveTabState("settings");
    }
    /**
     * CDXC:SettingsTabs 2026-05-13-16:05
     * Saving a control in Hotkeys, Agents, Actions, or Open In updates
     * the incoming settings prop. That prop sync must not reset the selected
     * tab; tab changes are owned by explicit navigation and initial open state.
     *
     * CDXC:SettingsNavigation 2026-06-12-04:13:
     * Ghostty terminal controls now save from the main Settings tab, so the tab
     * sync rule no longer treats Ghostty as a separate navigation target.
     */
    setDraft(normalizeghostexSettings(settings));
  }, [isFirstLaunchSetup, isOpen, settings]);

  useEffect(() => {
    if (
      !isOpen ||
      activeTab !== "settings" ||
      ghostexFolderStats ||
      ghostexFolderStatsLoading ||
      !onRequestGhostexFolderStats ||
      hasRequestedStorageStatsRef.current
    ) {
      return;
    }
    const sectionElement = storageSectionRef.current;
    if (!sectionElement) {
      return;
    }

    const requestStats = () => {
      hasRequestedStorageStatsRef.current = true;
      onRequestGhostexFolderStats();
    };

    /**
     * CDXC:SettingsStorage 2026-05-09-15:25
     * Folder-size scans can touch many files, so Settings waits until the
     * bottom storage card is near the viewport before asking native for stats.
     */
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          requestStats();
          observer.disconnect();
        }
      },
      { rootMargin: "96px 0px" },
    );
    observer.observe(sectionElement);
    return () => observer.disconnect();
  }, [
    activeTab,
    isOpen,
    onRequestGhostexFolderStats,
    settingsSearchQuery,
    ghostexFolderStats,
    ghostexFolderStatsLoading,
  ]);

  useEffect(() => {
    return () => {
      if (pendingTimeoutRef.current) {
        clearTimeout(pendingTimeoutRef.current);
      }
    };
  }, []);

  const clearPendingSettings = () => {
    if (pendingTimeoutRef.current) {
      clearTimeout(pendingTimeoutRef.current);
      pendingTimeoutRef.current = undefined;
    }
  };

  const flushPendingSettings = () => {
    clearPendingSettings();
    const pendingSettings = pendingSettingsRef.current;
    pendingSettingsRef.current = undefined;
    if (pendingSettings) {
      onChange(pendingSettings);
    }
  };

  const closeSettingsModal = () => {
    rememberActiveScrollPosition();
    flushPendingSettings();
    onClose();
  };

  const applySettings = (nextSettings: ghostexSettings) => {
    const normalizedSettings = normalizeghostexSettings(nextSettings);
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    setDraft(normalizedSettings);
    onChange(normalizedSettings);
  };

  /**
   * CDXC:Settings 2026-04-26-11:13: Numeric settings use sliders with adjacent
   * number boxes. Dragging or typing updates the visible value immediately, but
   * persists through a short trailing debounce to avoid flooding settings writes.
   * Number boxes keep local edit text so partial values can be typed cleanly.
   */
  const applySettingsDebounced = (nextSettings: ghostexSettings) => {
    const normalizedSettings = normalizeghostexSettings(nextSettings);
    pendingSettingsRef.current = normalizedSettings;
    setDraft(normalizedSettings);
    clearPendingSettings();
    pendingTimeoutRef.current = setTimeout(() => {
      const pendingSettings = pendingSettingsRef.current;
      pendingSettingsRef.current = undefined;
      pendingTimeoutRef.current = undefined;
      if (pendingSettings) {
        onChange(pendingSettings);
      }
    }, NUMERIC_SETTINGS_DEBOUNCE_MS);
  };

  /**
   * CDXC:Settings 2026-04-26-10:12: Settings changes must apply immediately.
   * The settings dialog keeps local state only for responsive controls, then
   * posts every normalized change instead of waiting for Save/Cancel actions.
   */
  const updateDraft = <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => {
    applySettings({ ...(pendingSettingsRef.current ?? draft), [key]: value });
  };
  const updateDraftDebounced = <Key extends keyof ghostexSettings>(
    key: Key,
    value: ghostexSettings[Key],
  ) => {
    applySettingsDebounced({ ...(pendingSettingsRef.current ?? draft), [key]: value });
  };
  const activeSidebarSettingsPresetId = getSidebarSettingsPresetId(
    pendingSettingsRef.current ?? draft,
  );
  const updateSidebarSettingsPreset = (presetId: SidebarSettingsPresetId) => {
    applySettings(applySidebarSettingsPreset(pendingSettingsRef.current ?? draft, presetId));
  };

  const resetSettings = () => applySettings(DEFAULT_ghostex_SETTINGS);
  const resetSetting = <Key extends keyof ghostexSettings>(key: Key) => {
    applySettings({
      ...(pendingSettingsRef.current ?? draft),
      [key]: DEFAULT_ghostex_SETTINGS[key],
    });
  };
  const getSettingModificationProps = <Key extends keyof ghostexSettings>(
    key: Key,
  ): Required<SettingModificationProps> => ({
    isModified: !Object.is(
      (pendingSettingsRef.current ?? draft)[key],
      DEFAULT_ghostex_SETTINGS[key],
    ),
    onResetToDefault: () => resetSetting(key),
  });

  const applyRecommendedGhosttySettings = () => {
    /**
     * CDXC:GhosttySettings 2026-04-30-01:48
     * The recommended Ghostty button must update both the visible ghostex controls
     * and the real Ghostty config keys that are not modeled in ghostex settings.
     */
    applySettings({
      ...draft,
      terminalCursorStyle: "bar",
      terminalFontFamily: "JetBrains Mono",
      terminalFontSize: 13,
      terminalFontWeight: 400,
      terminalLetterSpacing: 0,
      terminalLineHeight: 1.2,
      terminalMouseScrollMultiplierDiscrete: 1,
      terminalMouseScrollMultiplierPrecision: 1,
    });
    onGhosttySettingsAction?.("applyRecommendedGhosttySettings");
  };

  const resetGhosttySettingsToDefault = () => {
    /**
     * CDXC:GhosttySettings 2026-04-30-01:48
     * Resetting Ghostty defaults should also move the visible terminal
     * controls back to ghostex defaults, then remove managed keys from the real
     * Ghostty config so Ghostty's own defaults take effect.
     */
    applySettings({
      ...draft,
      terminalCursorStyle: DEFAULT_ghostex_SETTINGS.terminalCursorStyle,
      terminalFontFamily: DEFAULT_ghostex_SETTINGS.terminalFontFamily,
      terminalFontSize: DEFAULT_ghostex_SETTINGS.terminalFontSize,
      terminalFontWeight: DEFAULT_ghostex_SETTINGS.terminalFontWeight,
      terminalLetterSpacing: DEFAULT_ghostex_SETTINGS.terminalLetterSpacing,
      terminalLineHeight: DEFAULT_ghostex_SETTINGS.terminalLineHeight,
      terminalMouseScrollMultiplierDiscrete:
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierDiscrete,
      terminalMouseScrollMultiplierPrecision:
        DEFAULT_ghostex_SETTINGS.terminalMouseScrollMultiplierPrecision,
      terminalScrollToBottomWhenTyping: DEFAULT_ghostex_SETTINGS.terminalScrollToBottomWhenTyping,
    });
    onGhosttySettingsAction?.("resetGhosttySettingsToDefault");
  };

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          rememberActiveScrollPosition();
          flushPendingSettings();
          onClose();
        }
      }}
      open={isOpen}
    >
      <DialogContent
        className={cn(
          "ghostex-settings-shadcn settings-modal-dialog flex flex-col gap-0 overflow-hidden p-0 font-sans",
          isModalDarkTheme && "dark",
        )}
        data-sidebar-theme={modalTheme}
        onKeyDownCapture={handleSettingsModalKeyDownCapture}
        onEscapeKeyDown={(event) => {
          if (hasActiveHotkeyRecorder()) {
            event.preventDefault();
          }
        }}
        onOpenAutoFocus={(event) => {
          if (!isFirstLaunchSetup && isSearchableSettingsModalTab(activeTab)) {
            event.preventDefault();
            requestAnimationFrame(focusSearchInput);
          }
        }}
        onScrollCapture={handleSettingsModalScrollCapture}
        ref={dialogContentRef}
        showCloseButton={false}
      >
        <button
          aria-label="Close Settings"
          className="ghostex-modal-icon-close"
          onClick={closeSettingsModal}
          type="button"
        >
          <IconX aria-hidden="true" />
        </button>
        <TooltipProvider delayDuration={300}>
          <Tabs
            className="flex min-h-0 flex-1 flex-col"
            onValueChange={(value) => setActiveTab(value as SettingsModalTab)}
            value={activeTab}
          >
          <DialogHeader className="ghostex-modal-heading-bar">
            <DialogTitle className="ghostex-modal-heading-title">
              {isFirstLaunchSetup ? "Get started" : "Settings"}
            </DialogTitle>
            {isFirstLaunchSetup ? (
              <p className="mt-2 text-sm text-muted-foreground">
                Choose a few defaults for Ghostex. You can change everything later in Settings.
              </p>
            ) : null}
            {/*
             * CDXC:UnifiedSettings 2026-05-09-15:30
             * Settings is the single configuration surface for app controls,
             * terminal controls, Agents, Actions, Open In, and Hotkeys.
             *
             * CDXC:SettingsNavigation 2026-06-12-04:13:
             * Ghostty terminal settings are merged into the main Settings page
             * so one Settings search covers app settings and terminal settings.
             */}
            {!isFirstLaunchSetup ? (
            <div className="settings-modal-tabs-scroll mt-3">
              <TabsList>
                <TabsTrigger value="settings">Settings</TabsTrigger>
                <TabsTrigger value="osIntegration">OS Integration</TabsTrigger>
                <TabsTrigger value="integrations">Integrations</TabsTrigger>
                <TabsTrigger value="remote">Remote</TabsTrigger>
                <TabsTrigger value="projects">Projects</TabsTrigger>
                <TabsTrigger value="hotkeys">Hotkeys</TabsTrigger>
                <TabsTrigger value="agents">Agents</TabsTrigger>
                <TabsTrigger value="actions">Actions</TabsTrigger>
                <TabsTrigger value="openTargets">Open In</TabsTrigger>
              </TabsList>
            </div>
            ) : null}
            {!isFirstLaunchSetup && (activeTab === "settings" || activeTab === "hotkeys") ? (
              <SidebarSessionSearchField
                ariaLabel={
                  activeTab === "hotkeys"
                    ? "Search hotkeys"
                    : "Search settings"
                }
                clearLabel={
                  activeTab === "hotkeys"
                    ? "Clear hotkeys search"
                    : "Clear settings search"
                }
                inputClassName="settings-modal-search-input"
                inputRef={searchInputRef}
                placeholder={activeTab === "hotkeys" ? "Search hotkeys" : "Search settings"}
                query={activeTab === "hotkeys" ? hotkeysSearchQuery : settingsSearchQuery}
                setQuery={(nextQuery) => {
                  if (activeTab === "hotkeys") {
                    setHotkeysSearchQuery(nextQuery);
                    return;
                  }
                  setSettingsSearchQuery(nextQuery);
                }}
                toolbarClassName="settings-modal-search-toolbar"
              />
            ) : null}
          </DialogHeader>

          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="settings">
          {/* CDXC:Settings 2026-04-26-10:43: The settings dialog lives inside a
              narrow sidebar webview, so the Radix scroll area needs an explicit
              height instead of letting Dialog crop an auto-height viewport. */}
          {/* CDXC:UnifiedSettings 2026-05-09-17:08: The Settings dialog is now a
              tabbed surface with variable header height. The active tab owns
              the remaining vertical space so the dialog never clips the bottom
              of a fixed-height scroll area. */}
          {/* CDXC:SettingsNavigation 2026-05-13-08:05
              The main Settings tab uses a left section navigator inside the
              widened modal so long configuration groups remain directly
              reachable.

              CDXC:SettingsNavigation 2026-06-12-04:13:
              Terminal sections share this navigator with app settings so search
              and section jumps operate on one main Settings page. */}
          <div className="settings-main-tab-layout">
            <aside aria-label="Settings sections" className="settings-section-sidebar">
              {(isFirstLaunchSetup
                ? [
                    { id: "agents" as const, ref: agentsOnboardingSectionRef, title: "Agents" },
                    ...mainSettingsSectionNavigation,
                  ]
                : mainSettingsSectionNavigation
              )
                .filter((section) =>
                  section.id === "agents"
                    ? mainSectionVisible("agents", settingsSearch.sidebar)
                    : mainSectionVisible(section.id, section.searchResult),
                )
                .map((section) => (
                  <Button
                    className="settings-section-sidebar-button"
                    key={section.id}
                    onClick={() => scrollSettingsSectionIntoView(section.ref)}
                    type="button"
                    variant="ghost"
                  >
                    {section.title}
                  </Button>
                ))}
            </aside>
          <ScrollArea className="h-full min-h-0">
          <div className="flex flex-col gap-6 px-5 pb-5">
            {isFirstLaunchSetup && mainSectionVisible("agents", settingsSearch.sidebar) ? (
              <SettingsSection sectionRef={agentsOnboardingSectionRef} title="Agents">
                {mainSettingVisible(settingsSearch.sidebar, "agentAcceptAllEnabled") ? (
                  <ToggleField
                    checked={draft.agentAcceptAllEnabled}
                    description="Enable each supported agent's permission-bypass mode when launching sessions. Per-agent overrides live in Settings → Agents."
                    label="Accept All"
                    {...getSettingModificationProps("agentAcceptAllEnabled")}
                    onChange={(checked) => updateDraft("agentAcceptAllEnabled", checked)}
                  />
                ) : null}
              </SettingsSection>
            ) : null}
            {mainSectionVisible("sidebar", settingsSearch.sidebar) ? (
              <SettingsSection sectionRef={sidebarSectionRef} title="Sidebar">
              {/* CDXC:SidebarSettingsPresets 2026-06-12-07:10: Preset is the first Sidebar setting so users can apply Codex, Minimal, Detailed, or Recommended sidebar UI defaults before tuning individual controlled settings. */}
              {mainSettingVisible(settingsSearch.sidebar, "sidebarSettingsPreset") ? (
              <SidebarPresetField
                activePresetId={activeSidebarSettingsPresetId}
                description="Apply a sidebar UI preset."
                isModified={activeSidebarSettingsPresetId !== "codex"}
                label="Preset"
                onChange={updateSidebarSettingsPreset}
                onResetToDefault={() => updateSidebarSettingsPreset("codex")}
              />
              ) : null}
              {/* CDXC:SidebarPlacement 2026-05-06-17:32: Sidebar side remains
                  near the top of Sidebar settings so users can move the
                  sidebar to the right side without discovering the hotkey. */}
              {mainSettingVisible(settingsSearch.sidebar, "sidebarSide") ? (
              <SelectField
                description="Choose which side of the screen holds the sidebar."
                label="Side"
                {...getSettingModificationProps("sidebarSide")}
                onChange={(value) => updateDraft("sidebarSide", value as SidebarSide)}
                options={SIDEBAR_SIDE_OPTIONS}
                value={draft.sidebarSide}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "sidebarDefaultWidthPx") ? (
              <>
                {/*
                 * CDXC:SidebarChrome 2026-06-05-04:40:
                 * This setting changes only the explicit double-click reset target for the sidebar resize handle. App restart must keep restoring the last persisted sidebar width from native/Electron chrome state.
                 */}
                <SliderNumberField
                  description="Used when double-clicking the sidebar resize handle. App restart still restores your last manually set sidebar width."
                  label="Default Width"
                  {...getSettingModificationProps("sidebarDefaultWidthPx")}
                  max={MAX_SIDEBAR_DEFAULT_WIDTH_PX}
                  min={MIN_SIDEBAR_DEFAULT_WIDTH_PX}
                  onCommit={(value) => updateDraft("sidebarDefaultWidthPx", value)}
                  onChange={(value) => updateDraftDebounced("sidebarDefaultWidthPx", value)}
                  step={1}
                  value={draft.sidebarDefaultWidthPx}
                />
              </>
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "projectSessionListCollapsedCount") ? (
              <>
                {/*
                 * CDXC:ProjectSessionLists 2026-06-10-13:39:
                 * The project-header Show less button should preserve the old six-row default while letting users raise the collapsed project-session count, such as ten rows, without changing the per-project Show more / Show less state model.
                 */}
                <SliderNumberField
                  description="Project sessions kept visible after Show less."
                  label="Show Less Count"
                  {...getSettingModificationProps("projectSessionListCollapsedCount")}
                  max={MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT}
                  min={MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT}
                  onCommit={(value) => updateDraft("projectSessionListCollapsedCount", value)}
                  onChange={(value) =>
                    updateDraftDebounced("projectSessionListCollapsedCount", value)
                  }
                  step={1}
                  value={draft.projectSessionListCollapsedCount}
                />
              </>
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "sidebarTheme") ? (
              <StaticNoteField
                description="Dark Gray is active. Themes are coming back soon."
                label="Theme"
              />
              ) : null}
              {/* CDXC:SessionStatusIndicators 2026-05-07-18:20: The floating
                  AppKit indicator size is a Sidebar setting because it controls
                  sidebar-owned session navigation chrome outside the webview. */}
              {mainSettingVisible(settingsSearch.sidebar, "sessionStatusIndicatorSize") ? (
              <SelectField
                description="Scale the floating session status indicator."
                label="Floating Session Indicator Size"
                {...getSettingModificationProps("sessionStatusIndicatorSize")}
                onChange={(value) =>
                  updateDraft("sessionStatusIndicatorSize", value as SessionStatusIndicatorSize)
                }
                options={SESSION_STATUS_INDICATOR_SIZE_OPTIONS}
                value={draft.sessionStatusIndicatorSize}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "agentManagerZoomPercent") ? (
              <SliderNumberField
                description="Scale the agent manager UI."
                label="Agent Manager Zoom"
                {...getSettingModificationProps("agentManagerZoomPercent")}
                max={200}
                min={50}
                onCommit={(value) => updateDraft("agentManagerZoomPercent", value)}
                onChange={(value) => updateDraftDebounced("agentManagerZoomPercent", value)}
                step={1}
                value={draft.agentManagerZoomPercent}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "createSessionOnSidebarDoubleClick") ? (
              <ToggleField
                checked={draft.createSessionOnSidebarDoubleClick}
                description="Create a session from empty sidebar space."
                label="Double-click empty sidebar space to create a session"
                {...getSettingModificationProps("createSessionOnSidebarDoubleClick")}
                onChange={(checked) => updateDraft("createSessionOnSidebarDoubleClick", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "renameSessionOnDoubleClick") ? (
              <ToggleField
                checked={draft.renameSessionOnDoubleClick}
                description="Rename sessions directly from their cards."
                label="Double-click session cards to rename"
                {...getSettingModificationProps("renameSessionOnDoubleClick")}
                onChange={(checked) => updateDraft("renameSessionOnDoubleClick", checked)}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("statusIndicators", settingsSearch.statusIndicators) ? (
            <SettingsSection sectionRef={statusIndicatorsSectionRef} title="Status Indicators">
              {mainSettingVisible(
                settingsSearch.statusIndicators,
                "hideFloatingSessionStatusIndicators",
              ) ? (
              <ToggleField
                checked={!draft.hideFloatingSessionStatusIndicators}
                description="Show the desktop floating session status badges."
                label="Show Floating Session Indicators"
                {...getSettingModificationProps("hideFloatingSessionStatusIndicators")}
                onChange={(checked) =>
                  updateDraft("hideFloatingSessionStatusIndicators", !checked)
                }
              />
              ) : null}
              {mainSettingVisible(
                settingsSearch.statusIndicators,
                "hideMenuBarSessionStatusIndicators",
              ) ? (
              <ToggleField
                checked={!draft.hideMenuBarSessionStatusIndicators}
                description="Show the menu bar session status badges."
                label="Show Menu Bar Session Indicators"
                {...getSettingModificationProps("hideMenuBarSessionStatusIndicators")}
                onChange={(checked) =>
                  updateDraft("hideMenuBarSessionStatusIndicators", !checked)
                }
              />
              ) : null}
              {mainSettingVisible(settingsSearch.statusIndicators, "petOverlayEnabled") ? (
              <ToggleField
                checked={draft.petOverlayEnabled}
                description="Show a draggable floating animated pet."
                label="Wake Pet"
                {...getSettingModificationProps("petOverlayEnabled")}
                onChange={(checked) => updateDraft("petOverlayEnabled", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.statusIndicators, "selectedPetId") ? (
              <PetPickerField
                {...getSettingModificationProps("selectedPetId")}
                onChange={(value) => updateDraft("selectedPetId", value)}
                value={draft.selectedPetId}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("browser", settingsSearch.browser) ? (
            <SettingsSection sectionRef={browserSectionRef} title="Browser">
              {/* CDXC:BrowserPanes 2026-05-27-07:24: Settings no longer exposes Chrome Canary attachment. Browser actions always open in workspace browser panes, leaving this section focused on pane behavior controls. */}
              {/* CDXC:BrowserFeedbackTools 2026-05-22-09:18:
                  Browser-pane context menus should expose one feedback action whose injected tool is user-selectable: Agentation by default for structured visual annotations, or React Grab when explicitly selected. */}
              {mainSettingVisible(settingsSearch.browser, "browserFeedbackTool") ? (
              <SelectField
                description="Choose the feedback tool launched from browser pane menus."
                label="Feedback Tool"
                {...getSettingModificationProps("browserFeedbackTool")}
                onChange={(value) =>
                  updateDraft("browserFeedbackTool", value as BrowserFeedbackTool)
                }
                options={BROWSER_FEEDBACK_TOOL_OPTIONS}
                value={draft.browserFeedbackTool}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("sessionCards", settingsSearch.sessionCards) ? (
            <SettingsSection sectionRef={sessionCardsSectionRef} title="Session Cards">
              {/* CDXC:SidebarSessions 2026-05-16-08:46: Session-card agent identity should stay visible by default, with a user setting that makes those icons appear only while hovering a session row. */}
              {mainSettingVisible(settingsSearch.sessionCards, "hideSessionAgentIconUntilHover") ? (
              <ToggleField
                checked={draft.hideSessionAgentIconUntilHover}
                description="Hide session agent icons until a session row is hovered."
                label="Hide agent icon until hover"
                {...getSettingModificationProps("hideSessionAgentIconUntilHover")}
                onChange={(checked) => updateDraft("hideSessionAgentIconUntilHover", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sessionCards, "hideBrowserFaviconUntilHover") ? (
              <ToggleField
                checked={draft.hideBrowserFaviconUntilHover}
                description="Hide browser page favicons until a session row is hovered."
                label="Hide browser favicon until hover"
                {...getSettingModificationProps("hideBrowserFaviconUntilHover")}
                onChange={(checked) => updateDraft("hideBrowserFaviconUntilHover", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sessionCards, "showCloseButtonOnSessionCards") ? (
              <ToggleField
                checked={draft.showCloseButtonOnSessionCards}
                description="Reveal the close control when hovering a card."
                label="Show close button on hover"
                {...getSettingModificationProps("showCloseButtonOnSessionCards")}
                onChange={(checked) => updateDraft("showCloseButtonOnSessionCards", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sessionCards, "hideLastActiveTimeOnSessionCards") ? (
              <ToggleField
                checked={draft.hideLastActiveTimeOnSessionCards}
                description="Hide Last Active timestamps from session-card title rows."
                label="Hide last active time"
                {...getSettingModificationProps("hideLastActiveTimeOnSessionCards")}
                onChange={(checked) => updateDraft("hideLastActiveTimeOnSessionCards", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sessionCards, "showSessionCloseContextMenuAction") ? (
              <>
                {/*
                 * CDXC:SidebarContextMenu 2026-06-10-13:58:
                 * Session context menus should hide the destructive Close item by default. Place this opt-in directly above the command-copy opt-in because both settings reveal advanced context-menu actions.
                 */}
                <ToggleField
                  checked={draft.showSessionCloseContextMenuAction}
                  description="Show the Close item in session context menus."
                  label="Show Close option in context menu"
                  {...getSettingModificationProps("showSessionCloseContextMenuAction")}
                  onChange={(checked) =>
                    updateDraft("showSessionCloseContextMenuAction", checked)
                  }
                />
              </>
              ) : null}
              {mainSettingVisible(settingsSearch.sessionCards, "showSessionCommandCopyActions") ? (
              <>
                {/*
                 * CDXC:SidebarContextMenu 2026-06-09-23:17:
                 * Copy resume and Copy attach command are advanced session-card context-menu utilities. Keep both hidden unless this Settings toggle is enabled so the default menu stays focused on normal session actions.
                 */}
                <ToggleField
                  checked={draft.showSessionCommandCopyActions}
                  description="Show Copy resume and Copy attach command in session context menus."
                  label="Show command copy actions"
                  {...getSettingModificationProps("showSessionCommandCopyActions")}
                  onChange={(checked) => updateDraft("showSessionCommandCopyActions", checked)}
                />
              </>
              ) : null}
              {mainSettingVisible(settingsSearch.sessionCards, "showSessionDetailsCopyAction") ? (
              <>
                {/*
                 * CDXC:SidebarContextMenu 2026-06-11-23:08:
                 * Copy details is separate from command-copy actions because it copies metadata, not executable shell commands. Keep it opt-in so users choose when session ids and project paths appear in context menus.
                 */}
                <ToggleField
                  checked={draft.showSessionDetailsCopyAction}
                  description="Show Copy details in session context menus."
                  label="Show Copy details option"
                  {...getSettingModificationProps("showSessionDetailsCopyAction")}
                  onChange={(checked) => updateDraft("showSessionDetailsCopyAction", checked)}
                />
              </>
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("workspace", settingsSearch.workspace) ? (
            <SettingsSection sectionRef={workspaceSectionRef} title="Workspace">
              {/*
                CDXC:WorkspaceLayout 2026-05-30-07:24:
                Pane Gap is no longer configurable in the macOS app. Keep the
                Workspace section focused on color/debug controls while shared
                settings normalization pins the retained compatibility field to zero.
              */}
              {mainSettingVisible(settingsSearch.workspace, "workspaceActivePaneBorderColor") ? (
              <TextField
                description="CSS color for the focused pane border."
                label="Active Pane Border"
                {...getSettingModificationProps("workspaceActivePaneBorderColor")}
                onChange={(value) => updateDraft("workspaceActivePaneBorderColor", value)}
                value={draft.workspaceActivePaneBorderColor}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.workspace, "workspaceBackgroundColor") ? (
              <ColorField
                description="Color shown behind terminal panes."
                label="Terminal Background"
                {...getSettingModificationProps("workspaceBackgroundColor")}
                onChange={(value) => updateDraft("workspaceBackgroundColor", value)}
                value={draft.workspaceBackgroundColor}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.workspace, "clickToWakeSleepingSessions") ? (
              <ToggleField
                checked={draft.clickToWakeSleepingSessions}
                description="Selecting a sleeping pane tab shows a black placeholder; click the pane body to wake the session."
                label="Click to wake sleeping panes"
                {...getSettingModificationProps("clickToWakeSleepingSessions")}
                onChange={(checked) => updateDraft("clickToWakeSleepingSessions", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.workspace, "commandsPanelDefaultHeightPx") ? (
              <SliderNumberField
                description="Used when opening the command pane (F12 or sidebar) and when double-clicking its top resize rail."
                label="Command Pane Default Height"
                {...getSettingModificationProps("commandsPanelDefaultHeightPx")}
                max={MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX}
                min={MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX}
                onCommit={(value) => updateDraft("commandsPanelDefaultHeightPx", value)}
                onChange={(value) => updateDraftDebounced("commandsPanelDefaultHeightPx", value)}
                step={1}
                value={draft.commandsPanelDefaultHeightPx}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.workspace, "debuggingMode") ? (
              <ToggleField
                checked={draft.debuggingMode}
                description="Shows debugging controls and writes detailed app diagnostics to disk. This can affect performance, so turn it off when you do not need it."
                label="Debug logging and UI"
                {...getSettingModificationProps("debuggingMode")}
                onChange={(checked) => updateDraft("debuggingMode", checked)}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("terminal", settingsSearch.terminal) ? (
            <SettingsSection sectionRef={ghosttyTerminalSectionRef} title="Terminal">
              {/* CDXC:TerminalSettings 2026-04-26-18:36: Terminal settings in
                  ghostex edit the shared Ghostty config file, so users must see
                  that external Ghostty windows receive the same values and can
                  reload them with Ghostty's normal config shortcut.

                  CDXC:SettingsNavigation 2026-06-12-04:13:
                  Ghostty terminal controls live in the main Settings page so
                  the Settings search box finds app and terminal controls in one
                  pass. */}
              {mainSettingVisible(settingsSearch.terminal, "ghosttySettingsActions") ? (
                <>
                  <div className="rounded-none border border-destructive/45 bg-destructive/10 px-4 py-3 text-sm leading-6 text-foreground">
                    Whatever you set here also applies to your external Ghostty terminal
                    because this Ghostty terminal uses the same settings file. ghostex reloads
                    its embedded Ghostty terminal about 3 seconds after you stop changing
                    these controls; external Ghostty windows may still need Cmd+Shift+, to
                    reload.
                  </div>
                  <GhosttySettingsActions
                    onApplyRecommended={applyRecommendedGhosttySettings}
                    onOpenConfigFile={() => onGhosttySettingsAction?.("openGhosttyConfigFile")}
                    onOpenDocs={() => onGhosttySettingsAction?.("openGhosttySettingsDocs")}
                    onResetDefaults={resetGhosttySettingsToDefault}
                  />
                </>
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalGhosttyTheme") ? (
                <SelectField
                  contentClassName="max-h-80"
                  description="Choose a bundled Ghostty theme, or leave your existing Ghostty config in charge."
                  label="Theme"
                  {...getSettingModificationProps("terminalGhosttyTheme")}
                  onChange={(value) =>
                    updateDraft(
                      "terminalGhosttyTheme",
                      value === GHOSTTY_THEME_UNMANAGED_VALUE ? "" : value,
                    )
                  }
                  options={GHOSTTY_THEME_SETTING_OPTIONS}
                  showScrollButtons={false}
                  value={draft.terminalGhosttyTheme || GHOSTTY_THEME_UNMANAGED_VALUE}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalFontFamily") ? (
                <TextField
                  description="Type a Ghostty font-family name. Leave blank to use existing Ghostty config or Ghostty's platform default."
                  label="Font Family"
                  {...getSettingModificationProps("terminalFontFamily")}
                  onChange={(value) => updateDraft("terminalFontFamily", value)}
                  placeholder="Ghostty default"
                  value={draft.terminalFontFamily}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalFontSize") ? (
                <SliderNumberField
                  description="Set terminal text size."
                  label="Font Size"
                  {...getSettingModificationProps("terminalFontSize")}
                  max={32}
                  min={8}
                  onCommit={(value) => updateDraft("terminalFontSize", value)}
                  onChange={(value) => updateDraftDebounced("terminalFontSize", value)}
                  step={0.5}
                  value={draft.terminalFontSize}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalFontWeight") ? (
                <SliderNumberField
                  description="Set terminal text weight."
                  label="Font Weight"
                  {...getSettingModificationProps("terminalFontWeight")}
                  max={900}
                  min={100}
                  onCommit={(value) => updateDraft("terminalFontWeight", value)}
                  onChange={(value) => updateDraftDebounced("terminalFontWeight", value)}
                  step={50}
                  value={draft.terminalFontWeight}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalLineHeight") ? (
                <SliderNumberField
                  description="Adjust terminal row height."
                  label="Line Height"
                  {...getSettingModificationProps("terminalLineHeight")}
                  max={2}
                  min={0.8}
                  onCommit={(value) => updateDraft("terminalLineHeight", value)}
                  onChange={(value) => updateDraftDebounced("terminalLineHeight", value)}
                  step={0.1}
                  value={draft.terminalLineHeight}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalLetterSpacing") ? (
                <SliderNumberField
                  description="Adjust spacing between glyphs."
                  label="Letter Spacing"
                  {...getSettingModificationProps("terminalLetterSpacing")}
                  max={8}
                  min={-2}
                  onCommit={(value) => updateDraft("terminalLetterSpacing", value)}
                  onChange={(value) => updateDraftDebounced("terminalLetterSpacing", value)}
                  step={0.1}
                  value={draft.terminalLetterSpacing}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalCursorStyle") ? (
                <SelectField
                  description="Choose the cursor shape."
                  label="Cursor Style"
                  {...getSettingModificationProps("terminalCursorStyle")}
                  onChange={(value) =>
                    updateDraft("terminalCursorStyle", value as TerminalCursorStyle)
                  }
                  options={[
                    { label: "Line", value: "bar" },
                    { label: "Block", value: "block" },
                    { label: "Underline", value: "underline" },
                  ]}
                  value={draft.terminalCursorStyle}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalCursorStyleBlink") ? (
                <ToggleField
                  checked={draft.terminalCursorStyleBlink}
                  description="Blink the terminal cursor."
                  label="Cursor blink"
                  {...getSettingModificationProps("terminalCursorStyleBlink")}
                  onChange={(checked) => updateDraft("terminalCursorStyleBlink", checked)}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "sessionPersistenceProvider") ? (
                /* CDXC:SessionPersistence 2026-05-05-07:28
                    Session persistence is a provider choice for new terminal and
                    agent launches. Existing panes keep their current process;
                    new panes can use tmux, zmx, or zellij so restart restores by
                    attach first and recreate+resume only when the named session
                    is gone.

                   CDXC:SessionPersistence 2026-05-06-03:43
                    zellij shares the same Settings selector and semantics as
                    tmux/zmx instead of adding a separate mode-specific control.

                   CDXC:SessionPersistence 2026-05-08-14:04
                    Explain that users should use zmx with zmx-session-manager when they care about ssh from
                    other devices continuing sessions created through ghostex. Recommend zmx because it leaves Agent CLI tools unaffected while minor issues remain.

                   CDXC:SessionPersistence 2026-05-26-13:41:
                    zmx is now the default and recommended Settings option. Hide tmux and zellij from the dropdown without removing their code paths, so existing persisted provider sessions still normalize and launch.

                  CDXC:SessionPersistence 2026-05-28-04:24:
                    The Session Persistence setting should no longer be marked as Beta in Settings copy or search results.

                   CDXC:SessionPersistence 2026-06-04-01:57:
                    Users can disable persistence, but the Settings dropdown must warn that Android and iOS attach flows depend on persistent provider sessions. Show the warning only while Off is selected so the risk is visible at the decision point without making the default zmx state noisy. */
                <SelectField
                  description="Use zmx with zmx-session-manager when you care about using ssh from other devices to continue working on sessions created using Ghostex. It doesn't affect the Agent CLI tools at all. Mostly working great, few minor issues left to fix."
                  label="Session Persistence"
                  {...getSettingModificationProps("sessionPersistenceProvider")}
                  onChange={(value) =>
                    updateDraft(
                      "sessionPersistenceProvider",
                      value as SessionPersistenceProvider,
                    )
                  }
                  options={SESSION_PERSISTENCE_PROVIDER_OPTIONS}
                  supportingContent={
                    draft.sessionPersistenceProvider === "off" ? (
                      <div className="settings-persistence-warning" role="note">
                        <IconAlertTriangle aria-hidden="true" size={14} />
                        <span>
                          Android and iOS attach can have issues while persistence is disabled.
                        </span>
                      </div>
                    ) : undefined
                  }
                  value={draft.sessionPersistenceProvider}
                />
              ) : null}
              {draft.sessionPersistenceProvider !== "off" &&
              mainSettingVisible(settingsSearch.terminal, "showSessionIdInTerminalPanes") ? (
                /*
                 * CDXC:SessionPersistence 2026-05-23-00:50:
                 * The pane-local provider/session label is useful for zmx/tmux/zellij
                 * attach context. Keep this setting shown only when a persistence
                 * provider is selected, while the label renderer still requires each
                 * terminal pane to have provider metadata before showing text.
                 */
                <ToggleField
                  checked={draft.showSessionIdInTerminalPanes}
                  description="Show the provider session id in the top-right corner of each terminal pane."
                  label="Show session id in the top right of each terminal pane"
                  {...getSettingModificationProps("showSessionIdInTerminalPanes")}
                  onChange={(checked) => updateDraft("showSessionIdInTerminalPanes", checked)}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "promptEditorBackend") ? (
                /**
                 * CDXC:PromptEditorBackend 2026-05-11-14:38
                 * Ctrl+G prompt editing can render either through the native
                 * WebKit Monaco overlay or the gte TUI running inside the
                 * launching terminal. Keep the install action with the gte option.
                 *
                 * CDXC:PromptEditorBackend 2026-05-22-09:56:
                 * Settings copy must use gte for Ghostex Terminal Editor so users see the same name in the app, install command, and Ctrl+G editor selection.
                 *
                 * CDXC:PromptEditorBackend 2026-05-22-10:16:
                 * Selecting gte must not describe or launch a popup. gte runs in the terminal that invoked Ctrl+G, while Monaco remains the popup editor.
                 */
                <GtePromptEditingField
                  backend={draft.promptEditorBackend}
                  isModified={getSettingModificationProps("promptEditorBackend").isModified}
                  onInstall={() => onInstallGte?.()}
                  onChange={(backend) => updateDraft("promptEditorBackend", backend)}
                  onResetToDefault={
                    getSettingModificationProps("promptEditorBackend").onResetToDefault
                  }
                />
              ) : null}
              {draft.promptEditorBackend === "custom" &&
              mainSettingVisible(settingsSearch.terminal, "customPromptEditorCommand") ? (
                <TextField
                  description="Write the command exactly as the terminal should export it for $EDITOR and $VISUAL."
                  label="Custom Ctrl+G editor command"
                  {...getSettingModificationProps("customPromptEditorCommand")}
                  onChange={(value) => updateDraft("customPromptEditorCommand", value)}
                  placeholder="code --wait"
                  value={draft.customPromptEditorCommand}
                />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("terminalBehavior", settingsSearch.terminalBehavior) ? (
            <SettingsSection sectionRef={ghosttyBehaviorSectionRef} title="Terminal Behavior">
              {/* CDXC:TerminalBehaviorSettings 2026-04-29-09:32: Expose the
                  Ghostty settings users commonly tune: scrollback memory,
                  copy-on-select, close confirmation, clipboard safety,
                  pointer hiding, and native scrollbar visibility. These
                  controls write documented Ghostty config keys instead of
                  intercepting terminal behavior inside ghostex. */}
              {mainSettingVisible(
                settingsSearch.terminalBehavior,
                "terminalScrollbackLimitMb",
              ) ? (
                <SliderNumberField
                  description="Scrollback memory per terminal surface. Ghostty default is 10 MB and changes affect new terminals."
                  label="Scrollback limit"
                  {...getSettingModificationProps("terminalScrollbackLimitMb")}
                  max={200}
                  min={1}
                  onCommit={(value) => updateDraft("terminalScrollbackLimitMb", value)}
                  onChange={(value) =>
                    updateDraftDebounced("terminalScrollbackLimitMb", value)
                  }
                  step={1}
                  value={draft.terminalScrollbackLimitMb}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminalBehavior, "terminalCopyOnSelect") ? (
                <SelectField
                  description="Copy selected terminal text automatically."
                  label="Copy on select"
                  {...getSettingModificationProps("terminalCopyOnSelect")}
                  onChange={(value) =>
                    updateDraft("terminalCopyOnSelect", value as GhosttyCopyOnSelect)
                  }
                  options={GHOSTTY_COPY_ON_SELECT_OPTIONS}
                  value={draft.terminalCopyOnSelect}
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalBehavior,
                "terminalConfirmCloseSurface",
              ) ? (
                <SelectField
                  description="Confirm before closing terminal surfaces."
                  label="Confirm close"
                  {...getSettingModificationProps("terminalConfirmCloseSurface")}
                  onChange={(value) =>
                    updateDraft(
                      "terminalConfirmCloseSurface",
                      value as GhosttyConfirmCloseSurface,
                    )
                  }
                  options={GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS}
                  value={draft.terminalConfirmCloseSurface}
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalBehavior,
                "terminalClipboardTrimTrailingSpaces",
              ) ? (
                <ToggleField
                  checked={draft.terminalClipboardTrimTrailingSpaces}
                  description="Trim trailing whitespace when copying terminal text."
                  label="Trim trailing spaces on copy"
                  {...getSettingModificationProps("terminalClipboardTrimTrailingSpaces")}
                  onChange={(checked) =>
                    updateDraft("terminalClipboardTrimTrailingSpaces", checked)
                  }
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalBehavior,
                "terminalClipboardPasteProtection",
              ) ? (
                <ToggleField
                  checked={draft.terminalClipboardPasteProtection}
                  description="Ask before pasting text Ghostty considers unsafe."
                  label="Paste protection"
                  {...getSettingModificationProps("terminalClipboardPasteProtection")}
                  onChange={(checked) =>
                    updateDraft("terminalClipboardPasteProtection", checked)
                  }
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalBehavior,
                "terminalPastePreviewableImages",
              ) ? (
                <ToggleField
                  checked={draft.terminalPastePreviewableImages}
                  description={PASTE_PREVIEWABLE_IMAGES_DESCRIPTION}
                  label="Paste previewable images"
                  {...getSettingModificationProps("terminalPastePreviewableImages")}
                  onChange={(checked) =>
                    updateDraft("terminalPastePreviewableImages", checked)
                  }
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalBehavior,
                "terminalMouseHideWhileTyping",
              ) ? (
                <ToggleField
                  checked={draft.terminalMouseHideWhileTyping}
                  description="Hide the pointer while typing in the terminal."
                  label="Hide mouse while typing"
                  {...getSettingModificationProps("terminalMouseHideWhileTyping")}
                  onChange={(checked) =>
                    updateDraft("terminalMouseHideWhileTyping", checked)
                  }
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminalBehavior, "terminalScrollbar") ? (
                <SelectField
                  description="Control whether Ghostty shows its native scrollback scrollbar."
                  label="Scrollbar"
                  {...getSettingModificationProps("terminalScrollbar")}
                  onChange={(value) =>
                    updateDraft("terminalScrollbar", value as GhosttyScrollbar)
                  }
                  options={GHOSTTY_SCROLLBAR_OPTIONS}
                  value={draft.terminalScrollbar}
                />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("terminalScrolling", settingsSearch.terminalScrolling) ? (
            <SettingsSection sectionRef={ghosttyScrollingSectionRef} title="Terminal Scrolling">
              {/* CDXC:TerminalScrollSettings 2026-04-29-08:56: Ghostty
                  scroll speed is controlled by mouse-scroll-multiplier.
                  Precision and discrete devices need separate controls because
                  Ghostty defaults trackpads to 1 and notched wheels to 3.
                  The modal exposes 0.25-step sliders from 0.25 to 8 because
                  Ghostty's documented 0.01..10000 bounds are extreme. */}
              {mainSettingVisible(
                settingsSearch.terminalScrolling,
                "terminalMouseScrollMultiplierPrecision",
              ) ? (
                <SliderNumberField
                  description="Trackpads and high-resolution scroll wheels. Ghostty default is 1."
                  label="Precision scroll multiplier"
                  {...getSettingModificationProps("terminalMouseScrollMultiplierPrecision")}
                  max={8}
                  min={0.25}
                  onCommit={(value) =>
                    updateDraft("terminalMouseScrollMultiplierPrecision", value)
                  }
                  onChange={(value) =>
                    updateDraftDebounced("terminalMouseScrollMultiplierPrecision", value)
                  }
                  step={0.25}
                  value={draft.terminalMouseScrollMultiplierPrecision}
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalScrolling,
                "terminalMouseScrollMultiplierDiscrete",
              ) ? (
                <SliderNumberField
                  description="Traditional notched mouse wheels. Ghostty default is 3."
                  label="Discrete scroll multiplier"
                  {...getSettingModificationProps("terminalMouseScrollMultiplierDiscrete")}
                  max={8}
                  min={0.25}
                  onCommit={(value) =>
                    updateDraft("terminalMouseScrollMultiplierDiscrete", value)
                  }
                  onChange={(value) =>
                    updateDraftDebounced("terminalMouseScrollMultiplierDiscrete", value)
                  }
                  step={0.25}
                  value={draft.terminalMouseScrollMultiplierDiscrete}
                />
              ) : null}
              {mainSettingVisible(
                settingsSearch.terminalScrolling,
                "terminalScrollToBottomWhenTyping",
              ) ? (
                <ToggleField
                  checked={draft.terminalScrollToBottomWhenTyping}
                  description="Keep the prompt visible while typing."
                  label="Scroll to bottom when typing"
                  {...getSettingModificationProps("terminalScrollToBottomWhenTyping")}
                  onChange={(checked) =>
                    updateDraft("terminalScrollToBottomWhenTyping", checked)
                  }
                />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("editor", settingsSearch.editor) ? (
            <SettingsSection sectionRef={editorSectionRef} title="Editor">
              {mainSettingVisible(settingsSearch.editor, "defaultEditorCommand") ? (
              <SelectField
                description="Choose the command used when opening files in an external editor."
                label="Default editor command"
                {...getSettingModificationProps("defaultEditorCommand")}
                onChange={(value) =>
                  updateDraft("defaultEditorCommand", value as DefaultEditorCommand)
                }
                options={DEFAULT_EDITOR_COMMAND_OPTIONS}
                value={draft.defaultEditorCommand}
              />
              ) : null}
              {draft.defaultEditorCommand === "other" &&
              mainSettingVisible(settingsSearch.editor, "customDefaultEditorCommand") ? (
              <TextField
                description="Write the command exactly as it should be launched. The file path will be passed to it later."
                label="Custom editor command"
                {...getSettingModificationProps("customDefaultEditorCommand")}
                onChange={(value) => updateDraft("customDefaultEditorCommand", value)}
                placeholder="my-editor --reuse-window"
                value={draft.customDefaultEditorCommand}
              />
              ) : null}
              {/* CDXC:EditorPanes 2026-06-08-20:12: Embedded code-server panes
                  use Ghostex-owned bundled editor settings by default so the
                  macOS VS Code surface starts on Dark 2026. This toggle opts
                  into linking local VS Code settings, while the Insiders
                  checkbox only changes the linked config directory. */}
              {mainSettingVisible(settingsSearch.editor, "codeServerLinkVscodeUserConfig") ? (
              <ToggleField
                checked={draft.codeServerLinkVscodeUserConfig}
                description="Use local VS Code settings instead of the bundled editor defaults."
                label="Use VS Code settings"
                onChange={(checked) => updateDraft("codeServerLinkVscodeUserConfig", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.editor, "codeServerUseVscodeInsidersUserConfig") ? (
              <ToggleField
                checked={draft.codeServerUseVscodeInsidersUserConfig}
                description="Use the VS Code Insiders user settings directory."
                disabled={!draft.codeServerLinkVscodeUserConfig}
                label="Use VS Code Insiders settings"
                onChange={(checked) =>
                  updateDraft("codeServerUseVscodeInsidersUserConfig", checked)
                }
              />
              ) : null}
              {/* CDXC:ProjectDiffStats 2026-05-16-08:46: The project-header git line summary is useful but visually noisy for some workflows, so Settings owns a full hide toggle separate from the changed-file count toggle. */}
              {mainSettingVisible(settingsSearch.editor, "hideProjectHeaderDiffStats") ? (
              <ToggleField
                checked={draft.hideProjectHeaderDiffStats}
                description="Hide +added/-removed line counts next to project headers."
                label="Hide project git stats"
                {...getSettingModificationProps("hideProjectHeaderDiffStats")}
                onChange={(checked) => updateDraft("hideProjectHeaderDiffStats", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.editor, "showProjectEditorDiffFileCount") ? (
              <ToggleField
                checked={draft.showProjectEditorDiffFileCount}
                description="Show changed-file counts in project header git stats."
                label="Show editor file count"
                {...getSettingModificationProps("showProjectEditorDiffFileCount")}
                onChange={(checked) => updateDraft("showProjectEditorDiffFileCount", checked)}
              />
              ) : null}
              {mainSettingVisible(
                settingsSearch.editor,
                "showUntrackedProjectDiffWhenNoTrackedChanges",
              ) ? (
              <ToggleField
                checked={draft.showUntrackedProjectDiffWhenNoTrackedChanges}
                description="When tracked git diff is +0 -0, show untracked line counts in project headers."
                label="Show untracked lines without tracked changes"
                {...getSettingModificationProps("showUntrackedProjectDiffWhenNoTrackedChanges")}
                onChange={(checked) =>
                  updateDraft("showUntrackedProjectDiffWhenNoTrackedChanges", checked)
                }
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("autoSleep", settingsSearch.autoSleep) ? (
            <SettingsSection sectionRef={autoSleepSectionRef} title="Auto Sleep">
              {/* CDXC:AutoSleep 2026-05-28-08:32: Auto Sleep controls belong in one Settings section so VS Code, Git, Project, browser, and agent sessions can be tuned independently without hiding the relationship between the policies. */}
              {mainSettingVisible(settingsSearch.autoSleep, "autoSleepCodeEditorEnabled") ? (
              <ToggleField
                checked={draft.autoSleepCodeEditorEnabled}
                description="Sleep inactive VS Code panes after the selected idle period."
                label="Sleep inactive VS Code panes"
                {...getSettingModificationProps("autoSleepCodeEditorEnabled")}
                onChange={(checked) => updateDraft("autoSleepCodeEditorEnabled", checked)}
              />
              ) : null}
              {draft.autoSleepCodeEditorEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepCodeEditorIdleMinutes") ? (
              <SelectField
                description="Idle time before inactive VS Code panes sleep."
                label="VS Code idle time"
                {...getSettingModificationProps("autoSleepCodeEditorIdleMinutes")}
                onChange={(value) =>
                  updateDraft("autoSleepCodeEditorIdleMinutes", Number(value) as AutoSleepIdleMinutes)
                }
                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                  label: option.label,
                  value: String(option.value),
                }))}
                value={String(draft.autoSleepCodeEditorIdleMinutes)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.autoSleep, "autoSleepGitEditorEnabled") ? (
              <ToggleField
                checked={draft.autoSleepGitEditorEnabled}
                description="Sleep inactive Git panes after the selected idle period."
                label="Sleep inactive Git panes"
                {...getSettingModificationProps("autoSleepGitEditorEnabled")}
                onChange={(checked) => updateDraft("autoSleepGitEditorEnabled", checked)}
              />
              ) : null}
              {draft.autoSleepGitEditorEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepGitEditorIdleMinutes") ? (
              <SelectField
                description="Idle time before inactive Git panes sleep."
                label="Git idle time"
                {...getSettingModificationProps("autoSleepGitEditorIdleMinutes")}
                onChange={(value) =>
                  updateDraft("autoSleepGitEditorIdleMinutes", Number(value) as AutoSleepIdleMinutes)
                }
                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                  label: option.label,
                  value: String(option.value),
                }))}
                value={String(draft.autoSleepGitEditorIdleMinutes)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.autoSleep, "autoSleepProjectEditorEnabled") ? (
              <ToggleField
                checked={draft.autoSleepProjectEditorEnabled}
                description="Sleep inactive Project panes after the selected idle period."
                label="Sleep inactive Project panes"
                {...getSettingModificationProps("autoSleepProjectEditorEnabled")}
                onChange={(checked) => updateDraft("autoSleepProjectEditorEnabled", checked)}
              />
              ) : null}
              {draft.autoSleepProjectEditorEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepProjectEditorIdleMinutes") ? (
              <SelectField
                description="Idle time before inactive Project panes sleep."
                label="Project idle time"
                {...getSettingModificationProps("autoSleepProjectEditorIdleMinutes")}
                onChange={(value) =>
                  updateDraft("autoSleepProjectEditorIdleMinutes", Number(value) as AutoSleepIdleMinutes)
                }
                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                  label: option.label,
                  value: String(option.value),
                }))}
                value={String(draft.autoSleepProjectEditorIdleMinutes)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.autoSleep, "autoSleepBrowserSessionsEnabled") ? (
              <ToggleField
                checked={draft.autoSleepBrowserSessionsEnabled}
                description="Sleep inactive browser panes after the selected idle period."
                label="Sleep inactive browser panes"
                {...getSettingModificationProps("autoSleepBrowserSessionsEnabled")}
                onChange={(checked) => updateDraft("autoSleepBrowserSessionsEnabled", checked)}
              />
              ) : null}
              {draft.autoSleepBrowserSessionsEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepBrowserIdleMinutes") ? (
              <SelectField
                description="Idle time before inactive browser panes sleep."
                label="Browser idle time"
                {...getSettingModificationProps("autoSleepBrowserIdleMinutes")}
                onChange={(value) =>
                  updateDraft("autoSleepBrowserIdleMinutes", Number(value) as AutoSleepIdleMinutes)
                }
                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                  label: option.label,
                  value: String(option.value),
                }))}
                value={String(draft.autoSleepBrowserIdleMinutes)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.autoSleep, "autoSleepAgentSessionsEnabled") ? (
              <ToggleField
                checked={draft.autoSleepAgentSessionsEnabled}
                description="Sleep idle agent terminal sessions automatically."
                label="Sleep idle agent sessions"
                {...getSettingModificationProps("autoSleepAgentSessionsEnabled")}
                onChange={(checked) => updateDraft("autoSleepAgentSessionsEnabled", checked)}
              />
              ) : null}
              {draft.autoSleepAgentSessionsEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepAgentIdleMinutes") ? (
              <SelectField
                description="Idle time before eligible agent terminals sleep."
                label="Agent idle time"
                {...getSettingModificationProps("autoSleepAgentIdleMinutes")}
                onChange={(value) =>
                  updateDraft("autoSleepAgentIdleMinutes", Number(value) as AutoSleepIdleMinutes)
                }
                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                  label: option.label,
                  value: String(option.value),
                }))}
                value={String(draft.autoSleepAgentIdleMinutes)}
              />
              ) : null}
              {draft.autoSleepAgentSessionsEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepRequireAgentResumeCommand") ? (
              <ToggleField
                checked={draft.autoSleepRequireAgentResumeCommand}
                description="Only auto-sleep agent sessions Ghostex can wake with a resume command."
                label="Require resume command"
                {...getSettingModificationProps("autoSleepRequireAgentResumeCommand")}
                onChange={(checked) =>
                  updateDraft("autoSleepRequireAgentResumeCommand", checked)
                }
              />
              ) : null}
              {draft.autoSleepAgentSessionsEnabled &&
              mainSettingVisible(settingsSearch.autoSleep, "autoSleepFavoriteAgentSessions") ? (
              <ToggleField
                checked={draft.autoSleepFavoriteAgentSessions}
                description="Allow favorite agent sessions to auto-sleep."
                label="Include favorite agents"
                {...getSettingModificationProps("autoSleepFavoriteAgentSessions")}
                onChange={(checked) => updateDraft("autoSleepFavoriteAgentSessions", checked)}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("power", settingsSearch.power) ? (
            <SettingsSection sectionRef={powerSectionRef} title="Power">
              {mainSettingVisible(settingsSearch.power, "hideKeepAwakeTitlebarControl") ? (
              <ToggleField
                checked={draft.hideKeepAwakeTitlebarControl}
                description="Hide the keep-awake control from the title bar."
                label="Hide title-bar keep-awake control"
                {...getSettingModificationProps("hideKeepAwakeTitlebarControl")}
                onChange={(checked) => updateDraft("hideKeepAwakeTitlebarControl", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeDefaultDurationMinutes") ? (
              <SelectField
                description="Choose the duration used by the title-bar keep-awake button."
                label="Default keep-awake duration"
                {...getSettingModificationProps("keepAwakeDefaultDurationMinutes")}
                onChange={(value) =>
                  updateDraft("keepAwakeDefaultDurationMinutes", Number(value) as KeepAwakeDurationMinutes)
                }
                options={KEEP_AWAKE_DURATION_OPTIONS.map((option) => ({
                  label: option.label,
                  value: String(option.value),
                }))}
                value={String(draft.keepAwakeDefaultDurationMinutes)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeAllowDisplaySleep") ? (
              <ToggleField
                checked={draft.keepAwakeAllowDisplaySleep}
                description="Keep the Mac awake but allow the display to turn off."
                label="Allow display sleep"
                {...getSettingModificationProps("keepAwakeAllowDisplaySleep")}
                onChange={(checked) => updateDraft("keepAwakeAllowDisplaySleep", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakePreventLidSleep") ? (
              <ToggleField
                checked={draft.keepAwakePreventLidSleep}
                description="Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session. Keep Awake itself remains off until you enable it."
                label="Prevent lid-close sleep"
                {...getSettingModificationProps("keepAwakePreventLidSleep")}
                onChange={(checked) => updateDraft("keepAwakePreventLidSleep", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeActivateOnLaunch") ? (
              <ToggleField
                checked={draft.keepAwakeActivateOnLaunch}
                description="Start preventing sleep when Ghostex launches."
                label="Activate on launch"
                {...getSettingModificationProps("keepAwakeActivateOnLaunch")}
                onChange={(checked) => updateDraft("keepAwakeActivateOnLaunch", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeActivateOnExternalDisplay") ? (
              <ToggleField
                checked={draft.keepAwakeActivateOnExternalDisplay}
                description="Start preventing sleep when an external display is connected."
                label="Activate on external display"
                {...getSettingModificationProps("keepAwakeActivateOnExternalDisplay")}
                onChange={(checked) => updateDraft("keepAwakeActivateOnExternalDisplay", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeDeactivateBelowBatteryThreshold") ? (
              <ToggleField
                checked={draft.keepAwakeDeactivateBelowBatteryThreshold}
                description="Stop preventing sleep when battery capacity drops below the threshold."
                label="Deactivate below battery threshold"
                {...getSettingModificationProps("keepAwakeDeactivateBelowBatteryThreshold")}
                onChange={(checked) => updateDraft("keepAwakeDeactivateBelowBatteryThreshold", checked)}
              />
              ) : null}
              {draft.keepAwakeDeactivateBelowBatteryThreshold &&
              mainSettingVisible(settingsSearch.power, "keepAwakeBatteryThresholdPercent") ? (
              <SliderNumberField
                description="Battery percentage used by the threshold rule."
                label="Battery threshold"
                {...getSettingModificationProps("keepAwakeBatteryThresholdPercent")}
                max={90}
                min={10}
                onCommit={(value) => updateDraft("keepAwakeBatteryThresholdPercent", value)}
                onChange={(value) => updateDraftDebounced("keepAwakeBatteryThresholdPercent", value)}
                step={5}
                value={draft.keepAwakeBatteryThresholdPercent}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeDeactivateOnLowPowerMode") ? (
              <ToggleField
                checked={draft.keepAwakeDeactivateOnLowPowerMode}
                description="Stop preventing sleep when macOS Low Power Mode is enabled."
                label="Deactivate in Low Power Mode"
                {...getSettingModificationProps("keepAwakeDeactivateOnLowPowerMode")}
                onChange={(checked) => updateDraft("keepAwakeDeactivateOnLowPowerMode", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.power, "keepAwakeDeactivateOnUserSwitch") ? (
              <ToggleField
                checked={draft.keepAwakeDeactivateOnUserSwitch}
                description="Stop preventing sleep when this user session is no longer active."
                label="Deactivate on user switch"
                {...getSettingModificationProps("keepAwakeDeactivateOnUserSwitch")}
                onChange={(checked) => updateDraft("keepAwakeDeactivateOnUserSwitch", checked)}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("sounds", settingsSearch.sounds) ? (
            <SettingsSection sectionRef={soundsSectionRef} title="Sounds">
              {mainSettingVisible(settingsSearch.sounds, "completionBellEnabled") ? (
              <ToggleField
                checked={draft.completionBellEnabled}
                description="Play a completion sound when work finishes."
                label="Enable completion bell"
                {...getSettingModificationProps("completionBellEnabled")}
                onChange={(checked) => updateDraft("completionBellEnabled", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sounds, "completionSound") ? (
              <SoundField
                description="Sound for terminal completions."
                label="Completion Sound"
                {...getSettingModificationProps("completionSound")}
                onChange={(value) => updateDraft("completionSound", value)}
                onPlay={onPlayCompletionSound}
                value={draft.completionSound}
              />
              ) : null}
              {/* CDXC:SessionAttentionNotifications 2026-05-10-16:46:
                  Attention banners are separate from completion sounds because
                  users may want clickable macOS routing without audible alerts. */}
              {mainSettingVisible(settingsSearch.sounds, "showMacOSAttentionNotifications") ? (
              <ToggleField
                checked={draft.showMacOSAttentionNotifications}
                description="Show a macOS banner when a session needs attention."
                label="macOS Attention Notifications"
                {...getSettingModificationProps("showMacOSAttentionNotifications")}
                onChange={(checked) => {
                  updateDraft("showMacOSAttentionNotifications", checked);
                  if (checked) {
                    onRequestMacOSNotificationPermission?.();
                  }
                }}
              />
              ) : null}
              {/* CDXC:SessionAttentionNotifications 2026-05-11-01:14:
                  The Settings test button must run the real completion alert
                  path while the adjacent macOS button handles denied or muted
                  system notification permission outside ghostex settings. */}
              {mainSettingVisible(settingsSearch.sounds, "attentionNotificationActions") ? (
              <ActionButtonPairField
                actions={[
                  {
                    label: "Test agent task completion",
                    onClick: () => onTestAgentTaskCompletion?.(),
                  },
                  {
                    label: "macOS Notification Settings",
                    onClick: () => onOpenMacOSNotificationSettings?.(),
                  },
                ]}
                description="Run the current completion sound and notification flow, or open macOS notification permissions."
                label="Completion Alerts"
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sounds, "actionCompletionSound") ? (
              <SoundField
                description="Sound for action completions."
                label="Action Completion Sound"
                {...getSettingModificationProps("actionCompletionSound")}
                onChange={(value) => updateDraft("actionCompletionSound", value)}
                onPlay={onPlayCompletionSound}
                value={draft.actionCompletionSound}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSectionVisible("storage", settingsSearch.storage) ? (
              <div ref={storageSectionRef}>
                <GhostexFolderStatsSection
                  isLoading={ghostexFolderStatsLoading}
                  onOpenGhostexFolder={onOpenGhostexFolder}
                  stats={ghostexFolderStats}
                />
              </div>
            ) : null}

            {!isFirstLaunchSetup && !hasVisibleMainSettings ? (
              <div className="rounded-none border border-border bg-muted/30 px-4 py-6 text-center text-sm text-muted-foreground">
                No settings match your search.
              </div>
            ) : null}

            {isFirstLaunchSetup ? (
              <div className="flex justify-end pt-2">
                <Button
                  className="h-10 px-5 text-sm"
                  onClick={() => {
                    rememberActiveScrollPosition();
                    flushPendingSettings();
                    onClose();
                  }}
                  type="button"
                >
                  Continue
                </Button>
              </div>
            ) : (
              <>
                <Separator className="bg-border" />
                <div className="flex justify-between gap-3">
                  <Button
                    className="h-10 px-5 text-sm"
                    onClick={resetSettings}
                    type="button"
                    variant="outline"
                  >
                    Reset to defaults
                  </Button>
                </div>
              </>
            )}
          </div>
          </ScrollArea>
          </div>
          </TabsContent>
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="osIntegration">
            <OSIntegrationSettingsTab
              loading={osIntegrationStatusLoading}
              onRequestStatus={onRequestOSIntegrationStatus}
              onSetDefaults={onSetOSIntegrationDefaults}
              status={osIntegrationStatus}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="integrations">
            <IntegrationsSettingsTab
              agentHookStatus={agentHookStatus}
              agentHookStatusLoading={agentHookStatusLoading}
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              appShotsEnabled={draft.appShotsEnabled}
              appShotsHotkey={draft.appShotsHotkey}
              onAppShotsEnabledChange={(checked) => updateDraft("appShotsEnabled", checked)}
              onAppShotsHotkeyChange={(hotkey) => updateDraft("appShotsHotkey", hotkey)}
              onInstallAgentOrchestrationSkill={onInstallAgentOrchestrationSkill}
              onInstallAgentHooks={onInstallAgentHooks}
              onInstallBrowserControl={onInstallBrowserControl}
              onInstallComputerUseSkill={onInstallComputerUseSkill}
              onInstallCuaDriver={onInstallCuaDriver}
              onInstallGenerateTitleSkill={onInstallGenerateTitleSkill}
              onInstallGhostexCli={onInstallGhostexCli}
              onOpenAccessibilityPreferences={onOpenAccessibilityPreferences}
              onOpenFirstLaunchSetup={onOpenFirstLaunchSetup}
              onOpenScreenRecordingPreferences={onOpenScreenRecordingPreferences}
              onRequestAgentHookStatus={onRequestAgentHookStatus}
              onRequestGhostexCliStatus={onRequestGhostexCliStatus}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="remote">
            <RemoteSettingsTab
              initialRemoteMachineId={initialRemoteMachineId}
              isActive={isOpen && activeTab === "remote"}
              onChange={(nextRemoteMachines) =>
                applySettings({
                  ...draft,
                  remoteMachines: nextRemoteMachines,
                })
              }
              remoteMachines={draft.remoteMachines}
              vscode={vscode}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="projects">
            <ProjectsSettingsPanel projects={projects} vscode={vscode} />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="agents">
            <AgentsSettingsTab
              agentHookStatus={agentHookStatus}
              agentHookStatusLoading={agentHookStatusLoading}
              agentAcceptAllEnabled={draft.agentAcceptAllEnabled}
              customSessionTitleGenerationCommand={draft.customSessionTitleGenerationCommand}
              defaultPromptAgentId={draft.defaultPromptAgentId}
              sessionTitleGenerationAgent={draft.sessionTitleGenerationAgent}
              onAgentAcceptAllEnabledChange={(checked) =>
                applySettings({
                  ...draft,
                  agentAcceptAllEnabled: checked,
                })
              }
              onDefaultPromptAgentIdChange={(agentId) =>
                applySettings({
                  ...draft,
                  defaultPromptAgentId: agentId,
                })
              }
              onCustomSessionTitleGenerationCommandChange={(command) =>
                applySettings({
                  ...draft,
                  customSessionTitleGenerationCommand: command,
                })
              }
              onInstallAgentHooks={onInstallAgentHooks}
              onRequestAgentHookStatus={onRequestAgentHookStatus}
              onSessionTitleGenerationAgentChange={(agent) =>
                applySettings({
                  ...draft,
                  sessionTitleGenerationAgent: agent,
                })
              }
              vscode={vscode}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="actions">
            <ActionsSettingsTab vscode={vscode} />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="openTargets">
            <OpenTargetsSettingsTab
              onChange={(nextSettings) => applySettings(nextSettings)}
              settings={draft}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="hotkeys">
            <HotkeysSettingsTab
              hotkeys={draft.hotkeys}
              searchQuery={hotkeysSearchQuery}
              onChange={(hotkeys) => updateDraft("hotkeys", hotkeys)}
            />
          </TabsContent>
          ) : null}
          </Tabs>
        </TooltipProvider>
      </DialogContent>
    </Dialog>
  );
}

type SettingsAgentEditorState = {
  draft: AgentConfigDraft;
};

type SettingsCommandEditorState = {
  draft: CommandConfigDraft;
  lockedActionType?: SidebarActionType;
};

type SettingsOpenTargetEditorState = {
  draft: {
    argsText: string;
    command: string;
    label: string;
  };
  id?: string;
};

type RemoteMachineDraft = {
  id: string;
  name: string;
  sshHost: string;
  sshIdentityFile: string;
  sshPassword: string;
  sshPasswordSaved: boolean;
  sshPort: string;
  sshUser: string;
};

function createRemoteMachineDraft(): RemoteMachineDraft {
  return {
    id: `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
    name: "",
    sshHost: "",
    sshIdentityFile: "",
    sshPassword: "",
    sshPasswordSaved: false,
    sshPort: "",
    sshUser: "",
  };
}

function RemoteSettingsTab({
  initialRemoteMachineId,
  isActive,
  onChange,
  remoteMachines,
  vscode,
}: {
  initialRemoteMachineId?: string;
  isActive: boolean;
  onChange: (remoteMachines: RemoteMachineSettings[]) => void;
  remoteMachines: RemoteMachineSettings[];
  vscode?: WebviewApi;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isTailscaleHelpOpen, setIsTailscaleHelpOpen] = useState(false);
  const [newMachine, setNewMachine] = useState<RemoteMachineDraft>(() => createRemoteMachineDraft());
  const [sshPasswordDrafts, setSshPasswordDrafts] = useState<Record<string, string>>({});
  const lastTargetedRemoteMachineIdRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (!isActive || !initialRemoteMachineId) {
      if (!isActive) {
        lastTargetedRemoteMachineIdRef.current = undefined;
      }
      return;
    }
    if (lastTargetedRemoteMachineIdRef.current === initialRemoteMachineId) {
      return;
    }
    const animationFrame = requestAnimationFrame(() => {
      const targetCard = Array.from(
        containerRef.current?.querySelectorAll<HTMLElement>("[data-settings-remote-machine-id]") ?? [],
      ).find((candidate) => candidate.dataset.settingsRemoteMachineId === initialRemoteMachineId);
      if (!targetCard) {
        return;
      }
      /*
       * CDXC:RemoteMachines 2026-06-10-09:54:
       * Remote machine header Edit should land on the selected saved machine's
       * editable card, not just the generic Remote settings tab. Focus the name
       * field after scrolling because it is the first user-facing machine field.
       */
      targetCard.scrollIntoView({ behavior: "smooth", block: "center" });
      targetCard
        .querySelector<HTMLInputElement>("input[aria-label='Remote machine name']")
        ?.focus({ preventScroll: true });
      lastTargetedRemoteMachineIdRef.current = initialRemoteMachineId;
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [initialRemoteMachineId, isActive, remoteMachines]);

  const updateRemoteMachine = (machineId: string, patch: Partial<RemoteMachineDraft>) => {
    if (patch.sshPassword !== undefined) {
      setSshPasswordDrafts((drafts) => ({
        ...drafts,
        [machineId]: patch.sshPassword ?? "",
      }));
    }
    const settingsPatch = {
      name: patch.name,
      sshHost: patch.sshHost,
      sshIdentityFile: patch.sshIdentityFile,
      sshPort: patch.sshPort,
      sshUser: patch.sshUser,
    };
    if (Object.values(settingsPatch).every((value) => value === undefined)) {
      return;
    }
    const nextMachines = remoteMachines
      .map((machine) => {
        if (machine.id !== machineId) {
          return machine;
        }
        return normalizeRemoteMachineDraft({
          id: machine.id,
          name: patch.name ?? machine.name,
          sshHost: patch.sshHost ?? machine.sshHost,
          sshIdentityFile: patch.sshIdentityFile ?? machine.sshIdentityFile ?? "",
          sshPassword: "",
          sshPasswordSaved: machine.sshPasswordSaved === true,
          sshPort: patch.sshPort ?? (machine.sshPort ? String(machine.sshPort) : ""),
          sshUser: patch.sshUser ?? machine.sshUser ?? "",
        });
      })
      .filter((machine): machine is RemoteMachineSettings => Boolean(machine));
    onChange(normalizeRemoteMachineSettings(nextMachines));
  };

  const addRemoteMachine = () => {
    const machine = normalizeRemoteMachineDraft({
      ...newMachine,
      id: `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
    });
    if (!machine) {
      return;
    }
    onChange(normalizeRemoteMachineSettings([...remoteMachines, machine]));
    setNewMachine(createRemoteMachineDraft());
  };

  const removeRemoteMachine = (machineId: string) => {
    onChange(remoteMachines.filter((machine) => machine.id !== machineId));
  };

  const saveRemoteMachinePassword = (machine: RemoteMachineSettings) => {
    const password = sshPasswordDrafts[machine.id] ?? "";
    if (!password && machine.sshPasswordSaved !== true) {
      return;
    }
    /*
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * The Remote settings password field is a transient entry box. Send the
     * password only when the user presses the save-icon button, then clear the
     * React draft so the settings JSON and modal state never retain the secret.
     */
    vscode?.postMessage({
      password,
      remoteMachineId: machine.id,
      type: "saveRemoteMachinePassword",
    });
    setSshPasswordDrafts((drafts) => ({
      ...drafts,
      [machine.id]: "",
    }));
  };

  const canAddMachine = newMachine.name.trim().length > 0 && newMachine.sshHost.trim().length > 0;

  return (
    <div className="settings-tab-scroll scroll-mask-y" ref={containerRef}>
      <div className="settings-management-layout">
        <header className="settings-management-header">
          <div className="settings-management-header-text">
            <h3 className="settings-management-heading">Remote machines</h3>
            <p className="settings-management-description">
              Saved SSH machines appear as separate sidebar sections.
            </p>
          </div>
          <Popover onOpenChange={setIsTailscaleHelpOpen} open={isTailscaleHelpOpen}>
              <PopoverTrigger
                render={
                  <Button
                    className="settings-management-help-button"
                    size="sm"
                    type="button"
                    variant="outline"
                  />
                }
              >
                <IconInfoCircle aria-hidden="true" data-icon="inline-start" />
                Tailscale setup
              </PopoverTrigger>
            <PopoverContent
              align="end"
              className="w-80 max-w-[calc(100vw-2rem)] gap-3 p-4"
              onOpenAutoFocus={(event) => event.preventDefault()}
              side="top"
              sideOffset={8}
            >
              {/*
               * CDXC:RemoteMachines 2026-06-08-18:47:
               * Tailscale setup help should be a compact popover above Remote Machine settings, not a full modal, because it is contextual guidance for filling the SSH host rather than a blocking workflow.
               *
               * CDXC:RemoteMachines 2026-06-12-05:42:
               * The Remote machines header stacks the title over its muted subtitle on the left and pins Tailscale setup as an outline button on the right edge, so the contextual help reads as a real action opposite the header rather than a faint control wedged beside the subtitle.
               */}
              <PopoverHeader>
                <PopoverTitle className="text-sm">Tailscale setup</PopoverTitle>
                <PopoverDescription className="text-xs leading-5">
                  Use Tailscale when the remote machine is not reachable on your local network.
                </PopoverDescription>
              </PopoverHeader>
              <ol className="flex list-decimal flex-col gap-2 pl-5 text-xs leading-5 text-muted-foreground">
                <li>Install Tailscale on this Mac and sign in.</li>
                <li>Install Tailscale on the remote machine and sign in to the same tailnet.</li>
                <li>Confirm both machines are connected in Tailscale.</li>
                <li>Use the remote machine's Tailscale DNS name or Tailscale IP as the SSH host.</li>
              </ol>
              <p className="text-xs leading-5 text-muted-foreground">
                Ghostex still connects with SSH only; no Tailscale tokens or remote gxserver listener are required.
              </p>
            </PopoverContent>
          </Popover>
        </header>

        <div className="settings-management-list settings-remote-machine-list">
          {/*
           * CDXC:RemoteMachines 2026-06-12-05:42:
           * Add remote machine is the fixed first grid item (top-left), saved machines fill the remaining slots and wrap to new rows, and the empty placeholder occupies the slot beside the add card so the Remote tab always reads as a single uniform grid.
           *
           * CDXC:RemoteMachines 2026-06-02-23:47:
           * Remote settings require a human name and SSH host before saving because the sidebar section title comes from this user label and v1 remote connections support SSH only.
           */}
          <Card className="settings-remote-machine-card settings-remote-machine-add-card" size="sm">
            <div className="settings-remote-machine-summary settings-remote-machine-add-summary">
              <span aria-hidden="true" className="settings-remote-machine-add-icon">
                <IconPlus size={16} />
              </span>
              <CardTitle className="settings-remote-machine-add-title">Add remote machine</CardTitle>
            </div>
            <CardContent className="settings-remote-machine-body">
              <RemoteMachineFields
                draft={newMachine}
                hidePasswordField
                onChange={(patch) => setNewMachine((draft) => ({ ...draft, ...patch }))}
              />
              <div className="settings-management-actions settings-remote-machine-add-actions">
                <Button disabled={!canAddMachine} onClick={addRemoteMachine} type="button">
                  <IconPlus aria-hidden="true" />
                  Add Machine
                </Button>
              </div>
            </CardContent>
          </Card>
          {remoteMachines.length === 0 ? (
            <div className="settings-remote-machine-empty">
              <span aria-hidden="true" className="settings-remote-machine-empty-icon">
                <IconDeviceDesktop size={18} />
              </span>
              <span className="settings-remote-machine-empty-text">
                <span className="settings-remote-machine-empty-title">No machines yet</span>
                <span className="settings-remote-machine-empty-hint">
                  Add one to reach it over SSH from the sidebar.
                </span>
              </span>
            </div>
          ) : (
            remoteMachines.map((machine) => (
              <Card
                className="settings-remote-machine-card"
                data-settings-remote-machine-id={machine.id}
                key={machine.id}
                size="sm"
              >
                <div className="settings-remote-machine-summary settings-management-row">
                  <span className="settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted">
                    <IconDeviceDesktop aria-hidden="true" />
                  </span>
                  <span className="settings-management-main min-w-0 flex-1">
                    <span className="settings-management-title">{machine.name}</span>
                    <span className="settings-management-detail">{formatRemoteMachineSshTarget(machine)}</span>
                  </span>
                  <span className="settings-management-row-actions">
                    <Button
                      aria-label={`Remove ${machine.name}`}
                      onClick={() => removeRemoteMachine(machine.id)}
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconTrash aria-hidden="true" />
                    </Button>
                  </span>
                </div>
                <CardContent className="settings-remote-machine-body">
                  <RemoteMachineFields
                    draft={{
                      id: machine.id,
                      name: machine.name,
                      sshHost: machine.sshHost,
                      sshIdentityFile: machine.sshIdentityFile ?? "",
                      sshPassword: sshPasswordDrafts[machine.id] ?? "",
                      sshPasswordSaved: machine.sshPasswordSaved === true,
                      sshPort: machine.sshPort ? String(machine.sshPort) : "",
                      sshUser: machine.sshUser ?? "",
                    }}
                    onChange={(patch) => updateRemoteMachine(machine.id, patch)}
                    onPasswordSave={() => saveRemoteMachinePassword(machine)}
                    passwordSaveDisabled={!vscode}
                  />
                </CardContent>
              </Card>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function RemoteMachineFields({
  draft,
  hidePasswordField = false,
  onChange,
  onPasswordSave,
  passwordSaveDisabled = false,
}: {
  draft: RemoteMachineDraft;
  hidePasswordField?: boolean;
  onChange: (patch: Partial<RemoteMachineDraft>) => void;
  onPasswordSave?: () => void;
  passwordSaveDisabled?: boolean;
}) {
  const canSavePassword =
    !passwordSaveDisabled &&
    typeof onPasswordSave === "function" &&
    (draft.sshPassword.trim().length > 0 || draft.sshPasswordSaved);
  return (
    <FieldGroup className="settings-remote-machine-fields">
      <Field className="settings-remote-machine-field">
        <FieldLabel className="settings-remote-machine-field-label">Name</FieldLabel>
        <Input
          aria-label="Remote machine name"
          className="settings-remote-machine-input"
          maxLength={80}
          onChange={(event) => onChange({ name: event.currentTarget.value })}
          placeholder="Machine one"
          value={draft.name}
        />
      </Field>
      <Field className="settings-remote-machine-field">
        <FieldLabel className="settings-remote-machine-field-label">SSH host</FieldLabel>
        <Input
          aria-label="Remote machine SSH host"
          className="settings-remote-machine-input"
          maxLength={200}
          onChange={(event) => onChange({ sshHost: event.currentTarget.value })}
          placeholder="100.77.81.4"
          value={draft.sshHost}
        />
      </Field>
      <div className="settings-remote-machine-user-port">
        <Field className="settings-remote-machine-field">
          <FieldLabel className="settings-remote-machine-field-label">SSH user</FieldLabel>
          <Input
            aria-label="Remote machine SSH user"
            className="settings-remote-machine-input"
            maxLength={120}
            onChange={(event) => onChange({ sshUser: event.currentTarget.value })}
            placeholder="madda"
            value={draft.sshUser}
          />
        </Field>
        <Field className="settings-remote-machine-field">
          <FieldLabel className="settings-remote-machine-field-label">SSH port</FieldLabel>
          <Input
            aria-label="Remote machine SSH port"
            className="settings-remote-machine-input"
            inputMode="numeric"
            maxLength={5}
            onChange={(event) => onChange({ sshPort: event.currentTarget.value.replace(/[^0-9]/gu, "") })}
            placeholder="22"
            value={draft.sshPort}
          />
        </Field>
      </div>
      <Field className="settings-remote-machine-field">
        <FieldLabel className="settings-remote-machine-field-label">Identity file</FieldLabel>
        <Input
          aria-label="Remote machine SSH identity file"
          className="settings-remote-machine-input"
          maxLength={500}
          onChange={(event) => onChange({ sshIdentityFile: event.currentTarget.value })}
          placeholder="~/.ssh/id_ed25519"
          value={draft.sshIdentityFile}
        />
        <FieldDescription className="settings-remote-machine-field-description">
          {hidePasswordField
            ? "Provide an SSH identity file now, or add the machine and save an SSH password from its card."
            : "Provide either an SSH identity file or save an SSH password below."}
        </FieldDescription>
      </Field>
      {!hidePasswordField ? (
        <Field className="settings-remote-machine-field">
          <FieldLabel className="settings-remote-machine-field-label">Password</FieldLabel>
          <div className="settings-remote-machine-password-row">
            <Input
              aria-label="Remote machine SSH password"
              autoComplete="off"
              className="settings-remote-machine-input"
              maxLength={500}
              onChange={(event) => onChange({ sshPassword: event.currentTarget.value })}
              placeholder={draft.sshPasswordSaved ? "Saved in Keychain" : "SSH password"}
              type="password"
              value={draft.sshPassword}
            />
            <Button
              aria-label="Save SSH password"
              disabled={!canSavePassword}
              onClick={onPasswordSave}
              size="icon-sm"
              type="button"
              variant="secondary"
            >
              <IconDeviceFloppy aria-hidden="true" />
            </Button>
          </div>
          <FieldDescription className="settings-remote-machine-field-description">
            Passwords are stored in macOS Keychain. Leave blank and press Save to remove a saved password.
          </FieldDescription>
        </Field>
      ) : null}
    </FieldGroup>
  );
}

function normalizeRemoteMachineDraft(
  draft: RemoteMachineDraft & { id: string },
): RemoteMachineSettings | undefined {
  return normalizeRemoteMachineSettings([
    {
      id: draft.id,
      name: draft.name,
      sshHost: draft.sshHost,
      sshIdentityFile: draft.sshIdentityFile,
      sshPasswordSaved: draft.sshPasswordSaved,
      sshPort: draft.sshPort ? Number(draft.sshPort) : undefined,
      sshUser: draft.sshUser,
    },
  ])[0];
}

function formatRemoteMachineSshTarget(machine: RemoteMachineSettings): string {
  const host = machine.sshUser ? `${machine.sshUser}@${machine.sshHost}` : machine.sshHost;
  return machine.sshPort ? `${host}:${machine.sshPort}` : host;
}

function ProjectsSettingsPanel({
  projects,
  vscode,
}: {
  projects: SidebarProjectSettingsItem[];
  vscode?: WebviewApi;
}) {
  const [selectedProjectId, setSelectedProjectId] = useState(projects[0]?.projectId ?? "");
  const selectedProject =
    projects.find((project) => project.projectId === selectedProjectId) ?? projects[0];
  const [command, setCommand] = useState(selectedProject?.worktreeCommand ?? "");
  const [beadsDisplayKey, setBeadsDisplayKey] = useState(selectedProject?.beadsDisplayKey ?? "");

  useEffect(() => {
    if (!projects.some((project) => project.projectId === selectedProjectId)) {
      setSelectedProjectId(projects[0]?.projectId ?? "");
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    setCommand(selectedProject?.worktreeCommand ?? "");
    setBeadsDisplayKey(selectedProject?.beadsDisplayKey ?? "");
  }, [selectedProject?.beadsDisplayKey, selectedProject?.projectId, selectedProject?.worktreeCommand]);

  const saveCommand = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      command,
      projectId: selectedProject.projectId,
      type: "setProjectWorktreeCommand",
    });
  };

  const saveBeadsDisplayKey = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      displayKey: beadsDisplayKey,
      projectId: selectedProject.projectId,
      type: "setProjectBeadsDisplayKey",
    });
  };

  if (projects.length === 0) {
    return (
      <div className="settings-tab-scroll scroll-mask-y">
        <Empty>
          <EmptyHeader>
            <EmptyTitle>No projects</EmptyTitle>
            <EmptyDescription>Main projects will appear here.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      </div>
    );
  }

  return (
    <div className="settings-tab-scroll scroll-mask-y">
      {/*
       * CDXC:Worktrees 2026-05-18-23:07:
       * Main projects can store a setup command that runs inside every new worktree before the selected agent receives the first prompt. Keep worktree projects out of this list because they inherit from their parent project.
       */}
      <div className="projects-settings-layout">
        <div className="projects-settings-list" role="list">
          {projects.map((project) => (
            <button
              aria-pressed={selectedProject?.projectId === project.projectId}
              className="settings-management-row projects-settings-project"
              data-selected={String(selectedProject?.projectId === project.projectId)}
              key={project.projectId}
              onClick={() => setSelectedProjectId(project.projectId)}
              type="button"
            >
              <span className="settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted">
                <IconFolderOpen aria-hidden="true" />
              </span>
              <span className="settings-management-main min-w-0">
                <span className="settings-management-title">{project.name}</span>
                <span className="settings-management-detail">{project.path}</span>
              </span>
            </button>
          ))}
        </div>
        <Card className="settings-project-command-card">
          <CardContent className="flex flex-col gap-4 p-4">
            {/*
              CDXC:ProjectBoard 2026-05-23-14:35:
              Projects settings owns the three-letter ticket key shown on the board (for example ZMX-12) while Beads keeps hash ids internally.
            */}
            <FieldGroup>
              <Field>
                <FieldLabel>Ticket key</FieldLabel>
                <Input
                  aria-label="Ticket key"
                  maxLength={3}
                  onChange={(event) =>
                    setBeadsDisplayKey(event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ""))
                  }
                  placeholder="ZMX"
                  value={beadsDisplayKey}
                />
                <FieldDescription>
                  Three-letter prefix used for Linear-style ticket numbers on the Project board.
                </FieldDescription>
              </Field>
            </FieldGroup>
            <div className="settings-management-actions">
              <Button onClick={() => setBeadsDisplayKey("")} type="button" variant="outline">
                Clear
              </Button>
              <Button onClick={saveBeadsDisplayKey} type="button">
                Save Ticket Key
              </Button>
            </div>
            <FieldGroup>
              <Field>
                <FieldLabel>Worktree command</FieldLabel>
                <Textarea
                  aria-label="Worktree command"
                  className="settings-project-command-textarea"
                  onChange={(event) => setCommand(event.currentTarget.value)}
                  placeholder="bun install"
                  value={command}
                />
                <FieldDescription>
                  Runs in the new worktree folder before the project is added (Useful for .envs/installing dependencies/etc.)
                </FieldDescription>
              </Field>
            </FieldGroup>
            <div className="settings-management-actions">
              <Button onClick={() => setCommand("")} type="button" variant="outline">
                Clear
              </Button>
              <Button onClick={saveCommand} type="button">
                Save Command
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

type SettingsAgentDragData = {
  agentId: string;
  kind: "settings-agent";
};

type SettingsCommandDragData = {
  commandId: string;
  kind: "settings-command";
};

function OpenTargetsSettingsTab({
  onChange,
  settings,
}: {
  onChange: (settings: ghostexSettings) => void;
  settings: ghostexSettings;
}) {
  const [editorState, setEditorState] = useState<SettingsOpenTargetEditorState>();
  const hiddenIds = new Set(settings.workspaceOpenTargetHiddenIds);
  /**
   * CDXC:TitlebarOpenIn 2026-05-11-02:03
   * Settings shows installed built-ins as toggleable and unavailable built-ins
   * as disabled rows. Turning an installed target off writes only hidden ids,
   * so the startup scan can refresh availability without undoing that choice.
   */
  const availableBuiltInIds = new Set(settings.workspaceOpenTargetAvailability.availableTargetIds);

  const updateHiddenTarget = (targetId: string, isVisible: boolean) => {
    const nextHiddenIds = new Set(settings.workspaceOpenTargetHiddenIds);
    if (isVisible) {
      nextHiddenIds.delete(targetId);
    } else {
      nextHiddenIds.add(targetId);
    }
    onChange({
      ...settings,
      workspaceOpenTargetHiddenIds: normalizeWorkspaceOpenTargetHiddenIds([...nextHiddenIds]),
    });
  };

  const saveCustomTarget = () => {
    if (!editorState) {
      return;
    }
    const label = editorState.draft.label.trim();
    const command = editorState.draft.command.trim();
    if (!label || !command) {
      return;
    }
    const nextTarget: CustomWorkspaceOpenTarget = {
      args: editorState.draft.argsText
        .split("\n")
        .map((arg) => arg.trim())
        .filter(Boolean),
      command,
      id:
        editorState.id ??
        `${CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX}${createWorkspaceOpenTargetSlug(label)}-${Date.now().toString(36)}`,
      label,
    };
    const existingTargets = settings.customWorkspaceOpenTargets.filter(
      (target) => target.id !== editorState.id,
    );
    onChange({
      ...settings,
      customWorkspaceOpenTargets: normalizeCustomWorkspaceOpenTargets([
        ...existingTargets,
        nextTarget,
      ]),
    });
    setEditorState(undefined);
  };

  const removeCustomTarget = (targetId: string) => {
    onChange({
      ...settings,
      customWorkspaceOpenTargets: settings.customWorkspaceOpenTargets.filter(
        (target) => target.id !== targetId,
      ),
    });
  };

  return (
    <ScrollArea className="h-full min-h-0">
      <div className="flex flex-col gap-6 px-5 pb-5">
        <SettingsSection title="Open In">
          {/* CDXC:TitlebarOpenIn 2026-05-11-00:22
              Users need a Settings tab opened from the titlebar dropdown to
              show or hide IDE targets and add custom project-open commands.

              CDXC:TitlebarOpenIn 2026-05-16-23:24
              Settings must show the same Open In editor icons as the titlebar
              dropdown so users can scan Cursor, VS Code variants, Zed,
              Antigravity, VSCodium, and JetBrains-family targets by brand. */}
          <div className="flex flex-col gap-2">
            {BUILT_IN_WORKSPACE_OPEN_TARGETS.map((target) => {
              const isAvailable = target.id === "finder" || availableBuiltInIds.has(target.id);
              return (
                <div
                  className="flex items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2"
                  key={target.id}
                >
                  <div className="flex min-w-0 flex-1 items-center gap-3">
                    <OpenTargetSettingsIcon targetId={target.id} />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">{target.label}</div>
                      <div className="truncate text-xs text-muted-foreground">
                        {isAvailable
                          ? target.id === "finder"
                            ? "Built-in"
                            : target.commands?.join(", ") ?? "macOS"
                          : "Not installed"}
                      </div>
                    </div>
                  </div>
                  <Switch
                    checked={isAvailable && !hiddenIds.has(target.id)}
                    disabled={!isAvailable}
                    onCheckedChange={(checked) => updateHiddenTarget(target.id, checked)}
                  />
                </div>
              );
            })}
          </div>
        </SettingsSection>

        <SettingsSection title="Custom Open Targets">
          <div className="flex flex-col gap-2">
            {settings.customWorkspaceOpenTargets.map((target) => (
              <div
                className="flex items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2"
                key={target.id}
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{target.label}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {[target.command, ...target.args].join(" ")}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    onClick={() =>
                      setEditorState({
                        draft: {
                          argsText: target.args.join("\n"),
                          command: target.command,
                          label: target.label,
                        },
                        id: target.id,
                      })
                    }
                    size="icon-xs"
                    type="button"
                    variant="ghost"
                  >
                    <IconPencil aria-hidden="true" size={14} />
                    <span className="sr-only">Edit</span>
                  </Button>
                  <Button
                    onClick={() => removeCustomTarget(target.id)}
                    size="icon-xs"
                    type="button"
                    variant="ghost"
                  >
                    <IconTrash aria-hidden="true" size={14} />
                    <span className="sr-only">Remove</span>
                  </Button>
                </div>
              </div>
            ))}
            {editorState ? (
              <div className="flex flex-col gap-3 rounded-none border border-border/70 bg-card/40 p-3">
                <Input
                  aria-label="Open target name"
                  onChange={(event) =>
                    setEditorState({
                      ...editorState,
                      draft: { ...editorState.draft, label: event.currentTarget.value },
                    })
                  }
                  placeholder="Name"
                  value={editorState.draft.label}
                />
                <Input
                  aria-label="Open target command"
                  onChange={(event) =>
                    setEditorState({
                      ...editorState,
                      draft: { ...editorState.draft, command: event.currentTarget.value },
                    })
                  }
                  placeholder="Command"
                  value={editorState.draft.command}
                />
                <Textarea
                  aria-label="Open target arguments"
                  onChange={(event) =>
                    setEditorState({
                      ...editorState,
                      draft: { ...editorState.draft, argsText: event.currentTarget.value },
                    })
                  }
                  placeholder="Optional arguments, one per line"
                  value={editorState.draft.argsText}
                />
                <div className="flex justify-end gap-2">
                  <Button onClick={() => setEditorState(undefined)} type="button" variant="ghost">
                    Cancel
                  </Button>
                  <Button onClick={saveCustomTarget} type="button">
                    Save
                  </Button>
                </div>
              </div>
            ) : (
              <Button
                className="w-fit"
                onClick={() =>
                  setEditorState({ draft: { argsText: "", command: "", label: "" } })
                }
                type="button"
                variant="outline"
              >
                <IconPlus aria-hidden="true" size={16} />
                Add target
              </Button>
            )}
          </div>
        </SettingsSection>
      </div>
    </ScrollArea>
  );
}

function OpenTargetSettingsIcon({ targetId }: { targetId: string }) {
  if (targetId === "finder") {
    return (
      <IconFolderOpen
        aria-hidden="true"
        className="settings-open-target-icon text-muted-foreground"
      />
    );
  }
  const icon = getEditorBrandIconId(targetId);
  if (icon) {
    return <EditorBrandIcon className="settings-open-target-icon" icon={icon} />;
  }
  return (
    <IconCodeDots aria-hidden="true" className="settings-open-target-icon text-muted-foreground" />
  );
}

function OSIntegrationSettingsTab({
  loading,
  onRequestStatus,
  onSetDefaults,
  status,
}: {
  loading?: boolean;
  onRequestStatus?: () => void;
  onSetDefaults?: (target: "editor" | "terminalLinks" | "scriptRunner" | "all") => void;
  status?: SidebarOSIntegrationStatusMessage;
}) {
  const ghostexBundleId = status?.bundleIdentifier;
  const editorDefaultCount =
    status && ghostexBundleId
      ? Object.values(status.editorDefaults).filter((bundleId) => bundleId === ghostexBundleId)
          .length
      : 0;
  const scriptDefaultCount =
    status && ghostexBundleId
      ? Object.values(status.scriptDefaults).filter((bundleId) => bundleId === ghostexBundleId)
          .length
      : 0;
  const terminalDefault =
    Boolean(status?.terminalLinkDefaultBundleId && status.terminalLinkDefaultBundleId === ghostexBundleId);
  return (
    <ScrollArea className="h-full min-h-0">
      <div className="flex flex-col gap-6 px-5 pb-5">
        <SettingsSection title="Defaults">
          {/*
           * CDXC:OSIntegration 2026-05-27-18:06:
           * Ghostex registers as an available macOS editor and script handler
           * at install/build time, but Settings is the only place that changes
           * default editor, terminal-link, or script-runner ownership.
           */}
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Button
              className="h-10 justify-start px-4"
              disabled={!onSetDefaults}
              onClick={() => onSetDefaults?.("editor")}
              type="button"
              variant="outline"
            >
              <IconCodeDots aria-hidden="true" data-icon="inline-start" />
              Set as Default Editor
            </Button>
            <Button
              className="h-10 justify-start px-4"
              disabled={!onSetDefaults}
              onClick={() => onSetDefaults?.("terminalLinks")}
              type="button"
              variant="outline"
            >
              <IconTerminal2 aria-hidden="true" data-icon="inline-start" />
              Set Terminal Links
            </Button>
            <Button
              className="h-10 justify-start px-4"
              disabled={!onSetDefaults}
              onClick={() => onSetDefaults?.("scriptRunner")}
              type="button"
              variant="outline"
            >
              <IconPlayerPlay aria-hidden="true" data-icon="inline-start" />
              Set Script Runner
            </Button>
            <Button
              className="h-10 justify-start px-4"
              disabled={!onSetDefaults}
              onClick={() => onSetDefaults?.("all")}
              type="button"
            >
              <IconCircleCheckFilled aria-hidden="true" data-icon="inline-start" />
              Set All
            </Button>
          </div>
        </SettingsSection>

        <SettingsSection title="CLI">
          <div className="grid gap-2 rounded-none border border-border bg-muted/20 p-3 font-mono text-xs text-muted-foreground">
            <div>ghostex open ./folder</div>
            <div>ghostex edit --wait file.ts:12:3</div>
            <div>ghostex terminal --cwd /tmp --title Scratch -- echo hi</div>
            <div>ghostex ./file.txt</div>
          </div>
        </SettingsSection>

        <SettingsSection title="Diagnostics">
          <div className="flex flex-col gap-3 rounded-none border border-border bg-muted/20 p-3 text-sm text-muted-foreground">
            <div className="flex items-center justify-between gap-3">
              <span>{loading && !status ? "Checking macOS handlers..." : "macOS handler status"}</span>
              <Button
                className="h-8 px-3"
                disabled={loading || !onRequestStatus}
                onClick={onRequestStatus}
                type="button"
                variant="outline"
              >
                <IconRefresh aria-hidden="true" data-icon="inline-start" />
                Refresh
              </Button>
            </div>
            {status ? (
              <div className="grid gap-2">
                <OSIntegrationDiagnosticRow
                  label="Available editor"
                  value={status.registeredEditableFiles ? "Registered" : "Missing"}
                />
                <OSIntegrationDiagnosticRow
                  label="Available script runner"
                  value={status.registeredScriptRunner ? "Registered" : "Missing"}
                />
                <OSIntegrationDiagnosticRow
                  label="ghostex:// links"
                  value={
                    status.registeredGhostexURLScheme
                      ? terminalDefault
                        ? "Default"
                        : `Default: ${status.terminalLinkDefaultBundleId ?? "None"}`
                      : "Missing"
                  }
                />
                <OSIntegrationDiagnosticRow
                  label="Editor defaults"
                  value={`${editorDefaultCount}/${Object.keys(status.editorDefaults).length} sampled`}
                />
                <OSIntegrationDiagnosticRow
                  label="Script defaults"
                  value={`${scriptDefaultCount}/${Object.keys(status.scriptDefaults).length} sampled`}
                />
              </div>
            ) : (
              <div>Ghostex has not checked Launch Services yet.</div>
            )}
          </div>
        </SettingsSection>
      </div>
    </ScrollArea>
  );
}

function OSIntegrationDiagnosticRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span>{label}</span>
      <span className="text-right font-medium text-foreground">{value}</span>
    </div>
  );
}

const AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS = DEFAULT_SIDEBAR_AGENTS.filter(
  (agent) => agent.agentId !== "t3",
);
const AGENT_TYPE_SELECT_ITEMS = [
  { label: "Custom", value: "custom" },
  ...DEFAULT_SIDEBAR_AGENTS.map((agent) => ({
    label: agent.name,
    value: agent.icon,
  })),
];

function getCuaPermissionStatus(
  ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined,
  ghostexCliStatusLoading: boolean,
): { status: string; tone: "success" | "warning" | "neutral" } {
  if (ghostexCliStatusLoading && !ghostexCliStatus) {
    return { status: "Checking", tone: "neutral" };
  }
  if (ghostexCliStatus?.cuaDriverInstalled !== true) {
    return { status: "Driver Not Installed", tone: "warning" };
  }

  const accessibilityGranted = ghostexCliStatus.cuaDriverAccessibilityPermissionGranted;
  const screenRecordingGranted = ghostexCliStatus.cuaDriverScreenRecordingPermissionGranted;
  if (accessibilityGranted === true && screenRecordingGranted === true) {
    return { status: "Permissions Allowed", tone: "success" };
  }
  if (accessibilityGranted === false && screenRecordingGranted === false) {
    return { status: "Permissions Off - Open Settings", tone: "warning" };
  }
  if (accessibilityGranted === false) {
    return { status: "Accessibility Off - Open Settings", tone: "warning" };
  }
  if (screenRecordingGranted === false) {
    return { status: "Screen Recording Off - Open Settings", tone: "warning" };
  }
  if (accessibilityGranted === true) {
    return { status: "Screen Recording Unknown", tone: "warning" };
  }
  if (screenRecordingGranted === true) {
    return { status: "Accessibility Unknown", tone: "warning" };
  }
  return { status: "Permission Status Unknown", tone: "warning" };
}

function IntegrationsSettingsTab({
  agentHookStatus,
  agentHookStatusLoading,
  appShotsEnabled,
  appShotsHotkey,
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onAppShotsEnabledChange,
  onAppShotsHotkeyChange,
  onInstallAgentOrchestrationSkill,
  onInstallAgentHooks,
  onInstallBrowserControl,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallGenerateTitleSkill,
  onInstallGhostexCli,
  onOpenAccessibilityPreferences,
  onOpenFirstLaunchSetup,
  onOpenScreenRecordingPreferences,
  onRequestAgentHookStatus,
  onRequestGhostexCliStatus,
}: {
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentHookStatusLoading: boolean;
  appShotsEnabled: boolean;
  appShotsHotkey: AppShotsHotkey;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onAppShotsEnabledChange: (checked: boolean) => void;
  onAppShotsHotkeyChange: (hotkey: AppShotsHotkey) => void;
  onInstallAgentOrchestrationSkill?: () => void;
  onInstallAgentHooks?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGhostexCli?: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenFirstLaunchSetup?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onRequestAgentHookStatus?: () => void;
  onRequestGhostexCliStatus?: () => void;
}) {
  const installedHookCount =
    agentHookStatus?.agents.filter(
      (status) => status.status === "installed" || status.status === "notRequired",
    ).length ?? 0;
  const updateRequiredHookCount =
    agentHookStatus?.agents.filter((status) => status.status === "updateRequired").length ?? 0;
  const updateRequiredHookSummary =
    updateRequiredHookCount === 1 ? "1 needs update" : `${updateRequiredHookCount} need update`;
  const hookSummary = agentHookStatus
    ? agentHookStatus.errorMessage
      ? "Unable to check"
      : updateRequiredHookCount > 0
        ? updateRequiredHookSummary
        : `${installedHookCount}/${AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.length} installed`
    : agentHookStatusLoading
      ? "Checking"
      : "Not checked";
  const cliReady = ghostexCliStatus?.installed === true;
  const desktopControlReady =
    ghostexCliStatus?.cuaDriverInstalled === true &&
    ghostexCliStatus?.computerUseSkillInstalled === true;
  const t3RuntimeReady = ghostexCliStatus?.t3RuntimeInstalled === true;
  const t3RuntimeStatus =
    ghostexCliStatusLoading && !ghostexCliStatus
      ? "Checking"
      : t3RuntimeReady
        ? ghostexCliStatus?.t3RuntimeSource === "development"
          ? "Development"
          : "Bundled"
        : "Missing";
  const t3RuntimeDescription =
    ghostexCliStatus?.t3RuntimeDetail ??
    "T3 Code should be packaged with Ghostex so GUI coding panes can start without a developer checkout.";
  /**
   * CDXC:CuaPermissions 2026-05-29-06:00:
   * Cua Permissions status must be based on Cua Driver's own permission check,
   * because granting Cua Driver in macOS can still leave Ghostex's separate
   * Accessibility trust bit false. The row represents desktop automation
   * readiness for agents, not Ghostex's ability to synthesize input.
   */
  const cuaPermissionStatus = getCuaPermissionStatus(ghostexCliStatus, ghostexCliStatusLoading);

  return (
    <ScrollArea className="h-full min-h-0">
      <div className="flex flex-col gap-6 px-5 pb-5">
        {/*
         * CDXC:IntegrationsSetup 2026-05-27-04:17:
         * Settings owns one Integrations tab for post-onboarding setup. Keep
         * CLI, bundled Ghostex skills, agent hooks, Cua Driver, and macOS privacy
         * permissions on the same page so users can recover skipped first-launch
         * steps without hunting through unrelated tabs.
         *
         * CDXC:AgentSkills 2026-05-31-09:18:
         * Bundled Ghostex skills are explicit per-skill installs in Settings,
         * not hidden side effects of CLI setup. Each row explains what the skill
         * teaches agents and remains disabled until the Ghostex CLI is present.
         *
         * CDXC:CliInstall 2026-06-07-13:53:
         * Ghostex installs and repairs the app-bundled CLI automatically for
         * DMG and Homebrew installs. Settings should expose a manual Repair CLI
         * action for unusual PATH states, not a cask reinstall flow.
         */}
        <SettingsSection title="Integrations">
          <IntegrationSettingsRow
            description="Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup. gx is linked when that alias is available and not taken by another command."
            icon={IconTerminal2}
            status={ghostexCliStatusLoading && !ghostexCliStatus ? "Checking" : cliReady ? "Installed" : "Not installed"}
            tone={cliReady ? "success" : "warning"}
            title="Ghostex CLI"
          >
            <Button
              disabled={ghostexCliStatusLoading || !onInstallGhostexCli}
              onClick={onInstallGhostexCli}
              type="button"
              variant={cliReady ? "outline" : "default"}
            >
              <IconDownload aria-hidden="true" data-icon="inline-start" />
              Repair CLI
            </Button>
            <Button
              disabled={ghostexCliStatusLoading || !onRequestGhostexCliStatus}
              onClick={onRequestGhostexCliStatus}
              type="button"
              variant="ghost"
            >
              <IconRefresh aria-hidden="true" data-icon="inline-start" />
              Refresh
            </Button>
          </IntegrationSettingsRow>

          {/*
           * CDXC:T3CodePackaging 2026-06-06-05:50:
           * T3 Code panes are a core advertised Ghostex feature, so Settings -> Integrations must show whether the app build actually contains the managed T3 runtime instead of leaving users to discover a missing Web/t3code-server package through a pane startup failure.
           */}
          <IntegrationSettingsRow
            description={t3RuntimeDescription}
            icon={IconCodeDots}
            status={t3RuntimeStatus}
            tone={t3RuntimeReady ? "success" : "warning"}
            title="T3 Code Runtime"
          >
            <Button
              disabled={ghostexCliStatusLoading || !onRequestGhostexCliStatus}
              onClick={onRequestGhostexCliStatus}
              type="button"
              variant="ghost"
            >
              <IconRefresh aria-hidden="true" data-icon="inline-start" />
              Refresh
            </Button>
          </IntegrationSettingsRow>

          <BundledAgentSkillsPanel
            ghostexCliStatus={ghostexCliStatus}
            ghostexCliStatusLoading={ghostexCliStatusLoading}
            onInstallSkill={{
              agentOrchestration: onInstallAgentOrchestrationSkill,
              browserUse: onInstallBrowserControl,
              computerUse: onInstallComputerUseSkill,
              generateTitle: onInstallGenerateTitleSkill,
            }}
            onRefreshStatus={onRequestGhostexCliStatus}
          />

          <IntegrationSettingsRow
            description="Install agent hooks for supported CLIs so Ghostex can show In Progress and Needs Attention notifications and name sessions from the first message."
            icon={IconTools}
            status={hookSummary}
            tone={installedHookCount > 0 ? "success" : "warning"}
            title="Agent Hooks"
          >
            <Button
              disabled={agentHookStatusLoading || !onInstallAgentHooks}
              onClick={onInstallAgentHooks}
              type="button"
              variant={installedHookCount > 0 ? "outline" : "default"}
            >
              <IconDownload aria-hidden="true" data-icon="inline-start" />
              {updateRequiredHookCount > 0 ? "Update Hooks" : "Install Hooks"}
            </Button>
            <Button
              disabled={agentHookStatusLoading || !onRequestAgentHookStatus}
              onClick={onRequestAgentHookStatus}
              type="button"
              variant="ghost"
            >
              <IconRefresh aria-hidden="true" data-icon="inline-start" />
              Refresh
            </Button>
          </IntegrationSettingsRow>

          {/*
           * CDXC:AppShots 2026-06-12-11:12:
           * Settings copy must describe App Shots as an agent-session feature because captured context now targets the focused or recent agent instead of Codex only.
           */}
          <IntegrationSettingsRow
            description="Capture the frontmost app window and available Accessibility text, then stage it in the focused or recent agent session as local image context."
            icon={IconDeviceDesktop}
            status={appShotsEnabled ? "Enabled" : "Disabled"}
            tone={appShotsEnabled ? "success" : "neutral"}
            title="App Shots"
          >
            <div className="flex min-w-[190px] flex-col gap-2 sm:items-end">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">Enabled</span>
                <Switch
                  aria-label="Enable App Shots"
                  checked={appShotsEnabled}
                  onCheckedChange={onAppShotsEnabledChange}
                />
              </div>
              <Select
                disabled={!appShotsEnabled}
                onValueChange={(value) => onAppShotsHotkeyChange(value as AppShotsHotkey)}
                value={appShotsHotkey}
              >
                <SelectTrigger aria-label="App Shots hotkey" className="w-[190px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {APP_SHOTS_HOTKEY_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
          </IntegrationSettingsRow>

          <IntegrationSettingsRow
            description="Install Cua Driver for native macOS desktop automation. The bundled Ghostex Computer Use skill above teaches agents when and how to use it."
            icon={IconDeviceDesktop}
            status={ghostexCliStatusLoading && !ghostexCliStatus ? "Checking" : desktopControlReady ? "Installed" : "Not installed"}
            tone={desktopControlReady ? "success" : "warning"}
            title="Desktop Control Runtime"
          >
            <Button
              disabled={ghostexCliStatusLoading || desktopControlReady || !onInstallCuaDriver}
              onClick={onInstallCuaDriver}
              type="button"
              variant={desktopControlReady ? "outline" : "default"}
            >
              <IconDownload aria-hidden="true" data-icon="inline-start" />
              {desktopControlReady ? "Installed" : "Install Desktop Control"}
            </Button>
          </IntegrationSettingsRow>

          <IntegrationSettingsRow
            description="Cua Driver needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop."
            icon={IconSettings}
            status={cuaPermissionStatus.status}
            tone={cuaPermissionStatus.tone}
            title="Cua Permissions"
          >
            <Button
              disabled={!onOpenAccessibilityPreferences}
              onClick={onOpenAccessibilityPreferences}
              type="button"
              variant="outline"
            >
              Accessibility
            </Button>
            <Button
              disabled={!onOpenScreenRecordingPreferences}
              onClick={onOpenScreenRecordingPreferences}
              type="button"
              variant="outline"
            >
              Screen Recording
            </Button>
          </IntegrationSettingsRow>

          <IntegrationSettingsRow
            description="Reopen the first-launch setup flow any time for the guided version of these integrations and app tips."
            icon={IconInfoCircle}
            status="Available"
            tone="neutral"
            title="Setup Flow"
          >
            <Button
              disabled={!onOpenFirstLaunchSetup}
              onClick={onOpenFirstLaunchSetup}
              type="button"
              variant="outline"
            >
              Open Setup Flow
            </Button>
          </IntegrationSettingsRow>
        </SettingsSection>
      </div>
    </ScrollArea>
  );
}

function IntegrationSettingsRow({
  children,
  description,
  icon: Icon,
  status,
  title,
  tone,
}: {
  children: ReactNode;
  description: string;
  icon: typeof IconInfoCircle;
  status: string;
  title: string;
  tone: "success" | "warning" | "neutral";
}) {
  return (
    <Field className="rounded-none border border-border bg-muted/20 px-4 py-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex min-w-0 gap-3">
          <span className="mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-none bg-muted text-muted-foreground">
            <Icon aria-hidden="true" size={17} />
          </span>
          <FieldContent>
            <div className="flex flex-wrap items-center gap-2">
              <FieldTitle className="text-sm">{title}</FieldTitle>
              <span
                className={cn(
                  "inline-flex rounded-none border px-2 py-0.5 text-[11px] font-semibold",
                  tone === "success" &&
                    "border-emerald-500/40 bg-emerald-500/10 text-emerald-300",
                  tone === "warning" && "border-amber-500/40 bg-amber-500/10 text-amber-200",
                  tone === "neutral" && "border-border bg-card text-muted-foreground",
                )}
              >
                {status}
              </span>
            </div>
            <FieldDescription className="text-xs text-muted-foreground">
              {description}
            </FieldDescription>
          </FieldContent>
        </div>
        <div className="flex shrink-0 flex-wrap gap-2 sm:justify-end">{children}</div>
      </div>
    </Field>
  );
}

function AgentsSettingsTab({
  agentHookStatus,
  agentHookStatusLoading,
  agentAcceptAllEnabled,
  customSessionTitleGenerationCommand,
  defaultPromptAgentId,
  sessionTitleGenerationAgent,
  onAgentAcceptAllEnabledChange,
  onCustomSessionTitleGenerationCommandChange,
  onDefaultPromptAgentIdChange,
  onInstallAgentHooks,
  onRequestAgentHookStatus,
  onSessionTitleGenerationAgentChange,
  vscode,
}: {
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentHookStatusLoading: boolean;
  agentAcceptAllEnabled: boolean;
  customSessionTitleGenerationCommand: string;
  defaultPromptAgentId: string;
  sessionTitleGenerationAgent: SessionTitleGenerationAgent;
  onAgentAcceptAllEnabledChange: (checked: boolean) => void;
  onCustomSessionTitleGenerationCommandChange: (command: string) => void;
  onDefaultPromptAgentIdChange: (agentId: string) => void;
  onInstallAgentHooks?: () => void;
  onRequestAgentHookStatus?: () => void;
  onSessionTitleGenerationAgentChange: (agent: SessionTitleGenerationAgent) => void;
  vscode?: WebviewApi;
}) {
  const agents = useSidebarStore((state) => state.hud.agents);
  const acceptAllToggleId = useId();
  const [editorState, setEditorState] = useState<SettingsAgentEditorState>();
  const [draftAgentIds, setDraftAgentIds] = useState<string[]>();

  useEffect(() => {
    setDraftAgentIds((previousDraft) => reconcileDraftIds(previousDraft, agents, "agentId"));
  }, [agents]);

  const orderedAgents = useMemo(() => {
    const agentById = new Map(agents.map((agent) => [agent.agentId, agent]));
    const orderedAgentIds = draftAgentIds
      ? mergeIds(
          draftAgentIds,
          agents.map((agent) => agent.agentId),
        )
      : agents.map((agent) => agent.agentId);

    return orderedAgentIds
      .map((agentId) => agentById.get(agentId))
      .filter((agent): agent is SidebarAgentButton => agent !== undefined);
  }, [agents, draftAgentIds]);
  const promptAgentOptions = useMemo(
    () =>
      agents
        .filter((agent) => agent.agentId !== "t3" && Boolean(agent.command?.trim()))
        .map((agent) => ({ label: agent.name.trim() || agent.agentId, value: agent.agentId })),
    [agents],
  );
  const selectedDefaultPromptAgentId = promptAgentOptions.some(
    (option) => option.value === defaultPromptAgentId,
  )
    ? defaultPromptAgentId
    : promptAgentOptions.find((option) => option.value === DEFAULT_ghostex_SETTINGS.defaultPromptAgentId)
        ?.value ??
      promptAgentOptions[0]?.value ??
      "";
  const titleGenerationCommandPreview = getSessionTitleGenerationCommandPreview(
    sessionTitleGenerationAgent,
    {
      command: resolveSettingsTitleGenerationCommand(
        sessionTitleGenerationAgent,
        orderedAgents,
        customSessionTitleGenerationCommand,
      ),
    },
  );
  const hookStatusByAgentId = useMemo(
    () => new Map(agentHookStatus?.agents.map((status) => [status.agentId, status]) ?? []),
    [agentHookStatus],
  );
  const installedHookCount =
    agentHookStatus?.agents.filter((status) => status.status === "installed").length ?? 0;
  const updateRequiredHookCount =
    agentHookStatus?.agents.filter((status) => status.status === "updateRequired").length ?? 0;
  const updateRequiredHookSummary =
    updateRequiredHookCount === 1 ? "1 needs update" : `${updateRequiredHookCount} need update`;
  const hookStatusSummary = agentHookStatus
    ? agentHookStatus.errorMessage
      ? "Unable to check hooks"
      : updateRequiredHookCount > 0
        ? `${installedHookCount}/${AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.length} hooks ready, ${updateRequiredHookSummary}`
        : `${installedHookCount}/${AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.length} hooks ready`
    : agentHookStatusLoading
      ? "Checking hooks"
      : "Hook status not checked";

  const saveAgent = (draft: AgentConfigDraft) => {
    if (!vscode) {
      return;
    }
    vscode.postMessage({
      acceptAllMode: draft.acceptAllMode,
      agentId: draft.agentId,
      command: draft.command,
      icon: draft.icon,
      name: draft.name,
      type: "saveSidebarAgent",
    });
    setEditorState(undefined);
  };

  const deleteAgent = (agent: SidebarAgentButton) => {
    vscode?.postMessage({
      agentId: agent.agentId,
      type: "deleteSidebarAgent",
    });
  };

  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsAgentDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex =
      "index" in source && typeof source.index === "number" ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const nextAgentIds = moveId(
      orderedAgents.map((agent) => agent.agentId),
      source.initialIndex,
      targetIndex,
    );
    setDraftAgentIds(nextAgentIds);
    vscode?.postMessage({
      agentIds: nextAgentIds,
      requestId: createSettingsReorderRequestId("agents"),
      type: "syncSidebarAgentOrder",
    });
  }) satisfies DragDropEventHandlers["onDragEnd"];

  return (
    <ScrollArea className="h-full min-h-0">
      <div className="flex flex-col gap-6 px-5 pb-5">
        {!editorState ? (
          <SettingsSection title="Agent Hooks">
            <details className="group w-full">
              {/*
               * CDXC:AgentHookSettings 2026-05-23-10:05:
               * Settings -> Agents starts with a collapsed hook setup panel so reliable-resume requirements are discoverable without pushing normal agent ordering/editing controls down the tab. The panel must cover every current Ghostex CLI resume-hook agent, while T3 Code remains outside the hook list because its managed runtime does not use CLI hook capture.
               *
               * CDXC:AgentHookSettings 2026-06-11-17:45:
               * The collapsed header must use the same field label/description typography and bordered row spacing as the other Agents settings rows. The disclosure chevron points right when collapsed and rotates down when expanded.
               *
               * CDXC:AgentHookSettings 2026-06-12-04:34:
               * The hook setup UI should use the same labeled section card chrome as the Agents management list below so the Agents tab scans as consistent grouped settings instead of a loose disclosure row followed by a bordered list.
               */}
              <summary className="settings-management-row flex cursor-pointer list-none items-center justify-between gap-3 border border-border bg-muted/20 px-3 py-3 marker:hidden [&::-webkit-details-marker]:hidden">
                <div className="flex min-w-0 flex-1 items-center gap-2.5">
                  <IconChevronRight
                    aria-hidden="true"
                    className="size-4 shrink-0 text-muted-foreground transition-transform duration-150 group-open:rotate-90"
                  />
                  <FieldContent className="min-w-0 gap-1">
                    <FieldLabel className="text-sm">Agent resume hooks</FieldLabel>
                    <FieldDescription className="text-xs text-muted-foreground">
                      {hookStatusSummary}
                    </FieldDescription>
                  </FieldContent>
                </div>
                <span className="flex shrink-0 items-center">
                  <AgentHookStatusIcon isLoading={agentHookStatusLoading} status={undefined} />
                </span>
              </summary>
              <div className="mt-3 flex flex-col gap-4 border border-border/80 bg-muted/10 px-4 pb-4 pt-4">
                <div className="space-y-2 text-xs leading-5 text-muted-foreground">
                  <p>
                    Install hooks so Ghostex can capture each agent&apos;s native session id and
                    resume the exact conversation after sleep, reload, or app restart.
                  </p>
                  <p>
                    Hooks write only session metadata into Ghostex&apos;s session-state files. The
                    existing title-based restore path remains available when a hook has not captured
                    an id yet.
                  </p>
                  <p>T3 Code uses Ghostex&apos;s managed runtime, so it does not need a CLI hook.</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    disabled={!onInstallAgentHooks || agentHookStatusLoading}
                    onClick={onInstallAgentHooks}
                    type="button"
                    variant="outline"
                  >
                    <IconDownload aria-hidden="true" data-icon="inline-start" />
                    {updateRequiredHookCount > 0 ? "Update Hooks" : "Install Hooks"}
                  </Button>
                  <Button
                    disabled={!onRequestAgentHookStatus || agentHookStatusLoading}
                    onClick={onRequestAgentHookStatus}
                    type="button"
                    variant="ghost"
                  >
                    <IconRefresh aria-hidden="true" data-icon="inline-start" />
                    Refresh
                  </Button>
                </div>
                <div className="flex flex-col gap-2">
                  {agentHookStatus?.errorMessage ? (
                    <div className="rounded-none border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                      {agentHookStatus.errorMessage}
                    </div>
                  ) : null}
                  {AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.map((agent) => (
                    <AgentHookStatusRow
                      agent={{
                        agentId: agent.agentId,
                        command: agent.command,
                        icon: agent.icon,
                        isDefault: true,
                        name: agent.name,
                      }}
                      isLoading={agentHookStatusLoading && !agentHookStatus}
                      key={agent.agentId}
                      status={hookStatusByAgentId.get(agent.agentId)}
                    />
                  ))}
                </div>
                {agentHookStatus ? (
                  <FieldDescription className="truncate text-[11px] text-muted-foreground">
                    Hook state: {agentHookStatus.hookStateDirectory}
                  </FieldDescription>
                ) : null}
              </div>
            </details>
          </SettingsSection>
        ) : null}
        {!editorState ? (
          <SettingsSection title="Config">
            {/*
             * CDXC:AgentConfigSettings 2026-06-12-04:40:
             * Default prompt, title generation, custom title command, and global Accept All are configuration controls, not agent management rows. Group them under the same labeled SettingsSection chrome as Agent Hooks and Agents so the Agents tab scans as three consistent areas: hooks, config, and launchers.
             */}
            {promptAgentOptions.length > 0 ? (
              <SelectField
                description="Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection."
                isModified={defaultPromptAgentId !== DEFAULT_ghostex_SETTINGS.defaultPromptAgentId}
                label="Default Prompt Agent"
                onChange={onDefaultPromptAgentIdChange}
                onResetToDefault={() =>
                  onDefaultPromptAgentIdChange(DEFAULT_ghostex_SETTINGS.defaultPromptAgentId)
                }
                options={promptAgentOptions}
                value={selectedDefaultPromptAgentId}
              />
            ) : (
              <StaticNoteField
                description="Configure at least one CLI agent before selecting a default prompt agent."
                label="Default Prompt Agent"
              />
            )}
            {/*
             * CDXC:GxserverSessionTitle 2026-06-04-08:24:
             * First-prompt session-title generation needs its own agent selector instead of reusing Default Prompt Agent, because title generation is a gxserver-owned background job while prompt-launch defaults affect Git helpers, project-board prompts, and worktree starts.
             *
             * CDXC:GxserverSessionTitle 2026-06-04-22:44:
             * Show the disabled command preview directly under the selector so users can inspect the exact Codex, Cursor CLI, Claude, Grok Build, or Custom command template before Ghostex sends a background title-generation prompt.
             */}
            <SelectField
              description="Choose the headless agent Ghostex uses for first-prompt session title generation."
              isModified={
                sessionTitleGenerationAgent !==
                DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent
              }
              label="Title Generation Agent"
              onChange={(value) =>
                onSessionTitleGenerationAgentChange(value as SessionTitleGenerationAgent)
              }
              onResetToDefault={() =>
                onSessionTitleGenerationAgentChange(
                  DEFAULT_ghostex_SETTINGS.sessionTitleGenerationAgent,
                )
              }
              options={SESSION_TITLE_GENERATION_AGENT_OPTIONS}
              value={sessionTitleGenerationAgent}
            />
            <DisabledCommandPreviewField
              description="Preview of the command Ghostex sends to generate automatic first-prompt session titles."
              label="Title Generation Command"
              value={titleGenerationCommandPreview}
            />
            {sessionTitleGenerationAgent === "custom" ? (
              <TextField
                description="Run this command with the title prompt on stdin. It should print only the title."
                isModified={
                  customSessionTitleGenerationCommand !==
                  DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand
                }
                label="Custom Title Command"
                onChange={onCustomSessionTitleGenerationCommandChange}
                onResetToDefault={() =>
                  onCustomSessionTitleGenerationCommandChange(
                    DEFAULT_ghostex_SETTINGS.customSessionTitleGenerationCommand,
                  )
                }
                placeholder="title-generator"
                value={customSessionTitleGenerationCommand}
              />
            ) : null}
            <Field
              className="items-center justify-between rounded-none border border-border bg-muted/20 px-4 py-3"
              orientation="horizontal"
            >
              <FieldContent>
                <FieldLabel className="text-sm" htmlFor={acceptAllToggleId}>
                  Accept All
                </FieldLabel>
                <FieldDescription className="text-xs text-muted-foreground">
                  Enable each supported agent&apos;s permission-bypass mode when launching sessions.
                  Per-agent settings can inherit or override this default.
                </FieldDescription>
              </FieldContent>
              <Switch
                checked={agentAcceptAllEnabled}
                disabled={!vscode}
                id={acceptAllToggleId}
                onCheckedChange={onAgentAcceptAllEnabledChange}
              />
            </Field>
          </SettingsSection>
        ) : null}
        <SettingsSection
          actions={
            !editorState ? (
              <Button
                disabled={!vscode}
                onClick={() => setEditorState({ draft: { command: "", name: "" } })}
                type="button"
                variant="outline"
              >
                <IconPlus aria-hidden="true" data-icon="inline-start" />
                Add Agent
              </Button>
            ) : null
          }
          title={editorState ? "Agent" : "Agents"}
        >
          {editorState ? (
            <AgentSettingsEditor
              draft={editorState.draft}
              onCancel={() => setEditorState(undefined)}
              onSave={saveAgent}
            />
          ) : (
            <>
              {orderedAgents.length > 0 ? (
                <DragDropProvider onDragEnd={handleDragEnd}>
                  <div className="flex flex-col gap-2">
                    {orderedAgents.map((agent, index) => (
                      <SettingsAgentRow
                        agent={agent}
                        index={index}
                        key={agent.agentId}
                        onDelete={() => deleteAgent(agent)}
                        onEdit={() =>
                          setEditorState({
                            draft: {
                              acceptAllMode: agent.acceptAllMode ?? "inherit",
                              agentId: agent.agentId,
                              command: agent.command ?? "",
                              icon: agent.icon,
                              name: agent.name,
                            },
                          })
                        }
                      />
                    ))}
                  </div>
                </DragDropProvider>
              ) : (
                <Empty className="border border-border bg-muted/20">
                  <EmptyHeader>
                    <EmptyTitle>No agents configured</EmptyTitle>
                    <EmptyDescription>Add an agent launcher to start new sessions.</EmptyDescription>
                  </EmptyHeader>
                </Empty>
              )}
            </>
          )}
        </SettingsSection>
      </div>
    </ScrollArea>
  );
}

function resolveSettingsTitleGenerationCommand(
  agent: SessionTitleGenerationAgent,
  agents: readonly SidebarAgentButton[],
  customCommand: string,
): string | undefined {
  if (agent === "custom") {
    return customCommand.trim();
  }
  return agents.find((candidate) => candidate.agentId === agent)?.command?.trim();
}

function AgentHookStatusRow({
  agent,
  isLoading,
  status,
}: {
  agent: SidebarAgentButton;
  isLoading: boolean;
  status?: SidebarAgentHookStatusItem;
}) {
  const statusText = getAgentHookStatusText(status, isLoading);
  return (
    <div className="flex items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2">
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <span
          aria-hidden="true"
          className="settings-management-icon flex size-8 shrink-0 items-center justify-center bg-muted"
        >
          <SettingsAgentIcon agent={agent} />
        </span>
        <span className="min-w-0">
          <span className="block truncate text-sm font-medium">{agent.name}</span>
          <span className="block truncate text-xs text-muted-foreground">
            {status?.detail ?? agent.command ?? "Waiting for hook check"}
          </span>
        </span>
      </div>
      <span
        className={cn(
          "flex shrink-0 items-center gap-1.5 rounded-none px-2 py-1 text-xs font-medium",
          getAgentHookStatusClassName(status, isLoading),
        )}
      >
        <AgentHookStatusIcon isLoading={isLoading} status={status} />
        {statusText}
      </span>
    </div>
  );
}

function AgentHookStatusIcon({
  isLoading,
  status,
}: {
  isLoading: boolean;
  status?: SidebarAgentHookStatusItem;
}) {
  if (isLoading) {
    return <IconRefresh aria-hidden="true" className="size-3.5 animate-spin" />;
  }
  if (!status) {
    return <IconInfoCircle aria-hidden="true" className="size-3.5 text-muted-foreground" />;
  }
  switch (status.status) {
    case "installed":
      return <IconCircleCheckFilled aria-hidden="true" className="size-3.5 text-emerald-400" />;
    case "updateRequired":
      return <IconAlertTriangle aria-hidden="true" className="size-3.5 text-amber-400" />;
    case "cliMissing":
      return <IconAlertTriangle aria-hidden="true" className="size-3.5 text-amber-400" />;
    case "notRequired":
      return <IconInfoCircle aria-hidden="true" className="size-3.5 text-muted-foreground" />;
    case "missing":
      return <IconCircleX aria-hidden="true" className="size-3.5 text-destructive" />;
  }
}

function getAgentHookStatusText(
  status: SidebarAgentHookStatusItem | undefined,
  isLoading: boolean,
): string {
  if (isLoading) {
    return "Checking";
  }
  if (!status) {
    return "Not checked";
  }
  switch (status.status) {
    case "installed":
      return "Installed";
    case "updateRequired":
      return "Needs update";
    case "cliMissing":
      return "CLI missing";
    case "notRequired":
      return "Not required";
    case "missing":
      return "Missing";
  }
}

function getAgentHookStatusClassName(
  status: SidebarAgentHookStatusItem | undefined,
  isLoading: boolean,
): string {
  if (isLoading || !status) {
    return "bg-muted text-muted-foreground";
  }
  switch (status.status) {
    case "installed":
      return "bg-emerald-500/10 text-emerald-300";
    case "updateRequired":
      return "bg-amber-500/10 text-amber-300";
    case "cliMissing":
      return "bg-amber-500/10 text-amber-300";
    case "notRequired":
      return "bg-muted text-muted-foreground";
    case "missing":
      return "bg-destructive/10 text-destructive";
  }
}

function SettingsAgentRow({
  agent,
  index,
  onDelete,
  onEdit,
}: {
  agent: SidebarAgentButton;
  index: number;
  onDelete: () => void;
  onEdit: () => void;
}) {
  const sortable = useSortable({
    accept: "settings-agent",
    data: createSettingsAgentDragData(agent.agentId),
    group: "settings-agents",
    id: agent.agentId,
    index,
    type: "settings-agent",
  });

  const setRowRef = (element: HTMLDivElement | null) => {
    sortable.ref(element);
    sortable.sourceRef(element);
  };

  return (
    <div
      className="settings-management-row flex items-center gap-2 border border-border bg-muted/20 p-2"
      data-dragging={String(Boolean(sortable.isDragging))}
      ref={setRowRef}
    >
      <Button
        aria-label={`Reorder ${agent.name}`}
        ref={sortable.handleRef}
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <IconGripVertical aria-hidden="true" />
      </Button>
      <Button
        className="settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left"
        onClick={onEdit}
        type="button"
        variant="ghost"
      >
        <span
          aria-hidden="true"
          className="settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted"
        >
          <SettingsAgentIcon agent={agent} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-medium text-foreground">{agent.name}</span>
          <span className="block truncate text-xs text-muted-foreground">
            {agent.command?.trim() || "Not configured"}
          </span>
        </span>
      </Button>
      <span className="settings-management-row-actions">
        <Button
          aria-label={`Edit ${agent.name}`}
          onClick={onEdit}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <IconPencil aria-hidden="true" />
        </Button>
        <Button
          aria-label={`Delete ${agent.name}`}
          onClick={onDelete}
          size="icon-sm"
          type="button"
          variant="destructive"
        >
          <IconTrash aria-hidden="true" />
        </Button>
      </span>
    </div>
  );
}

function AgentSettingsEditor({
  draft,
  onCancel,
  onSave,
}: {
  draft: AgentConfigDraft;
  onCancel: () => void;
  onSave: (draft: AgentConfigDraft) => void;
}) {
  const [acceptAllMode, setAcceptAllMode] = useState<AgentAcceptAllMode>(draft.acceptAllMode ?? "inherit");
  const [command, setCommand] = useState(draft.command);
  const [icon, setIcon] = useState<SidebarAgentIcon | "custom">(draft.icon ?? "custom");
  const [name, setName] = useState(draft.name);
  const acceptAllModeId = useId();
  const agentTypeId = useId();
  const commandId = useId();
  const nameId = useId();
  const isSaveDisabled = name.trim().length === 0 || command.trim().length === 0;
  const resolvedAgentId = draft.agentId ?? getDefaultSidebarAgentByIcon(icon === "custom" ? undefined : icon)?.agentId ?? "";
  const acceptAllSupported = supportsAgentAcceptAll(resolvedAgentId, icon === "custom" ? undefined : icon);

  const updateAgentType = (value: string) => {
    const nextType = value as SidebarAgentIcon | "custom";
    const previousDefaultAgent = getDefaultSidebarAgentByIcon(
      icon === "custom" ? undefined : icon,
    );
    const nextDefaultAgent = getDefaultSidebarAgentByIcon(
      nextType === "custom" ? undefined : nextType,
    );

    setIcon(nextType);
    if (!nextDefaultAgent) {
      return;
    }

    setName((previousName) =>
      previousName.trim().length === 0 || previousName === previousDefaultAgent?.name
        ? nextDefaultAgent.name
        : previousName,
    );
    setCommand((previousCommand) =>
      previousCommand.trim().length === 0 || previousCommand === previousDefaultAgent?.command
        ? nextDefaultAgent.command
        : previousCommand,
    );
  };

  return (
    <>
      <Field className="gap-2.5">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={agentTypeId}>
            Agent type
          </FieldLabel>
        </FieldContent>
        <Select items={AGENT_TYPE_SELECT_ITEMS} onValueChange={updateAgentType} value={icon}>
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={agentTypeId}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              <SelectItem value="custom">Custom</SelectItem>
              {DEFAULT_SIDEBAR_AGENTS.map((agent) => (
                <SelectItem key={agent.agentId} value={agent.icon}>
                  {agent.name}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>
      <Field className="gap-2.5">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={nameId}>
            Name
          </FieldLabel>
        </FieldContent>
        <Input
          autoFocus
          className="h-10 px-3 text-sm"
          id={nameId}
          onChange={(event) => setName(event.currentTarget.value)}
          placeholder="Codex"
          value={name}
        />
      </Field>
      <Field className="gap-2.5">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={commandId}>
            Command
          </FieldLabel>
        </FieldContent>
        <Textarea
          id={commandId}
          onChange={(event) => setCommand(event.currentTarget.value)}
          placeholder="codex"
          rows={3}
          value={command}
        />
      </Field>
      <Field className="gap-2.5">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={acceptAllModeId}>
            Accept All
          </FieldLabel>
          <FieldDescription className="text-xs text-muted-foreground">
            {acceptAllSupported
              ? "Inherit uses the global Agents setting. Accept All applies this agent's permission-bypass mode at launch without changing the stored command."
              : "This agent does not expose a supported Accept All mode in Ghostex."}
          </FieldDescription>
        </FieldContent>
        <Select
          disabled={!acceptAllSupported}
          items={AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS}
          onValueChange={(value) => setAcceptAllMode(value as AgentAcceptAllMode)}
          value={acceptAllMode}
        >
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={acceptAllModeId}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>
      <div className="flex justify-end gap-3">
        <Button onClick={onCancel} type="button" variant="outline">
          Cancel
        </Button>
        <Button
          disabled={isSaveDisabled}
          onClick={() =>
            onSave({
              acceptAllMode,
              agentId: draft.agentId,
              command: command.trim(),
              icon: icon === "custom" ? undefined : icon,
              name: name.trim(),
            })
          }
          type="button"
        >
          Save
        </Button>
      </div>
    </>
  );
}

function ActionsSettingsTab({ vscode }: { vscode?: WebviewApi }) {
  const commands = useSidebarStore((state) => state.hud.commands);
  const [editorState, setEditorState] = useState<SettingsCommandEditorState>();
  const [draftCommandIds, setDraftCommandIds] = useState<string[]>();

  useEffect(() => {
    setDraftCommandIds((previousDraft) => reconcileDraftIds(previousDraft, commands, "commandId"));
  }, [commands]);

  const orderedCommands = useMemo(() => {
    const commandById = new Map(commands.map((command) => [command.commandId, command]));
    const orderedCommandIds = draftCommandIds
      ? mergeIds(
          draftCommandIds,
          commands.map((command) => command.commandId),
        )
      : commands.map((command) => command.commandId);

    return orderedCommandIds
      .map((commandId) => commandById.get(commandId))
      .filter((command): command is SidebarCommandButton => command !== undefined);
  }, [commands, draftCommandIds]);

  const openCreateCommandEditor = (actionType: SidebarActionType) => {
    setEditorState({
      draft: createSettingsCommandDraft(actionType),
      lockedActionType: actionType,
    });
  };

  const saveCommand = (draft: CommandConfigDraft) => {
    if (!vscode) {
      return;
    }
    vscode.postMessage({
      actionType: draft.actionType,
      closeTerminalOnExit: draft.closeTerminalOnExit,
      command: draft.command,
      commandId: draft.commandId,
      icon: draft.icon,
      iconColor: draft.iconColor,
      name: draft.name,
      playCompletionSound: draft.playCompletionSound,
      type: "saveSidebarCommand",
      url: draft.url,
    });
    setEditorState(undefined);
  };

  const deleteCommand = (commandId: string) => {
    vscode?.postMessage({
      commandId,
      type: "deleteSidebarCommand",
    });
    setEditorState(undefined);
  };

  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsCommandDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex =
      "index" in source && typeof source.index === "number" ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const nextCommandIds = moveId(
      orderedCommands.map((command) => command.commandId),
      source.initialIndex,
      targetIndex,
    );
    setDraftCommandIds(nextCommandIds);
    vscode?.postMessage({
      commandIds: nextCommandIds,
      requestId: createSettingsReorderRequestId("actions"),
      type: "syncSidebarCommandOrder",
    });
  }) satisfies DragDropEventHandlers["onDragEnd"];

  return (
    <ScrollArea className="h-full min-h-0">
      <div className="flex flex-col gap-6 px-5 pb-5">
        <SettingsSection
          actions={
            !editorState ? (
              <>
                <Button
                  disabled={!vscode}
                  onClick={() => openCreateCommandEditor("terminal")}
                  type="button"
                  variant="outline"
                >
                  <IconPlus aria-hidden="true" data-icon="inline-start" />
                  Terminal Action
                </Button>
                <Button
                  disabled={!vscode}
                  onClick={() => openCreateCommandEditor("browser")}
                  type="button"
                  variant="outline"
                >
                  <IconPlus aria-hidden="true" data-icon="inline-start" />
                  Browser Action
                </Button>
              </>
            ) : null
          }
          title={editorState ? "Action" : "Actions"}
        >
          {editorState ? (
            <ActionSettingsEditor
              draft={editorState.draft}
              existingCommands={commands}
              lockedActionType={editorState.lockedActionType}
              onCancel={() => setEditorState(undefined)}
              onSave={saveCommand}
            />
          ) : (
            <>
              {orderedCommands.length > 0 ? (
                <DragDropProvider onDragEnd={handleDragEnd}>
                  <div className="flex flex-col gap-2">
                    {orderedCommands.map((command, index) => (
                      <SettingsCommandRow
                        command={command}
                        index={index}
                        key={command.commandId}
                        onEdit={() =>
                          setEditorState({
                            draft: createSettingsCommandDraftFromButton(command),
                          })
                        }
                        onDelete={() => deleteCommand(command.commandId)}
                      />
                    ))}
                  </div>
                </DragDropProvider>
              ) : (
                <Empty className="border border-border bg-muted/20">
                  <EmptyHeader>
                    <EmptyTitle>No actions configured</EmptyTitle>
                    <EmptyDescription>Add a terminal or browser action.</EmptyDescription>
                  </EmptyHeader>
                </Empty>
              )}
            </>
          )}
        </SettingsSection>
      </div>
    </ScrollArea>
  );
}

function SettingsCommandRow({
  command,
  index,
  onDelete,
  onEdit,
}: {
  command: SidebarCommandButton;
  index: number;
  onDelete: () => void;
  onEdit: () => void;
}) {
  const sortable = useSortable({
    accept: "settings-command",
    data: createSettingsCommandDragData(command.commandId),
    group: "settings-commands",
    id: command.commandId,
    index,
    type: "settings-command",
  });

  const setRowRef = (element: HTMLDivElement | null) => {
    sortable.ref(element);
    sortable.sourceRef(element);
  };

  return (
    <div
      className="settings-management-row flex items-center gap-2 border border-border bg-muted/20 p-2"
      data-dragging={String(Boolean(sortable.isDragging))}
      ref={setRowRef}
    >
      <Button
        aria-label={`Reorder ${getActionTitle(command)}`}
        ref={sortable.handleRef}
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <IconGripVertical aria-hidden="true" />
      </Button>
      <Button
        className="settings-management-edit-button h-auto min-w-0 flex-1 justify-start gap-3 px-2 py-2 text-left"
        onClick={onEdit}
        type="button"
        variant="ghost"
      >
        <span
          aria-hidden="true"
          className="settings-management-icon flex size-9 shrink-0 items-center justify-center bg-muted"
        >
          <SettingsActionIcon command={command} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-medium text-foreground">
            {getActionTitle(command)}
          </span>
          <span className="block truncate text-xs text-muted-foreground">
            {getActionMeta(command)}
          </span>
        </span>
      </Button>
      <span className="settings-management-row-actions">
        <Button
          aria-label={`Edit ${getActionTitle(command)}`}
          onClick={onEdit}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <IconPencil aria-hidden="true" />
        </Button>
        <Button
          aria-label={`Delete ${getActionTitle(command)}`}
          onClick={onDelete}
          size="icon-sm"
          type="button"
          variant="destructive"
        >
          <IconTrash aria-hidden="true" />
        </Button>
      </span>
    </div>
  );
}

function ActionSettingsEditor({
  draft,
  existingCommands,
  lockedActionType,
  onCancel,
  onSave,
}: {
  draft: CommandConfigDraft;
  existingCommands: readonly SidebarCommandButton[];
  lockedActionType?: SidebarActionType;
  onCancel: () => void;
  onSave: (draft: CommandConfigDraft) => void;
}) {
  const [actionType, setActionType] = useState<SidebarActionType>(draft.actionType);
  const [closeTerminalOnExit, setCloseTerminalOnExit] = useState(draft.closeTerminalOnExit);
  const [command, setCommand] = useState(draft.command ?? "");
  const [icon, setIcon] = useState<SidebarCommandIcon>(
    draft.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
  );
  const [iconColor, setIconColor] = useState(draft.iconColor ?? DEFAULT_SIDEBAR_COMMAND_ICON_COLOR);
  const [name, setName] = useState(draft.name);
  const [playCompletionSound, setPlayCompletionSound] = useState(draft.playCompletionSound);
  const [url, setUrl] = useState(
    draft.url ??
      ((lockedActionType ?? draft.actionType) === "browser" ? DEFAULT_BROWSER_ACTION_URL : ""),
  );
  const actionTypeId = useId();
  const closeTerminalOnExitId = useId();
  const commandId = useId();
  const nameId = useId();
  const soundId = useId();
  const urlId = useId();
  const isActionTypeLocked = lockedActionType !== undefined;
  const targetValue = actionType === "browser" ? url.trim() : command.trim();
  const trimmedName = name.trim();
  const commandTitle = getSettingsCommandDraftTitle({ actionType, command, name, url });
  /**
   * CDXC:CommandPanes 2026-05-16-15:08:
   * Settings must enforce one action title per project because command-pane
   * reuse uses that title as the pane identifier. Blocking duplicates here
   * prevents saving an action that could target another action's command tab.
   */
  const hasDuplicateTitle = existingCommands.some(
    (commandButton) =>
      commandButton.commandId !== draft.commandId &&
      getSettingsCommandTitleKey(getSettingsCommandButtonTitle(commandButton)) ===
        getSettingsCommandTitleKey(commandTitle),
  );
  const isSaveDisabled = targetValue.length === 0 || hasDuplicateTitle;

  const getDraft = (): CommandConfigDraft => ({
    actionType,
    closeTerminalOnExit: actionType === "terminal" ? closeTerminalOnExit : false,
    command: actionType === "terminal" ? command.trim() : undefined,
    commandId: draft.commandId,
    icon,
    iconColor,
    name: trimmedName,
    playCompletionSound: actionType === "terminal" ? playCompletionSound : false,
    url: actionType === "browser" ? url.trim() : undefined,
  });

  return (
    <>
      {isActionTypeLocked ? null : (
        <Field className="gap-2.5">
          <FieldContent>
            <FieldLabel className="text-sm" htmlFor={actionTypeId}>
              Type
            </FieldLabel>
          </FieldContent>
          <Select
            onValueChange={(value) => {
              const nextActionType = value === "browser" ? "browser" : "terminal";
              setActionType(nextActionType);
              if (nextActionType === "browser" && url.trim().length === 0) {
                setUrl(DEFAULT_BROWSER_ACTION_URL);
              }
            }}
            value={actionType}
          >
            <SelectTrigger className="h-10 w-full px-3 text-sm" id={actionTypeId}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="terminal">Terminal</SelectItem>
                <SelectItem value="browser">Browser</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
      )}
      <Field className="gap-2.5" data-invalid={hasDuplicateTitle || undefined}>
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={nameId}>
            Text
          </FieldLabel>
        </FieldContent>
        <Input
          autoFocus
          aria-invalid={hasDuplicateTitle || undefined}
          className="h-10 px-3 text-sm"
          id={nameId}
          onChange={(event) => setName(event.currentTarget.value)}
          placeholder={actionType === "browser" ? "Docs" : "Dev"}
          value={name}
        />
        {hasDuplicateTitle ? (
          <FieldDescription className="text-sm">
            Another action already uses this title.
          </FieldDescription>
        ) : null}
      </Field>
      <CommandIconPicker
        icon={icon}
        iconColor={iconColor}
        onIconChange={setIcon}
        onIconColorChange={setIconColor}
      />
      {actionType === "browser" ? (
        <Field className="gap-2.5">
          <FieldContent>
            <FieldLabel className="text-sm" htmlFor={urlId}>
              URL
            </FieldLabel>
          </FieldContent>
          <Textarea
            id={urlId}
            onChange={(event) => setUrl(event.currentTarget.value)}
            placeholder={DEFAULT_BROWSER_ACTION_URL}
            rows={3}
            value={url}
          />
        </Field>
      ) : (
        <>
          <Field className="gap-2.5">
            <FieldContent>
              <FieldLabel className="text-sm" htmlFor={commandId}>
                Command
              </FieldLabel>
            </FieldContent>
            <Textarea
              id={commandId}
              onChange={(event) => setCommand(event.currentTarget.value)}
              placeholder="vp dev"
              rows={3}
              value={command}
            />
          </Field>
          <Field className="items-center justify-between" orientation="horizontal">
            <FieldContent>
              <FieldLabel className="text-sm" htmlFor={closeTerminalOnExitId}>
                Close terminal after the command finishes
              </FieldLabel>
            </FieldContent>
            <Switch
              checked={closeTerminalOnExit}
              id={closeTerminalOnExitId}
              onCheckedChange={setCloseTerminalOnExit}
            />
          </Field>
          <Field className="items-center justify-between" orientation="horizontal">
            <FieldContent>
              <FieldLabel className="text-sm" htmlFor={soundId}>
                Play completion sound
              </FieldLabel>
            </FieldContent>
            <Switch
              checked={playCompletionSound}
              id={soundId}
              onCheckedChange={setPlayCompletionSound}
            />
          </Field>
        </>
      )}
      <div className="flex justify-end gap-3">
        <Button onClick={onCancel} type="button" variant="outline">
          Cancel
        </Button>
        <Button disabled={isSaveDisabled} onClick={() => onSave(getDraft())} type="button">
          Save
        </Button>
      </div>
    </>
  );
}

function getSettingsCommandDraftTitle({
  actionType,
  command,
  name,
  url,
}: {
  actionType: SidebarActionType;
  command: string;
  name: string;
  url: string;
}): string {
  const normalizedName = normalizeSettingsCommandTitle(name);
  if (normalizedName) {
    return normalizedName;
  }
  const target = normalizeSettingsCommandTitle(actionType === "browser" ? url : command);
  return target?.slice(0, 20) ?? "";
}

function getSettingsCommandButtonTitle(command: SidebarCommandButton): string {
  const normalizedName = normalizeSettingsCommandTitle(command.name);
  if (normalizedName) {
    return normalizedName;
  }
  const target = normalizeSettingsCommandTitle(command.command ?? command.url);
  return target?.slice(0, 20) ?? "";
}

function getSettingsCommandTitleKey(value: string | undefined): string {
  return normalizeSettingsCommandTitle(value)?.toLocaleLowerCase() ?? "";
}

function normalizeSettingsCommandTitle(value: string | undefined): string | undefined {
  const normalized = value?.trim().replace(/\s+/g, " ");
  return normalized ? normalized : undefined;
}

function HotkeysSettingsTab({
  hotkeys,
  onChange,
  searchQuery,
}: {
  hotkeys?: ghostexHotkeySettings;
  onChange: (hotkeys: ghostexHotkeySettings) => void;
  searchQuery: string;
}) {
  const normalizedHotkeys = normalizeghostexHotkeySettings(hotkeys);
  const actionsSectionRef = useRef<HTMLDivElement>(null);
  const generalSectionRef = useRef<HTMLDivElement>(null);
  const groupsSectionRef = useRef<HTMLDivElement>(null);
  const navigationSectionRef = useRef<HTMLDivElement>(null);
  const paneActionsSectionRef = useRef<HTMLDivElement>(null);
  const sessionSlotsSectionRef = useRef<HTMLDivElement>(null);
  const duplicateIds = useMemo(
    () => getDuplicateHotkeyIds(normalizedHotkeys),
    [normalizedHotkeys],
  );
  const definitionsById = useMemo(
    () => new Map(GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition])),
    [],
  );
  /**
   * CDXC:Hotkeys 2026-05-13-16:05
   * Hotkey settings are split by workflow and searched independently from the
   * Settings tab. Section nav should jump to matching groups while
   * search still filters individual bindings inside each group.
   */
  const sectionSearches = useMemo(
    () =>
      Object.fromEntries(
        HOTKEY_SETTINGS_SECTIONS.map((section) => [
          section.id,
          getSettingsSectionSearch(
            searchQuery,
            section.title,
            section.ids.flatMap((id) => {
              const definition = definitionsById.get(id);
              return definition
                ? [
                    {
                      key: definition.id,
                      options: [{ label: definition.defaultKey, value: definition.defaultKey }],
                      subtitle: definition.description,
                      title: definition.title,
                    },
                  ]
                : [];
            }),
          ),
        ]),
      ) as Record<HotkeySettingsSectionId, SettingsSectionSearchResult>,
    [definitionsById, searchQuery],
  );
  const sectionRefs: Record<HotkeySettingsSectionId, RefObject<HTMLDivElement | null>> = {
    actions: actionsSectionRef,
    general: generalSectionRef,
    groups: groupsSectionRef,
    navigation: navigationSectionRef,
    paneActions: paneActionsSectionRef,
    sessionSlots: sessionSlotsSectionRef,
  };
  const visibleSections = HOTKEY_SETTINGS_SECTIONS.filter((section) =>
    shouldShowSettingsSection(sectionSearches[section.id]),
  );
  const hasVisibleHotkeys = visibleSections.length > 0;

  const updateHotkey = (id: ghostexHotkeyActionId, value: string) => {
    onChange(
      normalizeghostexHotkeySettings({
        ...normalizedHotkeys,
        [id]: normalizeHotkeyText(value),
      }),
    );
  };

  const resetHotkeys = () => {
    onChange(normalizeghostexHotkeySettings(DEFAULT_ghostex_HOTKEYS));
  };

  return (
    <div className="settings-main-tab-layout">
      <aside aria-label="Hotkey sections" className="settings-section-sidebar">
        {visibleSections.map((section) => (
          <Button
            className="settings-section-sidebar-button"
            key={section.id}
            onClick={() => sectionRefs[section.id].current?.scrollIntoView({ behavior: "smooth", block: "start" })}
            type="button"
            variant="ghost"
          >
            {section.title}
          </Button>
        ))}
      </aside>
      <ScrollArea className="h-full min-h-0">
        <div className="flex flex-col gap-6 px-5 pb-5">
          {visibleSections.map((section) => (
            <SettingsSection
              key={section.id}
              sectionRef={sectionRefs[section.id]}
              title={section.title}
            >
              {section.ids.flatMap((id) => {
                const definition = definitionsById.get(id);
                if (
                  !definition ||
                  !shouldShowSetting(sectionSearches[section.id], definition.id)
                ) {
                  return [];
                }
                const value = normalizedHotkeys[definition.id] ?? definition.defaultKey;
                const isDuplicate = duplicateIds.has(definition.id);
                return [
                  <Field className="gap-2.5" data-invalid={isDuplicate} key={definition.id}>
                    <FieldContent>
                      <FieldLabel className="text-sm" htmlFor={`hotkey-${definition.id}`}>
                        {definition.title}
                      </FieldLabel>
                      <FieldDescription className="text-sm">
                        {definition.description}
                      </FieldDescription>
                    </FieldContent>
                    <HotkeyRecorderField
                      ariaInvalid={isDuplicate}
                      id={`hotkey-${definition.id}`}
                      hotkey={value}
                      onChange={(nextHotkey) => updateHotkey(definition.id, nextHotkey)}
                    />
                  </Field>,
                ];
              })}
            </SettingsSection>
          ))}
          {!hasVisibleHotkeys ? (
            <div className="rounded-none border border-border bg-muted/30 px-4 py-6 text-center text-sm text-muted-foreground">
              No hotkeys match your search.
            </div>
          ) : null}
          <div className="flex justify-end">
            <Button onClick={resetHotkeys} type="button" variant="outline">
              Reset Hotkeys
            </Button>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}

function SettingsAgentIcon({ agent }: { agent: SidebarAgentButton }) {
  if (agent.icon) {
    return (
      <span
        aria-hidden="true"
        className="configure-agents-list-agent-icon"
        style={{
          backgroundColor: AGENT_LOGO_COLORS[agent.icon],
          maskImage: `url("${AGENT_LOGOS[agent.icon]}")`,
          WebkitMaskImage: `url("${AGENT_LOGOS[agent.icon]}")`,
        }}
      />
    );
  }

  return <IconCodeDots aria-hidden="true" />;
}

function SettingsActionIcon({ command }: { command: SidebarCommandButton }) {
  return (
    <SidebarCommandIconGlyph
      color={command.iconColor ?? DEFAULT_SIDEBAR_COMMAND_ICON_COLOR}
      icon={command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON}
      stroke={1.8}
    />
  );
}

function getActionTitle(command: SidebarCommandButton): string {
  const name = command.name.trim();
  if (name.length > 0) {
    return name;
  }

  const target = getActionTarget(command);
  return target ?? "Untitled Action";
}

function getActionMeta(command: SidebarCommandButton): string {
  const target = getActionTarget(command);
  const typeLabel = command.actionType === "browser" ? "Browser" : "Terminal";
  if (!target) {
    return `${typeLabel} - Not configured`;
  }

  return `${typeLabel} - ${target}`;
}

function getActionTarget(command: SidebarCommandButton): string | undefined {
  const target = command.actionType === "browser" ? command.url?.trim() : command.command?.trim();
  if (!target) {
    return undefined;
  }

  return target.split("\n")[0] || undefined;
}

function createSettingsCommandDraft(actionType: SidebarActionType): CommandConfigDraft {
  return {
    actionType,
    closeTerminalOnExit: false,
    command: actionType === "terminal" ? "" : undefined,
    commandId: undefined,
    icon: DEFAULT_SIDEBAR_COMMAND_ICON,
    iconColor: DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
    name: "",
    playCompletionSound: actionType === "terminal",
    url: actionType === "browser" ? DEFAULT_BROWSER_ACTION_URL : undefined,
  };
}

function createSettingsCommandDraftFromButton(command: SidebarCommandButton): CommandConfigDraft {
  return {
    actionType: command.actionType,
    closeTerminalOnExit: command.closeTerminalOnExit,
    command: command.command,
    commandId: command.commandId,
    icon: command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
    iconColor: command.iconColor ?? DEFAULT_SIDEBAR_COMMAND_ICON_COLOR,
    name: command.name,
    playCompletionSound: command.playCompletionSound,
    url: command.url,
  };
}

function getDuplicateHotkeyIds(hotkeys: ghostexHotkeySettings): Set<ghostexHotkeyActionId> {
  const idsByHotkey = new Map<string, ghostexHotkeyActionId[]>();
  for (const definition of GHOSTEX_HOTKEY_DEFINITIONS) {
    const hotkey = normalizeHotkeyText(hotkeys[definition.id] ?? definition.defaultKey);
    if (!hotkey) {
      continue;
    }
    idsByHotkey.set(hotkey, [...(idsByHotkey.get(hotkey) ?? []), definition.id]);
  }

  return new Set(
    Array.from(idsByHotkey.values())
      .filter((ids) => ids.length > 1)
      .flat(),
  );
}

function createSettingsAgentDragData(agentId: string): SettingsAgentDragData {
  return {
    agentId,
    kind: "settings-agent",
  };
}

function getSettingsAgentDragData(candidate: unknown): SettingsAgentDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (!isObjectRecord(data) || data.kind !== "settings-agent" || typeof data.agentId !== "string") {
    return undefined;
  }

  return {
    agentId: data.agentId,
    kind: "settings-agent",
  };
}

function createSettingsCommandDragData(commandId: string): SettingsCommandDragData {
  return {
    commandId,
    kind: "settings-command",
  };
}

function getSettingsCommandDragData(candidate: unknown): SettingsCommandDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (
    !isObjectRecord(data) ||
    data.kind !== "settings-command" ||
    typeof data.commandId !== "string"
  ) {
    return undefined;
  }

  return {
    commandId: data.commandId,
    kind: "settings-command",
  };
}

function moveId(ids: readonly string[], initialIndex: number, index: number): string[] {
  const nextIds = [...ids];
  const [id] = nextIds.splice(initialIndex, 1);
  if (id === undefined) {
    return nextIds;
  }

  nextIds.splice(index, 0, id);
  return nextIds;
}

function mergeIds(draftIds: readonly string[], syncedIds: readonly string[]): string[] {
  const syncedIdSet = new Set(syncedIds);
  const mergedIds = draftIds.filter((id) => syncedIdSet.has(id));

  for (const id of syncedIds) {
    if (!mergedIds.includes(id)) {
      mergedIds.push(id);
    }
  }

  return mergedIds;
}

function reconcileDraftIds<Item extends Record<Key, string>, Key extends keyof Item>(
  draftIds: readonly string[] | undefined,
  items: readonly Item[],
  key: Key,
): string[] | undefined {
  if (!draftIds) {
    return undefined;
  }

  const syncedIds = items.map((item) => item[key]);
  const nextDraftIds = mergeIds(draftIds, syncedIds);
  return haveSameOrder(nextDraftIds, syncedIds) ? undefined : nextDraftIds;
}

function haveSameOrder(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

function createSettingsReorderRequestId(kind: "actions" | "agents"): string {
  return `settings-${kind}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function hasData(candidate: unknown): candidate is { data?: unknown } {
  return isObjectRecord(candidate) && "data" in candidate;
}

function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function GhostexFolderStatsSection({
  isLoading,
  onOpenGhostexFolder,
  stats,
}: {
  isLoading: boolean;
  onOpenGhostexFolder?: () => void;
  stats?: SidebarGhostexFolderStatsMessage;
}) {
  const folders = stats?.folders ?? [];
  return (
    <SettingsSection title="Storage">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground">Ghostex folder</div>
          <div className="mt-1 truncate text-xs text-muted-foreground">
            {stats?.folderPath ?? "~/.ghostex"}
          </div>
        </div>
        <Button
          className="h-9 shrink-0 gap-2 px-3 text-sm"
          disabled={!onOpenGhostexFolder}
          onClick={onOpenGhostexFolder}
          type="button"
          variant="outline"
        >
          <IconFolderOpen aria-hidden="true" className="size-4" />
          Open Folder
        </Button>
      </div>

      {isLoading && !stats ? (
        <div className="rounded-none border border-border bg-muted/25 px-3 py-2 text-sm text-muted-foreground">
          Loading folder sizes...
        </div>
      ) : null}

      {stats?.errorMessage ? (
        <div className="rounded-none border border-destructive/45 bg-destructive/10 px-3 py-2 text-sm text-foreground">
          {stats.errorMessage}
        </div>
      ) : null}

      {stats && !stats.errorMessage ? (
        <div className="rounded-none border border-border bg-muted/20">
          {folders.length > 0 ? (
            folders.map((folder) => (
              <div
                className="flex items-center justify-between gap-3 border-b border-border px-3 py-2 text-sm last:border-b-0"
                key={folder.path}
              >
                <span className="min-w-0 truncate text-foreground">{folder.name}</span>
                <span className="shrink-0 tabular-nums text-muted-foreground">
                  {formatBytes(folder.sizeBytes)}
                </span>
              </div>
            ))
          ) : (
            <div className="px-3 py-2 text-sm text-muted-foreground">No folders found.</div>
          )}
          <div className="flex items-center justify-between gap-3 border-t border-border px-3 py-2 text-sm font-medium">
            <span>Total</span>
            <span className="tabular-nums">{formatBytes(stats.totalBytes)}</span>
          </div>
        </div>
      ) : null}
    </SettingsSection>
  );
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const decimals = value >= 10 || unitIndex === 0 ? 0 : 1;
  return `${value.toFixed(decimals)} ${units[unitIndex] ?? "B"}`;
}

function GhosttySettingsActions({
  onApplyRecommended,
  onOpenConfigFile,
  onOpenDocs,
  onResetDefaults,
}: {
  onApplyRecommended: () => void;
  onOpenConfigFile: () => void;
  onOpenDocs: () => void;
  onResetDefaults: () => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <Button className="h-10 px-4 text-sm" onClick={onResetDefaults} type="button" variant="outline">
        Reset Ghostty defaults
      </Button>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              className="h-10 px-4 text-sm"
              onClick={onApplyRecommended}
              type="button"
              variant="outline"
            >
              Apply recommended
            </Button>
          }
        />
        <TooltipContent className="whitespace-pre-line text-left" sideOffset={6}>
          {GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES.join("\n")}
        </TooltipContent>
      </Tooltip>
      <Button className="h-10 px-4 text-sm" onClick={onOpenDocs} type="button" variant="outline">
        Open Ghostty docs
      </Button>
      <Button
        className="h-10 px-4 text-sm"
        onClick={onOpenConfigFile}
        type="button"
        variant="outline"
      >
        Open Ghostty config
      </Button>
    </div>
  );
}

function GtePromptEditingField({
  backend,
  isModified,
  onChange,
  onInstall,
  onResetToDefault,
}: {
  backend: PromptEditorBackend;
  isModified?: boolean;
  onChange: (backend: PromptEditorBackend) => void;
  onInstall: () => void;
  onResetToDefault?: () => void;
}) {
  const id = useId();
  return (
    <SettingRow
      description="Choose which editor new terminals use when Ctrl+G asks the shell to edit prompt text."
      htmlFor={id}
      isModified={isModified}
      label="Ctrl+G prompt editor"
      onResetToDefault={onResetToDefault}
    >
      <div className="flex flex-col items-start gap-3">
        <Select onValueChange={(value) => onChange(value as PromptEditorBackend)} value={backend}>
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {PROMPT_EDITOR_BACKEND_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <Button
          className="h-9 w-fit px-3 text-sm"
          onClick={onInstall}
          type="button"
          variant="outline"
        >
          Install gte
        </Button>
      </div>
    </SettingRow>
  );
}

/**
 * CDXC:Settings 2026-04-26-21:27: The settings modal previews the same theme
 * as the sidebar. The modal updates immediately when the Theme select changes,
 * without waiting for the native host to echo a new HUD snapshot.
 */
function getSidebarThemeVariant(theme: SidebarTheme): SidebarThemeVariant {
  return theme.startsWith("light-") || theme === "plain-light" ? "light" : "dark";
}

function getSettingsSectionSearch(
  query: string,
  sectionTitle: string,
  settings: ReadonlyArray<SettingSearchDefinition>,
): SettingsSectionSearchResult {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) {
    return {
      isSearching: false,
      sectionMatches: true,
      visibleSettingKeys: new Set(settings.map((setting) => setting.key)),
    };
  }

  const searchItems = [
    {
      id: "__section",
      options: [],
      subtitle: "",
      title: sectionTitle,
    },
    ...settings.map((setting) => ({
      id: setting.key,
      options: setting.options?.flatMap((option) => [option.label, option.value]) ?? [],
      subtitle: setting.subtitle ?? "",
      title: setting.title,
    })),
  ];
  const fuse = new Fuse(searchItems, {
    ignoreLocation: true,
    includeScore: true,
    keys: [
      { name: "title", weight: 0.55 },
      { name: "subtitle", weight: 0.25 },
      { name: "options", weight: 0.2 },
    ],
    /**
     * CDXC:SettingsSearch 2026-05-13-16:05
     * Search should be useful without feeling random. A lower Fuse threshold
     * keeps section/settings/hotkey results close to the user's query instead
     * of surfacing weak fuzzy matches from unrelated settings.
     */
    threshold: 0.24,
  });
  const results = fuse.search(trimmedQuery);
  const sectionMatches = results.some((result) => result.item.id === "__section");
  return {
    isSearching: true,
    sectionMatches,
    visibleSettingKeys: new Set(
      results
        .map((result) => result.item.id)
        .filter((settingKey) => settingKey !== "__section"),
    ),
  };
}

function hasVisibleSettingsSearchResult(result: SettingsSectionSearchResult): boolean {
  return result.sectionMatches || result.visibleSettingKeys.size > 0;
}

function shouldShowSettingsSection(result: SettingsSectionSearchResult): boolean {
  return hasVisibleSettingsSearchResult(result);
}

function shouldShowSetting(result: SettingsSectionSearchResult, settingKey: string): boolean {
  return !result.isSearching || result.sectionMatches || result.visibleSettingKeys.has(settingKey);
}

function SettingsSection({
  actions,
  children,
  sectionRef,
  title,
}: {
  actions?: ReactNode;
  children: ReactNode;
  sectionRef?: RefObject<HTMLDivElement | null>;
  title: string;
}) {
  return (
    <div className="settings-section-anchor" ref={sectionRef}>
      <Card
        className={cn(
          "relative mt-5 overflow-visible pb-[25px] pt-8",
          actions && "settings-section-with-actions",
        )}
        size="sm"
      >
      {/* CDXC:Settings 2026-04-26-12:31: The target settings examples stack the
          text above controls. Keeping rows vertical avoids squeezing labels in
          the narrow ghostex sidebar modal. */}
      {/* CDXC:Settings 2026-04-26-21:00: Settings sections need extra space
          above each header, while adjacent settings should separate by rhythm
          instead of divider lines. */}
      {/* CDXC:Settings 2026-04-26-21:03: Each settings category is a distinct
          shadcn card. The heading is larger and sits over the top border so
          the card reads as a labeled group without reintroducing row dividers. */}
      {/* CDXC:Settings 2026-04-26-21:22: Section card labels must stay on one
          line and clear the card contents, including multi-word headings like
          Session Cards. */}
      {/* CDXC:Settings 2026-04-27-01:01: The title pill cannot use shadcn
          CardHeader because its container-query size containment makes
          max-content resolve to the padding width instead of the text width. */}
      {/* CDXC:Settings 2026-06-12-21:00: Settings section cards need exactly
          25px of total bottom space between their last row and the card border,
          matching the compact bordered card style used by Agent Hooks and
          adjacent grouped settings sections. */}
      <div className="settings-section-title-pill">
        <CardTitle className="settings-section-title-pill-text">{title}</CardTitle>
      </div>
      {/* CDXC:UnifiedSettings 2026-05-09-17:01: Agents and Actions management
          controls belong in the section header row. Action creation labels omit
          "Add", while the agent creation CTA keeps "Add Agent" per product
          requirements. */}
      {actions ? <div className="settings-section-header-actions">{actions}</div> : null}
      <CardContent className="pt-2">
        <FieldGroup className="gap-6">{children}</FieldGroup>
      </CardContent>
      </Card>
    </div>
  );
}

function SliderNumberField({
  description,
  isModified,
  label,
  max,
  min,
  onChange,
  onCommit,
  onResetToDefault,
  step,
  value,
}: {
  description?: string;
  label: string;
  max: number;
  min: number;
  onChange: (value: number) => void;
  onCommit: (value: number) => void;
  step: number;
  value: number;
} & SettingModificationProps) {
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [inputText, setInputText] = useState(() => formatSliderNumber(value, step));
  const valueText = formatSliderNumber(value, step);

  useEffect(() => {
    if (document.activeElement !== inputRef.current) {
      setInputText(valueText);
    }
  }, [valueText]);

  const updateValue = (nextValue: number) => {
    if (!Number.isFinite(nextValue)) {
      return value;
    }
    const clampedValue = clampNumber(snapNumberToStep(nextValue, min, step), min, max);
    onChange(clampedValue);
    return clampedValue;
  };

  const commitValue = (nextValue: number) => {
    const clampedValue = Number.isFinite(nextValue)
      ? clampNumber(snapNumberToStep(nextValue, min, step), min, max)
      : value;
    setInputText(formatSliderNumber(clampedValue, step));
    onCommit(clampedValue);
  };

  const updateInputText = (nextText: string) => {
    setInputText(nextText);
    const nextValue = Number(nextText);
    if (
      nextText.trim() === "" ||
      !Number.isFinite(nextValue) ||
      nextValue < min ||
      nextValue > max
    ) {
      return;
    }
    onChange(clampNumber(snapNumberToStep(nextValue, min, step), min, max));
  };

  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className="grid grid-cols-[minmax(0,1fr)_4.75rem] items-center gap-3">
        <Slider
          aria-label={label}
          max={max}
          min={min}
          onValueCommit={([nextValue]) => commitValue(nextValue ?? value)}
          onValueChange={([nextValue]) => updateValue(nextValue ?? value)}
          step={step}
          value={[value]}
        />
        <Input
          id={id}
          className="h-10 px-3 text-sm tabular-nums"
          onBlur={(event) => commitValue(Number(event.currentTarget.value))}
          onChange={(event) => updateInputText(event.currentTarget.value)}
          onFocus={(event) => event.currentTarget.select()}
          max={max}
          min={min}
          ref={inputRef}
          step={step}
          type="number"
          value={inputText}
        />
      </div>
    </SettingRow>
  );
}

function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function snapNumberToStep(value: number, min: number, step: number): number {
  /**
   * CDXC:Settings 2026-04-29-08:56
   * Slider-backed numeric settings must persist the same step increments the
   * UI presents. This keeps Ghostty scroll multipliers on 0.25 increments even
   * when users type values into the adjacent number field.
   */
  const decimals = Math.max(0, step.toString().split(".")[1]?.length ?? 0);
  const scaledValue = Math.round((value - min) / step) * step + min;
  return Number(scaledValue.toFixed(decimals));
}

function formatSliderNumber(value: number, step: number): string {
  if (Number.isInteger(step)) {
    return String(Math.round(value));
  }
  const decimals = Math.max(0, step.toString().split(".")[1]?.length ?? 0);
  return value.toFixed(decimals);
}

function ActionButtonField({
  children,
  description,
  label,
  onClick,
}: {
  children: ReactNode;
  description?: string;
  label: string;
  onClick: () => void;
}) {
  const id = useId();
  return (
    <SettingRow description={description} htmlFor={id} label={label}>
      <Button className="h-10 w-full justify-start px-3 text-sm" id={id} onClick={onClick} type="button">
        {children}
      </Button>
    </SettingRow>
  );
}

function ActionButtonPairField({
  actions,
  description,
  label,
}: {
  actions: ReadonlyArray<{ label: string; onClick: () => void }>;
  description?: string;
  label: string;
}) {
  const id = useId();
  return (
    <SettingRow description={description} htmlFor={id} label={label}>
      <div className="grid w-full grid-cols-1 gap-2 sm:grid-cols-2">
        {actions.map((action, index) => (
          <Button
            className="h-10 w-full justify-center px-3 text-center text-sm"
            id={index === 0 ? id : undefined}
            key={action.label}
            onClick={action.onClick}
            type="button"
            variant="outline"
          >
            {action.label}
          </Button>
        ))}
      </div>
    </SettingRow>
  );
}

function SelectField({
  contentClassName,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  options,
  showScrollButtons,
  supportingContent,
  value,
}: {
  contentClassName?: string;
  description?: string;
  label: string;
  onChange: (value: string) => void;
  options: ReadonlyArray<{ label: string; value: string }>;
  showScrollButtons?: boolean;
  supportingContent?: ReactNode;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <Select items={options} onValueChange={onChange} value={value}>
        <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent className={contentClassName} showScrollButtons={showScrollButtons}>
          <SelectGroup>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      {supportingContent}
    </SettingRow>
  );
}

function PetPickerField({
  isModified,
  onChange,
  onResetToDefault,
  value,
}: {
  onChange: (value: PetId) => void;
  value: PetId;
} & SettingModificationProps) {
  const id = useId();
  const selectedPet = PET_OPTIONS.find((option) => option.id === value) ?? PET_OPTIONS[0]!;
  return (
    <SettingRow
      description="Choose the pet sprite."
      htmlFor={id}
      isModified={isModified}
      label="Pet"
      onResetToDefault={onResetToDefault}
    >
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-none border border-border bg-muted/30">
          <PetAvatar className="scale-[0.42]" petId={selectedPet.id} />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <Select onValueChange={(nextValue) => onChange(nextValue as PetId)} value={value}>
            <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {PET_OPTIONS.map((option) => (
                  <SelectItem key={option.id} value={option.id}>
                    {option.displayName}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <div className="truncate text-xs text-muted-foreground">{selectedPet.description}</div>
        </div>
      </div>
    </SettingRow>
  );
}

function StaticNoteField({ description, label }: { description: string; label: string }) {
  const id = useId();
  /**
   * CDXC:SidebarTheme 2026-05-08-11:14
   * The Sidebar Theme setting should no longer expose Auto or the previous
   * theme presets. Show a non-editable note while theme selection is paused so
   * Settings communicates the temporary product state without offering hidden
   * values.
   */
  return (
    <SettingRow description={description} htmlFor={id} label={label}>
      <div
        className="rounded-none border border-border bg-muted/30 px-3 py-2 text-sm text-muted-foreground"
        id={id}
      >
        Themes are coming back soon.
      </div>
    </SettingRow>
  );
}

function SoundField({
  description,
  isModified,
  label,
  onChange,
  onPlay,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: CompletionSoundSetting) => void;
  onPlay?: (value: CompletionSoundSetting) => void;
  value: CompletionSoundSetting;
} & SettingModificationProps) {
  /**
   * CDXC:Settings 2026-04-29-17:01
   * Sound pickers have enough options that Radix hover-scroll buttons can
   * fight wheel scrolling inside the modal. Disable those auto-scroll zones so
   * mouse and trackpad wheel direction remains stable.
   *
   * CDXC:Settings 2026-05-11-02:06
   * Every sound picker needs an adjacent icon-only preview button so users can
   * audition the selected sound without changing settings or triggering the
   * broader agent-completion notification test flow.
   */
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className="grid grid-cols-[minmax(0,1fr)_2.5rem] items-center gap-2">
        <Select
          onValueChange={(nextValue) => onChange(nextValue as CompletionSoundSetting)}
          value={value}
        >
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent className="max-h-72" showScrollButtons={false}>
            <SelectGroup>
              {COMPLETION_SOUND_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                aria-label={`Play ${label}`}
                className="h-10 w-10 rounded-none"
                disabled={!onPlay}
                onClick={() => onPlay?.(value)}
                size="icon"
                type="button"
                variant="outline"
              >
                <IconPlayerPlay aria-hidden="true" className="size-4" />
              </Button>
            }
          />
          <TooltipContent sideOffset={6}>Play selected sound</TooltipContent>
        </Tooltip>
      </div>
    </SettingRow>
  );
}

function TextField({
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  placeholder,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: string) => void;
  placeholder?: string;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <Input
        id={id}
        className="h-10 px-3 text-sm"
        onChange={(event) => onChange(event.currentTarget.value)}
        placeholder={placeholder}
        value={value}
      />
    </SettingRow>
  );
}

function DisabledCommandPreviewField({
  description,
  label,
  value,
}: {
  description?: string;
  label: string;
  value: string;
}) {
  const id = useId();
  return (
    <SettingRow description={description} htmlFor={id} label={label}>
      <Textarea
        className="min-h-24 resize-none px-3 py-2 font-mono text-xs leading-5"
        disabled
        id={id}
        readOnly
        value={value}
      />
    </SettingRow>
  );
}

function ColorField({
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: string) => void;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const colorValue = normalizeColorInputValue(value);
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className="grid grid-cols-[2.75rem_minmax(0,1fr)] items-center gap-3">
        <Input
          aria-label={`${label} picker`}
          className="h-10 cursor-pointer rounded-none p-1"
          onChange={(event) => onChange(event.currentTarget.value)}
          type="color"
          value={colorValue}
        />
        <Input
          id={id}
          className="h-10 px-3 text-sm"
          onChange={(event) => onChange(event.currentTarget.value)}
          value={value}
        />
      </div>
    </SettingRow>
  );
}

function normalizeColorInputValue(value: string): string {
  return /^#[0-9a-f]{6}$/iu.test(value.trim()) ? value.trim() : "#121212";
}

function SidebarPresetField({
  activePresetId,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
}: {
  activePresetId?: SidebarSettingsPresetId;
  description?: string;
  label: string;
  onChange: (presetId: SidebarSettingsPresetId) => void;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className="flex flex-col gap-2">
        <ToggleGroup
          aria-label={label}
          className="w-full [&>[data-slot=toggle-group-item]]:flex-1"
          onValueChange={(value) => {
            const [nextPresetId] = value as SidebarSettingsPresetId[];
            if (nextPresetId) {
              onChange(nextPresetId);
            }
          }}
          value={activePresetId ? [activePresetId] : []}
          variant="outline"
        >
          {SIDEBAR_SETTINGS_PRESETS.map((preset, index) => (
            <ToggleGroupItem
              aria-label={preset.label}
              id={index === 0 ? id : undefined}
              key={preset.id}
              value={preset.id}
            >
              {preset.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {activePresetId ? null : <span className="text-sm text-muted-foreground">Custom</span>}
      </div>
    </SettingRow>
  );
}

function ToggleField({
  checked,
  description,
  disabled,
  isModified,
  label,
  onChange,
  onResetToDefault,
}: {
  checked: boolean;
  description?: string;
  disabled?: boolean;
  label: string;
  onChange: (checked: boolean) => void;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <Switch checked={checked} disabled={disabled} id={id} onCheckedChange={onChange} />
    </SettingRow>
  );
}

/**
 * CDXC:Settings 2026-05-06-12:57
 * CDXC:SettingsModifiedState 2026-05-07-18:03
 * Every changed settings control needs a small, low-emphasis asterisk to the
 * left of its label. Position it absolutely so modified-state indication does
 * not reflow setting titles, while the tooltip action still resets only that
 * setting to DEFAULT_ghostex_SETTINGS.
 */
function SettingRow({
  children,
  description,
  htmlFor,
  isModified,
  label,
  onResetToDefault,
}: {
  children: ReactNode;
  description?: string;
  htmlFor: string;
  isModified?: boolean;
  label: string;
  onResetToDefault?: () => void;
}) {
  return (
    <Field className="gap-2.5" orientation="vertical">
      <FieldContent>
        <FieldTitle className="relative text-sm">
          {isModified && onResetToDefault ? (
            <ModifiedSettingResetButton label={label} onResetToDefault={onResetToDefault} />
          ) : null}
          <FieldLabel className="text-sm" htmlFor={htmlFor}>
            {label}
          </FieldLabel>
        </FieldTitle>
        {description ? (
          <FieldDescription className="text-sm">{description}</FieldDescription>
        ) : null}
      </FieldContent>
      <div className="min-w-0">{children}</div>
    </Field>
  );
}

function ModifiedSettingResetButton({
  label,
  onResetToDefault,
}: {
  label: string;
  onResetToDefault: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            aria-label={`Reset ${label} to default`}
            className="settings-modified-reset-button"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onResetToDefault();
            }}
            size="icon-xs"
            type="button"
            variant="ghost"
          >
            <IconAsterisk aria-hidden="true" />
          </Button>
        }
      />
      <TooltipContent className="whitespace-pre-line text-center" sideOffset={6}>
        {MODIFIED_SETTING_TOOLTIP}
      </TooltipContent>
    </Tooltip>
  );
}
