import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
  type UIEvent as ReactUIEvent,
} from "react";
import { cn } from "@/packages/components/utils";
import { Button } from "@/packages/components/ui/button";
import { Command } from "@/packages/components/ui/command";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import {
  Select,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/packages/components/ui/select";
import { Separator } from "@/packages/components/ui/separator";
import { Switch } from "@/packages/components/ui/switch";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/packages/components/ui/tabs";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/packages/components/ui/tooltip";
import { SidebarSessionSearchField } from "./sidebar-session-search-overlay";
import {
  resolveSettingsModalTabForVisibility,
  shouldShowOSIntegrationSettingsTab,
  type SettingsModalTab,
  type SettingsModalTabVisibilityOptions,
} from "./settings-modal-tabs";
import {
  IconAlertTriangle,
  IconCashEdit,
  IconChevronDown,
  IconChevronRight,
  IconCloud,
  IconCodeDots,
  IconDeviceDesktop,
  IconExternalLink,
  IconFolderOpen,
  IconInfoCircle,
  IconKeyboard,
  IconPlayerPlay,
  IconSettings,
  IconTools,
} from "@tabler/icons-react";
import {
  COMPLETION_SOUND_OPTIONS,
  type CompletionSoundSetting,
} from "../shared/completion-sound";
import { GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES } from "../shared/ghostty-config-actions";
import {
  resolveSidebarTheme,
  type SidebarAppIconStateMessage,
  type SidebarAgentHookStatusMessage,
  type SidebarGhostexCliStatusMessage,
  type SidebarGhostexFolderStatsMessage,
  type SidebarOSIntegrationStatusMessage,
  type SidebarPluginSettingsItem,
  type SidebarPluginSettingsStatusMessage,
  type SidebarPortlessState,
  type SidebarProjectSettingsItem,
  type SidebarTheme,
  type SidebarThemeVariant,
} from "../shared/session-grid-contract";
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  DEFAULT_ghostex_SETTINGS,
  MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MAX_TERMINAL_PANE_PADDING_PX,
  MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  DIAGNOSTIC_LOGGING_SCENARIOS,
  GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
  GHOSTTY_COPY_ON_SELECT_OPTIONS,
  GHOSTTY_SCROLLBAR_OPTIONS,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  PREFERRED_AGENT_INTERFACE_OPTIONS,
  MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MIN_TERMINAL_PANE_PADDING_PX,
  MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  WINDOWS_TERMINAL_BACKEND_OPTIONS,
  type PromptEditorBackend,
  SESSION_PERSISTENCE_PROVIDER_OPTIONS,
  SESSION_CHAT_THEME_OPTIONS,
  SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS,
  SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_SIDE_OPTIONS,
  SIDEBAR_VERSION_OPTIONS,
  WEB_LINK_OPEN_TARGET_OPTIONS,
  applySidebarSettingsPreset,
  areDiagnosticLoggingSettingsEqual,
  getSidebarSettingsPresetId,
  COMMANDS_PANEL_SIDE_OPTIONS,
  MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  MAX_SIDEBAR_DEFAULT_WIDTH_PX,
  MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  MIN_SIDEBAR_DEFAULT_WIDTH_PX,
  SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS,
  SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP,
  normalizeghostexSettings,
  parseSidebarAutoSettleAfterDaysSelectValue,
  setDiagnosticLoggingScenario,
  sidebarAutoSettleAfterDaysSelectValue,
  type AutoSleepIdleMinutes,
  type DiagnosticLoggingScenarioId,
  type GhosttyConfirmCloseSurface,
  type GhosttyCopyOnSelect,
  type GhosttyScrollbar,
  type KeepAwakeDurationMinutes,
  type SessionPersistenceProvider,
  type SettingsModalNavigationState,
  type SidebarSettingsPresetId,
  type CommandsPanelSide,
  type SidebarSide,
  type TerminalBackgroundImageFit,
  type WebLinkOpenTarget,
  type TerminalCursorStyle,
  type ghostexSettingsPatch,
  type ghostexSettingsUpdateSource,
  type ghostexSettings,
} from "../shared/ghostex-settings";
import { type BundledGhostexAgentSkillId } from "../shared/ghostex-agent-skills";
import {
  FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS,
  isFirstLaunchSetupMainSettingVisible,
  type FirstLaunchSetupMainSettingKey,
} from "../shared/first-launch-setup-settings";
import { GHOSTEX_HOTKEY_DEFINITIONS } from "../shared/ghostex-hotkeys";
import {
  PET_CONTROLS_VISIBLE,
  PET_OPTIONS,
} from "../shared/pets";
import {
  areSidebarSessionTagListItemsEqual,
  DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  getSidebarSessionTagListItemLabel,
} from "../shared/session-tags";
import { type WebviewApi } from "./webview-api";
import {
  ActionButtonPairField,
  AppIconPickerField,
  ColorField,
  DiagnosticLoggingSettingsField,
  PetPickerField,
  PreferredAgentInterfaceField,
  SelectField,
  SessionChatThemeField,
  SettingButton,
  SettingRow,
  SettingsNativeScrollArea,
  SettingsSection,
  SettingsSelect,
  SettingsSelectContent,
  SidebarPresetField,
  SidebarProjectGroupStyleField,
  SidebarTagListSettingsField,
  SidebarVersionField,
  SliderNumberField,
  SoundField,
  StaticNoteField,
  TerminalDevServerIgnoredPortsField,
  TextField,
  ToggleField,
  WebColorPickerField,
  getDiagnosticLoggingScenarioStateForDuration,
} from "./settings-modal/fields";
import {
  areSettingsModalNavigationStatesEqual,
  getRememberedSettingsModalNavigationState,
  getRememberedSettingsModalScrollTop,
  getRememberedSettingsModalTab,
  rememberSettingsModalScrollTop,
  rememberSettingsModalTab,
} from "./settings-modal/navigation-memory";
import {
  SearchableExtraSettingsTabId,
  getExtraSettingsTabSearches,
  getGroupedSettingsSectionSearch,
  getHotkeySettingsSectionSearches,
  getMostlyVisibleSettingsSectionId,
  getSettingsSectionSearch,
  isAdvancedMainSetting,
  settingsTabSearchHasMatches,
  shouldShowSetting,
  shouldShowSettingsSection,
} from "./settings-modal/search";
import { AboutSettingsTab } from "./settings-modal/tabs/about";
import { ActionsSettingsTab } from "./settings-modal/tabs/actions";
import { AgentsSettingsTab } from "./settings-modal/tabs/agents";
import { HotkeysSettingsTab } from "./settings-modal/tabs/hotkeys";
import { IntegrationsSettingsTab } from "./settings-modal/tabs/integrations";
import { OpenTargetsSettingsTab } from "./settings-modal/tabs/open-targets";
import { OSIntegrationSettingsTab } from "./settings-modal/tabs/os-integration";
import { PluginsSettingsTab } from "./settings-modal/tabs/plugins";
import { ProjectsSettingsPanel } from "./settings-modal/tabs/projects";
import { RemoteSettingsTab } from "./settings-modal/tabs/remote";
import {
  DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET,
  DiagnosticLoggingDurationValue,
  HOTKEY_SETTINGS_SECTIONS,
  HotkeySettingsDefinitionById,
  HotkeySettingsSectionId,
  HotkeySettingsSectionRefs,
  HotkeySettingsSectionSearches,
  MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS,
  MAIN_SETTINGS_SECTION_SETTING_KEYS,
  MAIN_SETTINGS_SUBSECTION_NAVIGATION,
  MainSettingsScrollTargetId,
  MainSettingsSectionId,
  MainSettingsSectionRefs,
  MainSettingsSubsectionNavigationItem,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
  SettingModificationProps,
  SettingsSectionMeasurementItem,
  SettingsSectionNavigationItem,
  SettingsSectionSearchResult,
  SettingsSidebarPage,
  getMainSettingsSectionGroupId,
} from "./settings-modal/types";

export type { SettingsModalTab } from "./settings-modal-tabs";


const IS_WINDOWS_HOST =
  typeof navigator !== "undefined" && /Windows/iu.test(navigator.userAgent);
const NUMERIC_SETTINGS_DEBOUNCE_MS = 180;
const SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS = 220;
const GHOSTTY_THEME_UNMANAGED_VALUE = "__ghostex_ghostty_theme_unmanaged__";

const PASTE_PREVIEWABLE_IMAGES_DESCRIPTION =
  "Paste clipboard images as previewable Markdown links with Cmd+V or Ctrl+V. Hold Cmd over the linked path to preview it in the terminal, and see the same image preview in the Ctrl+G Rich Prompt Editor.";


export type MainSettingsInitialSectionId = MainSettingsScrollTargetId;

function getInitialSettingsModalTab(
  initialTab: SettingsModalTab,
  visibility: SettingsModalTabVisibilityOptions,
  storedNavigation: SettingsModalNavigationState,
): SettingsModalTab {
  /**
   * CDXC:Settings 2026-05-11-09:06
   * Settings remembers the last selected tab during the current app session. A
   * non-default entry point such as Hotkeys still opens its requested tab, then
   * that tab becomes the remembered choice for later ordinary Settings opens.
   *
   * CDXC:SettingsNavigation 2026-06-29-17:54:
   * Ordinary Settings opens should also restore the last closed Settings tab
   * from durable macOS settings storage after an app relaunch. Explicit entry
   * points still win so menu actions and deep links land on the requested page.
   */
  const requestedTab =
    initialTab !== "settings"
      ? initialTab
      : getRememberedSettingsModalTab(storedNavigation) ?? initialTab;
  return resolveSettingsModalTabForVisibility(requestedTab, visibility);
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

function isEditableSettingsModalElement(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) {
    return false;
  }
  if (element.isContentEditable) {
    return true;
  }
  return Boolean(element.closest("input, textarea, select, [contenteditable='true']"));
}

function getActiveSettingsModalScrollViewport(dialogElement: HTMLElement | null): HTMLElement | null {
  return (
    dialogElement
      ?.querySelector<HTMLElement>("[role='tabpanel'][data-state='active']")
      ?.querySelector<HTMLElement>("[data-slot='scroll-area-viewport']") ?? null
  );
}

function getMainSettingsSectionRef(
  sectionId: MainSettingsScrollTargetId,
  refs: MainSettingsSectionRefs,
): RefObject<HTMLDivElement | null> {
  return refs[sectionId];
}


function createNormalizedSettingsPatch(
  normalizedSettings: ghostexSettings,
  patch: ghostexSettingsPatch,
): ghostexSettingsPatch {
  return Object.fromEntries(
    (Object.keys(patch) as Array<keyof ghostexSettings>).map((key) => [
      key,
      normalizedSettings[key],
    ]),
  ) as ghostexSettingsPatch;
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
  automateIsExperimental?: boolean;
  firstLaunchSetupVisibleSettings?: ReadonlySet<FirstLaunchSetupMainSettingKey>;
  initialSection?: MainSettingsInitialSectionId;
  initialSearchQuery?: string;
  initialRemoteMachineId?: string;
  initialTab?: SettingsModalTab;
  isOpen: boolean;
  presentation?: SettingsModalPresentation;
  onChange: (settings: ghostexSettings, source?: ghostexSettingsUpdateSource) => void;
  onPatch?: (patch: ghostexSettingsPatch, source: ghostexSettingsUpdateSource) => void;
  onClose: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenMacOSNotificationSettings?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onOpenGhostexFolder?: () => void;
  onGhosttySettingsAction?: (action: GhosttySettingsAction) => void;
  onInstallAgentOrchestrationSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallFindPrevSessionSkill?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGhostexCli?: () => void;
  onInstallMoveCodexSessionSkill?: () => void;
  onPlayCompletionSound?: (sound: CompletionSoundSetting) => void;
  onRequestMacOSNotificationPermission?: () => void;
  onInstallAgentHooks?: () => void;
  onUninstallAgentHooks?: (agentIds?: readonly string[]) => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
  onUninstallBundledAgentSkills?: () => void;
  onRequestAgentHookStatus?: () => void;
  onRequestGhostexCliStatus?: () => void;
  onRequestGhostexFolderStats?: () => void;
  onRequestOSIntegrationStatus?: () => void;
  onRequestPluginSettingsStatus?: () => void;
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem["id"]) => void;
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
  pluginSettingsStatus?: SidebarPluginSettingsStatusMessage;
  pluginSettingsStatusLoading?: boolean;
  // CDXC:AppIconPicker 2026-06-25-21:50: Native App Icon state arrives prop-driven via the modal-state relay.
  appIconState?: SidebarAppIconStateMessage;
  /** Hosts without a native App Icon subsystem hide the section entirely. */
  appIconPickerUnavailable?: boolean;
  portless?: SidebarPortlessState;
};

