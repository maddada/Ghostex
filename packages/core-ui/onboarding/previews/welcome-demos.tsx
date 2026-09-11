import type { CSSProperties, ReactNode } from 'react';
import { AgentLogo, GhostexLogo, Icon, Lights, StatusPill, type StatusPillKind } from '../primitives';
import { box, useCycle, useReducedMotion, STAGE_HEIGHT, STAGE_WIDTH } from '../stage';

type Status = readonly [StatusPillKind, string];

/** A horizontal link between two cards with a travelling packet and a label (`zi`). */
export function DemoLink({
  x,
  y,
  w,
  send,
  label,
}: {
  x: number;
  y: number;
  w: number;
  send: string | null;
  label: string | null;
}) {
  return (
    <div className='dlink' style={box(x, y - 1, w, 2)}>
      <i className='dlink-end l' />
      <i className='dlink-end r' />
      {send && <span key={send} className={'dlink-pkt ' + send[0]} />}
      {label && (
        <span key={label} className='dlink-label'>
          {label}
        </span>
      )}
    </div>
  );
}

type LogLine = readonly [number, string, string];

function DemoCard({
  id,
  name,
  role,
  style,
  status,
  log,
  frame,
  lit,
}: {
  id: string;
  name: string;
  role: string;
  style: CSSProperties;
  status: Status;
  log: readonly LogLine[];
  frame: number;
  lit: boolean;
}) {
  return (
    <div className={'dcard' + (lit ? ' lit' : '')} style={style}>
      <div className='dcard-h'>
        <span className='abox sm'>
          <AgentLogo agentId={id} size={24} />
        </span>
        <span className='dcard-t'>
          <b>{name}</b>
          <em>{role}</em>
        </span>
        <StatusPill kind={status[0]}>{status[1]}</StatusPill>
      </div>
      <div className='dcard-log'>
        {log
          .filter(([at]) => at <= frame)
          .map(([at, cls, text]) => (
            <div key={text} className={'dl ' + cls + (at === frame ? ' fresh' : '')}>
              {text}
            </div>
          ))}
      </div>
    </div>
  );
}

const LEAD_LOG: readonly LogLine[] = [
  [0, 'me', 'Refactor the auth flow.'],
  [1, '', 'Edited src/auth/refresh.ts'],
  [2, 'acc', 'Handing the tests to Codex'],
  [6, 'ok', "Codex's tests pass. Ready to merge."],
];
const SUB_LOG: readonly LogLine[] = [
  [3, '', 'Got it: tests for refresh.ts'],
  [4, '', 'Wrote 6 tests'],
  [5, 'ok', '6 of 6 pass'],
];

export function AgentsTogetherDemo() {
  const frame = useCycle(7, 1150);
  const handing = frame >= 2 && frame < 5;
  return (
    <>
      <DemoCard
        id='claude'
        name='Claude Code'
        role='Lead agent'
        style={box(812, 300, 300, 320)}
        status={frame >= 6 ? ['done', 'Done'] : ['run', 'Working']}
        log={LEAD_LOG}
        frame={frame}
        lit={frame < 3 || frame >= 6}
      />
      <DemoLink
        x={1112}
        y={460}
        w={206}
        send={frame === 2 ? 'r2' : frame === 5 ? 'l5' : null}
        label={handing ? 'Write tests for refresh.ts' : frame >= 5 ? 'Tests pass' : null}
      />
      <DemoCard
        id='codex'
        name='Codex'
        role={frame >= 3 ? 'Started by Claude' : 'Sub-agent'}
        style={box(1318, 300, 300, 320)}
        status={frame >= 5 ? ['done', 'Done'] : frame >= 3 ? ['run', 'Working'] : ['idle', 'Waiting']}
        log={SUB_LOG}
        frame={frame}
        lit={frame >= 3 && frame < 6}
      />
    </>
  );
}

