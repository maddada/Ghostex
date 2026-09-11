import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import grainUrl from './assets/grain.png';
import type { OnboardingModalProps } from './contract';
import { DarkVeil } from './dark-veil';
import { ensureOnboardingFonts } from './fonts';
import { Nebula } from './nebula';
import { INITIAL_FLOW_STATE, type OnboardingFlowState, type PanelProps, withViewsOn } from './onboarding-state';
import { AgentsPanel } from './panels/agents';
import { FinishedPanel } from './panels/finished';
import { GetStartedPanel } from './panels/get-started';
import { MobilePanel } from './panels/mobile';
import { WelcomePanel } from './panels/welcome';
import { WorkspacePanel } from './panels/workspace';
import { GhostexLogo, Icon } from './primitives';
import { PANEL_COUNT, PANEL_DIVIDER_X, PANEL_LOCKUP_X, STAGE_HEIGHT, STAGE_WIDTH, VEIL_LEFT, box } from './stage';
import './onboarding.css';

export type OnboardingModalComponentProps = OnboardingModalProps & {
  /** Panel shown when the modal opens (1-5), or `'finished'` for the "You're set" screen. Storybook only. */
  initialPanel?: number | 'finished';
};

const PANELS: readonly ((props: PanelProps) => React.JSX.Element)[] = [
  WelcomePanel,
  AgentsPanel,
  WorkspacePanel,
  MobilePanel,
  GetStartedPanel,
];
const TOAST_MS = 2400;

function clampPanel(panel: number): number {
  return Math.max(1, Math.min(PANEL_COUNT, Math.round(panel) || 1));
}

/** Scales the fixed 1672x941 stage to fit whatever box the host gives the modal. */
function useStageScale(ref: React.RefObject<HTMLDivElement | null>, active: boolean): number {
  const [scale, setScale] = useState(1);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!active || !element) return;
    const measure = () => {
      const { clientWidth, clientHeight } = element;
      if (clientWidth > 0 && clientHeight > 0)
        setScale(Math.min(clientWidth / STAGE_WIDTH, clientHeight / STAGE_HEIGHT));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref, active]);
  return scale;
}

