#import <AppKit/AppKit.h>

// CDXC:FocusRouting 2026-10-10 WHY:
// Changing a window's style mask makes AppKit swap its frame view, which takes first responder and
// key status away from the GPUI view (GPUI's simple-fullscreen code restores both for the same
// reason). A GPUI view never takes first responder back from a click, so a window made borderless
// while it was already on screen and key dropped every typed key: the Files view's comment box
// under window glass on macOS showed a focused field that no key reached. The New Thread picker
// escaped it only because it is made borderless while still hidden. The window keeps the responder
// and key status it had.
static void GhostexGpuiMakeWindowBorderless(NSWindow *window) {
  NSResponder *responder = window.firstResponder;
  BOOL wasKey = window.isKeyWindow;
  window.styleMask = NSWindowStyleMaskBorderless;
  if (responder != nil && window.firstResponder != responder) {
    [window makeFirstResponder:responder];
  }
  if (wasKey && !window.isKeyWindow) {
    [window makeKeyWindow];
  }
}

// CDXC:AppModal 2026-11-09 WHY:
// Makes a chat pane dialog's window (Save to Markdown, Rewind) a borderless, shadowless, clear
// window. It used to add a WindowServer background blur (CGSSetWindowBackgroundBlurRadius, 24)
// across the whole pane on top of GPUI's own blur; with both, the chat behind Save to Markdown
// showed as solid black. Under glass the card alone is blurred, by GPUI
// (native_chat/child_window.rs `pane_dialog_blur_region`); off glass the dialog is a solid card
// over a light dim and needs no blur.
void GhostexGpuiPrepareChatDialogWindow(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  NSWindow *window = view.window;
  if (!window) return;
  GhostexGpuiMakeWindowBorderless(window);
  window.opaque = NO;
  window.backgroundColor = NSColor.clearColor;
  window.hasShadow = NO;
  [window invalidateShadow];
}

// CDXC:AppModal 2026-09-21 DECISION:
// User, on popups that open as their own window: "I don't like this at all", about the large
// rounded system frame, white ring and heavy drop shadow a titled macOS window brings, which the
// sidebar's in-window context menu does not have. Every chat popup and the New Thread picker goes
// borderless and shadowless so only the panel GPUI draws is visible.
void GhostexGpuiStripPopupWindowFrame(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  NSWindow *window = view.window;
  if (!window) return;
  GhostexGpuiMakeWindowBorderless(window);
  window.opaque = NO;
  window.backgroundColor = NSColor.clearColor;
  window.hasShadow = NO;
  [window invalidateShadow];
  // CDXC:AppModal 2026-09-24 WHY:
  // Rows of the chat menus (More actions, Mode, Context window, Switch Account) showed no hover:
  // AppKit hands a normal window's mouse moves to the key window's first responder, and these
  // popups never received them. They get the tracking area GPUI gives its own popup windows, which
  // delivers moves to the view whichever window is key, and stop the first-responder path so no
  // move arrives twice.
  window.acceptsMouseMovedEvents = NO;
  [view addTrackingArea:[[NSTrackingArea alloc]
                            initWithRect:NSZeroRect
                                 options:NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved |
                                         NSTrackingActiveAlways | NSTrackingInVisibleRect
                                   owner:view
                                userInfo:nil]];
}

static NSTimeInterval GhostexGpuiLastPointerPressAt = 0;
static id GhostexGpuiPointerPressMonitor = nil;

// CDXC:SessionChat 2026-09-22 WHY:
// A chat menu is a child window that closes when it loses key status, and a press anywhere in the
// app makes the pressed window key before the press is delivered, so "lost key while a mouse button
// is down, or within a moment of a press" is the user dismissing the menu. Key status the main
// window takes back without any press (an app activation still completing, or another program
// focusing the window) is not. The monitor sees every press the app receives, in native child views
// as well as GPUI's own, which GPUI's window handlers do not.
bool GhostexGpuiPointerPressedRecently(void) {
  if (GhostexGpuiPointerPressMonitor == nil) {
    GhostexGpuiPointerPressMonitor = [NSEvent
        addLocalMonitorForEventsMatchingMask:(NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown |
                                              NSEventMaskOtherMouseDown)
                                     handler:^NSEvent *(NSEvent *event) {
                                       GhostexGpuiLastPointerPressAt =
                                           [NSDate timeIntervalSinceReferenceDate];
                                       return event;
                                     }];
  }
  if (NSEvent.pressedMouseButtons != 0) return true;
  return [NSDate timeIntervalSinceReferenceDate] - GhostexGpuiLastPointerPressAt < 0.3;
}

bool GhostexGpuiApplicationIsActive(void) { return NSApp.isActive; }
