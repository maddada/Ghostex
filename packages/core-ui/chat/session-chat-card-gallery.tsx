import { useState, type ReactNode } from 'react';
import { Button } from '@/packages/components/ui/button';
import { Textarea } from '@/packages/components/ui/textarea';
import type { SessionChatInteractivePrompt, SessionChatTerminalNotice } from '@/packages/shared/session-chat';
import { SessionChatInteractiveCard } from './session-chat-interactive-card';
import { SessionChatTerminalNoticeCard } from './session-chat-terminal-notice-card';
import { SessionChatComposerNotReadyNotice } from './session-chat-composer-not-ready';
import { SessionChatAgentTasksPanel } from './session-chat-agent-tasks-panel';
import { SessionChatAgentFleetStrip } from './session-chat-agent-fleet-strip';
import { SessionChatActivityRow } from './session-chat-activity-row';
import { DETECTED_NOTICE_EXAMPLES } from './session-chat-card-gallery-notices';
import { DIALOG_EXAMPLES, PICKER_EXAMPLES } from './session-chat-card-gallery-dialogs';

type Outcome = 'success' | 'failure' | 'pending';
type Act = (label: string) => Promise<void>;
const AT = new Date().toISOString();
const APPROVAL: SessionChatInteractivePrompt = { kind: 'approval', tool: 'Bash', summary: 'git diff --stat' };
const QUESTION = {
  question: 'Which part should I update first?',
  header: 'Next step',
  multiSelect: false,
  options: [
    { label: 'Shared controls', description: 'Unify the buttons and inputs.' },
    { label: 'Card layouts', description: 'Align padding, borders and typography.' },
  ],
};
const PROMPTS: { label: string; prompt: SessionChatInteractivePrompt; readOnly?: boolean }[] = [
  { label: 'Command approval', prompt: APPROVAL },
  { label: 'Command approval without a command excerpt', prompt: { kind: 'approval', tool: 'Write' } },
  { label: 'Command approval, input held elsewhere', prompt: APPROVAL, readOnly: true },
  { label: 'Single-choice question with a custom answer', prompt: { kind: 'question', questions: [QUESTION] } },
  { label: 'Multiple-choice question', prompt: { kind: 'question', questions: [{ ...QUESTION, multiSelect: true }] } },
  {
    label: 'Question without a custom answer',
    prompt: { kind: 'question', questions: [{ ...QUESTION, allowCustom: false }] },
  },
  {
    label: 'Multiple questions, with Back / Skip / Next / Send answer',
    prompt: {
      kind: 'question',
      questions: [
        QUESTION,
        { ...QUESTION, header: 'Validation', question: 'Which sizes should I review?', multiSelect: true },
      ],
    },
  },
  { label: 'Question, input held elsewhere', prompt: { kind: 'question', questions: [QUESTION] }, readOnly: true },
];

const DELIVERY_NOTICES: SessionChatTerminalNotice[] = [
  {
    kind: 'deliveryFailed',
    severity: 'error',
    title: 'Message delivery could not be confirmed',
    detail: 'Check the terminal before sending again.',
    source: 'watchdog',
    detectedAt: AT,
  },
  {
    kind: 'apiRefusal',
    severity: 'warning',
    title: 'The agent declined this request',
    detail: 'Review the response in the terminal before continuing.',
    source: 'watchdog',
    detectedAt: AT,
  },
  {
    kind: 'queuedInput',
    severity: 'info',
    title: 'Input is queued in the terminal',
    detail: 'The agent has not consumed the queued input yet.',
    source: 'screen',
    detectedAt: AT,
  },
].map((notice) => ({
  ...notice,
  actions: [{ id: 'switchToTerminal', label: 'Open terminal', kind: 'switchToTerminal' }],
})) as SessionChatTerminalNotice[];

function Example({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className='ghostex-session-chat-scope grid min-w-0 gap-2' data-gallery-example={label}>
      <p className='text-xs text-muted-foreground'>{label}</p>
      {children}
    </div>
  );
}

