#import <AppKit/AppKit.h>
#import <dispatch/dispatch.h>

/*
 The app menu's About item shows the standard AppKit about panel (CFBundleName
 + versions from the packaged Info.plist), matching the macOS host's
 orderFrontStandardAboutPanel item. gpui exposes no about-panel API, so this
 stays a one-call AppKit shim with no state, logging, or payloads.
*/
void GhostexGpuiShowStandardAboutPanel(void) {
  void (^show)(void) = ^{
    [NSApp activateIgnoringOtherApps:YES];
    [NSApp orderFrontStandardAboutPanel:nil];
  };
  if ([NSThread isMainThread]) {
    show();
  } else {
    dispatch_async(dispatch_get_main_queue(), show);
  }
}
