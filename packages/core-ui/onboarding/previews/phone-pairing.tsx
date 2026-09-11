import { useState, type PointerEvent } from 'react';
import { AgentLogo, GhostexLogo, Icon, Lights, QrCode, Spinner } from '../primitives';
import { box, useLoopClock, useReducedMotion } from '../stage';
import { DemoLink } from './welcome-demos';

/** Timeline of the looping pairing demo, in ms (the prototype's `F`, plus a hold on the paired phone). */
const T = {
  easy: 2000,
  toggle: 2400,
  prompt: 2700,
  allow: 4300,
  qr: 4500,
  scan: 5300,
  paired: 6800,
  end: 8400,
  loop: 12400,
};

const PHONE_SESSIONS: readonly (readonly [string, string, string, string])[] = [
  ['claude', 'Claude Code', 'run', 'Running'],
  ['codex', 'Codex', 'work', 'Working'],
  ['cursor', 'Cursor Agent', 'run', 'Running'],
  ['other', 'Other agents', 'run', '3 running'],
];

function PhoneHome({ notify, toast }: { notify: boolean; toast: (message: string) => void }) {
  return (
    <>
      {notify && (
        <div className='ph-banner'>
          <AgentLogo agentId='codex' size={16} />
          <span>
            <b>Codex needs you</b>Keep the global fixtures, or inline them?
          </span>
        </div>
      )}
      <div className='ph-head'>
        <GhostexLogo size={28} />
        <span>Ghostex</span>
        {!notify && <span className='ph-quiet'>Notifications off</span>}
        <span className={'ph-bell' + (notify ? '' : ' off')} aria-hidden='true'>
          <Icon n='bell' size={20} />
          <i />
        </span>
      </div>
      <div className='ph-label'>My sessions</div>
      <button type='button' className='ph-card' onClick={() => toast('Your computer · 18 ms · 4 sessions')}>
        <span className='ph-mi'>
          <Icon n='monitor' size={17} />
        </span>
        <span>
          <b>This computer</b>
          <em>4 sessions</em>
          <span className='ph-conn'>
            <i />
            Connected
          </span>
        </span>
        <Icon n='chevR' size={14} className='dim' />
      </button>
      <div className='ph-sub'>Active agents</div>
      <div className='ph-rows'>
        {PHONE_SESSIONS.map(([id, name, kind, label]) => (
          <div key={id} className='ph-row'>
            <AgentLogo agentId={id} size={20} />
            <span className='nm'>{name}</span>
            <span className={'ph-pill ' + kind}>
              <i />
              {label}
              <Icon n='chevR' size={10} sw={2.4} />
            </span>
          </div>
        ))}
      </div>
      <div className='ph-primary'>
        Resume session <Icon n='arrowR' size={16} />
      </div>
      <div className='ph-secondary'>
        View all agents <Icon n='chevR' size={14} />
      </div>
      <nav className='ph-tabs'>
        {(
          [
            ['home', 'Home', true],
            ['layers', 'Agents', false],
            ['folder', 'Sessions', false],
            ['grid', 'More', false],
          ] as const
        ).map(([icon, label, on]) => (
          <button key={label} type='button' tabIndex={-1} className={on ? 'on' : ''}>
            <Icon n={icon} size={19} />
            {label}
          </button>
        ))}
      </nav>
    </>
  );
}

function PhoneDemo({ t }: { t: number }) {
  if (t < T.easy) {
    const stage = t < 900 ? 'get' : t < 1600 ? 'loading' : 'open';
    return (
      <div className='pd'>
        <GhostexLogo size={64} glow />
        <b>Ghostex Mobile</b>
        <em>Your agents, in your pocket</em>
        <span className={'pd-get ' + stage}>
          {stage === 'get' ? 'Get' : stage === 'loading' ? <Spinner /> : 'Open'}
        </span>
        <span className='pd-note'>Android or iPhone</span>
      </div>
    );
  }
  if (t < T.scan) {
    return (
      <div className='pd pd-connect'>
        <div className='ph-head'>
          <GhostexLogo size={28} />
          <span>Ghostex</span>
        </div>
        <div className='pd-empty'>
          <Icon n='monitor' size={34} />
          <b>Connect your computer</b>
          <em>Scan the code on your computer to pair.</em>
          <span className={'pd-btn' + (t >= T.qr + 400 ? ' tap' : '')}>
            <Icon n='target' size={15} />
            Scan code
          </span>
        </div>
      </div>
    );
  }
  if (t < T.paired) {
    return (
      <div className='pd pd-cam'>
        <div className='pd-cam-frame'>
          <QrCode size={124} />
          <span className='pd-scanline' />
        </div>
        <em>Point at the code on your computer</em>
      </div>
    );
  }
  return (
    <div className='pd'>
      <span className='pd-ok'>
        <Icon n='check' size={30} sw={2.4} />
      </span>
      <b>Paired</b>
      <em>Connected to this computer · 4 sessions</em>
    </div>
  );
}

