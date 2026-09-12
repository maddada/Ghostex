import { AccountSwitchCard } from './account-switch-card';
import type { AgentAccount } from '@/packages/shared/agent-accounts';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { useEffect, useRef, useState } from 'react';
import { IconCheck, IconChevronDown, IconChevronRight, IconPlayerPlay, IconX } from '@tabler/icons-react';
import { Button } from '@/packages/components/ui/button';
import { SegmentedControl, SegmentedControlItem } from '@/packages/components/ui/segmented-control';
import { AGENT_LOGOS } from '../agent-logos';
import { SessionChatComposer, type SessionChatComposerHandle } from '../chat/session-chat-composer';
import './account-switch-preview.css';

type Phase = 'switching' | 'resuming' | 'continuing' | 'success' | 'failed' | 'complete';
type Mode = 'automatic' | 'manual';
type Provider = 'claude' | 'codex';
type PreviewProps = { initialPhase: Phase; initialMode: Mode; provider: Provider; narrow: boolean };
const phaseText: Record<Phase, string> = {
  switching: 'Switching account',
  resuming: 'Resuming conversation',
  continuing: 'Continuing session',
  success: 'Account switched',
  failed: 'Switch needs attention',
  complete: 'Back to chat',
};

function AgentMark({ provider, account }: { provider: Provider; account?: string }) {
  return (
    <span className='as-preview-agent-mark' aria-hidden='true'>
      <span
        style={{ maskImage: `url("${AGENT_LOGOS[provider]}")`, WebkitMaskImage: `url("${AGENT_LOGOS[provider]}")` }}
      />
      {account && <b>{account}</b>}
    </span>
  );
}

