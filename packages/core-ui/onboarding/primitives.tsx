import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from 'react';
import { AGENT_LOGO_COLORS, getBrandAgentLogoStyle } from '../agent-logos';
import { getSidebarAgentIconById } from '@/packages/shared/sidebar-agents';
import ghostexLogoUrl from './assets/ghostex-logo.png';
import { box, useOutsideClick } from './stage';

const ICON_PATHS = {
  terminal: '<path d="M5 7l5 5-5 5M12.5 18h6.5"/>',
  browser:
    '<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="M3 9h18"/><path d="M6.3 6.8h.01M8.8 6.8h.01" stroke-width="2.2"/>',
  phone: '<rect x="7" y="2.5" width="10" height="19" rx="2.6"/><path d="M11 18.6h2"/>',
  home: '<path d="M4 11.2 12 4l8 7.2v8.3a1 1 0 0 1-1 1h-4.6v-5.8H9.6v5.8H5a1 1 0 0 1-1-1z" fill="currentColor" stroke="none"/>',
  users:
    '<circle cx="9" cy="8.5" r="3.2"/><path d="M3.5 19c.6-3 2.8-4.8 5.5-4.8s4.9 1.8 5.5 4.8"/><circle cx="16.6" cy="9.4" r="2.5"/><path d="M15.7 14.3c2.3.1 4.1 1.8 4.6 4.7"/>',
  org: '<rect x="9.5" y="3" width="5" height="4.5" rx="1"/><rect x="3" y="15.5" width="5" height="4.5" rx="1"/><rect x="16" y="15.5" width="5" height="4.5" rx="1"/><rect x="9.5" y="15.5" width="5" height="4.5" rx="1"/><path d="M12 7.5v8M5.5 15.5v-3h13v3"/>',
  grid: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.2"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.2"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.2"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.2"/>',
  bell: '<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>',
  monitor: '<rect x="3" y="4" width="18" height="12.5" rx="1.8"/><path d="M9 20.5h6M12 16.5v4"/>',
  lock: '<rect x="5" y="10.5" width="14" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>',
  shield: '<path d="M12 3l7 3v5.5c0 4.5-3 8-7 9.5-4-1.5-7-5-7-9.5V6z"/>',
  refresh: '<path d="M19.5 11.5A7.5 7.5 0 0 0 6 7.3M5 4v4h4M4.5 12.5A7.5 7.5 0 0 0 18 16.7M19 20v-4h-4"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  checkCircle: '<circle cx="12" cy="12" r="9"/><path d="M8 12.3l2.8 2.8 5.5-5.6"/>',
  folder:
    '<path d="M3 7a1.5 1.5 0 0 1 1.5-1.5h4.3l2 2.2h8.7A1.5 1.5 0 0 1 21 9.2V18a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z"/>',
  chat: '<path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v8a2.5 2.5 0 0 1-2.5 2.5H10l-4.5 3.5V17A2.5 2.5 0 0 1 4 14.5z"/><path d="M8.5 10.5h.01M12 10.5h.01M15.5 10.5h.01" stroke-width="2.2"/>',
  arrowR: '<path d="M4 12h15M13 6l6 6-6 6"/>',
  arrowL: '<path d="M20 12H5M11 6l-6 6 6 6"/>',
  chevR: '<path d="M9 5l7 7-7 7"/>',
  chevL: '<path d="M15 5l-7 7 7 7"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  external: '<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>',
  code: '<path d="M8 7l-5 5 5 5M16 7l5 5-5 5M13.5 5l-3 14"/>',
  kanban: '<rect x="3.5" y="3.5" width="17" height="17" rx="2"/><rect x="7.5" y="8.5" width="6" height="8" rx=".6"/>',
  bolt: '<path d="M13 2.5 5 13.5h6l-1 8 8-11h-6z" fill="currentColor" stroke="none"/>',
  target: '<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.2"/>',
  list: '<path d="M5 7h14M5 12h14M5 17h14"/>',
  send: '<path d="M4 12 20 4l-6 16-2.5-6.5z" fill="currentColor" stroke="none"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>',
  signal: '<path d="M4 18v-2M8 18v-5M12 18V9M16 18V6" stroke-width="2.2"/>',
  wifi: '<path d="M3.5 9.5a12 12 0 0 1 17 0M6.5 12.8a7.5 7.5 0 0 1 11 0M9.6 16a3 3 0 0 1 4.8 0"/>',
  pulse: '<path d="M3 12h4l2-5 4 10 2-5h6"/>',
  layers: '<path d="M12 3 3 8l9 5 9-5z"/><path d="M3 12.5l9 5 9-5M3 16.5l9 5 9-5"/>',
  sparkle: '<path d="M12 3c.6 4.5 2.5 6.4 7 7-4.5.6-6.4 2.5-7 7-.6-4.5-2.5-6.4-7-7 4.5-.6 6.4-2.5 7-7z"/>',
  wrench:
    '<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>',
} as const;

export type IconName = keyof typeof ICON_PATHS;

