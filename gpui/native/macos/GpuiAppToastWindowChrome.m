#import <AppKit/AppKit.h>

void GhostexGpuiRemoveToastPopupWindowChrome(void* nativeView) {
  @autoreleasepool {
    if (nativeView == NULL) {
      return;
    }

    NSView* view = (__bridge NSView*)nativeView;
    NSWindow* window = view.window;
    if (window == nil) {
      return;
    }

    /*
     CDXC:GPUIAppToastWindowChrome 2026-07-04:
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

    NSView* contentView = window.contentView;
    contentView.wantsLayer = YES;
    contentView.layer.backgroundColor = NSColor.clearColor.CGColor;
    view.wantsLayer = YES;
    view.layer.backgroundColor = NSColor.clearColor.CGColor;
  }
}

/*
 CDXC:GPUIMainWindowToasts 2026-08-18:
 gpui gives every WindowKind::PopUp window NSPopUpWindowLevel, which floats the
 toast panel above every other application. Toasts belong to the Ghostex main
 window, so attach the panel as a real AppKit child window at the parent's own
 level: it then stays ordered directly above the main window, follows it when
 the user moves it, disappears with it on miniaturize/hide, and no longer draws
 over whatever app the user switched to.
 */
void GhostexGpuiAttachToastPopupToMainWindow(void* toastNativeView, void* mainNativeView) {
  @autoreleasepool {
    if (toastNativeView == NULL || mainNativeView == NULL) {
      return;
    }

    NSWindow* toastWindow = ((__bridge NSView*)toastNativeView).window;
    NSWindow* mainWindow = ((__bridge NSView*)mainNativeView).window;
    if (toastWindow == nil || mainWindow == nil || toastWindow == mainWindow) {
      return;
    }

    toastWindow.level = mainWindow.level;
    if (toastWindow.parentWindow != mainWindow) {
      [mainWindow addChildWindow:toastWindow ordered:NSWindowAbove];
    }
  }
}

void GhostexGpuiPrepareTitlebarPopupWindow(void* nativeView) {
  @autoreleasepool {
    if (nativeView == NULL) {
      return;
    }

    NSView* view = (__bridge NSView*)nativeView;
    NSWindow* window = view.window;
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
      ((NSPanel*)window).becomesKeyOnlyIfNeeded = YES;
      window.hidesOnDeactivate = NO;
    }
    [window orderFrontRegardless];
  }
}