function AccountSwitchPreview({ initialPhase, initialMode, provider, narrow }: PreviewProps) {
  const [phase, setPhase] = useState(initialPhase);
  const [mode, setMode] = useState(initialMode);
  const [playing, setPlaying] = useState(false);
  const [terminal, setTerminal] = useState(false);
  const target = '70';
  const [sent, setSent] = useState('');
  const [noteOpen, setNoteOpen] = useState(false);
  const [summary, setSummary] = useState(false);
  const [draftSaved, setDraftSaved] = useState(false);
  const composer = useRef<SessionChatComposerHandle>(null);
  const providerName = provider === 'claude' ? 'Claude' : 'Codex';
  const steps: Phase[] =
    mode === 'automatic'
      ? ['switching', 'resuming', 'continuing', 'success', 'complete']
      : ['switching', 'resuming', 'success', 'complete'];
  const settled = phase === 'success' || phase === 'complete';
  const busy = !settled && phase !== 'failed';
  const verified = settled || phase === 'continuing';
  const activeAccount = verified ? target : '71';
  const targetLabel = 'x•••0@•••••.•••';
  const targetEmail = 'x•••0@•••••.•••';
  const sourceUsage = mode === 'automatic' ? 100 : 58;
  const targetUsage = 19;

  useEffect(() => {
    if (!playing || phase === 'complete' || phase === 'failed') return;
    const timer = window.setTimeout(
      () => {
        const next = steps[steps.indexOf(phase) + 1] ?? 'complete';
        setPhase(next);
        if (next === 'complete') setPlaying(false);
      },
      phase === 'success' ? 1800 : 2200
    );
    return () => window.clearTimeout(timer);
  }, [playing, phase, mode]);

  const start = () => {
    setPhase('switching');
    setPlaying(true);
    setTerminal(false);
    setSent('');
  };
  const changeMode = (next: string) => {
    setMode(next as Mode);
    setPhase('switching');
    setPlaying(false);
    setTerminal(false);
  };

  return (
    <main className='as-preview ghostex-session-chat-scope dark' data-chat-theme='dark' data-narrow={narrow}>
      <header className='as-preview-controls'>
        <span className='as-preview-label'>
          Account switching <span>Design preview</span>
        </span>
        <div className='as-preview-controls-actions'>
          <SegmentedControl size='sm' value={mode} onValueChange={changeMode} aria-label='Switch trigger'>
            <SegmentedControlItem value='automatic'>Automatic</SegmentedControlItem>
            <SegmentedControlItem value='manual'>Manual</SegmentedControlItem>
          </SegmentedControl>
          <Button size='sm' variant='outline' onClick={start}>
            <IconPlayerPlay />
            {playing ? 'Replay' : 'Play flow'}
          </Button>
          <Button
            size='sm'
            variant='ghost'
            aria-label='Next preview state'
            onClick={() => {
              setPlaying(false);
              setPhase(steps[(steps.indexOf(phase) + 1) % steps.length]);
            }}
          >
            <IconChevronRight />
          </Button>
        </div>
      </header>

      <section className='as-preview-chat' aria-label={`${providerName} chat preview`}>
        <header className='as-preview-chat-heading'>
          <span>
            Ghostex <IconChevronRight size={13} />
            <strong>Auto-switch to agents view handoff</strong>
          </span>
        </header>

        <div className='as-preview-conversation'>
          <div className='as-preview-transcript'>
            <div className='as-preview-user-message'>
              Keep the current project view when I switch projects. Open Agents when I select a session.
            </div>
            <div className='as-preview-assistant-message'>
              <span className='as-preview-bullet' />
              <p>
                I found where selecting a project also changes its view. I’ll update the session handoff so each project
                keeps its place.
              </p>
            </div>
            <div className='as-preview-tool-row'>
              <IconChevronRight size={13} />
              <span>Read</span>
              <code>mode_switcher_and_titlebar.rs</code>
              <span className='as-preview-tool-count'>214 lines</span>
            </div>
          </div>

          <div className='as-preview-status-region'>
            {phase !== 'complete' ? (
              <AccountSwitchCard
                progress={{
                  id: 'preview',
                  provider,
                  source: mode,
                  phase,
                  fromAccountId: '71',
                  toAccountId: target,
                  updatedAt: new Date().toISOString(),
                }}
                accounts={[
                  {
                    id: '71',
                    selector: '71',
                    name: 'x•••1@•••••.•••',
                    email: 'x•••1@•••••.•••',
                    values: [sourceUsage, 82, 96],
                    resets: ['2h 14m', '3d 6h', '3d 6h'],
                  },
                  {
                    id: target,
                    selector: target,
                    name: targetLabel,
                    email: targetEmail,
                    values: [targetUsage, 29, 56],
                    resets: ['4h 32m', '5d 2h', '5d 2h'],
                  },
                ].map(({ values, resets, ...account }): AgentAccount => ({
                  ...account,
                  provider,
                  color: 'neutral',
                  eligible: true,
                  registered: true,
                  sharedHistory: true,
                  status: 'ready',
                  sessionCount: 1,
                  usage: ['fiveHour', 'sevenDay', ...(provider === 'claude' ? ['fable'] : [])].map((id, i) => ({
                    id,
                    label: id,
                    usedPercent: values[i],
                    ...(i === 2 ? { model: 'Fable' } : {}),
                    resetsAt: new Date(
                      Date.now() +
                        resets[i]
                          .split(' ')
                          .reduce(
                            (ms, part) =>
                              ms +
                              parseInt(part) * (part.endsWith('d') ? 86400000 : part.endsWith('h') ? 3600000 : 60000),
                            0
                          )
                    ).toISOString(),
                  })),
                }))}
                onRetry={start}
              />
            ) : (
              <div className='as-preview-returned' role='status'>
                {sent ? (
                  <div className='as-preview-user-message'>{sent}</div>
                ) : mode === 'automatic' ? (
                  <div className='as-preview-assistant-message'>
                    <span className='as-preview-bullet' />
                    <p>I’m continuing the session handoff change from where I left off.</p>
                  </div>
                ) : (
                  <span>
                    <IconCheck size={16} /> {targetLabel} account is ready for your next message.
                  </span>
                )}
              </div>
            )}
          </div>
        </div>

        <div className='as-preview-composer-area' data-switching={busy}>
          <SessionChatComposer
            ref={composer}
            isWorking={mode === 'automatic' && settled && !sent}
            sendOnEnter
            theme='dark'
            placeholder='Add a follow-up…'
            sendBlockedReason={!settled ? 'Wait for the account switch to complete. Your draft is kept.' : null}
            onSend={(text) => {
              setSent(text);
              setPhase('complete');
              setPlaying(false);
            }}
            onInterrupt={() => {
              setMode('manual');
              setPhase('complete');
            }}
            onSessionNote={() => setNoteOpen(!noteOpen)}
            sessionNoteActive={noteOpen}
            summaryMode={summary}
            onToggleSummary={() => setSummary(!summary)}
            onStash={() => setDraftSaved(true)}
            onPickPaths={async () => []}
            hostActions={{
              onSwitchToTerminal: () => setTerminal(!terminal),
            }}
            optionPills={
              <>
                <span className='as-preview-model'>
                  <AgentMark provider={provider} account={activeAccount} />
                  {provider === 'claude' ? 'Claude Fable 5' : 'GPT 6 Astra'}
                  <IconChevronDown size={14} />
                </span>
                <span className='as-preview-effort'>
                  Extra High <IconChevronDown size={14} />
                </span>
                <span className='as-preview-context' aria-label='Context 18 percent' />
              </>
            }
          />
          {terminal && (
            <div className='as-preview-terminal'>
              <span>Terminal preview</span>
              <code>
                {providerName} · {phaseText[phase]}
              </code>
            </div>
          )}
          {noteOpen && (
            <div className='as-preview-note'>
              <span>Session note</span>
              <textarea aria-label='Session note' placeholder='Add a note for this conversation…' />
              <Button size='icon-sm' variant='ghost' aria-label='Close note' onClick={() => setNoteOpen(false)}>
                <IconX />
              </Button>
            </div>
          )}
          {draftSaved && (
            <p className='as-preview-local-notice' role='status'>
              Draft kept in this preview. <button onClick={() => setDraftSaved(false)}>Dismiss</button>
            </p>
          )}
          <div className='as-preview-chat-meta'>
            <span>Auto-switch to agents view handoff</span>
            <i>◆</i>
            <span>{verified ? targetEmail : 'x•••1@•••••.•••'}</span>
            <i>◆</i>
            <span>5h limit: {verified ? targetUsage : sourceUsage}%</span>
          </div>
        </div>
      </section>
    </main>
  );
}

