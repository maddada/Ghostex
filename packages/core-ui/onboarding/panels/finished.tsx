import { useEffect, useRef, useState } from 'react';
import { InstallGuidePopup } from '../install-guide-popup';
import {
  ONBOARDING_INSTALL_GUIDE_URL,
  agentDisplayName,
  allInstalledHaveHooks,
  defaultAgentId as resolveDefaultAgentId,
  finishOnboarding,
  isViewOn,
  type PanelProps,
  VIEW_KEYS,
} from '../onboarding-state';
import { AgentLogo, Cta, Eyebrow, GhostexLogo, Heading, Icon, Lights, Popup, Sub } from '../primitives';
import { box, folderBasename } from '../stage';

const VIEW_TITLES: Record<string, string> = {
  browser: 'Browser',
  docs: 'Docs',
  code: 'Code',
  kanban: 'Kanban',
  automate: 'Automate',
};

const DEMO_LOGS: Record<string, readonly (readonly [string, string])[]> = {
  claude: [
    ['me', 'Get familiar with this repo and suggest a first task.'],
    ['bot', 'Read package.json, src/ and tests/. 42 files, one flaky test in tests/auth.'],
    ['bot', 'Suggestion: fix the flaky refresh test first; it blocks CI. Want me to start?'],
  ],
  codex: [
    ['me', 'Get familiar with this repo and suggest a first task.'],
    ['bot', 'Scanned the repo: TypeScript, Bun, 42 files, 17 tests.'],
    ['bot', 'The legacy test harness has 12 call sites. Porting those is a good first task.'],
  ],
  cursor: [
    ['me', 'Get familiar with this repo and suggest a first task.'],
    ['bot', '42 files, 48 design tokens still hard-coded in SCSS.'],
    ['bot', 'Moving those tokens to CSS variables is a clean first task. Want me to start?'],
  ],
  terminal: [
    ['cmd', '$ ls'],
    ['out', 'README.md  package.json  src  tests'],
    ['cmd', '$ git status'],
    ['out', 'On branch main · nothing to commit, working tree clean'],
  ],
};
const GENERIC_LOG = DEMO_LOGS.claude;
/** Delay before the after-onboarding popup appears, so the summary lands first. */
const FOLLOW_UP_DELAY_MS = 900;

