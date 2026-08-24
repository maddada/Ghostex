/*
 * Fake macOS desktop: wallpaper, menu bar, dock, the simulated Ghostex window,
 * and the floating modal-window layer. Everything renders from the sandbox
 * store — this file owns no simulation state of its own beyond window focus,
 * which is pure desktop chrome.
 */
import { useState } from 'react';
import { ModalWindowFrame } from '../bridge/modal-window-frame';
import { useSandboxStore } from '../state/store';
import { Dock } from './dock';
import { GhostexWindow } from './ghostex-window';
import { MenuBar } from './menu-bar';
import './desktop.css';

export function Desktop() {
  const appPhase = useSandboxStore((s) => s.appPhase);
  const modalWindows = useSandboxStore((s) => s.modalWindows);
  const setTipsPanelOpen = useSandboxStore((s) => s.setTipsPanelOpen);
  const [windowFocused, setWindowFocused] = useState(true);
  const isRunning = appPhase === 'running';

  return (
    <div className='sbx-desktop'>
      <div
        className='sbx-desktop-wallpaper'
        onMouseDown={() => {
          setWindowFocused(false);
          setTipsPanelOpen(false);
        }}
      />
      <MenuBar />

      <div className='sbx-desktop-surface'>
        {isRunning ? (
          <div
            className='sbx-window-slot'
            data-modal-open={modalWindows.length > 0}
            onMouseDown={() => setWindowFocused(true)}
          >
            <GhostexWindow focused={windowFocused && modalWindows.length === 0} />
          </div>
        ) : (
          <div className='sbx-desktop-hint'>
            {appPhase === 'launching'
              ? 'Launching Ghostex…'
              : 'Click the Ghostex icon in the dock to launch the simulated app.'}
          </div>
        )}

        <div className='sbx-modal-layer'>
          {modalWindows.map((modalWindow, index) => (
            <div
              className='sbx-modal-slot'
              key={modalWindow.windowId}
              style={{
                transform: `translate(${index * 26}px, ${index * 22}px)`,
                zIndex: 100 + index,
              }}
            >
              <ModalWindowFrame window={modalWindow} />
            </div>
          ))}
        </div>
      </div>

      <Dock onFocusGhostex={() => setWindowFocused(true)} />
    </div>
  );
}
