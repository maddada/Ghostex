import { useId, useState, type ReactNode, type RefObject } from 'react';
import { IconChevronRight } from '@tabler/icons-react';
import { cn } from '@/packages/components/utils';
import { Button } from '@/packages/components/ui/button';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { Slider } from '@/packages/components/ui/slider';
import {
  MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
  MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT,
  MAX_WINDOW_GLASS_LIVE_BRIGHTNESS,
  MAX_WINDOW_GLASS_LIVE_SPEED,
  MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT,
  MIN_WINDOW_GLASS_LIVE_BRIGHTNESS,
  MIN_WINDOW_GLASS_LIVE_SPEED,
  MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
  MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT,
  MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT,
  MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT,
  SESSION_CHAT_THEME_OPTIONS,
  WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS,
  getGhosttyThemeSettingOptions,
  type DarkThemePreset,
  type LightThemePreset,
  type WindowGlassMode,
  type WindowGlassSource,
  type WindowGlassImagePlacement,
  type ghostexSettings,
} from '../../../shared/ghostex-settings';
import { type SidebarAppIconStateMessage } from '../../../shared/session-grid-contract';
import { GlassLiveGallery } from '../glass-live-gallery';
import {
  AppIconPickerField,
  SelectField,
  SettingRow,
  SettingsNativeScrollArea,
  SettingsSection,
  SliderNumberField,
  TextField,
  ToggleField,
  WebColorPickerField,
} from '../fields';
import {
  getRememberedThemeMoreOptionsOpen,
  rememberThemeMoreOptionsOpen,
  type ThemeMoreOptionsGroup,
} from '../navigation-memory';
import {
  APPEARANCE_CHOICES,
  COLOURFULNESS_CHOICES,
  ColourfulnessPreview,
  DARK_PRESET_FOR_LIGHT,
  LIGHT_PRESET_FOR_DARK,
  TRANSPARENCY_STRENGTH_MAX,
  TRANSPARENCY_STRENGTH_MIN,
  TRANSPARENCY_STRENGTH_STEP,
  ThemeSchemeTabs,
  ThemeSwatchGrid,
  colourfulnessDisplayStep,
  colourfulnessPatch,
  colourfulnessStepForPoints,
  colourfulnessStepIndex,
  darkThemeSwatches,
  initialThemeScheme,
  isTransparencyEnabled,
  lightThemeSwatches,
  themePresetLabel,
  transparencyStrengthFromSettings,
  transparencyStrengthPatch,
  windowGlassAvailable,
  windowGlassForTransparency,
  windowGlassPicturesAvailable,
  windowGlassRestartNote,
  type ThemeScheme,
} from '../theme-simple-controls';
import { APP_ICON_CONTROLS_VISIBLE } from '../search-catalog';
import { type SettingModificationProps, type SettingsSectionSearchResult } from '../types';

const GHOSTTY_THEME_UNMANAGED_VALUE = '__ghostex_ghostty_theme_unmanaged__';

/** The rows inside each group's own More options, so a search hit on one of them opens that group. */
const THEME_MORE_OPTIONS_KEYS: Readonly<Record<ThemeMoreOptionsGroup, readonly string[]>> = {
  colours: [
    'themeSidebarContrast',
    'themeWorkAreaContrast',
    'customSidebarTitlebarBackgroundDarknessPercent',
    'customSidebarTitlebarBackgroundTintColor',
    'customSidebarTitlebarLightBackgroundLightnessPercent',
    'customSidebarTitlebarLightBackgroundTintColor',
    'showActivePaneOutline',
    'workspaceActivePaneBorderColor',
  ],
  transparency: [
    'windowGlassSource',
    'windowGlassImageDark',
    'windowGlassImageLight',
    'windowGlassVideoDark',
    'windowGlassVideoLight',
    'windowGlassVideoOnlyOnPower',
    'windowGlassLiveStyleDark',
    'windowGlassLiveStyleLight',
    'windowGlassLiveSpeed',
    'windowGlassLiveBrightness',
    'windowGlassImagePlacement',
    'windowGlassSidebarOpacityDark',
    'windowGlassWorkAreaTintDark',
    'windowGlassSidebarOpacityLight',
    'windowGlassWorkAreaTintLight',
  ],
  chatTerminal: ['terminalGhosttyTheme', 'terminalGhosttyLightTheme'],
};

/** Use transparency, in the order the page reads it. */
/**
 * CDXC:Theming 2026-09-26 DECISION:
 * User: "here we should rename Automatic to 'Dark only' and if that option is picked then please hide all the different
 * options related to transparency on [the other] mode (bg image/video/strength of transparency etc..)". Dark only keeps
 * light mode opaque, so while it is picked every light-mode transparency control (the light tints, the light picture
 * and the light video) is hidden; their saved values stay for when Always is picked again.
 */
const USE_TRANSPARENCY_CHOICES: readonly { label: string; value: WindowGlassMode }[] = [
  { label: 'Dark only', value: 'auto' },
  { label: 'Always', value: 'frosted' },
  { label: 'Never', value: 'opaque' },
];

/** What the glass shows, as the four picture cards of step 1. */
const GLASS_SOURCE_CARDS: readonly { art: string; description: string; label: string; value: WindowGlassSource }[] = [
  {
    art: 'is-desktop',
    description: 'Everything behind Ghostex, blurred.',
    label: 'Desktop and windows',
    value: 'desktopAndWindows',
  },
  { art: 'is-wallpaper', description: 'Just your desktop picture.', label: 'Wallpaper', value: 'wallpaper' },
  { art: 'is-picture', description: 'An image you choose.', label: 'Picture', value: 'customImage' },
  { art: 'is-live', description: 'A calm animation, or your own video.', label: 'Live', value: 'live' },
];

