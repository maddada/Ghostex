#import <AppKit/AppKit.h>
#import <objc/runtime.h>

extern void GhostexGpuiCEFFocusGpuiRootView(void *nativeView);
extern void GhostexGpuiCEFFocusNativeView(void *nativeView);

@interface GhostexUsageKeyboardFocus : NSObject
@property(nonatomic, weak) NSResponder *previous;
@property(nonatomic, weak) NSView *root;
@end
@implementation GhostexUsageKeyboardFocus
@end

static char GhostexUsageKeyboardFocusKey;

// CDXC:AgentProviders 2026-09-12 WHY:
// A non-activating usage popup needs keyboard input in GPUI, otherwise Tab and Space still reach Chromium's composer. Preserve its responder until the popup closes.
void GhostexGpuiBeginUsageKeyboardFocus(void *nativeView) {
  NSView *root = (__bridge NSView *)nativeView;
  if (!root.window) return;
  GhostexUsageKeyboardFocus *state = [GhostexUsageKeyboardFocus new];
  state.previous = root.window.firstResponder;
  state.root = root;
  objc_setAssociatedObject(root.window, &GhostexUsageKeyboardFocusKey, state,
                           OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  GhostexGpuiCEFFocusGpuiRootView(nativeView);
}

void GhostexGpuiEndUsageKeyboardFocus(void *nativeView) {
  NSView *root = (__bridge NSView *)nativeView;
  NSWindow *window = root.window;
  if (!window) return;
  GhostexUsageKeyboardFocus *state =
      objc_getAssociatedObject(window, &GhostexUsageKeyboardFocusKey);
  NSResponder *previous = state.previous;
  if (window.firstResponder == state.root &&
      [previous isKindOfClass:[NSView class]] &&
      ((NSView *)previous).window == window) {
    GhostexGpuiCEFFocusNativeView((__bridge void *)previous);
  }
  objc_setAssociatedObject(window, &GhostexUsageKeyboardFocusKey, nil,
                           OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

void GhostexGpuiRemoveToastPopupWindowChrome(void *nativeView) {
  @autoreleasepool {
    if (nativeView == NULL) {
      return;
    }

    NSView *view = (__bridge NSView *)nativeView;
    NSWindow *window = view.window;
    if (window == nil) {
      return;
    }

    /*
     CDXC:AppModal 2026-07-04:
     App toasts render inside a transparent GPUI popup because native CEF and
     Ghostty child views draw above in-window GPUI layers. Strip all AppKit
     frame chrome from the popup host so macOS cannot draw a titlebar edge,
     border, or window shadow behind the actual toast card. Keep only the
     card border/background in GPUI.
     */
    window.styleMask = NSWindowStyleMaskNonactivatingPanel;
    window.titleVisibility = NSWindowTitleHidden;
    window.titlebarAppearsTransparent = YES;
    window.opaque = NO;
    window.backgroundColor = NSColor.clearColor;
    window.hasShadow = NO;
    [window invalidateShadow];

    NSView *contentView = window.contentView;
    contentView.wantsLayer = YES;
    contentView.layer.backgroundColor = NSColor.clearColor.CGColor;
    view.wantsLayer = YES;
    view.layer.backgroundColor = NSColor.clearColor.CGColor;
  }
}

/*
 CDXC:AppModal 2026-08-18:
 gpui gives every WindowKind::PopUp window NSPopUpWindowLevel, which floats the
 toast panel above every other application. Toasts belong to the Ghostex main
 window, so attach the panel as a real AppKit child window at the parent's own
 level: it then stays ordered directly above the main window, follows it when
 the user moves it, disappears with it on miniaturize/hide, and no longer draws
 over whatever app the user switched to.
 */
void GhostexGpuiAttachToastPopupToMainWindow(void *toastNativeView,
                                             void *mainNativeView) {
  @autoreleasepool {
    if (toastNativeView == NULL || mainNativeView == NULL) {
      return;
    }

    NSWindow *toastWindow = ((__bridge NSView *)toastNativeView).window;
    NSWindow *mainWindow = ((__bridge NSView *)mainNativeView).window;
    if (toastWindow == nil || mainWindow == nil || toastWindow == mainWindow) {
      return;
    }

    toastWindow.level = mainWindow.level;
    if (toastWindow.parentWindow != mainWindow) {
      [mainWindow addChildWindow:toastWindow ordered:NSWindowAbove];
    }
  }
}

void GhostexGpuiPrepareTitlebarPopupWindow(void *nativeView) {
  @autoreleasepool {
    if (nativeView == NULL) {
      return;
    }

    NSView *view = (__bridge NSView *)nativeView;
    NSWindow *window = view.window;
    if (window == nil) {
      return;
    }

    GhostexGpuiRemoveToastPopupWindowChrome(nativeView);
    if ([window isKindOfClass:[NSPanel class]]) {
      /*
       Titlebar dropdown panels must never take key status from the main
       window: the menu is mouse-driven, Escape is handled by the main
       window, and a key-stealing panel makes the whole app look
       deactivated the moment the menu opens.
       */
      ((NSPanel *)window).becomesKeyOnlyIfNeeded = YES;
      window.hidesOnDeactivate = NO;
    }
    [window orderFrontRegardless];
  }
}