const meta = {
  title: 'Chat/Account switching',
  component: AccountSwitchPreview,
  parameters: { layout: 'fullscreen' },
  args: { initialPhase: 'switching', initialMode: 'automatic', provider: 'claude', narrow: false },
  argTypes: {
    initialPhase: {
      control: 'select',
      options: ['switching', 'resuming', 'continuing', 'success', 'failed', 'complete'],
    },
    initialMode: { control: 'inline-radio', options: ['automatic', 'manual'] },
    provider: { control: 'inline-radio', options: ['claude', 'codex'] },
    narrow: { control: 'boolean' },
  },
  render: (args) => (
    <AccountSwitchPreview key={`${args.initialMode}-${args.initialPhase}-${args.provider}-${args.narrow}`} {...args} />
  ),
} satisfies Meta<typeof AccountSwitchPreview>;
export default meta;
type Story = StoryObj<typeof meta>;
export const AutomaticSwitch: Story = { name: '01 Automatic switch' };
export const ResumingConversation: Story = { name: '02 Resuming conversation', args: { initialPhase: 'resuming' } };
export const ContinuingTask: Story = { name: '03 Continuing task', args: { initialPhase: 'continuing' } };
export const AutomaticSuccess: Story = { name: '04 Automatic success', args: { initialPhase: 'success' } };
export const ManualSwitch: Story = { name: '05 Manual switch', args: { initialMode: 'manual' } };
export const ManualSuccess: Story = {
  name: '06 Manual success',
  args: { initialMode: 'manual', initialPhase: 'success' },
};
export const SwitchFailed: Story = { name: '07 Switch failed', args: { initialPhase: 'failed' } };
export const CodexSwitch: Story = { name: '08 Codex switch', args: { provider: 'codex' } };
export const NarrowPane: Story = { name: '09 Narrow pane', args: { narrow: true } };
