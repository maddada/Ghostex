import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type UIEvent as ReactUIEvent,
} from 'react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { Command } from '@/packages/components/ui/command';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/packages/components/ui/dialog';
import { Select, SelectGroup, SelectItem, SelectTrigger, SelectValue } from '@/packages/components/ui/select';
import { Separator } from '@/packages/components/ui/separator';
import { Switch } from '@/packages/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/packages/components/ui/tabs';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/packages/components/ui/tooltip';
import { SidebarSessionSearchField } from './sidebar-session-search-overlay';
import {
  resolveSettingsModalTabForVisibility,
  shouldShowOSIntegrationSettingsTab,
  type SettingsModalTab,
  type SettingsModalTabVisibilityOptions,
} from './settings-modal-tabs';
import { IconChevronDown, IconChevronRight, IconFolderOpen, IconInfoCircle } from '@tabler/icons-react';
import { type CompletionSoundSetting } from '../shared/completion-sound';
import { GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES } from '../shared/ghostty-config-actions';
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
} from '../shared/session-grid-contract';
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  DEFAULT_ghostex_SETTINGS,
  MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MAX_TERMINAL_PANE_PADDING_PX,
  MAX_TERMINAL_VIEW_WIDTH_PERCENT,
  MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS,
  GHOSTTY_COPY_ON_SELECT_OPTIONS,
  GHOSTTY_SCROLLBAR_OPTIONS,
  GHOSTTY_THEME_SETTING_OPTIONS,
  KEEP_AWAKE_DURATION_OPTIONS,
  MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MIN_TERMINAL_PANE_PADDING_PX,
  MIN_TERMINAL_VIEW_WIDTH_PERCENT,
  MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT,
  MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT,
  PROMPT_EDITOR_BACKEND_OPTIONS,
  WINDOWS_TERMINAL_BACKEND_OPTIONS,
  type PromptEditorBackend,
  SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS,
  SIDEBAR_SIDE_OPTIONS,
  WEB_LINK_OPEN_TARGET_OPTIONS,
  areDiagnosticLoggingSettingsEqual,
  COMMANDS_PANEL_SIDE_OPTIONS,
  MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  MAX_SIDEBAR_DEFAULT_WIDTH_PX,
  MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX,
  MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS,
  MIN_SIDEBAR_DEFAULT_WIDTH_PX,
  SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS,
  SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP,
  TERMINAL_VIEW_WIDTH_PERCENT_STEP,
  normalizeghostexSettings,
  parseSidebarAutoSettleAfterDaysSelectValue,
  sidebarAutoSettleAfterDaysSelectValue,
  type AutoSleepIdleMinutes,
  type GhosttyConfirmCloseSurface,
  type GhosttyCopyOnSelect,
  type GhosttyScrollbar,
  type KeepAwakeDurationMinutes,
  type SettingsModalNavigationState,
  type CommandsPanelSide,
  type SidebarSide,
  type TerminalBackgroundImageFit,
  type WebLinkOpenTarget,
  type TerminalCursorStyle,
  type ghostexSettingsPatch,
  type ghostexSettingsUpdateSource,
  type ghostexSettings,
} from '../shared/ghostex-settings';
import { type BundledGhostexAgentSkillId } from '../shared/ghostex-agent-skills';
import { type FirstLaunchSetupMainSettingKey } from '../shared/first-launch-setup-settings';
import { PET_CONTROLS_VISIBLE } from '../shared/pets';
import { areSidebarSessionTagListItemsEqual } from '../shared/session-tags';
import { type WebviewApi } from './webview-api';
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
} from './settings-modal/fields';
import {
  getRememberedSettingsModalScrollTop,
  getRememberedSettingsModalTab,
  rememberSettingsModalScrollTop,
  rememberSettingsModalTab,
} from './settings-modal/navigation-memory';
import { getMostlyVisibleSettingsSectionId, isAdvancedMainSetting } from './settings-modal/search';
import { AboutSettingsTab } from './settings-modal/tabs/about';
import { ActionsSettingsTab } from './settings-modal/tabs/actions';
import { AgentsSettingsTab } from './settings-modal/tabs/agents';
import { HotkeysSettingsTab } from './settings-modal/tabs/hotkeys';
import { IntegrationsSettingsTab } from './settings-modal/tabs/integrations';
import { OpenTargetsSettingsTab } from './settings-modal/tabs/open-targets';
import { OSIntegrationSettingsTab } from './settings-modal/tabs/os-integration';
import { PluginsSettingsTab } from './settings-modal/tabs/plugins';
import { ProjectsSettingsPanel } from './settings-modal/tabs/projects';
import { RemoteSettingsTab } from './settings-modal/tabs/remote';
import {
  HotkeySettingsSectionId,
  MainSettingsScrollTargetId,
  MainSettingsSectionRefs,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL,
  RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE,
  SettingsSidebarPage,
} from './settings-modal/types';
import {
  IS_WINDOWS_HOST,
  PASTE_PREVIEWABLE_IMAGES_DESCRIPTION,
  getMainSettingsGroupSearch,
  getMainSettingsSectionNavigation,
  getSettingsSearchSections,
} from './settings-modal/search-catalog';
import {
  createMainSettingsVisibility,
  createVisibleMainSettingsNavigation,
} from './settings-modal/main-settings-visibility';
import { useHotkeySettings } from './settings-modal/use-hotkey-settings';
import { createSettingsSidebarPages } from './settings-modal/sidebar-pages';
import { useSettingsModalEffects } from './settings-modal/use-settings-modal-effects';
import { createSettingsPersistence } from './settings-modal/settings-persistence';
import { useAppIconSettings } from './settings-modal/use-app-icon-settings';
import { createSettingsActions, type GhosttySettingsAction } from './settings-modal/settings-actions';
import { getActiveSettingsModalScrollViewport } from './settings-modal/scroll-targets';

export type { SettingsModalTab } from './settings-modal-tabs';

const GHOSTTY_THEME_UNMANAGED_VALUE = '__ghostex_ghostty_theme_unmanaged__';

export type MainSettingsInitialSectionId = MainSettingsScrollTargetId;

