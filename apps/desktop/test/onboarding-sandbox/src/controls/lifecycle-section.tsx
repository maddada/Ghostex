/*
 * App lifecycle: Launch / Quit / Relaunch, phase indicator, launch counter.
 * Relaunch semantics are the engine's (persisted state survives, in-memory
 * suppressions reset) — this section only drives the store actions.
 */
import { useSandboxStore } from '../state/store';
import type { SimAppPhase } from '../state/types';
import { Btn, Section } from './control-primitives';

const PHASE_LABEL: Record<SimAppPhase, string> = {
  notRunning: 'not running',
  launching: 'launching…',
  running: 'running',
};

export function LifecycleSection() {
  const appPhase = useSandboxStore((s) => s.appPhase);
  const launchCount = useSandboxStore((s) => s.launchCount);
  const launchApp = useSandboxStore((s) => s.launchApp);
  const quitApp = useSandboxStore((s) => s.quitApp);
  const relaunchApp = useSandboxStore((s) => s.relaunchApp);

  return (
    <Section
      badge={<span className={`cp-phase cp-phase--${appPhase}`}>{PHASE_LABEL[appPhase]}</span>}
      defaultOpen
      id='lifecycle'
      title='App lifecycle'
    >
      <div className='cp-btn-row'>
        <Btn
          disabled={appPhase !== 'notRunning'}
          onClick={launchApp}
          title='Run the real startup sequence (gxserver bootstrap + CEF init races)'
          tone='primary'
        >
          Launch
        </Btn>
        <Btn
          disabled={appPhase === 'notRunning'}
          onClick={quitApp}
          title='Close every window; persisted state file survives'
          tone='danger'
        >
          Quit
        </Btn>
        <Btn
          onClick={relaunchApp}
          title='Quit + launch: increments the launch counter and resets in-memory suppressions'
        >
          Relaunch
        </Btn>
      </div>
      <div className='cp-lifecycle-meta'>
        <span>
          launch <strong>#{launchCount}</strong>
        </span>
        <span className='cp-dim'>in-memory suppressions reset on every launch; the state file does not</span>
      </div>
    </Section>
  );
}