export function FinishedPanel({ props, flow, setFlow, go, toast }: PanelProps) {
  const { settings, agents, computerUseState } = props;
  const folder = flow.finishedPath ?? props.pickedProjectFolder ?? '';
  const project = folder ? folderBasename(folder) : 'Your project';
  const defaultAgent = resolveDefaultAgentId(settings, agents);
  const startWith = flow.startWith ?? defaultAgent ?? 'terminal';
  const sessionView = settings?.preferredAgentInterface ?? 'chat';
  const notify = settings?.showMacOSAttentionNotifications ?? true;
  const viewsOn = VIEW_KEYS.filter((key) => isViewOn(settings, key));
  const viewsSummary = viewsOn.length
    ? viewsOn
        .map((key) =>
          key === 'browser' && props.browserSkillInstalled ? 'Browser (browser skill on)' : VIEW_TITLES[key]
        )
        .join(', ')
    : 'None, agents only';
  const connected = flow.integrationOn && allInstalledHaveHooks(agents);

  const rows: readonly (readonly [string, string, number])[] = [
    ['Default agent', agentDisplayName(agents, defaultAgent), 2],
    [
      'Ghostex integration',
      !flow.integrationOn ? 'Off' : connected ? 'Every detected agent connected' : 'Skipped for now',
      2,
    ],
    [
      'Computer Use',
      computerUseState === 'on'
        ? 'On'
        : computerUseState === 'permissions'
          ? 'On · your computer will ask for permission'
          : computerUseState === 'installing'
            ? 'Installing…'
            : 'Off',
      2,
    ],
    ['Workspace views', viewsSummary, 3],
    [
      'Mobile',
      (flow.phoneQueued ? 'Remote settings open after this screen' : 'Not now') + (notify ? ' · agent alerts on' : ''),
      4,
    ],
    ['Project', folder || 'No folder chosen', 5],
    ['First session', startWith === 'terminal' ? 'Terminal, no agent' : agentDisplayName(agents, startWith), 5],
    ['Session view', sessionView === 'chat' ? 'Chat' : 'Terminal', 5],
    ...(flow.installQueued ? [['Install guide', 'Opens after this screen', 2] as const] : []),
  ];

  const log = DEMO_LOGS[startWith] ?? GENERIC_LOG;
  const terminalMode = startWith === 'terminal' || sessionView === 'terminal';
  const [shownLines, setShownLines] = useState(1);
  const logRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (shownLines >= log.length) return;
    const timer = window.setTimeout(() => setShownLines((value) => value + 1), 1100);
    return () => window.clearTimeout(timer);
  }, [shownLines, log.length]);
  useEffect(() => {
    logRef.current?.scrollTo({ top: 1e5, behavior: 'smooth' });
  }, [shownLines]);

  const [followUp, setFollowUp] = useState<'ask' | 'guide' | null>(null);
  useEffect(() => {
    if (!flow.installQueued) return;
    const timer = window.setTimeout(() => setFollowUp('ask'), FOLLOW_UP_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [flow.installQueued]);
  const later = () => {
    setFollowUp(null);
    toast('You can do this anytime from Settings');
  };
  const openGuide = (url: string) => (props.onOpenInstallGuide ?? props.onOpenExternalUrl)(url);
  const tabs = ['Agents', ...viewsOn.map((key) => VIEW_TITLES[key])];
  const agentName = agentDisplayName(agents, startWith);

  return (
    <>
      <Eyebrow x={44} y={118}>
        You're set
      </Eyebrow>
      <Heading x={44} y={145} w={720} size={52} l1='Ghostex is open.' l2={`${project} is ready.`} />
      <Sub x={44} y={264} w={680} size={16}>
        Here is everything you chose. Click a row to change it; all of it stays changeable in Settings.
      </Sub>
      <div className='glass sum' style={box(44, 320, 699, rows.length * 44 + 18)}>
        {rows.map(([key, value, panel]) => (
          <button key={key} type='button' className='sum-row' onClick={() => go(panel)}>
            <span className='k'>{key}</span>
            <span className='v'>{value}</span>
            <span className='edit'>Edit</span>
          </button>
        ))}
      </div>
      <div className='actions' style={{ position: 'absolute', left: 44, top: 340 + rows.length * 44 + 30 }}>
        <Cta filled style={{ height: 48, padding: '0 22px' }} onClick={() => finishOnboarding(props, flow)}>
          Start working
        </Cta>
        <button type='button' className='ghost' style={{ marginLeft: 22 }} onClick={() => go(5)}>
          Back to setup
        </button>
      </div>
      <div className='glass-n fwin' style={box(812, 92, 812, 704)}>
        <div className='fwin-bar'>
          <Lights />
          <GhostexLogo size={26} />
          <span className='fwin-t'>{project}</span>
          <div className='fwin-tabs'>
            {tabs.map((tab, index) => (
              <span key={tab} className={index === 0 ? 'on' : ''}>
                {tab}
              </span>
            ))}
          </div>
        </div>
        <div className='fwin-body'>
          <aside className='fwin-side'>
            <div className='label'>Projects</div>
            <div className='fs-item on mono'>{project}</div>
            <div className='label' style={{ marginTop: 18 }}>
              Sessions
            </div>
            <div className='fs-sess'>
              <span className='dot run' />
              <span className='mono'>first-session</span>
              <em>{startWith === 'terminal' ? 'shell' : startWith} · live</em>
            </div>
            {connected && <div className='fs-note'>Connected: status and titles come from the agent.</div>}
          </aside>
          <div className='fwin-main'>
            <div className='fm-head'>
              <AgentLogo agentId={startWith} size={18} />
              <span className='mono'>first-session</span>
              <span className='ph-pill run'>
                <i />
                running
              </span>
              <span className='mono dim' style={{ marginLeft: 'auto' }}>
                {folder}
              </span>
            </div>
            <div className={'fm-log' + (terminalMode ? ' term' : '')} ref={logRef}>
              {log.slice(0, shownLines).map(([kind, text], index) =>
                terminalMode ? (
                  <div key={index} className={'fl mono ' + kind}>
                    {kind === 'me' ? <span className='acc'>{'> '}</span> : null}
                    {text}
                  </div>
                ) : (
                  <div key={index} className={'fb ' + kind}>
                    {kind === 'bot' && <span className='fb-who'>{agentName}</span>}
                    {text}
                  </div>
                )
              )}
              {shownLines < log.length && (
                <span className='typing'>
                  <i />
                  <i />
                  <i />
                </span>
              )}
            </div>
            <div className='fm-input'>
              <span className='dim' style={{ flex: 1, fontSize: 13.5 }}>
                {startWith === 'terminal' ? 'Type a command…' : `Message ${agentName}`}
              </span>
              <span className='kbd'>⌃G</span>
            </div>
          </div>
        </div>
      </div>
      <div className='node glass-n ready on' style={box(1008, 814, 420, 70)}>
        <span className='rt'>
          <span className='okg on'>
            <Icon n='check' size={15} sw={2.6} />
          </span>
          Project ready
        </span>
        <span className='rs'>The session keeps running if you close this window.</span>
      </div>
      {followUp === 'ask' && (
        <Popup title='Open the install guide now?' onClose={later} width={480}>
          <p className='modal-p'>It lists agents you can add; install one and Ghostex picks it up.</p>
          <div className='modal-actions'>
            <button type='button' className='ghost' onClick={later}>
              Later
            </button>
            <Cta filled arrow={false} className='sm' onClick={() => setFollowUp('guide')}>
              Open guide
            </Cta>
          </div>
        </Popup>
      )}
      {followUp === 'guide' && (
        <InstallGuidePopup
          toast={toast}
          onClose={() => {
            setFollowUp(null);
            setFlow({ installQueued: false });
          }}
          onOpenUrl={props.onOpenExternalUrl}
          onOpenGuide={(url) => {
            openGuide(url || ONBOARDING_INSTALL_GUIDE_URL);
            setFollowUp(null);
            setFlow({ installQueued: false });
          }}
        />
      )}
    </>
  );
}
