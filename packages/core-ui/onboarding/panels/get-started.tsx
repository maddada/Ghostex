import { useState, type ReactNode } from 'react';
import type { DarkThemePreset, LightThemePreset, PreferredAgentInterface } from '@/packages/shared/ghostex-settings';
import {
  defaultAgentId as resolveDefaultAgentId,
  finishOnboarding,
  installedAgents,
  type PanelProps,
} from '../onboarding-state';
import {
  APPEARANCE_CHOICES,
  COLOURFULNESS_CHOICES,
  DARK_PRESET_FOR_LIGHT,
  LIGHT_PRESET_FOR_DARK,
  TRANSPARENCY_STRENGTH_MAX,
  TRANSPARENCY_STRENGTH_MIN,
  TRANSPARENCY_STRENGTH_STEP,
  ThemeSwatchGrid,
  colourfulnessPatch,
  colourfulnessStepIndex,
  darkThemeSwatches,
  initialThemeScheme,
  isTransparencyEnabled,
  lightThemeSwatches,
  themePresetLabel,
  transparencyStrengthFromSettings,
  transparencyStrengthPatch,
  windowGlassAvailable,
  windowGlassRestartNote,
  windowGlassForTransparency,
} from '../../settings-modal/theme-simple-controls';
import { Cta, Eyebrow, FootActions, Heading, Icon, Spinner, Sub, Toggle } from '../primitives';
import { box } from '../stage';

const SESSION_VIEWS: readonly (readonly [PreferredAgentInterface, string, string])[] = [
  ['chat', 'Chat', 'Cleaner agent conversation'],
  ['terminal', 'Terminal', 'Raw CLI interface'],
];
/** The project card and the look card sit side by side, centred on the full-width stage. */
const CARDS_LEFT = 156;
const CARDS_TOP = 290;
const CARDS_WIDTH = 1360;
/**
 * CDXC:Onboarding 2026-09-15 WHY:
 * The prototype laid the "Start with" tiles out at a fixed pitch of card width over tile count, which squeezed
 * fifteen installed agent CLIs into 36px tiles with every name overlapping. The card is flow layout now: up to
 * this many choices keep the name-and-detail tiles in one row, more become name-only chips that wrap, and the
 * card grows with them.
 */
const MAX_TILE_ROW = 4;