function NoticeExample({
  notice,
  act,
  readOnly = false,
}: {
  notice: SessionChatTerminalNotice;
  act: Act;
  readOnly?: boolean;
}) {
  return (
    <SessionChatTerminalNoticeCard
      notice={notice}
      canSend={!readOnly}
      onSendKeys={async () => act(`${notice.title}: send keys`)}
      onAnswerChoice={async (index) => act(`${notice.title}: option ${index + 1}`)}
      onAnswerDialog={async (answer) => act(`${notice.title}: ${answer.dialogAction ?? 'choose option'}`)}
      onSwitchToTerminal={() => {
        void act('Open terminal').catch(() => {});
      }}
    />
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section id={title.replaceAll(' ', '-')} className='grid min-w-0 scroll-mt-6 gap-5 border-t border-border pt-5'>
      <h2 className='text-base font-semibold'>{title}</h2>
      {children}
    </section>
  );
}

/** Story-only composition of the production cards. Actions never reach a session. */
export function SessionChatCardGallery() {
  const [version, setVersion] = useState(0);
  const [outcome, setOutcome] = useState<Outcome>('success');
  const [lastAction, setLastAction] = useState<string | null>(null);
  const act: Act = async (label) => {
    setLastAction(label);
    if (outcome === 'failure') throw new Error('Preview: the action could not be delivered. Try again.');
    if (outcome === 'pending') await new Promise<void>(() => {});
  };
  const groups = [...new Set(DETECTED_NOTICE_EXAMPLES.map((entry) => entry.group))];
  const sections = [
    'Questions and approvals',
    'Claude pickers',
    'Terminal menus and forms',
    'Input and delivery states',
    'Agent activity',
    ...groups,
  ];
  return (
    <>
      <div className='grid gap-3 border-t border-border pt-4'>
        <h2 className='text-base font-semibold'>Chat card gallery</h2>
        <p className='text-sm text-muted-foreground'>
          Production components with sample content. Expand titles and terminal output to compare their other states.
          The action result setting exposes pending and error styles when you click a card action.
        </p>
        <div className='flex flex-wrap items-center gap-3'>
          <label className='flex items-center gap-2 text-sm'>
            Action result
            <select
              aria-label='Preview action result'
              className='rounded border border-input bg-background px-2 py-1'
              value={outcome}
              onChange={(event) => setOutcome(event.target.value as Outcome)}
            >
              <option value='success'>Success</option>
              <option value='failure'>Failure</option>
              <option value='pending'>Pending</option>
            </select>
          </label>
          <Button
            size='sm'
            variant='outline'
            onClick={() => {
              setVersion((value) => value + 1);
              setLastAction(null);
            }}
          >
            Reset all examples
          </Button>
        </div>
        <nav aria-label='Card categories' className='flex flex-wrap gap-x-4 gap-y-2 text-xs text-muted-foreground'>
          {sections.map((title) => (
            <a key={title} className='underline underline-offset-2' href={`#${title.replaceAll(' ', '-')}`}>
              {title}
            </a>
          ))}
        </nav>
        <p role='status' className='text-xs text-muted-foreground'>
          {lastAction ? `${lastAction} (preview only).` : 'All actions stay in this preview.'}
        </p>
      </div>
      <div key={version} className='grid min-w-0 gap-8'>
        <Section title='Questions and approvals'>
          {PROMPTS.map(({ label, prompt, readOnly }) => (
            <Example key={label} label={label}>
              <SessionChatInteractiveCard
                prompt={prompt}
                canSend={!readOnly}
                onAnswer={async () => act(label)}
                onInterrupt={() => setLastAction(`${label}: dismissed`)}
                onSwitchToTerminal={() => setLastAction('Open terminal')}
              />
            </Example>
          ))}
        </Section>
        <Section title='Claude pickers'>
          {PICKER_EXAMPLES.map((notice, index) => (
            <Example key={notice.title} label={notice.title}>
              <NoticeExample notice={notice} act={act} />
              {index === 0 ? (
                <Textarea
                  aria-label='Resume picker draft'
                  className='rounded-lg'
                  placeholder='Keep typing here. Use Command+Enter (Control+Enter on Windows/Linux) for the left action, or Escape for the second.'
                />
              ) : null}
            </Example>
          ))}
          <Example label='Resume picker, input held elsewhere'>
            <NoticeExample notice={PICKER_EXAMPLES[0]} act={act} readOnly />
          </Example>
        </Section>
        <Section title='Terminal menus and forms'>
          {DIALOG_EXAMPLES.map((notice) => (
            <Example key={notice.title} label={notice.title}>
              <NoticeExample notice={notice} act={act} />
            </Example>
          ))}
          <Example label='Terminal text form, input held elsewhere'>
            <NoticeExample
              notice={DIALOG_EXAMPLES.find((notice) => notice.title === 'Rename thread')!}
              act={act}
              readOnly
            />
          </Example>
        </Section>
        <Section title='Input and delivery states'>
          <Example label='Composer not ready, expandable terminal excerpt'>
            <SessionChatComposerNotReadyNotice
              reason='The agent is waiting for setup to finish.'
              onOpenTerminal={() => setLastAction('Open terminal')}
              onReadTerminalTail={async () => {
                await act('Read terminal');
                return {
                  agentId: 'codex',
                  projectId: 'P1gallery',
                  sessionId: 'G1preview',
                  captured: true,
                  composerState: 'notReady',
                  reason: 'Setup is waiting for input.',
                  lines: ['Setup is waiting for input.', 'Complete setup to continue.'],
                };
              }}
            />
          </Example>
          <Example label='Composer not ready, terminal action only'>
            <SessionChatComposerNotReadyNotice
              reason='Finish the terminal dialog before sending.'
              onOpenTerminal={() => setLastAction('Open terminal')}
            />
          </Example>
          {DELIVERY_NOTICES.map((notice) => (
            <Example key={notice.kind} label={notice.kind}>
              <NoticeExample notice={notice} act={act} />
            </Example>
          ))}
          <Example label='Notice with actions, input held elsewhere'>
            <NoticeExample
              notice={
                DETECTED_NOTICE_EXAMPLES.find(({ notice }) => notice.title === 'Claude Code is waiting to continue')!
                  .notice
              }
              act={act}
              readOnly
            />
          </Example>
        </Section>
        <Section title='Agent activity'>
          <Example label='Task list with running, blocked and completed rows'>
            <SessionChatAgentTasksPanel
              tasks={{
                tasks: [
                  { id: '1', subject: 'Inventory card layouts', status: 'completed' },
                  { id: '2', subject: 'Collect examples', status: 'completed' },
                  {
                    id: '3',
                    subject: 'Review the gallery',
                    activeForm: 'Reviewing the gallery',
                    status: 'in_progress',
                  },
                  { id: '4', subject: 'Unify the styles', status: 'pending', blockedBy: ['3'] },
                ],
              }}
            />
          </Example>
          <Example label='Active sub-agents'>
            <SessionChatAgentFleetStrip
              fleet={{
                detectedAt: AT,
                agents: [
                  {
                    name: 'general-purpose',
                    task: 'Reviewing card layouts',
                    elapsedSeconds: 120,
                    tokens: '↓ 12k tokens',
                  },
                  { name: 'explore', task: 'Checking the shared controls', elapsedSeconds: 45, nested: 2 },
                ],
              }}
            />
          </Example>
          <Example label='Compaction progress'>
            <SessionChatActivityRow
              activity={{
                kind: 'compacting',
                label: 'Compacting conversation',
                detectedAt: AT,
                percent: 49,
                elapsedSeconds: 60,
              }}
            />
          </Example>
          <Example label='Background monitors'>
            <SessionChatActivityRow
              activity={{ kind: 'shells-running', label: '2 monitors still running', detectedAt: AT }}
            />
          </Example>
        </Section>
        {groups.map((group) => (
          <Section key={group} title={group}>
            {DETECTED_NOTICE_EXAMPLES.filter((entry) => entry.group === group).map(({ notice }, index) => (
              <Example key={`${notice.title}-${index}`} label={`${index + 1}. ${notice.kind} · ${notice.severity}`}>
                <NoticeExample notice={notice} act={act} />
              </Example>
            ))}
          </Section>
        ))}
      </div>
    </>
  );
}
