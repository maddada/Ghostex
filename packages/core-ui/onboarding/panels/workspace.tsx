import { useState } from 'react';
import type { OnboardingViewKey } from '../contract';
import { isViewOn, withViewsOn, type PanelProps, VIEW_KEYS } from '../onboarding-state';
import { WorkspaceWindow, type WorkspaceTab } from '../previews/workspace-window';
import { Cta, Eyebrow, Heading, Icon, Sub, Toggle, buttonProps, type IconName } from '../primitives';
import { box } from '../stage';

const VIEW_ROWS: readonly { id: OnboardingViewKey; icon: IconName; title: string; detail: string }[] = [
  { id: 'browser', icon: 'target', title: 'Browser', detail: 'Preview and inspect the app your agent is building.' },
  { id: 'docs', icon: 'list', title: 'Docs', detail: 'Markdown, mockups, diagrams and annotations.' },
  { id: 'code', icon: 'code', title: 'Code', detail: 'VS Code, built in: source, diffs and review.' },
  { id: 'kanban', icon: 'kanban', title: 'Kanban', detail: 'Split work into cards and hand each one to an agent.' },
  { id: 'automate', icon: 'bolt', title: 'Automate', detail: 'Run agents on a schedule, once or on repeat.' },
];
const RECOMMENDED: readonly OnboardingViewKey[] = ['docs', 'browser'];
const ROW_TOP: Record<Exclude<OnboardingViewKey, 'browser'>, number> = {
  docs: 464,
  code: 532,
  kanban: 600,
  automate: 668,
};

export function WorkspacePanel({ props, go, toast }: PanelProps) {
  const { settings } = props;
  const views = Object.fromEntries(VIEW_KEYS.map((key) => [key, isViewOn(settings, key)])) as Record<
    OnboardingViewKey,
    boolean
  >;
  const drive = views.browser && props.browserSkillInstalled;
  const recommendedOn = RECOMMENDED.every((key) => views[key]);

  const [agent, setAgent] = useState('claude');
  const [tab, setTab] = useState<WorkspaceTab>('browser');
  const [previewed, setPreviewed] = useState<WorkspaceTab | null>(null);
  const [recent, setRecent] = useState<OnboardingViewKey[]>(['docs', 'browser']);
  const current: WorkspaceTab =
    previewed ?? (tab === 'agents' || views[tab] ? tab : ([...recent].reverse().find((key) => views[key]) ?? 'agents'));
  const showTab = (next: WorkspaceTab) => {
    setPreviewed(null);
    setTab(next);
  };

  const setView = (key: OnboardingViewKey, on: boolean) => {
    if (!settings) return;
    props.onChange(withViewsOn(settings, { [key]: on }));
    if (on) {
      setRecent((list) => [...list.filter((item) => item !== key), key]);
      setTab(key);
      setPreviewed((value) => (value === key ? value : null));
    }
  };
  const toggleBrowserSkill = () => {
    if (!views.browser) return;
    if (props.browserSkillInstalled) props.onUninstallBrowserSkill();
    else props.onInstallBrowserSkill();
    showTab('browser');
  };
  const applyRecommended = () => {
    if (!settings) return;
    const missing = RECOMMENDED.filter((key) => !views[key]);
    if (missing.length === 0) return;
    props.onChange(withViewsOn(settings, Object.fromEntries(missing.map((key) => [key, true]))));
    setRecent((list) => [...list.filter((item) => !missing.includes(item)), ...missing]);
    showTab(missing[missing.length - 1]);
  };

  return (
    <>
      <Eyebrow x={60} y={118}>
        Workspace
      </Eyebrow>
      <Heading x={60} y={144} w={640} size={46} l1='Choose what lives next' l2='to your agents.' />
      <Sub x={60} y={250} w={632} size={16}>
        Start lean; anything you hide stays available in Settings.
      </Sub>
      <button
        type='button'
        className={'rec-chip' + (recommendedOn ? ' on' : '')}
        style={box(60, 290, undefined, 38)}
        onClick={applyRecommended}
      >
        <Icon n='checkCircle' size={18} />
        Recommended · Browser + Docs
      </button>
      <div className={'glass vgroup' + (views.browser ? ' on' : '')} style={box(60, 340, 632, 112)}>
        <div className='vrow inner' {...buttonProps(() => setPreviewed('browser'))}>
          <Icon n='target' size={22} className='vicon' />
          <div>
            <div className='nm lg'>Browser</div>
            <div className='ss'>Preview and inspect the app your agent is building.</div>
          </div>
          <Toggle on={views.browser} onClick={() => setView('browser', !views.browser)} label='Browser' />
        </div>
        <div className={'vsub' + (views.browser ? '' : ' off')} {...buttonProps(() => setPreviewed('browser'))}>
          <Icon n='wrench' size={16} className='vsub-i' />
          <div className='vsub-t'>
            <span className='vsub-n'>Give agents the browser skill</span>
            <span className='vsub-d'>Agents can open, click, type and screenshot pages in this browser.</span>
          </div>
          <Toggle
            size='sm'
            on={drive}
            disabled={!views.browser}
            onClick={toggleBrowserSkill}
            label='Give agents the browser skill'
          />
        </div>
      </div>
      {VIEW_ROWS.filter((row) => row.id !== 'browser').map((row) => (
        <div
          key={row.id}
          className={'glass vrow' + (views[row.id] ? ' on' : '')}
          style={box(60, ROW_TOP[row.id as keyof typeof ROW_TOP], 632, 58)}
          {...buttonProps(() => setPreviewed(row.id))}
        >
          <Icon n={row.icon} size={22} className='vicon' />
          <div>
            <div className='nm lg'>{row.title}</div>
            <div className='ss'>{row.detail}</div>
          </div>
          <Toggle on={views[row.id]} onClick={() => setView(row.id, !views[row.id])} label={row.title} />
        </div>
      ))}
      <Cta
        filled
        style={{ position: 'absolute', left: 60, top: 758, height: 46, padding: '0 22px' }}
        onClick={() => go(4)}
      >
        Continue
      </Cta>
      <WorkspaceWindow
        views={views}
        drive={drive}
        current={current}
        onTab={(next) => next !== current && showTab(next)}
        agent={agent}
        onAgent={(next) => {
          setAgent(next);
          showTab('agents');
        }}
        toast={toast}
      />
    </>
  );
}