export function Icon({
  n,
  size = 20,
  sw = 1.6,
  className,
  style,
}: {
  n: IconName;
  size?: number;
  sw?: number;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <svg
      className={'ic' + (className ? ' ' + className : '')}
      style={style}
      width={size}
      height={size}
      viewBox='0 0 24 24'
      fill='none'
      stroke='currentColor'
      strokeWidth={sw}
      strokeLinecap='round'
      strokeLinejoin='round'
      aria-hidden='true'
      dangerouslySetInnerHTML={{ __html: ICON_PATHS[n] }}
    />
  );
}

export function GhostexLogo({ size = 32, glow = false }: { size?: number; glow?: boolean }) {
  return (
    <img
      className={'glogo' + (glow ? ' glow' : '')}
      src={ghostexLogoUrl}
      width={size}
      height={size}
      alt='Ghostex'
      draggable={false}
    />
  );
}

/** The three agents the prototype shows inside the "Other agents" cluster. */
export const OTHER_AGENT_SAMPLE_IDS = ['gemini', 'opencode', 'pi'] as const;

/** Renders a sidebar agent logo by agent id, the "Other agents" cluster, or the terminal glyph. */
export function AgentLogo({ agentId, size = 24 }: { agentId: string; size?: number }) {
  if (agentId === 'other') return <OtherAgentsCluster size={size} />;
  if (agentId === 'terminal') return <Icon n='terminal' size={size} />;
  const icon = getSidebarAgentIconById(agentId);
  if (!icon) return <Icon n='sparkle' size={size} />;
  const dim = { width: size, height: size };
  const style = getBrandAgentLogoStyle(icon);
  const isMask = style.maskImage !== undefined;
  return (
    <span
      className={'aicon' + (isMask ? ' mask' : '')}
      style={isMask ? { ...dim, ...style, color: AGENT_LOGO_COLORS[icon] } : { ...dim, ...style }}
      aria-hidden='true'
    />
  );
}

function OtherAgentsCluster({ size = 24 }: { size?: number }) {
  const cell = Math.round(size * 0.47);
  return (
    <span className='olog' style={{ width: size, height: size }} aria-hidden='true'>
      {OTHER_AGENT_SAMPLE_IDS.map((id) => (
        <AgentLogo key={id} agentId={id} size={cell} />
      ))}
      <span className='olog-more' style={{ width: cell, height: cell, fontSize: Math.max(8, cell * 0.9) }}>
        +
      </span>
    </span>
  );
}

export function OtherAgentsStrip({ size = 20, more }: { size?: number; more: number }) {
  return (
    <span className='ostrip' aria-hidden='true'>
      {OTHER_AGENT_SAMPLE_IDS.map((id) => (
        <AgentLogo key={id} agentId={id} size={size} />
      ))}
      {more > 0 && <span className='ostrip-more'>+{more}</span>}
    </span>
  );
}

export function Toggle({
  on,
  onClick,
  size = 'lg',
  label,
  disabled,
}: {
  on: boolean;
  onClick?: () => void;
  size?: 'lg' | 'md' | 'sm';
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type='button'
      role='switch'
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      className={'tg tg-' + size + (on ? ' on' : '')}
      onClick={(event) => {
        event.stopPropagation();
        onClick?.();
      }}
    />
  );
}