/** The five-panel first-run onboarding (see contract.ts for the props and the decision behind it). */
export function OnboardingModal(props: OnboardingModalComponentProps) {
  const { isOpen, initialPanel = 1 } = props;
  const containerRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const scale = useStageScale(containerRef, isOpen);
  const [panel, setPanel] = useState(() => (initialPanel === 'finished' ? PANEL_COUNT : clampPanel(initialPanel)));
  const [flow, setFlowState] = useState<OnboardingFlowState>(() => ({
    ...INITIAL_FLOW_STATE,
    finished: initialPanel === 'finished',
  }));
  const [toast, setToast] = useState<{ id: number; message: string } | null>(null);
  const [epoch, setEpoch] = useState(0);

  useLayoutEffect(() => {
    if (isOpen) ensureOnboardingFonts();
  }, [isOpen]);

  const wasOpen = useRef(isOpen);
  useEffect(() => {
    if (isOpen && !wasOpen.current) {
      setPanel(initialPanel === 'finished' ? PANEL_COUNT : clampPanel(initialPanel));
      setFlowState({ ...INITIAL_FLOW_STATE, finished: initialPanel === 'finished' });
      setToast(null);
      setEpoch((value) => value + 1);
    }
    wasOpen.current = isOpen;
  }, [isOpen, initialPanel]);

  /** First run ever: Browser + Docs become the enabled views once; reopening from Tips > Setup leaves them alone (DECISION in contract.ts). */
  const appliedFirstRunViews = useRef(false);
  const { firstRun, settings, onChange } = props;
  useEffect(() => {
    if (!isOpen) {
      appliedFirstRunViews.current = false;
      return;
    }
    if (!firstRun || !settings || appliedFirstRunViews.current) return;
    appliedFirstRunViews.current = true;
    onChange(withViewsOn(settings, { browser: true, docs: true, code: false, kanban: false, automate: false }));
  }, [isOpen, firstRun, settings, onChange]);

  const setFlow = useCallback(
    (patch: Partial<OnboardingFlowState>) => setFlowState((current) => ({ ...current, ...patch })),
    []
  );
  const go = useCallback((next: number) => {
    setPanel(clampPanel(next));
    setFlowState((current) => (current.finished ? { ...current, finished: false } : current));
  }, []);
  const showToast = useCallback((message: string) => setToast({ id: Date.now() + Math.random(), message }), []);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
      const target = event.target as HTMLElement | null;
      if (target?.closest?.('input,textarea,[contenteditable]')) return;
      if (stageRef.current?.querySelector('.modal-back')) return;
      event.preventDefault();
      go(event.key === 'ArrowRight' ? panel + 1 : panel - 1);
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isOpen, panel, go]);

  if (!isOpen) return null;

  const Panel = PANELS[panel - 1];
  const dividerX = PANEL_DIVIDER_X[panel - 1];
  const lockupX = PANEL_LOCKUP_X[panel - 1];
  const footRight = dividerX - 35;
  const showFinished = flow.finished && panel === PANEL_COUNT;
  const panelProps: PanelProps = { props, flow, setFlow, go, toast: showToast };

  return (
    <div className='gx-onboarding' ref={containerRef} data-panel={panel}>
      <div className='stage' ref={stageRef} style={{ transform: `translate(-50%,-50%) scale(${scale})` }}>
        <Nebula active={isOpen} />
        <div
          className='veil'
          style={{
            ...box(VEIL_LEFT, 0, STAGE_WIDTH - VEIL_LEFT, STAGE_HEIGHT),
            clipPath: `inset(0 0 0 ${dividerX - VEIL_LEFT}px)`,
          }}
        >
          <DarkVeil
            active={isOpen}
            hueShift={25}
            noiseIntensity={0.01}
            scanlineIntensity={1}
            speed={0.6}
            warpAmount={0.2}
          />
        </div>
        {dividerX < STAGE_WIDTH && <div className='divider' style={{ left: dividerX }} />}
        <div className='lockup' style={{ left: lockupX - 9 }}>
          <GhostexLogo size={34} />
          <span>Ghostex</span>
        </div>
        <div className='counter' style={{ left: footRight - 120 }}>
          <b>{String(panel).padStart(2, '0')}</b> <span>/ 0{PANEL_COUNT}</span>
        </div>
        <div className='scene' key={`${epoch}-${panel}-${showFinished}`}>
          {showFinished ? <FinishedPanel {...panelProps} /> : <Panel {...panelProps} />}
        </div>
        <div className='foot' style={{ left: lockupX, width: footRight - lockupX }}>
          <button type='button' className='back' disabled={panel === 1} onClick={() => go(panel - 1)}>
            <Icon n='arrowL' size={18} />
            Back
          </button>
          <div className='dots' role='tablist' aria-label='Panels'>
            {PANELS.map((_, index) => (
              <button
                key={index}
                type='button'
                role='tab'
                aria-label={`Panel ${index + 1}`}
                aria-selected={index + 1 === panel}
                onClick={() => go(index + 1)}
              >
                <i className={index + 1 === panel ? 'on' : index + 1 < panel ? 'past' : ''} />
              </button>
            ))}
          </div>
          <button type='button' className='next' disabled={panel === PANEL_COUNT} onClick={() => go(panel + 1)}>
            Next
            <Icon n='arrowR' size={18} />
          </button>
        </div>
        {toast && (
          <div
            key={toast.id}
            className='toast'
            style={{ left: dividerX >= STAGE_WIDTH ? STAGE_WIDTH / 2 : dividerX + (STAGE_WIDTH - dividerX) / 2 }}
          >
            <Icon n='checkCircle' size={15} />
            {toast.message}
          </div>
        )}
        <div className='vignette' />
        <div className='grain' style={{ backgroundImage: `url(${grainUrl})` }} />
      </div>
    </div>
  );
}
