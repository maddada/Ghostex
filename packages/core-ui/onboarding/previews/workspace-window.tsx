import { useEffect, useState } from 'react';
import type { OnboardingViewKey } from '../contract';
import { AgentLogo, GhostexLogo, Icon, Lights, MoreMenu, type IconName } from '../primitives';
import { box, useNow, useReducedMotion } from '../stage';

export type WorkspaceTab = 'agents' | OnboardingViewKey;

const TAB_ORDER: readonly OnboardingViewKey[] = ['code', 'browser', 'kanban', 'automate', 'docs'];
const TAB_META: Record<WorkspaceTab, readonly [IconName, string]> = {
  agents: ['users', 'Agents'],
  code: ['code', 'Code'],
  browser: ['target', 'Browser'],
  kanban: ['kanban', 'Kanban'],
  automate: ['bolt', 'Automate'],
  docs: ['list', 'Docs'],
};
const DEMO_AGENTS: readonly (readonly [string, string, string])[] = [
  ['claude', 'Claude Code', 'Building feature...'],
  ['codex', 'Codex', 'Ready'],
  ['cursor', 'Cursor Agent', 'Ready'],
  ['other', 'Other agents', '20+ agents'],
];
const DEMO_CHATS: Record<string, readonly (readonly [string, string])[]> = {
  claude: [
    ['me', 'Build the pricing page.'],
    ['bot', 'Done: /pricing has three tiers and a yearly toggle. Tests pass.'],
  ],
  codex: [
    ['me', 'Review the pricing page.'],
    ['bot', 'Looks good. One nit: the yearly toggle has no label.'],
  ],
  cursor: [['bot', 'Ready when you are.']],
  other: [['bot', 'Gemini CLI and OpenCode are ready in this project.']],
};

function SessionView({ agent }: { agent: string }) {
  const meta = DEMO_AGENTS.find(([id]) => id === agent) ?? DEMO_AGENTS[0];
  return (
    <div className='wv wv-sess'>
      <div className='pane-h'>
        <AgentLogo agentId={agent} size={20} />
        <span className='pt'>{meta[1]}</span>
        <span className='ph-pill run'>
          <i />
          {agent === 'claude' ? 'Running' : 'Ready'}
        </span>
      </div>
      <div className='wv-msgs'>
        {DEMO_CHATS[agent].map(([kind, text]) => (
          <div key={text} className={'fb ' + kind}>
            {kind === 'bot' && <span className='fb-who'>{meta[1]}</span>}
            {text}
          </div>
        ))}
      </div>
      <div className='fm-input'>
        <span className='dim'>Message {meta[1]}</span>
      </div>
    </div>
  );
}

function BrowserView({ drive, toast }: { drive: boolean; toast: (message: string) => void }) {
  const reduced = useReducedMotion();
  const [clicked, setClicked] = useState(false);
  useEffect(() => {
    if (!drive || reduced) return;
    let repeat: number | undefined;
    const first = window.setTimeout(() => {
      setClicked(true);
      repeat = window.setInterval(() => setClicked(true), 5200);
    }, 2600);
    return () => {
      window.clearTimeout(first);
      if (repeat !== undefined) window.clearInterval(repeat);
    };
  }, [drive, reduced]);
  useEffect(() => {
    if (!clicked) return;
    const timer = window.setTimeout(() => setClicked(false), 2000);
    return () => window.clearTimeout(timer);
  }, [clicked]);
  return (
    <div className='wv'>
      <div className='urlbar mono'>
        <Icon n='arrowL' size={12} />
        <Icon n='arrowR' size={12} />
        <Icon n='refresh' size={12} />
        <span>http://localhost:3000</span>
      </div>
      <div className='site lg'>
        <div className='site-nav'>
          <span className='acme'>
            <b>A</b> Acme
          </span>
          <span>Product</span>
          <span>Docs</span>
          <span>Pricing</span>
          <span className='signin'>Sign in</span>
        </div>
        <div className='site-h'>
          {clicked ? "You're in." : 'Build faster'}
          <br />
          {clicked ? 'Welcome to Acme.' : 'with AI agents.'}
        </div>
        <div className='site-s'>From idea to production, together.</div>
        <button
          type='button'
          className='site-btn'
          onClick={() => {
            setClicked(true);
            toast('You clicked it');
          }}
        >
          Get started <Icon n='arrowR' size={13} />
          {drive && !reduced && <span className='agent-cursor' aria-hidden='true' />}
        </button>
        <div className='site-deco'>
          <Lights small />
          <i />
          <i />
          <i />
        </div>
      </div>
      <div className={'wv-foot skill' + (drive ? ' on' : '')}>
        <b>
          {drive
            ? 'Your agent is using the browser skill.'
            : "The browser skill is off, so agents can't touch this browser."}
        </b>
        <em>
          {drive
            ? 'It can open pages, click, type, read the page and take screenshots. Ask your agent to use it; remove it in Settings → Integrations.'
            : 'Switch it on and agents can open pages, click, type, read the page and take screenshots.'}
        </em>
      </div>
    </div>
  );
}

