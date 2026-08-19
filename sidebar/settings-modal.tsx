import { DragDropProvider, type DragDropEventHandlers } from "@dnd-kit/react";
import { isSortableOperation, useSortable } from "@dnd-kit/react/sortable";
import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type RefObject,
  type UIEvent as ReactUIEvent,
} from "react";
import { flushSync } from "react-dom";
import Fuse from "fuse.js";
import ColorPicker from "react-best-gradient-color-picker";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Card, CardContent, CardTitle } from "@/components/ui/card";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
import { Input as BaseInput } from "@/components/ui/input";
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
import { Textarea as BaseTextarea } from "@/components/ui/textarea";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { AppTooltip } from "./app-tooltip";
import { DisabledSettingControlTooltip } from "./disabled-setting-control-tooltip";
import { SidebarSessionSearchField } from "./sidebar-session-search-overlay";
import {
  resolveSettingsModalTabForVisibility,
  shouldShowOSIntegrationSettingsTab,
  type SettingsModalTab,
  type SettingsModalTabVisibilityOptions,
} from "./settings-modal-tabs";
import {
  IconAsterisk,
  IconAlertTriangle,
  IconArrowBigUp,
  IconBolt,
  IconCashEdit,
  IconChevronDown,
  IconChevronRight,
  IconCircleCheckFilled,
  IconCircleX,
  IconCloud,
  IconCodeDots,
  IconDeviceDesktop,
  IconDownload,
  IconEye,
  IconEyeOff,
  IconExternalLink,
  IconFileText,
  IconFolderOpen,
  IconGitCommit,
  IconGripVertical,
  IconInfoCircle,
  IconKeyboard,
  IconMinus,
  IconPalette,
  // CDXC:AppIconPicker 2026-06-25-21:50: Placeholder glyph for the default-icon tile and missing thumbnails.
  IconPhoto,
  IconPencil,
  IconPlayerPlay,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconDeviceFloppy,
  IconTerminal2,
  IconTools,
  IconTrash,
  IconWorld,
  IconX,
} from "@tabler/icons-react";
import { COMPLETION_SOUND_OPTIONS, type CompletionSoundSetting } from "../shared/completion-sound";
import { GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES } from "../shared/ghostty-config-actions";
import {
  resolveSidebarTheme,
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon picker consumes native state + per-icon info shapes.
  type SidebarAppIconInfo,
  type SidebarAppIconStateMessage,
  type SidebarAgentHookStatusMessage,
  type SidebarAgentHookStatusItem,
  type SidebarGhostexCliStatusMessage,
  type SidebarGhostexFolderStatsMessage,
  type SidebarOSIntegrationStatusMessage,
  type SidebarPluginSettingsItem,
  type SidebarPluginSettingsStatusMessage,
  type SidebarOSIntegrationStatusItem,
  type SidebarPortlessState,
  type SidebarProjectSettingsItem,
  type SidebarTheme,
  type SidebarThemeVariant,
} from "../shared/session-grid-contract";
import {
  AUTO_SLEEP_IDLE_MINUTE_OPTIONS,
  APP_SHOTS_HOTKEY_OPTIONS,
  DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
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
  SESSION_TITLE_GENERATION_AGENT_OPTIONS,
  SIDEBAR_AUTO_SETTLE_AFTER_DAYS_OPTIONS,
  SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS,
  SIDEBAR_SETTINGS_PRESETS,
  SIDEBAR_SIDE_OPTIONS,
  SIDEBAR_VERSION_OPTIONS,
  WEB_LINK_OPEN_TARGET_OPTIONS,
  applySidebarSettingsPreset,
  areDiagnosticLoggingSettingsEqual,
  getSessionTitleGenerationCommandPreview,
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
  normalizeTerminalDevServerIgnoredPortRuleInput,
  normalizeTerminalDevServerIgnoredPortRules,
  normalizeSettingsModalNavigationState,
  normalizeghostexSettings,
  normalizeRemoteMachineSettings,
  parseSidebarAutoSettleAfterDaysSelectValue,
  setDiagnosticLoggingScenario,
  sidebarAutoSettleAfterDaysSelectValue,
  type AppShotsHotkey,
  type AutoSleepIdleMinutes,
  type DiagnosticLoggingScenarioId,
  type DiagnosticLoggingSettings,
  type GhosttyConfirmCloseSurface,
  type GhosttyCopyOnSelect,
  type GhosttyScrollbar,
  type KeepAwakeDurationMinutes,
  type PortlessProtocol,
  type PreferredAgentInterface,
  type RemoteMachineSettings,
  type SessionPersistenceProvider,
  type SettingsModalNavigationState,
  type SessionTitleGenerationAgent,
  type SidebarSettingsPresetId,
  type SidebarProjectGroupStyle,
  type CommandsPanelSide,
  type SidebarSide,
  type SidebarVersion,
  type TerminalBackgroundImageFit,
  type WebLinkOpenTarget,
  type TerminalCursorStyle,
  type ghostexSettingsPatch,
  type ghostexSettingsUpdateSource,
  type ghostexSettings,
} from "../shared/ghostex-settings";
import type { SessionChatTheme } from "../shared/session-chat";
import {
  BUILT_IN_WORKSPACE_OPEN_TARGETS,
  CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX,
  createWorkspaceOpenTargetSlug,
  normalizeCustomWorkspaceOpenTargets,
  normalizeWorkspaceOpenTargetHiddenIds,
  type CustomWorkspaceOpenTarget,
} from "../shared/workspace-open-targets";
import {
  BUNDLED_GHOSTEX_AGENT_SKILLS,
  type BundledGhostexAgentSkillId,
} from "../shared/ghostex-agent-skills";
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
  isSidebarCommandConfigured,
  type SidebarActionType,
  type SidebarCommandButton,
  type SidebarCommandLink,
} from "../shared/sidebar-commands";
import {
  DEFAULT_SIDEBAR_COMMAND_ICON,
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
import { PET_CONTROLS_VISIBLE, PET_OPTIONS, type PetId } from "../shared/pets";
import {
  areSidebarSessionTagListItemsEqual,
  DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS,
  getSidebarSessionTagListItemLabel,
  normalizeSidebarSessionTagListItems,
  type SidebarSessionTagListItem,
} from "../shared/session-tags";
import type {
  NativePortlessAdminAction,
  NativePortlessAdminInstallAction,
} from "../shared/native-ghostty-host-protocol";
import { getBrandAgentLogoStyle } from "./agent-logos";
import { EditorBrandIcon, getEditorBrandIconId } from "./brand-icons";
import { BundledAgentSkillsPanel } from "./bundled-agent-skills-panel";
import { HotkeyRecorderField } from "./hotkey-recorder-field";
import { PetAvatar } from "./pet-avatar";
import { CommandIconPicker } from "./command-icon-picker";
import { SidebarCommandIconGlyph } from "./sidebar-command-icon";
import { SessionTagIcon } from "./session-tag-ui";
import { useSidebarStore } from "./sidebar-store";
import type { AgentConfigDraft } from "./agent-config-modal";
import type { WebviewApi } from "./webview-api";
import packageJson from "../package.json";

export type { SettingsModalTab } from "./settings-modal-tabs";

const GHOSTEX_DISCORD_URL = "https://discord.gg/df7b3G92CS";
const GHOSTEX_GITHUB_URL = "https://github.com/maddada/Ghostex";
const GHOSTEX_SPONSOR_URL = "https://github.com/sponsors/maddada";
const IS_WINDOWS_HOST =
  typeof navigator !== "undefined" && /Windows/iu.test(navigator.userAgent);
const NUMERIC_SETTINGS_DEBOUNCE_MS = 180;
const SETTINGS_MODAL_NAVIGATION_SCROLL_DEBOUNCE_MS = 220;
const GHOSTTY_THEME_UNMANAGED_VALUE = "__ghostex_ghostty_theme_unmanaged__";
const MODIFIED_SETTING_TOOLTIP = "Modified Setting.\n \nClick to Reset to Default";
const PASTE_PREVIEWABLE_IMAGES_DESCRIPTION =
  "Paste clipboard images as previewable Markdown links with Cmd+V or Ctrl+V. Hold Cmd over the linked path to preview it in the terminal, and see the same image preview in the Ctrl+G Rich Prompt Editor.";

/*
 * CDXC:SettingsTextFields 2026-06-15-18:19:
 * Settings text fields hold explicit configuration values, including Remote SSH names, users, hosts, ports, identity files, commands, and prompts. Disable browser and macOS text assistance at the Settings modal field boundary so autocomplete, autocorrect, capitalization, and spellcheck cannot rewrite user-entered configuration.
 */
function SettingsInput({
  autoCapitalize = "none",
  autoComplete = "off",
  autoCorrect = "off",
  spellCheck = false,
  ...props
}: ComponentProps<"input">) {
  return (
    <BaseInput
      autoCapitalize={autoCapitalize}
      autoComplete={autoComplete}
      autoCorrect={autoCorrect}
      spellCheck={spellCheck}
      {...props}
    />
  );
}

function SettingsTextarea({
  autoCapitalize = "none",
  autoComplete = "off",
  autoCorrect = "off",
  spellCheck = false,
  ...props
}: ComponentProps<"textarea">) {
  return (
    <BaseTextarea
      autoCapitalize={autoCapitalize}
      autoComplete={autoComplete}
      autoCorrect={autoCorrect}
      spellCheck={spellCheck}
      {...props}
    />
  );
}

function SettingsSelect({
  disabled,
  disabledReason,
  disabledTooltipClassName,
  onOpenChange,
  onValueChange,
  ...props
}: ComponentProps<typeof Select> & {
  disabledReason?: string;
  disabledTooltipClassName?: string;
}) {
  const [selectOpen, setSelectOpen] = useState(false);

  useEffect(() => {
    if (disabled && selectOpen) {
      setSelectOpen(false);
    }
  }, [disabled, selectOpen]);

  const closeSelect = () => {
    flushSync(() => {
      setSelectOpen(false);
    });
  };

  /*
   * CDXC:SettingsDropdowns 2026-06-19-19:22:
   * Settings select changes save immediately through the native modal host.
   * Close every Base UI popup before posting the setting update so portaled
   * dropdowns, including Default Prompt Agent and command editor selects,
   * cannot keep their modal focus trap alive while gxserver and native settings
   * hydration re-render the dialog.
   */
  const select = (
    <Select
      {...props}
      disabled={disabled}
      onOpenChange={(nextOpen, eventDetails) => {
        setSelectOpen(nextOpen);
        onOpenChange?.(nextOpen, eventDetails);
      }}
      onValueChange={(nextValue) => {
        closeSelect();
        onValueChange?.(nextValue);
      }}
      open={selectOpen}
    />
  );

  return (
    <DisabledSettingControlTooltip
      className={disabledTooltipClassName}
      disabled={disabled === true}
      reason={disabledReason}
    >
      {select}
    </DisabledSettingControlTooltip>
  );
}

function SettingButton({
  disabledReason,
  disabledTooltipClassName,
  ...props
}: ComponentProps<typeof Button> & {
  disabledReason: string;
  disabledTooltipClassName?: string;
}) {
  const disabled = props.disabled === true;
  return (
    <DisabledSettingControlTooltip
      className={disabledTooltipClassName}
      disabled={disabled}
      reason={disabledReason}
    >
      <Button {...props} />
    </DisabledSettingControlTooltip>
  );
}

function SettingSwitch({
  disabledReason,
  ...props
}: ComponentProps<typeof Switch> & {
  disabledReason: string;
}) {
  const disabled = props.disabled === true;
  return (
    <DisabledSettingControlTooltip disabled={disabled} reason={disabledReason}>
      <Switch {...props} />
    </DisabledSettingControlTooltip>
  );
}

function SettingsSelectContent({
  className,
  ...props
}: ComponentProps<typeof SelectContent>) {
  /*
   * CDXC:SettingsDropdowns 2026-06-16-16:58:
   * Settings Select popups are portaled outside the Settings dialog subtree.
   * Carry a stable class on the popup so row hover, focus, and selected states
   * can stay neutral gray instead of inheriting saturated app accent styling.
   */
  return <SelectContent className={cn("settings-select-content", className)} {...props} />;
}

function getHotkeySettingsSectionId(
  definition: (typeof GHOSTEX_HOTKEY_DEFINITIONS)[number],
): HotkeySettingsSectionId {
  switch (definition.action.kind) {
    case "focusedPaneAction":
    case "renameActiveSession":
    case "splitFocusedPane":
    case "terminalToolbarAction":
      return "paneActions";
    case "focusAdjacentGroup":
    case "focusDirection":
      return "navigation";
    case "focusSessionSlot":
      return definition.action.slotNumber > 0 ? "sessionSlots" : "navigation";
    case "jumpToProject":
      return "projects";
    case "runActionSlot":
      return "actions";
    default:
      return "general";
  }
}

const HOTKEY_SETTINGS_SECTIONS: readonly HotkeySettingsSectionDefinition[] = (
  [
    { id: "general", title: "General" },
    { id: "paneActions", title: "Pane Actions" },
    { id: "navigation", title: "Navigation" },
    { id: "projects", title: "Projects" },
    { id: "sessionSlots", title: "Session Slots" },
    { id: "actions", title: "Actions" },
  ] as const
).map((section) => ({
  ...section,
  /*
   * Settings is a view of the canonical hotkey catalog, not a second catalog.
   * Deriving each section prevents newly registered or unassigned actions from
   * silently disappearing until this modal's former hand-maintained ID lists
   * are updated separately.
   */
  ids: GHOSTEX_HOTKEY_DEFINITIONS.filter(
    (definition) => getHotkeySettingsSectionId(definition) === section.id,
  ).map((definition) => definition.id),
}));

type SettingSearchDefinition = {
  advanced?: boolean;
  key: string;
  options?: ReadonlyArray<{ label: string; value: string }>;
  subtitle?: string;
  title: string;
};

type SettingsSectionSearchResult = {
  groupTitleMatches?: boolean;
  isSearching: boolean;
  sectionMatches: boolean;
  visibleSettingKeys: Set<string>;
};

type SettingsSectionNavigationItem<SectionId extends string> = {
  id: SectionId;
  title: string;
};

type SettingsSectionMeasurementItem<SectionId extends string> = {
  id: SectionId;
  ref: RefObject<HTMLDivElement | null>;
};

type SettingsSidebarPageSection = {
  active: boolean;
  id: string;
  onSelect: () => void;
  /*
   * CDXC:SettingsNavigation 2026-08-19:
   * General groups several rendered section headers each ("Tools" holds
   * Browser, Editor, and Dev Servers), and those headers had no rail entry at
   * all. They expand as a third level under the active group instead of being
   * promoted to top-level destinations, which would undo the deliberate
   * "fewer sidebar destinations" grouping.
   */
  subsections?: readonly SettingsSidebarPageSubsection[];
  title: string;
};

type SettingsSidebarPageSubsection = {
  active: boolean;
  id: string;
  onSelect: () => void;
  title: string;
};

type SettingsSidebarPage = {
  icon: typeof IconSettings;
  id: SettingsModalTab;
  sections?: readonly SettingsSidebarPageSection[];
  title: string;
};

type HotkeySettingsDefinitionById = ReadonlyMap<
  ghostexHotkeyActionId,
  (typeof GHOSTEX_HOTKEY_DEFINITIONS)[number]
>;

type HotkeySettingsSectionRefs = Record<
  HotkeySettingsSectionId,
  RefObject<HTMLDivElement | null>
>;

type HotkeySettingsSectionSearches = Record<
  HotkeySettingsSectionId,
  SettingsSectionSearchResult
>;

type SettingModificationProps = {
  advanced?: boolean;
  isModified?: boolean;
  onResetToDefault?: () => void;
};

type MainSettingsSectionId =
  | "agents"
  | "appearance"
  | "chat"
  | "sidebar"
  | "terminal"
  | "tools"
  | "statusIndicators"
  | "notifications"
  | "system"
  | "advanced";

type MainSettingsScrollTargetId =
  | MainSettingsSectionId
  | "theming"
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon is an appearance section that sits next to Theming.
  | "appIcon"
  | "sidebarTags"
  | "sessionCards"
  | "debugging"
  | "terminalBehavior"
  | "terminalScrolling"
  | "terminalDevServers"
  | "builtInFeatures"
  | "browser"
  | "editor"
  | "autoSleep"
  | "power"
  | "sounds"
  | "storage"
  | "beta";

export type MainSettingsInitialSectionId = MainSettingsScrollTargetId;

type MainSettingsSectionRefs = Record<
  MainSettingsScrollTargetId,
  RefObject<HTMLDivElement | null>
>;

/*
 * CDXC:DebuggingSettings 2026-06-28-18:14:
 * Show debug UI controls is the visibility and routine-logging gate for the
 * support/debugging settings below it. When off, hide diagnostic scenario
 * logging and session context-menu debug utilities instead of leaving disabled
 * rows on screen.
 */
const DEBUGGING_MODE_DEPENDENT_SETTING_KEYS = [
  "diagnosticLogging",
  "showSessionCommandCopyActions",
  "showSessionDetailsCopyAction",
] as const;
const DEBUGGING_MODE_DEPENDENT_SETTING_KEY_SET = new Set<string>(
  DEBUGGING_MODE_DEPENDENT_SETTING_KEYS,
);

const MAIN_SETTINGS_SECTION_SETTING_KEYS: Record<
  MainSettingsSectionId,
  readonly string[]
> = {
  agents: ["agentAcceptAllEnabled"],
  /*
   * CDXC:SettingsNavigation 2026-06-30-01:23:
   * General Settings should expose fewer sidebar destinations. Group related
   * controls into user-facing sections while retaining internal subheadings and
   * legacy scroll targets for direct entries such as Power Settings.
   *
   * CDXC:SettingsNavigation 2026-06-30-01:23:
   * Notifications/Sounds and Status Indicators remain independent sections
   * instead of merging into Appearance because users distinguish audible or
   * system alerts from always-visible status surfaces.
   *
   * CDXC:SettingsNavigation 2026-06-30-10:35:
   * Settings should not expose a standalone Workspace section header or workspace sidebar destination. Active Pane Border belongs with General appearance tuning, Terminal Background and click-to-wake belong with Terminal controls, Command Pane Default Height belongs beside the other default size reset value, and Auto Sleep moves under System.
   */
  appearance: [
    "sidebarTheme",
    "customSidebarTitlebarBackgroundDarknessPercent",
    "customSidebarTitlebarBackgroundTintColor",
    "workspaceActivePaneBorderColor",
    "appIconSourceId",
  ],
  chat: [
    "preferredAgentInterface",
    "sessionChatTheme",
    "sessionChatFontFamily",
    "sessionChatTranscriptWidthPercent",
    "sessionChatVerboseMode",
  ],
  sidebar: [
    /*
     * CDXC:SidebarV2 2026-07-29:
     * Sidebar version stays near the top of General, ahead of its own V2-only
     * sub-settings.
     */
    "sidebarVersion",
    "sidebarV2Layout",
    "sidebarAutoSettleAfterDays",
    "sidebarSettingsPreset",
    /*
     * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
     * Every setting mutated by a sidebar preset must be visible directly below
     * the preset selector, even when Show Advanced is off. Keep preset-owned
     * session-card, project-stat, and menu-bar indicator controls in this
     * Sidebar group so users can inspect and tune exactly what a preset changed.
     *
     * CDXC:SidebarProjectStats 2026-06-16-02:14:
     * Project git-stat display controls belong with Sidebar settings because they change sidebar project rows, not editor behavior.
     * Use changed-file wording for the file-count toggle so it does not read like an editor-pane setting.
     */
    "showProjectIcons",
    "hideSessionAgentIconUntilHover",
    "hideBrowserFaviconUntilHover",
    "showCloseButtonOnSessionCards",
    "hideLastActiveTimeOnSessionCards",
    "hideProjectHeaderDiffStats",
    "showProjectEditorDiffFileCount",
    "hideMenuBarSessionStatusIndicators",
    "sidebarSide",
    "sidebarCollapseAnimationDurationMs",
    "sidebarDefaultWidthPx",
    "commandsPanelDefaultHeightPx",
    "commandsPanelSide",
    "projectSessionListCollapsedCount",
    "agentManagerZoomPercent",
    "createSessionOnSidebarDoubleClick",
    "renameSessionOnDoubleClick",
    "useColoredSessionAgentIcons",
    "showSessionCloseContextMenuAction",
    "sidebarSessionTagListItems",
  ],
  /*
   * CDXC:StatusIndicators 2026-05-20-12:00:
   * Status Indicators groups session presence surfaces that communicate status
   * at a glance.
   *
   * CDXC:StatusIndicators 2026-06-27-20:11:
   * The desktop floating session badge surface was removed from macOS and GPUI.
   *
   * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
   * The menu bar session indicator is preset-owned, so it now renders under
   * Sidebar next to the other preset-controlled rows. This section keeps the
   * floating pet settings without exposing the removed floating badge toggle or
   * size selector.
   */
  statusIndicators: [
    "petOverlayEnabled",
    "selectedPetId",
  ],
  terminal: [
    "ghosttySettingsActions",
    "terminalGhosttyTheme",
    "workspaceBackgroundColor",
    "terminalBackgroundImage",
    "terminalBackgroundImageOpacity",
    "terminalBackgroundImageFit",
    "terminalFontFamily",
    "terminalFontSize",
    "terminalFontWeight",
    "terminalLineHeight",
    "terminalLetterSpacing",
    "terminalPaneHorizontalPaddingPx",
    "terminalPaneVerticalPaddingPx",
    "terminalCursorStyle",
    "terminalCursorStyleBlink",
    "sessionPersistenceProvider",
    "clickToWakeSleepingSessions",
    "showSessionIdInTerminalPanes",
    "showNotificationOnTerminalBell",
    "promptEditorBackend",
    "terminalScrollbackLimitMb",
    "terminalCopyOnSelect",
    "terminalConfirmCloseSurface",
    "terminalClipboardTrimTrailingSpaces",
    "terminalClipboardPasteProtection",
    "terminalPastePreviewableImages",
    "terminalMouseHideWhileTyping",
    "terminalScrollbar",
    "terminalMouseScrollMultiplierPrecision",
    "terminalMouseScrollMultiplierDiscrete",
    "terminalScrollToBottomWhenTyping",
  ],
  tools: [
    "webLinkOpenTarget",
    "codeServerLinkVscodeUserConfig",
    "codeServerUseVscodeInsidersUserConfig",
    "showUntrackedProjectDiffWhenNoTrackedChanges",
    /*
     * CDXC:TerminalDevServers 2026-06-23-19:22:
     * Dev-server discovery preferences belong under Terminal settings because they govern terminal-output detection, while remaining separate from Ghostty config-backed terminal emulator controls.
     *
     * CDXC:WebLinkOpenTarget 2026-08-19:
     * Where a detected URL opens is no longer a Dev Servers row; it reads the Browser section's single web-link target.
     */
    "terminalDevServerDetectionEnabled",
    "terminalDevServerIgnoredPortRules",
  ],
  notifications: [
    "completionBellEnabled",
    "completionSound",
    "showMacOSAttentionNotifications",
    "attentionNotificationActions",
    "actionCompletionSound",
  ],
  system: [
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
    "hideKeepAwakeTitlebarControl",
    "keepAwakeDefaultDurationMinutes",
    "keepAwakeAllowDisplaySleep",
    "keepAwakePreventLidSleep",
    "keepAwakeActivateOnLaunch",
    "keepAwakeActivateOnExternalDisplay",
    "keepAwakeWhileWorkingSessions",
    "keepAwakeDeactivateBelowBatteryThreshold",
    "keepAwakeBatteryThresholdPercent",
    "keepAwakeDeactivateOnLowPowerMode",
    "keepAwakeDeactivateOnUserSwitch",
    "ghostexFolderStats",
  ],
  /*
   * CDXC:DebuggingSettings 2026-06-15-21:34:
   * Debugging controls belong in a dedicated bottom Settings section so support-oriented logging and session metadata copy actions are grouped away from everyday Workspace and Session Cards preferences.
   */
  advanced: [
    "showBetaFeatures",
    "debuggingMode",
    ...DEBUGGING_MODE_DEPENDENT_SETTING_KEYS,
  ],
};

const MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS = {
  ...MAIN_SETTINGS_SECTION_SETTING_KEYS,
  theming: [
    "sidebarTheme",
    "customSidebarTitlebarBackgroundDarknessPercent",
    "customSidebarTitlebarBackgroundTintColor",
  ],
  // CDXC:AppIconPicker 2026-06-25-21:50: App Icon owns the persisted Dock icon source id selection.
  appIcon: ["appIconSourceId"],
  sidebarTags: ["sidebarSessionTagListItems"],
  sessionCards: [
    "useColoredSessionAgentIcons",
    "showSessionCloseContextMenuAction",
  ],
  debugging: [
    "debuggingMode",
    ...DEBUGGING_MODE_DEPENDENT_SETTING_KEYS,
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
  terminalDevServers: [
    "terminalDevServerDetectionEnabled",
    "terminalDevServerIgnoredPortRules",
  ],
  builtInFeatures: [
    "codeViewTabHidden",
    "kanbanViewTabHidden",
    "automateViewTabHidden",
    "docsViewTabHidden",
  ],
  browser: ["webLinkOpenTarget"],
  editor: [
    "codeServerLinkVscodeUserConfig",
    "codeServerUseVscodeInsidersUserConfig",
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
    "keepAwakeWhileWorkingSessions",
    "keepAwakeDeactivateBelowBatteryThreshold",
    "keepAwakeBatteryThresholdPercent",
    "keepAwakeDeactivateOnLowPowerMode",
    "keepAwakeDeactivateOnUserSwitch",
  ],
  sounds: [
    "completionBellEnabled",
    "completionSound",
    "showMacOSAttentionNotifications",
    "attentionNotificationActions",
    "actionCompletionSound",
  ],
  storage: ["ghostexFolderStats"],
  beta: ["showBetaFeatures"],
} satisfies Record<MainSettingsScrollTargetId, readonly string[]>;

type MainSettingsSubsectionId =
  | "appIcon"
  | "autoSleep"
  | "beta"
  | "browser"
  | "debugging"
  | "editor"
  | "power"
  | "sessionCards"
  | "sidebar"
  | "sidebarTags"
  | "storage"
  | "terminal"
  | "terminalBehavior"
  | "terminalDevServers"
  | "terminalScrolling"
  | "theming";

type MainSettingsSubsectionNavigationItem = {
  id: MainSettingsSubsectionId;
  title: string;
};

/*
 * CDXC:SettingsNavigation 2026-08-19:
 * The rail rows for each General group, in the order the sections render on the
 * page. A group's own anchor is listed first so its header (Browser under
 * Tools, Terminal under Terminal) is reachable by name rather than only as the
 * side effect of clicking the group. Groups with a single section stay flat and
 * are omitted here.
 */
const MAIN_SETTINGS_SUBSECTION_NAVIGATION: Partial<
  Record<MainSettingsSectionId, readonly MainSettingsSubsectionNavigationItem[]>
> = {
  advanced: [
    { id: "beta", title: "Experimental" },
    { id: "debugging", title: "Debugging" },
  ],
  appearance: [
    { id: "theming", title: "Theming" },
    { id: "appIcon", title: "App Icon" },
  ],
  sidebar: [
    { id: "sidebar", title: "Sidebar" },
    { id: "sessionCards", title: "Session Cards" },
    { id: "sidebarTags", title: "Sidebar Tags" },
  ],
  system: [
    { id: "autoSleep", title: "Auto Sleep" },
    { id: "power", title: "Power" },
    { id: "storage", title: "Storage" },
  ],
  terminal: [
    { id: "terminal", title: "Terminal" },
    { id: "terminalBehavior", title: "Terminal Behavior" },
    { id: "terminalScrolling", title: "Terminal Scrolling" },
  ],
  tools: [
    { id: "browser", title: "Browser" },
    { id: "editor", title: "Editor" },
    { id: "terminalDevServers", title: "Dev Servers" },
  ],
};

const MAIN_SETTINGS_SUBSECTION_PARENT_IDS: Partial<
  Record<MainSettingsScrollTargetId, MainSettingsSectionId>
> = Object.fromEntries(
  (
    Object.entries(MAIN_SETTINGS_SUBSECTION_NAVIGATION) as Array<
      [MainSettingsSectionId, readonly MainSettingsSubsectionNavigationItem[]]
    >
  ).flatMap(([sectionId, subsections]) =>
    subsections.map((subsection) => [subsection.id, sectionId] as const),
  ),
);

/*
 * CDXC:SettingsNavigation 2026-08-19:
 * Scroll tracking now reports the exact section header in view so a nested row
 * can highlight itself. The rail's top-level row still highlights by group, so
 * map the tracked anchor back to the group that owns it.
 */
function getMainSettingsSectionGroupId(
  scrollTargetId: MainSettingsScrollTargetId,
): MainSettingsSectionId {
  return (
    MAIN_SETTINGS_SUBSECTION_PARENT_IDS[scrollTargetId] ??
    (scrollTargetId as MainSettingsSectionId)
  );
}

/**
 * CDXC:SidebarSessionRename 2026-06-26-06:27:
 * The double-click rename setting must disclose that enabling it makes single-click session selection respond a bit slower because the card waits for a possible second click before treating the gesture as normal selection.
 *
 * CDXC:SidebarSessionRename 2026-06-28-02:24:
 * The click-delay disclosure should render as a Settings row subtitle below the primary label instead of being embedded in parentheses in the label text, so the control title stays scannable while the tradeoff remains visible.
 */
const RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL = "Double-click session cards to rename";
const RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE =
  "Makes clicking on a session respond a bit slower so we can detect the double click";

type DiagnosticLoggingDurationValue = "off" | "15m" | "1h" | "always";

const DIAGNOSTIC_LOGGING_DURATION_OPTIONS: ReadonlyArray<{
  label: string;
  value: DiagnosticLoggingDurationValue;
}> = [
  { label: "Off", value: "off" },
  { label: "15 min", value: "15m" },
  { label: "1 hour", value: "1h" },
  { label: "Always", value: "always" },
];

const DEFAULT_DIAGNOSTIC_LOGGING_ENABLE_DURATION: DiagnosticLoggingDurationValue = "1h";
const DIAGNOSTIC_LOGGING_GROUPS: readonly ["macOS", "GPUI", "gxserver"] = [
  "macOS",
  "GPUI",
  "gxserver",
];

/*
 * CDXC:SettingsAdvanced 2026-06-16-01:35:
 * The first Settings page should default to everyday controls and hide precision tuning, support/debug toggles, context-menu utilities, and provider-specific terminal options until users enable Show Advanced. Search still exposes matching advanced controls so discoverability is not tied to browsing mode.
 *
 * CDXC:SettingsAdvanced 2026-06-16-01:53:
 * Superseded by CDXC:SettingsNavigation 2026-06-19-08:40.
 *
 * CDXC:SettingsNavigation 2026-06-19-08:40:
 * Show Advanced changes the density of the General Settings page, but the macOS Settings UI should still present it inside the same left sidebar as the section navigation rather than as separate header or footer chrome.
 *
 * CDXC:SettingsAdvanced 2026-06-16-08:12:
 * Browser feedback, Storage, session-card chrome, Workspace tuning, and Terminal Behavior controls are advanced-only browsing rows because the default General page should stay focused on common setup and daily preferences.
 *
 * CDXC:SidebarSettingsPresets 2026-06-30-22:22:
 * Preset-owned sidebar chrome is no longer advanced-only because users need to
 * see every setting a preset changes directly below the preset selector.
 *
 * CDXC:SettingsTheming 2026-06-16-08:58:
 * Theming controls should remain visible without Show Advanced. Do not mark Theme, Background Contrast, or Background Tint as advanced rows.
 *
 * CDXC:SettingsAdvanced 2026-06-16-09:20:
 * Empty-sidebar double-click creation remains a low-frequency interaction preference and should hide behind Show Advanced. The menu-bar indicator is preset-owned and stays beside the sidebar preset controls.
 *
 * CDXC:PetControlsVisibility 2026-07-21:
 * Wake Pet and the Pet picker are temporarily hidden from Settings while their
 * implementation and persisted values remain available for a possible return.
 *
 * CDXC:ExperimentalFeatures 2026-06-28-07:41:
 * Enable Experimental Features is the user-facing name for the persisted
 * showBetaFeatures gate. Show Advanced is a persisted browsing-density
 * preference, so keep the experimental gate hidden from ordinary settings
 * browsing until users enable advanced density or search for it.
 *
 * CDXC:SettingsAdvanced 2026-06-28-08:01:
 * Show Advanced persists as a Settings preference so advanced rows stay visible
 * after restart until the user disables the switch.
 *
 * CDXC:SettingsAdvanced 2026-06-16-18:19:
 * Hide last-active timestamps, completion sounds, macOS attention notification, action-completion sound, Sidebar Tags, and the sidebar interface-size slider are common preferences. Keep them visible without Show Advanced while leaving terminal, debugging, storage, and lower-frequency utility controls advanced.
 *
 */
const ADVANCED_MAIN_SETTING_KEYS = new Set<string>([
  "sidebarDefaultWidthPx",
  "projectSessionListCollapsedCount",
  "createSessionOnSidebarDoubleClick",
  "showSessionCloseContextMenuAction",
  "workspaceActivePaneBorderColor",
  "workspaceBackgroundColor",
  "terminalBackgroundImage",
  "terminalBackgroundImageOpacity",
  "terminalBackgroundImageFit",
  "clickToWakeSleepingSessions",
  "commandsPanelDefaultHeightPx",
  "ghosttySettingsActions",
  "terminalFontWeight",
  "terminalLineHeight",
  "terminalLetterSpacing",
  "terminalCursorStyleBlink",
  "showSessionIdInTerminalPanes",
  "promptEditorBackend",
  "terminalScrollbackLimitMb",
  "terminalCopyOnSelect",
  "terminalConfirmCloseSurface",
  "terminalClipboardTrimTrailingSpaces",
  "terminalClipboardPasteProtection",
  "terminalPastePreviewableImages",
  "terminalMouseHideWhileTyping",
  "terminalScrollbar",
  "terminalMouseScrollMultiplierPrecision",
  "terminalMouseScrollMultiplierDiscrete",
  "terminalScrollToBottomWhenTyping",
  "codeServerUseVscodeInsidersUserConfig",
  "codeServerLinkVscodeUserConfig",
  /*
   * CDXC:AppIconPicker 2026-06-28-06:05:
   * Custom Dock icons are advanced appearance personalization. Keep the control searchable, but hide it from normal Settings browsing and place it below Editor so it does not compete with daily sidebar/theme controls.
   */
  "appIconSourceId",
  "showUntrackedProjectDiffWhenNoTrackedChanges",
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
  "hideKeepAwakeTitlebarControl",
  "keepAwakeDefaultDurationMinutes",
  "keepAwakeAllowDisplaySleep",
  "keepAwakePreventLidSleep",
  "keepAwakeActivateOnLaunch",
  "keepAwakeActivateOnExternalDisplay",
  "keepAwakeWhileWorkingSessions",
  "keepAwakeDeactivateBelowBatteryThreshold",
  "keepAwakeBatteryThresholdPercent",
  "keepAwakeDeactivateOnLowPowerMode",
  "keepAwakeDeactivateOnUserSwitch",
  "attentionNotificationActions",
  "ghostexFolderStats",
  "showBetaFeatures",
  "debuggingMode",
  "diagnosticLogging",
  "showSessionCommandCopyActions",
  "showSessionDetailsCopyAction",
]);

type HotkeySettingsSectionId =
  | "general"
  | "paneActions"
  | "navigation"
  | "projects"
  | "sessionSlots"
  | "actions";

type HotkeySettingsSectionDefinition = {
  ids: readonly ghostexHotkeyActionId[];
  id: HotkeySettingsSectionId;
  title: string;
};

let rememberedSettingsModalTab: SettingsModalTab | undefined;
const rememberedSettingsModalScrollTopByTab: Partial<Record<SettingsModalTab, number>> = {};

/*
 * CDXC:SettingsPerformance 2026-06-29-00:40:
 * Settings must keep app-session tab and scroll memory, but the main SettingsModal render needs React Compiler coverage so scroll-section highlight updates do not re-render the whole long settings page.
 * Keep the mutable session memory behind helpers so SettingsModal does not directly reassign module variables and the compiler can memoize the large render tree.
 */
function getRememberedSettingsModalTab(
  storedNavigation: SettingsModalNavigationState,
): SettingsModalTab | undefined {
  return rememberedSettingsModalTab ?? storedNavigation.activeTab;
}

function rememberSettingsModalTab(tab: SettingsModalTab): void {
  rememberedSettingsModalTab = tab;
}

function getRememberedSettingsModalScrollTop(
  tab: SettingsModalTab,
  storedNavigation: SettingsModalNavigationState,
): number {
  return rememberedSettingsModalScrollTopByTab[tab] ?? storedNavigation.scrollTopByTab[tab] ?? 0;
}

function rememberSettingsModalScrollTop(tab: SettingsModalTab, scrollTop: number): void {
  rememberedSettingsModalScrollTopByTab[tab] = scrollTop;
}

function getRememberedSettingsModalNavigationState(
  activeTab: SettingsModalTab,
  storedNavigation: SettingsModalNavigationState,
): SettingsModalNavigationState {
  return normalizeSettingsModalNavigationState({
    activeTab,
    scrollTopByTab: {
      ...storedNavigation.scrollTopByTab,
      ...rememberedSettingsModalScrollTopByTab,
    },
  });
}

function areSettingsModalNavigationStatesEqual(
  left: SettingsModalNavigationState,
  right: SettingsModalNavigationState,
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

/*
 * CDXC:SettingsPerformance 2026-06-29-00:40:
 * Settings management rows still need dnd-kit to register one element as both sortable item and drag source, but the row components need React Compiler coverage.
 * Keep the callback-ref mutation behind this helper so render code does not directly invoke ref-named mutators.
 */
function setSettingsSortableRowElement(
  sortableRefs: Pick<ReturnType<typeof useSortable>, "ref" | "sourceRef">,
  element: HTMLDivElement | null,
): void {
  sortableRefs.ref(element);
  sortableRefs.sourceRef(element);
}

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

function getMostlyVisibleSettingsSectionId<SectionId extends string>(
  viewport: HTMLElement,
  sections: readonly SettingsSectionMeasurementItem<SectionId>[],
): SectionId | undefined {
  /*
   * CDXC:SettingsNavigation 2026-06-15-22:28:
   * Settings and Hotkeys section sidebars must track the section that occupies
   * the largest share of the scroll viewport so the highlighted nav item
   * follows reading position while users scroll long settings pages.
   */
  const viewportRect = viewport.getBoundingClientRect();
  const viewportCenter = viewportRect.top + viewportRect.height / 2;
  let bestSection:
    | {
        centerDistance: number;
        id: SectionId;
        visibleHeight: number;
      }
    | undefined;

  for (const section of sections) {
    const element = section.ref.current;
    if (!element) {
      continue;
    }

    const sectionRect = element.getBoundingClientRect();
    const visibleHeight = Math.max(
      0,
      Math.min(sectionRect.bottom, viewportRect.bottom) - Math.max(sectionRect.top, viewportRect.top),
    );
    if (visibleHeight <= 0) {
      continue;
    }

    const sectionCenter = sectionRect.top + sectionRect.height / 2;
    const centerDistance = Math.abs(sectionCenter - viewportCenter);
    if (
      !bestSection ||
      visibleHeight > bestSection.visibleHeight ||
      (visibleHeight === bestSection.visibleHeight && centerDistance < bestSection.centerDistance)
    ) {
      bestSection = { centerDistance, id: section.id, visibleHeight };
    }
  }

  return bestSection?.id;
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
  onUninstallAgentHooks?: () => void;
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
     * CDXC:AgentHookSettings 2026-06-29-01:26:
     * Integrations still requests hook status for the bottom hook-removal recovery card, but hook installation and per-agent hook status now belong in Settings -> Agents.
     *
     * CDXC:ComputerAgentControl 2026-05-27-06:58:
     * Settings should present the public skill names Ghostex Browser Use and Ghostex Computer Use.
     */
    if (activeTab === "integrations" && !agentHookStatus && !agentHookStatusLoading) {
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
          "Show experimental surfaces: OS Integration settings, Browser Profiles, Browser color scheme, and Keep Awake.",
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
    { icon: IconCodeDots, id: "agents", title: "Agents" },
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
                          ? "Show experimental settings, Automations and Automate pages, browser address-bar controls, and the Keep Awake title-bar button."
                          : "Show experimental settings, Automations Overview, browser address-bar controls, and the Keep Awake title-bar button."
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
                        <li>Browser address bar: Profiles</li>
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
              agentHookStatus={agentHookStatus}
              agentHookStatusLoading={agentHookStatusLoading}
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
              onInstallFable56OrchestrationSkill={onInstallFable56OrchestrationSkill}
              onInstallFindPrevSessionSkill={onInstallFindPrevSessionSkill}
              onInstallGenerateTitleSkill={onInstallGenerateTitleSkill}
              onInstallGhostexCli={onInstallGhostexCli}
              onInstallMoveCodexSessionSkill={onInstallMoveCodexSessionSkill}
              onUninstallAgentHooks={onUninstallAgentHooks}
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

function AboutSettingsTab({
  search,
  searchEmptyState,
  vscode,
}: {
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const links = [
    {
      description: "Chat with the community and get help.",
      label: "Join Discord",
      url: GHOSTEX_DISCORD_URL,
    },
    {
      description: "View the source, releases, and report issues.",
      label: "View on GitHub",
      url: GHOSTEX_GITHUB_URL,
    },
    {
      description: "Support the continued development of Ghostex.",
      label: "Sponsor Ghostex",
      url: GHOSTEX_SPONSOR_URL,
    },
  ] as const;

  if (search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <SettingsNativeScrollArea
        className="settings-main-scroll"
        viewportClassName="settings-native-scroll-viewport"
      >
        <div className="settings-page-width px-5 py-5">{searchEmptyState}</div>
      </SettingsNativeScrollArea>
    );
  }

  return (
    <SettingsNativeScrollArea
      className="settings-main-scroll"
      viewportClassName="settings-native-scroll-viewport"
    >
      <div className="settings-about-page settings-page-width">
        <header className="settings-about-header">
          <div className="settings-about-mark" aria-hidden="true">G</div>
          <div>
            <h2 className="settings-about-title">Ghostex</h2>
            <p className="settings-about-version">Version {packageJson.version}</p>
          </div>
        </header>
        <p className="settings-about-description">
          A workspace for building with coding agents.
        </p>
        <div className="settings-about-links">
          {links.map((link) => (
            <a
              className="settings-about-link"
              href={link.url}
              key={link.label}
              onClick={(event) => {
                if (!vscode) {
                  return;
                }
                event.preventDefault();
                vscode.postMessage({ type: "openExternalUrl", url: link.url });
              }}
              rel="noreferrer"
              target="_blank"
            >
              <span className="settings-about-link-copy">
                <span className="settings-about-link-title">{link.label}</span>
                <span className="settings-about-link-description">{link.description}</span>
              </span>
              <IconExternalLink aria-hidden="true" size={16} />
            </a>
          ))}
        </div>
      </div>
    </SettingsNativeScrollArea>
  );
}

function SettingsNativeScrollArea({
  children,
  className,
  onScrollCapture,
  viewportClassName,
  ...props
}: ComponentProps<"div"> & {
  viewportClassName?: string;
}) {
  return (
    <div {...props} className={cn("relative", className)} data-slot="scroll-area">
      {/*
       * CDXC:SettingsPerformance 2026-06-29-00:40:
       * Settings pages must scroll with native overflow instead of Base UI
       * ScrollArea because long pages do not need custom scrollbar metrics or
       * scroll-linked edge masks on every frame. Keep the viewport data-slot so
       * existing section tracking and padding CSS continue to target the
       * scrollable element.
       */}
      <div
        className={cn(
          "settings-native-scroll-viewport size-full overflow-x-hidden overflow-y-auto rounded-none outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1",
          viewportClassName,
        )}
        data-slot="scroll-area-viewport"
        onScrollCapture={onScrollCapture}
      >
        {children}
      </div>
    </div>
  );
}

type SettingsAgentEditorState = {
  draft: AgentConfigDraft;
};

type SettingsCommandEditorState = {
  draft: SettingsCommandDraft;
  lockedActionType?: SidebarActionType;
};

type SettingsCommandDraft = {
  actionType: SidebarActionType;
  closeTerminalOnExit: boolean;
  command?: string;
  commandId?: string;
  icon?: SidebarCommandIcon;
  links?: SidebarCommandLink[];
  name: string;
  playCompletionSound: boolean;
  showOnProjectRow: boolean;
  url?: string;
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
  wslDistribution: string;
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
    wslDistribution: "",
  };
}

function createRemoteMachineDraftFromSettings(
  machine: RemoteMachineSettings,
  sshPassword = "",
): RemoteMachineDraft {
  return {
    id: machine.id,
    name: machine.name,
    sshHost: machine.sshHost,
    sshIdentityFile: machine.sshIdentityFile ?? "",
    sshPassword,
    sshPasswordSaved: machine.sshPasswordSaved === true,
    sshPort: machine.sshPort ? String(machine.sshPort) : "",
    sshUser: machine.sshUser ?? "",
    wslDistribution: machine.wslDistribution ?? "",
  };
}

function applyRemoteMachineDraftPatch(
  draft: RemoteMachineDraft,
  patch: Partial<RemoteMachineDraft>,
): RemoteMachineDraft {
  return {
    ...draft,
    name: patch.name !== undefined ? patch.name : draft.name,
    sshHost: patch.sshHost !== undefined ? patch.sshHost : draft.sshHost,
    sshIdentityFile:
      patch.sshIdentityFile !== undefined ? patch.sshIdentityFile : draft.sshIdentityFile,
    sshPassword: patch.sshPassword !== undefined ? patch.sshPassword : draft.sshPassword,
    sshPasswordSaved:
      patch.sshPasswordSaved !== undefined ? patch.sshPasswordSaved : draft.sshPasswordSaved,
    sshPort: patch.sshPort !== undefined ? patch.sshPort : draft.sshPort,
    sshUser: patch.sshUser !== undefined ? patch.sshUser : draft.sshUser,
    wslDistribution:
      patch.wslDistribution !== undefined ? patch.wslDistribution : draft.wslDistribution,
  };
}

function RemoteSettingsTab({
  initialRemoteMachineId,
  isActive,
  onChange,
  remoteMachines,
  search,
  searchEmptyState,
  vscode,
}: {
  initialRemoteMachineId?: string;
  isActive: boolean;
  onChange: (remoteMachines: RemoteMachineSettings[]) => void;
  remoteMachines: RemoteMachineSettings[];
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [isTailscaleHelpOpen, setIsTailscaleHelpOpen] = useState(false);
  const [newMachine, setNewMachine] = useState<RemoteMachineDraft>(() => createRemoteMachineDraft());
  const [remoteMachineDraftsById, setRemoteMachineDraftsById] = useState<
    Record<string, RemoteMachineDraft>
  >({});
  const [sshPasswordDrafts, setSshPasswordDrafts] = useState<Record<string, string>>({});
  const lastTargetedRemoteMachineIdRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    const remoteMachineIds = new Set(remoteMachines.map((machine) => machine.id));
    setRemoteMachineDraftsById((drafts) => {
      let next: Record<string, RemoteMachineDraft> | undefined;
      for (const machineId of Object.keys(drafts)) {
        if (!remoteMachineIds.has(machineId)) {
          next ??= { ...drafts };
          delete next[machineId];
        }
      }
      return next ?? drafts;
    });
  }, [remoteMachines]);

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

  const getRemoteMachineEditDraft = (machine: RemoteMachineSettings): RemoteMachineDraft => {
    const draft =
      remoteMachineDraftsById[machine.id] ??
      createRemoteMachineDraftFromSettings(machine, sshPasswordDrafts[machine.id] ?? "");
    return {
      ...draft,
      sshPassword: sshPasswordDrafts[machine.id] ?? draft.sshPassword,
      sshPasswordSaved: machine.sshPasswordSaved === true,
    };
  };

  const updateRemoteMachine = (machineId: string, patch: Partial<RemoteMachineDraft>) => {
    const currentMachine = remoteMachines.find((machine) => machine.id === machineId);
    if (!currentMachine) {
      return;
    }
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
      wslDistribution: patch.wslDistribution,
    };
    if (Object.values(settingsPatch).every((value) => value === undefined)) {
      return;
    }
    const nextDraft = applyRemoteMachineDraftPatch(
      getRemoteMachineEditDraft(currentMachine),
      patch,
    );
    setRemoteMachineDraftsById((drafts) => ({
      ...drafts,
      [machineId]: nextDraft,
    }));
    const normalizedMachine = normalizeRemoteMachineDraft(nextDraft);
    /*
     * CDXC:RemoteMachines 2026-07-01-00:45:
     * Saved-machine edit fields can be temporarily invalid while the user types.
     * Keep empty required name/host edits in local React draft state so deleting
     * the last character cannot remove the saved machine; only a valid draft or
     * the explicit trash action may change Settings.remoteMachines.
     */
    if (!normalizedMachine) {
      return;
    }
    const nextMachines = remoteMachines
      .map((machine) => {
        if (machine.id !== machineId) {
          return machine;
        }
        return normalizedMachine;
      })
      .filter((machine): machine is RemoteMachineSettings => Boolean(machine));
    onChange(normalizeRemoteMachineSettings(nextMachines));
  };

  const addRemoteMachine = () => {
    const machineId = `remote-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
    const password = newMachine.sshPassword;
    const machine = normalizeRemoteMachineDraft({
      ...newMachine,
      id: machineId,
    });
    if (!machine) {
      return;
    }
    /*
     * CDXC:RemoteMachines 2026-06-24-10:40:
     * The add-machine card must show the same password row as saved-machine
     * cards so a new machine and a created machine keep matching grid height.
     * If a create-time password is present, create the machine with a stable id
     * first and send that password as a one-shot Keychain save for the same id;
     * raw SSH passwords still never enter normalized settings.
     */
    onChange(normalizeRemoteMachineSettings([...remoteMachines, machine]));
    if (password.trim().length > 0) {
      postRemoteMachinePasswordSave(machine.id, password);
    }
    setNewMachine(createRemoteMachineDraft());
  };

  const removeRemoteMachine = (machineId: string) => {
    setRemoteMachineDraftsById((drafts) => {
      if (!(machineId in drafts)) {
        return drafts;
      }
      const next = { ...drafts };
      delete next[machineId];
      return next;
    });
    setSshPasswordDrafts((drafts) => {
      if (!(machineId in drafts)) {
        return drafts;
      }
      const next = { ...drafts };
      delete next[machineId];
      return next;
    });
    onChange(remoteMachines.filter((machine) => machine.id !== machineId));
  };

  const postRemoteMachinePasswordSave = (remoteMachineId: string, password: string) => {
    /*
     * CDXC:RemoteMachines 2026-06-09-18:23:
     * The Remote settings password field is a transient entry box. Send the
     * password only from explicit Add Machine or save-icon actions, then clear
     * the React draft so the settings JSON and modal state never retain the
     * secret.
     */
    vscode?.postMessage({
      password,
      remoteMachineId,
      type: "saveRemoteMachinePassword",
    });
  };

  const saveRemoteMachinePassword = (machine: RemoteMachineSettings) => {
    const password = sshPasswordDrafts[machine.id] ?? "";
    if (!password && machine.sshPasswordSaved !== true) {
      return;
    }
    postRemoteMachinePasswordSave(machine.id, password);
    setSshPasswordDrafts((drafts) => ({
      ...drafts,
      [machine.id]: "",
    }));
  };

  const canAddMachine = newMachine.name.trim().length > 0 && newMachine.sshHost.trim().length > 0;

  if (search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <div className="settings-tab-scroll" ref={containerRef}>
        <div className="settings-management-layout">{searchEmptyState}</div>
      </div>
    );
  }

  return (
    <div className="settings-tab-scroll" ref={containerRef}>
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
            <div className="settings-remote-machine-summary settings-remote-machine-add-summary settings-management-row">
              <span aria-hidden="true" className="settings-management-icon settings-remote-machine-add-icon">
                <IconPlus size={16} />
              </span>
              <span className="settings-management-main min-w-0 flex-1">
                <CardTitle className="settings-management-title">Add remote machine</CardTitle>
                <span className="settings-management-detail">New SSH machine</span>
              </span>
            </div>
            <CardContent className="settings-remote-machine-body">
              <RemoteMachineFields
                draft={newMachine}
                identityDescription="Provide either an SSH identity file now or an SSH password below."
                onChange={(patch) => setNewMachine((draft) => ({ ...draft, ...patch }))}
                passwordDescription="Passwords are stored in macOS Keychain. Leave blank to add the machine without a saved password."
              />
              <div className="settings-management-actions settings-remote-machine-add-actions">
                <SettingButton
                  disabled={!canAddMachine}
                  disabledReason="Enter a machine name and SSH host first."
                  onClick={addRemoteMachine}
                  type="button"
                >
                  <IconPlus aria-hidden="true" />
                  Add Machine
                </SettingButton>
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
            remoteMachines.map((machine) => {
              const machineDraft = getRemoteMachineEditDraft(machine);
              const summaryMachine = normalizeRemoteMachineDraft(machineDraft) ?? machine;
              return (
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
                      <span className="settings-management-title">{summaryMachine.name}</span>
                      <span className="settings-management-detail">
                        {formatRemoteMachineSshTarget(summaryMachine)}
                      </span>
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
                      draft={machineDraft}
                      onChange={(patch) => updateRemoteMachine(machine.id, patch)}
                      onPasswordSave={() => saveRemoteMachinePassword(machine)}
                      passwordSaveDisabled={!vscode}
                    />
                    {/*
                     * CDXC:RemoteMachines 2026-06-23-08:30:
                     * Remote Settings needs a direct gxserver install action for
                     * first-run Ubuntu SSH machines. Reuse the reconnect flow so
                     * native opens the approval modal only after SSH proves
                     * gxserver is missing, and otherwise connects the existing
                     * remote daemon without reinstalling it.
                     */}
                    <div className="settings-management-actions settings-remote-machine-install-actions">
                      <SettingButton
                        disabled={!vscode || !machineDraft.sshHost.trim()}
                        disabledReason={
                          !machineDraft.sshHost.trim()
                            ? "Enter an SSH host first."
                            : "This action needs the Ghostex app connection."
                        }
                        onClick={() => {
                          vscode?.postMessage({
                            remoteMachineId: machine.id,
                            type: "reconnectRemoteMachine",
                          });
                        }}
                        type="button"
                        variant="secondary"
                      >
                        <IconDownload aria-hidden="true" />
                        Install / Connect gxserver
                      </SettingButton>
                    </div>
                  </CardContent>
                </Card>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}

function RemoteMachineFields({
  draft,
  identityDescription,
  onChange,
  onPasswordSave,
  passwordSaveDisabled = false,
  passwordDescription,
}: {
  draft: RemoteMachineDraft;
  identityDescription?: string;
  onChange: (patch: Partial<RemoteMachineDraft>) => void;
  onPasswordSave?: () => void;
  passwordSaveDisabled?: boolean;
  passwordDescription?: string;
}) {
  const showPasswordSaveButton = typeof onPasswordSave === "function";
  const canSavePassword =
    !passwordSaveDisabled &&
    showPasswordSaveButton &&
    (draft.sshPassword.trim().length > 0 || draft.sshPasswordSaved);
  return (
    <FieldGroup className="settings-remote-machine-fields">
      <Field className="settings-remote-machine-field">
        <FieldLabel className="settings-remote-machine-field-label">Name</FieldLabel>
        <SettingsInput
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
        <SettingsInput
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
          <SettingsInput
            aria-label="Remote machine SSH user"
            className="settings-remote-machine-input"
            maxLength={120}
            onChange={(event) => onChange({ sshUser: event.currentTarget.value })}
            placeholder="machine username"
            value={draft.sshUser}
          />
        </Field>
        <Field className="settings-remote-machine-field">
          <FieldLabel className="settings-remote-machine-field-label">SSH port</FieldLabel>
          <SettingsInput
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
        <SettingsInput
          aria-label="Remote machine SSH identity file"
          className="settings-remote-machine-input"
          maxLength={500}
          onChange={(event) => onChange({ sshIdentityFile: event.currentTarget.value })}
          placeholder="~/.ssh/id_ed25519"
          value={draft.sshIdentityFile}
        />
        <FieldDescription className="settings-remote-machine-field-description">
          {identityDescription ?? "Provide either an SSH identity file or save an SSH password below."}
        </FieldDescription>
      </Field>
      <Field className="settings-remote-machine-field">
        <FieldLabel className="settings-remote-machine-field-label">
          Windows WSL distribution
        </FieldLabel>
        <SettingsInput
          aria-label="Remote machine WSL distribution"
          className="settings-remote-machine-input"
          maxLength={120}
          onChange={(event) => onChange({ wslDistribution: event.currentTarget.value })}
          placeholder="Ubuntu-24.04"
          value={draft.wslDistribution}
        />
        <FieldDescription className="settings-remote-machine-field-description">
          Optional. Windows remotes run gxserver inside this WSL2 distribution; leave blank to use
          the default distribution.
        </FieldDescription>
      </Field>
      <Field className="settings-remote-machine-field">
        <FieldLabel className="settings-remote-machine-field-label">Password</FieldLabel>
        <div
          className={cn(
            "settings-remote-machine-password-row",
            !showPasswordSaveButton && "settings-remote-machine-password-row-single",
          )}
        >
          <SettingsInput
            aria-label="Remote machine SSH password"
            autoComplete="off"
            className="settings-remote-machine-input"
            maxLength={500}
            onChange={(event) => onChange({ sshPassword: event.currentTarget.value })}
            placeholder={draft.sshPasswordSaved ? "Saved in Keychain" : "SSH password"}
            type="password"
            value={draft.sshPassword}
          />
          {showPasswordSaveButton ? (
            <SettingButton
              aria-label="Save SSH password"
              disabled={!canSavePassword}
              disabledReason={
                passwordSaveDisabled
                  ? "Password saving needs the Ghostex app connection."
                  : "Enter a password to save first."
              }
              onClick={onPasswordSave}
              size="icon-sm"
              type="button"
              variant="secondary"
            >
              <IconDeviceFloppy aria-hidden="true" />
            </SettingButton>
          ) : null}
        </div>
        <FieldDescription className="settings-remote-machine-field-description">
          {passwordDescription ??
            "Passwords are stored in macOS Keychain. Leave blank and press Save to remove a saved password."}
        </FieldDescription>
      </Field>
    </FieldGroup>
  );
}

function normalizeRemoteMachineDraft(
  draft: RemoteMachineDraft & { id: string },
): RemoteMachineSettings | undefined {
  const wslDistribution = draft.wslDistribution.trim();
  if (
    wslDistribution &&
    (wslDistribution.startsWith("-") ||
      !/^[A-Za-z0-9][A-Za-z0-9._+() -]*$/u.test(wslDistribution))
  ) {
    return undefined;
  }
  return normalizeRemoteMachineSettings([
    {
      id: draft.id,
      name: draft.name,
      sshHost: draft.sshHost,
      sshIdentityFile: draft.sshIdentityFile,
      sshPasswordSaved: draft.sshPasswordSaved,
      sshPort: draft.sshPort ? Number(draft.sshPort) : undefined,
      sshUser: draft.sshUser,
      wslDistribution,
    },
  ])[0];
}

function formatRemoteMachineSshTarget(machine: RemoteMachineSettings): string {
  const host = machine.sshUser ? `${machine.sshUser}@${machine.sshHost}` : machine.sshHost;
  return machine.sshPort ? `${host}:${machine.sshPort}` : host;
}

/*
 * CDXC:GlobalProjectDefaults 2026-08-02:
 * A project field is "inherited" only while the project's own value is empty and
 * a Global Default exists to take its place. The badge marks that state next to
 * the field name, and the caller shows the inherited value as the input's
 * placeholder so the effective value is visible without leaving the page.
 */
function InheritedSettingBadge() {
  return (
    <Tooltip>
      <TooltipTrigger render={<span className="settings-inherited-badge">Inherited</span>} />
      <TooltipContent sideOffset={6}>Using the Global Default set above</TooltipContent>
    </Tooltip>
  );
}

function inheritedPlaceholder(projectValue: string, globalValue: string, fallback: string): string {
  return projectValue.trim().length === 0 && globalValue.trim().length > 0
    ? globalValue
    : fallback;
}

function ProjectsSettingsPanel({
  onGlobalBeadsDirectoryChange,
  onGlobalBeadsDisplayKeyChange,
  onGlobalDocsDirectoryChange,
  onGlobalWorktreeCommandChange,
  onManageAdditionalDocsFoldersChange,
  onPortlessEnabledChange,
  onPortlessProtocolChange,
  portless,
  projects,
  search,
  searchEmptyState,
  settings,
  vscode,
}: {
  onGlobalBeadsDirectoryChange: (value: string) => void;
  onGlobalBeadsDisplayKeyChange: (value: string) => void;
  onGlobalDocsDirectoryChange: (value: string) => void;
  onGlobalWorktreeCommandChange: (value: string) => void;
  onManageAdditionalDocsFoldersChange: (value: string) => void;
  onPortlessEnabledChange: (checked: boolean) => void;
  onPortlessProtocolChange: (protocol: PortlessProtocol) => void;
  portless?: SidebarPortlessState;
  projects: SidebarProjectSettingsItem[];
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
  vscode?: WebviewApi;
}) {
  const projectSelectorLabelId = useId();
  const projectSelectorValueId = useId();
  const [selectedProjectId, setSelectedProjectId] = useState(projects[0]?.projectId ?? "");
  const [isProjectSelectorOpen, setIsProjectSelectorOpen] = useState(false);
  const [projectSelectorQuery, setProjectSelectorQuery] = useState("");
  const selectedProject =
    projects.find((project) => project.projectId === selectedProjectId) ?? projects[0];
  const [command, setCommand] = useState(selectedProject?.worktreeCommand ?? "");
  const [beadsDisplayKey, setBeadsDisplayKey] = useState(selectedProject?.beadsDisplayKey ?? "");
  const [beadsDirectory, setBeadsDirectory] = useState(selectedProject?.beadsDirectory ?? "");
  const [docsDirectory, setDocsDirectory] = useState(selectedProject?.docsDirectory ?? "");
  /*
   * CDXC:GlobalProjectDefaults 2026-08-02:
   * Track inheritance against the live draft text rather than the saved project
   * value so the badge disappears the moment the user starts typing an override
   * and returns when they clear the field again.
   */
  const isWorktreeCommandInherited =
    command.trim().length === 0 && settings.globalWorktreeCommand.trim().length > 0;
  const isBeadsDisplayKeyInherited =
    beadsDisplayKey.trim().length === 0 && settings.globalBeadsDisplayKey.trim().length > 0;
  const isBeadsDirectoryInherited =
    beadsDirectory.trim().length === 0 && settings.globalBeadsDirectory.trim().length > 0;
  const isDocsDirectoryInherited =
    docsDirectory.trim().length === 0 && settings.globalDocsDirectory.trim().length > 0;

  useEffect(() => {
    if (!projects.some((project) => project.projectId === selectedProjectId)) {
      setSelectedProjectId(projects[0]?.projectId ?? "");
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    setCommand(selectedProject?.worktreeCommand ?? "");
    setBeadsDisplayKey(selectedProject?.beadsDisplayKey ?? "");
    setBeadsDirectory(selectedProject?.beadsDirectory ?? "");
    setDocsDirectory(selectedProject?.docsDirectory ?? "");
  }, [
    selectedProject?.beadsDirectory,
    selectedProject?.beadsDisplayKey,
    selectedProject?.docsDirectory,
    selectedProject?.projectId,
    selectedProject?.worktreeCommand,
  ]);

  useEffect(() => {
    if (!isProjectSelectorOpen) {
      return undefined;
    }
    const frame = window.requestAnimationFrame(() => {
      document
        .querySelector<HTMLInputElement>(".projects-settings-selector-popover [data-slot='command-input']")
        ?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isProjectSelectorOpen]);

  const selectProject = (projectId: string) => {
    setSelectedProjectId(projectId);
    setIsProjectSelectorOpen(false);
    setProjectSelectorQuery("");
  };

  const runPortlessSettingsAdminAction = (action: NativePortlessAdminAction) => {
    const requestId = createPortlessSettingsAdminRequestId(action);
    if (action === "remove") {
      vscode?.postMessage({
        action,
        requestId,
        type: "runPortlessSettingsAdminAction",
      });
      return;
    }
    vscode?.postMessage({
      action,
      protocol: settings.portlessProtocol,
      requestId,
      type: "runPortlessSettingsAdminAction",
    });
  };

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

  const saveBeadsDirectory = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      directory: beadsDirectory,
      projectId: selectedProject.projectId,
      type: "setProjectBeadsDirectory",
    });
  };

  const saveDocsDirectory = () => {
    if (!selectedProject) {
      return;
    }
    vscode?.postMessage({
      directory: docsDirectory,
      projectId: selectedProject.projectId,
      type: "setProjectDocsDirectory",
    });
  };

  return (
    <div className="settings-tab-scroll">
      {/*
       * CDXC:PortlessSettings 2026-06-23-03:47:
       * Projects settings starts with global Portless controls because the
       * background proxy and HTTP/HTTPS mode are app-wide settings. Keep
       * generated project and worktree domains read-only here; slug editing and
       * reset actions belong to a later phase.
       *
       * CDXC:Worktrees 2026-05-18-23:07:
       * Main projects can store a setup command that runs inside every new worktree before the selected agent receives the first prompt. Keep worktree projects out of this list because they inherit from their parent project.
       */}
      <div className="projects-settings-layout">
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)
          ? searchEmptyState
          : null}
        {PORTLESS_SETTINGS_VISIBLE ? (
        <PortlessGlobalSettingsPanel
          domainSummaries={getProjectPortlessDomainSummaries(projects, selectedProject, portless)}
          onAdminAction={runPortlessSettingsAdminAction}
          onEnabledChange={onPortlessEnabledChange}
          onProtocolChange={onPortlessProtocolChange}
          portless={portless}
          settings={settings}
        />
        ) : null}
        {shouldShowSettingsSection(search.sections.docs) ? (
        <Card className="settings-project-command-card">
          <CardContent className="flex flex-col gap-4 p-4">
            {/*
              CDXC:DocsSidebar 2026-06-30-11:42:
              Docs folder scanning is a global Projects setting, not selected-project metadata. Keep it above the project selector and accept comma-separated project-relative folder names so entries like "plans, my documents, folders/folder name" scan matching folders under each project root.
              Give this card an explicit Docs title so users coming from the Docs sidebar shortcut know the folder list controls Docs file discovery.

              CDXC:DocsRootAdditive 2026-08-09:
              This list is project-relative again. A Docs directory adds its own whole tree beside these folders instead of being narrowed by them, so the copy must not imply the two interact.
            */}
            <div className="settings-management-header-text">
              <h3 className="settings-management-heading">Docs</h3>
              <p className="settings-management-description">
                Docs scans docs, artifacts, and ai by default. Add more project-relative folders
                here.
              </p>
            </div>
            <FieldGroup>
              <Field>
                <FieldLabel>Docs folders</FieldLabel>
                <SettingsInput
                  aria-label="Docs folders"
                  onChange={(event) => onManageAdditionalDocsFoldersChange(event.currentTarget.value)}
                  placeholder="plans, my documents, folders/folder name"
                  value={settings.manageAdditionalDocsFolders}
                />
                <FieldDescription>
                  Comma-separated project-relative folders to scan recursively in Docs. Spaces around folder names are ignored. Leave blank to scan docs/, artifacts/, and ai/ plus root Markdown, HTML, and Excalidraw files. A Docs directory set below adds its whole tree on top of this.
                </FieldDescription>
              </Field>
            </FieldGroup>
          </CardContent>
        </Card>
        ) : null}
        {shouldShowSettingsSection(search.sections.globalDefaults) ? (
        <Card className="settings-project-command-card">
          <CardContent className="flex flex-col gap-4 p-4">
            {/*
              CDXC:GlobalProjectDefaults 2026-08-02:
              Global Defaults sits above the project selector because it configures every project at once. Each field mirrors the per-project field of the same name below; a project keeps winning whenever its own value is non-empty, so filling nothing in here leaves every project resolving exactly as it did before.
            */}
            <div className="settings-management-header-text">
              <h3 className="settings-management-heading">Global Defaults</h3>
              <p className="settings-management-description">
                Applied to every project that does not set its own value below.
              </p>
            </div>
            <FieldGroup>
              <Field>
                <FieldLabel>Worktree command</FieldLabel>
                <SettingsTextarea
                  aria-label="Global worktree command"
                  className="settings-project-command-textarea"
                  onChange={(event) => onGlobalWorktreeCommandChange(event.currentTarget.value)}
                  placeholder="bun install"
                  value={settings.globalWorktreeCommand}
                />
                <FieldDescription>
                  Runs in every new worktree folder unless the project sets its own command.
                </FieldDescription>
              </Field>
            </FieldGroup>
            <FieldGroup>
              <Field>
                <FieldLabel>Ticket key</FieldLabel>
                <SettingsInput
                  aria-label="Global ticket key"
                  maxLength={3}
                  onChange={(event) =>
                    onGlobalBeadsDisplayKeyChange(
                      event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ""),
                    )
                  }
                  placeholder="ZMX"
                  value={settings.globalBeadsDisplayKey}
                />
                <FieldDescription>
                  Ticket prefix for every project board unless the project sets its own key.
                </FieldDescription>
              </Field>
            </FieldGroup>
            <FieldGroup>
              <Field>
                <FieldLabel>Beads directory</FieldLabel>
                <SettingsInput
                  aria-label="Global Beads directory"
                  onChange={(event) => onGlobalBeadsDirectoryChange(event.currentTarget.value)}
                  placeholder="/Users/you/code/my-repo"
                  value={settings.globalBeadsDirectory}
                />
                <FieldDescription>
                  Absolute path every Project board reads its Beads workspace (.beads) from unless the project sets its own directory. Leave blank to keep using each project root.
                </FieldDescription>
              </Field>
            </FieldGroup>
            {/*
              CDXC:DocsRootDirectory 2026-08-09:
              Docs can show any absolute folder, not only the project's own repo
              folder, so a notes vault is browsable from every project.

              CDXC:DocsRootAdditive 2026-08-09:
              That folder is ADDED to the project's own docs, never swapped in
              for them, and the Docs folders list above stays project-relative.
              Say so here, because "Docs directory" reads like a replacement.
              A project that sets its own `docsDirectory` overrides this value,
              with the same additive meaning.
            */}
            <FieldGroup>
              <Field>
                <FieldLabel>Docs directory</FieldLabel>
                <SettingsInput
                  aria-label="Global Docs directory"
                  onChange={(event) => onGlobalDocsDirectoryChange(event.currentTarget.value)}
                  placeholder="/Users/you/Documents/vault"
                  value={settings.globalDocsDirectory}
                />
                <FieldDescription>
                  Extra folder every project's Docs surface shows unless the project sets its own. It is added alongside that project's own README, CLAUDE.md, docs/ and Docs folders — it never replaces them — and appears as one top-level folder named after itself. Leave blank to add nothing.
                </FieldDescription>
              </Field>
            </FieldGroup>
          </CardContent>
        </Card>
        ) : null}
        {!shouldShowSettingsSection(search.sections.projectSettings) ? null : projects.length ===
          0 ? (
        <Empty>
          <EmptyHeader>
            <EmptyTitle>No projects</EmptyTitle>
            <EmptyDescription>Main projects will appear here.</EmptyDescription>
          </EmptyHeader>
        </Empty>
        ) : (
        <>
        {/*
         * CDXC:ProjectSettings 2026-06-14-17:29:
         * The Projects settings tab should not render every project as a visible
         * button list. Keep one project selector at the top, open a searchable
         * dropdown of project paths on click, and bind the settings editor below
         * to the selected project.
         *
         * CDXC:ProjectSettings 2026-06-19-12:11:
         * The Projects settings page edits selected-project metadata only.
         * Do not expose project deletion from this page; removing the standalone
         * trash row keeps destructive project management out of this settings flow.
         */}
        <div className="projects-settings-selector">
          <span className="projects-settings-selector-label" id={projectSelectorLabelId}>
            Project
          </span>
          <Popover
            onOpenChange={(open) => {
              setIsProjectSelectorOpen(open);
              if (!open) {
                setProjectSelectorQuery("");
              }
            }}
            open={isProjectSelectorOpen}
          >
            <PopoverTrigger
              render={
                <Button
                  aria-expanded={isProjectSelectorOpen}
                  aria-labelledby={`${projectSelectorLabelId} ${projectSelectorValueId}`}
                  className="projects-settings-selector-trigger"
                  type="button"
                  variant="outline"
                />
              }
            >
              <span className="projects-settings-selector-icon" aria-hidden="true">
                <IconFolderOpen aria-hidden="true" />
              </span>
              <span className="projects-settings-selector-copy" id={projectSelectorValueId}>
                <span className="projects-settings-selector-name">{selectedProject?.name}</span>
                <span className="projects-settings-selector-path">{selectedProject?.path}</span>
              </span>
              <IconChevronDown aria-hidden="true" data-icon="inline-end" />
            </PopoverTrigger>
            <PopoverContent
              align="start"
              className="projects-settings-selector-popover"
              onOpenAutoFocus={(event) => event.preventDefault()}
              sideOffset={8}
            >
              <Command className="projects-settings-selector-command">
                <CommandInput
                  aria-label="Search projects"
                  className="projects-settings-selector-search pl-3"
                  clearLabel="Clear project search"
                  onValueChange={setProjectSelectorQuery}
                  placeholder="Search project paths"
                  spellCheck={false}
                  value={projectSelectorQuery}
                />
                <CommandList className="projects-settings-selector-list scroll-mask-y">
                  <CommandEmpty>No matching projects</CommandEmpty>
                  <CommandGroup heading="Projects">
                    {projects.map((project) => (
                      <CommandItem
                        className="projects-settings-selector-option"
                        data-checked={selectedProject?.projectId === project.projectId}
                        key={project.projectId}
                        onSelect={() => selectProject(project.projectId)}
                        value={`${project.name} ${project.path}`}
                      >
                        <IconFolderOpen aria-hidden="true" />
                        <span className="projects-settings-selector-option-copy">
                          <span className="projects-settings-selector-option-name">
                            {project.name}
                          </span>
                          <span className="projects-settings-selector-option-path">
                            {project.path}
                          </span>
                        </span>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>
        </div>
        <Card className="settings-project-command-card">
          <CardContent className="flex flex-col gap-4 p-4">
            {/*
              CDXC:ProjectSettings 2026-06-15-03:21:
              Worktree command is the primary Projects-page setup control, so it should be the first editable project field after selecting a project. Ticket key and Beads directory stay below because they configure board metadata.
            */}
            <FieldGroup>
              <Field>
                <FieldLabel>
                  Worktree command
                  {isWorktreeCommandInherited ? <InheritedSettingBadge /> : null}
                </FieldLabel>
                <SettingsTextarea
                  aria-label="Worktree command"
                  className="settings-project-command-textarea"
                  onChange={(event) => setCommand(event.currentTarget.value)}
                  placeholder={inheritedPlaceholder(
                    command,
                    settings.globalWorktreeCommand,
                    "bun install",
                  )}
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
            {/*
              CDXC:ProjectBoard 2026-05-23-14:35:
              Projects settings owns the three-letter ticket key shown on the board (for example ZMX-12) while Beads keeps hash ids internally.
            */}
            <FieldGroup>
              <Field>
                <FieldLabel>
                  Ticket key
                  {isBeadsDisplayKeyInherited ? <InheritedSettingBadge /> : null}
                </FieldLabel>
                <SettingsInput
                  aria-label="Ticket key"
                  maxLength={3}
                  onChange={(event) =>
                    setBeadsDisplayKey(event.currentTarget.value.toUpperCase().replace(/[^A-Z0-9]/gu, ""))
                  }
                  placeholder={inheritedPlaceholder(
                    beadsDisplayKey,
                    settings.globalBeadsDisplayKey,
                    "ZMX",
                  )}
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
            {/*
              CDXC:ProjectBoard 2026-06-13:
              Projects settings owns the directory the Project board launches its Beads workspace from. Leave blank to use the project root; otherwise the board reads `.beads` from this absolute path.
            */}
            <FieldGroup>
              <Field>
                <FieldLabel>
                  Beads directory
                  {isBeadsDirectoryInherited ? <InheritedSettingBadge /> : null}
                </FieldLabel>
                <SettingsInput
                  aria-label="Beads directory"
                  onChange={(event) => setBeadsDirectory(event.currentTarget.value)}
                  placeholder={inheritedPlaceholder(
                    beadsDirectory,
                    settings.globalBeadsDirectory,
                    "/Users/you/code/my-repo",
                  )}
                  value={beadsDirectory}
                />
                <FieldDescription>
                  Path to this project's Beads workspace (.beads). Leave blank to use the Global Default or project root.
                </FieldDescription>
              </Field>
            </FieldGroup>
            <div className="settings-management-actions">
              <Button onClick={() => setBeadsDirectory("")} type="button" variant="outline">
                Clear
              </Button>
              <Button onClick={saveBeadsDirectory} type="button">
                Save Beads Directory
              </Button>
            </div>
            {/*
              CDXC:DocsRootDirectory 2026-08-09:
              Projects settings owns this project's `docsDirectory`: the extra
              folder its Docs surface shows. Leave blank to use the Global
              Default.

              CDXC:DocsRootAdditive 2026-08-09:
              This project's own docs list either way, so `docsDirectory` only
              ever adds a tree beside them — it never replaces them.
            */}
            <FieldGroup>
              <Field>
                <FieldLabel>
                  Docs directory
                  {isDocsDirectoryInherited ? <InheritedSettingBadge /> : null}
                </FieldLabel>
                <SettingsInput
                  aria-label="Docs directory"
                  onChange={(event) => setDocsDirectory(event.currentTarget.value)}
                  placeholder={inheritedPlaceholder(
                    docsDirectory,
                    settings.globalDocsDirectory,
                    "/Users/you/Documents/vault",
                  )}
                  value={docsDirectory}
                />
                <FieldDescription>
                  Extra folder this project's Docs surface shows, in addition to the project's own docs. Leave blank to use the Global Default.
                </FieldDescription>
              </Field>
            </FieldGroup>
            <div className="settings-management-actions">
              <Button onClick={() => setDocsDirectory("")} type="button" variant="outline">
                Clear
              </Button>
              <Button onClick={saveDocsDirectory} type="button">
                Save Docs Directory
              </Button>
            </div>
          </CardContent>
        </Card>
        </>
        )}
      </div>
    </div>
  );
}

type PortlessSettingsDomainSummary = {
  domains: readonly {
    hostname: string;
    liveRoutes: readonly {
      kind: "primary" | "additional";
      port: number;
    }[];
  }[];
  kind: "project" | "worktree";
  projectId: string;
  title: string;
};

const PORTLESS_PROTOCOL_OPTIONS: readonly { label: string; value: PortlessProtocol }[] = [
  { label: "HTTPS", value: "https" },
  { label: "HTTP", value: "http" },
];

const PORTLESS_SETTINGS_RECOMMENDED_ADMIN_ACTIONS: readonly NativePortlessAdminInstallAction[] = [
  "install",
  "reconfigure",
  "retry",
];

const PORTLESS_SETTINGS_ADMIN_ACTION_LABELS: Record<NativePortlessAdminAction, string> = {
  install: "Install",
  reconfigure: "Reconfigure",
  remove: "Remove background proxy",
  retry: "Retry",
};

/*
 * CDXC:PortlessSettingsDisabled 2026-07-25:
 * Preserve the complete Portless Settings implementation for a later return,
 * but do not expose its controls while the app integration is disabled.
 */
const PORTLESS_SETTINGS_VISIBLE = false;

function PortlessGlobalSettingsPanel({
  domainSummaries,
  onAdminAction,
  onEnabledChange,
  onProtocolChange,
  portless,
  settings,
}: {
  domainSummaries: readonly PortlessSettingsDomainSummary[];
  onAdminAction: (action: NativePortlessAdminAction) => void;
  onEnabledChange: (checked: boolean) => void;
  onProtocolChange: (protocol: PortlessProtocol) => void;
  portless?: SidebarPortlessState;
  settings: ghostexSettings;
}) {
  const portlessToggleId = useId();
  const portlessProtocolLabelId = useId();
  const status = getPortlessSettingsStatus(portless, settings);
  const recommendedAction = getPortlessRecommendedSettingsAdminAction(portless);
  const showRemoveAction = portless?.health.setupOwnership === "ghostex";
  const removeAvailability = portless?.nativeAdmin.actions.remove;

  return (
    <section className="settings-modal-section settings-projects-global-settings">
      <div className="settings-projects-global-header">
        <div className="settings-management-header-text">
          {/*
            CDXC:PortlessSettings 2026-06-30-11:42:
            Projects global settings should title the Portless card as Portless and briefly define it, because the controls manage Ghostex's local-domain proxy rather than generic project metadata.
          */}
          <h3 className="settings-management-heading">Portless</h3>
          <p className="settings-management-description">
            Portless gives projects and worktrees stable local domains for dev servers through Ghostex's background proxy.
          </p>
        </div>
        <span className="settings-portless-status-badge" data-status={status.tone}>
          {status.label}
        </span>
      </div>
      <div className="settings-projects-global-body">
        <div className="settings-portless-control-row">
          <div className="settings-management-main">
            <label className="settings-management-title" htmlFor={portlessToggleId}>
              Portless
            </label>
            <span className="settings-management-detail">
              Create stable local domains for running project and worktree dev servers.
            </span>
          </div>
          <Switch
            checked={settings.portlessEnabled}
            id={portlessToggleId}
            onCheckedChange={onEnabledChange}
          />
        </div>
        <div className="settings-portless-control-row">
          <div className="settings-management-main">
            <span className="settings-management-title" id={portlessProtocolLabelId}>
              Protocol
            </span>
            <span className="settings-management-detail">
              Choose the standard local web port the background proxy should use.
            </span>
          </div>
          <ToggleGroup
            aria-labelledby={portlessProtocolLabelId}
            className="settings-portless-protocol-toggle"
            onValueChange={(value) => {
              const [protocol] = value as PortlessProtocol[];
              if (protocol) {
                onProtocolChange(protocol);
              }
            }}
            value={[settings.portlessProtocol]}
            variant="outline"
          >
            {PORTLESS_PROTOCOL_OPTIONS.map((option) => (
              <ToggleGroupItem key={option.value} value={option.value}>
                {option.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
        <div className="settings-portless-status-row">
          <IconInfoCircle aria-hidden="true" />
          <div className="settings-management-main">
            <span className="settings-management-title">Setup status</span>
            <span className="settings-management-detail">{status.detail}</span>
          </div>
        </div>
        <div className="settings-portless-actions" aria-label="Portless actions">
          {recommendedAction ? (
            <PortlessSettingsAdminActionButton
              action={recommendedAction}
              availability={portless?.nativeAdmin.actions[recommendedAction]}
              onAdminAction={onAdminAction}
            />
          ) : null}
          {settings.portlessEnabled ? (
            <Button onClick={() => onEnabledChange(false)} type="button" variant="outline">
              <IconCircleX aria-hidden="true" />
              Disable
            </Button>
          ) : null}
          {showRemoveAction ? (
            <PortlessSettingsAdminActionButton
              action="remove"
              availability={removeAvailability}
              onAdminAction={onAdminAction}
            />
          ) : null}
        </div>
        <PortlessAssignedDomainsSummary
          domainSummaries={domainSummaries}
          routePreviewStatus={portless?.presentation?.routePreviewStatus}
          settings={settings}
        />
      </div>
    </section>
  );
}

function PortlessSettingsAdminActionButton({
  action,
  availability,
  onAdminAction,
}: {
  action: NativePortlessAdminAction;
  availability?: SidebarPortlessState["nativeAdmin"]["actions"][NativePortlessAdminAction];
  onAdminAction: (action: NativePortlessAdminAction) => void;
}) {
  const Icon =
    action === "install"
      ? IconDownload
      : action === "retry"
        ? IconRefresh
        : action === "remove"
          ? IconTrash
          : IconTools;
  const disabled = availability?.available !== true;
  const disabledReason =
    availability?.unavailableReason === "localMacOnly"
      ? "This action is available only on the local Mac."
      : availability?.unavailableReason === "setupNotGhostexOwned"
        ? "Ghostex can’t change a setup it doesn’t own."
        : "No setup change is needed right now.";
  return (
    <SettingButton
      disabled={disabled}
      disabledReason={disabledReason}
      onClick={() => onAdminAction(action)}
      type="button"
      variant={action === "remove" ? "outline" : "default"}
    >
      <Icon aria-hidden="true" />
      {PORTLESS_SETTINGS_ADMIN_ACTION_LABELS[action]}
    </SettingButton>
  );
}

function PortlessAssignedDomainsSummary({
  domainSummaries,
  routePreviewStatus,
  settings,
}: {
  domainSummaries: readonly PortlessSettingsDomainSummary[];
  routePreviewStatus?: NonNullable<SidebarPortlessState["presentation"]>["routePreviewStatus"];
  settings: ghostexSettings;
}) {
  const emptyMessage = getPortlessAssignedDomainsEmptyMessage(routePreviewStatus, settings);
  return (
    <div className="settings-portless-domains">
      <div className="settings-management-main">
        <span className="settings-management-title">Assigned domains</span>
        <span className="settings-management-detail">
          Generated project and worktree domains are read-only.
        </span>
      </div>
      {domainSummaries.length > 0 ? (
        <ul aria-label="Assigned Portless domains" className="settings-portless-domain-list">
          {domainSummaries.map((summary) => (
            <li className="settings-portless-domain-group" key={summary.projectId}>
              <div className="settings-portless-domain-group-header">
                <span className="settings-portless-domain-group-title">{summary.title}</span>
                <span className="settings-portless-domain-group-kind">
                  {summary.kind === "worktree" ? "Worktree" : "Project"}
                </span>
              </div>
              <div className="settings-portless-domain-hosts">
                {summary.domains.map((domain) => (
                  <div className="settings-portless-domain-host" key={domain.hostname}>
                    <code className="settings-portless-domain-hostname">{domain.hostname}</code>
                    <span className="settings-portless-domain-meta">
                      {domain.liveRoutes.length > 0
                        ? domain.liveRoutes
                            .map((route) => `${route.kind === "primary" ? "Primary" : "Additional"} - port ${route.port}`)
                            .join(", ")
                        : "Assigned"}
                    </span>
                  </div>
                ))}
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <div className="settings-portless-domain-empty">{emptyMessage}</div>
      )}
    </div>
  );
}

function getPortlessSettingsStatus(
  portless: SidebarPortlessState | undefined,
  settings: ghostexSettings,
): { detail: string; label: string; tone: "active" | "disabled" | "failed" | "needsSetup" | "unknown" } {
  if (!settings.portlessEnabled) {
    return {
      detail: "Portless is off in Ghostex settings.",
      label: "Disabled",
      tone: "disabled",
    };
  }
  const health = portless?.health;
  if (!health) {
    return {
      detail: "Gxserver has not reported Portless setup metadata yet.",
      label: "Unknown",
      tone: "unknown",
    };
  }
  if (health.setupStatus === "active" && health.setupOwnership === "ghostex") {
    return {
      detail: `Ghostex is managing the ${health.protocol.toUpperCase()} background proxy.`,
      label: "Active",
      tone: "active",
    };
  }
  if (health.setupStatus === "failed") {
    return {
      detail: "Ghostex could not verify the managed background proxy.",
      label: "Failed",
      tone: "failed",
    };
  }
  if (health.setupStatus === "needed" && health.setupOwnership === "standalone") {
    return {
      detail: "A Portless service is installed, but Ghostex is not managing it.",
      label: "Reconfigure",
      tone: "needsSetup",
    };
  }
  if (health.setupStatus === "needed") {
    return {
      detail: "Install the Ghostex-managed background proxy to assign domains.",
      label: "Setup needed",
      tone: "needsSetup",
    };
  }
  if (health.setupStatus === "disabled") {
    return {
      detail: "Portless setup is disabled in the reported runtime state.",
      label: "Disabled",
      tone: "disabled",
    };
  }
  return {
    detail: "Portless setup state is not available yet.",
    label: "Unknown",
    tone: "unknown",
  };
}

function getPortlessRecommendedSettingsAdminAction(
  portless: SidebarPortlessState | undefined,
): NativePortlessAdminInstallAction | undefined {
  return PORTLESS_SETTINGS_RECOMMENDED_ADMIN_ACTIONS.find((action) => {
    const nativeAvailability = portless?.nativeAdmin.actions[action];
    const healthRecommendation = portless?.health.actions[action];
    return nativeAvailability?.available === true || healthRecommendation?.recommended === true;
  });
}

function getProjectPortlessDomainSummaries(
  projects: readonly SidebarProjectSettingsItem[],
  selectedProject: SidebarProjectSettingsItem | undefined,
  portless: SidebarPortlessState | undefined,
): readonly PortlessSettingsDomainSummary[] {
  const assignedDomains = portless?.presentation?.assignedDomains ?? [];
  if (!selectedProject || assignedDomains.length === 0) {
    return [];
  }
  const projectsById = new Map(projects.map((project) => [project.projectId, project]));
  const includedProjectIds = new Set<string>([selectedProject.projectId]);
  if (!selectedProject.worktreeParentProjectId) {
    for (const project of projects) {
      if (project.worktreeParentProjectId === selectedProject.projectId) {
        includedProjectIds.add(project.projectId);
      }
    }
  }
  const liveRoutesByProjectAndHostname = new Map<
    string,
    PortlessSettingsDomainSummary["domains"][number]["liveRoutes"]
  >();
  for (const preview of portless?.presentation?.routePreviews ?? []) {
    const key = `${preview.projectId}\0${preview.hostname}`;
    liveRoutesByProjectAndHostname.set(key, [
      ...(liveRoutesByProjectAndHostname.get(key) ?? []),
      {
        kind: preview.kind,
        port: preview.port,
      },
    ]);
  }
  const domainsByProjectId = new Map<string, PortlessSettingsDomainSummary["domains"][number][]>();
  for (const domain of assignedDomains) {
    if (!includedProjectIds.has(domain.projectId)) {
      continue;
    }
    const domains = domainsByProjectId.get(domain.projectId) ?? [];
    if (!domains.some((existingDomain) => existingDomain.hostname === domain.hostname)) {
      domains.push({
        hostname: domain.hostname,
        liveRoutes:
          liveRoutesByProjectAndHostname.get(`${domain.projectId}\0${domain.hostname}`) ?? [],
      });
    }
    domainsByProjectId.set(domain.projectId, domains);
  }
  return [...domainsByProjectId.entries()].map(([projectId, domains]) => {
    const project = projectsById.get(projectId);
    return {
      domains,
      kind: project?.worktreeParentProjectId ? "worktree" : "project",
      projectId,
      title: project?.name ?? "Project",
    };
  });
}

function getPortlessAssignedDomainsEmptyMessage(
  routePreviewStatus:
    | NonNullable<SidebarPortlessState["presentation"]>["routePreviewStatus"]
    | undefined,
  settings: ghostexSettings,
): string {
  if (!settings.portlessEnabled || routePreviewStatus === "disabled") {
    return "No domains are assigned while Portless is disabled.";
  }
  if (routePreviewStatus === "unavailable" || !routePreviewStatus) {
    return "No assigned domain metadata is available yet.";
  }
  return "No assigned domains are available for the selected project yet.";
}

function createPortlessSettingsAdminRequestId(action: NativePortlessAdminAction): string {
  return `portless-settings-${action}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

type SettingsAgentDragData = {
  agentId: string;
  kind: "settings-agent";
};

type SettingsCommandDragData = {
  commandId: string;
  kind: "settings-command";
};

type SettingsSidebarTagListItemDragData = {
  itemId: string;
  kind: "settings-sidebar-tag-list-item";
};

function OpenTargetsSettingsTab({
  onChange,
  search,
  searchEmptyState,
  settings,
}: {
  onChange: (settings: ghostexSettings) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
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
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)
          ? searchEmptyState
          : null}
        {shouldShowSettingsSection(search.sections.openIn) ? (
        <SettingsSection title="Open In">
          {/* CDXC:TitlebarOpenIn 2026-05-11-00:22
              Users need a Settings tab opened from the titlebar dropdown to
              show or hide IDE targets and add custom project-open commands.

              CDXC:TitlebarOpenIn 2026-05-16-23:24
              Settings must show the same Open In editor icons as the titlebar
              dropdown so users can scan Cursor, VS Code variants, Zed,
              Antigravity, VSCodium, and JetBrains-family targets by brand. */}
          <div className="flex flex-col gap-2">
            {BUILT_IN_WORKSPACE_OPEN_TARGETS.filter((target) =>
              shouldShowSetting(search.sections.openIn, `builtin:${target.id}`),
            ).map((target) => {
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
                  <SettingSwitch
                    checked={isAvailable && !hiddenIds.has(target.id)}
                    disabled={!isAvailable}
                    disabledReason={`Install ${target.label} to enable this option.`}
                    onCheckedChange={(checked) => updateHiddenTarget(target.id, checked)}
                  />
                </div>
              );
            })}
          </div>
        </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.customOpenTargets) ? (
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
                <SettingsInput
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
                <SettingsInput
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
                <SettingsTextarea
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
        ) : null}
      </div>
    </SettingsNativeScrollArea>
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
  search,
  searchEmptyState,
  status,
}: {
  loading?: boolean;
  onRequestStatus?: () => void;
  onSetDefaults?: (target: "editor" | "terminalLinks" | "scriptRunner" | "all") => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
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
  const statusItems = status?.statusItems ?? [];
  const visibleStatusItems = statusItems.slice(0, 6);
  const remainingStatusItemCount = Math.max(0, statusItems.length - visibleStatusItems.length);
  return (
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)
          ? searchEmptyState
          : null}
        {shouldShowSettingsSection(search.sections.defaults) ? (
        <SettingsSection title="Defaults">
          {/*
           * CDXC:OSIntegration 2026-05-27-18:06:
           * Ghostex registers as an available macOS editor and script handler
           * at install/build time, but Settings is the only place that changes
           * default editor, terminal-link, or script-runner ownership.
           */}
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <SettingButton
              className="h-10 w-full justify-start px-4"
              disabled={!onSetDefaults}
              disabledReason="macOS default-app changes aren’t available here."
              disabledTooltipClassName="w-full"
              onClick={() => onSetDefaults?.("editor")}
              type="button"
              variant="outline"
            >
              <IconCodeDots aria-hidden="true" data-icon="inline-start" />
              Set as Default Editor
            </SettingButton>
            <SettingButton
              className="h-10 w-full justify-start px-4"
              disabled={!onSetDefaults}
              disabledReason="macOS default-app changes aren’t available here."
              disabledTooltipClassName="w-full"
              onClick={() => onSetDefaults?.("terminalLinks")}
              type="button"
              variant="outline"
            >
              <IconTerminal2 aria-hidden="true" data-icon="inline-start" />
              Set Terminal Links
            </SettingButton>
            <SettingButton
              className="h-10 w-full justify-start px-4"
              disabled={!onSetDefaults}
              disabledReason="macOS default-app changes aren’t available here."
              disabledTooltipClassName="w-full"
              onClick={() => onSetDefaults?.("scriptRunner")}
              type="button"
              variant="outline"
            >
              <IconPlayerPlay aria-hidden="true" data-icon="inline-start" />
              Set Script Runner
            </SettingButton>
            <SettingButton
              className="h-10 w-full justify-start px-4"
              disabled={!onSetDefaults}
              disabledReason="macOS default-app changes aren’t available here."
              disabledTooltipClassName="w-full"
              onClick={() => onSetDefaults?.("all")}
              type="button"
            >
              <IconCircleCheckFilled aria-hidden="true" data-icon="inline-start" />
              Set All
            </SettingButton>
          </div>
        </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.cli) ? (
        <SettingsSection title="CLI">
          <div className="grid gap-2 rounded-none border border-border bg-muted/20 p-3 font-mono text-xs text-muted-foreground">
            <div>ghostex open ./folder</div>
            <div>ghostex edit --wait file.ts:12:3</div>
            <div>ghostex terminal --cwd /tmp --title Scratch -- echo hi</div>
            <div>ghostex ./file.txt</div>
          </div>
        </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.diagnostics) ? (
        <SettingsSection title="Diagnostics">
          <div className="flex flex-col gap-3 rounded-none border border-border bg-muted/20 p-3 text-sm text-muted-foreground">
            <div className="flex items-center justify-between gap-3">
              <span>{loading && !status ? "Checking macOS handlers..." : "macOS handler status"}</span>
              <SettingButton
                className="h-8 px-3"
                disabled={loading || !onRequestStatus}
                disabledReason={
                  loading
                    ? "macOS handler status is being checked."
                    : "Status checks aren’t available here."
                }
                onClick={onRequestStatus}
                type="button"
                variant="outline"
              >
                <IconRefresh aria-hidden="true" data-icon="inline-start" />
                Refresh
              </SettingButton>
            </div>
            {status ? (
              <div className="grid gap-2">
                {statusItems.length > 0 ? (
                  <div className="grid gap-2 rounded-none border border-destructive/30 bg-destructive/5 p-3 text-xs text-muted-foreground">
                    {/*
                     * CDXC:OSIntegration 2026-06-24-15:10:
                     * Settings must account for shared Launch Services status items without exposing raw OSStatus values or native paths. Show generic repair guidance and sanitized target/extension labels so the same UI works for Swift and GPUI senders.
                     */}
                    <div className="flex items-start gap-2">
                      <IconAlertTriangle
                        aria-hidden="true"
                        className="mt-0.5 shrink-0 text-destructive"
                        size={16}
                      />
                      <div className="grid gap-1">
                        <div className="font-medium text-foreground">
                          {getOSIntegrationStatusNoticeTitle(statusItems)}
                        </div>
                        <div>{getOSIntegrationStatusNoticeDescription(statusItems)}</div>
                      </div>
                    </div>
                    <div className="grid gap-1">
                      {visibleStatusItems.map((item, index) => (
                        <div className="flex items-center justify-between gap-3" key={index}>
                          <span>{formatOSIntegrationStatusItemSubject(item)}</span>
                          <span className="text-right font-medium text-foreground">
                            {formatOSIntegrationStatusItemReason(item)}
                          </span>
                        </div>
                      ))}
                      {remainingStatusItemCount > 0 ? (
                        <div className="text-muted-foreground">
                          {remainingStatusItemCount} more handler updates need attention.
                        </div>
                      ) : null}
                    </div>
                  </div>
                ) : null}
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
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

function getOSIntegrationStatusNoticeTitle(
  items: readonly SidebarOSIntegrationStatusItem[],
): string {
  if (items.some((item) => item.reason === "unsupportedPlatform")) {
    return "macOS Launch Services is unavailable in this build.";
  }
  return "Some macOS handler updates need attention.";
}

function getOSIntegrationStatusNoticeDescription(
  items: readonly SidebarOSIntegrationStatusItem[],
): string {
  if (items.some((item) => item.reason === "unsupportedPlatform")) {
    return "This platform cannot inspect or change macOS app defaults.";
  }
  return "Refresh after macOS finishes updating Launch Services, or choose Ghostex manually in macOS Open With/System Settings.";
}

function formatOSIntegrationStatusItemSubject(item: SidebarOSIntegrationStatusItem): string {
  const fileExtension = formatOSIntegrationStatusExtension(item.extension);
  if (item.target === "editor") {
    return fileExtension ? `Editor default .${fileExtension}` : "Editor defaults";
  }
  if (item.target === "scriptRunner") {
    return fileExtension ? `Script runner .${fileExtension}` : "Script runner";
  }
  if (item.target === "terminalLinks") {
    return item.scheme === "ghostex" ? "Terminal links ghostex://" : "Terminal links";
  }
  if (item.target === "bundleRegistration") {
    return item.operation === "registerBundle" ? "App registration" : "App identity";
  }
  return "Platform support";
}

function formatOSIntegrationStatusExtension(extension: string | undefined): string | undefined {
  if (!extension || !/^[A-Za-z0-9][A-Za-z0-9_-]{0,24}$/u.test(extension)) {
    return undefined;
  }
  return extension;
}

function formatOSIntegrationStatusItemReason(item: SidebarOSIntegrationStatusItem): string {
  switch (item.reason) {
    case "bundleIdentifierMissing":
      return "App identity missing";
    case "bundleRegistrationFailed":
      return "Registration failed";
    case "contentTypeUnavailable":
      return "File type unavailable";
    case "invalidTarget":
      return "Unsupported action";
    case "launchServicesRejected":
      return "Default change rejected";
    case "unsupportedPlatform":
      return "Unavailable";
  }
}

function OSIntegrationDiagnosticRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span>{label}</span>
      <span className="text-right font-medium text-foreground">{value}</span>
    </div>
  );
}

const AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS = DEFAULT_SIDEBAR_AGENTS;
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

function hasRemovableAgentHooks(
  agentHookStatus: SidebarAgentHookStatusMessage | undefined,
): boolean {
  if (!agentHookStatus || agentHookStatus.errorMessage) {
    return false;
  }
  return agentHookStatus.agents.some(
    (status) =>
      status.hookInstalled || status.status === "installed" || status.status === "updateRequired",
  );
}

function hasInstalledBundledAgentSkills(
  ghostexCliStatus: SidebarGhostexCliStatusMessage | undefined,
): boolean {
  return (
    ghostexCliStatus?.agentOrchestrationSkillInstalled === true ||
    ghostexCliStatus?.browserSkillInstalled === true ||
    ghostexCliStatus?.embeddedBrowserSkillInstalled === true ||
    ghostexCliStatus?.computerUseSkillInstalled === true ||
    ghostexCliStatus?.fable56OrchestrationSkillInstalled === true ||
    ghostexCliStatus?.findPrevSessionSkillInstalled === true ||
    ghostexCliStatus?.generateTitleSkillInstalled === true ||
    ghostexCliStatus?.moveCodexSessionSkillInstalled === true
  );
}

type PluginVisibilitySettingKey =
  | "codeViewTabHidden"
  | "browserViewTabHidden"
  | "kanbanViewTabHidden"
  | "automateViewTabHidden"
  | "docsViewTabHidden"
  | "tipsAndTricksTitlebarButtonHidden"
  | "resourcesTitlebarButtonHidden"
  | "gitActionsTitlebarButtonHidden"
  | "quickActionsTitlebarButtonHidden"
  | "openInTitlebarButtonHidden";

function PluginsSettingsTab({
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onInstallCuaDriver,
  onRequestGhostexCliStatus,
  onRequestStatus,
  onReinstallPlugin,
  onUpdateSetting,
  search,
  searchEmptyState,
  settings,
  status,
  statusLoading,
}: {
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onInstallCuaDriver?: () => void;
  onRequestGhostexCliStatus?: () => void;
  onRequestStatus?: () => void;
  onReinstallPlugin?: (pluginId: SidebarPluginSettingsItem["id"]) => void;
  onUpdateSetting: (key: PluginVisibilitySettingKey, value: boolean) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  settings: ghostexSettings;
  status?: SidebarPluginSettingsStatusMessage;
  statusLoading: boolean;
}) {
  const statusById = new Map(status?.plugins.map((plugin) => [plugin.id, plugin]));
  const code = statusById.get("code");
  const kanban = statusById.get("kanban");
  const cef = statusById.get("cef");
  const cuaDriverInstalled = ghostexCliStatus?.cuaDriverInstalled === true;
  const cuaDriverManagedUpdatesSupported =
    ghostexCliStatus?.cuaDriverManagedUpdatesSupported !== false;
  const cuaDriverUpdateAvailable = ghostexCliStatus?.cuaDriverUpdateAvailable;
  const cuaDriverStatus =
    ghostexCliStatusLoading || !ghostexCliStatus
      ? "Checking"
      : !cuaDriverInstalled
        ? "Not installed"
        : cuaDriverUpdateAvailable === true
          ? "Update available"
          : cuaDriverUpdateAvailable === false
            ? "Up to date"
            : "Installed";
  const cuaDriverActionLabel = !cuaDriverManagedUpdatesSupported
    ? cuaDriverInstalled
      ? "View downloads"
      : "Download"
    : !cuaDriverInstalled
      ? "Install"
      : cuaDriverUpdateAvailable === true
        ? "Upgrade"
        : "Check for updates";
  const showViewTab = (key: string) => shouldShowSetting(search.sections.viewTabs, key);
  const showQuickAccessButton = (key: string) =>
    shouldShowSetting(search.sections.quickAccessButtons, key);

  return (
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)
          ? searchEmptyState
          : null}
        {shouldShowSettingsSection(search.sections.viewTabs) ? (
          <SettingsSection
            description="Choose which project workareas appear in the title bar. Hiding a tab does not stop its runtime or disable its other entry points."
            title="Plugins"
          >
            {showViewTab("code") ? (
              <PluginManagedSettingsRow
                description="Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex."
                icon={IconCodeDots}
                onReinstall={() => onReinstallPlugin?.("code")}
                onVisibleChange={(visible) => onUpdateSetting("codeViewTabHidden", !visible)}
                reinstallAvailable={Boolean(onReinstallPlugin && code?.canReinstall)}
                runtime={code}
                title="Code"
                visible={!settings.codeViewTabHidden}
              />
            ) : null}
            {showViewTab("browser") ? (
              <PluginManagedSettingsRow
                description="Open websites alongside your project and keep useful pages organized without leaving Ghostex. If it’s the last choice beside Agents, hiding it clears the switcher too."
                icon={IconWorld}
                onVisibleChange={(visible) => onUpdateSetting("browserViewTabHidden", !visible)}
                title="Browser"
                visible={!settings.browserViewTabHidden}
              />
            ) : null}
            {showViewTab("kanban") ? (
              <PluginManagedSettingsRow
                description="Plan upcoming work, organize tasks by progress, and keep your whole project easy to follow at a glance."
                icon={IconPlayerPlay}
                onReinstall={() => onReinstallPlugin?.("kanban")}
                onVisibleChange={(visible) => onUpdateSetting("kanbanViewTabHidden", !visible)}
                reinstallAvailable={Boolean(onReinstallPlugin && kanban?.canReinstall)}
                runtime={kanban}
                title="Kanban"
                visible={!settings.kanbanViewTabHidden}
              />
            ) : null}
            {showViewTab("automate") ? (
              <PluginManagedSettingsRow
                description="Turn repeatable project routines into simple workflows you can run whenever you need them."
                icon={IconBolt}
                onVisibleChange={(visible) => onUpdateSetting("automateViewTabHidden", !visible)}
                title="Automate"
                visible={!settings.automateViewTabHidden}
              />
            ) : null}
            {showViewTab("docs") ? (
              <PluginManagedSettingsRow
                description="Browse your project’s notes, plans, and reference files together in one focused reading space."
                icon={IconFileText}
                onVisibleChange={(visible) => onUpdateSetting("docsViewTabHidden", !visible)}
                title="Docs"
                visible={!settings.docsViewTabHidden}
              />
            ) : null}
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.components) ? (
          <SettingsSection
            actions={
              <SettingButton
                disabled={
                  statusLoading ||
                  ghostexCliStatusLoading ||
                  (!onRequestStatus && !onRequestGhostexCliStatus)
                }
                disabledReason={
                  statusLoading || ghostexCliStatusLoading
                    ? "Plugin status is being checked."
                    : "Status refresh isn’t available here."
                }
                onClick={() => {
                  onRequestStatus?.();
                  onRequestGhostexCliStatus?.();
                }}
                type="button"
                variant="ghost"
              >
                <IconRefresh
                  aria-hidden="true"
                  className={cn((statusLoading || ghostexCliStatusLoading) && "animate-spin")}
                  data-icon="inline-start"
                />
                Refresh
              </SettingButton>
            }
            description={
              <>
                <span className="block">
                  Runtime components shared by Ghostex surfaces and agent workflows.
                </span>
                <span className="block">Check their status and keep them up to date here.</span>
              </>
            }
            descriptionClassName="pb-2"
            title="Shared components"
          >
            {shouldShowSetting(search.sections.components, "cuaDriver") ? (
              <IntegrationSettingsRow
                description="Cua Driver powers /ghostex-browser-use and /ghostex-computer-use. Install both skills from the Integrations page."
                icon={IconDeviceDesktop}
                status={cuaDriverStatus}
                title="Cua Driver"
                tone={
                  cuaDriverUpdateAvailable === true
                    ? "warning"
                    : cuaDriverInstalled
                      ? "success"
                      : "warning"
                }
                version={ghostexCliStatus?.cuaDriverVersion}
              >
                <SettingButton
                  disabled={ghostexCliStatusLoading || !onInstallCuaDriver}
                  disabledReason={
                    ghostexCliStatusLoading
                      ? "Cua Driver status is being checked."
                      : "Cua Driver installation isn’t available here."
                  }
                  onClick={onInstallCuaDriver}
                  type="button"
                  variant={
                    cuaDriverInstalled && cuaDriverUpdateAvailable !== true ? "outline" : "default"
                  }
                >
                  {cuaDriverManagedUpdatesSupported && cuaDriverInstalled ? (
                    <IconRefresh aria-hidden="true" data-icon="inline-start" />
                  ) : (
                    <IconDownload aria-hidden="true" data-icon="inline-start" />
                  )}
                  {cuaDriverActionLabel}
                </SettingButton>
              </IntegrationSettingsRow>
            ) : null}
            {shouldShowSetting(search.sections.components, "cef") ? (
              <PluginManagedSettingsRow
                description="Chromium Embedded Framework powers Ghostex web surfaces and remains enabled because the app requires it."
                icon={IconDeviceDesktop}
                onReinstall={() => onReinstallPlugin?.("cef")}
                reinstallAvailable={Boolean(onReinstallPlugin && cef?.canReinstall)}
                runtime={cef}
                title="Chromium runtime (CEF)"
              />
            ) : null}
          </SettingsSection>
        ) : null}

        {shouldShowSettingsSection(search.sections.quickAccessButtons) ? (
          <SettingsSection
            description="This is the same button cluster shown on the right side of the title bar. Click any button to show or hide it; its feature stays available everywhere else."
            title="Quick access buttons"
          >
            <Field className="rounded-none border border-border bg-muted/20 px-4 py-3">
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <FieldContent>
                  <FieldTitle className="text-sm">Titlebar preview</FieldTitle>
                  <FieldDescription className="text-xs text-muted-foreground">
                    Bright buttons are enabled and shown. Outlined buttons are hidden.
                  </FieldDescription>
                </FieldContent>
                <ButtonGroup
                  aria-label="Quick access button visibility"
                  className="shrink-0 gap-[2px] [&>[data-slot]~[data-slot]]:border-l!"
                >
                  {showQuickAccessButton("tips") ? (
                    <QuickAccessTitlebarButton
                      icon={IconInfoCircle}
                      label="Tips"
                      onToggle={() =>
                        onUpdateSetting(
                          "tipsAndTricksTitlebarButtonHidden",
                          !settings.tipsAndTricksTitlebarButtonHidden,
                        )
                      }
                      visible={!settings.tipsAndTricksTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton("resources") ? (
                    <QuickAccessTitlebarButton
                      icon={IconDeviceDesktop}
                      label="Resources"
                      onToggle={() =>
                        onUpdateSetting(
                          "resourcesTitlebarButtonHidden",
                          !settings.resourcesTitlebarButtonHidden,
                        )
                      }
                      visible={!settings.resourcesTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton("gitActions") ? (
                    <QuickAccessTitlebarButton
                      icon={IconGitCommit}
                      label="Git actions"
                      onToggle={() =>
                        onUpdateSetting(
                          "gitActionsTitlebarButtonHidden",
                          !settings.gitActionsTitlebarButtonHidden,
                        )
                      }
                      visible={!settings.gitActionsTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton("quickActions") ? (
                    <QuickAccessTitlebarButton
                      icon={IconPlayerPlay}
                      label="Quick Actions"
                      onToggle={() =>
                        onUpdateSetting(
                          "quickActionsTitlebarButtonHidden",
                          !settings.quickActionsTitlebarButtonHidden,
                        )
                      }
                      visible={!settings.quickActionsTitlebarButtonHidden}
                    />
                  ) : null}
                  {showQuickAccessButton("openIn") ? (
                    <QuickAccessTitlebarButton
                      icon={IconFolderOpen}
                      label="Open In"
                      onToggle={() =>
                        onUpdateSetting(
                          "openInTitlebarButtonHidden",
                          !settings.openInTitlebarButtonHidden,
                        )
                      }
                      visible={!settings.openInTitlebarButtonHidden}
                    />
                  ) : null}
                </ButtonGroup>
              </div>
            </Field>
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

function PluginManagedSettingsRow({
  description,
  icon,
  onReinstall,
  onVisibleChange,
  reinstallAvailable,
  runtime,
  title,
  visible,
}: {
  description: string;
  icon: typeof IconInfoCircle;
  onReinstall?: () => void;
  onVisibleChange?: (visible: boolean) => void;
  reinstallAvailable?: boolean;
  runtime?: SidebarPluginSettingsItem;
  title: string;
  visible?: boolean;
}) {
  const busy = runtime !== undefined && !["installed", "notInstalled", "failed"].includes(runtime.status);
  const actionLabel = runtime?.status === "notInstalled" ? "Install" : "Reinstall";
  const detail = runtime
    ? `${description}${runtime.errorMessage ? ` · ${runtime.errorMessage}` : ""}`
    : description;
  const tone = runtime
    ? runtime.status === "installed"
      ? "success"
      : runtime.status === "failed"
        ? "warning"
        : "neutral"
    : "success";
  return (
    <IntegrationSettingsRow
      description={detail}
      icon={icon}
      status={runtime?.statusLabel ?? "Built in"}
      title={title}
      tone={tone}
      version={runtime?.version}
    >
      {onReinstall ? (
        <SettingButton
          disabled={busy || !reinstallAvailable}
          disabledReason={
            busy
              ? `${title} is being installed.`
              : "This build does not provide a reinstallable remote component."
          }
          onClick={onReinstall}
          type="button"
          variant="outline"
        >
          <IconRefresh
            aria-hidden="true"
            className={cn(busy && "animate-spin")}
            data-icon="inline-start"
          />
          {actionLabel}
        </SettingButton>
      ) : null}
      {onVisibleChange && visible !== undefined ? (
        <label className="flex h-8 items-center gap-2 px-1 text-xs text-muted-foreground">
          Visible
          <Switch
            aria-label={`Show ${title} in the title bar`}
            checked={visible}
            onCheckedChange={onVisibleChange}
          />
        </label>
      ) : null}
    </IntegrationSettingsRow>
  );
}

function VersionInfoButton({ label, version }: { label: string; version: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <AppTooltip content={copied ? `Copied ${version}` : version}>
      <Button
        aria-label={`Copy ${label} version ${version}`}
        onClick={() => {
          void navigator.clipboard.writeText(version).then(
            () => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1200);
            },
            () => undefined,
          );
        }}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <IconInfoCircle aria-hidden="true" />
      </Button>
    </AppTooltip>
  );
}

function QuickAccessTitlebarButton({
  icon: Icon,
  label,
  onToggle,
  visible,
}: {
  icon: typeof IconInfoCircle;
  label: string;
  onToggle: () => void;
  visible: boolean;
}) {
  return (
    <AppTooltip
      content={`${label} is ${visible ? "shown" : "hidden"}. Click to ${visible ? "hide" : "show"}.`}
    >
      <Button
        aria-label={`${visible ? "Hide" : "Show"} ${label} in the title bar`}
        aria-pressed={visible}
        onClick={onToggle}
        size="icon"
        style={
          visible
            ? { backgroundColor: "#e5e5e5", borderColor: "#e5e5e5", color: "#0a0a0a" }
            : undefined
        }
        type="button"
        variant="outline"
      >
        <Icon aria-hidden="true" />
      </Button>
    </AppTooltip>
  );
}

function IntegrationsSettingsTab({
  agentHookStatus,
  agentHookStatusLoading,
  appShotsEnabled,
  appShotsHotkey,
  appShotsMetadataEnabled,
  ghostexCliStatus,
  ghostexCliStatusLoading,
  onAppShotsEnabledChange,
  onAppShotsHotkeyChange,
  onAppShotsMetadataEnabledChange,
  onInstallAgentOrchestrationSkill,
  onInstallBrowserControl,
  onInstallBrowserUseSkill,
  onInstallComputerUseSkill,
  onInstallFable56OrchestrationSkill,
  onInstallFindPrevSessionSkill,
  onInstallGenerateTitleSkill,
  onInstallGhostexCli,
  onInstallMoveCodexSessionSkill,
  onUninstallAgentHooks,
  onUninstallBundledAgentSkill,
  onUninstallBundledAgentSkills,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
  onRequestGhostexCliStatus,
  search,
  searchEmptyState,
}: {
  agentHookStatus?: SidebarAgentHookStatusMessage;
  agentHookStatusLoading: boolean;
  appShotsEnabled: boolean;
  appShotsHotkey: AppShotsHotkey;
  appShotsMetadataEnabled: boolean;
  ghostexCliStatus?: SidebarGhostexCliStatusMessage;
  ghostexCliStatusLoading: boolean;
  onAppShotsEnabledChange: (checked: boolean) => void;
  onAppShotsHotkeyChange: (hotkey: AppShotsHotkey) => void;
  onAppShotsMetadataEnabledChange: (checked: boolean) => void;
  onInstallAgentOrchestrationSkill?: () => void;
  onInstallBrowserControl?: () => void;
  onInstallBrowserUseSkill?: () => void;
  onInstallComputerUseSkill?: () => void;
  onInstallFable56OrchestrationSkill?: () => void;
  onInstallFindPrevSessionSkill?: () => void;
  onInstallGenerateTitleSkill?: () => void;
  onInstallGhostexCli?: () => void;
  onInstallMoveCodexSessionSkill?: () => void;
  onUninstallAgentHooks?: () => void;
  onUninstallBundledAgentSkill?: (skillId: BundledGhostexAgentSkillId) => void;
  onUninstallBundledAgentSkills?: () => void;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
  onRequestGhostexCliStatus?: () => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
}) {
  const showIntegrationRow = (settingKey: string) =>
    shouldShowSetting(search.sections.integrations, settingKey);
  const agentHooksAvailableForUninstall = hasRemovableAgentHooks(agentHookStatus);
  const bundledAgentSkillsAvailableForUninstall = hasInstalledBundledAgentSkills(ghostexCliStatus);
  const cliReady = ghostexCliStatus?.installed === true;
  /**
   * CDXC:CuaPermissions 2026-05-29-06:00:
   * Cua Permissions status must be based on Cua Driver's own permission check,
   * because granting Cua Driver in macOS can still leave Ghostex's separate
   * Accessibility trust bit false. The row represents desktop automation
   * readiness for agents, not Ghostex's ability to synthesize input.
   */
  const cuaPermissionStatus = getCuaPermissionStatus(ghostexCliStatus, ghostexCliStatusLoading);

  return (
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {/*
         * CDXC:IntegrationsSetup 2026-05-27-04:17:
         * Settings owns one Integrations tab for post-onboarding CLI, bundled
         * Ghostex skills, and macOS privacy permissions. The Cua Driver runtime
         * lifecycle itself belongs to Plugins.
         *
         * CDXC:AgentHookSettings 2026-06-29-01:26:
         * Agent hook install/status UI lives in Settings -> Agents, where the detailed per-agent hook list already exists. Integrations should not duplicate that setup row; it only keeps hook removal as a recovery action at the bottom of the page.
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
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)
          ? searchEmptyState
          : null}
        {shouldShowSettingsSection(search.sections.integrations) ? (
        <SettingsSection title="Integrations">
          {showIntegrationRow("ghostexCli") ? (
          <IntegrationSettingsRow
            description="Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup. gx is linked when that alias is available and not taken by another command."
            icon={IconTerminal2}
            status={ghostexCliStatusLoading && !ghostexCliStatus ? "Checking" : cliReady ? "Installed" : "Not installed"}
            tone={cliReady ? "success" : "warning"}
            title="Ghostex CLI"
          >
            <SettingButton
              disabled={ghostexCliStatusLoading || !onInstallGhostexCli}
              disabledReason={
                ghostexCliStatusLoading
                  ? "CLI status is being checked."
                  : "CLI repair isn’t available here."
              }
              onClick={onInstallGhostexCli}
              type="button"
              variant={cliReady ? "outline" : "default"}
            >
              <IconDownload aria-hidden="true" data-icon="inline-start" />
              Repair CLI
            </SettingButton>
            <SettingButton
              disabled={ghostexCliStatusLoading || !onRequestGhostexCliStatus}
              disabledReason={
                ghostexCliStatusLoading
                  ? "CLI status is being checked."
                  : "CLI status refresh isn’t available here."
              }
              onClick={onRequestGhostexCliStatus}
              type="button"
              variant="ghost"
            >
              <IconRefresh aria-hidden="true" data-icon="inline-start" />
              Refresh
            </SettingButton>
          </IntegrationSettingsRow>
          ) : null}

          {showIntegrationRow("bundledAgentSkills") ? (
          <BundledAgentSkillsPanel
            ghostexCliStatus={ghostexCliStatus}
            ghostexCliStatusLoading={ghostexCliStatusLoading}
            onInstallSkill={{
              agentOrchestration: onInstallAgentOrchestrationSkill,
              browserUse: onInstallBrowserUseSkill,
              computerUse: onInstallComputerUseSkill,
              embeddedBrowserUse: onInstallBrowserControl,
              fable56Orchestration: onInstallFable56OrchestrationSkill,
              findPrevSession: onInstallFindPrevSessionSkill,
              generateTitle: onInstallGenerateTitleSkill,
              moveCodexSession: onInstallMoveCodexSessionSkill,
            }}
            onRefreshStatus={onRequestGhostexCliStatus}
            onUninstallSkill={onUninstallBundledAgentSkill}
          />
          ) : null}

          {/*
           * CDXC:AppShots 2026-06-12-11:12:
           * Settings copy must describe App Shots as an agent-session feature because captured context now targets the focused or recent agent instead of Codex only.
           *
           * CDXC:AppShots 2026-06-15-02:01:
           * App Shots should be instant screenshot capture. Settings copy must not promise OCR, Accessibility text extraction, or other app-content scraping.
           *
           * CDXC:AppShots 2026-06-29-02:59:
           * App Shot prompt metadata is disabled by default and must be a visible opt-in under the App Shots row, because routine captures should paste only the image link unless the user asks for window metadata.
           */}
          {showIntegrationRow("appShots") ? (
          <IntegrationSettingsRow
            badge="Beta"
            description="Capture the frontmost app window, then stage it in the focused or recent agent session as local image context."
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
              <SettingsSelect
                disabled={!appShotsEnabled}
                disabledReason="Turn on App Shots first."
                onValueChange={(value) => onAppShotsHotkeyChange(value as AppShotsHotkey)}
                value={appShotsHotkey}
              >
                <SelectTrigger aria-label="App Shots hotkey" className="w-[190px]">
                  <SelectValue />
                </SelectTrigger>
                <SettingsSelectContent>
                  <SelectGroup>
                    {APP_SHOTS_HOTKEY_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SettingsSelectContent>
              </SettingsSelect>
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">Metadata</span>
                <SettingSwitch
                  aria-label="Include App Shots metadata"
                  checked={appShotsMetadataEnabled}
                  disabled={!appShotsEnabled}
                  disabledReason="Turn on App Shots first."
                  onCheckedChange={onAppShotsMetadataEnabledChange}
                />
              </div>
            </div>
          </IntegrationSettingsRow>
          ) : null}

          {showIntegrationRow("cuaPermissions") ? (
          <IntegrationSettingsRow
            description="Cua Driver needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop."
            icon={IconSettings}
            status={cuaPermissionStatus.status}
            tone={cuaPermissionStatus.tone}
            title="Cua Permissions"
          >
            <SettingButton
              disabled={!onOpenAccessibilityPreferences}
              disabledReason="Accessibility settings aren’t available here."
              onClick={onOpenAccessibilityPreferences}
              type="button"
              variant="outline"
            >
              Accessibility
            </SettingButton>
            <SettingButton
              disabled={!onOpenScreenRecordingPreferences}
              disabledReason="Screen Recording settings aren’t available here."
              onClick={onOpenScreenRecordingPreferences}
              type="button"
              variant="outline"
            >
              Screen Recording
            </SettingButton>
          </IntegrationSettingsRow>
          ) : null}
          {/*
            CDXC:SettingsIntegrations 2026-06-19-14:51:
            macOS Settings > Integrations should not include a Setup Flow launcher row.
            Keep setup access owned by first-launch and other explicit entry points instead of listing it as an integration setting.
          */}
        </SettingsSection>
        ) : null}
        {/*
          CDXC:IntegrationsSetup 2026-06-21-02:54:
          Hooks & Skills removal is an integration recovery action, so keep it as the final card in Settings > Integrations rather than a General Settings advanced section. Disable actions when status proves the corresponding Ghostex-owned artifacts are already absent, so users cannot click no-op recovery buttons.

          CDXC:AgentHookSettings 2026-06-29-01:26:
          Hook installation moved to Settings > Agents, so the Integrations recovery card must point users there for reinstall while bundled skills remain reinstallable from this page.
        */}
        {shouldShowSettingsSection(search.sections.recovery) ? (
        <SettingsSection
          description="Remove Ghostex-owned setup artifacts. You can install hooks again from Settings > Agents and bundled skills again from the rows above."
          title="Hooks & Skills"
        >
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <SettingButton
              className="h-10 w-full px-4 text-sm"
              disabled={agentHookStatusLoading || !agentHooksAvailableForUninstall || !onUninstallAgentHooks}
              disabledReason={
                agentHookStatusLoading
                  ? "Hook status is being checked."
                  : !agentHooksAvailableForUninstall
                    ? "No Ghostex hooks are installed."
                    : "Hook removal isn’t available here."
              }
              disabledTooltipClassName="w-full"
              onClick={onUninstallAgentHooks}
              type="button"
              variant="outline"
            >
              <IconTrash aria-hidden="true" data-icon="inline-start" />
              Uninstall Hooks
            </SettingButton>
            <SettingButton
              className="h-10 w-full px-4 text-sm"
              disabled={ghostexCliStatusLoading || !bundledAgentSkillsAvailableForUninstall || !onUninstallBundledAgentSkills}
              disabledReason={
                ghostexCliStatusLoading
                  ? "Skill status is being checked."
                  : !bundledAgentSkillsAvailableForUninstall
                    ? "No bundled Ghostex skills are installed."
                    : "Skill removal isn’t available here."
              }
              disabledTooltipClassName="w-full"
              onClick={onUninstallBundledAgentSkills}
              type="button"
              variant="outline"
            >
              <IconTrash aria-hidden="true" data-icon="inline-start" />
              Uninstall Skills
            </SettingButton>
          </div>
        </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

function IntegrationSettingsRow({
  badge,
  children,
  description,
  icon: Icon,
  status,
  title,
  tone,
  version,
}: {
  badge?: string;
  children: ReactNode;
  description: string;
  icon: typeof IconInfoCircle;
  status: string;
  title: string;
  tone: "success" | "warning" | "neutral";
  version?: string;
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
              {badge ? (
                /*
                 * CDXC:AppShots 2026-06-13-19:51:
                 * Settings must visibly mark App Shots as Beta while keeping
                 * the separate Enabled/Disabled status badge for its toggle
                 * state.
                 */
                <span className="inline-flex rounded-none border border-sky-500/40 bg-sky-500/10 px-2 py-0.5 text-[11px] font-semibold text-sky-200">
                  {badge}
                </span>
              ) : null}
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
              {version ? <VersionInfoButton label={title} version={version} /> : null}
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
  search,
  searchEmptyState,
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
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
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
        .filter((agent) => Boolean(agent.command?.trim()))
        .map((agent) => ({ label: agent.name.trim() || agent.agentId, value: agent.agentId })),
    [agents],
  );
  const normalizedDefaultPromptAgentId =
    defaultPromptAgentId.trim() || DEFAULT_ghostex_SETTINGS.defaultPromptAgentId;
  const promptAgentHasSavedDefault = promptAgentOptions.some(
    (option) => option.value === normalizedDefaultPromptAgentId,
  );
  const promptAgentSelectOptions = promptAgentHasSavedDefault
    ? promptAgentOptions
    : [
        /*
         * CDXC:GxserverAgentSettings 2026-06-19-08:58:
         * Default Prompt Agent is gxserver-owned and may name a custom or hidden
         * agent before the local launcher registry has a command for it. Show
         * that saved id as unavailable instead of rendering Codex as selected,
         * so Settings never silently rewrites or masks the canonical choice.
         */
        {
          label: `Unavailable (${normalizedDefaultPromptAgentId})`,
          value: normalizedDefaultPromptAgentId,
        },
        ...promptAgentOptions,
      ];
  const selectedDefaultPromptAgentId = normalizedDefaultPromptAgentId;
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
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)
          ? searchEmptyState
          : null}
        {!editorState && shouldShowSettingsSection(search.sections.agentHooks) ? (
          <SettingsSection title="Agent Hooks">
            <details className="group w-full">
              {/*
               * CDXC:AgentHookSettings 2026-05-23-10:05:
               * Settings -> Agents starts with a collapsed hook setup panel so reliable-resume requirements are discoverable without pushing normal agent ordering/editing controls down the tab. The panel covers every current Ghostex CLI resume-hook agent.
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
                </div>
                <div className="flex flex-wrap gap-2">
                  <SettingButton
                    disabled={!onInstallAgentHooks || agentHookStatusLoading}
                    disabledReason={
                      agentHookStatusLoading
                        ? "Hook status is being checked."
                        : "Hook installation isn’t available here."
                    }
                    onClick={onInstallAgentHooks}
                    type="button"
                    variant="outline"
                  >
                    <IconDownload aria-hidden="true" data-icon="inline-start" />
                    {updateRequiredHookCount > 0 ? "Update Hooks" : "Install Hooks"}
                  </SettingButton>
                  <SettingButton
                    disabled={!onRequestAgentHookStatus || agentHookStatusLoading}
                    disabledReason={
                      agentHookStatusLoading
                        ? "Hook status is being checked."
                        : "Hook status refresh isn’t available here."
                    }
                    onClick={onRequestAgentHookStatus}
                    type="button"
                    variant="ghost"
                  >
                    <IconRefresh aria-hidden="true" data-icon="inline-start" />
                    Refresh
                  </SettingButton>
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
        {!editorState && shouldShowSettingsSection(search.sections.config) ? (
          <SettingsSection title="Config">
            {/*
             * CDXC:AgentConfigSettings 2026-06-12-04:40:
             * Default prompt, title generation, custom title command, and global Accept All are configuration controls, not agent management rows. Group them under the same labeled SettingsSection chrome as Agent Hooks and Agents so the Agents tab scans as three consistent areas: hooks, config, and launchers.
             */}
            {!shouldShowSetting(search.sections.config, "defaultPromptAgent") ? null : promptAgentOptions.length >
              0 ? (
              <SelectField
                description="Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection."
                isModified={defaultPromptAgentId !== DEFAULT_ghostex_SETTINGS.defaultPromptAgentId}
                label="Default Prompt Agent"
                onChange={onDefaultPromptAgentIdChange}
                onResetToDefault={() =>
                  onDefaultPromptAgentIdChange(DEFAULT_ghostex_SETTINGS.defaultPromptAgentId)
                }
                options={promptAgentSelectOptions}
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
            {shouldShowSetting(search.sections.config, "titleGenerationAgent") ? (
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
            ) : null}
            {shouldShowSetting(search.sections.config, "titleGenerationCommand") ? (
            <DisabledCommandPreviewField
              description="Preview of the command Ghostex sends to generate automatic first-prompt session titles."
              label="Title Generation Command"
              value={titleGenerationCommandPreview}
            />
            ) : null}
            {sessionTitleGenerationAgent === "custom" &&
            shouldShowSetting(search.sections.config, "customTitleCommand") ? (
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
            {shouldShowSetting(search.sections.config, "acceptAll") ? (
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
              <SettingSwitch
                checked={agentAcceptAllEnabled}
                disabled={!vscode}
                disabledReason="This change needs the Ghostex app connection."
                id={acceptAllToggleId}
                onCheckedChange={onAgentAcceptAllEnabledChange}
              />
            </Field>
            ) : null}
          </SettingsSection>
        ) : null}
        {editorState || shouldShowSettingsSection(search.sections.agentList) ? (
        <SettingsSection
          actions={
            !editorState ? (
              <SettingButton
                disabled={!vscode}
                disabledReason="Adding agents needs the Ghostex app connection."
                onClick={() => setEditorState({ draft: { command: "", name: "" } })}
                type="button"
                variant="outline"
              >
                <IconPlus aria-hidden="true" data-icon="inline-start" />
                Add Agent
              </SettingButton>
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
        ) : null}
      </div>
    </SettingsNativeScrollArea>
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
  const { handleRef, isDragging } = sortable;

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className="settings-management-row flex items-center gap-2 border border-border bg-muted/20 p-2"
      data-dragging={String(Boolean(isDragging))}
      ref={setRowRef}
    >
      <Button
        aria-label={`Reorder ${agent.name}`}
        ref={handleRef}
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
        <SettingsSelect
          items={AGENT_TYPE_SELECT_ITEMS}
          onValueChange={updateAgentType}
          value={icon}
        >
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={agentTypeId}>
            <SelectValue />
          </SelectTrigger>
          <SettingsSelectContent>
            <SelectGroup>
              <SelectItem value="custom">Custom</SelectItem>
              {DEFAULT_SIDEBAR_AGENTS.map((agent) => (
                <SelectItem key={agent.agentId} value={agent.icon}>
                  {agent.name}
                </SelectItem>
              ))}
            </SelectGroup>
          </SettingsSelectContent>
        </SettingsSelect>
      </Field>
      <Field className="gap-2.5">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={nameId}>
            Name
          </FieldLabel>
        </FieldContent>
        <SettingsInput
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
        <SettingsTextarea
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
        <SettingsSelect
          disabled={!acceptAllSupported}
          disabledReason="This agent doesn’t support Accept All."
          disabledTooltipClassName="w-full"
          items={AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS}
          onValueChange={(value) => setAcceptAllMode(value as AgentAcceptAllMode)}
          value={acceptAllMode}
        >
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={acceptAllModeId}>
            <SelectValue />
          </SelectTrigger>
          <SettingsSelectContent>
            <SelectGroup>
              {AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SettingsSelectContent>
        </SettingsSelect>
      </Field>
      <div className="flex justify-end gap-3">
        <Button onClick={onCancel} type="button" variant="outline">
          Cancel
        </Button>
        <SettingButton
          disabled={isSaveDisabled}
          disabledReason={
            name.trim().length === 0 && command.trim().length === 0
              ? "Enter a name and command first."
              : name.trim().length === 0
                ? "Enter an agent name first."
                : "Enter an agent command first."
          }
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
        </SettingButton>
      </div>
    </>
  );
}

/*
CDXC:GlobalActions 2026-08-01:
Settings > Actions holds two lists that behave identically and differ only in
who owns them: Global Actions apply to every project and live in gxserver,
Project Actions belong to one project (and its worktrees) and live in project
metadata. The scope drives the bridge message types and the copy; everything
else — ordering, drag reorder, the editor, duplicate-title validation — is one
implementation, so the two lists cannot drift apart.
*/
type SettingsCommandScope = "global" | "project";

type SettingsCommandScopeEditorState = SettingsCommandEditorState & {
  scope: SettingsCommandScope;
};

function useSettingsCommandOrder(commands: readonly SidebarCommandButton[]) {
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

  return { orderedCommands, setDraftCommandIds };
}

function ActionsSettingsTab({
  getSettingModificationProps,
  hideTabStripNewBrowserButton,
  hideTabStripNewChatButton,
  hideTabStripNewTerminalButton,
  onHideTabStripNewBrowserButtonChange,
  onHideTabStripNewChatButtonChange,
  onHideTabStripNewTerminalButtonChange,
  search,
  searchEmptyState,
  vscode,
}: {
  getSettingModificationProps: <Key extends keyof ghostexSettings>(
    key: Key,
  ) => Required<SettingModificationProps>;
  hideTabStripNewBrowserButton: boolean;
  hideTabStripNewChatButton: boolean;
  hideTabStripNewTerminalButton: boolean;
  onHideTabStripNewBrowserButtonChange: (checked: boolean) => void;
  onHideTabStripNewChatButtonChange: (checked: boolean) => void;
  onHideTabStripNewTerminalButtonChange: (checked: boolean) => void;
  search: SettingsTabSearch;
  searchEmptyState?: ReactNode;
  vscode?: WebviewApi;
}) {
  const commands = useSidebarStore((state) => state.hud.commands);
  const globalCommands = useSidebarStore((state) => state.hud.globalCommands);
  const [editorState, setEditorState] = useState<SettingsCommandScopeEditorState>();

  const emptyGlobalCommands = useMemo<SidebarCommandButton[]>(() => [], []);
  const { orderedCommands, setDraftCommandIds } = useSettingsCommandOrder(commands);
  const {
    orderedCommands: orderedGlobalCommands,
    setDraftCommandIds: setDraftGlobalCommandIds,
  } = useSettingsCommandOrder(globalCommands ?? emptyGlobalCommands);
  /*
  CDXC:ProjectActions 2026-06-15-15:29:
  When no Actions have a saved terminal command or browser URL, the top of Settings > Actions should explain that frequently used commands can be set here for one-click or hotkey execution.
  */
  const hasConfiguredActions = useMemo(
    () =>
      [...orderedGlobalCommands, ...orderedCommands].some((command) =>
        isSidebarCommandConfigured(command),
      ),
    [orderedCommands, orderedGlobalCommands],
  );

  const deleteCommand = (scope: SettingsCommandScope, commandId: string) => {
    vscode?.postMessage({
      commandId,
      type: scope === "global" ? "deleteGlobalSidebarCommand" : "deleteSidebarCommand",
    });
    setEditorState(undefined);
  };

  const saveCommand = (scope: SettingsCommandScope, draft: SettingsCommandDraft) => {
    if (!vscode) {
      return;
    }
    /*
     * The two messages carry an identical payload and differ only in `type`,
     * but the bridge message union discriminates on that field, so it is
     * written as a literal in each branch rather than computed.
     */
    const payload = {
      actionType: draft.actionType,
      closeTerminalOnExit: draft.closeTerminalOnExit,
      command: draft.command,
      commandId: draft.commandId,
      icon: draft.icon,
      links: draft.links,
      name: draft.name,
      playCompletionSound: draft.playCompletionSound,
      showOnProjectRow: draft.showOnProjectRow,
      url: draft.url,
    };
    if (scope === "global") {
      vscode.postMessage({ ...payload, type: "saveGlobalSidebarCommand" });
    } else {
      vscode.postMessage({ ...payload, type: "saveSidebarCommand" });
    }
    setEditorState(undefined);
  };

  const reorderCommands = (scope: SettingsCommandScope, nextCommandIds: string[]) => {
    if (scope === "global") {
      setDraftGlobalCommandIds(nextCommandIds);
    } else {
      setDraftCommandIds(nextCommandIds);
    }
    vscode?.postMessage({
      commandIds: nextCommandIds,
      requestId: createSettingsReorderRequestId(scope === "global" ? "globalActions" : "actions"),
      type: scope === "global" ? "syncGlobalSidebarCommandOrder" : "syncSidebarCommandOrder",
    });
  };

  if (!editorState && search.tab.isSearching && !hasVisibleSettingsSearchResult(search.tab)) {
    return (
      <SettingsNativeScrollArea className="h-full min-h-0">
        <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">{searchEmptyState}</div>
      </SettingsNativeScrollArea>
    );
  }

  /*
   * Editing replaces both lists with the single editor, the same way the one
   * Actions list behaved before Global Actions existed. Showing the other list
   * alongside an open editor would let a user start a second edit and lose the
   * first draft.
   */
  if (editorState) {
    const editorScope = editorState.scope;
    const editorCommandId = editorState.draft.commandId;
    return (
      <SettingsNativeScrollArea className="h-full min-h-0">
        <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
          <SettingsSection title={editorScope === "global" ? "Global Action" : "Action"}>
            <ActionSettingsEditor
              draft={editorState.draft}
              existingCommands={editorScope === "global" ? orderedGlobalCommands : commands}
              lockedActionType={editorState.lockedActionType}
              onCancel={() => setEditorState(undefined)}
              onDelete={
                editorCommandId ? () => deleteCommand(editorScope, editorCommandId) : undefined
              }
              onSave={(draft) => saveCommand(editorScope, draft)}
            />
          </SettingsSection>
        </div>
      </SettingsNativeScrollArea>
    );
  }

  return (
    <SettingsNativeScrollArea className="h-full min-h-0">
      <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
        {!hasConfiguredActions ? (
          <div className="flex items-start gap-3 border border-border bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
            <IconInfoCircle aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-foreground" />
            <p className="m-0">
              Set frequently used terminal or browser commands here so you can run them with one
              click or a hotkey.
            </p>
          </div>
        ) : null}
        <ActionsSettingsSection
          commands={orderedGlobalCommands}
          description="Global actions apply to every project and are stored by the Ghostex daemon, so they follow you to every app that connects to it. They appear in the tab strip above your tabs."
          emptyDescription="Add a terminal or browser action that should be available in every project."
          emptyTitle="No global actions configured"
          onCreate={(actionType) =>
            setEditorState({
              draft: createSettingsCommandDraft(actionType),
              lockedActionType: actionType,
              scope: "global",
            })
          }
          onDelete={(commandId) => deleteCommand("global", commandId)}
          onEdit={(command) =>
            setEditorState({
              draft: createSettingsCommandDraftFromButton(command),
              scope: "global",
            })
          }
          onReorder={(nextCommandIds) => reorderCommands("global", nextCommandIds)}
          title="Global Actions"
          vscode={vscode}
        />
        <ActionsSettingsSection
          commands={orderedCommands}
          /*
           * CDXC:ActionsSettings 2026-06-15-14:00:
           * The Actions section header needs explanatory copy because users may
           * not know that terminal actions run in quick command terminals,
           * browser actions open panes, project actions are shared with
           * worktrees, and right-click exposes every configured project action.
           */
          description="Actions are custom shortcuts for repeat work. Add terminal actions to run saved commands in quick command terminals, or browser actions to open saved URLs in browser panes. These actions are shared between a main project and its worktrees, and you can right-click the action button to show all configured actions for that project."
          emptyDescription="Add a terminal or browser action."
          emptyTitle="No actions configured"
          onCreate={(actionType) =>
            setEditorState({
              draft: createSettingsCommandDraft(actionType),
              lockedActionType: actionType,
              scope: "project",
            })
          }
          onDelete={(commandId) => deleteCommand("project", commandId)}
          onEdit={(command) =>
            setEditorState({
              draft: createSettingsCommandDraftFromButton(command),
              scope: "project",
            })
          }
          onReorder={(nextCommandIds) => reorderCommands("project", nextCommandIds)}
          title="Project Actions"
          vscode={vscode}
        />
        {/*
         * CDXC:GlobalActions 2026-08-01:
         * The built-in tab strip buttons are toggled here, next to the Global
         * Actions that share the strip with them, because that is where a user
         * goes when the strip is too crowded. The pane actions menu has no
         * toggle: it is the only route to the remaining pane actions.
         */}
        <SettingsSection
          description="Global actions share the tab strip with these built-in buttons. Hide the ones you do not use to make room."
          title="Tab Strip Buttons"
        >
          <ToggleField
            checked={hideTabStripNewTerminalButton}
            description="Hide the New Terminal button from the tab strip."
            {...getSettingModificationProps("hideTabStripNewTerminalButton")}
            label="Hide New Terminal button"
            onChange={onHideTabStripNewTerminalButtonChange}
          />
          <ToggleField
            checked={hideTabStripNewBrowserButton}
            description="Hide the New Browser Tab button from the tab strip."
            {...getSettingModificationProps("hideTabStripNewBrowserButton")}
            label="Hide New Browser Tab button"
            onChange={onHideTabStripNewBrowserButtonChange}
          />
        </SettingsSection>
      </div>
    </SettingsNativeScrollArea>
  );
}

function ActionsSettingsSection({
  commands,
  description,
  emptyDescription,
  emptyTitle,
  onCreate,
  onDelete,
  onEdit,
  onReorder,
  title,
  vscode,
}: {
  commands: readonly SidebarCommandButton[];
  description: string;
  emptyDescription: string;
  emptyTitle: string;
  onCreate: (actionType: SidebarActionType) => void;
  onDelete: (commandId: string) => void;
  onEdit: (command: SidebarCommandButton) => void;
  onReorder: (nextCommandIds: string[]) => void;
  title: string;
  vscode?: WebviewApi;
}) {
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

    onReorder(
      moveId(
        commands.map((command) => command.commandId),
        source.initialIndex,
        targetIndex,
      ),
    );
  }) satisfies DragDropEventHandlers["onDragEnd"];

  return (
    <SettingsSection
      actions={
        <>
          <SettingButton
            disabled={!vscode}
            disabledReason="Adding actions needs the Ghostex app connection."
            onClick={() => onCreate("terminal")}
            type="button"
            variant="outline"
          >
            <IconPlus aria-hidden="true" data-icon="inline-start" />
            Terminal Action
          </SettingButton>
          <SettingButton
            disabled={!vscode}
            disabledReason="Adding actions needs the Ghostex app connection."
            onClick={() => onCreate("browser")}
            type="button"
            variant="outline"
          >
            <IconPlus aria-hidden="true" data-icon="inline-start" />
            Browser Action
          </SettingButton>
        </>
      }
      description={description}
      title={title}
    >
      {commands.length > 0 ? (
        <DragDropProvider onDragEnd={handleDragEnd}>
          <div className="flex flex-col gap-2">
            {commands.map((command, index) => (
              <SettingsCommandRow
                command={command}
                index={index}
                key={command.commandId}
                onEdit={() => onEdit(command)}
                onDelete={() => onDelete(command.commandId)}
              />
            ))}
          </div>
        </DragDropProvider>
      ) : (
        <Empty className="border border-border bg-muted/20">
          <EmptyHeader>
            <EmptyTitle>{emptyTitle}</EmptyTitle>
            <EmptyDescription>{emptyDescription}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </SettingsSection>
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
  const { handleRef, isDragging } = sortable;

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className="settings-management-row flex items-center gap-2 border border-border bg-muted/20 p-2"
      data-dragging={String(Boolean(isDragging))}
      ref={setRowRef}
    >
      <Button
        aria-label={`Reorder ${getActionTitle(command)}`}
        ref={handleRef}
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
  onDelete,
  onSave,
}: {
  draft: SettingsCommandDraft;
  existingCommands: readonly SidebarCommandButton[];
  lockedActionType?: SidebarActionType;
  onCancel: () => void;
  onDelete?: () => void;
  onSave: (draft: SettingsCommandDraft) => void;
}) {
  const [actionType, setActionType] = useState<SidebarActionType>(draft.actionType);
  const [closeTerminalOnExit, setCloseTerminalOnExit] = useState(draft.closeTerminalOnExit);
  const [command, setCommand] = useState(draft.command ?? "");
  const [icon, setIcon] = useState<SidebarCommandIcon>(
    draft.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
  );
  const [links, setLinks] = useState<SidebarCommandLink[]>(draft.links ?? []);
  const [name, setName] = useState(draft.name);
  const [playCompletionSound, setPlayCompletionSound] = useState(draft.playCompletionSound);
  const [showOnProjectRow, setShowOnProjectRow] = useState(draft.showOnProjectRow);
  const [url, setUrl] = useState(
    draft.url ??
      ((lockedActionType ?? draft.actionType) === "browser" ? DEFAULT_BROWSER_ACTION_URL : ""),
  );
  const actionTypeId = useId();
  const closeTerminalOnExitId = useId();
  const commandId = useId();
  const nameId = useId();
  const showOnProjectRowId = useId();
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

  const getDraft = (): SettingsCommandDraft => ({
    actionType,
    closeTerminalOnExit: actionType === "terminal" ? closeTerminalOnExit : false,
    command: actionType === "terminal" ? command.trim() : undefined,
    commandId: draft.commandId,
    icon,
    links:
      actionType === "terminal"
        ? links
            .map((link) => ({ target: link.target, url: link.url.trim() }))
            .filter((link) => link.url.length > 0)
        : undefined,
    name: trimmedName,
    playCompletionSound: actionType === "terminal" ? playCompletionSound : false,
    showOnProjectRow,
    url: actionType === "browser" ? url.trim() : undefined,
  });

  const updateLink = (index: number, update: Partial<SidebarCommandLink>) => {
    setLinks((currentLinks) =>
      currentLinks.map((link, linkIndex) =>
        linkIndex === index ? { ...link, ...update } : link,
      ),
    );
  };

  return (
    <>
      {isActionTypeLocked ? null : (
        <Field className="gap-2.5">
          <FieldContent>
            <FieldLabel className="text-sm" htmlFor={actionTypeId}>
              Type
            </FieldLabel>
          </FieldContent>
          <SettingsSelect
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
            <SettingsSelectContent>
              <SelectGroup>
                <SelectItem value="terminal">Terminal</SelectItem>
                <SelectItem value="browser">Browser</SelectItem>
              </SelectGroup>
            </SettingsSelectContent>
          </SettingsSelect>
        </Field>
      )}
      <Field className="gap-2.5" data-invalid={hasDuplicateTitle || undefined}>
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={nameId}>
            Text
          </FieldLabel>
        </FieldContent>
        <SettingsInput
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
        onIconChange={setIcon}
      />
      {actionType === "browser" ? (
        <Field className="gap-2.5">
          <FieldContent>
            <FieldLabel className="text-sm" htmlFor={urlId}>
              URL
            </FieldLabel>
          </FieldContent>
          <SettingsTextarea
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
            <SettingsTextarea
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
          {/*
           * CDXC:ProjectActions 2026-07-31-12:00:
           * Terminal actions can open saved links whenever they run, so a dev
           * action can start the server and bring up its localhost URL in the
           * same click. Each link picks the project's integrated browser or the
           * user's default external browser.
           */}
          <Field className="gap-2.5">
            <FieldContent>
              <FieldLabel className="text-sm">Open links when this action runs</FieldLabel>
              <FieldDescription className="text-sm">
                Open saved URLs, like your dev server&apos;s localhost address, alongside the
                command. Each link can open in the project&apos;s integrated browser or your
                default browser.
              </FieldDescription>
            </FieldContent>
            {links.length > 0 ? (
              <div className="flex flex-col gap-2">
                {links.map((link, index) => (
                  <div className="flex items-center gap-2" key={index}>
                    <SettingsInput
                      aria-label={`Link ${index + 1} URL`}
                      autoFocus={link.url.length === 0}
                      className="h-10 min-w-0 flex-1 px-3 text-sm"
                      onChange={(event) => updateLink(index, { url: event.currentTarget.value })}
                      placeholder={DEFAULT_BROWSER_ACTION_URL}
                      value={link.url}
                    />
                    <SettingsSelect
                      onValueChange={(value) =>
                        updateLink(index, {
                          target: value === "external" ? "external" : "integrated",
                        })
                      }
                      value={link.target}
                    >
                      <SelectTrigger
                        aria-label={`Link ${index + 1} target`}
                        className="h-10 w-44 shrink-0 px-3 text-sm"
                      >
                        <SelectValue />
                      </SelectTrigger>
                      <SettingsSelectContent>
                        <SelectGroup>
                          <SelectItem value="integrated">Integrated browser</SelectItem>
                          <SelectItem value="external">External browser</SelectItem>
                        </SelectGroup>
                      </SettingsSelectContent>
                    </SettingsSelect>
                    <Button
                      aria-label={`Remove link ${index + 1}`}
                      onClick={() =>
                        setLinks((currentLinks) =>
                          currentLinks.filter((_, linkIndex) => linkIndex !== index),
                        )
                      }
                      size="icon-sm"
                      type="button"
                      variant="ghost"
                    >
                      <IconX aria-hidden="true" />
                    </Button>
                  </div>
                ))}
              </div>
            ) : null}
            <Button
              className="self-start"
              onClick={() => setLinks([...links, { target: "integrated", url: "" }])}
              size="sm"
              type="button"
              variant="outline"
            >
              <IconPlus aria-hidden="true" data-icon="inline-start" />
              Add link
            </Button>
          </Field>
        </>
      )}
      {/*
       * CDXC:ProjectActions 2026-08-01:
       * Both terminal and browser actions can opt into the project's sidebar
       * row, so this toggle lives outside the action-type branch above.
       */}
      <Field className="items-center justify-between" orientation="horizontal">
        <FieldContent>
          <FieldLabel className="text-sm" htmlFor={showOnProjectRowId}>
            Show on the project&apos;s sidebar row
          </FieldLabel>
        </FieldContent>
        <Switch
          checked={showOnProjectRow}
          id={showOnProjectRowId}
          onCheckedChange={setShowOnProjectRow}
        />
      </Field>
      {/*
       * CDXC:ActionsSettings 2026-06-18-10:11:
       * Settings > Actions must let users delete any selected action from the edit surface itself, including default Build/Test actions whose deletion is represented by deletedDefaultCommandIds. Keep this wired to the same deleteSidebarCommand path as the row trash button so default and custom actions share one behavior.
       */}
      <div className="flex items-center justify-between gap-3">
        {onDelete ? (
          <Button onClick={onDelete} type="button" variant="destructive">
            <IconTrash aria-hidden="true" data-icon="inline-start" />
            Delete
          </Button>
        ) : (
          <span aria-hidden="true" />
        )}
        <div className="flex justify-end gap-3">
          <Button onClick={onCancel} type="button" variant="outline">
            Cancel
          </Button>
          <SettingButton
            disabled={isSaveDisabled}
            disabledReason={
              hasDuplicateTitle
                ? "Choose a unique action title."
                : actionType === "browser"
                  ? "Enter a URL first."
                  : "Enter a command first."
            }
            onClick={() => onSave(getDraft())}
            type="button"
          >
            Save
          </SettingButton>
        </div>
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
  definitionsById,
  expandCollapsedProjectsOnJump,
  expandCollapsedProjectsOnJumpModification,
  hotkeys,
  onActiveSectionChange,
  onChange,
  onExpandCollapsedProjectsOnJumpChange,
  onShowLessForExpandedProjectJumpsChange,
  searchQuery,
  sectionRefs,
  sectionSearches,
  showLessForExpandedProjectJumps,
  showLessForExpandedProjectJumpsModification,
  visibleSections,
}: {
  definitionsById: HotkeySettingsDefinitionById;
  expandCollapsedProjectsOnJump: boolean;
  expandCollapsedProjectsOnJumpModification: Required<SettingModificationProps>;
  hotkeys?: ghostexHotkeySettings;
  onActiveSectionChange: (sectionId: HotkeySettingsSectionId) => void;
  onChange: (hotkeys: ghostexHotkeySettings) => void;
  onExpandCollapsedProjectsOnJumpChange: (checked: boolean) => void;
  onShowLessForExpandedProjectJumpsChange: (checked: boolean) => void;
  searchQuery: string;
  sectionRefs: HotkeySettingsSectionRefs;
  sectionSearches: HotkeySettingsSectionSearches;
  showLessForExpandedProjectJumps: boolean;
  showLessForExpandedProjectJumpsModification: Required<SettingModificationProps>;
  visibleSections: readonly HotkeySettingsSectionDefinition[];
}) {
  const normalizedHotkeys = normalizeghostexHotkeySettings(hotkeys);
  const defaultHotkeys = normalizeghostexHotkeySettings(DEFAULT_ghostex_HOTKEYS);
  const duplicateIds = useMemo(
    () => getDuplicateHotkeyIds(normalizedHotkeys),
    [normalizedHotkeys],
  );
  const pendingHotkeySectionViewportRef = useRef<HTMLElement | null>(null);
  const hotkeySectionFrameRef = useRef<number | undefined>(undefined);
  /**
   * CDXC:Hotkeys 2026-05-13-16:05
   * Superseded by CDXC:SettingsNavigation 2026-06-24-22:16.
   *
   * CDXC:SettingsNavigation 2026-06-24-22:16:
   * Hotkey section refs and search results are owned by SettingsModal so the
   * shared sidebar can expand Hotkeys and jump into its internal sections.
   * The same top search query filters General and Hotkeys instead of keeping a
   * hidden tab-specific search state.
   */
  const visibleHotkeySectionNavigation: SettingsSectionNavigationItem<HotkeySettingsSectionId>[] =
    visibleSections.map((section) => ({
      id: section.id,
      title: section.title,
    }));
  const visibleHotkeySectionMeasurementItems: SettingsSectionMeasurementItem<HotkeySettingsSectionId>[] =
    visibleSections.map((section) => ({
      id: section.id,
      ref: sectionRefs[section.id],
    }));
  const visibleHotkeySectionIds = visibleHotkeySectionNavigation
    .map((section) => section.id)
    .join("|");
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
    onChange(defaultHotkeys);
  };

  const scheduleHotkeySectionMeasurement = (viewport: HTMLElement) => {
    /*
     * CDXC:SettingsPerformance 2026-06-29-00:40:
     * Hotkeys uses the same active-section measurement as General Settings.
     * Keep scroll handlers cheap by measuring section rects once per animation
     * frame instead of on every scroll event.
     */
    pendingHotkeySectionViewportRef.current = viewport;
    if (hotkeySectionFrameRef.current !== undefined) {
      return;
    }
    hotkeySectionFrameRef.current = requestAnimationFrame(() => {
      hotkeySectionFrameRef.current = undefined;
      const pendingViewport = pendingHotkeySectionViewportRef.current;
      pendingHotkeySectionViewportRef.current = null;
      if (!pendingViewport?.isConnected) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        pendingViewport,
        visibleHotkeySectionMeasurementItems,
      );
      if (mostlyVisibleSectionId) {
        onActiveSectionChange(mostlyVisibleSectionId);
      }
    });
  };

  const handleHotkeySettingsScrollCapture = (event: ReactUIEvent<HTMLDivElement>) => {
    if (!(event.target instanceof HTMLElement) || event.target.dataset.slot !== "scroll-area-viewport") {
      return;
    }
    scheduleHotkeySectionMeasurement(event.target);
  };

  useEffect(() => {
    return () => {
      if (hotkeySectionFrameRef.current !== undefined) {
        cancelAnimationFrame(hotkeySectionFrameRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const animationFrame = requestAnimationFrame(() => {
      const firstSection = visibleHotkeySectionMeasurementItems[0];
      const viewport = firstSection?.ref.current?.closest<HTMLElement>("[data-slot='scroll-area-viewport']");
      if (!viewport) {
        return;
      }
      const mostlyVisibleSectionId = getMostlyVisibleSettingsSectionId(
        viewport,
        visibleHotkeySectionMeasurementItems,
      );
      if (mostlyVisibleSectionId) {
        onActiveSectionChange(mostlyVisibleSectionId);
      }
    });
    return () => cancelAnimationFrame(animationFrame);
  }, [onActiveSectionChange, searchQuery, visibleHotkeySectionIds]);

  return (
    <div className="settings-main-tab-layout">
      <SettingsNativeScrollArea
        className="settings-main-scroll h-full min-h-0"
        onScrollCapture={handleHotkeySettingsScrollCapture}
      >
        <div className="settings-page-width flex flex-col gap-6 px-5 pb-5">
          {visibleSections.map((section) => (
            <SettingsSection
              key={section.id}
              sectionRef={sectionRefs[section.id]}
              title={section.title}
            >
              {section.id === "projects" &&
              shouldShowSetting(sectionSearches.projects, "expandCollapsedProjectsOnJump") ? (
                <ToggleField
                  checked={expandCollapsedProjectsOnJump}
                  description="Reveal a collapsed project row before focusing it from Jump to Project hotkeys."
                  label="Expand Collapsed Projects on Jump"
                  {...expandCollapsedProjectsOnJumpModification}
                  onChange={onExpandCollapsedProjectsOnJumpChange}
                />
              ) : null}
              {section.id === "projects" &&
              expandCollapsedProjectsOnJump &&
              shouldShowSetting(sectionSearches.projects, "showLessForExpandedProjectJumps") ? (
                <ToggleField
                  checked={showLessForExpandedProjectJumps}
                  description="After a project jump expands a collapsed project, switch that project session list to Show less."
                  label="Use Show less After Jump Expand"
                  {...showLessForExpandedProjectJumpsModification}
                  onChange={onShowLessForExpandedProjectJumpsChange}
                />
              ) : null}
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
                      originalHotkey={defaultHotkeys[definition.id] ?? ""}
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
      </SettingsNativeScrollArea>
    </div>
  );
}

function SettingsAgentIcon({ agent }: { agent: SidebarAgentButton }) {
  if (agent.icon) {
    return (
      <span
        aria-hidden="true"
        className="configure-agents-list-agent-icon"
        style={getBrandAgentLogoStyle(agent.icon)}
      />
    );
  }

  return <IconCodeDots aria-hidden="true" />;
}

function SettingsActionIcon({ command }: { command: SidebarCommandButton }) {
  return (
    <SidebarCommandIconGlyph
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

function createSettingsCommandDraft(actionType: SidebarActionType): SettingsCommandDraft {
  return {
    actionType,
    closeTerminalOnExit: false,
    command: actionType === "terminal" ? "" : undefined,
    commandId: undefined,
    icon: DEFAULT_SIDEBAR_COMMAND_ICON,
    links: [],
    name: "",
    playCompletionSound: actionType === "terminal",
    showOnProjectRow: false,
    url: actionType === "browser" ? DEFAULT_BROWSER_ACTION_URL : undefined,
  };
}

function createSettingsCommandDraftFromButton(command: SidebarCommandButton): SettingsCommandDraft {
  return {
    actionType: command.actionType,
    closeTerminalOnExit: command.closeTerminalOnExit,
    command: command.command,
    commandId: command.commandId,
    icon: command.icon ?? DEFAULT_SIDEBAR_COMMAND_ICON,
    links: command.links ?? [],
    name: command.name,
    playCompletionSound: command.playCompletionSound,
    showOnProjectRow: command.showOnProjectRow,
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

function createSettingsSidebarTagListItemDragData(
  itemId: string,
): SettingsSidebarTagListItemDragData {
  return {
    itemId,
    kind: "settings-sidebar-tag-list-item",
  };
}

function getSettingsSidebarTagListItemDragData(
  candidate: unknown,
): SettingsSidebarTagListItemDragData | undefined {
  if (!hasData(candidate)) {
    return undefined;
  }

  const data = candidate.data;
  if (
    !isObjectRecord(data) ||
    data.kind !== "settings-sidebar-tag-list-item" ||
    typeof data.itemId !== "string"
  ) {
    return undefined;
  }

  return {
    itemId: data.itemId,
    kind: "settings-sidebar-tag-list-item",
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

function createSettingsReorderRequestId(kind: "actions" | "agents" | "globalActions"): string {
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

function getHotkeySettingsSectionSearches({
  definitionsById,
  expandCollapsedProjectsOnJump,
  searchQuery,
}: {
  definitionsById: HotkeySettingsDefinitionById;
  expandCollapsedProjectsOnJump: boolean;
  searchQuery: string;
}): HotkeySettingsSectionSearches {
  return Object.fromEntries(
    HOTKEY_SETTINGS_SECTIONS.map((section) => {
      const projectJumpSettings: SettingSearchDefinition[] =
        section.id === "projects"
          ? [
              {
                key: "expandCollapsedProjectsOnJump",
                subtitle: "Reveal a collapsed Projects row before focusing it from Jump to Project hotkeys.",
                title: "Expand collapsed projects on jump",
              },
              ...(expandCollapsedProjectsOnJump
                ? [
                    {
                      key: "showLessForExpandedProjectJumps",
                      subtitle:
                        "After a project jump expands a collapsed project, switch that project session list to Show less.",
                      title: "Use Show less after jump expand",
                    },
                  ]
                : []),
            ]
          : [];
      return [
        section.id,
        getSettingsSectionSearch(
          searchQuery,
          section.title,
          [
            ...projectJumpSettings,
            ...section.ids.flatMap((id) => {
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
          ],
        ),
      ];
    }),
  ) as HotkeySettingsSectionSearches;
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

function getGroupedSettingsSectionSearch(
  query: string,
  sectionTitle: string,
  sections: readonly SettingsSectionSearchResult[],
): SettingsSectionSearchResult {
  const groupTitleResult = getSettingsSectionSearch(query, sectionTitle, []);
  const visibleSettingKeys = new Set<string>(groupTitleResult.visibleSettingKeys);
  for (const section of sections) {
    for (const settingKey of section.visibleSettingKeys) {
      visibleSettingKeys.add(settingKey);
    }
  }
  return {
    groupTitleMatches: groupTitleResult.sectionMatches,
    isSearching: groupTitleResult.isSearching || sections.some((section) => section.isSearching),
    sectionMatches:
      groupTitleResult.sectionMatches || sections.some((section) => section.sectionMatches),
    visibleSettingKeys,
  };
}

function hasVisibleSettingsSearchResult(result: SettingsSectionSearchResult): boolean {
  return result.sectionMatches || result.visibleSettingKeys.size > 0;
}

type SettingsTabSearchSectionDefinition = {
  id: string;
  settings: readonly SettingSearchDefinition[];
  title: string;
};

type SettingsTabSearch = {
  sections: Record<string, SettingsSectionSearchResult>;
  tab: SettingsSectionSearchResult;
};

type SearchableExtraSettingsTabId =
  | "about"
  | "actions"
  | "agents"
  | "integrations"
  | "plugins"
  | "openTargets"
  | "osIntegration"
  | "projects"
  | "remote";

type ExtraSettingsTabSearches = Record<SearchableExtraSettingsTabId, SettingsTabSearch>;

/**
 * CDXC:SettingsSearch 2026-07-22-00:00:
 * The one global Settings search field must find settings on every Settings
 * page, not only General and Hotkeys. Non-General pages keep their own static
 * search definitions here so the sidebar can filter pages to those with
 * matches and each page can filter its own sections and rows.
 */
const EXTRA_SETTINGS_TAB_SEARCH_SECTIONS: Record<
  SearchableExtraSettingsTabId,
  { sections: readonly SettingsTabSearchSectionDefinition[]; title: string }
> = {
  about: {
    sections: [
      {
        id: "about",
        settings: [
          { key: "version", subtitle: "Ghostex app version.", title: "Version" },
          { key: "discord", subtitle: "Chat with the community and get help.", title: "Join Discord" },
          {
            key: "github",
            subtitle: "View the source, releases, and report issues.",
            title: "View on GitHub",
          },
          {
            key: "sponsor",
            subtitle: "Support the continued development of Ghostex.",
            title: "Sponsor Ghostex",
          },
        ],
        title: "About",
      },
    ],
    title: "About",
  },
  actions: {
    sections: [
      {
        id: "actions",
        settings: [
          {
            key: "terminalAction",
            subtitle:
              "Add terminal actions to run saved commands in quick command terminals with one click or a hotkey.",
            title: "Terminal Action",
          },
          {
            key: "browserAction",
            subtitle: "Add browser actions to open saved URLs in browser panes.",
            title: "Browser Action",
          },
          {
            key: "actionShortcuts",
            subtitle:
              "Actions are custom shortcuts for repeat work, shared between a main project and its worktrees.",
            title: "Custom actions",
          },
          {
            key: "globalActions",
            subtitle:
              "Global actions apply to every project, are stored by the Ghostex daemon, and appear in the tab strip above your tabs.",
            title: "Global Actions",
          },
          {
            key: "hideTabStripNewTerminalButton",
            subtitle: "Hide the New Terminal button from the tab strip.",
            title: "Hide New Terminal button",
          },
          {
            key: "hideTabStripNewBrowserButton",
            subtitle: "Hide the New Browser Tab button from the tab strip.",
            title: "Hide New Browser Tab button",
          },
        ],
        title: "Actions",
      },
    ],
    title: "Actions",
  },
  agents: {
    sections: [
      {
        id: "agentHooks",
        settings: [
          {
            key: "agentResumeHooks",
            options: AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.map((agent) => ({
              label: agent.name,
              value: agent.name,
            })),
            subtitle:
              "Install hooks so Ghostex can capture each agent's native session id and resume the exact conversation after sleep, reload, or app restart.",
            title: "Agent resume hooks",
          },
        ],
        title: "Agent Hooks",
      },
      {
        id: "config",
        settings: [
          {
            key: "defaultPromptAgent",
            subtitle:
              "Choose the agent used by Git helper prompts, project board Start Work, and the default worktree first-prompt selection.",
            title: "Default Prompt Agent",
          },
          {
            key: "titleGenerationAgent",
            options: SESSION_TITLE_GENERATION_AGENT_OPTIONS,
            subtitle:
              "Choose the headless agent Ghostex uses for first-prompt session title generation.",
            title: "Title Generation Agent",
          },
          {
            key: "titleGenerationCommand",
            subtitle:
              "Preview of the command Ghostex sends to generate automatic first-prompt session titles.",
            title: "Title Generation Command",
          },
          {
            key: "customTitleCommand",
            subtitle:
              "Run this command with the title prompt on stdin. It should print only the title.",
            title: "Custom Title Command",
          },
          {
            key: "acceptAll",
            subtitle:
              "Enable each supported agent's permission-bypass mode when launching sessions. Per-agent settings can inherit or override this default.",
            title: "Accept All",
          },
        ],
        title: "Config",
      },
      {
        id: "agentList",
        settings: [
          {
            key: "addAgent",
            options: DEFAULT_SIDEBAR_AGENTS.map((agent) => ({
              label: agent.name,
              value: agent.name,
            })),
            subtitle: "Add, reorder, edit, or delete agent launchers used to start new sessions.",
            title: "Add Agent",
          },
        ],
        title: "Agents",
      },
    ],
    title: "Agents",
  },
  integrations: {
    sections: [
      {
        id: "integrations",
        settings: [
          {
            key: "ghostexCli",
            subtitle:
              "Ghostex keeps the app-bundled ghostex command linked automatically for mobile apps and CLI-backed integration setup.",
            title: "Ghostex CLI",
          },
          {
            key: "bundledAgentSkills",
            options: BUNDLED_GHOSTEX_AGENT_SKILLS.map((skill) => ({
              label: skill.name,
              value: skill.skillName,
            })),
            subtitle:
              "Install the Ghostex skills you want agents to discover. Each skill is copied to ~/.agents/skills and can be updated independently.",
            title: "Bundled Agent Skills",
          },
          {
            key: "appShots",
            options: APP_SHOTS_HOTKEY_OPTIONS,
            subtitle:
              "Capture the frontmost app window, then stage it in the focused or recent agent session as local image context.",
            title: "App Shots",
          },
          {
            key: "cuaPermissions",
            subtitle:
              "Cua Driver needs Accessibility to click and type in apps, and Screen Recording to understand what is visible on the desktop.",
            title: "Cua Permissions",
          },
        ],
        title: "Integrations",
      },
      {
        id: "recovery",
        settings: [
          {
            key: "uninstallHooks",
            subtitle: "Remove Ghostex-owned agent hook setup artifacts.",
            title: "Uninstall Hooks",
          },
          {
            key: "uninstallSkills",
            subtitle: "Remove installed bundled Ghostex agent skills.",
            title: "Uninstall Skills",
          },
        ],
        title: "Hooks & Skills",
      },
    ],
    title: "Integrations",
  },
  plugins: {
    sections: [
      {
        id: "viewTabs",
        settings: [
          { key: "code", subtitle: "Show Code in the title bar and manage its VS Code runtime.", title: "Code" },
          { key: "browser", subtitle: "Show or hide Browser in the title bar.", title: "Browser" },
          { key: "kanban", subtitle: "Show Kanban in the title bar and manage its Beads runtime.", title: "Kanban" },
          { key: "automate", subtitle: "Show or hide Automate in the title bar.", title: "Automate" },
          { key: "docs", subtitle: "Show or hide Docs in the title bar.", title: "Docs" },
        ],
        title: "Plugins",
      },
      {
        id: "components",
        settings: [
          {
            key: "cuaDriver",
            subtitle:
              "Install or upgrade Cua Driver for Ghostex Browser Use and native Desktop Control.",
            title: "Cua Driver",
          },
          { key: "cef", subtitle: "Inspect or reinstall the Chromium runtime used by Ghostex web surfaces.", title: "Chromium runtime (CEF)" },
        ],
        title: "Shared components",
      },
      {
        id: "quickAccessButtons",
        settings: [
          { key: "tips", subtitle: "Show or hide the Tips & Tricks titlebar button.", title: "Tips & Tricks" },
          { key: "resources", subtitle: "Show or hide the Resources titlebar button.", title: "Resources" },
          { key: "gitActions", subtitle: "Show or hide the Git actions titlebar button.", title: "Git actions" },
          { key: "quickActions", subtitle: "Show or hide the Quick Actions titlebar button.", title: "Quick Actions" },
          { key: "openIn", subtitle: "Show or hide the Open In titlebar button.", title: "Open In" },
        ],
        title: "Quick access buttons",
      },
    ],
    title: "Customize",
  },
  openTargets: {
    sections: [
      {
        id: "openIn",
        settings: BUILT_IN_WORKSPACE_OPEN_TARGETS.map((target) => ({
          key: `builtin:${target.id}`,
          subtitle: "Show or hide this app on session Open In menus.",
          title: target.label,
        })),
        title: "Open In",
      },
      {
        id: "customOpenTargets",
        settings: [
          {
            key: "addTarget",
            subtitle: "Add a custom command Ghostex uses to open workspaces.",
            title: "Add target",
          },
        ],
        title: "Custom Open Targets",
      },
    ],
    title: "Open In",
  },
  osIntegration: {
    sections: [
      {
        id: "defaults",
        settings: [
          {
            key: "setDefaultEditor",
            subtitle: "Make Ghostex the default macOS editor for supported file types.",
            title: "Set as Default Editor",
          },
          {
            key: "setTerminalLinks",
            subtitle: "Make Ghostex the handler for ghostex:// terminal links.",
            title: "Set Terminal Links",
          },
          {
            key: "setScriptRunner",
            subtitle: "Make Ghostex the default macOS script runner.",
            title: "Set Script Runner",
          },
          {
            key: "setAll",
            subtitle: "Set Ghostex as default editor, terminal-link handler, and script runner.",
            title: "Set All",
          },
        ],
        title: "Defaults",
      },
      {
        id: "cli",
        settings: [
          {
            key: "cliCommands",
            subtitle: "Command-line examples: ghostex open, ghostex edit, ghostex terminal.",
            title: "ghostex command line",
          },
        ],
        title: "CLI",
      },
      {
        id: "diagnostics",
        settings: [
          {
            key: "handlerStatus",
            subtitle:
              "Check macOS Launch Services registration for editor defaults, script runner, and ghostex:// links.",
            title: "macOS handler status",
          },
        ],
        title: "Diagnostics",
      },
    ],
    title: "OS Integration",
  },
  projects: {
    sections: [
      {
        id: "docs",
        settings: [
          {
            key: "docsFolders",
            subtitle:
              "Comma-separated project-relative folders to scan recursively in Docs.",
            title: "Docs folders",
          },
        ],
        title: "Docs",
      },
      {
        id: "globalDefaults",
        settings: [
          {
            key: "globalWorktreeCommand",
            subtitle: "Worktree command every project uses unless it sets its own.",
            title: "Global worktree command",
          },
          {
            key: "globalTicketKey",
            subtitle: "Ticket key every project uses unless it sets its own.",
            title: "Global ticket key",
          },
          {
            key: "globalBeadsDirectory",
            subtitle: "Beads directory every project uses unless it sets its own.",
            title: "Global Beads directory",
          },
          {
            key: "globalDocsDirectory",
            subtitle:
              "Extra folder Docs shows in every project, alongside that project's own docs.",
            title: "Global Docs directory",
          },
        ],
        title: "Global Defaults",
      },
      {
        id: "projectSettings",
        settings: [
          {
            key: "worktreeCommand",
            subtitle:
              "Runs in the new worktree folder before the project is added (useful for .envs, installing dependencies, etc.).",
            title: "Worktree command",
          },
          {
            key: "ticketKey",
            subtitle:
              "Three-letter prefix used for Linear-style ticket numbers on the Project board.",
            title: "Ticket key",
          },
          {
            key: "beadsDirectory",
            subtitle:
              "Absolute path the Project board reads its Beads workspace (.beads) from.",
            title: "Beads directory",
          },
          {
            key: "docsDirectory",
            subtitle:
              "Extra folder this project's Docs surface shows, in addition to its own docs.",
            title: "Docs directory",
          },
        ],
        title: "Project settings",
      },
    ],
    title: "Projects",
  },
  remote: {
    sections: [
      {
        id: "remoteMachines",
        settings: [
          {
            key: "addMachine",
            subtitle: "Saved SSH machines appear as separate sidebar sections.",
            title: "Add remote machine",
          },
          { key: "sshHost", subtitle: "Remote machine SSH host.", title: "SSH host" },
          { key: "sshUser", subtitle: "Remote machine SSH user.", title: "SSH user" },
          { key: "sshPort", subtitle: "Remote machine SSH port.", title: "SSH port" },
          {
            key: "identityFile",
            subtitle: "SSH identity file used to connect to the remote machine.",
            title: "Identity file",
          },
          {
            key: "password",
            subtitle: "SSH passwords are stored in macOS Keychain.",
            title: "Password",
          },
          {
            key: "tailscaleSetup",
            subtitle:
              "Use Tailscale when the remote machine is not reachable on your local network.",
            title: "Tailscale setup",
          },
          {
            key: "installGxserver",
            subtitle: "Install or connect gxserver on a saved remote machine.",
            title: "Install / Connect gxserver",
          },
        ],
        title: "Remote machines",
      },
    ],
    title: "Remote",
  },
};

function getExtraSettingsTabSearch(
  query: string,
  tab: SearchableExtraSettingsTabId,
): SettingsTabSearch {
  const definition = EXTRA_SETTINGS_TAB_SEARCH_SECTIONS[tab];
  const tabTitleResult = getSettingsSectionSearch(query, definition.title, []);
  const sections = Object.fromEntries(
    definition.sections.map((section) => {
      const sectionResult = getSettingsSectionSearch(query, section.title, section.settings);
      return [
        section.id,
        // A tab-title match (e.g. "remote") should reveal the whole page, so
        // treat every section on that page as matching.
        tabTitleResult.sectionMatches
          ? { ...sectionResult, sectionMatches: true }
          : sectionResult,
      ];
    }),
  );
  return {
    sections,
    tab: getGroupedSettingsSectionSearch(query, definition.title, Object.values(sections)),
  };
}

function getExtraSettingsTabSearches(query: string): ExtraSettingsTabSearches {
  return Object.fromEntries(
    (Object.keys(EXTRA_SETTINGS_TAB_SEARCH_SECTIONS) as SearchableExtraSettingsTabId[]).map(
      (tab) => [tab, getExtraSettingsTabSearch(query, tab)],
    ),
  ) as ExtraSettingsTabSearches;
}

function settingsTabSearchHasMatches(search: SettingsTabSearch): boolean {
  return hasVisibleSettingsSearchResult(search.tab);
}

function isAdvancedMainSetting(settingKey: string): boolean {
  return ADVANCED_MAIN_SETTING_KEYS.has(settingKey);
}

function shouldShowSettingsSection(
  result: SettingsSectionSearchResult,
  showAdvancedSettings = true,
): boolean {
  if (!hasVisibleSettingsSearchResult(result)) {
    return false;
  }
  if (result.isSearching || showAdvancedSettings) {
    return true;
  }
  return Array.from(result.visibleSettingKeys).some((settingKey) => !isAdvancedMainSetting(settingKey));
}

function shouldShowSetting(
  result: SettingsSectionSearchResult,
  settingKey: string,
  showAdvancedSettings = true,
): boolean {
  if (result.isSearching) {
    return result.sectionMatches || result.visibleSettingKeys.has(settingKey);
  }
  return showAdvancedSettings || !isAdvancedMainSetting(settingKey);
}

function TerminalDevServerIgnoredPortsField({
  advanced,
  ignoredPortRules,
  isModified,
  onChange,
  onResetToDefault,
}: {
  advanced?: boolean;
  ignoredPortRules: readonly string[];
  onChange: (ignoredPortRules: readonly string[]) => void;
} & SettingModificationProps) {
  const id = useId();
  const [inputValue, setInputValue] = useState("");
  const [error, setError] = useState("");
  const addIgnoredPortRule = () => {
    const canonicalRule = normalizeTerminalDevServerIgnoredPortRuleInput(inputValue);
    if (!canonicalRule) {
      setError("Enter a port (e.g. 9229) or a range (e.g. 24678-24680).");
      return;
    }
    setError("");
    setInputValue("");
    onChange(normalizeTerminalDevServerIgnoredPortRules([...ignoredPortRules, canonicalRule]));
  };
  const removeIgnoredPortRule = (rule: string) => {
    onChange(
      normalizeTerminalDevServerIgnoredPortRules(
        ignoredPortRules.filter((ignoredPortRule) => ignoredPortRule !== rule),
      ),
    );
  };

  return (
    <SettingRow
      advanced={advanced}
      description="Servers on these ports are hidden from the server menu. Enter a port or an inclusive range."
      htmlFor={id}
      isModified={isModified}
      label="Ignored ports"
      onResetToDefault={onResetToDefault}
    >
      <div className="grid gap-3" id={id}>
        <div className="grid gap-2">
          {ignoredPortRules.length === 0 ? (
            <div className="text-sm text-muted-foreground">No ignored ports.</div>
          ) : (
            ignoredPortRules.map((rule) => (
              <div
                className="flex min-h-9 items-center justify-between gap-3 rounded-none border border-border/70 bg-card/40 px-3 py-2"
                key={rule}
              >
                <span className="min-w-0 truncate font-mono text-sm">{rule}</span>
                <Button
                  aria-label={`Remove ignored port ${rule}`}
                  onClick={() => removeIgnoredPortRule(rule)}
                  size="icon-xs"
                  type="button"
                  variant="ghost"
                >
                  <IconTrash aria-hidden="true" size={14} />
                </Button>
              </div>
            ))
          )}
        </div>
        <div className="flex items-center gap-2">
          <SettingsInput
            aria-invalid={Boolean(error)}
            aria-label="Ignored port or range"
            className="h-10 min-w-0 flex-1 px-3 text-sm"
            onChange={(event) => {
              setInputValue(event.currentTarget.value);
              if (error) {
                setError("");
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                addIgnoredPortRule();
              }
            }}
            placeholder="e.g. 9229 or 24678-24680"
            value={inputValue}
          />
          <SettingButton
            disabled={!inputValue.trim()}
            disabledReason="Enter a port or port range first."
            onClick={addIgnoredPortRule}
            type="button"
            variant="outline"
          >
            <IconPlus aria-hidden="true" data-icon="inline-start" />
            Add
          </SettingButton>
        </div>
        {error ? (
          <div className="text-sm text-destructive" role="alert">
            {error}
          </div>
        ) : null}
      </div>
    </SettingRow>
  );
}

function SettingsSection({
  actions,
  children,
  description,
  descriptionClassName,
  sectionRef,
  title,
}: {
  actions?: ReactNode;
  children: ReactNode;
  description?: ReactNode;
  descriptionClassName?: string;
  sectionRef?: RefObject<HTMLDivElement | null>;
  title: string;
}) {
  return (
    <div className="settings-section-anchor" ref={sectionRef}>
      <Card
        className={cn(
          "settings-section-card relative mt-5 overflow-visible pb-[25px] pt-8",
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
        {description ? (
          <p
            className={cn(
              "m-0 pb-5 text-sm leading-6 text-muted-foreground",
              descriptionClassName,
            )}
          >
            {description}
          </p>
        ) : null}
        <FieldGroup className="gap-6">{children}</FieldGroup>
      </CardContent>
      </Card>
    </div>
  );
}

function SliderNumberField({
  advanced,
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
  advanced?: boolean;
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
      advanced={advanced}
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
        <SettingsInput
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
  advanced,
  children,
  description,
  label,
  onClick,
}: {
  advanced?: boolean;
  children: ReactNode;
  description?: string;
  label: string;
  onClick: () => void;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <Button className="h-10 w-full justify-start px-3 text-sm" id={id} onClick={onClick} type="button">
        {children}
      </Button>
    </SettingRow>
  );
}

function ActionButtonPairField({
  advanced,
  actions,
  description,
  label,
}: {
  advanced?: boolean;
  actions: ReadonlyArray<{ label: string; onClick: () => void }>;
  description?: string;
  label: string;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
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
  advanced,
  contentClassName,
  description,
  disabled,
  disabledReason,
  isModified,
  label,
  onChange,
  onResetToDefault,
  options,
  showScrollButtons,
  supportingContent,
  value,
}: {
  advanced?: boolean;
  contentClassName?: string;
  description?: string;
  disabled?: boolean;
  disabledReason?: string;
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
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <SettingsSelect
        disabled={disabled}
        disabledReason={disabledReason}
        disabledTooltipClassName="w-full"
        items={options}
        onValueChange={onChange}
        value={value}
      >
        <SelectTrigger className="h-10 w-full px-3 text-sm" disabled={disabled} id={id}>
          <SelectValue />
        </SelectTrigger>
        <SettingsSelectContent className={contentClassName} showScrollButtons={showScrollButtons}>
          <SelectGroup>
            {options.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectGroup>
        </SettingsSelectContent>
      </SettingsSelect>
      {supportingContent}
    </SettingRow>
  );
}

function StaticNoteField({
  advanced,
  description,
  label,
  surface = "boxed",
  value = "Not available",
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  surface?: "boxed" | "plain";
  value?: string;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <div
        className={
          surface === "plain"
            ? "text-sm text-muted-foreground"
            : "rounded-none border border-border bg-muted/30 px-3 py-2 text-sm text-muted-foreground"
        }
        id={id}
      >
        {value}
      </div>
    </SettingRow>
  );
}

function PetPickerField({
  advanced,
  isModified,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  onChange: (value: PetId) => void;
  value: PetId;
} & SettingModificationProps) {
  const id = useId();
  const selectedPet = PET_OPTIONS.find((option) => option.id === value) ?? PET_OPTIONS[0]!;
  return (
    <SettingRow
      advanced={advanced}
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
          <SettingsSelect
            onValueChange={(nextValue) => onChange(nextValue as PetId)}
            value={value}
          >
            <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
              <SelectValue />
            </SelectTrigger>
            <SettingsSelectContent>
              <SelectGroup>
                {PET_OPTIONS.map((option) => (
                  <SelectItem key={option.id} value={option.id}>
                    {option.displayName}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SettingsSelectContent>
          </SettingsSelect>
          <div className="truncate text-xs text-muted-foreground">{selectedPet.description}</div>
        </div>
      </div>
    </SettingRow>
  );
}

/**
 * CDXC:AppIconPicker 2026-06-28-06:05:
 * App Icon is an advanced custom-image flow, not a preset gallery. Render one
 * selected-icon preview, one Select Image button, and an X on the custom preview
 * to restore the empty/default source id. Selection still posts to native first;
 * persistence happens upstream only after native confirms with appIconState.
 */
function AppIconPickerField({
  advanced,
  error,
  onChooseFile,
  onSelect,
  state,
}: {
  advanced?: boolean;
  error?: string;
  onChooseFile: () => void;
  onSelect: (sourceId: string) => void;
  state: SidebarAppIconStateMessage | undefined;
}) {
  const id = useId();
  const allIcons: SidebarAppIconInfo[] = state?.icons ?? [];
  const defaultIcon = allIcons.find((icon) => icon.id === "");
  const icons = allIcons.filter((icon) => icon.id !== "");
  const selectedId = state?.selectedId ?? "";
  const isDefaultSelected = selectedId === "";
  const selectedIcon = isDefaultSelected
    ? defaultIcon
    : icons.find((icon) => icon.id === selectedId);

  const previewIcon = selectedIcon ?? defaultIcon;

  return (
    <SettingRow
      advanced={advanced}
      description="Choose a PNG for the macOS Dock and app-switcher icon."
      htmlFor={id}
      label="Custom app icon"
    >
      <div className="flex min-w-0 flex-col gap-3">
        <div className="flex min-w-0 items-center gap-3">
          <div className="relative flex size-16 shrink-0 items-center justify-center overflow-visible">
            <div className="flex size-16 items-center justify-center overflow-hidden rounded-none border border-border bg-muted/30">
              {previewIcon ? (
                <img
                  alt={previewIcon.name}
                  className="size-full object-contain"
                  src={previewIcon.thumbnailDataUrl}
                />
              ) : (
                <IconPhoto aria-hidden="true" className="size-7 text-muted-foreground" />
              )}
            </div>
            {!isDefaultSelected ? (
              <Tooltip>
                <TooltipTrigger
                  render={
                    <button
                      aria-label="Use default app icon"
                      className="absolute -right-2 -top-2 flex size-6 items-center justify-center rounded-none border border-border bg-background text-muted-foreground shadow-sm hover:text-foreground"
                      onClick={() => onSelect("")}
                      type="button"
                    >
                      <IconX aria-hidden="true" className="size-3.5" />
                    </button>
                  }
                />
                <TooltipContent sideOffset={6}>Use default icon</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-2">
            <Button
              className="h-9 w-fit rounded-none px-3 text-sm"
              id={id}
              onClick={onChooseFile}
              type="button"
              variant="outline"
            >
              <IconDownload aria-hidden="true" data-icon="inline-start" />
              Select Image
            </Button>
            <div className="truncate text-xs text-muted-foreground">
              {isDefaultSelected ? "Using the bundled Ghostex icon." : selectedIcon?.name ?? selectedId}
            </div>
          </div>
        </div>

        {error ? (
          <div className="flex items-start gap-2 rounded-none border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            <IconAlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
            <span className="min-w-0">{error}</span>
          </div>
        ) : null}
      </div>
    </SettingRow>
  );
}

function SoundField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onPlay,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
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
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className="grid grid-cols-[minmax(0,1fr)_2.5rem] items-center gap-2">
        <SettingsSelect
          onValueChange={(nextValue) => onChange(nextValue as CompletionSoundSetting)}
          value={value}
        >
          <SelectTrigger className="h-10 w-full px-3 text-sm" id={id}>
            <SelectValue />
          </SelectTrigger>
          <SettingsSelectContent className="max-h-72" showScrollButtons={false}>
            <SelectGroup>
              {COMPLETION_SOUND_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SettingsSelectContent>
        </SettingsSelect>
        <DisabledSettingControlTooltip
          disabled={!onPlay}
          reason="Sound preview isn’t available here."
        >
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
        </DisabledSettingControlTooltip>
      </div>
    </SettingRow>
  );
}

function TextField({
  advanced,
  browseLabel,
  description,
  isModified,
  label,
  onBrowse,
  onChange,
  onResetToDefault,
  placeholder,
  value,
}: {
  advanced?: boolean;
  browseLabel?: string;
  description?: string;
  label: string;
  onBrowse?: () => void;
  onChange: (value: string) => void;
  placeholder?: string;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [inputValue, setInputValue] = useState(value);

  useEffect(() => {
    /*
     * CDXC:SettingsTextFields 2026-06-19-16:53:
     * Immediate-save Settings text fields must keep the user's focused edit
     * buffer while native settings hydration echoes persisted values back into
     * the modal host. Sync external values only when the field is not actively
     * editing so Font Family and command fields do not repaint focus back to
     * Settings search after the first typed character.
     */
    if (inputRef.current?.ownerDocument.activeElement === inputRef.current) {
      return;
    }
    setInputValue(value);
  }, [value]);

  const updateInputValue = (nextValue: string) => {
    setInputValue(nextValue);
    onChange(nextValue);
  };

  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      {onBrowse ? (
        <div className="grid grid-cols-[minmax(0,1fr)_2.5rem] items-center gap-2">
          <SettingsInput
            id={id}
            className="h-10 px-3 text-sm"
            onBlur={(event) => updateInputValue(event.currentTarget.value)}
            onChange={(event) => updateInputValue(event.currentTarget.value)}
            placeholder={placeholder}
            ref={inputRef}
            value={inputValue}
          />
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  aria-label={browseLabel ?? `Browse for ${label}`}
                  className="h-10 w-10 rounded-none"
                  onClick={onBrowse}
                  size="icon"
                  type="button"
                  variant="outline"
                >
                  <IconFolderOpen aria-hidden="true" className="size-4" />
                </Button>
              }
            />
            <TooltipContent sideOffset={6}>{browseLabel ?? "Browse…"}</TooltipContent>
          </Tooltip>
        </div>
      ) : (
        <SettingsInput
          id={id}
          className="h-10 px-3 text-sm"
          onBlur={(event) => updateInputValue(event.currentTarget.value)}
          onChange={(event) => updateInputValue(event.currentTarget.value)}
          placeholder={placeholder}
          ref={inputRef}
          value={inputValue}
        />
      )}
    </SettingRow>
  );
}

function DisabledCommandPreviewField({
  advanced,
  description,
  label,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  value: string;
}) {
  const id = useId();
  return (
    <SettingRow advanced={advanced} description={description} htmlFor={id} label={label}>
      <SettingsTextarea
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
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: string) => void;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const colorValue = normalizeColorInputValue(value);
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <div className="grid grid-cols-[2.75rem_minmax(0,1fr)] items-center gap-3">
        <SettingsInput
          aria-label={`${label} picker`}
          className="h-10 cursor-pointer rounded-none p-1"
          onChange={(event) => onChange(event.currentTarget.value)}
          type="color"
          value={colorValue}
        />
        <SettingsInput
          id={id}
          className="h-10 px-3 text-sm"
          onChange={(event) => onChange(event.currentTarget.value)}
          value={value}
        />
      </div>
    </SettingRow>
  );
}

const SIDEBAR_TITLEBAR_TINT_SWATCHES: ReadonlyArray<{ label: string; value: string }> = [
  { label: "White", value: "#ffffff" },
  { label: "Neutral Gray", value: DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR },
  { label: "Black", value: "#000000" },
  { label: "Steel", value: "#4f6672" },
  { label: "Red", value: "#884444" },
  { label: "Orange", value: "#8a5330" },
  { label: "Amber", value: "#8a6a2f" },
  { label: "Olive", value: "#657a3f" },
  { label: "Green", value: "#3f7a5f" },
  { label: "Teal", value: "#2f7d66" },
  { label: "Cyan", value: "#287c7f" },
  { label: "Blue", value: "#336699" },
  { label: "Indigo", value: "#4f5f96" },
  { label: "Violet", value: "#6c4f8f" },
  { label: "Pink", value: "#854f7a" },
  { label: "Rose", value: "#8a4f5f" },
];

function WebColorPickerField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onCommit,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: string) => void;
  onCommit?: (value: string) => void;
  value: string;
} & SettingModificationProps) {
  const id = useId();
  const savedColorValue = normalizeColorInputValue(
    value,
    DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
  );
  const [colorText, setColorText] = useState(savedColorValue);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerValue, setPickerValue] = useState(savedColorValue);
  const colorValue = normalizePickerColorValue(colorText, savedColorValue);

  useEffect(() => {
    setColorText(savedColorValue);
    setPickerValue(savedColorValue);
  }, [savedColorValue]);

  const previewColor = (nextColor: string) => {
    const normalizedColor = normalizePickerColorValue(
      nextColor,
      DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR,
    );
    setColorText(normalizedColor);
    setPickerValue(nextColor);
    onChange(normalizedColor);
    return normalizedColor;
  };

  const commitColor = (nextColor: string) => {
    const normalizedColor = previewColor(nextColor);
    onCommit?.(normalizedColor);
  };
  const commitColorAfterClosingPicker = (nextColor: string) => {
    /*
     * CDXC:SidebarTitlebarColors 2026-06-19-19:51:
     * The custom tint picker is a nested Base UI dialog inside Settings.
     * Close the dialog before the final setting commit so native settings
     * hydration cannot re-render while the picker still owns modal focus.
     */
    flushSync(() => {
      setPickerOpen(false);
    });
    commitColor(nextColor);
  };

  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      {/*
        CDXC:SidebarTitlebarColors 2026-06-15-15:28:
        Background Tint must be a web picker, not input[type=color], so the
        macOS color panel never opens. Use swatches plus a hex field and let
        shared settings normalize the saved color.

        CDXC:SidebarTitlebarColors 2026-06-15-16:04:
        The first tint picker rendered as a full-width framed popover trigger,
        which made the Settings section look like an empty bordered slab. Keep
        the control inline and compact: swatches first, hex value second, no
        extra container chrome.

        CDXC:SidebarTitlebarColors 2026-06-15-16:13:
        Users need both more tint presets and a way to pick any tint color.
        Keep presets inline, and put the full web picker behind a compact
        swatch trigger so the Settings row does not regain the oversized
        framed surface that was removed.

        CDXC:SidebarTitlebarColors 2026-06-15-16:13:
        Picker dragging should preview immediately from local color state while
        the saved tint setting still uses the existing debounced Settings write
        path before native sidebar/titlebar chrome is updated.

        CDXC:SidebarTitlebarColors 2026-06-15-17:34:
        Replace the hand-built hue picker with the same
        react-best-gradient-color-picker control used in Sharptabs. Keep this
        setting solid-color only, and expose it as a simple Pick Color dialog
        rather than showing technical hue/saturation labels in the Settings row.

        CDXC:SidebarTitlebarColors 2026-06-19-13:44:
        Background Tint presets should scan as neutrals first and then a hue-wheel sequence. Keep only fifteen presets by removing near-duplicate Sky and Purple stops, and use compact row spacing so the custom picker and hex field remain on the same row.

        CDXC:SidebarTitlebarColors 2026-06-19-14:20:
        Add Black to the neutral preset group because white, gray, and black are all valid untinted chrome choices. Keep the input two character cells narrower than the first compact layout so the added swatch does not force the hex field onto a second row.

        CDXC:SidebarTitlebarColors 2026-06-19-14:36:
        The hex field should use exactly the remaining first-row width after the swatches and custom picker. Use a zero-basis flexible input instead of a fixed character width so it fills the right-side remainder without wrapping to a second line.
      */}
      <div className="flex flex-wrap items-center gap-1.5">
        {SIDEBAR_TITLEBAR_TINT_SWATCHES.map((swatch) => {
          const isSelected = colorValue === swatch.value;
          return (
            <AppTooltip content={swatch.label} key={swatch.value}>
              <Button
                aria-label={`Use ${swatch.label} tint`}
                aria-pressed={isSelected}
                className={cn(
                  "size-7 min-w-0 shrink-0 border p-0",
                  isSelected ? "border-ring ring-2 ring-ring/45" : "border-border/80",
                )}
                onClick={() => commitColor(swatch.value)}
                style={{ backgroundColor: swatch.value }}
                type="button"
                variant="ghost"
              />
            </AppTooltip>
          );
        })}
        <AppTooltip content="Pick custom tint color">
          <Button
            aria-label={`${label} custom color picker`}
            className="h-8 min-w-0 gap-2 px-2 text-xs"
            onClick={() => {
              setPickerValue(colorValue);
              setPickerOpen(true);
            }}
            type="button"
            variant="outline"
          >
            <span
              aria-hidden="true"
              className="size-4 shrink-0 border border-border"
              style={{ backgroundColor: colorValue }}
            />
            <IconPalette aria-hidden="true" data-icon="inline-end" />
          </Button>
        </AppTooltip>
        <Dialog
          open={pickerOpen}
          onOpenChange={(open) => {
            if (!open) {
              commitColorAfterClosingPicker(colorValue);
              return;
            }
            setPickerOpen(open);
          }}
        >
          <DialogContent className="w-[22rem] gap-4 p-4" showCloseButton={false}>
            <DialogHeader>
              <DialogTitle>Pick Color</DialogTitle>
            </DialogHeader>
            <div className="mx-auto">
              <ColorPicker
                hideAdvancedSliders
                hideColorGuide
                hideColorTypeBtns
                hideEyeDrop
                hideGradientAngle
                hideGradientControls
                hideGradientStop
                hideGradientType
                hideInputType
                hideOpacity
                hidePresets
                idSuffix="sidebar-titlebar-tint"
                onChange={previewColor}
                value={pickerValue}
                width={294}
              />
            </div>
            <DialogFooter>
              <Button
                onClick={() => {
                  commitColorAfterClosingPicker(colorValue);
                }}
                type="button"
              >
                Done
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
        <SettingsInput
          aria-label={`${label} hex color`}
          className="h-8 min-w-0 flex-1 px-2 font-mono text-xs uppercase"
          id={id}
          inputMode="text"
          onBlur={() => commitColor(colorText)}
          onChange={(event) => {
            const nextValue = event.currentTarget.value;
            setColorText(nextValue);
            if (/^#[0-9a-f]{6}$/iu.test(nextValue.trim())) {
              onChange(nextValue.trim().toLowerCase());
            }
          }}
          placeholder={DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR}
          spellCheck={false}
          value={colorText}
        />
      </div>
    </SettingRow>
  );
}

function normalizeColorInputValue(value: string, fallback = "#121212"): string {
  const normalized = value.trim().toLowerCase();
  return /^#[0-9a-f]{6}$/u.test(normalized) ? normalized : fallback;
}

function normalizePickerColorValue(value: string, fallback = "#121212"): string {
  const normalized = value.trim().toLowerCase();
  if (/^#[0-9a-f]{6}$/u.test(normalized)) {
    return normalized;
  }
  const rgbMatch = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})(?:\s*,\s*(?:0|1|0?\.\d+))?\s*\)$/u.exec(normalized);
  if (!rgbMatch) {
    return fallback;
  }
  return rgbToHexColor({
    blue: Number(rgbMatch[3] ?? 0),
    green: Number(rgbMatch[2] ?? 0),
    red: Number(rgbMatch[1] ?? 0),
  });
}

function rgbToHexColor(color: { blue: number; green: number; red: number }): string {
  const toHexComponent = (component: number) =>
    clampNumber(component, 0, 255).toString(16).padStart(2, "0");
  return `#${toHexComponent(color.red)}${toHexComponent(color.green)}${toHexComponent(color.blue)}`;
}

function SidebarPresetField({
  activePresetId,
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
}: {
  activePresetId?: SidebarSettingsPresetId;
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (presetId: SidebarSettingsPresetId) => void;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
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

function SidebarProjectGroupStyleField({
  advanced,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  description?: string;
  label: string;
  onChange: (value: SidebarProjectGroupStyle) => void;
  value: SidebarProjectGroupStyle;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <ToggleGroup
        aria-label={label}
        className="w-full [&>[data-slot=toggle-group-item]]:flex-1"
        onValueChange={(nextValues) => {
          const [nextValue] = nextValues as SidebarProjectGroupStyle[];
          if (nextValue) {
            onChange(nextValue);
          }
        }}
        value={[value]}
        variant="outline"
      >
        {SIDEBAR_PROJECT_GROUP_STYLE_OPTIONS.map((option, index) => (
          <ToggleGroupItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </SettingRow>
  );
}

/*
 * CDXC:SidebarV2 2026-07-29:
 * The sidebar version selector reuses the Preset toggle-group shape so the
 * sidebar version setting reads as one two-option switch, with a New badge on
 * the row label while the Inbox sidebar is still rolling out.
 */
function SidebarVersionField({
  advanced,
  badge,
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  advanced?: boolean;
  badge?: string;
  description?: string;
  label: string;
  onChange: (value: SidebarVersion) => void;
  value: SidebarVersion;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      badge={badge}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
    >
      <ToggleGroup
        aria-label={label}
        className="w-full [&>[data-slot=toggle-group-item]]:flex-1"
        onValueChange={(nextValue) => {
          const [ nextVersion ] = nextValue as SidebarVersion[];
          if (nextVersion) {
            onChange(nextVersion);
          }
        }}
        value={[ value ]}
        variant="outline"
      >
        {SIDEBAR_VERSION_OPTIONS.map((option, index) => (
          <ToggleGroupItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </SettingRow>
  );
}

function PreferredAgentInterfaceField({
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: PreferredAgentInterface) => void;
  value: PreferredAgentInterface;
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
      <ToggleGroup
        aria-label={label}
        className="w-full [&>[data-slot=toggle-group-item]]:flex-1"
        onValueChange={(nextValue) => {
          const [nextInterface] = nextValue as PreferredAgentInterface[];
          if (nextInterface) {
            onChange(nextInterface);
          }
        }}
        value={[value]}
        variant="outline"
      >
        {PREFERRED_AGENT_INTERFACE_OPTIONS.map((option, index) => (
          <ToggleGroupItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </SettingRow>
  );
}

function SessionChatThemeField({
  description,
  isModified,
  label,
  onChange,
  onResetToDefault,
  value,
}: {
  description?: string;
  label: string;
  onChange: (value: SessionChatTheme) => void;
  value: SessionChatTheme;
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
      <ToggleGroup
        aria-label={label}
        className="w-full [&>[data-slot=toggle-group-item]]:flex-1"
        onValueChange={(nextValues) => {
          const [nextValue] = nextValues as SessionChatTheme[];
          if (nextValue) {
            onChange(nextValue);
          }
        }}
        value={[value]}
        variant="outline"
      >
        {SESSION_CHAT_THEME_OPTIONS.map((option, index) => (
          <ToggleGroupItem
            aria-label={option.label}
            id={index === 0 ? id : undefined}
            key={option.value}
            value={option.value}
          >
            {option.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </SettingRow>
  );
}

function ToggleField({
  advanced,
  checked,
  description,
  disabled,
  disabledReason,
  isModified,
  label,
  onChange,
  onResetToDefault,
  subtitle,
}: {
  advanced?: boolean;
  checked: boolean;
  description?: string;
  disabled?: boolean;
  disabledReason?: string;
  label: string;
  onChange: (checked: boolean) => void;
  subtitle?: string;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow
      advanced={advanced}
      description={description}
      htmlFor={id}
      isModified={isModified}
      label={label}
      onResetToDefault={onResetToDefault}
      subtitle={subtitle}
    >
      {disabled && disabledReason ? (
        <SettingSwitch
          checked={checked}
          disabled
          disabledReason={disabledReason}
          id={id}
          onCheckedChange={onChange}
        />
      ) : (
        <Switch checked={checked} disabled={disabled} id={id} onCheckedChange={onChange} />
      )}
    </SettingRow>
  );
}

function DiagnosticLoggingSettingsField({
  isModified,
  onChange,
  onResetToDefault,
  value,
}: {
  isModified?: boolean;
  onChange: (
    scenarioId: DiagnosticLoggingScenarioId,
    duration: DiagnosticLoggingDurationValue,
  ) => void;
  onResetToDefault?: () => void;
  value: DiagnosticLoggingSettings;
}) {
  const idBase = useId();
  return (
    <SettingRow
      description="Routine logs are off by default and write only when Show debug UI controls and their scenario are enabled. Enable only the repro area you need; important warnings, errors, and crashes remain captured."
      htmlFor={`${idBase}-native-terminal-focus`}
      isModified={isModified}
      label="Diagnostic disk logging scenarios"
      onResetToDefault={onResetToDefault}
    >
      <div className="grid gap-4">
        {DIAGNOSTIC_LOGGING_GROUPS.map((group) => {
          const scenarios = DIAGNOSTIC_LOGGING_SCENARIOS.filter(
            (scenario) => scenario.group === group,
          );
          return (
            <div className="grid gap-2" key={group}>
              <div className="text-xs font-medium uppercase tracking-normal text-muted-foreground">
                {group}
              </div>
              <div className="grid gap-2">
                {scenarios.map((scenario) => {
                  const scenarioId = scenario.id as DiagnosticLoggingScenarioId;
                  const duration = getDiagnosticLoggingScenarioDuration(value, scenarioId);
                  const checked = duration !== "off";
                  const switchId = `${idBase}-${scenario.id.replaceAll(".", "-")}`;
                  return (
                    <div
                      className="grid gap-2 border-t border-border/70 pt-2 first:border-t-0 first:pt-0"
                      key={scenario.id}
                    >
                      <div className="flex min-w-0 items-start justify-between gap-3">
                        <div className="min-w-0">
                          <FieldLabel className="text-sm" htmlFor={switchId}>
                            {scenario.label}
                          </FieldLabel>
                          <div className="mt-0.5 break-words text-xs text-muted-foreground">
                            {scenario.logFiles.join(", ")}
                          </div>
                        </div>
                        <Switch
                          checked={checked}
                          id={switchId}
                          onCheckedChange={(nextChecked) =>
                            onChange(
                              scenarioId,
                              nextChecked ? DEFAULT_DIAGNOSTIC_LOGGING_ENABLE_DURATION : "off",
                            )
                          }
                        />
                      </div>
                      {checked ? (
                        <SettingsSelect
                          onValueChange={(nextValue) =>
                            onChange(scenarioId, nextValue as DiagnosticLoggingDurationValue)
                          }
                          value={duration}
                        >
                          <SelectTrigger className="h-8 w-full sm:w-36">
                            <SelectValue />
                          </SelectTrigger>
                          <SettingsSelectContent>
                            {DIAGNOSTIC_LOGGING_DURATION_OPTIONS.filter(
                              (option) => option.value !== "off",
                            ).map((option) => (
                              <SelectItem key={option.value} value={option.value}>
                                {option.label}
                              </SelectItem>
                            ))}
                          </SettingsSelectContent>
                        </SettingsSelect>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </SettingRow>
  );
}

function getDiagnosticLoggingScenarioDuration(
  value: DiagnosticLoggingSettings,
  scenarioId: DiagnosticLoggingScenarioId,
  now: Date = new Date(),
): DiagnosticLoggingDurationValue {
  const scenario = value.scenarios[scenarioId];
  if (!scenario?.enabled) {
    return "off";
  }
  if (!scenario.expiresAt) {
    return "always";
  }
  const expiresAtMs = Date.parse(scenario.expiresAt);
  if (!Number.isFinite(expiresAtMs) || expiresAtMs <= now.getTime()) {
    return "off";
  }
  const remainingMs = expiresAtMs - now.getTime();
  return remainingMs <= 30 * 60 * 1000 ? "15m" : "1h";
}

function getDiagnosticLoggingScenarioStateForDuration(
  duration: DiagnosticLoggingDurationValue,
  now: Date = new Date(),
) {
  /*
   * CDXC:ChromeResponsivenessDiagnostics 2026-06-30-23:52:
   * Some lag/crash diagnostic scenarios are enabled by default so repro logs
   * exist immediately after update. Persist Off as an explicit disabled state
   * instead of deleting the scenario so Settings can override those defaults.
   */
  switch (duration) {
    case "15m":
      return {
        enabled: true,
        expiresAt: new Date(now.getTime() + 15 * 60 * 1000).toISOString(),
      };
    case "1h":
      return {
        enabled: true,
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000).toISOString(),
      };
    case "always":
      return { enabled: true };
    case "off":
      return { enabled: false };
  }
}

function SidebarTagListSettingsField({
  isModified,
  items,
  onChange,
  onResetToDefault,
}: {
  isModified: boolean;
  items: readonly SidebarSessionTagListItem[];
  onChange: (items: readonly SidebarSessionTagListItem[]) => void;
  onResetToDefault: () => void;
}) {
  const normalizedItems = normalizeSidebarSessionTagListItems(items);
  const handleDragEnd = ((event) => {
    if (event.canceled || !isSortableOperation(event.operation)) {
      return;
    }

    const { source, target } = event.operation;
    const sourceData = source ? getSettingsSidebarTagListItemDragData(source) : undefined;
    if (!source || !sourceData) {
      return;
    }

    const targetIndex =
      "index" in source && typeof source.index === "number" ? source.index : target?.index;
    if (targetIndex == null || source.initialIndex === targetIndex) {
      return;
    }

    const itemsById = new Map<string, SidebarSessionTagListItem>(
      normalizedItems.map((item) => [item.id, item]),
    );
    onChange(
      moveId(
        normalizedItems.map((item) => item.id),
        source.initialIndex,
        targetIndex,
      ).flatMap((itemId) => itemsById.get(itemId) ?? []),
    );
  }) satisfies DragDropEventHandlers["onDragEnd"];

  const updateItem = (
    itemId: string,
    patch: Partial<Pick<SidebarSessionTagListItem, "enabled" | "visible">>,
  ) => {
    onChange(
      normalizedItems.map((item) =>
        item.id === itemId
          ? ({
              ...item,
              ...patch,
            } as SidebarSessionTagListItem)
          : item,
      ),
    );
  };
  const updateItemEnabled = (itemId: string, enabled: boolean) => {
    /*
     * CDXC:SessionTagFilters 2026-06-15-22:10:
     * The Settings switch is the primary on/off control for tag filters.
     * Switching a row off should also hide it from the sidebar filter menu,
     * while switching it back on restores visibility so the eye icon and switch
     * cannot drift into a half-on reset state.
     */
    updateItem(itemId, { enabled, visible: enabled });
  };

  const updateItemVisible = (itemId: string, visible: boolean) => {
    /*
     * CDXC:SessionTagFilters 2026-06-15-22:10:
     * The eye button mirrors the same on/off model as the switch for tag rows.
     * Showing a hidden row should re-enable it; hiding a row should disable it
     * so Settings does not present hidden filters as enabled.
     */
    updateItem(itemId, { enabled: visible, visible });
  };

  return (
    <details className="group w-full">
      {/*
       * CDXC:SessionTagFilters 2026-06-13-17:50:
       * The bottom main Settings area starts collapsed and mirrors the
       * configurable-list chrome used by tab context menu item settings:
       * full-width rows, drag handles, enabled switches, and visibility icons.
       * Separators are real rows so users can move or hide them with tags.
       *
       * CDXC:SessionTagFilters 2026-06-15-14:02:
       * The expanded Sidebar Tags list should attach directly to the disclosure
       * header; no vertical gutter belongs between the header and its rows.
       */}
      <summary className="settings-management-row flex cursor-pointer list-none items-center justify-between gap-3 border border-border bg-muted/20 px-3 py-3 marker:hidden [&::-webkit-details-marker]:hidden">
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <IconChevronRight
            aria-hidden="true"
            className="size-4 shrink-0 text-muted-foreground transition-transform duration-150 group-open:rotate-90"
          />
          <FieldContent className="min-w-0 gap-1">
            <FieldLabel className="text-sm">Tag filter list</FieldLabel>
            <FieldDescription className="text-xs text-muted-foreground">
              Reorder, hide, or disable sidebar tag filters and separators.
            </FieldDescription>
          </FieldContent>
        </div>
        <SettingButton
          disabled={!isModified}
          disabledReason="These tag settings already match the defaults."
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onResetToDefault();
          }}
          type="button"
          variant="outline"
        >
          Reset to Default
        </SettingButton>
      </summary>
      <div className="border border-border/80 bg-muted/10 p-3">
        <DragDropProvider onDragEnd={handleDragEnd}>
          <div className="flex w-full flex-col gap-2">
            {normalizedItems.map((item, index) => (
              <SidebarTagListSettingsRow
                index={index}
                item={item}
                key={item.id}
                onEnabledChange={(enabled) => updateItemEnabled(item.id, enabled)}
                onVisibleChange={(visible) => updateItemVisible(item.id, visible)}
              />
            ))}
          </div>
        </DragDropProvider>
      </div>
    </details>
  );
}

function SidebarTagListSettingsRow({
  index,
  item,
  onEnabledChange,
  onVisibleChange,
}: {
  index: number;
  item: SidebarSessionTagListItem;
  onEnabledChange: (enabled: boolean) => void;
  onVisibleChange: (visible: boolean) => void;
}) {
  const sortable = useSortable({
    accept: "settings-sidebar-tag-list-item",
    data: createSettingsSidebarTagListItemDragData(item.id),
    group: "settings-sidebar-tag-list-items",
    id: item.id,
    index,
    type: "settings-sidebar-tag-list-item",
  });
  const { handleRef, isDragging } = sortable;
  const isDimmed = !item.enabled || !item.visible;
  const label = getSidebarSessionTagListItemLabel(item);

  const setRowRef = (element: HTMLDivElement | null) => {
    setSettingsSortableRowElement(sortable, element);
  };

  return (
    <div
      className={cn(
        "settings-management-row flex w-full items-center gap-2 border border-border bg-muted/20 p-2",
        isDimmed && "text-muted-foreground",
      )}
      data-dragging={String(Boolean(isDragging))}
      data-enabled={String(item.enabled)}
      data-visible={String(item.visible)}
      ref={setRowRef}
    >
      <Button
        aria-label={`Reorder ${label}`}
        ref={handleRef}
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <IconGripVertical aria-hidden="true" />
      </Button>
      <div className="flex min-w-0 flex-1 items-center gap-3 px-2 py-2">
        <span
          aria-hidden="true"
          className="settings-management-icon flex size-8 shrink-0 items-center justify-center bg-muted"
        >
          {item.type === "separator" ? (
            <IconMinus className="text-muted-foreground" size={16} stroke={2} />
          ) : (
            <SessionTagIcon
              className="session-tag-colored-icon"
              fillFavorite
              size={15}
              stroke={1.8}
              tag={item.type === "untagged" ? "untagged" : item.tag}
            />
          )}
        </span>
        <span className="min-w-0 flex-1">
          <span
            className={cn(
              "block truncate text-sm font-medium",
              item.type === "separator" && "italic text-muted-foreground",
            )}
          >
            {label}
          </span>
        </span>
      </div>
      <Switch
        aria-label={`${item.enabled ? "Disable" : "Enable"} ${label}`}
        checked={item.enabled}
        onCheckedChange={onEnabledChange}
      />
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              aria-label={`${item.visible ? "Hide" : "Show"} ${label}`}
              className="shrink-0"
              onClick={() => onVisibleChange(!item.visible)}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              {item.visible ? (
                <IconEye aria-hidden="true" size={16} stroke={1.9} />
              ) : (
                <IconEyeOff aria-hidden="true" size={16} stroke={1.9} />
              )}
            </Button>
          }
        />
        <TooltipContent sideOffset={6}>{item.visible ? "Hide" : "Show"}</TooltipContent>
      </Tooltip>
    </div>
  );
}

/**
 * CDXC:Settings 2026-05-06-12:57
 * CDXC:SettingsModifiedState 2026-05-07-18:03
 * Every changed settings control needs a small, low-emphasis asterisk to the
 * left of its label. Position it absolutely so modified-state indication does
 * not reflow setting titles, while the tooltip action still resets only that
 * setting to DEFAULT_ghostex_SETTINGS.
 *
 * CDXC:SettingsDensity 2026-06-15-20:53:
 * Main Settings rows should not show explanatory subtitles inline because the
 * modal needs to stay dense and scannable. Reveal a compact info trigger only
 * while the row is hovered or focused, then show the description in a
 * right-side tooltip capped at 350px.
 *
 * CDXC:SettingsAdvanced 2026-06-16-10:40:
 * Advanced rows should no longer use a text badge. Mark them with a light blue
 * up-arrow affordance beside the label actions, immediately before the info
 * button when one is present, so the label stays compact while hover explains
 * the row as an Advanced Setting.
 *
 * CDXC:SettingsAdvanced 2026-06-16-18:22:
 * The advanced up-arrow is a persistent scan marker, not hover-only chrome, and
 * needs a small gap from the label so advanced rows are visible at rest.
 */
function SettingRow({
  advanced,
  badge,
  children,
  description,
  htmlFor,
  isModified,
  label,
  onResetToDefault,
  subtitle,
}: {
  advanced?: boolean;
  /*
   * CDXC:SidebarV2 2026-07-29:
   * Rows for newly shipped settings may carry a short label badge, matching the
   * Beta badge treatment already used by integration rows.
   */
  badge?: string;
  children: ReactNode;
  description?: string;
  htmlFor: string;
  isModified?: boolean;
  label: string;
  onResetToDefault?: () => void;
  subtitle?: string;
}) {
  return (
    <Field className="settings-row gap-2.5" orientation="vertical">
      <FieldContent>
        <FieldTitle className="settings-row-title text-sm">
          <span className="settings-row-label-line">
            {isModified && onResetToDefault ? (
              <ModifiedSettingResetButton label={label} onResetToDefault={onResetToDefault} />
            ) : null}
            <FieldLabel className="text-sm" htmlFor={htmlFor}>
              {label}
            </FieldLabel>
            {badge ? (
              /*
               * CDXC:SidebarV2 2026-07-29:
               * The badge reads the shadcn theme tokens the rest of this modal
               * uses, so it inverts with the Light themes instead of painting a
               * fixed dark-theme sky tint that washes out on white.
               */
              <span className="inline-flex rounded-none border border-primary/30 bg-primary/10 px-2 py-0.5 text-[11px] font-semibold text-primary">
                {badge}
              </span>
            ) : null}
            {advanced ? <AdvancedSettingTooltip label={label} /> : null}
            {description ? <SettingDescriptionTooltip description={description} label={label} /> : null}
          </span>
        </FieldTitle>
        {subtitle ? (
          <FieldDescription className="settings-row-subtitle">{subtitle}</FieldDescription>
        ) : null}
        {description ? (
          <FieldDescription className="sr-only">{description}</FieldDescription>
        ) : null}
      </FieldContent>
      <div className="min-w-0">{children}</div>
    </Field>
  );
}

function AdvancedSettingTooltip({ label }: { label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            aria-label={`${label} is an advanced setting`}
            className="settings-row-advanced-button"
            type="button"
          >
            <IconArrowBigUp aria-hidden="true" />
          </button>
        }
      />
      <TooltipContent sideOffset={6}>Advanced Setting</TooltipContent>
    </Tooltip>
  );
}

function SettingDescriptionTooltip({ description, label }: { description: string; label: string }) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            aria-label={`${label} setting details`}
            className="settings-row-info-button"
            type="button"
          >
            <IconInfoCircle aria-hidden="true" />
          </button>
        }
      />
      <TooltipContent
        className="settings-row-info-tooltip"
        side="right"
        sideOffset={8}
        style={{ maxWidth: "min(350px, calc(100vw - 32px))" }}
      >
        {description}
      </TooltipContent>
    </Tooltip>
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