export function GetStartedPanel({ props, flow, setFlow, toast }: PanelProps) {
  const { settings, agents, pickedProjectFolder, hasProjects } = props;
  const defaultAgent = resolveDefaultAgentId(settings, agents);
  const installed = installedAgents(agents);
  const ordered = defaultAgent
    ? [
        installed.find((agent) => agent.agentId === defaultAgent)!,
        ...installed.filter((agent) => agent.agentId !== defaultAgent),
      ]
    : installed;
  const tiles: readonly { id: string; name: string; detail: string }[] = [
    ...ordered.map((agent) => ({
      id: agent.agentId,
      name: agent.name,
      detail: agent.agentId === defaultAgent ? 'Default agent' : 'Switch anytime',
    })),
    { id: 'terminal', name: 'Terminal', detail: 'No agent yet' },
  ];
  const startWith = flow.startWith ?? defaultAgent ?? 'terminal';
  const sessionView = settings?.preferredAgentInterface ?? 'chat';
  const folder = pickedProjectFolder?.trim() ?? '';
  const canOpen = folder.length > 0 || hasProjects === true;
  /** "Open Ghostex" is a host round trip: busy until the project and session exist, error stays on the panel. */
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string>();

  const setSessionView = (view: PreferredAgentInterface) => {
    if (!settings) return;
    props.onChange({ ...settings, preferredAgentInterface: view });
  };
  const updateSettings = (patch: Partial<NonNullable<typeof settings>>) => {
    if (!settings) return;
    props.onChange({ ...settings, ...patch });
  };
  const openGhostex = () => {
    if (folder) {
      if (opening) return;
      setOpening(true);
      setOpenError(undefined);
      props.onFinishFirstLaunch({ agentId: startWith, path: folder }).then(
        () => setFlow({ finishedPath: folder, finished: true }),
        (error: unknown) => {
          setOpening(false);
          setOpenError(error instanceof Error && error.message ? error.message : 'Ghostex could not open the project.');
        }
      );
      return;
    }
    if (hasProjects) finishOnboarding(props, flow);
  };
  const advancedLater = () => {
    props.onOpenSettings?.();
    props.onClose();
  };

  return (
    <>
      <Eyebrow x={486} y={150} w={700}>
        Get started
      </Eyebrow>
      <Heading x={336} y={182} w={1000} size={48} center l1='Open your first project in Ghostex.' />
      <Sub x={336} y={252} w={1000} size={16.5} center>
        One folder, one agent, one default view and a look you like. Everything else can change later.
      </Sub>
      <div className='pcol' style={box(CARDS_LEFT, CARDS_TOP, CARDS_WIDTH)}>
        <div className='pcards'>
          <div className='glass pcard'>
            <div className='label'>Project folder</div>
            <div className='pfield'>
              <Icon n='folder' size={22} className='dimc2' />
              <span className={'path' + (folder ? '' : ' dim')}>{folder || 'Choose a folder to start in'}</span>
              <button type='button' className='choose' onClick={props.onPickProjectFolder}>
                Choose folder
              </button>
            </div>
            <div className='label'>Start with</div>
            {tiles.length <= MAX_TILE_ROW ? (
              <div className='opts' style={{ gridTemplateColumns: `repeat(${tiles.length}, minmax(0, 1fr))` }}>
                {tiles.map((tile) => (
                  <button
                    key={tile.id}
                    type='button'
                    className={'opt' + (startWith === tile.id ? ' sel' : '')}
                    onClick={() => setFlow({ startWith: tile.id })}
                  >
                    <span className='nm'>{tile.name}</span>
                    <span className='ss'>{tile.detail}</span>
                  </button>
                ))}
              </div>
            ) : (
              <div className='chips'>
                {tiles.map((tile) => (
                  <button
                    key={tile.id}
                    type='button'
                    className={'opt chip' + (startWith === tile.id ? ' sel' : '')}
                    title={tile.detail}
                    onClick={() => setFlow({ startWith: tile.id })}
                  >
                    {tile.name}
                  </button>
                ))}
              </div>
            )}
            <div className='label'>Default session view</div>
            <div className='opts' style={{ gridTemplateColumns: `repeat(${SESSION_VIEWS.length}, minmax(0, 1fr))` }}>
              {SESSION_VIEWS.map(([id, name, detail]) => (
                <button
                  key={id}
                  type='button'
                  className={'opt' + (sessionView === id ? ' sel' : '')}
                  onClick={() => setSessionView(id)}
                >
                  <span className='nm'>{name}</span>
                  <span className='ss'>{detail}</span>
                </button>
              ))}
            </div>
          </div>
          {settings ? (
            <LookCard
              settings={settings}
              onOpenThemeSettings={props.onOpenSettings ? () => props.onOpenSettings?.('theme') : undefined}
              onUpdate={updateSettings}
              toast={toast}
            />
          ) : null}
        </div>
        <p className='sub center pnote-flow'>
          {openError ? (
            <span style={{ color: '#ff6b62' }} role='alert'>
              {openError}
            </span>
          ) : opening ? (
            'Adding the project and opening its first session…'
          ) : canOpen ? (
            "That's it. The workspace teaches the deeper features once you are inside."
          ) : (
            'Choose a folder above to open your first project.'
          )}
        </p>
      </div>
      <FootActions panel={5}>
        <button type='button' className='ghost' onClick={advancedLater} disabled={opening}>
          Advanced settings later
        </button>
        <Cta filled onClick={openGhostex} disabled={!canOpen || opening} arrow={!opening}>
          {opening ? (
            <>
              <Spinner /> Opening…
            </>
          ) : (
            'Open Ghostex'
          )}
        </Cta>
      </FootActions>
    </>
  );
}