export function SettingsModal({
  agentHookStatus,
  agentHookStatusLoading = false,
  automateIsExperimental = true,
  firstLaunchSetupVisibleSettings,
  initialSection,
  initialSearchQuery,
  initialRemoteMachineId,
  initialTab = "settings",
  isOpen,
  onChange,
  onPatch,
  onClose,
  presentation = "default",
  onOpenAccessibilityPreferences,
  onOpenMacOSNotificationSettings,
  onOpenScreenRecordingPreferences,
  onOpenGhostexFolder,
  onGhosttySettingsAction,
  onInstallAgentOrchestrationSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallFable56OrchestrationSkill,
  onInstallFindPrevSessionSkill,
  onInstallGenerateTitleSkill,
  onInstallGhostexCli,
  onInstallMoveCodexSessionSkill,
  onPlayCompletionSound,
  onRequestMacOSNotificationPermission,
  onInstallAgentHooks,
  onUninstallAgentHooks,
  onUninstallBundledAgentSkill,
  onUninstallBundledAgentSkills,
  onRequestAgentHookStatus,
  onRequestGhostexCliStatus,
  onRequestGhostexFolderStats,
  onRequestOSIntegrationStatus,
  onRequestPluginSettingsStatus,
  onReinstallPlugin,
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
  pluginSettingsStatus,
  pluginSettingsStatusLoading = false,
  // CDXC:AppIconPicker 2026-06-25-21:50: Prop-driven App Icon state replaces direct host-event listeners.
  appIconState,
  appIconPickerUnavailable = false,
  portless,
}: SettingsModalProps) {
  const isFirstLaunchSetup = presentation === "firstLaunchSetup";
  const normalizedInitialSettings = normalizeghostexSettings(settings);
  const [draft, setDraft] = useState<ghostexSettings>(normalizedInitialSettings);
  /*
   * CDXC:SettingsAdvanced 2026-06-28-18:14:
   * Show Advanced must use the persisted settings draft as its single source of
   * truth. A separate React state can initialize before native settings hydrate
   * and make the switch look disabled again when Settings reopens.
   */
  const showAdvancedSettings = draft.showAdvancedSettings;
  const [settingsSearchQuery, setSettingsSearchQuery] = useState("");
  const [activeMainSettingsSectionId, setActiveMainSettingsSectionId] =
    useState<MainSettingsScrollTargetId>("sidebar");
  const [activeHotkeySettingsSectionId, setActiveHotkeySettingsSectionId] =
    useState<HotkeySettingsSectionId>("general");
  const [expandedSettingsSidebarPages, setExpandedSettingsSidebarPages] = useState<
    Partial<Record<SettingsModalTab, boolean>>
  >({
    settings: true,
  });
  const showOSIntegrationSettingsTab = shouldShowOSIntegrationSettingsTab({
    isFirstLaunchSetup,
    showBetaFeatures: draft.showBetaFeatures,
  });
  const [activeTab, setActiveTabState] = useState<SettingsModalTab>(() =>
    getInitialSettingsModalTab(
      initialTab,
      {
        showOSIntegrationSettingsTab: shouldShowOSIntegrationSettingsTab({
          isFirstLaunchSetup,
          showBetaFeatures: normalizedInitialSettings.showBetaFeatures,
        }),
      },
      normalizedInitialSettings.settingsModalNavigation,
    ),
  );
  const dialogContentRef = useRef<HTMLDivElement>(null);
  const showAdvancedSettingsId = useId();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const pendingSettingsRef = useRef<ghostexSettings | undefined>(undefined);
  const pendingSettingsPatchRef = useRef<ghostexSettingsPatch | undefined>(undefined);
  const pendingTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pendingNavigationPersistTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const autoSleepSectionRef = useRef<HTMLDivElement>(null);
  const browserSectionRef = useRef<HTMLDivElement>(null);
  const editorSectionRef = useRef<HTMLDivElement>(null);
  const ghosttyBehaviorSectionRef = useRef<HTMLDivElement>(null);
  const ghosttyScrollingSectionRef = useRef<HTMLDivElement>(null);
  const ghosttyTerminalSectionRef = useRef<HTMLDivElement>(null);
  const terminalDevServersSectionRef = useRef<HTMLDivElement>(null);
  const powerSectionRef = useRef<HTMLDivElement>(null);
  const statusIndicatorsSectionRef = useRef<HTMLDivElement>(null);
  const sessionCardsSectionRef = useRef<HTMLDivElement>(null);
  const debuggingSectionRef = useRef<HTMLDivElement>(null);
  const betaSectionRef = useRef<HTMLDivElement>(null);
  const agentsOnboardingSectionRef = useRef<HTMLDivElement>(null);
  const sidebarSectionRef = useRef<HTMLDivElement>(null);
  const themingSectionRef = useRef<HTMLDivElement>(null);
  const chatSectionRef = useRef<HTMLDivElement>(null);
  // CDXC:AppIconPicker 2026-06-25-21:50: Anchor ref so the App Icon section participates in Settings nav scrolling.
  const appIconSectionRef = useRef<HTMLDivElement>(null);
  const sidebarTagsSectionRef = useRef<HTMLDivElement>(null);
  const soundsSectionRef = useRef<HTMLDivElement>(null);
  const storageSectionRef = useRef<HTMLDivElement>(null);
  const hotkeyActionsSectionRef = useRef<HTMLDivElement>(null);
  const hotkeyGeneralSectionRef = useRef<HTMLDivElement>(null);
  const hotkeyNavigationSectionRef = useRef<HTMLDivElement>(null);
  const hotkeyPaneActionsSectionRef = useRef<HTMLDivElement>(null);
  const hotkeyProjectsSectionRef = useRef<HTMLDivElement>(null);
  const hotkeySessionSlotsSectionRef = useRef<HTMLDivElement>(null);
  const hasRequestedStorageStatsRef = useRef(false);
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * The App Icon picker is prop-driven: native pushes appIconState through the
   * modal-state relay (mirroring osIntegrationStatus), so this component only
   * holds the local error string and the in-flight pending selection. The
   * pending source id lets confirm-before-persist write the user's selection on
   * the next ok state instead of native's reported selectedId.
   */
  const [appIconError, setAppIconError] = useState<string | undefined>(undefined);
  const pendingAppIconSourceIdRef = useRef<string | undefined>(undefined);
  const handledAppIconStateRef = useRef<SidebarAppIconStateMessage | undefined>(undefined);
  const hasRequestedAppIconsRef = useRef(false);
  const pendingMainSettingsSectionViewportRef = useRef<HTMLElement | null>(null);
  const mainSettingsSectionFrameRef = useRef<number | undefined>(undefined);
  const modalTheme = resolveSidebarTheme(draft.sidebarTheme, getSidebarThemeVariant(theme));
  const isModalDarkTheme = getSidebarThemeVariant(modalTheme) === "dark";
  const rememberActiveScrollPosition = () => {
    const viewport = getActiveSettingsModalScrollViewport(dialogContentRef.current);
    if (viewport) {
      rememberSettingsModalScrollTop(activeTab, viewport.scrollTop);
    }
  };
  const shouldFocusSettingsSearchInput = useCallback((inputElement: HTMLInputElement): boolean => {
    /*
     * CDXC:SettingsSearch 2026-06-25-21:21:
     * The visible Settings search field may prefill from deep links and
     * printable-key capture, but it must never steal typing focus from an
     * already-focused input, textarea, select, or contenteditable field,
     * including Settings popover fields rendered through portals. Let search
     * refocus itself while it is active and otherwise focus only when no
     * editable control owns the user's text entry.
     */
    const activeElement = inputElement.ownerDocument.activeElement;
    if (!activeElement || activeElement === inputElement) {
      return true;
    }
    return !isEditableSettingsModalElement(activeElement);
  }, []);
  const focusSearchInput = useCallback((): boolean => {
    if (isFirstLaunchSetup) {
      return false;
    }
    const inputElement = searchInputRef.current;
    if (!inputElement || !shouldFocusSettingsSearchInput(inputElement)) {
      return false;
    }
    inputElement.focus({ preventScroll: true });
    return true;
  }, [isFirstLaunchSetup, shouldFocusSettingsSearchInput]);
  const scheduleMainSettingsSectionMeasurement = (viewport: HTMLElement) => {
    /*
     * CDXC:SettingsPerformance 2026-06-29-00:40:
     * General Settings is long, and section tracking reads layout for every
     * visible section. Batch that work to one requestAnimationFrame per scroll
     * frame so raw scroll events only persist scrollTop and stay lightweight.
     */
    pendingMainSettingsSectionViewportRef.current = viewport;
    if (mainSettingsSectionFrameRef.current !== undefined) {
      return;
    }
    mainSettingsSectionFrameRef.current = requestAnimationFrame(() => {
      mainSettingsSectionFrameRef.current = undefined;
      const pendingViewport = pendingMainSettingsSectionViewportRef.current;
      pendingMainSettingsSectionViewportRef.current = null;
      if (!pendingViewport?.isConnected) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        pendingViewport,
        getMainSettingsSectionMeasurementItems(),
      );
      if (mostlyVisibleSectionId) {
        setActiveMainSettingsSectionId((currentSectionId) =>
          currentSectionId === mostlyVisibleSectionId ? currentSectionId : mostlyVisibleSectionId,
        );
      }
    });
  };
  const handleSettingsModalScrollCapture = (event: ReactUIEvent<HTMLDivElement>) => {
    if (event.target instanceof HTMLElement && event.target.dataset.slot === "scroll-area-viewport") {
      rememberSettingsModalScrollTop(activeTab, event.target.scrollTop);
      scheduleSettingsModalNavigationPersist(activeTab);
      if (activeTab === "settings") {
        scheduleMainSettingsSectionMeasurement(event.target);
      }
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
      event.key.length !== 1 ||
      isEditableSettingsModalEventTarget(event.target) ||
      isEditableSettingsModalElement(event.currentTarget.ownerDocument.activeElement)
    ) {
      return;
    }

    event.preventDefault();
    setSettingsSearchQuery(`${settingsSearchQuery}${event.key}`);
    requestAnimationFrame(focusSearchInput);
  };
  const setActiveTab = (nextTab: SettingsModalTab) => {
    const visibleTab = resolveSettingsModalTabForVisibility(nextTab, {
      showOSIntegrationSettingsTab,
    });
    rememberActiveScrollPosition();
    rememberSettingsModalTab(visibleTab);
    persistSettingsModalNavigation(visibleTab);
    if (visibleTab === "settings" || visibleTab === "hotkeys") {
      setExpandedSettingsSidebarPages((expandedPages) => ({
        ...expandedPages,
        [visibleTab]: true,
      }));
    }
    setActiveTabState(visibleTab);
  };

  const toggleSettingsSidebarPage = (pageId: SettingsModalTab) => {
    setExpandedSettingsSidebarPages((expandedPages) => ({
      ...expandedPages,
      [pageId]: !expandedPages[pageId],
    }));
  };

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    const nextTab = getInitialSettingsModalTab(
      initialTab,
      { showOSIntegrationSettingsTab },
      (pendingSettingsRef.current ?? draft).settingsModalNavigation,
    );
    rememberActiveScrollPosition();
    rememberSettingsModalTab(nextTab);
    persistSettingsModalNavigation(nextTab);
    setActiveTabState(nextTab);
  }, [initialTab, isOpen]);

  useEffect(() => {
    if (activeTab !== "osIntegration" || showOSIntegrationSettingsTab) {
      return;
    }
    rememberSettingsModalTab("settings");
    setActiveTabState("settings");
  }, [activeTab, showOSIntegrationSettingsTab]);

  useEffect(() => {
    return () => {
      if (mainSettingsSectionFrameRef.current !== undefined) {
        cancelAnimationFrame(mainSettingsSectionFrameRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!isOpen || isFirstLaunchSetup || !initialSearchQuery?.trim()) {
      return;
    }
    const nextQuery = initialSearchQuery.trim();
    /**
     * CDXC:SessionPersistence 2026-06-04-02:52:
     * Titlebar Tips notices can deep-link into Settings by opening a searchable
     * tab and pre-filling the search box with the setting label. Seed the
     * correct tab-specific query instead of typing through the DOM so repeated
     * opens land on the intended control without depending on focus timing.
     *
     * CDXC:SettingsNavigation 2026-06-24-22:16:
     * Settings has one top search field for the sidebar-driven modal. Seed the
     * shared Settings query for every non-first-launch entry point so Hotkeys
     * and General use the same search state.
     */
    setSettingsSearchQuery(nextQuery);
    const animationFrame = requestAnimationFrame(() => {
      if (focusSearchInput()) {
        searchInputRef.current?.select();
      }
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [focusSearchInput, initialSearchQuery, initialTab, isFirstLaunchSetup, isOpen]);

  useEffect(() => {
    if (
      !isOpen ||
      !showOSIntegrationSettingsTab ||
      activeTab !== "osIntegration" ||
      osIntegrationStatus ||
      osIntegrationStatusLoading
    ) {
      return;
    }
    onRequestOSIntegrationStatus?.();
  }, [
    activeTab,
    isOpen,
    onRequestOSIntegrationStatus,
    osIntegrationStatus,
    osIntegrationStatusLoading,
    showOSIntegrationSettingsTab,
  ]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    /**
     * CDXC:SettingsNavigation 2026-05-26-18:47:
     * During one app session, reopening Settings should return to the same tab
     * and scroll position the user left. Keep that state in module memory so it
     * survives modal remounts.
     *
     * CDXC:SettingsNavigation 2026-06-29-17:54:
     * App relaunch should also restore the last closed Settings location from
     * persisted settings, while in-memory state remains the fastest source for
     * repeated opens during the same app run.
     *
     * CDXC:SettingsSearch 2026-05-26-18:47:
     * When a searchable Settings tab opens, ordinary typing should enter the
     * active tab's search box even if Radix focus starts on a tab, button, or
     * another non-text control. Text fields and recorders keep their own input.
     *
     * CDXC:SettingsSearch 2026-06-19-16:53:
     * Settings search must not steal printable keys from a focused Settings
     * text field during native settings round-trips. Check both the key event
     * target and the document active element before forwarding a character to
     * the search box because WebKit can dispatch through modal chrome while the
     * editable field still owns focus.
     */
    const animationFrame = requestAnimationFrame(() => {
      const viewport = getActiveSettingsModalScrollViewport(dialogContentRef.current);
      if (viewport) {
        viewport.scrollTop = getRememberedSettingsModalScrollTop(
          activeTab,
          (pendingSettingsRef.current ?? draft).settingsModalNavigation,
        );
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
    if (!isOpen || (activeTab !== "integrations" && activeTab !== "plugins")) {
      return;
    }
    /**
     * CDXC:CuaDriverPlugins 2026-08-09:
     * Integrations needs CLI, skill, and macOS permission state; Plugins uses
     * the same native-owned payload for Cua Driver version/update state. Probe
     * only while one of those pages is active.
     *
     * CDXC:AgentHookSettings 2026-08-19-11:20:
     * Hook install, per-agent status, and hook removal all live in Settings -> Agents, so Integrations no longer probes hook status at all.
     *
     * CDXC:ComputerAgentControl 2026-05-27-06:58:
     * Settings should present the public skill names Ghostex Browser Use and Ghostex Computer Use.
     */
    if (!ghostexCliStatus && !ghostexCliStatusLoading) {
      onRequestGhostexCliStatus?.();
    }
  }, [activeTab, ghostexCliStatus, ghostexCliStatusLoading, isOpen, onRequestGhostexCliStatus]);

  /**
   * CDXC:SettingsSearch 2026-05-04-02:30
   * Settings search must be fuzzy and cover section titles, setting subtitles,
   * and selectable option text so users can find controls by the value they
   * want to choose, not only by the visible setting label.
   */
  const settingsSearch = {
    // CDXC:AppIconPicker 2026-06-25-21:50: Make the App Icon section findable by Settings search.
    appIcon: getSettingsSectionSearch(settingsSearchQuery, "App Icon", [
      {
        key: "appIconSourceId",
        subtitle:
          "Choose the macOS Dock and app-switcher icon. The app file icon may also change when macOS allows it.",
        title: "App Icon",
      },
    ]),
    builtInFeatures: getSettingsSectionSearch(
      settingsSearchQuery,
      "Built-in feature switches",
      [
        {
          key: "codeViewTabHidden",
          subtitle: "Show or hide Code in the title bar without disabling its runtime.",
          title: "Code",
        },
        {
          key: "kanbanViewTabHidden",
          subtitle: "Show or hide Kanban in the title bar without disabling its runtime.",
          title: "Kanban",
        },
        {
          key: "automateViewTabHidden",
          subtitle: "Show or hide Automate in the title bar without disabling its runtime.",
          title: "Automate",
        },
        {
          key: "docsViewTabHidden",
          subtitle: "Show or hide Docs in the title bar without disabling its runtime.",
          title: "Docs",
        },
      ],
    ),
    browser: getSettingsSectionSearch(settingsSearchQuery, "Browser", [
      {
        key: "webLinkOpenTarget",
        options: WEB_LINK_OPEN_TARGET_OPTIONS,
        subtitle:
          "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.",
        title: "Open links in",
      },
    ]),
    editor: getSettingsSectionSearch(settingsSearchQuery, "Editor", [
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
        key: "keepAwakeWhileWorkingSessions",
        subtitle: "Keep the Mac awake while sessions are working and for 20 minutes after.",
        title: "Keep awake for working sessions",
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
        key: "useColoredSessionAgentIcons",
        subtitle: "Render session and selected-agent logos with colored brand artwork instead of monochrome masks.",
        title: "Use colored agent icons",
      },
      /*
       * CDXC:SidebarSessions 2026-05-15-19:46:
       * Settings must not expose the card-hotkey visibility row; session-card shortcut visibility is no longer configurable from the modal.
       */
      {
        key: "showSessionCloseContextMenuAction",
        subtitle: "Show the Close item in session context menus.",
        title: "Show Close option in context menu",
      },
    ]),
    statusIndicators: getSettingsSectionSearch(
      settingsSearchQuery,
      "Status Indicators",
      PET_CONTROLS_VISIBLE
        ? [
            /*
             * CDXC:StatusIndicators 2026-05-20-12:00:
             * Status Indicators groups session presence surfaces that communicate
             * status at a glance.
             *
             * CDXC:StatusIndicators 2026-06-27-20:11:
             * The removed floating session badge and its size selector must not
             * appear in macOS or GPUI Settings.
             *
             * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
             * The menu bar session indicator now lives under Sidebar because sidebar
             * presets mutate it.
             */
            {
              key: "petOverlayEnabled",
              subtitle: "Show the draggable animated pet in the native sidebar.",
              title: "Wake Pet",
            },
            {
              key: "selectedPetId",
              options: PET_OPTIONS.map((option) => ({
                label: option.displayName,
                value: option.id,
              })),
              subtitle: "Choose the pet sprite.",
              title: "Pet",
            },
          ]
        : [],
    ),
    sidebar: getSettingsSectionSearch(settingsSearchQuery, "Sidebar", [
      /*
       * CDXC:SidebarV2 2026-07-29:
       * Sidebar version must be findable by searching for the new Inbox
       * sidebar, its Classic alternative, or the Group by Project sub-mode.
       */
      {
        key: "sidebarVersion",
        options: SIDEBAR_VERSION_OPTIONS,
        subtitle:
          "Choose the classic sidebar or the new Inbox sidebar, a flat list of sessions across all projects.",
        title: "Sidebar version",
      },
      {
        key: "sidebarV2Layout",
        subtitle: "Group Inbox sidebar sessions into collapsible project groups.",
        title: "Group by project",
      },
      /*
       * CDXC:SidebarV2Lifecycle 2026-07-29:
       * Auto-settle must be findable by searching for "settle", "snooze", or
       * "inactive", because the visible symptom is a session leaving the inbox.
       */
      {
        key: "sidebarAutoSettleAfterDays",
        options: SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS,
        subtitle:
          "Move inactive Inbox sidebar sessions to the Settled shelf after this many days. Working and blocked sessions never settle.",
        title: "Auto-settle inactive sessions",
      },
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
        key: "sidebarProjectGroupStyle",
        options: SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
        subtitle: "Choose how project groups are marked in the sidebar.",
        title: "Project group style",
      },
      {
        key: "showProjectIcons",
        subtitle: "Show project artwork or a folder or worktree icon beside project names.",
        title: "Show project icons",
      },
      /*
       * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
       * Search metadata follows the visible row order: preset-controlled rows
       * sit immediately after Preset, before independent sidebar sizing and
       * placement controls.
       */
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
      {
        key: "hideLastActiveTimeOnSessionCards",
        subtitle: "Hide Last Active timestamps from session-card title rows.",
        title: "Hide last active time",
      },
      {
        key: "hideProjectHeaderDiffStats",
        subtitle: "Hide +added/-removed line counts in sidebar project rows.",
        title: "Hide project git stats",
      },
      {
        key: "showProjectEditorDiffFileCount",
        subtitle: "Show changed-file counts in sidebar project row git stats.",
        title: "Show changed-file count",
      },
      {
        key: "hideMenuBarSessionStatusIndicators",
        subtitle: "Show the menu bar session status badges.",
        title: "Show Menu Bar Session Indicators",
      },
      {
        key: "sidebarSide",
        options: SIDEBAR_SIDE_OPTIONS,
        subtitle: "Choose which side of the screen holds the sidebar.",
        title: "Side",
      },
      {
        key: "sidebarCollapseAnimationDurationMs",
        subtitle: "Set how quickly sidebar sections, groups, and projects expand or collapse. Set to 0 for no animation.",
        title: "Collapse animation speed",
      },
      {
        key: "sidebarDefaultWidthPx",
        subtitle: "Width restored when double-clicking the sidebar resize handle.",
        title: "Default Width",
      },
      {
        key: "commandsPanelDefaultHeightPx",
        subtitle: "Height used when opening the command pane and when double-clicking its top resize rail.",
        title: "Command Pane Default Height",
      },
      {
        key: "commandsPanelSide",
        options: COMMANDS_PANEL_SIDE_OPTIONS,
        subtitle: "Dock the command pane below the workspace or to its right.",
        title: "Command Pane Side",
      },
      {
        key: "projectSessionListCollapsedCount",
        subtitle: "Number of project sessions kept visible after Show less.",
        title: "Show Less Count",
      },
      {
        key: "agentManagerZoomPercent",
        subtitle: "Scale the sidebar interface.",
        title: "Sidebar Interface Size",
      },
      {
        key: "createSessionOnSidebarDoubleClick",
        subtitle: "Create a session from empty sidebar space.",
        title: "Double-click empty sidebar space to create a session",
      },
      {
        key: "renameSessionOnDoubleClick",
        subtitle: RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
        title: RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
      },
    ]),
    theming: getSettingsSectionSearch(settingsSearchQuery, "Theming", [
      {
        key: "sidebarTheme",
        subtitle: "Light theme coming soon.",
        title: "Theme",
      },
      {
        key: "customSidebarTitlebarBackgroundDarknessPercent",
        subtitle: "Contrast level for the sidebar and titlebar background.",
        title: "Background Contrast",
      },
      {
        key: "customSidebarTitlebarBackgroundTintColor",
        subtitle: "Subtle tint color for the sidebar and titlebar background.",
        title: "Background Tint",
      },
      {
        key: "workspaceActivePaneBorderColor",
        subtitle: "CSS color for the focused pane border.",
        title: "Active Pane Border",
      },
    ]),
    chat: getSettingsSectionSearch(settingsSearchQuery, "Chat", [
      {
        key: "preferredAgentInterface",
        options: PREFERRED_AGENT_INTERFACE_OPTIONS,
        subtitle:
          "Automatically switch to chat as soon as Ghostex detects that an agent session supports it.",
        title: "Default view for compatible agents",
      },
      {
        key: "sessionChatTheme",
        options: SESSION_CHAT_THEME_OPTIONS,
        subtitle: "Choose the palette used by chat messages, thinking, tools, edits, and Markdown.",
        title: "Chat appearance",
      },
      {
        key: "sessionChatFontFamily",
        subtitle: "Use any installed font in chat messages and the prompt composer.",
        title: "Chat font family",
      },
      {
        key: "sessionChatTranscriptWidthPercent",
        subtitle: "Set the width of the message transcript without changing the prompt composer.",
        title: "Chat message width",
      },
      {
        key: "sessionChatVerboseMode",
        subtitle:
          "Expand thinking blocks to show their tool calls by default. Each chat can override it from its composer.",
        title: "Verbose mode",
      },
    ]),
    sidebarTags: getSettingsSectionSearch(settingsSearchQuery, "Sidebar Tags", [
      {
        key: "sidebarSessionTagListItems",
        options: [
          ...DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS.map((item) => ({
            label: getSidebarSessionTagListItemLabel(item),
            value: item.id,
          })),
          { label: "Hide tag", value: "hide" },
          { label: "Disable tag", value: "disable" },
          { label: "Reorder tags", value: "reorder" },
        ],
        subtitle:
          "Reorder, hide, or disable sidebar tag filters and their separators.",
        title: "Tag Filter List",
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
        subtitle: "Show Ghostex data-folder sizes and open the resolved storage folder.",
        title: "Ghostex folder",
      },
    ]),
    terminal: getSettingsSectionSearch(settingsSearchQuery, "Terminal", [
      ...(IS_WINDOWS_HOST
        ? [
            {
              key: "windowsTerminalBackend",
              options: WINDOWS_TERMINAL_BACKEND_OPTIONS,
              subtitle: "Windows terminals currently run through WSL2.",
              title: "Windows terminal backend",
            },
            {
              key: "windowsWslDistribution",
              subtitle:
                "Optional exact distro name from `wsl.exe --list --verbose`; blank uses automatic WSL2 discovery.",
              title: "WSL distribution",
            },
          ]
        : []),
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
        key: "workspaceBackgroundColor",
        subtitle: "Color shown behind terminal panes.",
        title: "Terminal Background",
      },
      {
        key: "terminalBackgroundImage",
        subtitle: "Absolute path to an image drawn behind terminal panes.",
        title: "Background Image",
      },
      {
        key: "terminalBackgroundImageOpacity",
        subtitle: "Blend the background image toward the terminal background color.",
        title: "Background Image Opacity",
      },
      {
        key: "terminalBackgroundImageFit",
        options: [
          { label: "Cover", value: "cover" },
          { label: "Contain", value: "contain" },
          { label: "Stretch", value: "stretch" },
          { label: "Natural size", value: "natural" },
        ],
        subtitle: "How the background image is scaled inside each pane.",
        title: "Background Image Fit",
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
        key: "terminalPaneHorizontalPaddingPx",
        subtitle: "Add left and right inner padding inside every terminal pane.",
        title: "Horizontal Padding",
      },
      {
        key: "terminalPaneVerticalPaddingPx",
        subtitle: "Add top and bottom inner padding inside every terminal pane.",
        title: "Vertical Padding",
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
      {
        key: "clickToWakeSleepingSessions",
        subtitle: "Select sleeping pane tabs without waking them until the empty pane is clicked.",
        title: "Click to Wake Sleeping Panes",
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
        key: "showNotificationOnTerminalBell",
        subtitle: "Treat terminal bell events as session attention.",
        title: "Show notification on terminal bell",
      },
      {
        key: "promptEditorBackend",
        options: PROMPT_EDITOR_BACKEND_OPTIONS,
        subtitle:
          "Choose which editor Ctrl+G uses when a terminal prompt asks for $EDITOR.",
        title: "Ctrl+G prompt editor",
      },
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
    terminalDevServers: getSettingsSectionSearch(settingsSearchQuery, "Dev Servers", [
      {
        key: "terminalDevServerDetectionEnabled",
        subtitle: "Detect localhost dev server URLs from terminal output.",
        title: "Detect running servers in terminals",
      },
      {
        key: "terminalDevServerIgnoredPortRules",
        options: [
          { label: "9229", value: "9229" },
          { label: "24678-24680", value: "24678-24680" },
        ],
        subtitle: "Hide detected servers on specific ports or inclusive port ranges.",
        title: "Ignored ports",
      },
    ]),
    beta: getSettingsSectionSearch(settingsSearchQuery, "Experimental", [
      /*
       * CDXC:ExperimentalFeatures 2026-06-28-07:41:
       * Settings search should find the advanced experimental gate by label and
       * by the concrete surfaces it enables so the required inventory stays
       * discoverable without tying Agents Hub to this gate.
       */
      {
        key: "showBetaFeatures",
        subtitle:
          "Show experimental surfaces: OS Integration settings, Browser color scheme, and Keep Awake.",
        title: "Enable Experimental Features",
      },
    ]),
    debugging: getSettingsSectionSearch(settingsSearchQuery, "Debugging", [
      /*
       * CDXC:DiagnosticsSettings 2026-06-06-07:09:
       * Show debug UI controls is the global gate for routine diagnostic disk
       * logging as well as debug-only UI. Scenario controls narrow which
       * routine log area writes while the global gate is on; important
       * warnings, errors, and crashes remain available independently.
       *
       * CDXC:DebuggingSettings 2026-06-15-21:34:
       * The Debugging section owns support and diagnostic toggles at the bottom of Settings, including command copy actions and Copy details, so users can find debug-only context-menu features together.
       *
       * CDXC:DiagnosticsSettings 2026-06-27-22:07:
       * Disk logging needs exact scenario controls. Search should match both
       * the scenario labels and their support-bundle file names so a user can
       * enable only the requested repro log without browsing every Debugging row.
       */
      {
        key: "debuggingMode",
        subtitle: "Show debug-only UI controls and allow enabled routine diagnostic logs.",
        title: "Show debug UI controls",
      },
      {
        key: "diagnosticLogging",
        options: DIAGNOSTIC_LOGGING_SCENARIOS.flatMap((scenario) => [
          { label: scenario.label, value: scenario.id },
          ...scenario.logFiles.map((logFile) => ({ label: logFile, value: logFile })),
        ]),
        subtitle: "Choose routine repro log areas while Show debug UI controls is on. Important warnings, errors, and crashes remain captured when it is off.",
        title: "Diagnostic disk logging scenarios",
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
  };
  const mainSettingsGroupSearch = {
    appearance: getGroupedSettingsSectionSearch(settingsSearchQuery, "Appearance", [
      settingsSearch.theming,
      settingsSearch.appIcon,
    ]),
    chat: settingsSearch.chat,
    sidebar: getGroupedSettingsSectionSearch(settingsSearchQuery, "Sidebar", [
      settingsSearch.sidebar,
      settingsSearch.sessionCards,
      settingsSearch.sidebarTags,
    ]),
    terminal: getGroupedSettingsSectionSearch(settingsSearchQuery, "Terminal", [
      settingsSearch.terminal,
      settingsSearch.terminalBehavior,
      settingsSearch.terminalScrolling,
    ]),
    tools: getGroupedSettingsSectionSearch(settingsSearchQuery, "Tools", [
      settingsSearch.browser,
      settingsSearch.editor,
      settingsSearch.terminalDevServers,
    ]),
    statusIndicators: settingsSearch.statusIndicators,
    notifications: getGroupedSettingsSectionSearch(settingsSearchQuery, "Notifications", [
      settingsSearch.sounds,
    ]),
    system: getGroupedSettingsSectionSearch(settingsSearchQuery, "System", [
      settingsSearch.autoSleep,
      settingsSearch.power,
      settingsSearch.storage,
    ]),
    advanced: getGroupedSettingsSectionSearch(settingsSearchQuery, "Advanced", [
      settingsSearch.beta,
      settingsSearch.debugging,
    ]),
  };
  const mainSettingsSectionNavigation: Array<{
    id: MainSettingsSectionId;
    searchResult: SettingsSectionSearchResult;
    title: string;
  }> = [
    /*
     * Keep these destinations in the same order as their first rendered
     * section anchors below. The grouped pages intentionally collect related
     * subsections, but clicking down this rail should always move down the
     * Settings page instead of jumping above an earlier-looking destination.
     */
    { id: "sidebar", searchResult: mainSettingsGroupSearch.sidebar, title: "Sidebar" },
    {
      id: "appearance",
      searchResult: mainSettingsGroupSearch.appearance,
      title: "Appearance",
    },
    { id: "chat", searchResult: mainSettingsGroupSearch.chat, title: "Chat" },
    ...(PET_CONTROLS_VISIBLE
      ? [
          {
            id: "statusIndicators" as const,
            searchResult: mainSettingsGroupSearch.statusIndicators,
            title: "Status Indicators",
          },
        ]
      : []),
    {
      id: "tools",
      searchResult: mainSettingsGroupSearch.tools,
      title: "Tools",
    },
    /*
     * CDXC:SettingsNavigation 2026-06-12-04:13:
     * Ghostty terminal controls belong on the main Settings page so one search query can find app settings and terminal settings together.
     */
    { id: "terminal", searchResult: mainSettingsGroupSearch.terminal, title: "Terminal" },
    {
      id: "system",
      searchResult: mainSettingsGroupSearch.system,
      title: "System",
    },
    {
      id: "notifications",
      searchResult: mainSettingsGroupSearch.notifications,
      title: "Notifications",
    },
    { id: "advanced", searchResult: mainSettingsGroupSearch.advanced, title: "Advanced" },
  ];
  const settingMatchesGroupedSectionTitle = (settingKey: string) =>
    (Object.entries(MAIN_SETTINGS_SECTION_SETTING_KEYS) as Array<
      [MainSettingsSectionId, readonly string[]]
    >).some(([sectionId, settingKeys]) => {
      if (sectionId === "agents") {
        return false;
      }
      return (
        mainSettingsGroupSearch[sectionId].groupTitleMatches === true &&
        settingKeys.includes(settingKey)
      );
    });
  const subsectionMatchesGroupedSectionTitle = (sectionId: MainSettingsScrollTargetId) =>
    MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS[sectionId].some((settingKey) =>
      settingMatchesGroupedSectionTitle(settingKey),
    );
  const visibleFirstLaunchMainSettings =
    firstLaunchSetupVisibleSettings ?? FIRST_LAUNCH_SETUP_VISIBLE_MAIN_SETTINGS;
  const keepAwakeSettingsVisible = isFirstLaunchSetup || draft.showBetaFeatures;
  const debuggingModeDependentSettingsVisible = draft.debuggingMode;
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
    if (settingsSearchQuery.trim() && settingMatchesGroupedSectionTitle(settingKey)) {
      return true;
    }
    return shouldShowSetting(sectionResult, settingKey, showAdvancedSettings);
  };
  const debuggingSettingVisible = (settingKey: string) => {
    if (
      !debuggingModeDependentSettingsVisible &&
      DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET.has(settingKey)
    ) {
      return false;
    }
    return mainSettingVisible(settingsSearch.debugging, settingKey);
  };
  const mainSectionVisible = (
    sectionId: MainSettingsSectionId,
    sectionResult: SettingsSectionSearchResult,
  ) => {
    /*
     * CDXC:TitlebarKeepAwake 2026-06-19-13:13:
     * Keep Awake is experimental-only in the regular macOS Settings UI. Hide
     * the Power section until Enable Experimental Features is enabled, while
     * preserving the first-launch lid-close preference required by onboarding.
     */
    if (
      sectionId === "advanced" &&
      !isFirstLaunchSetup &&
      !debuggingModeDependentSettingsVisible
    ) {
      return (
        shouldShowSettingsSection(settingsSearch.beta, showAdvancedSettings) ||
        shouldShowSetting(settingsSearch.debugging, "debuggingMode", showAdvancedSettings)
      );
    }
    if (isFirstLaunchSetup) {
      return MAIN_SETTINGS_SECTION_SETTING_KEYS[sectionId].some((settingKey) =>
        isFirstLaunchSetupMainSettingVisible(
          settingKey as FirstLaunchSetupMainSettingKey,
          visibleFirstLaunchMainSettings,
        ),
      );
    }
    return shouldShowSettingsSection(sectionResult, showAdvancedSettings);
  };
  const mainSubsectionVisible = (
    sectionId: MainSettingsScrollTargetId,
    sectionResult: SettingsSectionSearchResult,
  ) => {
    if (sectionId === "power" && !keepAwakeSettingsVisible) {
      return false;
    }
    if (sectionId === "appIcon" && appIconPickerUnavailable) {
      return false;
    }
    if (
      sectionId === "debugging" &&
      !isFirstLaunchSetup &&
      !debuggingModeDependentSettingsVisible
    ) {
      return (
        subsectionMatchesGroupedSectionTitle(sectionId) ||
        shouldShowSetting(sectionResult, "debuggingMode", showAdvancedSettings)
      );
    }
    if (isFirstLaunchSetup) {
      return MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS[sectionId].some((settingKey) =>
        isFirstLaunchSetupMainSettingVisible(
          settingKey as FirstLaunchSetupMainSettingKey,
          visibleFirstLaunchMainSettings,
        ),
      );
    }
    if (settingsSearchQuery.trim() && subsectionMatchesGroupedSectionTitle(sectionId)) {
      return true;
    }
    return shouldShowSettingsSection(sectionResult, showAdvancedSettings);
  };
  const mainSettingsSectionRefs: MainSettingsSectionRefs = {
    agents: agentsOnboardingSectionRef,
    advanced: betaSectionRef,
    appearance: themingSectionRef,
    appIcon: appIconSectionRef,
    autoSleep: autoSleepSectionRef,
    beta: betaSectionRef,
    builtInFeatures: browserSectionRef,
    browser: browserSectionRef,
    chat: chatSectionRef,
    debugging: debuggingSectionRef,
    editor: editorSectionRef,
    notifications: soundsSectionRef,
    power: powerSectionRef,
    sessionCards: sessionCardsSectionRef,
    sidebar: sidebarSectionRef,
    sidebarTags: sidebarTagsSectionRef,
    sounds: soundsSectionRef,
    statusIndicators: statusIndicatorsSectionRef,
    storage: storageSectionRef,
    system: powerSectionRef,
    tools: browserSectionRef,
    terminal: ghosttyTerminalSectionRef,
    terminalBehavior: ghosttyBehaviorSectionRef,
    terminalDevServers: terminalDevServersSectionRef,
    terminalScrolling: ghosttyScrollingSectionRef,
    theming: themingSectionRef,
  };
  const scrollMainSettingsSectionIntoView = (sectionId: MainSettingsScrollTargetId) => {
    getMainSettingsSectionRef(sectionId, mainSettingsSectionRefs).current?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    });
  };
  const visibleMainSettingsSectionNavigation: Array<
    SettingsSectionNavigationItem<MainSettingsSectionId> & {
      searchResult: SettingsSectionSearchResult;
      subsections: readonly MainSettingsSubsectionNavigationItem[];
    }
  > =
    (isFirstLaunchSetup
      ? [
          {
            id: "agents" as const,
            searchResult: settingsSearch.sidebar,
            title: "Agents",
          },
          ...mainSettingsSectionNavigation,
        ]
      : mainSettingsSectionNavigation
    )
      .filter((section) =>
        section.id === "agents"
          ? mainSectionVisible("agents", settingsSearch.sidebar)
          : mainSectionVisible(section.id, section.searchResult),
      )
      .map((section) => ({
        ...section,
        /*
         * CDXC:SettingsNavigation 2026-08-19:
         * A nested row must not outlive the section it points at, so hide the
         * ones a search query, Show Advanced, or an unavailable capability
         * (Power without experimental features, App Icon off macOS) already
         * removed from the page.
         */
        subsections: (MAIN_SETTINGS_SUBSECTION_NAVIGATION[section.id] ?? []).filter((subsection) =>
          mainSubsectionVisible(subsection.id, settingsSearch[subsection.id]),
        ),
      }));
  const getMainSettingsSectionMeasurementItems = (): SettingsSectionMeasurementItem<MainSettingsScrollTargetId>[] =>
    visibleMainSettingsSectionNavigation.flatMap((section) =>
      (section.subsections.length > 0
        ? section.subsections.map((subsection) => subsection.id)
        : [section.id as MainSettingsScrollTargetId]
      ).map((scrollTargetId) => ({
        id: scrollTargetId,
        ref: getMainSettingsSectionRef(scrollTargetId, mainSettingsSectionRefs),
      })),
    );
  const activeMainSettingsGroupId = getMainSettingsSectionGroupId(activeMainSettingsSectionId);
  const hasVisibleMainSettings = visibleMainSettingsSectionNavigation.length > 0;
  const visibleMainSettingsSectionIds = visibleMainSettingsSectionNavigation
    .map((section) =>
      [section.id, ...section.subsections.map((subsection) => subsection.id)].join(">"),
    )
    .join("|");
  const hotkeyDefinitionsById = useMemo<HotkeySettingsDefinitionById>(
    () => new Map(GHOSTEX_HOTKEY_DEFINITIONS.map((definition) => [definition.id, definition])),
    [],
  );
  const hotkeySectionSearches = useMemo(() => {
    const sectionSearches = getHotkeySettingsSectionSearches({
      definitionsById: hotkeyDefinitionsById,
      expandCollapsedProjectsOnJump: draft.expandCollapsedProjectsOnJump,
      searchQuery: settingsSearchQuery,
    });
    /*
     * CDXC:SettingsSearch 2026-07-22-00:00:
     * A query matching the Hotkeys page title (e.g. "hotkeys") should reveal
     * the whole page, mirroring how section-title matches reveal their rows.
     */
    if (!getSettingsSectionSearch(settingsSearchQuery, "Hotkeys", []).sectionMatches) {
      return sectionSearches;
    }
    return Object.fromEntries(
      Object.entries(sectionSearches).map(([sectionId, sectionResult]) => [
        sectionId,
        { ...sectionResult, sectionMatches: true },
      ]),
    ) as HotkeySettingsSectionSearches;
  }, [draft.expandCollapsedProjectsOnJump, hotkeyDefinitionsById, settingsSearchQuery]);
  const extraSettingsTabSearches = useMemo(
    () => getExtraSettingsTabSearches(settingsSearchQuery),
    [settingsSearchQuery],
  );
  const isSettingsSearching = !isFirstLaunchSetup && settingsSearchQuery.trim().length > 0;
  const hotkeySectionRefs: HotkeySettingsSectionRefs = {
    actions: hotkeyActionsSectionRef,
    general: hotkeyGeneralSectionRef,
    navigation: hotkeyNavigationSectionRef,
    paneActions: hotkeyPaneActionsSectionRef,
    projects: hotkeyProjectsSectionRef,
    sessionSlots: hotkeySessionSlotsSectionRef,
  };
  const visibleHotkeySections = HOTKEY_SETTINGS_SECTIONS.filter((section) =>
    shouldShowSettingsSection(hotkeySectionSearches[section.id]),
  );
  const visibleHotkeySectionNavigation: SettingsSectionNavigationItem<HotkeySettingsSectionId>[] =
    visibleHotkeySections.map((section) => ({
      id: section.id,
      title: section.title,
    }));
  const scrollHotkeySettingsSectionIntoView = (sectionId: HotkeySettingsSectionId) => {
    hotkeySectionRefs[sectionId].current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  /*
   * CDXC:SettingsNavigation 2026-06-24-22:16:
   * Settings no longer has a top tab bar. Keep top-level Settings pages in the
   * left sidebar and let section-rich pages expand there so navigation, section
   * jumps, search results, and the Show Advanced footer share one rail.
   *
   * CDXC:SettingsNavigation 2026-06-25-17:12:
   * Top-level Settings categories need Tabler icons in the left sidebar, while
   * nested section rows stay text-only so expandable sections do not read as
   * separate main categories.
   */
  const settingsSidebarPageHasSearchMatches = (pageId: SettingsModalTab): boolean => {
    if (!isSettingsSearching) {
      return true;
    }
    if (pageId === "settings") {
      return (
        hasVisibleMainSettings ||
        getSettingsSectionSearch(settingsSearchQuery, "General", []).sectionMatches
      );
    }
    if (pageId === "hotkeys") {
      return visibleHotkeySections.length > 0;
    }
    return settingsTabSearchHasMatches(
      extraSettingsTabSearches[pageId as SearchableExtraSettingsTabId],
    );
  };
  /*
   * CDXC:SettingsSearch 2026-07-22-00:00:
   * While searching, the sidebar rail keeps only the Settings pages that have
   * matches so one query locates settings across every page, not just the
   * page currently open.
   */
  const allSettingsSidebarPages: SettingsSidebarPage[] = [
    {
      icon: IconSettings,
      id: "settings",
      sections: visibleMainSettingsSectionNavigation.map((section) => ({
        active: activeTab === "settings" && activeMainSettingsGroupId === section.id,
        id: section.id,
        onSelect: () => {
          setActiveMainSettingsSectionId(section.id);
          setActiveTab("settings");
          requestAnimationFrame(() => scrollMainSettingsSectionIntoView(section.id));
        },
        /*
         * CDXC:SettingsNavigation 2026-08-19:
         * A group whose first anchor carries the group's own name (Sidebar,
         * Terminal) would otherwise render "Sidebar > Sidebar". Drop that row
         * from the rail only: scroll tracking still measures the anchor, so
         * reading that header keeps the group row highlighted.
         */
        subsections: section.subsections
          .filter((subsection) => subsection.title !== section.title)
          .map((subsection) => ({
            active: activeTab === "settings" && activeMainSettingsSectionId === subsection.id,
            id: subsection.id,
            onSelect: () => {
              setActiveMainSettingsSectionId(subsection.id);
              setActiveTab("settings");
              requestAnimationFrame(() => scrollMainSettingsSectionIntoView(subsection.id));
            },
            title: subsection.title,
          })),
        title: section.title,
      })),
      title: "General",
    },
    { icon: IconCodeDots, id: "agents", title: "Agents" },
    { icon: IconTools, id: "integrations", title: "Integrations" },
    { icon: IconCashEdit, id: "plugins", title: "Customize" },
    { icon: IconCloud, id: "remote", title: "Remote" },
    { icon: IconFolderOpen, id: "projects", title: "Projects" },
    {
      icon: IconKeyboard,
      id: "hotkeys",
      sections: visibleHotkeySectionNavigation.map((section) => ({
        active: activeTab === "hotkeys" && activeHotkeySettingsSectionId === section.id,
        id: section.id,
        onSelect: () => {
          setActiveHotkeySettingsSectionId(section.id);
          setActiveTab("hotkeys");
          requestAnimationFrame(() => scrollHotkeySettingsSectionIntoView(section.id));
        },
        title: section.title,
      })),
      title: "Hotkeys",
    },
    { icon: IconPlayerPlay, id: "actions", title: "Actions" },
    { icon: IconExternalLink, id: "openTargets", title: "Open In" },
    ...(showOSIntegrationSettingsTab
      ? [{ icon: IconDeviceDesktop, id: "osIntegration" as const, title: "OS Integration" }]
      : []),
    { icon: IconInfoCircle, id: "about", title: "About" },
  ];
  const settingsSidebarPages: SettingsSidebarPage[] = allSettingsSidebarPages.filter((page) =>
    settingsSidebarPageHasSearchMatches(page.id),
  );
  const settingsSearchMatchingPages = isSettingsSearching ? settingsSidebarPages : [];

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
      advanced: betaSectionRef,
      appearance: themingSectionRef,
      autoSleep: autoSleepSectionRef,
      builtInFeatures: browserSectionRef,
      browser: browserSectionRef,
      chat: chatSectionRef,
      editor: editorSectionRef,
      notifications: soundsSectionRef,
      power: powerSectionRef,
      sessionCards: sessionCardsSectionRef,
      sidebar: sidebarSectionRef,
      sounds: soundsSectionRef,
      beta: betaSectionRef,
      statusIndicators: statusIndicatorsSectionRef,
      storage: storageSectionRef,
      system: powerSectionRef,
      sidebarTags: sidebarTagsSectionRef,
      debugging: debuggingSectionRef,
      tools: browserSectionRef,
      terminal: ghosttyTerminalSectionRef,
      terminalBehavior: ghosttyBehaviorSectionRef,
      terminalScrolling: ghosttyScrollingSectionRef,
      terminalDevServers: terminalDevServersSectionRef,
      theming: themingSectionRef,
      // CDXC:AppIconPicker 2026-06-25-21:50: Allow titlebar/deep-link navigation to scroll to App Icon.
      appIcon: appIconSectionRef,
      agents: agentsOnboardingSectionRef,
    });
    const animationFrame = requestAnimationFrame(() => {
      targetSectionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, initialSection, isOpen]);

  useEffect(() => {
    if (!isOpen || activeTab !== "settings") {
      return;
    }

    const animationFrame = requestAnimationFrame(() => {
      const viewport = getActiveSettingsModalScrollViewport(dialogContentRef.current);
      if (!viewport) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        viewport,
        getMainSettingsSectionMeasurementItems(),
      );
      if (mostlyVisibleSectionId) {
        setActiveMainSettingsSectionId((currentSectionId) =>
          currentSectionId === mostlyVisibleSectionId ? currentSectionId : mostlyVisibleSectionId,
        );
      }
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, isOpen, settingsSearchQuery, visibleMainSettingsSectionIds]);
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

  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * Request the current icon list once whenever the App Icon settings surface
   * opens, mirroring the lazy native-data requests used elsewhere in Settings.
   * Native answers through the appIconState prop (relayed via the modal host).
   */
  useEffect(() => {
    if (!isOpen || activeTab !== "settings" || !vscode || appIconPickerUnavailable) {
      hasRequestedAppIconsRef.current = false;
      return;
    }
    if (hasRequestedAppIconsRef.current) {
      return;
    }
    hasRequestedAppIconsRef.current = true;
    vscode.postMessage({ type: "listAppIcons" });
  }, [activeTab, appIconPickerUnavailable, isOpen, vscode]);

  useEffect(() => {
    return () => {
      if (pendingTimeoutRef.current) {
        clearTimeout(pendingTimeoutRef.current);
      }
      if (pendingNavigationPersistTimeoutRef.current) {
        clearTimeout(pendingNavigationPersistTimeoutRef.current);
      }
    };
  }, []);

  const clearPendingSettings = () => {
    if (pendingTimeoutRef.current) {
      clearTimeout(pendingTimeoutRef.current);
      pendingTimeoutRef.current = undefined;
    }
  };

  const clearPendingNavigationPersist = () => {
    if (pendingNavigationPersistTimeoutRef.current) {
      clearTimeout(pendingNavigationPersistTimeoutRef.current);
      pendingNavigationPersistTimeoutRef.current = undefined;
    }
  };

  const postSettingsPatch = (
    patch: ghostexSettingsPatch,
    source: ghostexSettingsUpdateSource,
    fallbackSettings: ghostexSettings,
  ) => {
    if (Object.keys(patch).length === 0) {
      return;
    }
    if (onPatch) {
      onPatch(patch, source);
      return;
    }
    onChange(fallbackSettings, source);
  };

  const persistSettingsModalNavigation = (navigationActiveTab: SettingsModalTab = activeTab) => {
    rememberActiveScrollPosition();
    const pendingSettings = pendingSettingsRef.current;
    const pendingPatch = pendingSettingsPatchRef.current;
    const baseSettings = pendingSettings ?? draft;
    const nextSettings = isFirstLaunchSetup
      ? baseSettings
      : normalizeghostexSettings({
          ...baseSettings,
          settingsModalNavigation: getRememberedSettingsModalNavigationState(
            navigationActiveTab,
            baseSettings.settingsModalNavigation,
          ),
        });
    const shouldPersistNavigation =
      !isFirstLaunchSetup &&
      !areSettingsModalNavigationStatesEqual(
        baseSettings.settingsModalNavigation,
        nextSettings.settingsModalNavigation,
      );
    /*
     * CDXC:SettingsNavigation 2026-06-30-04:47:
     * Native Settings is an AppKit child window, so closing it with native
     * chrome can bypass the React Dialog close callback. Persist page changes
     * immediately and scroll changes after they settle; close remains a final
     * flush for pending numeric edits and any unsaved navigation state.
     *
     * CDXC:RemoteMachines 2026-06-30-15:18:
     * Navigation persistence is a patch-only write. Opening or scrolling Settings
     * must never post a full draft that could overwrite unrelated domains such as
     * remoteMachines from stale modal state.
     */
    clearPendingNavigationPersist();
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    pendingSettingsPatchRef.current = undefined;
    if (pendingSettings || shouldPersistNavigation) {
      setDraft(nextSettings);
      postSettingsPatch(
        {
          ...(pendingPatch ?? {}),
          ...(shouldPersistNavigation
            ? { settingsModalNavigation: nextSettings.settingsModalNavigation }
            : {}),
        },
        pendingPatch ? "settings:control" : "settings:navigation",
        nextSettings,
      );
    }
  };

  const scheduleSettingsModalNavigationPersist = (
    navigationActiveTab: SettingsModalTab = activeTab,
  ) => {
    if (isFirstLaunchSetup) {
      return;
    }
    clearPendingNavigationPersist();
    pendingNavigationPersistTimeoutRef.current = setTimeout(() => {
      pendingNavigationPersistTimeoutRef.current = undefined;
      persistSettingsModalNavigation(navigationActiveTab);
    }, SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS);
  };

  const closeSettingsModal = () => {
    persistSettingsModalNavigation(activeTab);
    onClose();
  };

  const applySettings = (
    nextSettings: ghostexSettings,
    source: ghostexSettingsUpdateSource = "settings:bulk",
  ) => {
    const normalizedSettings = normalizeghostexSettings(nextSettings);
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    pendingSettingsPatchRef.current = undefined;
    setDraft(normalizedSettings);
    onChange(normalizedSettings, source);
  };

  const applySettingsPatch = (
    patch: ghostexSettingsPatch,
    source: ghostexSettingsUpdateSource = "settings:control",
  ) => {
    const normalizedSettings = normalizeghostexSettings({
      ...(pendingSettingsRef.current ?? draft),
      ...patch,
    });
    const normalizedPatch = createNormalizedSettingsPatch(normalizedSettings, patch);
    clearPendingSettings();
    pendingSettingsRef.current = undefined;
    pendingSettingsPatchRef.current = undefined;
    setDraft(normalizedSettings);
    postSettingsPatch(normalizedPatch, source, normalizedSettings);
  };

  /**
   * CDXC:Settings 2026-04-26-11:13: Numeric settings use sliders with adjacent
   * number boxes. Dragging or typing updates the visible value immediately, but
   * persists through a short trailing debounce to avoid flooding settings writes.
   * Number boxes keep local edit text so partial values can be typed cleanly.
   */
  const applySettingsPatchDebounced = (
    patch: ghostexSettingsPatch,
    source: ghostexSettingsUpdateSource = "settings:control",
  ) => {
    const normalizedSettings = normalizeghostexSettings({
      ...(pendingSettingsRef.current ?? draft),
      ...patch,
    });
    const normalizedPatch = createNormalizedSettingsPatch(normalizedSettings, patch);
    pendingSettingsRef.current = normalizedSettings;
    pendingSettingsPatchRef.current = {
      ...(pendingSettingsPatchRef.current ?? {}),
      ...normalizedPatch,
    };
    setDraft(normalizedSettings);
    clearPendingSettings();
    pendingTimeoutRef.current = setTimeout(() => {
      const pendingSettings = pendingSettingsRef.current;
      const pendingPatch = pendingSettingsPatchRef.current;
      pendingSettingsRef.current = undefined;
      pendingSettingsPatchRef.current = undefined;
      pendingTimeoutRef.current = undefined;
      if (pendingSettings) {
        postSettingsPatch(pendingPatch ?? {}, source, pendingSettings);
      }
    }, NUMERIC_SETTINGS_DEBOUNCE_MS);
  };

  /**
   * CDXC:Settings 2026-04-26-10:12: Settings changes must apply immediately.
   * The settings dialog keeps local state only for responsive controls, then
   * posts every normalized change instead of waiting for Save/Cancel actions.
   */
  const updateDraft = <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => {
    applySettingsPatch({ [key]: value } as Pick<ghostexSettings, Key>);
  };
  const updateShowAdvancedSettings = (checked: boolean) => {
    /*
     * CDXC:SettingsAdvanced 2026-06-28-08:01:
     * Show Advanced is settings chrome, but it still needs immediate durable
     * persistence so restart hydration reopens Settings with the same advanced
     * row visibility the user explicitly chose.
     */
    applySettingsPatch({ showAdvancedSettings: checked });
  };
  const updateDiagnosticLoggingScenario = (
    scenarioId: DiagnosticLoggingScenarioId,
    duration: DiagnosticLoggingDurationValue,
  ) => {
    updateDraft(
      "diagnosticLogging",
      setDiagnosticLoggingScenario(
        (pendingSettingsRef.current ?? draft).diagnosticLogging,
        scenarioId,
        getDiagnosticLoggingScenarioStateForDuration(duration),
      ),
    );
  };
  const updateDraftDebounced = <Key extends keyof ghostexSettings>(
    key: Key,
    value: ghostexSettings[Key],
  ) => {
    applySettingsPatchDebounced({ [key]: value } as Pick<ghostexSettings, Key>);
  };
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * Confirm-before-persist is prop-driven: native relays appIconState into this
   * component through the modal-state plumbing (exactly like osIntegrationStatus),
   * so react to each new prop value. On an ok state, persist the in-flight
   * pending selection (falling back to native's selectedId) and clear any error;
   * on a failed state, drop the pending id and surface the error without writing
   * appIconSourceId.
   *
   * CDXC:SettingsPerformance 2026-06-29-00:40:
   * Process each native appIconState once inside this effect instead of updating
   * a closure ref during render, because SettingsModal needs React Compiler
   * coverage to reduce large settings-page rerenders during scroll navigation.
   */
  useEffect(() => {
    if (!appIconState) {
      return;
    }
    if (handledAppIconStateRef.current === appIconState) {
      return;
    }
    handledAppIconStateRef.current = appIconState;
    if (appIconState.ok) {
      setAppIconError(undefined);
      const pendingSourceId = pendingAppIconSourceIdRef.current;
      const confirmedSourceId =
        pendingSourceId !== undefined ? pendingSourceId : appIconState.selectedId;
      pendingAppIconSourceIdRef.current = undefined;
      const currentSettings = pendingSettingsRef.current ?? draft;
      if (currentSettings.appIconSourceId !== confirmedSourceId) {
        updateDraft("appIconSourceId", confirmedSourceId);
      }
      return;
    }
    pendingAppIconSourceIdRef.current = undefined;
    setAppIconError(
      typeof appIconState.error === "string" && appIconState.error.trim()
        ? appIconState.error.trim()
        : "Could not update the app icon.",
    );
  }, [appIconState, draft]);
  /**
   * CDXC:AppIconPicker 2026-06-25-21:50:
   * Selecting, choosing a file, revealing the folder, and resetting all post the
   * exact wire-contract messages to native. The selection messages record the
   * pending source id and clear any prior error; the sidebar persists nothing
   * until the matching ok: true appIconState arrives.
   */
  const selectAppIcon = (sourceId: string) => {
    if (!vscode) {
      return;
    }
    pendingAppIconSourceIdRef.current = sourceId;
    setAppIconError(undefined);
    vscode.postMessage({ type: "setAppIcon", sourceId });
  };
  const chooseAppIconFile = () => {
    if (!vscode) {
      return;
    }
    setAppIconError(undefined);
    vscode.postMessage({ type: "pickAppIconFile" });
  };
  /**
   * CDXC:TerminalBackgroundImage 2026-08-01:
   * The Browse button next to Settings -> Terminal -> Background Image opens a
   * native file dialog host-side; the picked absolute path comes back as a
   * terminalBackgroundImageFilePicked host message and lands in the draft like
   * a typed path. Native pickers only exist in the desktop app, so web hosts
   * (which set appIconPickerUnavailable) render the plain text field instead.
   */
  const nativeFilePickerAvailable = Boolean(vscode) && !appIconPickerUnavailable;
  const chooseTerminalBackgroundImageFile = () => {
    if (!vscode) {
      return;
    }
    vscode.postMessage({ type: "pickTerminalBackgroundImageFile" });
  };
  useEffect(() => {
    if (!isOpen || !nativeFilePickerAvailable) {
      return;
    }
    const handlePickedBackgroundImage = (event: Event) => {
      const message = (event as CustomEvent<unknown>).detail;
      if (
        !message ||
        typeof message !== "object" ||
        !("type" in message) ||
        message.type !== "terminalBackgroundImageFilePicked"
      ) {
        return;
      }
      const path = "path" in message && typeof message.path === "string" ? message.path.trim() : "";
      if (!path) {
        return;
      }
      updateDraft("terminalBackgroundImage", path);
    };
    window.addEventListener("ghostex-app-modal-host-message", handlePickedBackgroundImage);
    return () => {
      window.removeEventListener("ghostex-app-modal-host-message", handlePickedBackgroundImage);
    };
  }, [isOpen, nativeFilePickerAvailable]);
  const activeSidebarSettingsPresetId = getSidebarSettingsPresetId(draft);
  const updateSidebarSettingsPreset = (presetId: SidebarSettingsPresetId) => {
    applySettings(applySidebarSettingsPreset(pendingSettingsRef.current ?? draft, presetId));
  };

  const resetSettings = () => {
    /*
     * CDXC:AppIconPicker 2026-06-26-23:42:
     * Reset to defaults must update the runtime Dock/app-switcher icon as well
     * as persisted settings. Post the default source id to native before writing
     * defaults so the current app session does not keep showing a stale custom
     * icon until restart.
     */
    pendingAppIconSourceIdRef.current = "";
    setAppIconError(undefined);
    vscode?.postMessage({ type: "setAppIcon", sourceId: "" });
    applySettings({
      ...DEFAULT_ghostex_SETTINGS,
      remoteMachines: (pendingSettingsRef.current ?? draft).remoteMachines,
    });
  };
  const resetSetting = <Key extends keyof ghostexSettings>(key: Key) => {
    applySettingsPatch({ [key]: DEFAULT_ghostex_SETTINGS[key] } as Pick<ghostexSettings, Key>);
  };
  const getSettingModificationProps = <Key extends keyof ghostexSettings>(
    key: Key,
  ): Required<SettingModificationProps> => ({
    advanced: isAdvancedMainSetting(String(key)),
    isModified: !Object.is(draft[key], DEFAULT_ghostex_SETTINGS[key]),
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

  const settingsSearchEmptyState = isSettingsSearching ? (
    <SettingsSearchNoMatchesNotice
      activeTab={activeTab}
      matchingPages={settingsSearchMatchingPages}
      onSelectPage={setActiveTab}
    />
  ) : null;

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          closeSettingsModal();
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
          if (!isFirstLaunchSetup) {
            event.preventDefault();
            requestAnimationFrame(focusSearchInput);
          }
        }}
        onScrollCapture={handleSettingsModalScrollCapture}
        ref={dialogContentRef}
        showCloseButton={false}
      >
        <TooltipProvider delayDuration={300}>
          <Tabs
            className="flex min-h-0 flex-1 flex-col"
            onValueChange={(value) => setActiveTab(value as SettingsModalTab)}
            orientation="vertical"
            value={activeTab}
          >
          <DialogHeader className="ghostex-modal-heading-bar">
            {/*
             * CDXC:SettingsWindow 2026-06-25-17:05:
             * Native Settings windows already show "Ghostex Settings" in the
             * AppKit titlebar. Do not duplicate a visible "Settings" heading in
             * React; keep a hidden DialogTitle so the dialog remains named for
             * accessibility while first-launch setup keeps its visible title.
             */}
            <div className={cn("settings-modal-title-row", !isFirstLaunchSetup && "sr-only")}>
              <DialogTitle className="ghostex-modal-heading-title">
                {isFirstLaunchSetup ? "Get started" : "Ghostex Settings"}
              </DialogTitle>
            </div>
            {isFirstLaunchSetup ? (
              <p className="mt-2 text-sm text-muted-foreground">
                Choose a few defaults for Ghostex. You can change everything later in Settings.
              </p>
            ) : null}
          </DialogHeader>

          <div
            className={cn(
              "settings-modal-body-layout",
              isFirstLaunchSetup && "settings-modal-body-layout-first-launch",
            )}
          >
            {!isFirstLaunchSetup ? (
              <SettingsSidebarNavigation
                expandedPages={expandedSettingsSidebarPages}
                pages={settingsSidebarPages}
                showAdvancedSettings={showAdvancedSettings}
                showAdvancedSettingsId={showAdvancedSettingsId}
                onShowAdvancedSettingsChange={updateShowAdvancedSettings}
                onTogglePage={toggleSettingsSidebarPage}
              />
            ) : null}
            <div className="settings-modal-main-column">
              {/*
               * CDXC:UnifiedSettings 2026-05-09-15:30
               * Settings is the single configuration surface for app controls,
               * terminal controls, Agents, Actions, Open In, and Hotkeys.
               *
               * CDXC:SettingsNavigation 2026-06-12-04:13:
               * Ghostty terminal settings are merged into the main Settings page
               * so one Settings search covers app settings and terminal settings.
               *
               * CDXC:SettingsNavigation 2026-06-15-03:06:
               * OS Integration should be the final Settings tab because default
               * app-handler actions are less frequently used than daily app,
               * integration, remote, project, hotkey, agent, action, and Open In
               * controls.
               *
               * CDXC:SettingsNavigation 2026-06-15-20:48:
               * The first navigation label should read General so the modal
               * title can own the Settings name while the page label describes
               * its general app and terminal preference content.
               *
               * CDXC:SettingsNavigation 2026-06-24-22:16:
               * Top-level Settings tabs belong in the left sidebar, while one
               * global search field stays at the top of the content column.
               */}
              {!isFirstLaunchSetup ? (
                <div className="settings-modal-search-row">
                  <SidebarSessionSearchField
                    ariaLabel="Search settings"
                    autoCapitalize="none"
                    autoComplete="off"
                    autoCorrect="off"
                    clearLabel="Clear settings search"
                    inputClassName="settings-modal-search-input"
                    inputRef={searchInputRef}
                    placeholder="Search settings"
                    query={settingsSearchQuery}
                    setQuery={setSettingsSearchQuery}
                    shouldFocusOnQueryChange={shouldFocusSettingsSearchInput}
                    spellCheck={false}
                    toolbarClassName="settings-modal-search-toolbar"
                  />
                </div>
              ) : null}

          <TabsContent className="settings-main-tabs-content mt-0 min-h-0 flex-1 overflow-hidden" value="settings">
          {/* CDXC:Settings 2026-04-26-10:43: The settings dialog lives inside a
              narrow sidebar webview, so the Radix scroll area needs an explicit
              height instead of letting Dialog crop an auto-height viewport. */}
          {/* CDXC:UnifiedSettings 2026-05-09-17:08: The Settings dialog is now a
              tabbed surface with variable header height. The active tab owns
              the remaining vertical space so the dialog never clips the bottom
              of a fixed-height scroll area. */}
          {/* CDXC:SettingsNavigation 2026-05-13-08:05:
              Superseded by CDXC:SettingsNavigation 2026-06-24-22:16.

              CDXC:SettingsNavigation 2026-06-12-04:13:
              Terminal sections share this navigator with app settings so search
              and section jumps operate on one main Settings page.

              CDXC:SettingsNavigation 2026-06-24-22:16:
              General section jumps now come from the shared Settings sidebar
              outside this tab panel, while this panel owns only scrollable
              General settings content. */}
          <div className="settings-main-tab-layout">
            <SettingsNativeScrollArea className="settings-main-scroll h-full min-h-0">
              <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
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
            {mainSubsectionVisible("sidebar", settingsSearch.sidebar) ? (
              <SettingsSection sectionRef={sidebarSectionRef} title="Sidebar">
              {/*
               * CDXC:SidebarV2 2026-07-29:
               * Sidebar version stays near the top of the General tab so the
               * opt-in Inbox sidebar is discoverable without scrolling. Its
               * Group by Project sub-mode only appears while V2 is selected,
               * because the classic sidebar has no such layout.
               */}
              {mainSettingVisible(settingsSearch.sidebar, "sidebarVersion") ? (
              <SidebarVersionField
                badge="New"
                description="Classic keeps today's project-grouped sidebar. Inbox (V2) is a flat, position-stable list of sessions across all projects."
                label="Sidebar version"
                {...getSettingModificationProps("sidebarVersion")}
                onChange={(sidebarVersion) => updateDraft("sidebarVersion", sidebarVersion)}
                value={draft.sidebarVersion}
              />
              ) : null}
              {draft.sidebarVersion === "v2" &&
              mainSettingVisible(settingsSearch.sidebar, "sidebarV2Layout") ? (
              <ToggleField
                checked={draft.sidebarV2Layout === "byProject"}
                description="Group Inbox sidebar sessions into collapsible project groups instead of one flat list."
                label="Group by project"
                subtitle="Inbox sidebar only."
                {...getSettingModificationProps("sidebarV2Layout")}
                onChange={(checked) =>
                  updateDraft("sidebarV2Layout", checked ? "byProject" : "flat")
                }
              />
              ) : null}
              {/*
               * CDXC:SidebarV2Lifecycle 2026-07-29:
               * Auto-settle is nested with the other Inbox-only controls: it
               * changes nothing in the classic sidebar, and showing it there
               * would advertise a shelf that does not exist. gxserver reads the
               * same key from the shared settings file for its server-side
               * sweep, so this one control drives both ends.
               */}
              {draft.sidebarVersion === "v2" &&
              mainSettingVisible(settingsSearch.sidebar, "sidebarAutoSettleAfterDays") ? (
              <SelectField
                description="Move sessions with no meaningful activity to the Settled shelf. Working and blocked sessions never settle automatically."
                label="Auto-settle inactive sessions"
                {...getSettingModificationProps("sidebarAutoSettleAfterDays")}
                onChange={(value) =>
                  updateDraft(
                    "sidebarAutoSettleAfterDays",
                    parseSidebarAutoSettleAfterDaysSelectValue(value),
                  )
                }
                options={SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS}
                value={sidebarAutoSettleAfterDaysSelectValue(draft.sidebarAutoSettleAfterDays)}
              />
              ) : null}
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
              {mainSettingVisible(settingsSearch.sidebar, "sidebarProjectGroupStyle") ? (
              <SidebarProjectGroupStyleField
                description="Choose how project groups are marked without adding group or project card borders."
                label="Project group style"
                {...getSettingModificationProps("sidebarProjectGroupStyle")}
                onChange={(value) => updateDraft("sidebarProjectGroupStyle", value)}
                value={draft.sidebarProjectGroupStyle}
              />
              ) : null}
              {/*
               * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
               * Users need every preset-mutated setting directly under the preset selector so applying Recommended, Codex, Minimal, or Detailed has an inspectable effect without hunting through Session Cards, Project rows, or Status Indicators.
               */}
              {mainSettingVisible(settingsSearch.sidebar, "showProjectIcons") ? (
              <ToggleField
                checked={draft.showProjectIcons}
                description="Show project artwork or a folder or worktree icon beside project names."
                label="Show project icons"
                {...getSettingModificationProps("showProjectIcons")}
                onChange={(checked) => updateDraft("showProjectIcons", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "hideSessionAgentIconUntilHover") ? (
              <ToggleField
                checked={draft.hideSessionAgentIconUntilHover}
                description="Hide session agent icons until a session row is hovered."
                label="Hide agent icon until hover"
                {...getSettingModificationProps("hideSessionAgentIconUntilHover")}
                onChange={(checked) => updateDraft("hideSessionAgentIconUntilHover", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "hideBrowserFaviconUntilHover") ? (
              <ToggleField
                checked={draft.hideBrowserFaviconUntilHover}
                description="Hide browser page favicons until a session row is hovered."
                label="Hide browser favicon until hover"
                {...getSettingModificationProps("hideBrowserFaviconUntilHover")}
                onChange={(checked) => updateDraft("hideBrowserFaviconUntilHover", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "showCloseButtonOnSessionCards") ? (
              <ToggleField
                checked={draft.showCloseButtonOnSessionCards}
                description="Reveal the close control when hovering a card."
                label="Show close button on hover"
                {...getSettingModificationProps("showCloseButtonOnSessionCards")}
                onChange={(checked) => updateDraft("showCloseButtonOnSessionCards", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "hideLastActiveTimeOnSessionCards") ? (
              <ToggleField
                checked={draft.hideLastActiveTimeOnSessionCards}
                description="Hide Last Active timestamps from session-card title rows."
                label="Hide last active time"
                {...getSettingModificationProps("hideLastActiveTimeOnSessionCards")}
                onChange={(checked) => updateDraft("hideLastActiveTimeOnSessionCards", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "hideProjectHeaderDiffStats") ? (
              <ToggleField
                checked={draft.hideProjectHeaderDiffStats}
                description="Hide +added/-removed line counts in sidebar project rows."
                label="Hide project git stats"
                {...getSettingModificationProps("hideProjectHeaderDiffStats")}
                onChange={(checked) => updateDraft("hideProjectHeaderDiffStats", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "showProjectEditorDiffFileCount") ? (
              <ToggleField
                checked={draft.showProjectEditorDiffFileCount}
                description="Show changed-file counts in sidebar project row git stats."
                label="Show changed-file count"
                {...getSettingModificationProps("showProjectEditorDiffFileCount")}
                onChange={(checked) => updateDraft("showProjectEditorDiffFileCount", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "hideMenuBarSessionStatusIndicators") ? (
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
              {mainSettingVisible(settingsSearch.sidebar, "sidebarCollapseAnimationDurationMs") ? (
              <SliderNumberField
                description="Duration in milliseconds for expanding and collapsing sidebar sections, groups, and projects. Set to 0 for instant changes."
                label="Collapse Animation Duration"
                {...getSettingModificationProps("sidebarCollapseAnimationDurationMs")}
                max={MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS}
                min={MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS}
                onCommit={(value) => updateDraft("sidebarCollapseAnimationDurationMs", value)}
                onChange={(value) =>
                  updateDraftDebounced("sidebarCollapseAnimationDurationMs", value)
                }
                step={SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS}
                value={draft.sidebarCollapseAnimationDurationMs}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.sidebar, "commandsPanelDefaultHeightPx") ? (
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
              {mainSettingVisible(settingsSearch.sidebar, "commandsPanelSide") ? (
              <SelectField
                description="Where terminal Actions and F12 open the command pane: below the workspace or as a column to its right."
                label="Command Pane Side"
                {...getSettingModificationProps("commandsPanelSide")}
                onChange={(value) => updateDraft("commandsPanelSide", value as CommandsPanelSide)}
                options={COMMANDS_PANEL_SIDE_OPTIONS}
                value={draft.commandsPanelSide}
              />
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
              {mainSettingVisible(settingsSearch.sidebar, "agentManagerZoomPercent") ? (
              /*
               * CDXC:SidebarInterface 2026-06-16-18:19:
               * Keep the persisted agentManagerZoomPercent key for compatibility, but label the Settings control as Sidebar Interface Size because it changes the visible sidebar interface scale.
               */
              <SliderNumberField
                description="Scale the sidebar interface."
                label="Sidebar Interface Size"
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
              /*
               * CDXC:SidebarSessions 2026-06-16-09:20:
               * Creating sessions by double-clicking empty sidebar space is a low-frequency interaction preference, so hide it behind Show Advanced while keeping rename-on-card-double-click as a normal sidebar behavior setting.
               */
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
                label={RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL}
                subtitle={RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE}
                {...getSettingModificationProps("renameSessionOnDoubleClick")}
                onChange={(checked) => updateDraft("renameSessionOnDoubleClick", checked)}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSubsectionVisible("theming", settingsSearch.theming) ? (
              <SettingsSection sectionRef={themingSectionRef} title="Theming">
                {/*
                  CDXC:SettingsTheming 2026-06-15-21:35:
                  General settings needs Theming as the second section, separate
                  from Sidebar layout controls.

                  CDXC:SettingsTheming 2026-06-16-01:35:
                  Theming remains a distinct section on the General settings
                  page so theme-related controls scan separately from Sidebar
                  layout controls.

                  CDXC:SettingsTheming 2026-06-16-08:58:
                  Theme selection is not ready for the Settings UI. Hide the
                  dropdown control and show a simple "Light theme coming soon"
                  message while keeping all Theming rows visible without Show
                  Advanced.

                  CDXC:SidebarTitlebarColors 2026-06-15-13:22:
                  Users should only pick the sidebar/titlebar background. The
                  foreground is derived automatically from that background so
                  light and dark custom colors keep readable chrome.

                  CDXC:SidebarTitlebarColors 2026-06-15-13:45:
                  Replace the freeform background color picker with a constrained
                  contrast slider. The slider outputs calibrated dark
                  backgrounds so sidebar row states remain predictable.

                  CDXC:SidebarTitlebarColors 2026-06-15-15:01:
                  Limit the contrast slider to 85-100 because lower values made
                  custom sidebar chrome too gray.

                  CDXC:SidebarTitlebarColors 2026-06-15-15:15:
                  Call the user-facing control Contrast while keeping the stored
                  background darkness key stable for existing settings and native
                  startup compatibility.

                  CDXC:SidebarTitlebarColors 2026-06-15-15:28:
                  Add Background Tint as a web-only color picker. Do not use
                  input[type=color], because macOS replaces that with a native
                  color panel instead of the in-app picker requested here.
                */}
                {mainSettingVisible(settingsSearch.theming, "sidebarTheme") ? (
                  <StaticNoteField
                    label="Theme"
                    surface="plain"
                    value="Light theme coming soon"
                  />
                ) : null}
                {mainSettingVisible(
                  settingsSearch.theming,
                  "customSidebarTitlebarBackgroundDarknessPercent",
                ) ? (
                  <SliderNumberField
                    description="85 is softer gray; 100 is black. Text and icons adjust automatically."
                    label="Background Contrast"
                    {...getSettingModificationProps("customSidebarTitlebarBackgroundDarknessPercent")}
                    max={MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT}
                    min={MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT}
                    onCommit={(value) =>
                      updateDraft("customSidebarTitlebarBackgroundDarknessPercent", value)
                    }
                    onChange={(value) =>
                      updateDraftDebounced("customSidebarTitlebarBackgroundDarknessPercent", value)
                    }
                    step={1}
                    value={draft.customSidebarTitlebarBackgroundDarknessPercent}
                  />
                ) : null}
                {mainSettingVisible(settingsSearch.theming, "customSidebarTitlebarBackgroundTintColor") ? (
                  <WebColorPickerField
                    description="Applies a subtle hue to the sidebar and titlebar background."
                    label="Background Tint"
                    {...getSettingModificationProps("customSidebarTitlebarBackgroundTintColor")}
                    onChange={(value) =>
                      updateDraftDebounced("customSidebarTitlebarBackgroundTintColor", value)
                    }
                    onCommit={(value) => updateDraft("customSidebarTitlebarBackgroundTintColor", value)}
                    value={draft.customSidebarTitlebarBackgroundTintColor}
                  />
                ) : null}
                {mainSettingVisible(settingsSearch.theming, "workspaceActivePaneBorderColor") ? (
                  <TextField
                    description="CSS color for the focused pane border."
                    label="Active Pane Border"
                    {...getSettingModificationProps("workspaceActivePaneBorderColor")}
                    onChange={(value) => updateDraft("workspaceActivePaneBorderColor", value)}
                    value={draft.workspaceActivePaneBorderColor}
                  />
                ) : null}
              </SettingsSection>
            ) : null}

            {mainSectionVisible("chat", settingsSearch.chat) ? (
              <SettingsSection sectionRef={chatSectionRef} title="Chat">
                {mainSettingVisible(settingsSearch.chat, "preferredAgentInterface") ? (
                  <PreferredAgentInterfaceField
                    description="Chat switches on automatically as soon as Ghostex detects a compatible agent. The terminal stays live in the background, and you can switch back at any time."
                    label="Default Agent View"
                    {...getSettingModificationProps("preferredAgentInterface")}
                    onChange={(preferredAgentInterface) =>
                      updateDraft("preferredAgentInterface", preferredAgentInterface)
                    }
                    value={draft.preferredAgentInterface}
                  />
                ) : null}
                {mainSettingVisible(settingsSearch.chat, "sessionChatTheme") ? (
                  <SessionChatThemeField
                    description="Changes chat content only; the surrounding Ghostex app remains dark."
                    label="Appearance"
                    {...getSettingModificationProps("sessionChatTheme")}
                    onChange={(value) => updateDraft("sessionChatTheme", value)}
                    value={draft.sessionChatTheme}
                  />
                ) : null}
                {mainSettingVisible(settingsSearch.chat, "sessionChatFontFamily") ? (
                  <TextField
                    description="Type an installed font family name. Leave blank to use the app font."
                    label="Font Family"
                    {...getSettingModificationProps("sessionChatFontFamily")}
                    onChange={(value) => updateDraft("sessionChatFontFamily", value)}
                    placeholder="App default"
                    value={draft.sessionChatFontFamily}
                  />
                ) : null}
                {mainSettingVisible(settingsSearch.chat, "sessionChatTranscriptWidthPercent") ? (
                  <SliderNumberField
                    description="Adjust message width only. The prompt composer at the bottom keeps its current width."
                    label="Message Width (%)"
                    {...getSettingModificationProps("sessionChatTranscriptWidthPercent")}
                    max={MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT}
                    min={MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT}
                    onCommit={(value) => updateDraft("sessionChatTranscriptWidthPercent", value)}
                    onChange={(value) =>
                      updateDraftDebounced("sessionChatTranscriptWidthPercent", value)
                    }
                    step={SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP}
                    value={draft.sessionChatTranscriptWidthPercent}
                  />
                ) : null}
                {mainSettingVisible(settingsSearch.chat, "sessionChatVerboseMode") ? (
                  <ToggleField
                    checked={draft.sessionChatVerboseMode}
                    description="Expand thinking blocks to show their tool calls by default. Individual command and output details remain collapsible. This is the default for new chats; the Verbose pill in a chat's composer overrides it for that chat only."
                    label="Verbose Mode"
                    {...getSettingModificationProps("sessionChatVerboseMode")}
                    onChange={(checked) => updateDraft("sessionChatVerboseMode", checked)}
                  />
                ) : null}
              </SettingsSection>
            ) : null}

            {PET_CONTROLS_VISIBLE && mainSectionVisible("statusIndicators", settingsSearch.statusIndicators) ? (
            <SettingsSection sectionRef={statusIndicatorsSectionRef} title="Status Indicators">
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

            {mainSubsectionVisible("browser", settingsSearch.browser) ? (
            <SettingsSection sectionRef={browserSectionRef} title="Browser">
              {/* CDXC:BrowserPanes 2026-05-27-07:24: Settings no longer exposes Chrome Canary attachment. Browser actions always open in workspace browser panes, leaving this section focused on pane behavior controls. */}
              {mainSettingVisible(settingsSearch.browser, "webLinkOpenTarget") ? (
              /*
               * CDXC:TerminalLinkInAppBrowser 2026-07-02-13:05:
               * Command-clicked terminal web links route into the project
               * Browser view by default, and the in-app toast points users at
               * this control. Keep it a normal visible Browser setting so the
               * toast's "change in settings" hint stays discoverable.
               *
               * CDXC:GPUISessionChatLinks 2026-08-18:
               * The same control also routes web links clicked in session chat,
               * so one Browser setting covers every agent-sent web link.
               *
               * CDXC:WebLinkOpenTarget 2026-08-19:
               * Detected dev-server rows read it too. This replaced a Browser
               * toggle plus a Dev Servers dropdown that answered the same
               * question with opposite defaults; a select rather than a toggle
               * because the destination, not an on/off state, is the choice.
               */
              <SelectField
                description="Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser."
                label="Open links in"
                {...getSettingModificationProps("webLinkOpenTarget")}
                onChange={(value) => updateDraft("webLinkOpenTarget", value as WebLinkOpenTarget)}
                options={WEB_LINK_OPEN_TARGET_OPTIONS}
                value={draft.webLinkOpenTarget}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSubsectionVisible("sessionCards", settingsSearch.sessionCards) ? (
            <SettingsSection sectionRef={sessionCardsSectionRef} title="Session Cards">
              {/* CDXC:SidebarSessionAgentIcons 2026-06-29-23:58: Users need a Session Cards toggle for colored agent brand artwork while the default sidebar remains monochrome and favorite rows no longer gold-tint agent logos. CDXC:SidebarSessionAgentIcons 2026-06-30-22:40: The colored agent icon setting must also color the selected-agent launcher icon so the Mac sidebar picker and session cards use the same agent identity mode. */}
              {mainSettingVisible(settingsSearch.sessionCards, "useColoredSessionAgentIcons") ? (
              <ToggleField
                checked={draft.useColoredSessionAgentIcons}
                description="Render session and selected-agent logos with colored brand artwork instead of monochrome masks."
                label="Use colored agent icons"
                {...getSettingModificationProps("useColoredSessionAgentIcons")}
                onChange={(checked) => updateDraft("useColoredSessionAgentIcons", checked)}
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
            </SettingsSection>
            ) : null}

            {mainSubsectionVisible("terminal", settingsSearch.terminal) ? (
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
                  {/* CDXC:TerminalSettings 2026-06-23-05:48:
                      The shared-config notice is informational, not a warning, so
                      it uses the neutral Info box pattern (muted border/background
                      plus an info icon) instead of any colored alert tint, matching
                      the IconInfoCircle info boxes used elsewhere in Settings. */}
                  <div className="flex items-start gap-3 rounded-none border border-border bg-muted/20 px-4 py-3 text-sm leading-6 text-muted-foreground">
                    <IconInfoCircle aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-foreground" />
                    <p className="m-0">
                      Whatever you set here also applies to your external Ghostty terminal
                      because this Ghostty terminal uses the same settings file. ghostex reloads
                      its embedded Ghostty terminal about 3 seconds after you stop changing
                      these controls; external Ghostty windows may still need Cmd+Shift+, to
                      reload.
                    </p>
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
              {IS_WINDOWS_HOST &&
              mainSettingVisible(settingsSearch.terminal, "windowsTerminalBackend") ? (
                <SelectField
                  description="Windows terminals currently use WSL2 with gxserver and zmx persistence. PowerShell mode will be added later."
                  label="Windows terminal backend"
                  {...getSettingModificationProps("windowsTerminalBackend")}
                  onChange={() => updateDraft("windowsTerminalBackend", "wsl")}
                  options={WINDOWS_TERMINAL_BACKEND_OPTIONS}
                  value={draft.windowsTerminalBackend}
                />
              ) : null}
              {IS_WINDOWS_HOST &&
              mainSettingVisible(settingsSearch.terminal, "windowsWslDistribution") ? (
                <TextField
                  description="Leave blank to use the default initialized WSL2 distribution. If discovery cannot find the intended install, enter its exact name as shown by `wsl.exe --list --verbose` (for example, Ubuntu-24.04). Ghostex never installs WSL automatically."
                  label="WSL Distribution"
                  {...getSettingModificationProps("windowsWslDistribution")}
                  onChange={(value) => updateDraft("windowsWslDistribution", value)}
                  placeholder="Automatic"
                  value={draft.windowsWslDistribution}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "workspaceBackgroundColor") ? (
                <ColorField
                  description="Color shown behind terminal panes."
                  label="Terminal Background"
                  {...getSettingModificationProps("workspaceBackgroundColor")}
                  onChange={(value) => updateDraft("workspaceBackgroundColor", value)}
                  value={draft.workspaceBackgroundColor}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalBackgroundImage") ? (
                <TextField
                  browseLabel="Choose image file"
                  description="Absolute path to an image drawn behind terminal panes. Leave blank for none."
                  label="Background Image"
                  {...getSettingModificationProps("terminalBackgroundImage")}
                  onBrowse={
                    nativeFilePickerAvailable ? chooseTerminalBackgroundImageFile : undefined
                  }
                  onChange={(value) => updateDraft("terminalBackgroundImage", value)}
                  placeholder="/Users/you/Pictures/background.png"
                  value={draft.terminalBackgroundImage}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalBackgroundImageOpacity") ? (
                <SliderNumberField
                  description="Blend the background image toward the terminal background color."
                  label="Background Image Opacity"
                  {...getSettingModificationProps("terminalBackgroundImageOpacity")}
                  max={1}
                  min={0}
                  onCommit={(value) => updateDraft("terminalBackgroundImageOpacity", value)}
                  onChange={(value) =>
                    updateDraftDebounced("terminalBackgroundImageOpacity", value)
                  }
                  step={0.05}
                  value={draft.terminalBackgroundImageOpacity}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalBackgroundImageFit") ? (
                <SelectField
                  description="How the background image is scaled inside each pane."
                  label="Background Image Fit"
                  {...getSettingModificationProps("terminalBackgroundImageFit")}
                  onChange={(value) =>
                    updateDraft("terminalBackgroundImageFit", value as TerminalBackgroundImageFit)
                  }
                  options={[
                    { label: "Cover", value: "cover" },
                    { label: "Contain", value: "contain" },
                    { label: "Stretch", value: "stretch" },
                    { label: "Natural size", value: "natural" },
                  ]}
                  value={draft.terminalBackgroundImageFit}
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
              {mainSettingVisible(settingsSearch.terminal, "terminalPaneHorizontalPaddingPx") ? (
                /*
                 * CDXC:TerminalPanePadding 2026-06-25-21:27:
                 * Horizontal terminal padding is a native pane content inset,
                 * not spacing between split panes. Keep the slider integer-pixel
                 * based and default it to zero so existing terminal layouts stay
                 * edge-to-edge until the user opts in.
                 */
                <SliderNumberField
                  description="Add left and right inner padding inside every terminal pane."
                  label="Horizontal Padding"
                  {...getSettingModificationProps("terminalPaneHorizontalPaddingPx")}
                  max={MAX_TERMINAL_PANE_PADDING_PX}
                  min={MIN_TERMINAL_PANE_PADDING_PX}
                  onCommit={(value) => updateDraft("terminalPaneHorizontalPaddingPx", value)}
                  onChange={(value) => updateDraftDebounced("terminalPaneHorizontalPaddingPx", value)}
                  step={1}
                  value={draft.terminalPaneHorizontalPaddingPx}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "terminalPaneVerticalPaddingPx") ? (
                /*
                 * CDXC:TerminalPanePadding 2026-06-25-21:27:
                 * Vertical terminal padding uses the same native content inset as
                 * horizontal padding while leaving pane titlebars, split dividers,
                 * and terminal chrome in their existing frames.
                 */
                <SliderNumberField
                  description="Add top and bottom inner padding inside every terminal pane."
                  label="Vertical Padding"
                  {...getSettingModificationProps("terminalPaneVerticalPaddingPx")}
                  max={MAX_TERMINAL_PANE_PADDING_PX}
                  min={MIN_TERMINAL_PANE_PADDING_PX}
                  onCommit={(value) => updateDraft("terminalPaneVerticalPaddingPx", value)}
                  onChange={(value) => updateDraftDebounced("terminalPaneVerticalPaddingPx", value)}
                  step={1}
                  value={draft.terminalPaneVerticalPaddingPx}
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
                    Users can disable persistence, but the Settings dropdown must warn that the React Native Android attach flow depends on persistent provider sessions. Show the warning only while Off is selected so the risk is visible at the decision point without making the default zmx state noisy. */
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
                          React Native Android attach can have issues while persistence is disabled.
                        </span>
                      </div>
                    ) : undefined
                  }
                  value={draft.sessionPersistenceProvider}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "clickToWakeSleepingSessions") ? (
                <ToggleField
                  checked={draft.clickToWakeSleepingSessions}
                  description="Selecting a sleeping pane tab shows a black placeholder; click the pane body to wake the session."
                  label="Click to wake sleeping panes"
                  {...getSettingModificationProps("clickToWakeSleepingSessions")}
                  onChange={(checked) => updateDraft("clickToWakeSleepingSessions", checked)}
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
              {mainSettingVisible(settingsSearch.terminal, "showNotificationOnTerminalBell") ? (
                /*
                 * CDXC:TerminalBellAttention 2026-07-01-01:13:
                 * Terminal bell notifications belong with Terminal settings because
                 * the event originates from shell/PTY behavior, not agent completion
                 * audio. Keep the setting off by default so failed zsh completion
                 * tabs do not create macOS banners or #95d7f6 attention chrome.
                 */
                <ToggleField
                  checked={draft.showNotificationOnTerminalBell}
                  description="Treat terminal bell events as session attention."
                  label="Show notification on terminal bell"
                  {...getSettingModificationProps("showNotificationOnTerminalBell")}
                  onChange={(checked) => updateDraft("showNotificationOnTerminalBell", checked)}
                />
              ) : null}
              {mainSettingVisible(settingsSearch.terminal, "promptEditorBackend") ? (
                /**
                 * CDXC:PromptEditorBackend 2026-05-11-14:38
                 * Ctrl+G prompt editing can render through the native WebKit
                 * Monaco editor or leave the terminal's machine-level editor
                 * settings untouched.
                 *
                 * CDXC:PromptEditorBackend 2026-06-30-00:08:
                 * The Settings dropdown must only offer Monaco and "Use default
                 * from this machine"; remove gte install/use and custom command
                 * controls from this surface.
                 */
                <PromptEditorBackendField
                  advanced={getSettingModificationProps("promptEditorBackend").advanced}
                  backend={draft.promptEditorBackend}
                  isModified={getSettingModificationProps("promptEditorBackend").isModified}
                  onChange={(backend) => updateDraft("promptEditorBackend", backend)}
                  onResetToDefault={
                    getSettingModificationProps("promptEditorBackend").onResetToDefault
                  }
                />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSubsectionVisible("terminalBehavior", settingsSearch.terminalBehavior) ? (
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

            {mainSubsectionVisible("terminalScrolling", settingsSearch.terminalScrolling) ? (
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

            {mainSubsectionVisible("terminalDevServers", settingsSearch.terminalDevServers) ? (
              <SettingsSection
                description="Choose how Ghostex discovers running dev servers and which ports stay hidden. Detected URLs follow Browser → Open links in."
                sectionRef={terminalDevServersSectionRef}
                title="Dev Servers"
              >
                {/*
                 * CDXC:TerminalDevServers 2026-06-23-19:22:
                 * Dev-server settings are terminal-adjacent app behavior. Keep detection, one launch destination, and ignored port rules together so users can tune server discovery without editing terminal emulator config or managing individual browser targets.
                 */}
                {mainSettingVisible(
                  settingsSearch.terminalDevServers,
                  "terminalDevServerDetectionEnabled",
                ) ? (
                  <ToggleField
                    checked={draft.terminalDevServerDetectionEnabled}
                    description="Detect localhost dev server URLs from terminal output."
                    label="Detect running servers in terminals"
                    {...getSettingModificationProps("terminalDevServerDetectionEnabled")}
                    onChange={(checked) =>
                      updateDraft("terminalDevServerDetectionEnabled", checked)
                    }
                  />
                ) : null}
                {mainSettingVisible(
                  settingsSearch.terminalDevServers,
                  "terminalDevServerIgnoredPortRules",
                ) ? (
                  <TerminalDevServerIgnoredPortsField
                    ignoredPortRules={draft.terminalDevServerIgnoredPortRules}
                    {...getSettingModificationProps("terminalDevServerIgnoredPortRules")}
                    onChange={(ignoredPortRules) =>
                      updateDraft("terminalDevServerIgnoredPortRules", ignoredPortRules)
                    }
                  />
                ) : null}
              </SettingsSection>
            ) : null}

            {mainSubsectionVisible("editor", settingsSearch.editor) ? (
            <SettingsSection sectionRef={editorSectionRef} title="Editor">
              {/* CDXC:EditorPanes 2026-06-08-20:12: Embedded code-server panes
                  use Ghostex-owned bundled editor settings by default so the
                  macOS VS Code surface starts on Dark 2026. This toggle opts
                  into linking local VS Code settings, while the Insiders
                  checkbox only changes the linked config directory. */}
              {mainSettingVisible(settingsSearch.editor, "codeServerLinkVscodeUserConfig") ? (
              <ToggleField
                advanced={isAdvancedMainSetting("codeServerLinkVscodeUserConfig")}
                checked={draft.codeServerLinkVscodeUserConfig}
                description="Use local VS Code settings instead of the bundled editor defaults."
                label="Use VS Code settings"
                onChange={(checked) => updateDraft("codeServerLinkVscodeUserConfig", checked)}
              />
              ) : null}
              {mainSettingVisible(settingsSearch.editor, "codeServerUseVscodeInsidersUserConfig") ? (
              <ToggleField
                advanced={isAdvancedMainSetting("codeServerUseVscodeInsidersUserConfig")}
                checked={draft.codeServerUseVscodeInsidersUserConfig}
                description="Use the VS Code Insiders user settings directory."
                disabled={!draft.codeServerLinkVscodeUserConfig}
                disabledReason="Turn on “Link VS Code user settings” first."
                label="Use VS Code Insiders settings"
                onChange={(checked) =>
                  updateDraft("codeServerUseVscodeInsidersUserConfig", checked)
                }
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

            {/*
             * CDXC:AppIconPicker 2026-06-28-06:05:
             * The advanced App Icon section is a custom-image control, not a bundled preset picker. Show one preview, one Select Image action, and an inline X on the custom preview to restore the default icon; omit separate reset and folder-reveal actions so the flow stays direct.
             */}
            {mainSubsectionVisible("appIcon", settingsSearch.appIcon) ? (
            <SettingsSection
              description="Changes the Dock and app-switcher icon. The app file icon may also change when macOS allows it."
              sectionRef={appIconSectionRef}
              title="App Icon"
            >
              {mainSettingVisible(settingsSearch.appIcon, "appIconSourceId") ? (
              <AppIconPickerField
                advanced={isAdvancedMainSetting("appIconSourceId")}
                error={appIconError}
                onChooseFile={chooseAppIconFile}
                onSelect={selectAppIcon}
                state={appIconState}
              />
              ) : null}
            </SettingsSection>
            ) : null}

            {mainSubsectionVisible("autoSleep", settingsSearch.autoSleep) ? (
            <SettingsSection sectionRef={autoSleepSectionRef} title="Auto Sleep">
              {/* CDXC:AutoSleep 2026-05-28-08:32: Auto Sleep controls belong in one Settings section so VS Code, Git, Project, Manage, browser, and agent sessions can be tuned independently without hiding the relationship between the policies. */}
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

            {mainSubsectionVisible("power", settingsSearch.power) ? (
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
              {mainSettingVisible(settingsSearch.power, "keepAwakeWhileWorkingSessions") ? (
              <ToggleField
                checked={draft.keepAwakeWhileWorkingSessions}
                description="Keep the Mac awake while any session is Working and for 20 minutes after, so you have time to reply."
                label="Keep awake for working sessions"
                {...getSettingModificationProps("keepAwakeWhileWorkingSessions")}
                onChange={(checked) => updateDraft("keepAwakeWhileWorkingSessions", checked)}
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

            {mainSubsectionVisible("sounds", settingsSearch.sounds) ? (
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
                advanced={isAdvancedMainSetting("attentionNotificationActions")}
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

            {mainSubsectionVisible("sidebarTags", settingsSearch.sidebarTags) ? (
              <SettingsSection
                sectionRef={sidebarTagsSectionRef}
                title="Sidebar Tags"
              >
                {mainSettingVisible(settingsSearch.sidebarTags, "sidebarSessionTagListItems") ? (
                  <SidebarTagListSettingsField
                    isModified={
                      !areSidebarSessionTagListItemsEqual(
                        draft.sidebarSessionTagListItems,
                        DEFAULT_ghostex_SETTINGS.sidebarSessionTagListItems,
                      )
                    }
                    items={draft.sidebarSessionTagListItems}
                    onChange={(items) => updateDraft("sidebarSessionTagListItems", items)}
                    onResetToDefault={() =>
                      updateDraft(
                        "sidebarSessionTagListItems",
                        DEFAULT_ghostex_SETTINGS.sidebarSessionTagListItems,
                      )
                    }
                  />
                ) : null}
              </SettingsSection>
            ) : null}

            {mainSubsectionVisible("storage", settingsSearch.storage) ? (
              <div ref={storageSectionRef}>
                <GhostexFolderStatsSection
                  isLoading={ghostexFolderStatsLoading}
                  onOpenGhostexFolder={onOpenGhostexFolder}
                  stats={ghostexFolderStats}
                />
              </div>
            ) : null}

            {mainSubsectionVisible("beta", settingsSearch.beta) ? (
              <SettingsSection sectionRef={betaSectionRef} title="Experimental">
                {mainSettingVisible(settingsSearch.beta, "showBetaFeatures") ? (
                  <>
                    {/*
                     * CDXC:ExperimentalFeatures 2026-06-28-07:41:
                     * The Experimental section must keep a current visible
                     * inventory of every surface enabled by Enable Experimental
                     * Features. Update this list whenever a new experimental
                     * Settings tab, titlebar button, or browser address-bar
                     * control is added or removed.
                     *
                     * CDXC:TitlebarKeepAwake 2026-06-19-13:13:
                     * Keep Awake belongs in the Experimental inventory because
                     * the Power settings section, titlebar button, and titlebar
                     * runtime automation stay hidden until Enable Experimental
                     * Features is enabled.
                     *
                     * CDXC:GPUIAutomateStable 2026-07-26:
                     * GPUI has graduated project Automate from this gate. The
                     * shared macOS host still inventories Automate here, while
                     * GPUI lists only the Quick Automations Overview preview.
                     */}
                    <ToggleField
                      checked={draft.showBetaFeatures}
                      description={
                        automateIsExperimental
                          ? "Show experimental settings, Automations and Automate pages, and the Keep Awake title-bar button."
                          : "Show experimental settings, Automations Overview, and the Keep Awake title-bar button."
                      }
                      label="Enable Experimental Features"
                      {...getSettingModificationProps("showBetaFeatures")}
                      onChange={(checked) => updateDraft("showBetaFeatures", checked)}
                    />
                    <div className="rounded-[var(--settings-radius-control)] border border-border bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
                      <div className="mb-2 font-medium text-foreground">Enabled when on</div>
                      <ul className="grid gap-1.5">
                        <li>OS Integration settings tab</li>
                        <li>
                          {automateIsExperimental
                            ? "Automations Overview and project Automate pages"
                            : "Automations Overview"}
                        </li>
                        <li>Title bar and Power settings: Keep Awake</li>
                      </ul>
                    </div>
                  </>
                ) : null}
              </SettingsSection>
            ) : null}

            {mainSubsectionVisible("debugging", settingsSearch.debugging) ? (
              <SettingsSection sectionRef={debuggingSectionRef} title="Debugging">
                {debuggingSettingVisible("debuggingMode") ? (
                  <ToggleField
                    checked={draft.debuggingMode}
                    description={
                      draft.debuggingMode
                        ? "Shows debug-only UI controls and allows the enabled diagnostic scenarios below to write routine logs."
                        : "Turn on to reveal debug-only controls and allow routine diagnostic logging. Important warnings, errors, and crashes remain captured."
                    }
                    label="Show debug UI controls"
                    {...getSettingModificationProps("debuggingMode")}
                    onChange={(checked) => updateDraft("debuggingMode", checked)}
                  />
                ) : null}
                {debuggingSettingVisible("diagnosticLogging") ? (
                  <DiagnosticLoggingSettingsField
                    isModified={
                      !areDiagnosticLoggingSettingsEqual(
                        draft.diagnosticLogging,
                        DEFAULT_ghostex_SETTINGS.diagnosticLogging,
                      )
                    }
                    onChange={updateDiagnosticLoggingScenario}
                    onResetToDefault={() =>
                      updateDraft(
                        "diagnosticLogging",
                        DEFAULT_ghostex_SETTINGS.diagnosticLogging,
                      )
                    }
                    value={draft.diagnosticLogging}
                  />
                ) : null}
                {debuggingSettingVisible("showSessionCommandCopyActions") ? (
                  <>
                    {/*
                     * CDXC:SidebarContextMenu 2026-06-09-23:17:
                     * Copy resume and Copy attach command are advanced session-card context-menu utilities. Keep both hidden unless this Settings toggle is enabled so the default menu stays focused on normal session actions.
                     *
                     * CDXC:DebuggingSettings 2026-06-15-21:34:
                     * Command copy actions are support-oriented session-card context-menu controls and should appear in the bottom Debugging section rather than the everyday Session Cards section.
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
                {debuggingSettingVisible("showSessionDetailsCopyAction") ? (
                  <>
                    {/*
                     * CDXC:SidebarContextMenu 2026-06-11-23:08:
                     * Copy details is separate from command-copy actions because it copies metadata, not executable shell commands. Keep it opt-in so users choose when session ids and project paths appear in context menus.
                     *
                     * CDXC:DebuggingSettings 2026-06-15-21:34:
                     * Copy details can expose support metadata in the context menu, so Settings groups it with Debugging rather than normal session-card appearance controls.
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

            {!isFirstLaunchSetup && !hasVisibleMainSettings ? (
              <SettingsSearchNoMatchesNotice
                activeTab={activeTab}
                matchingPages={settingsSearchMatchingPages}
                onSelectPage={setActiveTab}
              />
            ) : null}

            {isFirstLaunchSetup ? (
              <div className="flex justify-end pt-2">
                <Button
                  className="h-10 px-5 text-sm"
                  onClick={closeSettingsModal}
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
          </SettingsNativeScrollArea>
          </div>
          </TabsContent>
          {!isFirstLaunchSetup && showOSIntegrationSettingsTab ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="osIntegration">
            <OSIntegrationSettingsTab
              loading={osIntegrationStatusLoading}
              onRequestStatus={onRequestOSIntegrationStatus}
              onSetDefaults={onSetOSIntegrationDefaults}
              search={extraSettingsTabSearches.osIntegration}
              searchEmptyState={settingsSearchEmptyState}
              status={osIntegrationStatus}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="integrations">
            <IntegrationsSettingsTab
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              appShotsEnabled={draft.appShotsEnabled}
              appShotsHotkey={draft.appShotsHotkey}
              appShotsMetadataEnabled={draft.appShotsMetadataEnabled}
              onAppShotsEnabledChange={(checked) => updateDraft("appShotsEnabled", checked)}
              onAppShotsHotkeyChange={(hotkey) => updateDraft("appShotsHotkey", hotkey)}
              onAppShotsMetadataEnabledChange={(checked) =>
                updateDraft("appShotsMetadataEnabled", checked)
              }
              onInstallAgentOrchestrationSkill={onInstallAgentOrchestrationSkill}
              onInstallBrowserControl={onInstallBrowserControl}
              onInstallBrowserUseSkill={onInstallBrowserUseSkill}
              onInstallComputerUseSkill={onInstallComputerUseSkill}
              onInstallCuaDriver={onInstallCuaDriver}
              onInstallFable56OrchestrationSkill={onInstallFable56OrchestrationSkill}
              onInstallFindPrevSessionSkill={onInstallFindPrevSessionSkill}
              onInstallGenerateTitleSkill={onInstallGenerateTitleSkill}
              onInstallGhostexCli={onInstallGhostexCli}
              onInstallMoveCodexSessionSkill={onInstallMoveCodexSessionSkill}
              onOpenExternalUrl={(url) => vscode?.postMessage({ type: "openExternalUrl", url })}
              onUninstallBundledAgentSkill={onUninstallBundledAgentSkill}
              onUninstallBundledAgentSkills={onUninstallBundledAgentSkills}
              onOpenAccessibilityPreferences={onOpenAccessibilityPreferences}
              onOpenScreenRecordingPreferences={onOpenScreenRecordingPreferences}
              onRequestGhostexCliStatus={onRequestGhostexCliStatus}
              search={extraSettingsTabSearches.integrations}
              searchEmptyState={settingsSearchEmptyState}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="plugins">
            <PluginsSettingsTab
              ghostexCliStatus={ghostexCliStatus}
              ghostexCliStatusLoading={ghostexCliStatusLoading}
              onInstallCuaDriver={onInstallCuaDriver}
              onRequestGhostexCliStatus={onRequestGhostexCliStatus}
              onRequestStatus={onRequestPluginSettingsStatus}
              onReinstallPlugin={onReinstallPlugin}
              onUpdateSetting={updateDraft}
              search={extraSettingsTabSearches.plugins}
              searchEmptyState={settingsSearchEmptyState}
              settings={draft}
              status={pluginSettingsStatus}
              statusLoading={pluginSettingsStatusLoading}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="remote">
            <RemoteSettingsTab
              initialRemoteMachineId={initialRemoteMachineId}
              isActive={isOpen && activeTab === "remote"}
              onChange={(nextRemoteMachines) =>
                applySettingsPatch(
                  {
                    remoteMachines: nextRemoteMachines,
                  },
                  "settings:remoteMachines",
                )
              }
              remoteMachines={draft.remoteMachines}
              search={extraSettingsTabSearches.remote}
              searchEmptyState={settingsSearchEmptyState}
              vscode={vscode}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="projects">
            <ProjectsSettingsPanel
              onGlobalBeadsDirectoryChange={(value) => updateDraft("globalBeadsDirectory", value)}
              onGlobalBeadsDisplayKeyChange={(value) =>
                updateDraft("globalBeadsDisplayKey", value)
              }
              onGlobalDocsDirectoryChange={(value) => updateDraft("globalDocsDirectory", value)}
              onGlobalWorktreeCommandChange={(value) =>
                updateDraft("globalWorktreeCommand", value)
              }
              onManageAdditionalDocsFoldersChange={(value) =>
                updateDraft("manageAdditionalDocsFolders", value)
              }
              onPortlessEnabledChange={(checked) => updateDraft("portlessEnabled", checked)}
              onPortlessProtocolChange={(protocol) => updateDraft("portlessProtocol", protocol)}
              portless={portless}
              projects={projects}
              search={extraSettingsTabSearches.projects}
              searchEmptyState={settingsSearchEmptyState}
              settings={draft}
              vscode={vscode}
            />
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
                updateDraft("agentAcceptAllEnabled", checked)
              }
              onDefaultPromptAgentIdChange={(agentId) =>
                updateDraft("defaultPromptAgentId", agentId)
              }
              onCustomSessionTitleGenerationCommandChange={(command) =>
                updateDraft("customSessionTitleGenerationCommand", command)
              }
              onInstallAgentHooks={onInstallAgentHooks}
              onRequestAgentHookStatus={onRequestAgentHookStatus}
              onSessionTitleGenerationAgentChange={(agent) =>
                updateDraft("sessionTitleGenerationAgent", agent)
              }
              onUninstallAgentHooks={onUninstallAgentHooks}
              search={extraSettingsTabSearches.agents}
              searchEmptyState={settingsSearchEmptyState}
              vscode={vscode}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="actions">
            <ActionsSettingsTab
              getSettingModificationProps={getSettingModificationProps}
              hideTabStripNewBrowserButton={draft.hideTabStripNewBrowserButton}
              hideTabStripNewChatButton={draft.hideTabStripNewChatButton}
              hideTabStripNewTerminalButton={draft.hideTabStripNewTerminalButton}
              onHideTabStripNewBrowserButtonChange={(checked) =>
                updateDraft("hideTabStripNewBrowserButton", checked)
              }
              onHideTabStripNewChatButtonChange={(checked) =>
                updateDraft("hideTabStripNewChatButton", checked)
              }
              onHideTabStripNewTerminalButtonChange={(checked) =>
                updateDraft("hideTabStripNewTerminalButton", checked)
              }
              search={extraSettingsTabSearches.actions}
              searchEmptyState={settingsSearchEmptyState}
              vscode={vscode}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="openTargets">
            <OpenTargetsSettingsTab
              onChange={(nextSettings) => applySettings(nextSettings)}
              search={extraSettingsTabSearches.openTargets}
              searchEmptyState={settingsSearchEmptyState}
              settings={draft}
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="settings-main-tabs-content mt-0 min-h-0 flex-1 overflow-hidden" value="hotkeys">
            <HotkeysSettingsTab
              definitionsById={hotkeyDefinitionsById}
              expandCollapsedProjectsOnJump={draft.expandCollapsedProjectsOnJump}
              expandCollapsedProjectsOnJumpModification={getSettingModificationProps(
                "expandCollapsedProjectsOnJump",
              )}
              hotkeys={draft.hotkeys}
              sectionRefs={hotkeySectionRefs}
              sectionSearches={hotkeySectionSearches}
              showLessForExpandedProjectJumps={draft.showLessForExpandedProjectJumps}
              showLessForExpandedProjectJumpsModification={getSettingModificationProps(
                "showLessForExpandedProjectJumps",
              )}
              visibleSections={visibleHotkeySections}
              searchQuery={settingsSearchQuery}
              onChange={(hotkeys) => updateDraft("hotkeys", hotkeys)}
              onActiveSectionChange={(sectionId) =>
                setActiveHotkeySettingsSectionId((currentSectionId) =>
                  currentSectionId === sectionId ? currentSectionId : sectionId,
                )
              }
              onExpandCollapsedProjectsOnJumpChange={(checked) =>
                updateDraft("expandCollapsedProjectsOnJump", checked)
              }
              onShowLessForExpandedProjectJumpsChange={(checked) =>
                updateDraft("showLessForExpandedProjectJumps", checked)
              }
            />
          </TabsContent>
          ) : null}
          {!isFirstLaunchSetup ? (
          <TabsContent className="mt-0 min-h-0 flex-1 overflow-hidden" value="about">
            <AboutSettingsTab
              search={extraSettingsTabSearches.about}
              searchEmptyState={settingsSearchEmptyState}
              vscode={vscode}
            />
          </TabsContent>
          ) : null}
            </div>
          </div>
          </Tabs>
        </TooltipProvider>
      </DialogContent>
    </Dialog>
  );
}

function SettingsSearchNoMatchesNotice({
  activeTab,
  matchingPages,
  onSelectPage,
}: {
  activeTab: SettingsModalTab;
  matchingPages: readonly SettingsSidebarPage[];
  onSelectPage: (pageId: SettingsModalTab) => void;
}) {
  const otherPages = matchingPages.filter((page) => page.id !== activeTab);
  return (
    <div className="rounded-none border border-border bg-muted/30 px-4 py-6 text-center text-sm text-muted-foreground">
      <p>
        {otherPages.length
          ? "No settings on this page match your search."
          : "No settings match your search."}
      </p>
      {otherPages.length ? (
        <div className="mt-3 flex flex-wrap items-center justify-center gap-2">
          <span>Matches on:</span>
          {otherPages.map((page) => {
            const PageIcon = page.icon;
            return (
              <Button
                key={page.id}
                onClick={() => onSelectPage(page.id)}
                size="sm"
                type="button"
                variant="outline"
              >
                <PageIcon aria-hidden="true" data-icon="inline-start" />
                {page.title}
              </Button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

function SettingsSidebarNavigation({
  expandedPages,
  onShowAdvancedSettingsChange,
  onTogglePage,
  pages,
  showAdvancedSettings,
  showAdvancedSettingsId,
}: {
  expandedPages: Partial<Record<SettingsModalTab, boolean>>;
  onShowAdvancedSettingsChange: (checked: boolean) => void;
  onTogglePage: (pageId: SettingsModalTab) => void;
  pages: readonly SettingsSidebarPage[];
  showAdvancedSettings: boolean;
  showAdvancedSettingsId: string;
}) {
  return (
    <aside aria-label="Settings pages and sections" className="settings-section-sidebar">
      <TabsList className="settings-sidebar-tabs-list vertical-scroll-fade-mask">
        {pages.map((page) => {
          const hasSections = Boolean(page.sections?.length);
          const expanded = Boolean(expandedPages[page.id]);
          const PageIcon = page.icon;
          return (
            <div
              className={cn(
                "settings-sidebar-page-group",
                page.id === "about" && "settings-sidebar-page-group-about",
              )}
              key={page.id}
            >
              <div className="settings-sidebar-page-row">
                {/*
                 * CDXC:SettingsNavigation 2026-06-29-21:45:
                 * Expandable Settings sidebar headers must expand and collapse from the full visible header, not only from the disclosure chevron, because the row highlight presents the icon, label, and chevron as one control.
                 */}
                <TabsTrigger
                  className="settings-sidebar-tab-trigger"
                  onClick={() => {
                    if (hasSections) {
                      onTogglePage(page.id);
                    }
                  }}
                  value={page.id}
                >
                  <PageIcon aria-hidden="true" data-icon="inline-start" />
                  <span className="settings-sidebar-page-title truncate">{page.title}</span>
                </TabsTrigger>
                {hasSections ? (
                  <Button
                    aria-label={`${expanded ? "Collapse" : "Expand"} ${page.title} sections`}
                    className="settings-sidebar-page-disclosure"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      onTogglePage(page.id);
                    }}
                    size="icon-xs"
                    type="button"
                    variant="ghost"
                  >
                    {expanded ? (
                      <IconChevronDown aria-hidden="true" />
                    ) : (
                      <IconChevronRight aria-hidden="true" />
                    )}
                  </Button>
                ) : null}
              </div>
              {hasSections && expanded ? (
                <div className="settings-sidebar-subsection-list">
                  {page.sections?.map((section) => (
                    <Fragment key={section.id}>
                      <Button
                        className="settings-section-sidebar-button settings-sidebar-subsection-button"
                        data-active={section.active ? "true" : "false"}
                        onClick={section.onSelect}
                        type="button"
                        variant="ghost"
                      >
                        {section.title}
                      </Button>
                      {/*
                       * CDXC:SettingsNavigation 2026-08-19:
                       * Nested rows belong to the section being read, so they
                       * appear under the active group only. Showing every
                       * group's rows at once would turn a nine-row rail into a
                       * twenty-row one and bury the categories.
                       */}
                      {section.active && section.subsections?.length ? (
                        <div className="settings-sidebar-nested-subsection-list">
                          {section.subsections.map((subsection) => (
                            <Button
                              className="settings-section-sidebar-button settings-sidebar-subsection-button settings-sidebar-nested-subsection-button"
                              data-active={subsection.active ? "true" : "false"}
                              key={subsection.id}
                              onClick={subsection.onSelect}
                              type="button"
                              variant="ghost"
                            >
                              {subsection.title}
                            </Button>
                          ))}
                        </div>
                      ) : null}
                    </Fragment>
                  ))}
                </div>
              ) : null}
            </div>
          );
        })}
      </TabsList>
      {/*
       * CDXC:SettingsNavigation 2026-06-24-22:16:
       * The sidebar owns both top-level Settings pages and expandable section
       * links, while Show Advanced remains pinned to the bottom of that same
       * rail instead of returning to header chrome.
       */}
      <div className="settings-section-sidebar-footer">
        <label className="settings-show-advanced-toggle" htmlFor={showAdvancedSettingsId}>
          <span className="settings-show-advanced-copy">Show Advanced</span>
          <Switch
            checked={showAdvancedSettings}
            id={showAdvancedSettingsId}
            onCheckedChange={onShowAdvancedSettingsChange}
          />
        </label>
      </div>
    </aside>
  );
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
            {stats?.folderPath ?? "~/.local/share/ghostex"}
          </div>
        </div>
        <SettingButton
          className="h-9 shrink-0 gap-2 px-3 text-sm"
          disabled={!onOpenGhostexFolder}
          disabledReason="Folder access isn’t available here."
          onClick={onOpenGhostexFolder}
          type="button"
          variant="outline"
        >
          <IconFolderOpen aria-hidden="true" className="size-4" />
          Open Folder
        </SettingButton>
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

function PromptEditorBackendField({
  advanced,
  backend,
  isModified,
  onChange,
  onResetToDefault,
}: {
  advanced?: boolean;
  backend: PromptEditorBackend;
  isModified?: boolean;
  onChange: (backend: PromptEditorBackend) => void;
  onResetToDefault?: () => void;
}) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description="Choose which editor new terminals use when Ctrl+G asks the shell to edit prompt text."
      htmlFor={id}
      isModified={isModified}
      label="Ctrl+G prompt editor"
      onResetToDefault={onResetToDefault}
    >
      <SettingsSelect
        onValueChange={(value) => onChange(value as PromptEditorBackend)}
        value={backend}
      >
        <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
          <SelectValue />
        </SelectTrigger>
        <SettingsSelectContent>
          <SelectGroup>
            {PROMPT_EDITOR_BACKEND_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SettingsSelectContent>
      </SettingsSelect>
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