function getInitialSettingsModalTab(
  initialTab: SettingsModalTab,
  visibility: SettingsModalTabVisibilityOptions,
  storedNavigation: SettingsModalNavigationState
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
    initialTab !== 'settings' ? initialTab : (getRememberedSettingsModalTab(storedNavigation) ?? initialTab);
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

export type { GhosttySettingsAction };

export type SettingsModalPresentation = 'default' | 'firstLaunchSetup';

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
  onInstallCliSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallCuaDriver?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallManageBeadsSkill?: () => void;
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
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem['id']) => void;
  onSetOSIntegrationDefaults?: (target: 'editor' | 'terminalLinks' | 'scriptRunner' | 'all') => void;
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
  initialTab = 'settings',
  isOpen,
  onChange,
  onPatch,
  onClose,
  presentation = 'default',
  onOpenAccessibilityPreferences,
  onOpenMacOSNotificationSettings,
  onOpenScreenRecordingPreferences,
  onOpenGhostexFolder,
  onGhosttySettingsAction,
  onInstallCliSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallCuaDriver,
  onInstallFable56OrchestrationSkill,
  onInstallManageBeadsSkill,
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
  theme = 'dark-blue',
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
  const isFirstLaunchSetup = presentation === 'firstLaunchSetup';
  const normalizedInitialSettings = normalizeghostexSettings(settings);
  const [draft, setDraft] = useState<ghostexSettings>(normalizedInitialSettings);
  /*
   * CDXC:SettingsAdvanced 2026-06-28-18:14:
   * Show Advanced must use the persisted settings draft as its single source of
   * truth. A separate React state can initialize before native settings hydrate
   * and make the switch look disabled again when Settings reopens.
   */
  const showAdvancedSettings = draft.showAdvancedSettings;
  const [settingsSearchQuery, setSettingsSearchQuery] = useState('');
  const [activeMainSettingsSectionId, setActiveMainSettingsSectionId] = useState<MainSettingsScrollTargetId>('sidebar');
  const [activeHotkeySettingsSectionId, setActiveHotkeySettingsSectionId] =
    useState<HotkeySettingsSectionId>('general');
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
      normalizedInitialSettings.settingsModalNavigation
    )
  );
  const dialogContentRef = useRef<HTMLDivElement>(null);
  const showAdvancedSettingsId = useId();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const pendingSettingsRef = useRef<ghostexSettings | undefined>(undefined);
  const pendingSettingsPatchRef = useRef<ghostexSettingsPatch | undefined>(undefined);
  const pendingTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pendingNavigationPersistTimeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
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
  // CDXC:AnonymousAnalytics 2026-08-26: Anchor for the always-visible Privacy section.
  const privacySectionRef = useRef<HTMLDivElement>(null);
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
  const isModalDarkTheme = getSidebarThemeVariant(modalTheme) === 'dark';
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
        getMainSettingsSectionMeasurementItems()
      );
      if (mostlyVisibleSectionId) {
        setActiveMainSettingsSectionId((currentSectionId) =>
          currentSectionId === mostlyVisibleSectionId ? currentSectionId : mostlyVisibleSectionId
        );
      }
    });
  };
  const handleSettingsModalScrollCapture = (event: ReactUIEvent<HTMLDivElement>) => {
    if (event.target instanceof HTMLElement && event.target.dataset.slot === 'scroll-area-viewport') {
      rememberSettingsModalScrollTop(activeTab, event.target.scrollTop);
      scheduleSettingsModalNavigationPersist(activeTab);
      if (activeTab === 'settings') {
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
    if (visibleTab === 'settings' || visibleTab === 'hotkeys') {
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
      (pendingSettingsRef.current ?? draft).settingsModalNavigation
    );
    rememberActiveScrollPosition();
    rememberSettingsModalTab(nextTab);
    persistSettingsModalNavigation(nextTab);
    setActiveTabState(nextTab);
  }, [initialTab, isOpen]);

  useEffect(() => {
    if (activeTab !== 'osIntegration' || showOSIntegrationSettingsTab) {
      return;
    }
    rememberSettingsModalTab('settings');
    setActiveTabState('settings');
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
      activeTab !== 'osIntegration' ||
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
          (pendingSettingsRef.current ?? draft).settingsModalNavigation
        );
      }
      focusSearchInput();
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [activeTab, isFirstLaunchSetup, isOpen]);

  useEffect(() => {
    if (!isOpen || activeTab !== 'agents' || agentHookStatus || agentHookStatusLoading) {
      return;
    }
    onRequestAgentHookStatus?.();
  }, [activeTab, agentHookStatus, agentHookStatusLoading, isOpen, onRequestAgentHookStatus]);

  useEffect(() => {
    if (!isOpen || activeTab !== 'integrations') {
      return;
    }
    /**
     * CDXC:CuaDriverPlugins 2026-08-09:
     * Integrations owns CLI, skill, Trycua lifecycle, and macOS permission
     * state. Probe only while that page is active.
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

  const settingsSearch = getSettingsSearchSections(settingsSearchQuery, draft);
  const mainSettingsGroupSearch = getMainSettingsGroupSearch(settingsSearchQuery, settingsSearch);
  const mainSettingsSectionNavigation = getMainSettingsSectionNavigation(mainSettingsGroupSearch);
  const { debuggingSettingVisible, mainSectionVisible, mainSettingVisible, mainSubsectionVisible } =
    createMainSettingsVisibility({
      appIconPickerUnavailable,
      draft,
      firstLaunchSetupVisibleSettings,
      isFirstLaunchSetup,
      mainSettingsGroupSearch,
      settingsSearch,
      settingsSearchQuery,
      showAdvancedSettings,
    });
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
    privacy: privacySectionRef,
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
  const {
    activeMainSettingsGroupId,
    getMainSettingsSectionMeasurementItems,
    hasVisibleMainSettings,
    scrollMainSettingsSectionIntoView,
    visibleMainSettingsSectionIds,
    visibleMainSettingsSectionNavigation,
  } = createVisibleMainSettingsNavigation({
    activeMainSettingsSectionId,
    isFirstLaunchSetup,
    mainSectionVisible,
    mainSettingsSectionNavigation,
    mainSettingsSectionRefs,
    mainSubsectionVisible,
    settingsSearch,
  });
  const {
    extraSettingsTabSearches,
    hotkeyDefinitionsById,
    hotkeySectionRefs,
    hotkeySectionSearches,
    isSettingsSearching,
    scrollHotkeySettingsSectionIntoView,
    visibleHotkeySectionNavigation,
    visibleHotkeySections,
  } = useHotkeySettings({
    draft,
    hotkeyActionsSectionRef,
    hotkeyGeneralSectionRef,
    hotkeyNavigationSectionRef,
    hotkeyPaneActionsSectionRef,
    hotkeyProjectsSectionRef,
    hotkeySessionSlotsSectionRef,
    isFirstLaunchSetup,
    settingsSearchQuery,
  });
  const { settingsSearchMatchingPages, settingsSidebarPages } = createSettingsSidebarPages({
    activeHotkeySettingsSectionId,
    activeMainSettingsGroupId,
    activeMainSettingsSectionId,
    activeTab,
    extraSettingsTabSearches,
    hasVisibleMainSettings,
    isSettingsSearching,
    scrollHotkeySettingsSectionIntoView,
    scrollMainSettingsSectionIntoView,
    setActiveHotkeySettingsSectionId,
    setActiveMainSettingsSectionId,
    setActiveTab,
    settingsSearchQuery,
    showOSIntegrationSettingsTab,
    visibleHotkeySectionNavigation,
    visibleHotkeySections,
    visibleMainSettingsSectionNavigation,
  });

  useSettingsModalEffects({
    activeTab,
    agentsOnboardingSectionRef,
    appIconPickerUnavailable,
    appIconSectionRef,
    autoSleepSectionRef,
    betaSectionRef,
    browserSectionRef,
    chatSectionRef,
    debuggingSectionRef,
    dialogContentRef,
    editorSectionRef,
    getMainSettingsSectionMeasurementItems,
    ghostexFolderStats,
    ghostexFolderStatsLoading,
    ghosttyBehaviorSectionRef,
    ghosttyScrollingSectionRef,
    ghosttyTerminalSectionRef,
    hasRequestedAppIconsRef,
    hasRequestedStorageStatsRef,
    initialSection,
    isFirstLaunchSetup,
    isOpen,
    onRequestGhostexFolderStats,
    pendingNavigationPersistTimeoutRef,
    pendingTimeoutRef,
    powerSectionRef,
    privacySectionRef,
    sessionCardsSectionRef,
    setActiveMainSettingsSectionId,
    setActiveTabState,
    setDraft,
    settings,
    settingsSearchQuery,
    sidebarSectionRef,
    sidebarTagsSectionRef,
    soundsSectionRef,
    statusIndicatorsSectionRef,
    storageSectionRef,
    terminalDevServersSectionRef,
    themingSectionRef,
    visibleMainSettingsSectionIds,
    vscode,
  });

  const {
    applySettings,
    applySettingsPatch,
    closeSettingsModal,
    persistSettingsModalNavigation,
    scheduleSettingsModalNavigationPersist,
    updateDiagnosticLoggingScenario,
    updateDraft,
    updateDraftDebounced,
    updateShowAdvancedSettings,
  } = createSettingsPersistence({
    activeTab,
    draft,
    isFirstLaunchSetup,
    onChange,
    onClose,
    onPatch,
    pendingNavigationPersistTimeoutRef,
    pendingSettingsPatchRef,
    pendingSettingsRef,
    pendingTimeoutRef,
    rememberActiveScrollPosition,
    setDraft,
  });
  const { chooseAppIconFile, chooseTerminalBackgroundImageFile, nativeFilePickerAvailable, selectAppIcon } =
    useAppIconSettings({
      appIconPickerUnavailable,
      appIconState,
      draft,
      handledAppIconStateRef,
      isOpen,
      pendingAppIconSourceIdRef,
      pendingSettingsRef,
      setAppIconError,
      updateDraft,
      vscode,
    });
  const {
    activeSidebarSettingsPresetId,
    applyRecommendedGhosttySettings,
    getSettingModificationProps,
    resetGhosttySettingsToDefault,
    resetSettings,
    updateSidebarSettingsPreset,
  } = createSettingsActions({
    applySettings,
    applySettingsPatch,
    draft,
    onGhosttySettingsAction,
    pendingAppIconSourceIdRef,
    pendingSettingsRef,
    setAppIconError,
    vscode,
  });

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
          'ghostex-settings-shadcn settings-modal-dialog flex flex-col gap-0 overflow-hidden p-0 font-sans',
          isModalDarkTheme && 'dark'
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
            className='flex min-h-0 flex-1 flex-col'
            onValueChange={(value) => setActiveTab(value as SettingsModalTab)}
            orientation='vertical'
            value={activeTab}
          >
            <DialogHeader className='ghostex-modal-heading-bar'>
              {/*
               * CDXC:SettingsWindow 2026-06-25-17:05:
               * Native Settings windows already show "Ghostex Settings" in the
               * AppKit titlebar. Do not duplicate a visible "Settings" heading in
               * React; keep a hidden DialogTitle so the dialog remains named for
               * accessibility while first-launch setup keeps its visible title.
               */}
              <div className={cn('settings-modal-title-row', !isFirstLaunchSetup && 'sr-only')}>
                <DialogTitle className='ghostex-modal-heading-title'>
                  {isFirstLaunchSetup ? 'Get started' : 'Ghostex Settings'}
                </DialogTitle>
              </div>
              {isFirstLaunchSetup ? (
                <p className='mt-2 text-sm text-muted-foreground'>
                  Choose a few defaults for Ghostex. You can change everything later in Settings.
                </p>
              ) : null}
            </DialogHeader>

            <div
              className={cn(
                'settings-modal-body-layout',
                isFirstLaunchSetup && 'settings-modal-body-layout-first-launch'
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
              <div className='settings-modal-main-column'>
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
                  <div className='settings-modal-search-row'>
                    <SidebarSessionSearchField
                      ariaLabel='Search settings'
                      autoCapitalize='none'
                      autoComplete='off'
                      autoCorrect='off'
                      clearLabel='Clear settings search'
                      inputClassName='settings-modal-search-input'
                      inputRef={searchInputRef}
                      placeholder='Search settings'
                      query={settingsSearchQuery}
                      setQuery={setSettingsSearchQuery}
                      shouldFocusOnQueryChange={shouldFocusSettingsSearchInput}
                      spellCheck={false}
                      toolbarClassName='settings-modal-search-toolbar'
                    />
                  </div>
                ) : null}

                <TabsContent
                  className='settings-main-tabs-content mt-0 min-h-0 flex-1 overflow-hidden'
                  value='settings'
                >
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
                  <div className='settings-main-tab-layout'>
                    <SettingsNativeScrollArea className='settings-main-scroll h-full min-h-0'>
                      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
                        {isFirstLaunchSetup && mainSectionVisible('agents', settingsSearch.sidebar) ? (
                          <SettingsSection sectionRef={agentsOnboardingSectionRef} title='Agents'>
                            {mainSettingVisible(settingsSearch.sidebar, 'agentAcceptAllEnabled') ? (
                              <ToggleField
                                checked={draft.agentAcceptAllEnabled}
                                description="Enable each supported agent's permission-bypass mode when launching sessions. Per-agent overrides live in Settings → Agents."
                                label='Accept All'
                                {...getSettingModificationProps('agentAcceptAllEnabled')}
                                onChange={(checked) => updateDraft('agentAcceptAllEnabled', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}
                        {/*
                         * CDXC:SettingsNavigation 2026-08-24:
                         * Keep every General sidebar group's child sections
                         * contiguous and in MAIN_SETTINGS_SUBSECTION_NAVIGATION
                         * order. Interleaving groups makes adjacent rail links
                         * jump across unrelated content and breaks the visible
                         * hierarchy promised by the navigation.
                         */}
                        {mainSubsectionVisible('sidebar', settingsSearch.sidebar) ? (
                          <SettingsSection sectionRef={sidebarSectionRef} title='Sidebar'>
                            {/*
                             * CDXC:SidebarV2 2026-07-29:
                             * Sidebar version stays near the top of the General tab so the
                             * opt-in Inbox sidebar is discoverable without scrolling. Its
                             * Group by Project sub-mode only appears while V2 is selected,
                             * because the classic sidebar has no such layout.
                             */}
                            {mainSettingVisible(settingsSearch.sidebar, 'sidebarVersion') ? (
                              <SidebarVersionField
                                description="Classic keeps today's project-grouped sidebar. Inbox (V2) is a flat, position-stable list of sessions across all projects."
                                label='Sidebar version'
                                {...getSettingModificationProps('sidebarVersion')}
                                onChange={(sidebarVersion) => updateDraft('sidebarVersion', sidebarVersion)}
                                value={draft.sidebarVersion}
                              />
                            ) : null}
                            {draft.sidebarVersion === 'v2' &&
                            mainSettingVisible(settingsSearch.sidebar, 'sidebarV2Layout') ? (
                              <ToggleField
                                checked={draft.sidebarV2Layout === 'byProject'}
                                description='Group Inbox sidebar sessions into collapsible project groups instead of one flat list.'
                                label='Group by project'
                                subtitle='Inbox sidebar only.'
                                {...getSettingModificationProps('sidebarV2Layout')}
                                onChange={(checked) => updateDraft('sidebarV2Layout', checked ? 'byProject' : 'flat')}
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
                            {draft.sidebarVersion === 'v2' &&
                            mainSettingVisible(settingsSearch.sidebar, 'sidebarAutoSettleAfterDays') ? (
                              <SelectField
                                description='Move sessions with no meaningful activity to the Settled shelf. Working and blocked sessions never settle automatically.'
                                label='Auto-settle inactive sessions'
                                {...getSettingModificationProps('sidebarAutoSettleAfterDays')}
                                onChange={(value) =>
                                  updateDraft(
                                    'sidebarAutoSettleAfterDays',
                                    parseSidebarAutoSettleAfterDaysSelectValue(value)
                                  )
                                }
                                options={SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS}
                                value={sidebarAutoSettleAfterDaysSelectValue(draft.sidebarAutoSettleAfterDays)}
                              />
                            ) : null}
                            {/* CDXC:SidebarSettingsPresets 2026-06-12-07:10: Preset is the first Sidebar setting so users can apply Codex, Minimal, Detailed, or Recommended sidebar UI defaults before tuning individual controlled settings. */}
                            {mainSettingVisible(settingsSearch.sidebar, 'sidebarSettingsPreset') ? (
                              <SidebarPresetField
                                activePresetId={activeSidebarSettingsPresetId}
                                description='Apply a sidebar UI preset.'
                                isModified={activeSidebarSettingsPresetId !== 'recommended'}
                                label='Preset'
                                onChange={updateSidebarSettingsPreset}
                                onResetToDefault={() => updateSidebarSettingsPreset('recommended')}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'sidebarProjectGroupStyle') ? (
                              <SidebarProjectGroupStyleField
                                description='Choose how project groups are marked without adding group or project card borders.'
                                label='Project group style'
                                {...getSettingModificationProps('sidebarProjectGroupStyle')}
                                onChange={(value) => updateDraft('sidebarProjectGroupStyle', value)}
                                value={draft.sidebarProjectGroupStyle}
                              />
                            ) : null}
                            {/*
                             * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
                             * Users need every preset-mutated setting directly under the preset selector so applying Recommended, Codex, Minimal, or Detailed has an inspectable effect without hunting through Session Cards, Project rows, or Status Indicators.
                             */}
                            {mainSettingVisible(settingsSearch.sidebar, 'showProjectIcons') ? (
                              <ToggleField
                                checked={draft.showProjectIcons}
                                description='Show project artwork or a folder or worktree icon beside project names.'
                                label='Show project icons'
                                {...getSettingModificationProps('showProjectIcons')}
                                onChange={(checked) => updateDraft('showProjectIcons', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'hideSessionAgentIconUntilHover') ? (
                              <ToggleField
                                checked={draft.hideSessionAgentIconUntilHover}
                                description='Hide session agent icons until a session row is hovered.'
                                label='Hide agent icon until hover'
                                {...getSettingModificationProps('hideSessionAgentIconUntilHover')}
                                onChange={(checked) => updateDraft('hideSessionAgentIconUntilHover', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'hideBrowserFaviconUntilHover') ? (
                              <ToggleField
                                checked={draft.hideBrowserFaviconUntilHover}
                                description='Hide browser page favicons until a session row is hovered.'
                                label='Hide browser favicon until hover'
                                {...getSettingModificationProps('hideBrowserFaviconUntilHover')}
                                onChange={(checked) => updateDraft('hideBrowserFaviconUntilHover', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'showCloseButtonOnSessionCards') ? (
                              <ToggleField
                                checked={draft.showCloseButtonOnSessionCards}
                                description='Reveal the close control when hovering a card.'
                                label='Show close button on hover'
                                {...getSettingModificationProps('showCloseButtonOnSessionCards')}
                                onChange={(checked) => updateDraft('showCloseButtonOnSessionCards', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'hideLastActiveTimeOnSessionCards') ? (
                              <ToggleField
                                checked={draft.hideLastActiveTimeOnSessionCards}
                                description='Hide Last Active timestamps from session-card title rows.'
                                label='Hide last active time'
                                {...getSettingModificationProps('hideLastActiveTimeOnSessionCards')}
                                onChange={(checked) => updateDraft('hideLastActiveTimeOnSessionCards', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'hideProjectHeaderDiffStats') ? (
                              <ToggleField
                                checked={draft.hideProjectHeaderDiffStats}
                                description='Hide +added/-removed line counts in sidebar project rows.'
                                label='Hide project git stats'
                                {...getSettingModificationProps('hideProjectHeaderDiffStats')}
                                onChange={(checked) => updateDraft('hideProjectHeaderDiffStats', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'showProjectEditorDiffFileCount') ? (
                              <ToggleField
                                checked={draft.showProjectEditorDiffFileCount}
                                description='Show changed-file counts in sidebar project row git stats.'
                                label='Show changed-file count'
                                {...getSettingModificationProps('showProjectEditorDiffFileCount')}
                                onChange={(checked) => updateDraft('showProjectEditorDiffFileCount', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'hideMenuBarSessionStatusIndicators') ? (
                              <ToggleField
                                checked={!draft.hideMenuBarSessionStatusIndicators}
                                description='Show the menu bar session status badges.'
                                label='Show Menu Bar Session Indicators'
                                {...getSettingModificationProps('hideMenuBarSessionStatusIndicators')}
                                onChange={(checked) => updateDraft('hideMenuBarSessionStatusIndicators', !checked)}
                              />
                            ) : null}
                            {/* CDXC:SidebarPlacement 2026-05-06-17:32: Sidebar side remains
                  near the top of Sidebar settings so users can move the
                  sidebar to the right side without discovering the hotkey. */}
                            {mainSettingVisible(settingsSearch.sidebar, 'sidebarSide') ? (
                              <SelectField
                                description='Choose which side of the screen holds the sidebar.'
                                label='Side'
                                {...getSettingModificationProps('sidebarSide')}
                                onChange={(value) => updateDraft('sidebarSide', value as SidebarSide)}
                                options={SIDEBAR_SIDE_OPTIONS}
                                value={draft.sidebarSide}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'sidebarDefaultWidthPx') ? (
                              <>
                                {/*
                                 * CDXC:SidebarChrome 2026-06-05-04:40:
                                 * This setting changes only the explicit double-click reset target for the sidebar resize handle. App restart must keep restoring the last persisted sidebar width from native/Electron chrome state.
                                 */}
                                <SliderNumberField
                                  description='Used when double-clicking the sidebar resize handle. App restart still restores your last manually set sidebar width.'
                                  label='Default Width'
                                  {...getSettingModificationProps('sidebarDefaultWidthPx')}
                                  max={MAX_SIDEBAR_DEFAULT_WIDTH_PX}
                                  min={MIN_SIDEBAR_DEFAULT_WIDTH_PX}
                                  onCommit={(value) => updateDraft('sidebarDefaultWidthPx', value)}
                                  onChange={(value) => updateDraftDebounced('sidebarDefaultWidthPx', value)}
                                  step={1}
                                  value={draft.sidebarDefaultWidthPx}
                                />
                              </>
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'sidebarCollapseAnimationDurationMs') ? (
                              <SliderNumberField
                                description='Duration in milliseconds for expanding and collapsing sidebar sections, groups, and projects. Set to 0 for instant changes.'
                                label='Collapse Animation Duration'
                                {...getSettingModificationProps('sidebarCollapseAnimationDurationMs')}
                                max={MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS}
                                min={MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS}
                                onCommit={(value) => updateDraft('sidebarCollapseAnimationDurationMs', value)}
                                onChange={(value) => updateDraftDebounced('sidebarCollapseAnimationDurationMs', value)}
                                step={SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS}
                                value={draft.sidebarCollapseAnimationDurationMs}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'commandsPanelDefaultHeightPx') ? (
                              <SliderNumberField
                                description='Used when opening the command pane (F12 or sidebar) and when double-clicking its top resize rail.'
                                label='Command Pane Default Height'
                                {...getSettingModificationProps('commandsPanelDefaultHeightPx')}
                                max={MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX}
                                min={MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX}
                                onCommit={(value) => updateDraft('commandsPanelDefaultHeightPx', value)}
                                onChange={(value) => updateDraftDebounced('commandsPanelDefaultHeightPx', value)}
                                step={1}
                                value={draft.commandsPanelDefaultHeightPx}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'commandsPanelSide') ? (
                              <SelectField
                                description='Where terminal Actions and F12 open the command pane: below the workspace or as a column to its right.'
                                label='Command Pane Side'
                                {...getSettingModificationProps('commandsPanelSide')}
                                onChange={(value) => updateDraft('commandsPanelSide', value as CommandsPanelSide)}
                                options={COMMANDS_PANEL_SIDE_OPTIONS}
                                value={draft.commandsPanelSide}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'projectSessionListCollapsedCount') ? (
                              <>
                                {/*
                                 * CDXC:ProjectSessionLists 2026-06-10-13:39:
                                 * The project-header Show less button should preserve the old six-row default while letting users raise the collapsed project-session count, such as ten rows, without changing the per-project Show more / Show less state model.
                                 */}
                                <SliderNumberField
                                  description='Project sessions kept visible after Show less.'
                                  label='Show Less Count'
                                  {...getSettingModificationProps('projectSessionListCollapsedCount')}
                                  max={MAX_PROJECT_SESSION_LIST_COLLAPSED_COUNT}
                                  min={MIN_PROJECT_SESSION_LIST_COLLAPSED_COUNT}
                                  onCommit={(value) => updateDraft('projectSessionListCollapsedCount', value)}
                                  onChange={(value) => updateDraftDebounced('projectSessionListCollapsedCount', value)}
                                  step={1}
                                  value={draft.projectSessionListCollapsedCount}
                                />
                              </>
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'agentManagerZoomPercent') ? (
                              /*
                               * CDXC:SidebarInterface 2026-06-16-18:19:
                               * Keep the persisted agentManagerZoomPercent key for compatibility, but label the Settings control as Sidebar Interface Size because it changes the visible sidebar interface scale.
                               */
                              <SliderNumberField
                                description='Scale the sidebar interface.'
                                label='Sidebar Interface Size'
                                {...getSettingModificationProps('agentManagerZoomPercent')}
                                max={200}
                                min={50}
                                onCommit={(value) => updateDraft('agentManagerZoomPercent', value)}
                                onChange={(value) => updateDraftDebounced('agentManagerZoomPercent', value)}
                                step={1}
                                value={draft.agentManagerZoomPercent}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'createSessionOnSidebarDoubleClick') ? (
                              /*
                               * CDXC:SidebarSessions 2026-08-26:
                               * Creating sessions from empty space and renaming sessions from
                               * cards are both low-frequency double-click preferences, so both
                               * live behind Show Advanced.
                               */
                              <ToggleField
                                checked={draft.createSessionOnSidebarDoubleClick}
                                description='Create a session from empty sidebar space.'
                                label='Double-click empty sidebar space to create a session'
                                {...getSettingModificationProps('createSessionOnSidebarDoubleClick')}
                                onChange={(checked) => updateDraft('createSessionOnSidebarDoubleClick', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'enableSessionParking') ? (
                              <ToggleField
                                checked={draft.enableSessionParking}
                                description='Add Park to session menus and move parked sessions into a collapsible section at the bottom of the sidebar.'
                                label='Enable session parking'
                                {...getSettingModificationProps('enableSessionParking')}
                                onChange={(checked) => updateDraft('enableSessionParking', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sidebar, 'renameSessionOnDoubleClick') ? (
                              <ToggleField
                                checked={draft.renameSessionOnDoubleClick}
                                description='Rename sessions directly from their cards.'
                                label={RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL}
                                subtitle={RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE}
                                {...getSettingModificationProps('renameSessionOnDoubleClick')}
                                onChange={(checked) => updateDraft('renameSessionOnDoubleClick', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('sessionCards', settingsSearch.sessionCards) ? (
                          <SettingsSection sectionRef={sessionCardsSectionRef} title='Session Cards'>
                            {/* CDXC:SidebarSessionAgentIcons 2026-06-29-23:58: Users need a Session Cards toggle for colored agent brand artwork while the default sidebar remains monochrome and favorite rows no longer gold-tint agent logos. CDXC:SidebarSessionAgentIcons 2026-06-30-22:40: The colored agent icon setting must also color the selected-agent launcher icon so the Mac sidebar picker and session cards use the same agent identity mode. */}
                            {mainSettingVisible(settingsSearch.sessionCards, 'useColoredSessionAgentIcons') ? (
                              <ToggleField
                                checked={draft.useColoredSessionAgentIcons}
                                description='Render session and selected-agent logos with colored brand artwork instead of monochrome masks.'
                                label='Use colored agent icons'
                                {...getSettingModificationProps('useColoredSessionAgentIcons')}
                                onChange={(checked) => updateDraft('useColoredSessionAgentIcons', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sessionCards, 'showSessionCloseContextMenuAction') ? (
                              <>
                                {/*
                                 * CDXC:SidebarContextMenu 2026-06-10-13:58:
                                 * Session context menus should hide the destructive Close item by default. Place this opt-in directly above the command-copy opt-in because both settings reveal advanced context-menu actions.
                                 */}
                                <ToggleField
                                  checked={draft.showSessionCloseContextMenuAction}
                                  description='Show the Close item in session context menus.'
                                  label='Show Close option in context menu'
                                  {...getSettingModificationProps('showSessionCloseContextMenuAction')}
                                  onChange={(checked) => updateDraft('showSessionCloseContextMenuAction', checked)}
                                />
                              </>
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('sidebarTags', settingsSearch.sidebarTags) ? (
                          <SettingsSection sectionRef={sidebarTagsSectionRef} title='Sidebar Tags'>
                            {mainSettingVisible(settingsSearch.sidebarTags, 'sidebarSessionTagListItems') ? (
                              <SidebarTagListSettingsField
                                isModified={
                                  !areSidebarSessionTagListItemsEqual(
                                    draft.sidebarSessionTagListItems,
                                    DEFAULT_ghostex_SETTINGS.sidebarSessionTagListItems
                                  )
                                }
                                items={draft.sidebarSessionTagListItems}
                                onChange={(items) => updateDraft('sidebarSessionTagListItems', items)}
                                onResetToDefault={() =>
                                  updateDraft(
                                    'sidebarSessionTagListItems',
                                    DEFAULT_ghostex_SETTINGS.sidebarSessionTagListItems
                                  )
                                }
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('theming', settingsSearch.theming) ? (
                          <SettingsSection sectionRef={themingSectionRef} title='Theming'>
                            {/*
                  CDXC:SettingsTheming 2026-06-15-21:35:
                  General settings needs Theming in the second group, separate
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
                            {mainSettingVisible(settingsSearch.theming, 'sidebarTheme') ? (
                              <StaticNoteField label='Theme' surface='plain' value='Light theme coming soon' />
                            ) : null}
                            {mainSettingVisible(
                              settingsSearch.theming,
                              'customSidebarTitlebarBackgroundDarknessPercent'
                            ) ? (
                              <SliderNumberField
                                description='85 is softer gray; 100 is black. Text and icons adjust automatically.'
                                label='Background Contrast'
                                {...getSettingModificationProps('customSidebarTitlebarBackgroundDarknessPercent')}
                                max={MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT}
                                min={MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT}
                                onCommit={(value) =>
                                  updateDraft('customSidebarTitlebarBackgroundDarknessPercent', value)
                                }
                                onChange={(value) =>
                                  updateDraftDebounced('customSidebarTitlebarBackgroundDarknessPercent', value)
                                }
                                step={1}
                                value={draft.customSidebarTitlebarBackgroundDarknessPercent}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.theming, 'customSidebarTitlebarBackgroundTintColor') ? (
                              <WebColorPickerField
                                description='Applies a subtle hue to the sidebar and titlebar background.'
                                label='Background Tint'
                                {...getSettingModificationProps('customSidebarTitlebarBackgroundTintColor')}
                                onChange={(value) =>
                                  updateDraftDebounced('customSidebarTitlebarBackgroundTintColor', value)
                                }
                                onCommit={(value) => updateDraft('customSidebarTitlebarBackgroundTintColor', value)}
                                value={draft.customSidebarTitlebarBackgroundTintColor}
                              />
                            ) : null}
                            {/*
                  CDXC:AccentColor 2026-08-24:
                  The accent color drives --ghostex-accent on every React
                  surface, so it uses the same web color picker as Background
                  Tint instead of a native input[type=color].
                */}
                            {mainSettingVisible(settingsSearch.theming, 'accentColor') ? (
                              <WebColorPickerField
                                description='Highlight color for accent text and status highlights.'
                                label='Accent Color'
                                {...getSettingModificationProps('accentColor')}
                                onChange={(value) => updateDraftDebounced('accentColor', value)}
                                onCommit={(value) => updateDraft('accentColor', value)}
                                value={draft.accentColor}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.theming, 'workspaceActivePaneBorderColor') ? (
                              <TextField
                                description='CSS color for the focused pane border.'
                                label='Active Pane Border'
                                {...getSettingModificationProps('workspaceActivePaneBorderColor')}
                                onChange={(value) => updateDraft('workspaceActivePaneBorderColor', value)}
                                value={draft.workspaceActivePaneBorderColor}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {/*
                         * CDXC:AppIconPicker 2026-06-28-06:05:
                         * The advanced App Icon section is a custom-image control, not a bundled preset picker. Show one preview, one Select Image action, and an inline X on the custom preview to restore the default icon; omit separate reset and folder-reveal actions so the flow stays direct.
                         */}
                        {mainSubsectionVisible('appIcon', settingsSearch.appIcon) ? (
                          <SettingsSection
                            description='Changes the Dock and app-switcher icon. The app file icon may also change when macOS allows it.'
                            sectionRef={appIconSectionRef}
                            title='App Icon'
                          >
                            {mainSettingVisible(settingsSearch.appIcon, 'appIconSourceId') ? (
                              <AppIconPickerField
                                advanced={isAdvancedMainSetting('appIconSourceId')}
                                error={appIconError}
                                onChooseFile={chooseAppIconFile}
                                onSelect={selectAppIcon}
                                state={appIconState}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSectionVisible('chat', settingsSearch.chat) ? (
                          <SettingsSection sectionRef={chatSectionRef} title='Chat'>
                            {mainSettingVisible(settingsSearch.chat, 'preferredAgentInterface') ? (
                              <PreferredAgentInterfaceField
                                description='Chat switches on automatically as soon as Ghostex detects a compatible agent. The terminal stays live in the background, and you can switch back at any time. Settings > Agents can override this for one agent at a time.'
                                label='Default Agent View'
                                {...getSettingModificationProps('preferredAgentInterface')}
                                onChange={(preferredAgentInterface) =>
                                  updateDraft('preferredAgentInterface', preferredAgentInterface)
                                }
                                value={draft.preferredAgentInterface}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.chat, 'sessionChatTheme') ? (
                              <SessionChatThemeField
                                description='Changes chat content only; the surrounding Ghostex app remains dark.'
                                label='Appearance'
                                {...getSettingModificationProps('sessionChatTheme')}
                                onChange={(value) => updateDraft('sessionChatTheme', value)}
                                value={draft.sessionChatTheme}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.chat, 'sessionChatFontFamily') ? (
                              <TextField
                                description='Type an installed font family name. Leave blank to use the app font.'
                                label='Font Family'
                                {...getSettingModificationProps('sessionChatFontFamily')}
                                onChange={(value) => updateDraft('sessionChatFontFamily', value)}
                                placeholder='App default'
                                value={draft.sessionChatFontFamily}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.chat, 'sessionChatTranscriptWidthPercent') ? (
                              <SliderNumberField
                                description='Adjust message width only. The prompt composer at the bottom keeps its current width.'
                                label='Message Width (%)'
                                {...getSettingModificationProps('sessionChatTranscriptWidthPercent')}
                                max={MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT}
                                min={MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT}
                                onCommit={(value) => updateDraft('sessionChatTranscriptWidthPercent', value)}
                                onChange={(value) => updateDraftDebounced('sessionChatTranscriptWidthPercent', value)}
                                step={SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP}
                                value={draft.sessionChatTranscriptWidthPercent}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.chat, 'sessionChatVerboseMode') ? (
                              <ToggleField
                                checked={draft.sessionChatVerboseMode}
                                description="Expand thinking blocks to show their tool calls by default. Individual command and output details remain collapsible. This is the default for new chats; the Verbose pill in a chat's composer overrides it for that chat only."
                                label='Verbose Mode'
                                {...getSettingModificationProps('sessionChatVerboseMode')}
                                onChange={(checked) => updateDraft('sessionChatVerboseMode', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {PET_CONTROLS_VISIBLE &&
                        mainSectionVisible('statusIndicators', settingsSearch.statusIndicators) ? (
                          <SettingsSection sectionRef={statusIndicatorsSectionRef} title='Status Indicators'>
                            {mainSettingVisible(settingsSearch.statusIndicators, 'petOverlayEnabled') ? (
                              <ToggleField
                                checked={draft.petOverlayEnabled}
                                description='Show a draggable floating animated pet.'
                                label='Wake Pet'
                                {...getSettingModificationProps('petOverlayEnabled')}
                                onChange={(checked) => updateDraft('petOverlayEnabled', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.statusIndicators, 'selectedPetId') ? (
                              <PetPickerField
                                {...getSettingModificationProps('selectedPetId')}
                                onChange={(value) => updateDraft('selectedPetId', value)}
                                value={draft.selectedPetId}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('browser', settingsSearch.browser) ? (
                          <SettingsSection sectionRef={browserSectionRef} title='Browser'>
                            {/* CDXC:BrowserPanes 2026-05-27-07:24: Settings no longer exposes Chrome Canary attachment. Browser actions always open in workspace browser panes, leaving this section focused on pane behavior controls. */}
                            {mainSettingVisible(settingsSearch.browser, 'webLinkOpenTarget') ? (
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
                                description='Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.'
                                label='Open links in'
                                {...getSettingModificationProps('webLinkOpenTarget')}
                                onChange={(value) => updateDraft('webLinkOpenTarget', value as WebLinkOpenTarget)}
                                options={WEB_LINK_OPEN_TARGET_OPTIONS}
                                value={draft.webLinkOpenTarget}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('terminalDevServers', settingsSearch.terminalDevServers) ? (
                          <SettingsSection
                            description='Choose how Ghostex discovers running dev servers and which ports stay hidden. Detected URLs follow Browser → Open links in.'
                            sectionRef={terminalDevServersSectionRef}
                            title='Dev Servers'
                          >
                            {/*
                             * CDXC:TerminalDevServers 2026-06-23-19:22:
                             * Dev-server settings are terminal-adjacent app behavior. Keep detection, one launch destination, and ignored port rules together so users can tune server discovery without editing terminal emulator config or managing individual browser targets.
                             */}
                            {mainSettingVisible(
                              settingsSearch.terminalDevServers,
                              'terminalDevServerDetectionEnabled'
                            ) ? (
                              <ToggleField
                                checked={draft.terminalDevServerDetectionEnabled}
                                description='Detect localhost dev server URLs from terminal output.'
                                label='Detect running servers in terminals'
                                {...getSettingModificationProps('terminalDevServerDetectionEnabled')}
                                onChange={(checked) => updateDraft('terminalDevServerDetectionEnabled', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(
                              settingsSearch.terminalDevServers,
                              'terminalDevServerIgnoredPortRules'
                            ) ? (
                              <TerminalDevServerIgnoredPortsField
                                ignoredPortRules={draft.terminalDevServerIgnoredPortRules}
                                {...getSettingModificationProps('terminalDevServerIgnoredPortRules')}
                                onChange={(ignoredPortRules) =>
                                  updateDraft('terminalDevServerIgnoredPortRules', ignoredPortRules)
                                }
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('editor', settingsSearch.editor) ? (
                          <SettingsSection sectionRef={editorSectionRef} title='Editor'>
                            {/* CDXC:EditorPanes 2026-06-08-20:12: Embedded code-server panes
                  use Ghostex-owned bundled editor settings by default so the
                  macOS VS Code surface starts on Dark 2026. This toggle opts
                  into linking local VS Code settings, while the Insiders
                  checkbox only changes the linked config directory. */}
                            {mainSettingVisible(settingsSearch.editor, 'codeServerLinkVscodeUserConfig') ? (
                              <ToggleField
                                advanced={isAdvancedMainSetting('codeServerLinkVscodeUserConfig')}
                                checked={draft.codeServerLinkVscodeUserConfig}
                                description='Use local VS Code settings instead of the bundled editor defaults.'
                                label='Use VS Code settings'
                                onChange={(checked) => updateDraft('codeServerLinkVscodeUserConfig', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.editor, 'codeServerUseVscodeInsidersUserConfig') ? (
                              <ToggleField
                                advanced={isAdvancedMainSetting('codeServerUseVscodeInsidersUserConfig')}
                                checked={draft.codeServerUseVscodeInsidersUserConfig}
                                description='Use the VS Code Insiders user settings directory.'
                                disabled={!draft.codeServerLinkVscodeUserConfig}
                                disabledReason='Turn on “Link VS Code user settings” first.'
                                label='Use VS Code Insiders settings'
                                onChange={(checked) => updateDraft('codeServerUseVscodeInsidersUserConfig', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(
                              settingsSearch.editor,
                              'showUntrackedProjectDiffWhenNoTrackedChanges'
                            ) ? (
                              <ToggleField
                                checked={draft.showUntrackedProjectDiffWhenNoTrackedChanges}
                                description='When tracked git diff is +0 -0, show untracked line counts in project headers.'
                                label='Show untracked lines without tracked changes'
                                {...getSettingModificationProps('showUntrackedProjectDiffWhenNoTrackedChanges')}
                                onChange={(checked) =>
                                  updateDraft('showUntrackedProjectDiffWhenNoTrackedChanges', checked)
                                }
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('terminal', settingsSearch.terminal) ? (
                          <SettingsSection sectionRef={ghosttyTerminalSectionRef} title='Terminal'>
                            {/* CDXC:TerminalSettings 2026-04-26-18:36: Terminal settings in
                  ghostex edit the shared Ghostty config file, so users must see
                  that external Ghostty windows receive the same values and can
                  reload them with Ghostty's normal config shortcut.

                  CDXC:SettingsNavigation 2026-06-12-04:13:
                  Ghostty terminal controls live in the main Settings page so
                  the Settings search box finds app and terminal controls in one
                  pass. */}
                            {mainSettingVisible(settingsSearch.terminal, 'ghosttySettingsActions') ? (
                              <>
                                {/* CDXC:TerminalSettings 2026-06-23-05:48:
                      The shared-config notice is informational, not a warning, so
                      it uses the neutral Info box pattern (muted border/background
                      plus an info icon) instead of any colored alert tint, matching
                      the IconInfoCircle info boxes used elsewhere in Settings. */}
                                <div className='flex items-start gap-3 rounded-none border border-border bg-muted/20 px-4 py-3 text-sm leading-6 text-muted-foreground'>
                                  <IconInfoCircle
                                    aria-hidden='true'
                                    className='mt-0.5 size-4 shrink-0 text-foreground'
                                  />
                                  <p className='m-0'>
                                    Whatever you set here also applies to your external Ghostty terminal because this
                                    Ghostty terminal uses the same settings file. ghostex reloads its embedded Ghostty
                                    terminal about 3 seconds after you stop changing these controls; external Ghostty
                                    windows may still need Cmd+Shift+, to reload.
                                  </p>
                                </div>
                                <GhosttySettingsActions
                                  onApplyRecommended={applyRecommendedGhosttySettings}
                                  onOpenConfigFile={() => onGhosttySettingsAction?.('openGhosttyConfigFile')}
                                  onOpenDocs={() => onGhosttySettingsAction?.('openGhosttySettingsDocs')}
                                  onResetDefaults={resetGhosttySettingsToDefault}
                                />
                              </>
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalGhosttyTheme') ? (
                              <SelectField
                                contentClassName='max-h-80'
                                description='Choose a bundled Ghostty theme, or leave your existing Ghostty config in charge.'
                                label='Theme'
                                {...getSettingModificationProps('terminalGhosttyTheme')}
                                onChange={(value) =>
                                  updateDraft(
                                    'terminalGhosttyTheme',
                                    value === GHOSTTY_THEME_UNMANAGED_VALUE ? '' : value
                                  )
                                }
                                options={GHOSTTY_THEME_SETTING_OPTIONS}
                                showScrollButtons={false}
                                value={draft.terminalGhosttyTheme || GHOSTTY_THEME_UNMANAGED_VALUE}
                              />
                            ) : null}
                            {IS_WINDOWS_HOST &&
                            mainSettingVisible(settingsSearch.terminal, 'windowsTerminalBackend') ? (
                              <SelectField
                                description='Windows terminals currently use WSL2 with gxserver and zmx persistence. PowerShell mode will be added later.'
                                label='Windows terminal backend'
                                {...getSettingModificationProps('windowsTerminalBackend')}
                                onChange={() => updateDraft('windowsTerminalBackend', 'wsl')}
                                options={WINDOWS_TERMINAL_BACKEND_OPTIONS}
                                value={draft.windowsTerminalBackend}
                              />
                            ) : null}
                            {IS_WINDOWS_HOST &&
                            mainSettingVisible(settingsSearch.terminal, 'windowsWslDistribution') ? (
                              <TextField
                                description='Leave blank to use the default initialized WSL2 distribution. If discovery cannot find the intended install, enter its exact name as shown by `wsl.exe --list --verbose` (for example, Ubuntu-24.04). Ghostex never installs WSL automatically.'
                                label='WSL Distribution'
                                {...getSettingModificationProps('windowsWslDistribution')}
                                onChange={(value) => updateDraft('windowsWslDistribution', value)}
                                placeholder='Automatic'
                                value={draft.windowsWslDistribution}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'workspaceBackgroundColor') ? (
                              <ColorField
                                description='Color shown behind terminal panes.'
                                label='Terminal Background'
                                {...getSettingModificationProps('workspaceBackgroundColor')}
                                onChange={(value) => updateDraft('workspaceBackgroundColor', value)}
                                value={draft.workspaceBackgroundColor}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalBackgroundImage') ? (
                              <TextField
                                browseLabel='Choose image file'
                                description='Absolute path to an image drawn behind terminal panes. Leave blank for none.'
                                label='Background Image'
                                {...getSettingModificationProps('terminalBackgroundImage')}
                                onBrowse={nativeFilePickerAvailable ? chooseTerminalBackgroundImageFile : undefined}
                                onChange={(value) => updateDraft('terminalBackgroundImage', value)}
                                placeholder='/Users/you/Pictures/background.png'
                                value={draft.terminalBackgroundImage}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalBackgroundImageOpacity') ? (
                              <SliderNumberField
                                description='Blend the background image toward the terminal background color.'
                                label='Background Image Opacity'
                                {...getSettingModificationProps('terminalBackgroundImageOpacity')}
                                max={1}
                                min={0}
                                onCommit={(value) => updateDraft('terminalBackgroundImageOpacity', value)}
                                onChange={(value) => updateDraftDebounced('terminalBackgroundImageOpacity', value)}
                                step={0.05}
                                value={draft.terminalBackgroundImageOpacity}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalBackgroundImageFit') ? (
                              <SelectField
                                description='How the background image is scaled inside each pane.'
                                label='Background Image Fit'
                                {...getSettingModificationProps('terminalBackgroundImageFit')}
                                onChange={(value) =>
                                  updateDraft('terminalBackgroundImageFit', value as TerminalBackgroundImageFit)
                                }
                                options={[
                                  { label: 'Cover', value: 'cover' },
                                  { label: 'Contain', value: 'contain' },
                                  { label: 'Stretch', value: 'stretch' },
                                  { label: 'Natural size', value: 'natural' },
                                ]}
                                value={draft.terminalBackgroundImageFit}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalFontFamily') ? (
                              <TextField
                                description="Type a Ghostty font-family name. Leave blank to use existing Ghostty config or Ghostty's platform default."
                                label='Font Family'
                                {...getSettingModificationProps('terminalFontFamily')}
                                onChange={(value) => updateDraft('terminalFontFamily', value)}
                                placeholder='Ghostty default'
                                value={draft.terminalFontFamily}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalFontSize') ? (
                              <SliderNumberField
                                description='Set terminal text size.'
                                label='Font Size'
                                {...getSettingModificationProps('terminalFontSize')}
                                max={32}
                                min={8}
                                onCommit={(value) => updateDraft('terminalFontSize', value)}
                                onChange={(value) => updateDraftDebounced('terminalFontSize', value)}
                                step={0.5}
                                value={draft.terminalFontSize}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalFontWeight') ? (
                              <SliderNumberField
                                description='Set terminal text weight.'
                                label='Font Weight'
                                {...getSettingModificationProps('terminalFontWeight')}
                                max={900}
                                min={100}
                                onCommit={(value) => updateDraft('terminalFontWeight', value)}
                                onChange={(value) => updateDraftDebounced('terminalFontWeight', value)}
                                step={50}
                                value={draft.terminalFontWeight}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalLineHeight') ? (
                              <SliderNumberField
                                description='Adjust terminal row height.'
                                label='Line Height'
                                {...getSettingModificationProps('terminalLineHeight')}
                                max={2}
                                min={0.8}
                                onCommit={(value) => updateDraft('terminalLineHeight', value)}
                                onChange={(value) => updateDraftDebounced('terminalLineHeight', value)}
                                step={0.1}
                                value={draft.terminalLineHeight}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalLetterSpacing') ? (
                              <SliderNumberField
                                description='Adjust spacing between glyphs.'
                                label='Letter Spacing'
                                {...getSettingModificationProps('terminalLetterSpacing')}
                                max={8}
                                min={-2}
                                onCommit={(value) => updateDraft('terminalLetterSpacing', value)}
                                onChange={(value) => updateDraftDebounced('terminalLetterSpacing', value)}
                                step={0.1}
                                value={draft.terminalLetterSpacing}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalNarrowerViewEnabled') ? (
                              <ToggleField
                                checked={draft.terminalNarrowerViewEnabled}
                                description='Center and constrain terminal content on wide panes. Narrow panes stay full-width.'
                                label='Narrower Terminal View'
                                {...getSettingModificationProps('terminalNarrowerViewEnabled')}
                                onChange={(checked) => updateDraft('terminalNarrowerViewEnabled', checked)}
                              />
                            ) : null}
                            {draft.terminalNarrowerViewEnabled &&
                            mainSettingVisible(settingsSearch.terminal, 'terminalViewWidthPercent') ? (
                              <SliderNumberField
                                description='Set the centered terminal body width. Panes 1070px wide or narrower remain full-width.'
                                label='Terminal Width (%)'
                                {...getSettingModificationProps('terminalViewWidthPercent')}
                                max={MAX_TERMINAL_VIEW_WIDTH_PERCENT}
                                min={MIN_TERMINAL_VIEW_WIDTH_PERCENT}
                                onCommit={(value) => updateDraft('terminalViewWidthPercent', value)}
                                onChange={(value) => updateDraftDebounced('terminalViewWidthPercent', value)}
                                step={TERMINAL_VIEW_WIDTH_PERCENT_STEP}
                                value={draft.terminalViewWidthPercent}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalLayoutApplyToAllTerminals') ? (
                              <ToggleField
                                checked={draft.terminalLayoutApplyToAllTerminals}
                                description='Apply Terminal Width and Padding to command and editor companion terminals too. Turn off to limit them to agent-session Terminal View.'
                                label='Apply Layout to All Terminals'
                                {...getSettingModificationProps('terminalLayoutApplyToAllTerminals')}
                                onChange={(checked) => updateDraft('terminalLayoutApplyToAllTerminals', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalPaneHorizontalPaddingPx') ? (
                              /*
                               * CDXC:TerminalPanePadding 2026-06-25-21:27:
                               * Horizontal terminal padding is a native pane content inset,
                               * not spacing between split panes. Keep the slider integer-pixel
                               * based. The 16px default matches Chat's horizontal content inset.
                               */
                              <SliderNumberField
                                description='Add left and right inner padding inside the terminal content area.'
                                label='Horizontal Padding'
                                {...getSettingModificationProps('terminalPaneHorizontalPaddingPx')}
                                max={MAX_TERMINAL_PANE_PADDING_PX}
                                min={MIN_TERMINAL_PANE_PADDING_PX}
                                onCommit={(value) => updateDraft('terminalPaneHorizontalPaddingPx', value)}
                                onChange={(value) => updateDraftDebounced('terminalPaneHorizontalPaddingPx', value)}
                                step={1}
                                value={draft.terminalPaneHorizontalPaddingPx}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalPaneVerticalPaddingPx') ? (
                              /*
                               * CDXC:TerminalPanePadding 2026-06-25-21:27:
                               * Vertical terminal padding uses the same native content inset as
                               * horizontal padding while leaving pane titlebars, split dividers,
                               * and terminal chrome in their existing frames.
                               */
                              <SliderNumberField
                                description='Add top and bottom inner padding inside the terminal content area.'
                                label='Vertical Padding'
                                {...getSettingModificationProps('terminalPaneVerticalPaddingPx')}
                                max={MAX_TERMINAL_PANE_PADDING_PX}
                                min={MIN_TERMINAL_PANE_PADDING_PX}
                                onCommit={(value) => updateDraft('terminalPaneVerticalPaddingPx', value)}
                                onChange={(value) => updateDraftDebounced('terminalPaneVerticalPaddingPx', value)}
                                step={1}
                                value={draft.terminalPaneVerticalPaddingPx}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalCursorStyle') ? (
                              <SelectField
                                description='Choose the cursor shape.'
                                label='Cursor Style'
                                {...getSettingModificationProps('terminalCursorStyle')}
                                onChange={(value) => updateDraft('terminalCursorStyle', value as TerminalCursorStyle)}
                                options={[
                                  { label: 'Line', value: 'bar' },
                                  { label: 'Block', value: 'block' },
                                  { label: 'Underline', value: 'underline' },
                                ]}
                                value={draft.terminalCursorStyle}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'terminalCursorStyleBlink') ? (
                              <ToggleField
                                checked={draft.terminalCursorStyleBlink}
                                description='Blink the terminal cursor.'
                                label='Cursor blink'
                                {...getSettingModificationProps('terminalCursorStyleBlink')}
                                onChange={(checked) => updateDraft('terminalCursorStyleBlink', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'clickToWakeSleepingSessions') ? (
                              <ToggleField
                                checked={draft.clickToWakeSleepingSessions}
                                description='Selecting a sleeping pane tab shows a black placeholder; click the pane body to wake the session.'
                                label='Click to wake sleeping panes'
                                {...getSettingModificationProps('clickToWakeSleepingSessions')}
                                onChange={(checked) => updateDraft('clickToWakeSleepingSessions', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'showSessionIdInTerminalPanes') ? (
                              /*
                               * CDXC:SessionPersistence 2026-05-23-00:50:
                               * The pane-local provider/session label is useful for zmx/tmux/zellij
                               * attach context. The label renderer still requires each terminal pane
                               * to have provider metadata before showing text.
                               */
                              <ToggleField
                                checked={draft.showSessionIdInTerminalPanes}
                                description='Show the provider session id in the top-right corner of each terminal pane.'
                                label='Show session id in the top right of each terminal pane'
                                {...getSettingModificationProps('showSessionIdInTerminalPanes')}
                                onChange={(checked) => updateDraft('showSessionIdInTerminalPanes', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'showNotificationOnTerminalBell') ? (
                              /*
                               * CDXC:TerminalBellAttention 2026-07-01-01:13:
                               * Terminal bell notifications belong with Terminal settings because
                               * the event originates from shell/PTY behavior, not agent completion
                               * audio. Keep the setting off by default so failed zsh completion
                               * tabs do not create macOS banners or #95d7f6 attention chrome.
                               */
                              <ToggleField
                                checked={draft.showNotificationOnTerminalBell}
                                description='Treat terminal bell events as session attention.'
                                label='Show notification on terminal bell'
                                {...getSettingModificationProps('showNotificationOnTerminalBell')}
                                onChange={(checked) => updateDraft('showNotificationOnTerminalBell', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminal, 'promptEditorBackend') ? (
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
                                advanced={getSettingModificationProps('promptEditorBackend').advanced}
                                backend={draft.promptEditorBackend}
                                isModified={getSettingModificationProps('promptEditorBackend').isModified}
                                onChange={(backend) => updateDraft('promptEditorBackend', backend)}
                                onResetToDefault={getSettingModificationProps('promptEditorBackend').onResetToDefault}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('terminalBehavior', settingsSearch.terminalBehavior) ? (
                          <SettingsSection sectionRef={ghosttyBehaviorSectionRef} title='Terminal Behavior'>
                            {/* CDXC:TerminalBehaviorSettings 2026-04-29-09:32: Expose the
                  Ghostty settings users commonly tune: scrollback memory,
                  copy-on-select, close confirmation, clipboard safety,
                  pointer hiding, and native scrollbar visibility. These
                  controls write documented Ghostty config keys instead of
                  intercepting terminal behavior inside ghostex. */}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalScrollbackLimitMb') ? (
                              <SliderNumberField
                                description='Scrollback memory per terminal surface. Ghostty default is 10 MB and changes affect new terminals.'
                                label='Scrollback limit'
                                {...getSettingModificationProps('terminalScrollbackLimitMb')}
                                max={200}
                                min={1}
                                onCommit={(value) => updateDraft('terminalScrollbackLimitMb', value)}
                                onChange={(value) => updateDraftDebounced('terminalScrollbackLimitMb', value)}
                                step={1}
                                value={draft.terminalScrollbackLimitMb}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalCopyOnSelect') ? (
                              <SelectField
                                description='Copy selected terminal text automatically.'
                                label='Copy on select'
                                {...getSettingModificationProps('terminalCopyOnSelect')}
                                onChange={(value) => updateDraft('terminalCopyOnSelect', value as GhosttyCopyOnSelect)}
                                options={GHOSTTY_COPY_ON_SELECT_OPTIONS}
                                value={draft.terminalCopyOnSelect}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalConfirmCloseSurface') ? (
                              <SelectField
                                description='Confirm before closing terminal surfaces.'
                                label='Confirm close'
                                {...getSettingModificationProps('terminalConfirmCloseSurface')}
                                onChange={(value) =>
                                  updateDraft('terminalConfirmCloseSurface', value as GhosttyConfirmCloseSurface)
                                }
                                options={GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS}
                                value={draft.terminalConfirmCloseSurface}
                              />
                            ) : null}
                            {mainSettingVisible(
                              settingsSearch.terminalBehavior,
                              'terminalClipboardTrimTrailingSpaces'
                            ) ? (
                              <ToggleField
                                checked={draft.terminalClipboardTrimTrailingSpaces}
                                description='Trim trailing whitespace when copying terminal text.'
                                label='Trim trailing spaces on copy'
                                {...getSettingModificationProps('terminalClipboardTrimTrailingSpaces')}
                                onChange={(checked) => updateDraft('terminalClipboardTrimTrailingSpaces', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalClipboardPasteProtection') ? (
                              <ToggleField
                                checked={draft.terminalClipboardPasteProtection}
                                description='Ask before pasting text Ghostty considers unsafe.'
                                label='Paste protection'
                                {...getSettingModificationProps('terminalClipboardPasteProtection')}
                                onChange={(checked) => updateDraft('terminalClipboardPasteProtection', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalPastePreviewableImages') ? (
                              <ToggleField
                                checked={draft.terminalPastePreviewableImages}
                                description={PASTE_PREVIEWABLE_IMAGES_DESCRIPTION}
                                label='Paste previewable images'
                                {...getSettingModificationProps('terminalPastePreviewableImages')}
                                onChange={(checked) => updateDraft('terminalPastePreviewableImages', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalMouseHideWhileTyping') ? (
                              <ToggleField
                                checked={draft.terminalMouseHideWhileTyping}
                                description='Hide the pointer while typing in the terminal.'
                                label='Hide mouse while typing'
                                {...getSettingModificationProps('terminalMouseHideWhileTyping')}
                                onChange={(checked) => updateDraft('terminalMouseHideWhileTyping', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.terminalBehavior, 'terminalScrollbar') ? (
                              <SelectField
                                description='Control whether Ghostty shows its native scrollback scrollbar.'
                                label='Scrollbar'
                                {...getSettingModificationProps('terminalScrollbar')}
                                onChange={(value) => updateDraft('terminalScrollbar', value as GhosttyScrollbar)}
                                options={GHOSTTY_SCROLLBAR_OPTIONS}
                                value={draft.terminalScrollbar}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('terminalScrolling', settingsSearch.terminalScrolling) ? (
                          <SettingsSection sectionRef={ghosttyScrollingSectionRef} title='Terminal Scrolling'>
                            {/* CDXC:TerminalScrollSettings 2026-04-29-08:56: Ghostty
                  scroll speed is controlled by mouse-scroll-multiplier.
                  Precision and discrete devices need separate controls because
                  Ghostty defaults trackpads to 1 and notched wheels to 3.
                  The modal exposes 0.25-step sliders from 0.25 to 8 because
                  Ghostty's documented 0.01..10000 bounds are extreme. */}
                            {mainSettingVisible(
                              settingsSearch.terminalScrolling,
                              'terminalMouseScrollMultiplierPrecision'
                            ) ? (
                              <SliderNumberField
                                description='Trackpads and high-resolution scroll wheels. Ghostty default is 1.'
                                label='Precision scroll multiplier'
                                {...getSettingModificationProps('terminalMouseScrollMultiplierPrecision')}
                                max={8}
                                min={0.25}
                                onCommit={(value) => updateDraft('terminalMouseScrollMultiplierPrecision', value)}
                                onChange={(value) =>
                                  updateDraftDebounced('terminalMouseScrollMultiplierPrecision', value)
                                }
                                step={0.25}
                                value={draft.terminalMouseScrollMultiplierPrecision}
                              />
                            ) : null}
                            {mainSettingVisible(
                              settingsSearch.terminalScrolling,
                              'terminalMouseScrollMultiplierDiscrete'
                            ) ? (
                              <SliderNumberField
                                description='Traditional notched mouse wheels. Ghostty default is 3.'
                                label='Discrete scroll multiplier'
                                {...getSettingModificationProps('terminalMouseScrollMultiplierDiscrete')}
                                max={8}
                                min={0.25}
                                onCommit={(value) => updateDraft('terminalMouseScrollMultiplierDiscrete', value)}
                                onChange={(value) =>
                                  updateDraftDebounced('terminalMouseScrollMultiplierDiscrete', value)
                                }
                                step={0.25}
                                value={draft.terminalMouseScrollMultiplierDiscrete}
                              />
                            ) : null}
                            {mainSettingVisible(
                              settingsSearch.terminalScrolling,
                              'terminalScrollToBottomWhenTyping'
                            ) ? (
                              <ToggleField
                                checked={draft.terminalScrollToBottomWhenTyping}
                                description='Keep the prompt visible while typing.'
                                label='Scroll to bottom when typing'
                                {...getSettingModificationProps('terminalScrollToBottomWhenTyping')}
                                onChange={(checked) => updateDraft('terminalScrollToBottomWhenTyping', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('autoSleep', settingsSearch.autoSleep) ? (
                          <SettingsSection sectionRef={autoSleepSectionRef} title='Auto Sleep'>
                            {/* CDXC:AutoSleep 2026-05-28-08:32: Auto Sleep controls belong in one Settings section so VS Code, Git, Project, Manage, browser, and agent sessions can be tuned independently without hiding the relationship between the policies. */}
                            {mainSettingVisible(settingsSearch.autoSleep, 'autoSleepCodeEditorEnabled') ? (
                              <ToggleField
                                checked={draft.autoSleepCodeEditorEnabled}
                                description='Sleep inactive VS Code panes after the selected idle period.'
                                label='Sleep inactive VS Code panes'
                                {...getSettingModificationProps('autoSleepCodeEditorEnabled')}
                                onChange={(checked) => updateDraft('autoSleepCodeEditorEnabled', checked)}
                              />
                            ) : null}
                            {draft.autoSleepCodeEditorEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepCodeEditorIdleMinutes') ? (
                              <SelectField
                                description='Idle time before inactive VS Code panes sleep.'
                                label='VS Code idle time'
                                {...getSettingModificationProps('autoSleepCodeEditorIdleMinutes')}
                                onChange={(value) =>
                                  updateDraft('autoSleepCodeEditorIdleMinutes', Number(value) as AutoSleepIdleMinutes)
                                }
                                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                                  label: option.label,
                                  value: String(option.value),
                                }))}
                                value={String(draft.autoSleepCodeEditorIdleMinutes)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.autoSleep, 'autoSleepGitEditorEnabled') ? (
                              <ToggleField
                                checked={draft.autoSleepGitEditorEnabled}
                                description='Sleep inactive Git panes after the selected idle period.'
                                label='Sleep inactive Git panes'
                                {...getSettingModificationProps('autoSleepGitEditorEnabled')}
                                onChange={(checked) => updateDraft('autoSleepGitEditorEnabled', checked)}
                              />
                            ) : null}
                            {draft.autoSleepGitEditorEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepGitEditorIdleMinutes') ? (
                              <SelectField
                                description='Idle time before inactive Git panes sleep.'
                                label='Git idle time'
                                {...getSettingModificationProps('autoSleepGitEditorIdleMinutes')}
                                onChange={(value) =>
                                  updateDraft('autoSleepGitEditorIdleMinutes', Number(value) as AutoSleepIdleMinutes)
                                }
                                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                                  label: option.label,
                                  value: String(option.value),
                                }))}
                                value={String(draft.autoSleepGitEditorIdleMinutes)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.autoSleep, 'autoSleepProjectEditorEnabled') ? (
                              <ToggleField
                                checked={draft.autoSleepProjectEditorEnabled}
                                description='Sleep inactive Project panes after the selected idle period.'
                                label='Sleep inactive Project panes'
                                {...getSettingModificationProps('autoSleepProjectEditorEnabled')}
                                onChange={(checked) => updateDraft('autoSleepProjectEditorEnabled', checked)}
                              />
                            ) : null}
                            {draft.autoSleepProjectEditorEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepProjectEditorIdleMinutes') ? (
                              <SelectField
                                description='Idle time before inactive Project panes sleep.'
                                label='Project idle time'
                                {...getSettingModificationProps('autoSleepProjectEditorIdleMinutes')}
                                onChange={(value) =>
                                  updateDraft(
                                    'autoSleepProjectEditorIdleMinutes',
                                    Number(value) as AutoSleepIdleMinutes
                                  )
                                }
                                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                                  label: option.label,
                                  value: String(option.value),
                                }))}
                                value={String(draft.autoSleepProjectEditorIdleMinutes)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.autoSleep, 'autoSleepBrowserSessionsEnabled') ? (
                              <ToggleField
                                checked={draft.autoSleepBrowserSessionsEnabled}
                                description='Sleep inactive browser panes after the selected idle period.'
                                label='Sleep inactive browser panes'
                                {...getSettingModificationProps('autoSleepBrowserSessionsEnabled')}
                                onChange={(checked) => updateDraft('autoSleepBrowserSessionsEnabled', checked)}
                              />
                            ) : null}
                            {draft.autoSleepBrowserSessionsEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepBrowserIdleMinutes') ? (
                              <SelectField
                                description='Idle time before inactive browser panes sleep.'
                                label='Browser idle time'
                                {...getSettingModificationProps('autoSleepBrowserIdleMinutes')}
                                onChange={(value) =>
                                  updateDraft('autoSleepBrowserIdleMinutes', Number(value) as AutoSleepIdleMinutes)
                                }
                                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                                  label: option.label,
                                  value: String(option.value),
                                }))}
                                value={String(draft.autoSleepBrowserIdleMinutes)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.autoSleep, 'autoSleepAgentSessionsEnabled') ? (
                              <ToggleField
                                checked={draft.autoSleepAgentSessionsEnabled}
                                description='Sleep idle agent terminal sessions automatically.'
                                label='Sleep idle agent sessions'
                                {...getSettingModificationProps('autoSleepAgentSessionsEnabled')}
                                onChange={(checked) => updateDraft('autoSleepAgentSessionsEnabled', checked)}
                              />
                            ) : null}
                            {draft.autoSleepAgentSessionsEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepAgentIdleMinutes') ? (
                              <SelectField
                                description='Idle time before eligible agent terminals sleep.'
                                label='Agent idle time'
                                {...getSettingModificationProps('autoSleepAgentIdleMinutes')}
                                onChange={(value) =>
                                  updateDraft('autoSleepAgentIdleMinutes', Number(value) as AutoSleepIdleMinutes)
                                }
                                options={AUTO_SLEEP_IDLE_MINUTE_OPTIONS.map((option) => ({
                                  label: option.label,
                                  value: String(option.value),
                                }))}
                                value={String(draft.autoSleepAgentIdleMinutes)}
                              />
                            ) : null}
                            {draft.autoSleepAgentSessionsEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepRequireAgentResumeCommand') ? (
                              <ToggleField
                                checked={draft.autoSleepRequireAgentResumeCommand}
                                description='Only auto-sleep agent sessions Ghostex can wake with a resume command.'
                                label='Require resume command'
                                {...getSettingModificationProps('autoSleepRequireAgentResumeCommand')}
                                onChange={(checked) => updateDraft('autoSleepRequireAgentResumeCommand', checked)}
                              />
                            ) : null}
                            {draft.autoSleepAgentSessionsEnabled &&
                            mainSettingVisible(settingsSearch.autoSleep, 'autoSleepFavoriteAgentSessions') ? (
                              <ToggleField
                                checked={draft.autoSleepFavoriteAgentSessions}
                                description='Allow favorite agent sessions to auto-sleep.'
                                label='Include favorite agents'
                                {...getSettingModificationProps('autoSleepFavoriteAgentSessions')}
                                onChange={(checked) => updateDraft('autoSleepFavoriteAgentSessions', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('power', settingsSearch.power) ? (
                          <SettingsSection sectionRef={powerSectionRef} title='Power'>
                            {mainSettingVisible(settingsSearch.power, 'hideKeepAwakeTitlebarControl') ? (
                              <ToggleField
                                checked={draft.hideKeepAwakeTitlebarControl}
                                description='Hide the keep-awake control from the title bar.'
                                label='Hide title-bar keep-awake control'
                                {...getSettingModificationProps('hideKeepAwakeTitlebarControl')}
                                onChange={(checked) => updateDraft('hideKeepAwakeTitlebarControl', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeDefaultDurationMinutes') ? (
                              <SelectField
                                description='Choose the duration used by the title-bar keep-awake button.'
                                label='Default keep-awake duration'
                                {...getSettingModificationProps('keepAwakeDefaultDurationMinutes')}
                                onChange={(value) =>
                                  updateDraft(
                                    'keepAwakeDefaultDurationMinutes',
                                    Number(value) as KeepAwakeDurationMinutes
                                  )
                                }
                                options={KEEP_AWAKE_DURATION_OPTIONS.map((option) => ({
                                  label: option.label,
                                  value: String(option.value),
                                }))}
                                value={String(draft.keepAwakeDefaultDurationMinutes)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeAllowDisplaySleep') ? (
                              <ToggleField
                                checked={draft.keepAwakeAllowDisplaySleep}
                                description='Keep the Mac awake but allow the display to turn off.'
                                label='Allow display sleep'
                                {...getSettingModificationProps('keepAwakeAllowDisplaySleep')}
                                onChange={(checked) => updateDraft('keepAwakeAllowDisplaySleep', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakePreventLidSleep') ? (
                              <ToggleField
                                checked={draft.keepAwakePreventLidSleep}
                                description='Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session. Keep Awake itself remains off until you enable it.'
                                label='Prevent lid-close sleep'
                                {...getSettingModificationProps('keepAwakePreventLidSleep')}
                                onChange={(checked) => updateDraft('keepAwakePreventLidSleep', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeActivateOnLaunch') ? (
                              <ToggleField
                                checked={draft.keepAwakeActivateOnLaunch}
                                description='Start preventing sleep when Ghostex launches.'
                                label='Activate on launch'
                                {...getSettingModificationProps('keepAwakeActivateOnLaunch')}
                                onChange={(checked) => updateDraft('keepAwakeActivateOnLaunch', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeActivateOnExternalDisplay') ? (
                              <ToggleField
                                checked={draft.keepAwakeActivateOnExternalDisplay}
                                description='Start preventing sleep when an external display is connected.'
                                label='Activate on external display'
                                {...getSettingModificationProps('keepAwakeActivateOnExternalDisplay')}
                                onChange={(checked) => updateDraft('keepAwakeActivateOnExternalDisplay', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeWhileWorkingSessions') ? (
                              <ToggleField
                                checked={draft.keepAwakeWhileWorkingSessions}
                                description='Keep the Mac awake while any session is Working and for 20 minutes after, so you have time to reply.'
                                label='Keep awake for working sessions'
                                {...getSettingModificationProps('keepAwakeWhileWorkingSessions')}
                                onChange={(checked) => updateDraft('keepAwakeWhileWorkingSessions', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeDeactivateBelowBatteryThreshold') ? (
                              <ToggleField
                                checked={draft.keepAwakeDeactivateBelowBatteryThreshold}
                                description='Stop preventing sleep when battery capacity drops below the threshold.'
                                label='Deactivate below battery threshold'
                                {...getSettingModificationProps('keepAwakeDeactivateBelowBatteryThreshold')}
                                onChange={(checked) => updateDraft('keepAwakeDeactivateBelowBatteryThreshold', checked)}
                              />
                            ) : null}
                            {draft.keepAwakeDeactivateBelowBatteryThreshold &&
                            mainSettingVisible(settingsSearch.power, 'keepAwakeBatteryThresholdPercent') ? (
                              <SliderNumberField
                                description='Battery percentage used by the threshold rule.'
                                label='Battery threshold'
                                {...getSettingModificationProps('keepAwakeBatteryThresholdPercent')}
                                max={90}
                                min={10}
                                onCommit={(value) => updateDraft('keepAwakeBatteryThresholdPercent', value)}
                                onChange={(value) => updateDraftDebounced('keepAwakeBatteryThresholdPercent', value)}
                                step={5}
                                value={draft.keepAwakeBatteryThresholdPercent}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeDeactivateOnLowPowerMode') ? (
                              <ToggleField
                                checked={draft.keepAwakeDeactivateOnLowPowerMode}
                                description='Stop preventing sleep when macOS Low Power Mode is enabled.'
                                label='Deactivate in Low Power Mode'
                                {...getSettingModificationProps('keepAwakeDeactivateOnLowPowerMode')}
                                onChange={(checked) => updateDraft('keepAwakeDeactivateOnLowPowerMode', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.power, 'keepAwakeDeactivateOnUserSwitch') ? (
                              <ToggleField
                                checked={draft.keepAwakeDeactivateOnUserSwitch}
                                description='Stop preventing sleep when this user session is no longer active.'
                                label='Deactivate on user switch'
                                {...getSettingModificationProps('keepAwakeDeactivateOnUserSwitch')}
                                onChange={(checked) => updateDraft('keepAwakeDeactivateOnUserSwitch', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('storage', settingsSearch.storage) ? (
                          <div ref={storageSectionRef}>
                            <GhostexFolderStatsSection
                              isLoading={ghostexFolderStatsLoading}
                              onOpenGhostexFolder={onOpenGhostexFolder}
                              stats={ghostexFolderStats}
                            />
                          </div>
                        ) : null}

                        {mainSubsectionVisible('sounds', settingsSearch.sounds) ? (
                          <SettingsSection sectionRef={soundsSectionRef} title='Sounds'>
                            {mainSettingVisible(settingsSearch.sounds, 'completionBellEnabled') ? (
                              <ToggleField
                                checked={draft.completionBellEnabled}
                                description='Play a completion sound when work finishes.'
                                label='Enable completion bell'
                                {...getSettingModificationProps('completionBellEnabled')}
                                onChange={(checked) => updateDraft('completionBellEnabled', checked)}
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sounds, 'completionSound') ? (
                              <SoundField
                                description='Sound for terminal completions.'
                                label='Completion Sound'
                                {...getSettingModificationProps('completionSound')}
                                onChange={(value) => updateDraft('completionSound', value)}
                                onPlay={onPlayCompletionSound}
                                value={draft.completionSound}
                              />
                            ) : null}
                            {/* CDXC:SessionAttentionNotifications 2026-05-10-16:46:
                  Attention banners are separate from completion sounds because
                  users may want clickable macOS routing without audible alerts. */}
                            {mainSettingVisible(settingsSearch.sounds, 'showMacOSAttentionNotifications') ? (
                              <ToggleField
                                checked={draft.showMacOSAttentionNotifications}
                                description='Show a macOS banner when a session needs attention.'
                                label='macOS Attention Notifications'
                                {...getSettingModificationProps('showMacOSAttentionNotifications')}
                                onChange={(checked) => {
                                  updateDraft('showMacOSAttentionNotifications', checked);
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
                            {mainSettingVisible(settingsSearch.sounds, 'attentionNotificationActions') ? (
                              <ActionButtonPairField
                                advanced={isAdvancedMainSetting('attentionNotificationActions')}
                                actions={[
                                  {
                                    label: 'Test agent task completion',
                                    onClick: () => onTestAgentTaskCompletion?.(),
                                  },
                                  {
                                    label: 'macOS Notification Settings',
                                    onClick: () => onOpenMacOSNotificationSettings?.(),
                                  },
                                ]}
                                description='Run the current completion sound and notification flow, or open macOS notification permissions.'
                                label='Completion Alerts'
                              />
                            ) : null}
                            {mainSettingVisible(settingsSearch.sounds, 'actionCompletionSound') ? (
                              <SoundField
                                description='Sound for action completions.'
                                label='Action Completion Sound'
                                {...getSettingModificationProps('actionCompletionSound')}
                                onChange={(value) => updateDraft('actionCompletionSound', value)}
                                onPlay={onPlayCompletionSound}
                                value={draft.actionCompletionSound}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {/*
                         * CDXC:AnonymousAnalytics 2026-08-26:
                         * The analytics opt-out is its own always-visible
                         * Privacy section. It must never sit behind Show
                         * Advanced or the Experimental gate: a user who wants
                         * to turn analytics off has to be able to find the
                         * switch by browsing Settings.
                         */}
                        {mainSectionVisible('privacy', settingsSearch.privacy) ? (
                          <SettingsSection sectionRef={privacySectionRef} title='Privacy'>
                            {mainSettingVisible(settingsSearch.privacy, 'analyticsEnabled') ? (
                              <ToggleField
                                checked={draft.analyticsEnabled}
                                description='Share anonymous usage data (OS, feature usage, counts). Never prompts, file paths, project names, or anything personal.'
                                label='Anonymous usage analytics'
                                {...getSettingModificationProps('analyticsEnabled')}
                                onChange={(checked) => updateDraft('analyticsEnabled', checked)}
                              />
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('beta', settingsSearch.beta) ? (
                          <SettingsSection sectionRef={betaSectionRef} title='Experimental'>
                            {mainSettingVisible(settingsSearch.beta, 'showBetaFeatures') ? (
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
                                      ? 'Show experimental settings, Automations and Automate pages, and the Keep Awake title-bar button.'
                                      : 'Show experimental settings, Automations Overview, and the Keep Awake title-bar button.'
                                  }
                                  label='Enable Experimental Features'
                                  {...getSettingModificationProps('showBetaFeatures')}
                                  onChange={(checked) => updateDraft('showBetaFeatures', checked)}
                                />
                                <div className='rounded-[var(--settings-radius-control)] border border-border bg-muted/20 px-4 py-3 text-sm text-muted-foreground'>
                                  <div className='mb-2 font-medium text-foreground'>Enabled when on</div>
                                  <ul className='grid gap-1.5'>
                                    <li>OS Integration settings tab</li>
                                    <li>
                                      {automateIsExperimental
                                        ? 'Automations Overview and project Automate pages'
                                        : 'Automations Overview'}
                                    </li>
                                    <li>Title bar and Power settings: Keep Awake</li>
                                  </ul>
                                </div>
                              </>
                            ) : null}
                          </SettingsSection>
                        ) : null}

                        {mainSubsectionVisible('debugging', settingsSearch.debugging) ? (
                          <SettingsSection sectionRef={debuggingSectionRef} title='Debugging'>
                            {debuggingSettingVisible('debuggingMode') ? (
                              <ToggleField
                                checked={draft.debuggingMode}
                                description={
                                  draft.debuggingMode
                                    ? 'Shows debug-only UI controls and allows the enabled diagnostic scenarios below to write routine logs.'
                                    : 'Turn on to reveal debug-only controls and allow routine diagnostic logging. Important warnings, errors, and crashes remain captured.'
                                }
                                label='Show debug UI controls'
                                {...getSettingModificationProps('debuggingMode')}
                                onChange={(checked) => updateDraft('debuggingMode', checked)}
                              />
                            ) : null}
                            {debuggingSettingVisible('diagnosticLogging') ? (
                              <DiagnosticLoggingSettingsField
                                isModified={
                                  !areDiagnosticLoggingSettingsEqual(
                                    draft.diagnosticLogging,
                                    DEFAULT_ghostex_SETTINGS.diagnosticLogging
                                  )
                                }
                                onChange={updateDiagnosticLoggingScenario}
                                onResetToDefault={() =>
                                  updateDraft('diagnosticLogging', DEFAULT_ghostex_SETTINGS.diagnosticLogging)
                                }
                                value={draft.diagnosticLogging}
                              />
                            ) : null}
                            {debuggingSettingVisible('showSessionCommandCopyActions') ? (
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
                                  description='Show Copy resume and Copy attach command in session context menus.'
                                  label='Show command copy actions'
                                  {...getSettingModificationProps('showSessionCommandCopyActions')}
                                  onChange={(checked) => updateDraft('showSessionCommandCopyActions', checked)}
                                />
                              </>
                            ) : null}
                            {debuggingSettingVisible('showSessionDetailsCopyAction') ? (
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
                                  description='Show Copy details in session context menus.'
                                  label='Show Copy details option'
                                  {...getSettingModificationProps('showSessionDetailsCopyAction')}
                                  onChange={(checked) => updateDraft('showSessionDetailsCopyAction', checked)}
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
                          <div className='flex justify-end pt-2'>
                            <Button className='h-8 px-3 text-[13px]' onClick={closeSettingsModal} type='button'>
                              Continue
                            </Button>
                          </div>
                        ) : (
                          <>
                            <Separator className='bg-border' />
                            <div className='flex justify-between gap-3'>
                              <Button
                                className='h-8 px-3 text-[13px]'
                                onClick={resetSettings}
                                type='button'
                                variant='outline'
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
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='osIntegration'>
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
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='integrations'>
                    <IntegrationsSettingsTab
                      ghostexCliStatus={ghostexCliStatus}
                      ghostexCliStatusLoading={ghostexCliStatusLoading}
                      appShotsEnabled={draft.appShotsEnabled}
                      appShotsHotkey={draft.appShotsHotkey}
                      appShotsMetadataEnabled={draft.appShotsMetadataEnabled}
                      onAppShotsEnabledChange={(checked) => updateDraft('appShotsEnabled', checked)}
                      onAppShotsHotkeyChange={(hotkey) => updateDraft('appShotsHotkey', hotkey)}
                      onAppShotsMetadataEnabledChange={(checked) => updateDraft('appShotsMetadataEnabled', checked)}
                      onInstallCliSkill={onInstallCliSkill}
                      onInstallBrowserControl={onInstallBrowserControl}
                      onInstallBrowserUseSkill={onInstallBrowserUseSkill}
                      onInstallComputerUseSkill={onInstallComputerUseSkill}
                      onInstallCuaDriver={onInstallCuaDriver}
                      onInstallFable56OrchestrationSkill={onInstallFable56OrchestrationSkill}
                      onInstallManageBeadsSkill={onInstallManageBeadsSkill}
                      onInstallGenerateTitleSkill={onInstallGenerateTitleSkill}
                      onInstallGhostexCli={onInstallGhostexCli}
                      onInstallMoveCodexSessionSkill={onInstallMoveCodexSessionSkill}
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
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='plugins'>
                    <PluginsSettingsTab
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
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='remote'>
                    <RemoteSettingsTab
                      initialRemoteMachineId={initialRemoteMachineId}
                      isActive={isOpen && activeTab === 'remote'}
                      onChange={(nextRemoteMachines) =>
                        applySettingsPatch(
                          {
                            remoteMachines: nextRemoteMachines,
                          },
                          'settings:remoteMachines'
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
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='projects'>
                    <ProjectsSettingsPanel
                      onGlobalBeadsDirectoryChange={(value) => updateDraft('globalBeadsDirectory', value)}
                      onGlobalBeadsDisplayKeyChange={(value) => updateDraft('globalBeadsDisplayKey', value)}
                      onGlobalDocsDirectoryChange={(value) => updateDraft('globalDocsDirectory', value)}
                      onGlobalWorktreeCommandChange={(value) => updateDraft('globalWorktreeCommand', value)}
                      onManageAdditionalDocsFoldersChange={(value) => updateDraft('manageAdditionalDocsFolders', value)}
                      onPortlessEnabledChange={(checked) => updateDraft('portlessEnabled', checked)}
                      onPortlessProtocolChange={(protocol) => updateDraft('portlessProtocol', protocol)}
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
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='agents'>
                    <AgentsSettingsTab
                      agentHookStatus={agentHookStatus}
                      agentHookStatusLoading={agentHookStatusLoading}
                      agentAcceptAllEnabled={draft.agentAcceptAllEnabled}
                      customSessionTitleGenerationCommand={draft.customSessionTitleGenerationCommand}
                      defaultPromptAgentId={draft.defaultPromptAgentId}
                      preferredAgentInterface={draft.preferredAgentInterface}
                      preferredAgentInterfaceOverrides={draft.preferredAgentInterfaceOverrides}
                      sessionTitleGenerationAgent={draft.sessionTitleGenerationAgent}
                      onAgentAcceptAllEnabledChange={(checked) => updateDraft('agentAcceptAllEnabled', checked)}
                      onDefaultPromptAgentIdChange={(agentId) => updateDraft('defaultPromptAgentId', agentId)}
                      onCustomSessionTitleGenerationCommandChange={(command) =>
                        updateDraft('customSessionTitleGenerationCommand', command)
                      }
                      onInstallAgentHooks={onInstallAgentHooks}
                      onPreferredAgentInterfaceOverridesChange={(overrides) =>
                        updateDraft('preferredAgentInterfaceOverrides', overrides)
                      }
                      onRequestAgentHookStatus={onRequestAgentHookStatus}
                      onSessionTitleGenerationAgentChange={(agent) => updateDraft('sessionTitleGenerationAgent', agent)}
                      onUninstallAgentHooks={onUninstallAgentHooks}
                      search={extraSettingsTabSearches.agents}
                      searchEmptyState={settingsSearchEmptyState}
                      vscode={vscode}
                    />
                  </TabsContent>
                ) : null}
                {!isFirstLaunchSetup ? (
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='actions'>
                    <ActionsSettingsTab
                      getSettingModificationProps={getSettingModificationProps}
                      hideTabStripNewBrowserButton={draft.hideTabStripNewBrowserButton}
                      hideTabStripNewTerminalButton={draft.hideTabStripNewTerminalButton}
                      onHideTabStripNewBrowserButtonChange={(checked) =>
                        updateDraft('hideTabStripNewBrowserButton', checked)
                      }
                      onHideTabStripNewTerminalButtonChange={(checked) =>
                        updateDraft('hideTabStripNewTerminalButton', checked)
                      }
                      search={extraSettingsTabSearches.actions}
                      searchEmptyState={settingsSearchEmptyState}
                      vscode={vscode}
                    />
                  </TabsContent>
                ) : null}
                {!isFirstLaunchSetup ? (
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='openTargets'>
                    <OpenTargetsSettingsTab
                      onChange={(nextSettings) => applySettings(nextSettings)}
                      search={extraSettingsTabSearches.openTargets}
                      searchEmptyState={settingsSearchEmptyState}
                      settings={draft}
                    />
                  </TabsContent>
                ) : null}
                {!isFirstLaunchSetup ? (
                  <TabsContent
                    className='settings-main-tabs-content mt-0 min-h-0 flex-1 overflow-hidden'
                    value='hotkeys'
                  >
                    <HotkeysSettingsTab
                      definitionsById={hotkeyDefinitionsById}
                      expandCollapsedProjectsOnJump={draft.expandCollapsedProjectsOnJump}
                      expandCollapsedProjectsOnJumpModification={getSettingModificationProps(
                        'expandCollapsedProjectsOnJump'
                      )}
                      hotkeys={draft.hotkeys}
                      sectionRefs={hotkeySectionRefs}
                      sectionSearches={hotkeySectionSearches}
                      showLessForExpandedProjectJumps={draft.showLessForExpandedProjectJumps}
                      showLessForExpandedProjectJumpsModification={getSettingModificationProps(
                        'showLessForExpandedProjectJumps'
                      )}
                      visibleSections={visibleHotkeySections}
                      searchQuery={settingsSearchQuery}
                      onChange={(hotkeys) => updateDraft('hotkeys', hotkeys)}
                      onActiveSectionChange={(sectionId) =>
                        setActiveHotkeySettingsSectionId((currentSectionId) =>
                          currentSectionId === sectionId ? currentSectionId : sectionId
                        )
                      }
                      onExpandCollapsedProjectsOnJumpChange={(checked) =>
                        updateDraft('expandCollapsedProjectsOnJump', checked)
                      }
                      onShowLessForExpandedProjectJumpsChange={(checked) =>
                        updateDraft('showLessForExpandedProjectJumps', checked)
                      }
                    />
                  </TabsContent>
                ) : null}
                {!isFirstLaunchSetup ? (
                  <TabsContent className='mt-0 min-h-0 flex-1 overflow-hidden' value='about'>
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
    <div className='rounded-none border border-border bg-muted/30 px-4 py-6 text-center text-sm text-muted-foreground'>
      <p>{otherPages.length ? 'No settings on this page match your search.' : 'No settings match your search.'}</p>
      {otherPages.length ? (
        <div className='mt-3 flex flex-wrap items-center justify-center gap-2'>
          <span>Matches on:</span>
          {otherPages.map((page) => {
            const PageIcon = page.icon;
            return (
              <Button key={page.id} onClick={() => onSelectPage(page.id)} type='button' variant='outline'>
                <PageIcon aria-hidden='true' data-icon='inline-start' />
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
  const [expandedSections, setExpandedSections] = useState<ReadonlySet<string>>(() => new Set());
  const sectionDisclosureIdPrefix = useId();
  const toggleSection = (sectionKey: string) => {
    setExpandedSections((currentSections) => {
      const nextSections = new Set(currentSections);
      if (nextSections.has(sectionKey)) {
        nextSections.delete(sectionKey);
      } else {
        nextSections.add(sectionKey);
      }
      return nextSections;
    });
  };

  return (
    <aside aria-label='Settings pages and sections' className='settings-section-sidebar'>
      <TabsList className='settings-sidebar-tabs-list vertical-scroll-fade-mask'>
        {pages.map((page) => {
          const hasSections = Boolean(page.sections?.length);
          const expanded = Boolean(expandedPages[page.id]);
          const PageIcon = page.icon;
          return (
            <div
              className={cn('settings-sidebar-page-group', page.id === 'about' && 'settings-sidebar-page-group-about')}
              key={page.id}
            >
              <div className='settings-sidebar-page-row' data-expanded={String(expanded)}>
                {/*
                 * CDXC:SettingsNavigation 2026-06-29-21:45:
                 * Expandable Settings sidebar headers must expand and collapse from the full visible header, not only from the disclosure chevron, because the row highlight presents the icon, label, and chevron as one control.
                 */}
                <TabsTrigger
                  aria-expanded={hasSections ? expanded : undefined}
                  className='settings-sidebar-tab-trigger'
                  onClick={() => {
                    if (hasSections) {
                      onTogglePage(page.id);
                    }
                  }}
                  value={page.id}
                >
                  <PageIcon aria-hidden='true' data-icon='inline-start' />
                  <span className='settings-sidebar-page-title truncate'>{page.title}</span>
                </TabsTrigger>
                {hasSections ? (
                  <Button
                    aria-expanded={expanded}
                    aria-label={`${expanded ? 'Collapse' : 'Expand'} ${page.title} sections`}
                    className='settings-sidebar-page-disclosure'
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      onTogglePage(page.id);
                    }}
                    size='icon-xs'
                    type='button'
                    variant='ghost'
                  >
                    {expanded ? <IconChevronDown aria-hidden='true' /> : <IconChevronRight aria-hidden='true' />}
                  </Button>
                ) : null}
              </div>
              {hasSections && expanded ? (
                <div className='settings-sidebar-subsection-list'>
                  {page.sections?.map((section) => {
                    const hasSubsections = Boolean(section.subsections?.length);
                    const sectionKey = `${page.id}:${section.id}`;
                    const sectionExpanded = hasSubsections && expandedSections.has(sectionKey);
                    const subsectionListId = `${sectionDisclosureIdPrefix}-${page.id}-${section.id}`;
                    return (
                      <Fragment key={section.id}>
                        <div className='settings-sidebar-section-row'>
                          <Button
                            aria-controls={hasSubsections ? subsectionListId : undefined}
                            aria-current={section.active ? 'location' : undefined}
                            aria-expanded={hasSubsections ? sectionExpanded : undefined}
                            className='settings-section-sidebar-button settings-sidebar-subsection-button'
                            data-active={section.active ? 'true' : 'false'}
                            onClick={() => {
                              section.onSelect();
                              if (hasSubsections) {
                                toggleSection(sectionKey);
                              }
                            }}
                            type='button'
                            variant='ghost'
                          >
                            {section.title}
                          </Button>
                          {hasSubsections ? (
                            <Button
                              aria-controls={subsectionListId}
                              aria-expanded={sectionExpanded}
                              aria-label={`${sectionExpanded ? 'Collapse' : 'Expand'} ${section.title} subsections`}
                              className='settings-sidebar-section-disclosure'
                              onClick={() => toggleSection(sectionKey)}
                              size='icon-xs'
                              type='button'
                              variant='ghost'
                            >
                              {sectionExpanded ? (
                                <IconChevronDown aria-hidden='true' />
                              ) : (
                                <IconChevronRight aria-hidden='true' />
                              )}
                            </Button>
                          ) : null}
                        </div>
                        {/*
                         * CDXC:SettingsNavigation 2026-08-24:
                         * Expansion is explicit navigation state, independent
                         * from the scroll-active section. This keeps an opened
                         * third-level list stable while scroll tracking updates
                         * both its parent and exact active subsection.
                         */}
                        {sectionExpanded ? (
                          <div className='settings-sidebar-nested-subsection-list' id={subsectionListId}>
                            {section.subsections?.map((subsection) => (
                              <Button
                                aria-current={subsection.active ? 'location' : undefined}
                                className='settings-section-sidebar-button settings-sidebar-subsection-button settings-sidebar-nested-subsection-button'
                                data-active={subsection.active ? 'true' : 'false'}
                                key={subsection.id}
                                onClick={subsection.onSelect}
                                type='button'
                                variant='ghost'
                              >
                                {subsection.title}
                              </Button>
                            ))}
                          </div>
                        ) : null}
                      </Fragment>
                    );
                  })}
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
      <div className='settings-section-sidebar-footer'>
        <label className='settings-show-advanced-toggle' htmlFor={showAdvancedSettingsId}>
          <span className='settings-show-advanced-copy'>Show Advanced</span>
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
    <SettingsSection title='Storage'>
      <div className='flex items-start justify-between gap-3'>
        <div className='min-w-0'>
          <div className='text-sm font-medium text-foreground'>Ghostex folder</div>
          <div className='mt-1 truncate text-xs text-muted-foreground'>
            {stats?.folderPath ?? '~/.local/share/ghostex'}
          </div>
        </div>
        <SettingButton
          className='h-9 shrink-0 gap-2 px-3 text-sm'
          disabled={!onOpenGhostexFolder}
          disabledReason='Folder access isn’t available here.'
          onClick={onOpenGhostexFolder}
          type='button'
          variant='outline'
        >
          <IconFolderOpen aria-hidden='true' className='size-4' />
          Open Folder
        </SettingButton>
      </div>

      {isLoading && !stats ? (
        <div className='rounded-none border border-border bg-muted/25 px-3 py-2 text-sm text-muted-foreground'>
          Loading folder sizes...
        </div>
      ) : null}

      {stats?.errorMessage ? (
        <div className='rounded-none border border-destructive/45 bg-destructive/10 px-3 py-2 text-sm text-foreground'>
          {stats.errorMessage}
        </div>
      ) : null}

      {stats && !stats.errorMessage ? (
        <div className='rounded-none border border-border bg-muted/20'>
          {folders.length > 0 ? (
            folders.map((folder) => (
              <div
                className='flex items-center justify-between gap-3 border-b border-border px-3 py-2 text-sm last:border-b-0'
                key={folder.path}
              >
                <span className='min-w-0 truncate text-foreground'>{folder.name}</span>
                <span className='shrink-0 tabular-nums text-muted-foreground'>{formatBytes(folder.sizeBytes)}</span>
              </div>
            ))
          ) : (
            <div className='px-3 py-2 text-sm text-muted-foreground'>No folders found.</div>
          )}
          <div className='flex items-center justify-between gap-3 border-t border-border px-3 py-2 text-sm font-medium'>
            <span>Total</span>
            <span className='tabular-nums'>{formatBytes(stats.totalBytes)}</span>
          </div>
        </div>
      ) : null}
    </SettingsSection>
  );
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const decimals = value >= 10 || unitIndex === 0 ? 0 : 1;
  return `${value.toFixed(decimals)} ${units[unitIndex] ?? 'B'}`;
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
    <div className='grid grid-cols-1 gap-3 sm:grid-cols-2'>
      <Button className='h-8 px-3 text-[13px]' onClick={onResetDefaults} type='button' variant='outline'>
        Reset Ghostty defaults
      </Button>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button className='h-8 px-3 text-[13px]' onClick={onApplyRecommended} type='button' variant='outline'>
              Apply recommended
            </Button>
          }
        />
        <TooltipContent className='whitespace-pre-line text-left' sideOffset={6}>
          {GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES.join('\n')}
        </TooltipContent>
      </Tooltip>
      <Button className='h-8 px-3 text-[13px]' onClick={onOpenDocs} type='button' variant='outline'>
        Open Ghostty docs
      </Button>
      <Button className='h-8 px-3 text-[13px]' onClick={onOpenConfigFile} type='button' variant='outline'>
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
      description='Choose which editor new terminals use when Ctrl+G asks the shell to edit prompt text.'
      htmlFor={id}
      isModified={isModified}
      label='Ctrl+G prompt editor'
      onResetToDefault={onResetToDefault}
    >
      <SettingsSelect onValueChange={(value) => onChange(value as PromptEditorBackend)} value={backend}>
        <SelectTrigger className='h-8 w-full px-3 text-[13px]' id={id}>
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
  return theme.startsWith('light-') || theme === 'plain-light' ? 'light' : 'dark';
}