/**
 * CDXC:Onboarding 2026-09-23 DECISION:
 * User: "please add the transparency setting and theme (just the non advanced stuff) to the onboarding setup's last
 * page". The Get started panel carries the Theme page's simple choices in a card beside the project card: Appearance,
 * the dark and light theme cards and Enable Transparency, from the same shared controls as Settings -> Theme
 * (settings-modal/theme-simple-controls.tsx), applied live. Custom is left out here because tuning it needs the
 * Advanced colour rows; it stays on the Theme page.
 *
 * CDXC:Onboarding 2026-09-23 DECISION:
 * User: "please add a color contrast 5 options slider in the setup and add transparency strength selection (5 options
 * also) / also we should automatically activate night mode for them if they enable transparency (switch it from
 * auto/light to dark and indicate this with a toast". Turning Enable Transparency on here also sets Appearance to Dark
 * (Automatic glass only shows in dark mode) and says so in a toast. Then: "enable transparency toggle is fugly also lets
 * somehow have a switch that lets you see just dark or just light themes no need to show all of them there / and by
 * default we just pick the same color in the other scheme". Transparency is one row (switch, strength slider, value);
 * Theme shows one appearance's cards behind a Dark | Light switch, and picking a look fills in the matching look for
 * the other appearance until that one is picked on purpose.
 *
 * CDXC:Onboarding 2026-09-25 DECISION:
 * User, of the Theme settings revamp: "keep in mind we'll apply this to the last page in the setup modal. But in the
 * setup modal, we'll just have the simple thing, and then we need to tell it that you can go to settings to modify the
 * theme even more." The look card keeps only the simple controls from Settings -> Theme (Appearance, the colour squares
 * behind Dark | Light tabs, Colourfulness, one Transparency row) and ends with "More theme options in Settings ->
 * Theme", which opens Settings on the Theme page. The project and agent choices stay in the card on the left.
 */