function DocsView() {
  const [saved, setSaved] = useState(false);
  return (
    <div className='wv'>
      <div className='pane-h'>
        <span className='pt'>Project plan</span>
        <button
          type='button'
          className={'edit-chip' + (saved ? ' saved' : '')}
          onClick={() => setSaved((value) => !value)}
        >
          {saved ? (
            <>
              <Icon n='check' size={12} /> Saved
            </>
          ) : (
            'Editing...'
          )}
        </button>
      </div>
      <div className='doc'>
        <div className='doc-p'>
          An overview of the architecture, key components, and next steps for the agent-driven application.
        </div>
        <div className='doc-s'>System diagram</div>
        <div className='doc-dia'>
          <div className='dnode'>
            <Icon n='users' size={16} />
            User
          </div>
          <span className='doc-line' />
          <div className='dnode agent'>Agent</div>
          <span className='doc-line' />
          <div className='doc-col'>
            <div className='dnode row'>
              <Icon n='target' size={15} />
              Browser
            </div>
            <div className='dnode row'>
              <Icon n='list' size={15} />
              Docs
            </div>
          </div>
        </div>
        <div className='doc-s'>Next steps</div>
        <ul className='doc-ul'>
          <li>Ship the pricing page</li>
          <li>Label the yearly toggle</li>
          <li>Hand release notes to Automate</li>
        </ul>
      </div>
    </div>
  );
}

const DIFF_LINES: readonly (readonly [string, string])[] = [
  ['ctx', "import { lock } from './lock';"],
  ['ctx', ''],
  ['ctx', 'export async function refresh(session: Session) {'],
  ['del', '  const token = await fetchToken(session);'],
  ['del', '  session.token = token;'],
  ['add', '  return lock.run(session.id, async () => {'],
  ['add', '    const token = await fetchToken(session);'],
  ['add', '    session.token = token;'],
  ['add', '    return token;'],
  ['add', '  });'],
  ['ctx', '}'],
];

function CodeView() {
  const reduced = useReducedMotion();
  const [shown, setShown] = useState(() => (reduced ? DIFF_LINES.length : 0));
  useEffect(() => {
    if (reduced) return;
    const timer = window.setInterval(() => setShown((value) => (value >= DIFF_LINES.length + 4 ? 0 : value + 1)), 420);
    return () => window.clearInterval(timer);
  }, [reduced]);
  return (
    <div className='wv wv-code'>
      <aside className='code-tree mono'>
        <span className='dim'>src</span>
        <span className='dim'>{'  auth'}</span>
        <span className='on'>{'    refresh.ts'}</span>
        <span>{'    lock.ts'}</span>
        <span>{'  pricing.tsx'}</span>
      </aside>
      <div className='code-main'>
        <div className='code-tabs mono'>
          <span className='on'>refresh.ts</span>
          <span>M</span>
        </div>
        <div className='code-lines mono'>
          {DIFF_LINES.slice(0, Math.min(shown, DIFF_LINES.length)).map(([kind, text], index) => (
            <div key={index} className={'cl ' + kind}>
              <span className='ln'>{index + 1}</span>
              <span className='sg'>{kind === 'add' ? '+' : kind === 'del' ? '−' : ' '}</span>
              {text}
            </div>
          ))}
          {shown < DIFF_LINES.length && <span className='cur' />}
        </div>
      </div>
    </div>
  );
}

const KANBAN_CARDS: readonly (readonly [string, string])[] = [
  ['Label the yearly toggle', 'codex'],
  ['Fix the flaky refresh test', 'claude'],
  ['Move tokens to CSS variables', 'cursor'],
  ['Write release notes', 'claude'],
];