function wirePath(x1: number, y1: number, x2: number, y2: number, radius = 16): string {
  if (Math.abs(x2 - x1) < 1) return `M${x1} ${y1}V${y2}`;
  const midY = (y1 + y2) / 2;
  const dx = Math.sign(x2 - x1);
  const dy = Math.sign(y2 - y1);
  const r = Math.min(radius, Math.abs(x2 - x1) / 2, Math.abs(midY - y1));
  return `M${x1} ${y1}V${midY - dy * r}Q${x1} ${midY} ${x1 + dx * r} ${midY}H${x2 - dx * r}Q${x2} ${midY} ${x2} ${midY + dy * r}V${y2}`;
}

function Wire({
  d,
  active = false,
  ends = [],
  delay = 0,
}: {
  d: string;
  active?: boolean;
  ends?: readonly (readonly [number, number])[];
  delay?: number;
}) {
  const reduced = useReducedMotion();
  return (
    <g className={'wire' + (active ? ' active' : '')}>
      <path d={d} className='wire-glow' />
      <path d={d} className='wire-line' />
      {ends.map(([cx, cy], index) => (
        <circle key={index} cx={cx} cy={cy} r='3.4' className='wire-dot' />
      ))}
      {!reduced && (
        <circle r={active ? 3.2 : 2.3} className='wire-pulse'>
          <animateMotion dur={(active ? 1.15 : 3.6) + 's'} begin={delay + 's'} repeatCount='indefinite' path={d} />
        </circle>
      )}
      {active && !reduced && (
        <circle r='2.6' className='wire-pulse'>
          <animateMotion dur='1.15s' begin='0.57s' repeatCount='indefinite' path={d} />
        </circle>
      )}
    </g>
  );
}

function Wires({ children }: { children: ReactNode }) {
  return (
    <svg
      className='wires'
      width={STAGE_WIDTH}
      height={STAGE_HEIGHT}
      viewBox={`0 0 ${STAGE_WIDTH} ${STAGE_HEIGHT}`}
      aria-hidden='true'
    >
      {children}
    </svg>
  );
}

type TranscriptPair = readonly [readonly [string, string], readonly [string, string] | null];

const TRANSCRIPT: readonly TranscriptPair[] = [
  [['cmd', '$ claude'], null],
  [
    ['me', '> Fix the flaky refresh test'],
    ['me', 'Fix the flaky refresh test'],
  ],
  [
    ['tool', '● Read src/auth/refresh.ts'],
    ['tool', 'Read refresh.ts'],
  ],
  [
    ['tool', '● Update src/auth/refresh.ts  +12 −4'],
    ['tool', 'Edited refresh.ts · +12 −4'],
  ],
  [
    ['tool', '● Bash bun test  17 passed'],
    ['tool', 'Ran the tests · 17 passed'],
  ],
  [
    ['out', 'Fixed: the refresh now waits for the lock.'],
    ['bot', 'Fixed. The refresh now waits for the lock, and all 17 tests pass.'],
  ],
];

export function ChatTerminalDemo() {
  const frame = useCycle(TRANSCRIPT.length, 1150);
  const shown = TRANSCRIPT.slice(0, frame + 1);
  return (
    <>
      <Wires>
        <Wire d={wirePath(1215, 196, 1000, 250)} ends={[[1000, 250]]} active />
        <Wire d={wirePath(1215, 196, 1430, 250)} ends={[[1430, 250]]} active delay={0.5} />
      </Wires>
      <div className='dsess' style={box(1095, 158, 240, 38)}>
        <span className='dot run' />
        refactor-auth · one session
      </div>
      <div className='dpane' style={box(810, 250, 380, 440)}>
        <div className='dpane-h'>
          <Icon n='terminal' size={16} />
          Terminal
        </div>
        <div className='dpane-b mono'>
          {shown.map(([line], index) => (
            <div key={index} className={'tline ' + line[0] + (index === frame ? ' fresh' : '')}>
              {line[1]}
            </div>
          ))}
          <span className='caret sm' />
        </div>
      </div>
      <div className='dpane' style={box(1240, 250, 380, 440)}>
        <div className='dpane-h'>
          <Icon n='chat' size={16} />
          Chat view
        </div>
        <div className='dpane-b'>
          {shown.map(([, line], index) =>
            line ? (
              <div key={index} className={'cline ' + line[0] + (index === frame ? ' fresh' : '')}>
                {line[0] === 'bot' && (
                  <span className='who'>
                    <AgentLogo agentId='claude' size={13} /> Claude Code
                  </span>
                )}
                {line[0] === 'tool' && <Icon n='check' size={13} sw={2.2} />}
                {line[1]}
              </div>
            ) : null
          )}
        </div>
      </div>
    </>
  );
}

