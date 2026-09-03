#import <AppKit/AppKit.h>

/*
CDXC:Terminal 2026-07-12:
Hide-while-typing cursor concealment for the GPUI-composited terminal
element. This lives apart from GpuiTerminalAppKitAdapter.m because the
composited element (used by every terminal-rendering binary, including the
temporary terminal-element-demo) needs only this call, while the host-view
adapter object requires the app binary's Rust key/IME callback exports at
link time. Keep this shim to cursor concealment only.
*/
void GhostexGpuiTerminalHideMouseCursorUntilMouseMoves(void) {
  [NSCursor setHiddenUntilMouseMoves:YES];
}
