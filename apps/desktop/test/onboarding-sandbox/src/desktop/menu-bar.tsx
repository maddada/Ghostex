/*
 * Fake macOS menu bar. The active application name follows the simulated app
 * phase so a launch is visible at the very top of the screen, exactly like the
 * real desktop.
 */
import { useEffect, useState } from 'react';
import { useSandboxStore } from '../state/store';
import { AppleGlyph, BatteryGlyph, ControlCenterGlyph, SearchGlyph, WifiGlyph } from './icons';
import './menu-bar.css';

const GHOSTEX_MENUS = ['File', 'Edit', 'View', 'Terminal', 'Window', 'Help'];
const FINDER_MENUS = ['File', 'Edit', 'View', 'Go', 'Window', 'Help'];

function useMenuBarClock(): string {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  return now
    .toLocaleString(undefined, {
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      month: 'short',
      weekday: 'short',
    })
    .replace(/,\s*(?=\d{1,2}:)/, '  ');
}

export function MenuBar() {
  const appPhase = useSandboxStore((s) => s.appPhase);
  const clock = useMenuBarClock();
  const isGhostexActive = appPhase !== 'notRunning';
  const menus = isGhostexActive ? GHOSTEX_MENUS : FINDER_MENUS;

  return (
    <div className='sbx-menu-bar'>
      <div className='sbx-menu-bar-left'>
        <span className='sbx-menu-bar-apple'>
          <AppleGlyph />
        </span>
        <span className='sbx-menu-bar-app' data-launching={appPhase === 'launching'}>
          {isGhostexActive ? 'Ghostex' : 'Finder'}
        </span>
        {menus.map((menu) => (
          <span className='sbx-menu-bar-item' key={menu}>
            {menu}
          </span>
        ))}
      </div>
      <div className='sbx-menu-bar-right'>
        <span className='sbx-menu-bar-status'>
          <BatteryGlyph />
        </span>
        <span className='sbx-menu-bar-status'>
          <WifiGlyph />
        </span>
        <span className='sbx-menu-bar-status'>
          <SearchGlyph />
        </span>
        <span className='sbx-menu-bar-status'>
          <ControlCenterGlyph />
        </span>
        <span className='sbx-menu-bar-clock'>{clock}</span>
      </div>
    </div>
  );
}