function LookCard({
  settings,
  onOpenThemeSettings,
  onUpdate,
  toast,
}: {
  settings: NonNullable<PanelProps['props']['settings']>;
  onOpenThemeSettings?: () => void;
  onUpdate: (patch: Partial<NonNullable<PanelProps['props']['settings']>>) => void;
  toast: PanelProps['toast'];
}) {
  const transparencyOn = isTransparencyEnabled(settings.windowGlass);
  const colourfulnessIndex = colourfulnessStepIndex(settings);
  const strength = transparencyStrengthFromSettings(settings);
  /** Which appearance's cards are shown; starts on the one the window is using. */
  const [scheme, setScheme] = useState<'dark' | 'light'>(() => initialThemeScheme(settings.sidebarTheme));
  /** Once a look is picked on each side on purpose, the sides stop following each other. */
  const [pickedOnPurpose, setPickedOnPurpose] = useState({ dark: false, light: false });
  const pickDark = (darkThemePreset: DarkThemePreset) => {
    const patch: Partial<NonNullable<PanelProps['props']['settings']>> = { darkThemePreset };
    if (!pickedOnPurpose.light && darkThemePreset !== 'custom') {
      patch.lightThemePreset = LIGHT_PRESET_FOR_DARK[darkThemePreset];
    }
    setPickedOnPurpose((picked) => ({ ...picked, dark: true }));
    onUpdate(patch);
  };
  const pickLight = (lightThemePreset: LightThemePreset) => {
    const patch: Partial<NonNullable<PanelProps['props']['settings']>> = { lightThemePreset };
    if (!pickedOnPurpose.dark && lightThemePreset !== 'custom') {
      patch.darkThemePreset = DARK_PRESET_FOR_LIGHT[lightThemePreset];
    }
    setPickedOnPurpose((picked) => ({ ...picked, light: true }));
    onUpdate(patch);
  };
  const toggleTransparency = () => {
    const turningOn = !transparencyOn;
    const patch: Partial<NonNullable<PanelProps['props']['settings']>> = {
      windowGlass: windowGlassForTransparency(settings.windowGlass, turningOn),
    };
    const notes: string[] = [];
    if (turningOn && !settings.sidebarTheme.startsWith('dark')) {
      patch.sidebarTheme = 'dark-2';
      setScheme('dark');
      notes.push('Switched to Dark appearance so the transparency shows.');
    }
    const restartNote = windowGlassRestartNote().trim();
    if (turningOn && restartNote) {
      notes.push(restartNote);
    }
    if (notes.length > 0) {
      toast(notes.join(' '));
    }
    onUpdate(patch);
  };
  return (
    <div className='glass pcard plook'>
      <div className='label'>Appearance</div>
      <div className='opts' style={{ gridTemplateColumns: `repeat(${APPEARANCE_CHOICES.length}, minmax(0, 1fr))` }}>
        {APPEARANCE_CHOICES.map((choice) => (
          <button
            key={choice.value}
            type='button'
            className={'opt appearance' + (settings.sidebarTheme === choice.value ? ' sel' : '')}
            onClick={() => onUpdate({ sidebarTheme: choice.value })}
          >
            <span className='nm'>{choice.label}</span>
          </button>
        ))}
      </div>
      <div className='plook-theme-head'>
        <span className='label'>
          Theme colour{' '}
          <span className='plook-colour-readout'>
            {themePresetLabel(scheme, scheme === 'dark' ? settings.darkThemePreset : settings.lightThemePreset)}
          </span>
        </span>
        <div className='plook-scheme' role='tablist' aria-label='Show themes for'>
          {(['dark', 'light'] as const).map((value) => (
            <button
              key={value}
              type='button'
              role='tab'
              aria-selected={scheme === value}
              className={scheme === value ? 'sel' : undefined}
              onClick={() => setScheme(value)}
            >
              {value === 'dark' ? 'Dark' : 'Light'}
            </button>
          ))}
        </div>
      </div>
      {scheme === 'dark' ? (
        <ThemeSwatchGrid
          className='plook-colour-grid'
          label='Dark mode colour'
          onSelect={pickDark}
          swatches={darkThemeSwatches(settings)}
          value={settings.darkThemePreset}
        />
      ) : (
        <ThemeSwatchGrid
          className='plook-colour-grid'
          label='Light mode colour'
          onSelect={pickLight}
          swatches={lightThemeSwatches(settings)}
          value={settings.lightThemePreset}
        />
      )}
      <StepSlider
        label='Colourfulness'
        steps={COLOURFULNESS_CHOICES.map((choice) => choice.label)}
        value={colourfulnessIndex}
        onChange={(index) => onUpdate(colourfulnessPatch(index))}
      />
      {windowGlassAvailable() ? (
        <SliderRow
          disabled={!transparencyOn}
          label='Transparency'
          leading={<Toggle size='sm' on={transparencyOn} onClick={toggleTransparency} label='Transparency' />}
          max={TRANSPARENCY_STRENGTH_MAX}
          min={TRANSPARENCY_STRENGTH_MIN}
          onChange={(value) => onUpdate(transparencyStrengthPatch(value))}
          step={TRANSPARENCY_STRENGTH_STEP}
          value={strength.nearest}
          valueText={!transparencyOn ? 'Off' : strength.exact === undefined ? 'Custom' : `${strength.nearest}%`}
        />
      ) : null}
      {onOpenThemeSettings ? (
        <button type='button' className='plook-more-link' onClick={onOpenThemeSettings}>
          More theme options in Settings → Theme ›
        </button>
      ) : null}
    </div>
  );
}

/** One row: the label, a five-stop slider, and the current stop's name. `value` -1 means none (tuned by hand). */
function StepSlider({
  label,
  steps,
  value,
  onChange,
}: {
  label: string;
  steps: readonly string[];
  value: number;
  onChange: (index: number) => void;
}) {
  const current = value < 0 ? undefined : steps[value];
  return (
    <SliderRow
      label={label}
      max={steps.length - 1}
      min={0}
      onChange={onChange}
      step={1}
      value={value < 0 ? Math.floor((steps.length - 1) / 2) : value}
      valueText={current ?? 'Custom'}
    />
  );
}

/** One row: the label on the left, the slider, and its value on the right. */
function SliderRow({
  disabled,
  label,
  leading,
  min,
  max,
  step,
  value,
  valueText,
  onChange,
}: {
  disabled?: boolean;
  label: string;
  /** A control between the label and the slider, such as the switch that turns it on. */
  leading?: ReactNode;
  min: number;
  max: number;
  step: number;
  value: number;
  valueText: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className={'pstep' + (disabled ? ' off' : '')}>
      <span className='pstep-label'>{label}</span>
      {leading}
      <input
        aria-label={label}
        aria-valuetext={valueText}
        className='pstep-range'
        disabled={disabled}
        max={max}
        min={min}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
        step={step}
        type='range'
        value={value}
      />
      <span className='pstep-value'>{valueText}</span>
    </div>
  );
}