function ComputerCard({ t, notify }: { t: number; notify: boolean }) {
  const toggled = t >= T.toggle;
  const allowed = t >= T.allow;
  const qrShown = t >= T.qr;
  const paired = t >= T.paired;
  const done = t >= T.end;
  const typed = Math.max(0, Math.min(8, Math.floor((t - T.prompt - 300) / 140)));
  return (
    <div className='glass pc' style={box(806, 246, 420, 448)}>
      <div className='pc-bar'>
        <Lights small />
        <span className='dim'>Settings</span>
        <b>Remote</b>
        {done && !notify && <span className='pc-quiet'>Notifications off</span>}
        <span className='pc-host mono'>This computer</span>
      </div>
      <div className='pc-body'>
        <div className='pc-t'>Connect your phone</div>
        <div className='pc-s'>Easy Connect is the simplest way to reach this computer from your phone.</div>
        <div className='pc-row'>
          <b>Easy Connect</b>
          <span className='pc-rec'>Recommended</span>
          <span className={'tg tg-sm' + (toggled ? ' on' : '')} aria-hidden='true' />
        </div>
        <div className='pc-row sub'>
          <span>Remote access</span>
          <span className={allowed ? 'ok mono' : 'dim mono'}>{allowed ? 'on' : 'off'}</span>
        </div>
        <div className='pc-qr'>
          {qrShown ? (
            <QrCode size={128} />
          ) : (
            <div className='pc-qr-off'>
              <Icon n='lock' size={22} />
            </div>
          )}
          <div className='pc-qr-t'>
            {paired ? (
              <span className='ok pc-paired'>
                <Icon n='checkCircle' size={16} /> Paired with your phone
              </span>
            ) : qrShown ? (
              'Scan it in the Ghostex app.'
            ) : (
              'Turn on Easy Connect to show the code.'
            )}
          </div>
        </div>
      </div>
      {t >= T.prompt && t < T.allow && (
        <div className='pc-sheet'>
          <Icon n='lock' size={22} />
          <b>Ghostex wants to turn on remote access.</b>
          <em>Enter your password to allow this.</em>
          <span className='pc-pw mono'>
            {'•'.repeat(typed)}
            <i className='pc-caret' />
          </span>
          <span className={'pc-allow' + (t > T.allow - 400 ? ' tap' : '')}>Allow</span>
        </div>
      )}
    </div>
  );
}

function DesktopNotification() {
  return (
    <div className='pc-notif' role='status' style={box(926, 178, 300)}>
      <AgentLogo agentId='codex' size={20} />
      <span className='pc-notif-t'>
        <span className='pc-notif-h'>
          <b>Codex needs you</b>
          <em>now</em>
        </span>
        Keep the global fixtures, or inline them?
      </span>
    </div>
  );
}

/** The right column of the Mobile panel: a looping Easy Connect pairing demo with the phone mockup. */
export function PairingPreview({ notify, toast }: { notify: boolean; toast: (message: string) => void }) {
  const reduced = useReducedMotion();
  const t = useLoopClock(T.loop);
  const [tilt, setTilt] = useState({ x: 0, y: 0 });
  const paired = t >= T.end;
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (reduced) return;
    const rect = event.currentTarget.getBoundingClientRect();
    setTilt({
      x: ((event.clientY - rect.top) / rect.height - 0.5) * -5,
      y: ((event.clientX - rect.left) / rect.width - 0.5) * 8,
    });
  };
  return (
    <>
      <ComputerCard t={t} notify={notify} />
      {paired && notify && <DesktopNotification />}
      <DemoLink
        x={1226}
        y={470}
        w={90}
        send={t >= T.scan && t < T.scan + 1000 ? 'l' : t >= T.paired && t < T.paired + 1000 ? 'r' : null}
        label={t >= T.scan && t < T.paired ? 'Scanning' : t >= T.paired ? 'Paired' : null}
      />
      <div className='devices' onPointerMove={onPointerMove} onPointerLeave={() => setTilt({ x: 0, y: 0 })}>
        <div
          className='phone'
          style={{
            transform: `perspective(1400px) rotateY(${-6 + tilt.y}deg) rotateX(${2 + tilt.x}deg) rotateZ(1deg)`,
          }}
        >
          <span className='ph-island' />
          <div className='ph-status'>
            <span>9:41</span>
            <span className='ph-sig'>
              <Icon n='signal' size={14} sw={2} />
              <Icon n='wifi' size={14} sw={2} />
              <span className='batt' />
            </span>
          </div>
          {paired ? <PhoneHome notify={notify} toast={toast} /> : <PhoneDemo t={t} />}
        </div>
      </div>
    </>
  );
}