type UpdateDraft = <Key extends keyof ghostexSettings>(key: Key, value: ghostexSettings[Key]) => void;

/**
 * CDXC:Theming 2026-09-25 DECISION:
 * User: "I don't like our themes settings in the settings dialog. For example, the video stuff: the position of the image comes before the video selection … even the advanced part shouldn't be just one advanced part. For example, for the glass, I can just do advanced just for the glass, not for the whole everything else", then approved the revamp mockup (docs/2026-09-25/theme-settings-revamp). Theme is three groups, each with its own More options instead of one page-wide Advanced: Colours (Appearance, the theme colour squares with Dark mode / Light mode tabs, Colourfulness; More: the sidebar and work area set apart, a custom colour per appearance, the active pane outline), Transparency (Enable transparency, Strength; More: 1 what shows behind the glass, 2 the pictures or videos, 3 their position, then the tints and Use transparency), and Chat and terminal (the two themes; More: the terminal palettes), plus links to related rows on General. A search hit inside a group's More options opens it. Supersedes the 2026-09-23 single Advanced disclosure and the theme-card layout.
 */
export function ThemeSettingsTab({
  appIconError,
  appIconSectionRef,
  appIconState,
  chooseAppIconFile,
  chooseWindowGlassImageFile,
  chooseWindowGlassVideoFile,
  draft,
  getSettingModificationProps,
  nativeFilePickerAvailable,
  onOpenRelatedSetting,
  rowVisible,
  searchEmptyState,
  searchResults,
  selectAppIcon,
  showAppIcon,
  themingSectionRef,
  updateDraft,
  updateDraftDebounced,
  updateDraftMany,
  windowGlassVideoError,
}: {
  appIconError?: string;
  appIconSectionRef: RefObject<HTMLDivElement | null>;
  appIconState?: SidebarAppIconStateMessage;
  chooseAppIconFile: () => void;
  chooseWindowGlassImageFile: (appearance: 'dark' | 'light') => void;
  chooseWindowGlassVideoFile: (appearance: 'dark' | 'light') => void;
  draft: ghostexSettings;
  getSettingModificationProps: <Key extends keyof ghostexSettings>(key: Key) => Required<SettingModificationProps>;
  nativeFilePickerAvailable: boolean;
  /** Opens General searched for a related setting's title. */
  onOpenRelatedSetting: (query: string) => void;
  /** Whether a row survives the current search; every row shows while nothing is searched. */
  rowVisible: (result: SettingsSectionSearchResult, settingKey: string) => boolean;
  searchEmptyState?: ReactNode;
  searchResults: { appIcon: SettingsSectionSearchResult; theming: SettingsSectionSearchResult };
  selectAppIcon: (sourceId: string) => void;
  showAppIcon: boolean;
  themingSectionRef: RefObject<HTMLDivElement | null>;
  updateDraft: UpdateDraft;
  updateDraftDebounced: UpdateDraft;
  /** Saves several settings in one change, for the friendly controls that drive the deeper ones. */
  updateDraftMany: (patch: Partial<ghostexSettings>) => void;
  /** Why the last picked glass video file was refused, for its appearance's row. */
  windowGlassVideoError?: { appearance: 'dark' | 'light'; message: string };
}) {
  const appearanceId = useId();
  const colourId = useId();
  const colourfulnessId = useId();
  const useTransparencyId = useId();
  const [scheme, setScheme] = useState<ThemeScheme>(() => initialThemeScheme(draft.sidebarTheme));
  const [pickedOnPurpose, setPickedOnPurpose] = useState({ dark: false, light: false });
  const [splitAreas, setSplitAreas] = useState(
    () =>
      draft.themeSidebarContrast !== draft.themeWorkAreaContrast ||
      colourfulnessStepForPoints(draft.themeSidebarContrast) < 0
  );
  const [moreOpen, setMoreOpenState] = useState<Record<ThemeMoreOptionsGroup, boolean>>(() => ({
    colours: getRememberedThemeMoreOptionsOpen('colours'),
    transparency: getRememberedThemeMoreOptionsOpen('transparency'),
    chatTerminal: getRememberedThemeMoreOptionsOpen('chatTerminal'),
  }));
  const setMoreOpen = (group: ThemeMoreOptionsGroup, open: boolean) => {
    rememberThemeMoreOptionsOpen(group, open);
    setMoreOpenState((current) => ({ ...current, [group]: open }));
  };
  const theming = searchResults.theming;
  const isSearching = theming.isSearching;
  const visible = (key: string) => rowVisible(theming, key);
  const appIconVisible =
    APP_ICON_CONTROLS_VISIBLE && showAppIcon && rowVisible(searchResults.appIcon, 'appIconSourceId');
  const moreHasHit = (group: ThemeMoreOptionsGroup) =>
    isSearching && THEME_MORE_OPTIONS_KEYS[group].some((key) => visible(key));
  const moreShown = (group: ThemeMoreOptionsGroup) => moreOpen[group] || moreHasHit(group);
  const glassAvailable = windowGlassAvailable();
  const picturesAvailable = windowGlassPicturesAvailable();
  const glassOn = isTransparencyEnabled(draft.windowGlass);
  // Dark only never shows glass in light mode, so its light-mode controls are hidden.
  const darkOnlyGlass = draft.windowGlass === 'auto';
  const glassAppearances: readonly ('dark' | 'light')[] = darkOnlyGlass ? ['dark'] : ['dark', 'light'];

  const coloursVisible =
    ['sidebarTheme', 'darkThemePreset', 'lightThemePreset'].some(visible) ||
    visible('themeSidebarContrast') ||
    moreHasHit('colours');
  const transparencyVisible = glassAvailable && (visible('windowGlass') || moreHasHit('transparency'));
  const chatTerminalVisible =
    visible('sessionChatTheme') || visible('terminalColorScheme') || moreHasHit('chatTerminal');
  const anythingVisible =
    coloursVisible || transparencyVisible || chatTerminalVisible || appIconVisible || !isSearching;

  const strength = transparencyStrengthFromSettings(draft);
  const colourfulnessIndex = colourfulnessStepIndex(draft);
  const applyPatch = updateDraftMany;

  const darkPaired =
    draft.darkThemePreset !== 'custom' && draft.lightThemePreset === LIGHT_PRESET_FOR_DARK[draft.darkThemePreset];
  const selectDarkPreset = (preset: DarkThemePreset) => {
    const patch: Partial<ghostexSettings> = { darkThemePreset: preset };
    if (!pickedOnPurpose.light && darkPaired && preset !== 'custom') {
      patch.lightThemePreset = LIGHT_PRESET_FOR_DARK[preset];
    }
    setPickedOnPurpose((picked) => ({ ...picked, dark: true }));
    applyPatch(patch);
  };
  const lightPaired =
    draft.lightThemePreset !== 'custom' && draft.darkThemePreset === DARK_PRESET_FOR_LIGHT[draft.lightThemePreset];
  const selectLightPreset = (preset: LightThemePreset) => {
    const patch: Partial<ghostexSettings> = { lightThemePreset: preset };
    if (!pickedOnPurpose.dark && lightPaired && preset !== 'custom') {
      patch.darkThemePreset = DARK_PRESET_FOR_LIGHT[preset];
    }
    setPickedOnPurpose((picked) => ({ ...picked, light: true }));
    applyPatch(patch);
  };
  const selectedPreset = scheme === 'dark' ? draft.darkThemePreset : draft.lightThemePreset;
  const otherScheme: ThemeScheme = scheme === 'dark' ? 'light' : 'dark';
  const otherPreset = scheme === 'dark' ? draft.lightThemePreset : draft.darkThemePreset;
  const followsNote =
    (scheme === 'dark' ? darkPaired && !pickedOnPurpose.light : lightPaired && !pickedOnPurpose.dark) &&
    otherPreset !== 'custom'
      ? `${otherScheme === 'light' ? 'Light' : 'Dark'} mode follows with ${themePresetLabel(otherScheme, otherPreset)} until you pick one there.`
      : `${otherScheme === 'light' ? 'Light' : 'Dark'} mode keeps its own colour (${otherPreset === 'custom' ? 'Custom' : themePresetLabel(otherScheme, otherPreset)}).`;

  const moreOptionsButton = (group: ThemeMoreOptionsGroup, label: string, summary: string) =>
    isSearching ? null : (
      <button
        aria-expanded={moreShown(group)}
        className={cn('theme-more-options-btn', `theme-more-${group}-options-btn`)}
        onClick={() => setMoreOpen(group, !moreOpen[group])}
        type='button'
      >
        <IconChevronRight
          aria-hidden='true'
          className={cn('theme-more-options-chevron', moreShown(group) && 'is-open')}
        />
        <span className='theme-more-options-label'>{label}</span>
        <span className='theme-more-options-summary'>{summary}</span>
      </button>
    );

  const usesPicture = draft.windowGlassSource !== 'desktopAndWindows';
  const liveSlots = darkOnlyGlass
    ? [draft.windowGlassLiveStyleDark]
    : [draft.windowGlassLiveStyleDark, draft.windowGlassLiveStyleLight];
  const liveUsesAnimation = liveSlots.some((style) => style !== 'video');
  const liveUsesVideo = liveSlots.includes('video');
  // A Live animation is drawn over the window itself, so only a video has a position to pick.
  const usesPlacement = usesPicture && (draft.windowGlassSource !== 'live' || liveUsesVideo);

  return (
    <SettingsNativeScrollArea className='settings-main-scroll h-full min-h-0'>
      <div className='settings-page-width flex flex-col gap-6 px-5 pb-5'>
        {!anythingVisible ? searchEmptyState : null}
        {coloursVisible ? (
          <SettingsSection
            description='Pick a colour and how see-through Ghostex is. Each group has its own More options.'
            sectionRef={themingSectionRef}
            title='Colours'
          >
            {visible('sidebarTheme') ? (
              <SettingRow
                description='System follows your computer’s light or dark mode.'
                htmlFor={appearanceId}
                label='Appearance'
                {...getSettingModificationProps('sidebarTheme')}
                advanced={false}
              >
                <SegmentedControl
                  aria-label='Appearance'
                  onValueChange={(value) => updateDraft('sidebarTheme', value as ghostexSettings['sidebarTheme'])}
                  value={draft.sidebarTheme}
                >
                  {APPEARANCE_CHOICES.map((choice, index) => (
                    <SegmentedControlItem
                      id={index === 0 ? appearanceId : undefined}
                      key={choice.value}
                      value={choice.value}
                    >
                      {choice.label}
                    </SegmentedControlItem>
                  ))}
                </SegmentedControl>
              </SettingRow>
            ) : null}
            {visible('darkThemePreset') || visible('lightThemePreset') ? (
              <SettingRow
                description='The colour of the sidebar and window in each mode. Picking one gives the other mode the same colour until you pick one there.'
                htmlFor={colourId}
                label='Theme colour'
                labelAddon={
                  <span className='theme-colour-readout'>
                    {selectedPreset === 'custom' ? 'Custom' : themePresetLabel(scheme, selectedPreset)}
                  </span>
                }
                wide
                {...getSettingModificationProps(scheme === 'dark' ? 'darkThemePreset' : 'lightThemePreset')}
                advanced={false}
              >
                <div className='theme-colour-picker'>
                  <ThemeSchemeTabs onChange={setScheme} value={scheme} />
                  {scheme === 'dark' ? (
                    <ThemeSwatchGrid
                      id={colourId}
                      label='Dark mode colour'
                      onSelect={selectDarkPreset}
                      swatches={darkThemeSwatches(draft)}
                      value={draft.darkThemePreset}
                    />
                  ) : (
                    <ThemeSwatchGrid
                      id={colourId}
                      label='Light mode colour'
                      onSelect={selectLightPreset}
                      swatches={lightThemeSwatches(draft)}
                      value={draft.lightThemePreset}
                    />
                  )}
                  <p className='theme-colour-pairing-note'>{followsNote}</p>
                </div>
              </SettingRow>
            ) : null}
            {visible('themeSidebarContrast') || visible('themeWorkAreaContrast') ? (
              <SettingRow
                description='How much of the colour shows in the sidebar and work area. Vivid is lighter and more colourful, Subtle is deeper and nearly neutral.'
                htmlFor={colourfulnessId}
                label='Colourfulness'
                {...getSettingModificationProps('themeSidebarContrast')}
                advanced={false}
              >
                <div className='theme-colourfulness-control'>
                  {colourfulnessIndex < 0 ? (
                    <span className='theme-colourfulness-split'>Set per area</span>
                  ) : (
                    <ColourfulnessSlider
                      id={colourfulnessId}
                      label='Colourfulness'
                      onChange={(step) => applyPatch(colourfulnessPatch(step))}
                      step={colourfulnessIndex}
                    />
                  )}
                  <ColourfulnessPreview scheme={scheme} settings={draft} />
                </div>
              </SettingRow>
            ) : null}
            {moreOptionsButton(
              'colours',
              'More colour options',
              'separate sidebar and work area, custom colour, active pane outline'
            )}
            {moreShown('colours') ? (
              <>
                {visible('themeSidebarContrast') || visible('themeWorkAreaContrast') ? (
                  <ToggleField
                    checked={splitAreas}
                    description='Off: one Colourfulness for both.'
                    label='Set the sidebar and work area separately'
                    {...getSettingModificationProps('themeWorkAreaContrast')}
                    advanced={false}
                    onChange={(checked) => {
                      setSplitAreas(checked);
                      if (!checked) {
                        applyPatch(colourfulnessPatch(colourfulnessDisplayStep(draft.themeSidebarContrast)));
                      }
                    }}
                  />
                ) : null}
                {splitAreas && visible('themeSidebarContrast') ? (
                  <StepSliderRow
                    label='Sidebar colourfulness'
                    onChange={(step) => updateDraft('themeSidebarContrast', COLOURFULNESS_CHOICES[step]!.points)}
                    step={colourfulnessDisplayStep(draft.themeSidebarContrast)}
                    {...getSettingModificationProps('themeSidebarContrast')}
                  />
                ) : null}
                {splitAreas && visible('themeWorkAreaContrast') ? (
                  <StepSliderRow
                    label='Work area colourfulness'
                    onChange={(step) => updateDraft('themeWorkAreaContrast', COLOURFULNESS_CHOICES[step]!.points)}
                    step={colourfulnessDisplayStep(draft.themeWorkAreaContrast)}
                    {...getSettingModificationProps('themeWorkAreaContrast')}
                  />
                ) : null}
                {visible('customSidebarTitlebarBackgroundTintColor') ||
                visible('customSidebarTitlebarBackgroundDarknessPercent') ? (
                  <ToggleField
                    checked={draft.darkThemePreset === 'custom'}
                    description='Pick any tint instead of a colour square for dark mode.'
                    label='Custom colour in dark mode'
                    {...getSettingModificationProps('darkThemePreset')}
                    advanced={false}
                    onChange={(checked) =>
                      updateDraft(
                        'darkThemePreset',
                        checked
                          ? 'custom'
                          : draft.lightThemePreset !== 'custom'
                            ? DARK_PRESET_FOR_LIGHT[draft.lightThemePreset]
                            : 'gray'
                      )
                    }
                  />
                ) : null}
                {/*
                  CDXC:Theming 2026-06-15-13:45:
                  Replace the freeform background color picker with a constrained contrast slider. The slider outputs
                  calibrated dark backgrounds so sidebar row states remain predictable.

                  CDXC:Theming 2026-06-15-15:28:
                  Add Background Tint as a web-only color picker. Do not use input[type=color], because macOS replaces
                  that with a native color panel instead of the in-app picker requested here.
                */}
                {draft.darkThemePreset === 'custom' && visible('customSidebarTitlebarBackgroundTintColor') ? (
                  <WebColorPickerField
                    dependent
                    description='Applies a subtle hue to the dark sidebar and window chrome background.'
                    label='Dark mode tint'
                    {...getSettingModificationProps('customSidebarTitlebarBackgroundTintColor')}
                    onChange={(value) => updateDraftDebounced('customSidebarTitlebarBackgroundTintColor', value)}
                    onCommit={(value) => updateDraft('customSidebarTitlebarBackgroundTintColor', value)}
                    value={draft.customSidebarTitlebarBackgroundTintColor}
                  />
                ) : null}
                {draft.darkThemePreset === 'custom' && visible('customSidebarTitlebarBackgroundDarknessPercent') ? (
                  <SliderNumberField
                    dependent
                    description='85 is softer gray; 100 is black. Text and icons adjust automatically.'
                    label='Dark mode depth'
                    {...getSettingModificationProps('customSidebarTitlebarBackgroundDarknessPercent')}
                    max={MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT}
                    min={MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT}
                    onCommit={(value) => updateDraft('customSidebarTitlebarBackgroundDarknessPercent', value)}
                    onChange={(value) => updateDraftDebounced('customSidebarTitlebarBackgroundDarknessPercent', value)}
                    step={1}
                    value={draft.customSidebarTitlebarBackgroundDarknessPercent}
                  />
                ) : null}
                {visible('customSidebarTitlebarLightBackgroundTintColor') ||
                visible('customSidebarTitlebarLightBackgroundLightnessPercent') ? (
                  <ToggleField
                    checked={draft.lightThemePreset === 'custom'}
                    description='Pick any tint instead of a colour square for light mode.'
                    label='Custom colour in light mode'
                    {...getSettingModificationProps('lightThemePreset')}
                    advanced={false}
                    onChange={(checked) =>
                      updateDraft(
                        'lightThemePreset',
                        checked
                          ? 'custom'
                          : draft.darkThemePreset !== 'custom'
                            ? LIGHT_PRESET_FOR_DARK[draft.darkThemePreset]
                            : 'gray'
                      )
                    }
                  />
                ) : null}
                {draft.lightThemePreset === 'custom' && visible('customSidebarTitlebarLightBackgroundTintColor') ? (
                  <WebColorPickerField
                    dependent
                    description='Applies a subtle hue to the light sidebar and window chrome background.'
                    label='Light mode tint'
                    {...getSettingModificationProps('customSidebarTitlebarLightBackgroundTintColor')}
                    onChange={(value) => updateDraftDebounced('customSidebarTitlebarLightBackgroundTintColor', value)}
                    onCommit={(value) => updateDraft('customSidebarTitlebarLightBackgroundTintColor', value)}
                    value={draft.customSidebarTitlebarLightBackgroundTintColor}
                  />
                ) : null}
                {draft.lightThemePreset === 'custom' &&
                visible('customSidebarTitlebarLightBackgroundLightnessPercent') ? (
                  <SliderNumberField
                    dependent
                    description='60 is a deeper gray; 100 is white. Text and icons adjust automatically.'
                    label='Light mode depth'
                    {...getSettingModificationProps('customSidebarTitlebarLightBackgroundLightnessPercent')}
                    max={MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT}
                    min={MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT}
                    onCommit={(value) => updateDraft('customSidebarTitlebarLightBackgroundLightnessPercent', value)}
                    onChange={(value) =>
                      updateDraftDebounced('customSidebarTitlebarLightBackgroundLightnessPercent', value)
                    }
                    step={1}
                    value={draft.customSidebarTitlebarLightBackgroundLightnessPercent}
                  />
                ) : null}
                {visible('showActivePaneOutline') ? (
                  <ToggleField
                    checked={draft.showActivePaneOutline}
                    description='Show an outline around the currently focused pane.'
                    label='Show active pane outline'
                    {...getSettingModificationProps('showActivePaneOutline')}
                    advanced={false}
                    onChange={(checked) => updateDraft('showActivePaneOutline', checked)}
                  />
                ) : null}
                {draft.showActivePaneOutline && visible('workspaceActivePaneBorderColor') ? (
                  <WebColorPickerField
                    dependent
                    description='Color of the outline around the currently focused pane.'
                    label='Active pane outline colour'
                    {...getSettingModificationProps('workspaceActivePaneBorderColor')}
                    advanced={false}
                    onChange={(value) => updateDraftDebounced('workspaceActivePaneBorderColor', value)}
                    onCommit={(value) => updateDraft('workspaceActivePaneBorderColor', value)}
                    value={draft.workspaceActivePaneBorderColor}
                  />
                ) : null}
              </>
            ) : null}
          </SettingsSection>
        ) : null}

        {transparencyVisible ? (
          <SettingsSection title='Transparency'>
            {visible('windowGlass') ? (
              <ToggleField
                checked={glassOn}
                description={`Let your desktop show softly through the window.${windowGlassRestartNote()}`}
                label='Enable transparency'
                {...getSettingModificationProps('windowGlass')}
                advanced={false}
                onChange={(checked) =>
                  updateDraft('windowGlass', windowGlassForTransparency(draft.windowGlass, checked))
                }
              />
            ) : null}
            {glassOn && visible('windowGlass') ? (
              <SliderNumberField
                description={
                  strength.exact === undefined
                    ? 'Tuned by hand under More transparency options; moving this resets all four tints.'
                    : 'Higher shows more of what is behind the window.'
                }
                label='Strength'
                {...getSettingModificationProps('windowGlassSidebarOpacityDark')}
                advanced={false}
                max={TRANSPARENCY_STRENGTH_MAX}
                min={TRANSPARENCY_STRENGTH_MIN}
                onChange={(value) => applyPatch(transparencyStrengthPatch(value))}
                onCommit={(value) => applyPatch(transparencyStrengthPatch(value))}
                step={TRANSPARENCY_STRENGTH_STEP}
                value={strength.nearest}
              />
            ) : null}
            {moreOptionsButton(
              'transparency',
              'More transparency options',
              'what shows behind, sidebar and work area tints, when to use it'
            )}
            {moreShown('transparency') ? (
              <>
                {glassOn && picturesAvailable && visible('windowGlassSource') ? (
                  <ThemeSubhead step={1}>What shows behind the glass</ThemeSubhead>
                ) : null}
                {glassOn && picturesAvailable && visible('windowGlassSource') ? (
                  <div className='theme-stacked-row'>
                    <div
                      className='theme-glass-source-cards'
                      role='radiogroup'
                      aria-label='What shows behind the glass'
                    >
                      {GLASS_SOURCE_CARDS.map((card) => (
                        <button
                          aria-checked={draft.windowGlassSource === card.value}
                          className={cn(
                            'theme-glass-source-card',
                            `theme-glass-source-${card.value}-card`,
                            draft.windowGlassSource === card.value && 'is-selected'
                          )}
                          key={card.value}
                          onClick={() => updateDraft('windowGlassSource', card.value)}
                          role='radio'
                          type='button'
                        >
                          <span aria-hidden='true' className={cn('theme-glass-source-art', card.art)} />
                          <span className='theme-glass-source-name'>{card.label}</span>
                          <span className='theme-glass-source-description'>{card.description}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                ) : null}
                {glassOn && picturesAvailable && usesPicture && draft.windowGlassSource !== 'wallpaper' ? (
                  <ThemeSubhead step={2}>
                    {draft.windowGlassSource === 'live' ? 'Choose the style' : 'Choose the pictures'}
                  </ThemeSubhead>
                ) : null}
                {glassOn &&
                picturesAvailable &&
                draft.windowGlassSource === 'customImage' &&
                nativeFilePickerAvailable &&
                (visible('windowGlassImageDark') || visible('windowGlassImageLight')) ? (
                  <div className='theme-stacked-row'>
                    <div className='theme-glass-picture-pair'>
                      {glassAppearances.map((appearance) => {
                        const key = appearance === 'dark' ? 'windowGlassImageDark' : 'windowGlassImageLight';
                        const path = draft[key];
                        return (
                          <div
                            className={cn('theme-glass-picture-slot', `theme-glass-picture-${appearance}-slot`)}
                            key={key}
                          >
                            <span
                              aria-hidden='true'
                              className={cn('theme-glass-picture-thumb', `is-${appearance}`, !path && 'is-empty')}
                            >
                              {path ? null : 'No picture yet'}
                            </span>
                            <span className='theme-glass-picture-mode'>
                              {appearance === 'dark' ? 'Dark mode' : 'Light mode'}
                            </span>
                            <span className='theme-glass-picture-file' title={path || undefined}>
                              {path ? (path.split('/').pop() ?? path) : 'Shows the live blur'}
                            </span>
                            <span className='theme-glass-picture-actions'>
                              <Button
                                onClick={() => chooseWindowGlassImageFile(appearance)}
                                size='sm'
                                type='button'
                                variant='secondary'
                              >
                                {path ? 'Change…' : 'Choose…'}
                              </Button>
                              {path ? (
                                <Button onClick={() => updateDraft(key, '')} size='sm' type='button' variant='ghost'>
                                  Clear
                                </Button>
                              ) : null}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                ) : null}
                {glassOn && picturesAvailable && draft.windowGlassSource === 'customImage' && !nativeFilePickerAvailable
                  ? glassAppearances.map((appearance) => {
                      const key = appearance === 'dark' ? 'windowGlassImageDark' : 'windowGlassImageLight';
                      return visible(key) ? (
                        <TextField
                          dependent
                          description={`The picture the glass blurs in ${appearance} mode.`}
                          key={key}
                          label={appearance === 'dark' ? 'Picture for dark mode' : 'Picture for light mode'}
                          {...getSettingModificationProps(key)}
                          onChange={(value) => updateDraft(key, value.trim())}
                          placeholder={`/Users/you/Pictures/${appearance}.jpg`}
                          value={draft[key]}
                        />
                      ) : null;
                    })
                  : null}
                {glassOn &&
                picturesAvailable &&
                draft.windowGlassSource === 'live' &&
                (visible('windowGlassLiveStyleDark') || visible('windowGlassLiveStyleLight')) ? (
                  <div className='theme-stacked-row'>
                    <GlassLiveGallery
                      canChooseVideo={nativeFilePickerAvailable}
                      darkOnly={darkOnlyGlass}
                      darkStyle={draft.windowGlassLiveStyleDark}
                      darkVideo={draft.windowGlassVideoDark}
                      lightStyle={draft.windowGlassLiveStyleLight}
                      lightVideo={draft.windowGlassVideoLight}
                      onChooseVideo={chooseWindowGlassVideoFile}
                      onClearVideo={(appearance) =>
                        updateDraft(appearance === 'dark' ? 'windowGlassVideoDark' : 'windowGlassVideoLight', '')
                      }
                      onPick={(appearance, style) =>
                        updateDraft(
                          appearance === 'dark' ? 'windowGlassLiveStyleDark' : 'windowGlassLiveStyleLight',
                          style
                        )
                      }
                      videoError={windowGlassVideoError}
                    />
                  </div>
                ) : null}
                {glassOn &&
                picturesAvailable &&
                draft.windowGlassSource === 'live' &&
                liveUsesAnimation &&
                visible('windowGlassLiveSpeed') ? (
                  <SliderNumberField
                    dependent
                    description='How fast the Live animation moves. 1 is its own calm pace. Your own video plays as it is.'
                    label='Speed'
                    {...getSettingModificationProps('windowGlassLiveSpeed')}
                    max={MAX_WINDOW_GLASS_LIVE_SPEED}
                    min={MIN_WINDOW_GLASS_LIVE_SPEED}
                    onCommit={(value) => updateDraft('windowGlassLiveSpeed', value)}
                    onChange={(value) => updateDraftDebounced('windowGlassLiveSpeed', value)}
                    step={0.25}
                    value={draft.windowGlassLiveSpeed}
                  />
                ) : null}
                {glassOn &&
                picturesAvailable &&
                draft.windowGlassSource === 'live' &&
                liveUsesAnimation &&
                visible('windowGlassLiveBrightness') ? (
                  <SliderNumberField
                    dependent
                    description='How bright the Live animation glows behind the glass. Lower keeps it a subtle glow.'
                    label='Brightness'
                    {...getSettingModificationProps('windowGlassLiveBrightness')}
                    max={MAX_WINDOW_GLASS_LIVE_BRIGHTNESS}
                    min={MIN_WINDOW_GLASS_LIVE_BRIGHTNESS}
                    onCommit={(value) => updateDraft('windowGlassLiveBrightness', value)}
                    onChange={(value) => updateDraftDebounced('windowGlassLiveBrightness', value)}
                    step={1}
                    value={draft.windowGlassLiveBrightness}
                  />
                ) : null}
                {glassOn &&
                picturesAvailable &&
                draft.windowGlassSource === 'live' &&
                visible('windowGlassVideoOnlyOnPower') ? (
                  <ToggleField
                    checked={draft.windowGlassVideoOnlyOnPower}
                    dependent
                    description='Pause the Live animation or video while your computer runs on battery. It always pauses while Ghostex is in the background or hidden.'
                    label='Play only when plugged in'
                    {...getSettingModificationProps('windowGlassVideoOnlyOnPower')}
                    onChange={(checked) => updateDraft('windowGlassVideoOnlyOnPower', checked)}
                  />
                ) : null}
                {glassOn && picturesAvailable && usesPlacement && visible('windowGlassImagePlacement') ? (
                  <ThemeSubhead step={draft.windowGlassSource === 'wallpaper' ? 2 : 3}>Position</ThemeSubhead>
                ) : null}
                {glassOn && picturesAvailable && usesPlacement && visible('windowGlassImagePlacement') ? (
                  <SelectField
                    description='Stays with the desktop can trail the window while you drag it.'
                    label='Picture position'
                    {...getSettingModificationProps('windowGlassImagePlacement')}
                    onChange={(value) => updateDraft('windowGlassImagePlacement', value as WindowGlassImagePlacement)}
                    options={WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS}
                    value={draft.windowGlassImagePlacement}
                  />
                ) : null}
                {glassOn ? <ThemeSubhead>Fine-tune the tints</ThemeSubhead> : null}
                {glassOn && visible('windowGlassSidebarOpacityDark') ? (
                  <SliderNumberField
                    description='How much of the desktop the sidebar hides in dark mode. Lower shows more of your desktop through it.'
                    label='Sidebar tint in dark mode'
                    {...getSettingModificationProps('windowGlassSidebarOpacityDark')}
                    max={MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT}
                    min={MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT}
                    onCommit={(value) => updateDraft('windowGlassSidebarOpacityDark', value)}
                    onChange={(value) => updateDraftDebounced('windowGlassSidebarOpacityDark', value)}
                    step={1}
                    value={draft.windowGlassSidebarOpacityDark}
                  />
                ) : null}
                {glassOn && visible('windowGlassWorkAreaTintDark') ? (
                  <SliderNumberField
                    description='How much of the desktop the work area hides in dark mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it.'
                    label='Work area tint in dark mode'
                    {...getSettingModificationProps('windowGlassWorkAreaTintDark')}
                    max={MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT}
                    min={MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT}
                    onCommit={(value) => updateDraft('windowGlassWorkAreaTintDark', value)}
                    onChange={(value) => updateDraftDebounced('windowGlassWorkAreaTintDark', value)}
                    step={1}
                    value={draft.windowGlassWorkAreaTintDark}
                  />
                ) : null}
                {glassOn && !darkOnlyGlass && visible('windowGlassSidebarOpacityLight') ? (
                  <SliderNumberField
                    description='How much of the desktop the sidebar hides in light mode. Lower shows more of your desktop through it.'
                    label='Sidebar tint in light mode'
                    {...getSettingModificationProps('windowGlassSidebarOpacityLight')}
                    max={MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT}
                    min={MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT}
                    onCommit={(value) => updateDraft('windowGlassSidebarOpacityLight', value)}
                    onChange={(value) => updateDraftDebounced('windowGlassSidebarOpacityLight', value)}
                    step={1}
                    value={draft.windowGlassSidebarOpacityLight}
                  />
                ) : null}
                {glassOn && !darkOnlyGlass && visible('windowGlassWorkAreaTintLight') ? (
                  <SliderNumberField
                    description='How much of the desktop the work area hides in light mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it.'
                    label='Work area tint in light mode'
                    {...getSettingModificationProps('windowGlassWorkAreaTintLight')}
                    max={MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT}
                    min={MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT}
                    onCommit={(value) => updateDraft('windowGlassWorkAreaTintLight', value)}
                    onChange={(value) => updateDraftDebounced('windowGlassWorkAreaTintLight', value)}
                    step={1}
                    value={draft.windowGlassWorkAreaTintLight}
                  />
                ) : null}
                {visible('windowGlass') ? (
                  <SettingRow
                    description={`Dark only keeps light mode opaque.${windowGlassRestartNote()}`}
                    htmlFor={useTransparencyId}
                    label='Use transparency'
                    {...getSettingModificationProps('windowGlass')}
                  >
                    <SegmentedControl
                      aria-label='Use transparency'
                      onValueChange={(value) => updateDraft('windowGlass', value as WindowGlassMode)}
                      value={draft.windowGlass}
                    >
                      {USE_TRANSPARENCY_CHOICES.map((choice, index) => (
                        <SegmentedControlItem
                          id={index === 0 ? useTransparencyId : undefined}
                          key={choice.value}
                          value={choice.value}
                        >
                          {choice.label}
                        </SegmentedControlItem>
                      ))}
                    </SegmentedControl>
                  </SettingRow>
                ) : null}
              </>
            ) : null}
          </SettingsSection>
        ) : null}

        {chatTerminalVisible ? (
          <SettingsSection title='Chat and terminal'>
            {visible('sessionChatTheme') ? (
              <SelectField
                description='Follow the app theme, or choose a separate appearance for chat.'
                label='Chat theme'
                {...getSettingModificationProps('sessionChatTheme')}
                onChange={(value) => updateDraft('sessionChatTheme', value as ghostexSettings['sessionChatTheme'])}
                options={SESSION_CHAT_THEME_OPTIONS}
                value={draft.sessionChatTheme}
              />
            ) : null}
            {visible('terminalColorScheme') ? (
              <SelectField
                description='Follow the app theme, or choose a separate appearance for terminals.'
                label='Terminal theme'
                {...getSettingModificationProps('terminalColorScheme')}
                onChange={(value) =>
                  updateDraft('terminalColorScheme', value as ghostexSettings['terminalColorScheme'])
                }
                options={SESSION_CHAT_THEME_OPTIONS}
                value={draft.terminalColorScheme}
              />
            ) : null}
            {moreOptionsButton('chatTerminal', 'More chat and terminal options', 'terminal palettes')}
            {moreShown('chatTerminal') ? (
              <>
                {visible('terminalGhosttyTheme') ? (
                  <SelectField
                    contentClassName='max-h-80'
                    description='Uses your configured Ghostty dark theme, or GitHub Dark when no theme is configured.'
                    label='Terminal palette in dark mode'
                    {...getSettingModificationProps('terminalGhosttyTheme')}
                    onChange={(value) =>
                      updateDraft('terminalGhosttyTheme', value === GHOSTTY_THEME_UNMANAGED_VALUE ? '' : value)
                    }
                    options={getGhosttyThemeSettingOptions(draft.terminalGhosttyTheme)}
                    showScrollButtons={false}
                    value={draft.terminalGhosttyTheme || GHOSTTY_THEME_UNMANAGED_VALUE}
                  />
                ) : null}
                {visible('terminalGhosttyLightTheme') ? (
                  <SelectField
                    contentClassName='max-h-80'
                    description='Uses your configured Ghostty light theme, or GitHub Light when no theme is configured.'
                    label='Terminal palette in light mode'
                    {...getSettingModificationProps('terminalGhosttyLightTheme')}
                    onChange={(value) => updateDraft('terminalGhosttyLightTheme', value)}
                    options={getGhosttyThemeSettingOptions(draft.terminalGhosttyLightTheme).filter(
                      (option) => option.value !== GHOSTTY_THEME_UNMANAGED_VALUE
                    )}
                    showScrollButtons={false}
                    value={draft.terminalGhosttyLightTheme}
                  />
                ) : null}
              </>
            ) : null}
          </SettingsSection>
        ) : null}

        {/*
         * CDXC:Icons 2026-06-28-06:05:
         * The App Icon section is a custom-image control, not a bundled preset picker. Show one preview, one Select Image action, and an inline X on the custom preview to restore the default icon; omit separate reset and folder-reveal actions so the flow stays direct.
         */}
        {appIconVisible ? (
          <SettingsSection
            description='Changes the Dock and app-switcher icon. The app file icon may also change when the operating system allows it.'
            sectionRef={appIconSectionRef}
            title='App Icon'
          >
            <AppIconPickerField
              advanced={false}
              error={appIconError}
              onChooseFile={chooseAppIconFile}
              onSelect={selectAppIcon}
              state={appIconState}
            />
          </SettingsSection>
        ) : null}

        {!isSearching ? (
          <SettingsSection title='Related settings on General'>
            <RelatedSettingLink label='Chat font and size' onOpen={() => onOpenRelatedSetting('Chat font')} />
            <RelatedSettingLink label='Sidebar size' onOpen={() => onOpenRelatedSetting('Sidebar Interface Size')} />
            <RelatedSettingLink
              label='Terminal background colour and image'
              onOpen={() => onOpenRelatedSetting('Terminal background')}
            />
          </SettingsSection>
        ) : null}
      </div>
    </SettingsNativeScrollArea>
  );
}

/** A small heading inside a group's More options, numbered when it is a step of the glass picture flow. */
function ThemeSubhead({ children, step }: { children: ReactNode; step?: number }) {
  return (
    <div className='theme-more-subhead'>
      {step ? <span className='theme-more-step-badge'>{step}</span> : null}
      {children}
    </div>
  );
}

/** The five-step Colourfulness slider with Subtle and Vivid at its ends. */
function ColourfulnessSlider({
  id,
  label,
  onChange,
  step,
}: {
  id?: string;
  label: string;
  onChange: (step: number) => void;
  step: number;
}) {
  return (
    <span className='theme-colourfulness-slider'>
      <span className='theme-colourfulness-end'>Subtle</span>
      <Slider
        aria-label={label}
        className='theme-colourfulness-track'
        id={id}
        max={COLOURFULNESS_CHOICES.length - 1}
        min={0}
        onValueChange={(value) => {
          const next = value[0];
          if (typeof next === 'number' && next !== step) {
            onChange(next);
          }
        }}
        step={1}
        value={[step]}
      />
      <span className='theme-colourfulness-end'>Vivid</span>
      <span className='theme-colourfulness-value'>{COLOURFULNESS_CHOICES[step]?.label}</span>
    </span>
  );
}

/** A dependent row holding one area's Colourfulness slider. */
function StepSliderRow({
  label,
  onChange,
  step,
  ...modification
}: {
  label: string;
  onChange: (step: number) => void;
  step: number;
} & SettingModificationProps) {
  const id = useId();
  return (
    <SettingRow dependent htmlFor={id} label={label} {...modification}>
      <ColourfulnessSlider id={id} label={label} onChange={onChange} step={step} />
    </SettingRow>
  );
}

function RelatedSettingLink({ label, onOpen }: { label: string; onOpen: () => void }) {
  const id = useId();
  return (
    <SettingRow htmlFor={id} label={label}>
      <Button className='h-8 px-3' id={id} onClick={onOpen} type='button' variant='outline'>
        Open
      </Button>
    </SettingRow>
  );
}
