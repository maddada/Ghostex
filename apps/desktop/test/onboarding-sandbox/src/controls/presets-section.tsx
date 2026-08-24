/*
 * Scenario presets. Rendered dynamically from SCENARIO_PRESETS so the engine
 * agent can add/rename presets without touching the control panel.
 */
import { SCENARIO_PRESETS } from '../engine/presets';
import { useSandboxStore } from '../state/store';
import { Btn, Note, Section, Toggle } from './control-primitives';
import { usePersistedState } from './controls-storage';

export function PresetsSection() {
  const appPhase = useSandboxStore((s) => s.appPhase);
  const applyPreset = useSandboxStore((s) => s.applyPreset);
  const relaunchApp = useSandboxStore((s) => s.relaunchApp);
  const [autoRelaunch, setAutoRelaunch] = usePersistedState('presets.autoRelaunch', true);

  const running = appPhase !== 'notRunning';

  return (
    <Section badge={SCENARIO_PRESETS.length} defaultOpen id='presets' title='Scenario presets'>
      <div className='cp-inline-option'>
        <Toggle
          checked={autoRelaunch}
          label='Relaunch after applying'
          onChange={setAutoRelaunch}
          title='Startup-only effects (flag burning, gxserver race) are only observable on a fresh launch'
        />
      </div>
      <div className='cp-preset-list'>
        {SCENARIO_PRESETS.map((preset) => (
          <button
            className='cp-preset'
            key={preset.id}
            onClick={() => {
              applyPreset(preset.id);
              if (autoRelaunch && running) relaunchApp();
            }}
            title={preset.description}
            type='button'
          >
            <span className='cp-preset-label'>{preset.label}</span>
            <span className='cp-preset-description'>{preset.description}</span>
          </button>
        ))}
      </div>
      {running && !autoRelaunch ? (
        <Note tone='warning'>
          The app is running. Onboarding decisions are made at startup, so relaunch to observe this preset&apos;s
          effects.
          <Btn onClick={relaunchApp} tone='ghost'>
            Relaunch now
          </Btn>
        </Note>
      ) : null}
    </Section>
  );
}