export function MoreMenu({
  items,
  onPick,
  align = 'right',
  label = 'More',
}: {
  items: readonly string[];
  onPick?: (item: string) => void;
  align?: 'left' | 'right';
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useOutsideClick(ref, () => setOpen(false), open);
  return (
    <span className='menu' ref={ref}>
      <button
        type='button'
        className='more'
        aria-label={label}
        onClick={(event) => {
          event.stopPropagation();
          setOpen((value) => !value);
        }}
      >
        <i />
        <i />
        <i />
      </button>
      {open && (
        <span className={'menu-pop ' + align}>
          {items.map((item) => (
            <button
              key={item}
              type='button'
              onClick={(event) => {
                event.stopPropagation();
                setOpen(false);
                onPick?.(item);
              }}
            >
              {item}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}

/** The prototype's in-stage dialog: a dimmed backdrop over the stage with a glass card (`qn`). */
export function Popup({
  title,
  onClose,
  children,
  width = 480,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        close.current();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);
  return (
    <div className='modal-back' onPointerDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className='modal glass' style={{ width }} role='dialog' aria-label={title}>
        <div className='modal-head'>
          <span>{title}</span>
          <button type='button' className='icon-btn' onClick={onClose} aria-label='Close'>
            <Icon n='x' size={16} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

export type StatusPillKind = 'run' | 'done' | 'idle' | 'need';

export function StatusPill({ kind, children }: { kind: StatusPillKind; children: ReactNode }) {
  return (
    <span className={'dpill ' + kind}>
      <i />
      {children}
    </span>
  );
}

export function Lights({ small }: { small?: boolean }) {
  return (
    <span className={'lights' + (small ? ' sm' : '')}>
      <i />
      <i />
      <i />
    </span>
  );
}

export function Spinner({ large }: { large?: boolean }) {
  return <span className={'spinner' + (large ? ' lg' : '')} />;
}

export function Eyebrow({ x, y, w, children }: { x: number; y: number; w?: number; children: ReactNode }) {
  return (
    <div className={'eyebrow' + (w ? ' center' : '')} style={{ position: 'absolute', left: x, top: y, width: w }}>
      <i />
      {children}
    </div>
  );
}

export function Heading({
  x,
  y,
  w,
  size = 50,
  l1,
  l2,
  center,
}: {
  x: number;
  y: number;
  w: number;
  size?: number;
  l1: string;
  l2?: string;
  center?: boolean;
}) {
  return (
    <h1
      className={'h1' + (center ? ' center' : '')}
      style={{ position: 'absolute', left: x, top: y, width: w, fontSize: size, lineHeight: `${size}px` }}
    >
      {l1}
      {l2 && (
        <>
          <br />
          <span className='h1-2'>{l2}</span>
        </>
      )}
    </h1>
  );
}

export function Sub({
  x,
  y,
  w,
  children,
  size = 15,
  center,
}: {
  x: number;
  y: number;
  w: number;
  children: ReactNode;
  size?: number;
  center?: boolean;
}) {
  return (
    <p
      className={'sub' + (center ? ' center' : '')}
      style={{ position: 'absolute', left: x, top: y, width: w, fontSize: size }}
    >
      {children}
    </p>
  );
}

export function Cta({
  style,
  filled,
  children,
  onClick,
  arrow = true,
  disabled,
  className = '',
}: {
  style?: CSSProperties;
  filled?: boolean;
  children: ReactNode;
  onClick?: () => void;
  arrow?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type='button'
      className={'cta ' + (filled ? 'filled ' : 'outline ') + className}
      style={style}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
      {arrow && <Icon n='arrowR' size={18} />}
    </button>
  );
}

/** Makes a non-button element act like a button for pointer and keyboard (`Ul`). */
export function buttonProps(onActivate: () => void) {
  return {
    role: 'button' as const,
    tabIndex: 0,
    onClick: onActivate,
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
      if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
      event.preventDefault();
      onActivate();
    },
  };
}

const QR_CELLS =
  '0,0 5,0 10,0 15,0 20,0 25,0 30,0 45,0 55,0 70,0 75,0 80,0 85,0 90,0 95,0 100,0 0,5 30,5 50,5 70,5 100,5 0,10 10,10 15,10 20,10 30,10 45,10 60,10 70,10 80,10 85,10 90,10 100,10 0,15 10,15 15,15 20,15 30,15 45,15 70,15 80,15 85,15 90,15 100,15 0,20 10,20 15,20 20,20 30,20 55,20 60,20 70,20 80,20 85,20 90,20 100,20 0,25 30,25 50,25 70,25 100,25 0,30 5,30 10,30 15,30 20,30 25,30 30,30 40,30 50,30 60,30 70,30 75,30 80,30 85,30 90,30 95,30 100,30 30,40 50,40 10,45 20,45 25,45 35,45 55,45 65,45 75,45 80,45 85,45 90,45 100,45 0,50 10,50 25,50 30,50 40,50 45,50 70,50 75,50 85,50 90,50 10,55 15,55 20,55 35,55 45,55 50,55 55,55 60,55 65,55 75,55 0,60 25,60 30,60 45,60 50,60 60,60 65,60 70,60 75,60 85,60 90,60 95,60 45,65 50,65 55,65 65,65 75,65 95,65 100,65 0,70 5,70 10,70 15,70 20,70 25,70 30,70 45,70 55,70 60,70 65,70 70,70 75,70 85,70 95,70 100,70 0,75 30,75 45,75 65,75 70,75 75,75 100,75 0,80 10,80 15,80 20,80 30,80 50,80 75,80 100,80 0,85 10,85 15,85 20,85 30,85 50,85 55,85 70,85 75,85 80,85 85,85 95,85 100,85 0,90 10,90 15,90 20,90 30,90 55,90 60,90 65,90 85,90 90,90 95,90 100,90 0,95 30,95 50,95 75,95 100,95 0,100 5,100 10,100 15,100 20,100 25,100 30,100 45,100 50,100 65,100 70,100 80,100 85,100 95,100'
    .split(' ')
    .map((cell) => cell.split(',').map(Number) as [number, number]);

/** Decorative pairing code for the previews; the real code lives in Settings -> Remote. */
export function QrCode({ size = 120 }: { size?: number }) {
  return (
    <svg className='qr-svg' width={size} height={size} viewBox='-6 -6 117 117' aria-label='Pairing QR code'>
      <rect x='-6' y='-6' width='117' height='117' rx='6' fill='#f1f3f7' />
      {QR_CELLS.map(([x, y]) => (
        <rect key={`${x}-${y}`} x={x} y={y} width='5' height='5' fill='#0b0d12' />
      ))}
    </svg>
  );
}

export { box };
