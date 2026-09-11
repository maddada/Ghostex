import type { ReactNode } from 'react';
import type { OnboardingComputerUseState } from '../contract';
import { AgentLogo, Icon, Lights, StatusPill, type StatusPillKind } from '../primitives';
import { box, useCycle } from '../stage';

export function ExtensionPanel({
  lead,
  terms,
  children,
}: {
  lead: string;
  terms: readonly (readonly [string, string])[];
  children: ReactNode;
}) {
  return (
    <div className='glass xpanel' style={box(885, 174, 700, 586)}>
      <p className='xp-lead'>{lead}</p>
      <div className='xp-stage'>{children}</div>
      <div className='xp-terms'>
        {terms.map(([term, text]) => (
          <div key={term} className='xp-row'>
            <em>{term}</em>
            <span>{text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export const INTEGRATION_LEAD =
  "Ghostex adds a small helper to each agent's own settings, so it can show what every agent is doing.";
export const INTEGRATION_TERMS: readonly (readonly [string, string])[] = [
  ["What's added", "A small Ghostex helper in each agent's own settings."],
  ['What you get', 'Live status, alerts, session names, the chat view and resume.'],
  ['Your agent', 'Still runs as its normal CLI, and nothing goes through a Ghostex cloud.'],
  ['To remove it', 'Settings → Agents, one agent or all at once.'],
];
export const COMPUTER_USE_LEAD =
  'This hands over the whole machine: supported agents can see your screen and drive apps outside Ghostex.';
export const COMPUTER_USE_TERMS: readonly (readonly [string, string])[] = [
  ["What's added", 'A Computer Use skill for your agents and a small helper app.'],
  ['What agents can do', 'Click, type and read other apps when a task needs it.'],
  ['To turn it off', 'Settings → Integrations, any time.'],
];

const INTEGRATION_SESSIONS: readonly (readonly [string, string, string])[] = [
  ['claude', 'claude', 'refactor-auth'],
  ['codex', 'codex', 'port-tests'],
  ['cursor', 'cursor-agent', 'css-tokens'],
];
const HELPER_FILES: readonly (readonly [string, string])[] = [
  ['claude', '~/.claude/settings.json'],
  ['codex', '~/.codex/hooks.json'],
  ['cursor', '~/.cursor/hooks.json'],
];

export function IntegrationPreview({ on }: { on: boolean }) {
  const frame = useCycle(4, 1300);
  const status: Record<string, readonly [StatusPillKind, string]> = {
    claude: frame >= 2 ? ['done', 'Done'] : ['run', 'Working'],
    codex: frame === 1 || frame === 2 ? ['need', 'Needs you'] : ['run', 'Working'],
    cursor: ['run', 'Working'],
  };
  return (
    <div className='pv-int'>
      {[false, true].map((withIntegration) => (
        <div key={withIntegration ? 'with' : 'without'} className={'pv-col' + (withIntegration === on ? ' lit' : '')}>
          <div className='pv-col-h'>
            <b>{withIntegration ? 'With the integration' : 'Without it'}</b>
            {withIntegration === on && <span className='pv-now'>Your choice</span>}
          </div>
          <div className='pv-slot'>
            {!withIntegration && <div className='pv-alert none'>No alerts: you only find out when you look.</div>}
            {withIntegration && (frame === 1 || frame === 2) && (
              <div className='pv-alert'>
                <AgentLogo agentId='codex' size={14} />
                <span>
                  <b>Codex needs you</b>Keep the global fixtures, or inline them?
                </span>
              </div>
            )}
          </div>
          {INTEGRATION_SESSIONS.map(([id, command, name]) => (
            <div key={id} className='pv-srow'>
              {withIntegration ? <AgentLogo agentId={id} size={16} /> : <Icon n='terminal' size={16} />}
              <span className='mono'>{withIntegration ? name : command}</span>
              {withIntegration ? (
                <StatusPill kind={status[id][0]}>{status[id][1]}</StatusPill>
              ) : (
                <StatusPill kind='idle'>No status</StatusPill>
              )}
            </div>
          ))}
          <p className='pv-cap'>
            {withIntegration
              ? 'Named after the task, live, and it tells you when an agent is waiting.'
              : 'Plain terminals. You check each one yourself.'}
          </p>
        </div>
      ))}
      <div className='pv-files'>
        <div className='label'>Where the helper goes</div>
        {HELPER_FILES.map(([id, file]) => (
          <div key={id} className='pv-file'>
            <AgentLogo agentId={id} size={14} />
            {file}
          </div>
        ))}
        <div className='pv-file dim'>…and the matching settings file of each other agent you connect.</div>
      </div>
    </div>
  );
}

const CURSOR_PATH: readonly (readonly [number, number])[] = [
  [500, 230],
  [70, 76],
  [70, 76],
  [46, 190],
  [46, 190],
  [500, 230],
];
const CALENDAR_DAYS: readonly (readonly [string, readonly string[]])[] = [
  ['WED', ['Design sync']],
  ['THU', ['Standup']],
  ['FRI', []],
];

function computerUseCaption(state: OnboardingComputerUseState): string {
  switch (state) {
    case 'on':
      return 'Claude Code is using Calendar';
    case 'installing':
      return 'Installing Computer Use…';
    case 'permissions':
      return 'Waiting for your permission';
    default:
      return 'What Claude Code could do in Calendar';
  }
}

export function ComputerUsePreview({
  state,
  onOpenAccessibilityPreferences,
  onOpenScreenRecordingPreferences,
}: {
  state: OnboardingComputerUseState;
  onOpenAccessibilityPreferences?: () => void;
  onOpenScreenRecordingPreferences?: () => void;
}) {
  const frame = useCycle(CURSOR_PATH.length, 1100, 2);
  const [cursorX, cursorY] = CURSOR_PATH[frame];
  const askingPermission = state === 'permissions';
  const permissionRow = (icon: 'shield' | 'monitor', name: string, why: string, open?: () => void) =>
    askingPermission && open ? (
      <button type='button' className='pv-perm act' onClick={open}>
        <Icon n={icon} size={16} />
        <b>{name}</b>
        <em>{why}</em>
        <span className='pv-open'>Open</span>
      </button>
    ) : (
      <div className='pv-perm'>
        <Icon n={icon} size={16} />
        <b>{name}</b>
        <em>{why}</em>
      </div>
    );
  return (
    <div className='pv-cu'>
      <span className='pv-cu-who'>
        <AgentLogo agentId='claude' size={14} />
        {computerUseCaption(state)}
      </span>
      <div className='pv-app'>
        <div className='pv-app-bar'>
          <Lights small />
          Calendar
        </div>
        <span className='pv-lbl' style={{ left: 20, top: 48 }}>
          Title
        </span>
        <span className={'pv-field' + (frame === 1 || frame === 2 ? ' focus' : '')} style={{ top: 64 }}>
          {frame >= 2 && <span className='pv-typed'>Release review</span>}
        </span>
        <span className='pv-lbl' style={{ left: 20, top: 112 }}>
          When
        </span>
        <span className='pv-field' style={{ top: 128 }}>
          Friday · 15:00
        </span>
        <span className={'pv-save' + (frame === 4 ? ' tap' : '')}>Save</span>
        <div className='pv-cal'>
          {CALENDAR_DAYS.map(([day, events]) => (
            <div key={day} className='pv-day'>
              <b>{day}</b>
              {events.map((event) => (
                <span key={event} className='pv-ev'>
                  {event}
                </span>
              ))}
              {day === 'FRI' && frame >= 4 && <span className='pv-ev new'>Release review · 15:00</span>}
            </div>
          ))}
        </div>
        <span
          className={'pv-cursor' + (frame === 1 || frame === 4 ? ' click' : '')}
          style={{ transform: `translate(${cursorX}px,${cursorY}px)` }}
        />
      </div>
      <div className='pv-perms'>
        <div className='label'>{askingPermission ? 'Your computer is asking for' : 'Your computer asks once for'}</div>
        {permissionRow('shield', 'Accessibility', 'so agents can click and type', onOpenAccessibilityPreferences)}
        {permissionRow('monitor', 'Screen Recording', 'so agents can see the screen', onOpenScreenRecordingPreferences)}
      </div>
    </div>
  );
}