function KanbanView() {
  const reduced = useReducedMotion();
  const [step, setStep] = useState(0);
  useEffect(() => {
    if (reduced) return;
    const timer = window.setInterval(() => setStep((value) => (value + 1) % 5), 1800);
    return () => window.clearInterval(timer);
  }, [reduced]);
  const columnOf = (index: number) => (index < step - 1 ? 'done' : index === step - 1 ? 'prog' : 'todo');
  const columns: readonly (readonly [string, string])[] = [
    ['todo', 'To do'],
    ['prog', 'In progress'],
    ['done', 'Done'],
  ];
  return (
    <div className='wv wv-kan'>
      {columns.map(([id, title]) => {
        const cards = KANBAN_CARDS.filter((_, index) => columnOf(index) === id);
        return (
          <div key={id} className='kcol'>
            <div className='kh'>
              {title} <b>{cards.length}</b>
            </div>
            {cards.map(([text, agent]) => (
              <div key={text} className={'kcard ' + id}>
                <span>{text}</span>
                {id !== 'todo' && <AgentLogo agentId={agent} size={14} />}
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}

const AUTOMATIONS: readonly (readonly [string, string, string])[] = [
  ['Nightly dependency bump', 'Every night at 02:00', 'codex'],
  ['Weekly issue triage', 'Mondays at 09:00', 'claude'],
  ['Release notes', 'Once, Friday at 17:00', 'cursor'],
];

function AutomateView({ toast }: { toast: (message: string) => void }) {
  const now = useNow(1000);
  const seconds = 59 - (Math.floor(now / 1000) % 60);
  return (
    <div className='wv wv-auto'>
      <div className='pane-h'>
        <span className='pt'>Automations</span>
        <span className='anext mono'>next run in 00:{String(seconds).padStart(2, '0')}</span>
      </div>
      {AUTOMATIONS.map(([title, schedule, agent], index) => (
        <div key={title} className='job'>
          <span className={'ad' + (index === 0 ? ' on' : '')} />
          <span className='job-t'>
            <b>{title}</b>
            <em>{schedule}</em>
          </span>
          <AgentLogo agentId={agent} size={18} />
        </div>
      ))}
      <button type='button' className='job-new' onClick={() => toast('New automation')}>
        <Icon n='plus' size={14} /> New automation
      </button>
    </div>
  );
}

function ViewBody({
  tab,
  drive,
  agent,
  toast,
}: {
  tab: WorkspaceTab;
  drive: boolean;
  agent: string;
  toast: (message: string) => void;
}) {
  switch (tab) {
    case 'agents':
      return <SessionView agent={agent} />;
    case 'browser':
      return <BrowserView drive={drive} toast={toast} />;
    case 'docs':
      return <DocsView />;
    case 'code':
      return <CodeView />;
    case 'kanban':
      return <KanbanView />;
    case 'automate':
      return <AutomateView toast={toast} />;
  }
}

/** The mock workspace window on the Workspace panel: titlebar tabs mirror the toggles on the left (`Pm`). */
export function WorkspaceWindow({
  views,
  drive,
  current,
  onTab,
  agent,
  onAgent,
  toast,
}: {
  views: Record<OnboardingViewKey, boolean>;
  drive: boolean;
  current: WorkspaceTab;
  onTab: (tab: WorkspaceTab) => void;
  agent: string;
  onAgent: (agent: string) => void;
  toast: (message: string) => void;
}) {
  const tabs: WorkspaceTab[] = ['agents', ...TAB_ORDER.filter((view) => views[view] || view === current)];
  return (
    <>
      <div className='glass wswin' style={box(758, 116, 875, 720)} />
      <div className='wsbar' style={box(758, 116, 875, 54)}>
        <Lights />
        <GhostexLogo size={28} />
        <span className='wst'>my-app</span>
        <div className='wtabs' role='tablist' aria-label='Workspace views'>
          {tabs.map((tab) => {
            const previewOnly = tab !== 'agents' && !views[tab];
            return (
              <button
                key={tab}
                type='button'
                role='tab'
                aria-selected={current === tab}
                className={'wtab' + (previewOnly ? ' ghost' : current === tab ? ' on' : '')}
                onClick={() => onTab(tab)}
              >
                <Icon n={TAB_META[tab][0]} size={14} />
                {TAB_META[tab][1]}
                {tab === 'browser' && drive && <span className='wtab-badge'>skill on</span>}
                {previewOnly && <span className='wtab-adds'>↑ adds this tab</span>}
              </button>
            );
          })}
        </div>
        <MoreMenu items={['New session', 'Split right', 'Settings']} onPick={(item) => toast(item)} />
      </div>
      <div className='wscol' style={box(760, 182, 200, 640)}>
        <div className='wscol-h'>
          Agents
          <button
            type='button'
            className='icon-btn'
            onClick={() => toast('New agent session')}
            aria-label='New agent session'
          >
            <Icon n='plus' size={16} />
          </button>
        </div>
        {DEMO_AGENTS.map(([id, name, status]) => (
          <button
            key={id}
            type='button'
            className={'wsa' + (agent === id && current === 'agents' ? ' on' : '')}
            onClick={() => onAgent(id)}
          >
            <span className='abox sm'>
              <AgentLogo agentId={id} size={22} />
            </span>
            <span>
              <b>{name}</b>
              <em>
                <i />
                {status}
              </em>
            </span>
          </button>
        ))}
      </div>
      <div key={current} className={'wview wview-' + current} style={box(972, 182, 650, 640)}>
        <ViewBody tab={current} drive={drive} agent={agent} toast={toast} />
      </div>
    </>
  );
}