const MONITOR_SESSIONS: readonly (readonly [string, string])[] = [
  ['claude', 'refactor-auth'],
  ['codex', 'port-legacy-tests'],
  ['cursor', 'tokens-to-css-vars'],
];

export function DesktopMobileDemo() {
  const frame = useCycle(6, 1150);
  const codexStatus: Status =
    frame >= 5
      ? ['done', 'Done']
      : frame >= 4
        ? ['run', 'Working']
        : frame >= 1
          ? ['need', 'Needs you']
          : ['run', 'Working'];
  const statusFor = (id: string): Status => (id === 'codex' ? codexStatus : ['run', 'Working']);
  return (
    <>
      <div className='dmon' style={box(800, 262, 450, 312)}>
        <div className='dmon-bar'>
          <Lights small />
          <GhostexLogo size={18} />
          <b>orbit-api</b>
        </div>
        <div className='dmon-body'>
          {MONITOR_SESSIONS.map(([id, name]) => (
            <div key={name} className={'dmon-row' + (id === 'codex' && frame >= 1 ? ' hot' : '')}>
              <AgentLogo agentId={id} size={16} />
              <span className='mono'>{name}</span>
              <StatusPill kind={statusFor(id)[0]}>{statusFor(id)[1]}</StatusPill>
            </div>
          ))}
          <div key={frame >= 4 ? 'r' : 'q'} className='dmon-detail'>
            {frame >= 1 && frame < 4 && (
              <>
                <AgentLogo agentId='codex' size={14} /> Keep the global fixtures, or inline them?
              </>
            )}
            {frame >= 4 && (
              <>
                <span className='ok'>Replied from your phone:</span> Inline them
              </>
            )}
          </div>
        </div>
      </div>
      <div className='dmon-neck' style={box(990, 574, 70, 30)} />
      <div className='dmon-foot' style={box(945, 603, 160, 7)} />
      <div className='dcap' style={box(800, 628, 450, 20)}>
        Your computer keeps the work
      </div>
      <DemoLink
        x={1250}
        y={440}
        w={170}
        send={frame === 1 ? 'r1' : frame === 4 ? 'l4' : null}
        label={frame === 1 || frame === 2 ? 'Codex needs you' : frame === 3 || frame === 4 ? 'Inline them' : 'Live'}
      />
      <div className='dphone' style={box(1420, 236, 196, 404)}>
        <span className='dphone-island' />
        {(frame === 1 || frame === 2) && (
          <div className='dphone-banner'>
            <AgentLogo agentId='codex' size={13} />
            <span>
              <b>Codex needs you</b>Keep global, or inline?
            </span>
          </div>
        )}
        <div className='dphone-h'>
          <GhostexLogo size={16} /> Ghostex
        </div>
        {frame < 2 ? (
          <div className='dphone-rows'>
            {MONITOR_SESSIONS.map(([id, name]) => (
              <div key={name} className='dphone-row'>
                <AgentLogo agentId={id} size={13} />
                <span className='mono'>{name}</span>
                <StatusPill kind={statusFor(id)[0]}>{statusFor(id)[1]}</StatusPill>
              </div>
            ))}
          </div>
        ) : (
          <div className='dphone-chat'>
            <div className='dpc bot'>Keep the global fixtures, or inline them?</div>
            <div className='dpc-quick'>
              <span>Keep global</span>
              <span className={frame === 3 ? 'tap' : frame > 3 ? 'picked' : ''}>Inline them</span>
            </div>
            {frame >= 3 && <div className='dpc me'>Inline them</div>}
            {frame >= 5 && <div className='dpc bot'>Inlined 12 fixtures. Tests pass.</div>}
          </div>
        )}
      </div>
      <div className='dcap' style={box(1420, 656, 196, 20)}>
        Your phone steers it
      </div>
    </>
  );
}
