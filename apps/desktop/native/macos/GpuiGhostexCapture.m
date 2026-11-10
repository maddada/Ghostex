// Ghostex Capture on macOS: keeps the floating button and its windows above other apps and in
// every Space, moves them, registers the system-wide hotkeys, and takes the screenshots.
//
// CDXC:GhostexCapture 2026-09-30 WHY:
// GPUI opens the windows (so the same views run on Windows and Linux) but has no call to move an
// open window, keep a pop-up visible while another app is active, or register a system-wide
// hotkey. Carbon's RegisterEventHotKey is used because it needs no Accessibility or Input
// Monitoring permission, unlike an NSEvent global monitor. Screenshots use the same WindowServer
// API as App Shots, which needs this file compiled against the macOS 13 deployment target.

#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>
#import <CoreGraphics/CoreGraphics.h>

extern void GhostexGpuiCaptureHotkeyPressed(uint32_t hotkey_id);

static const OSType GhostexGpuiCaptureHotkeySignature = 'GXCP';
static EventHandlerRef GhostexGpuiCaptureHotkeyHandler = NULL;
static EventHotKeyRef GhostexGpuiCaptureHotkeyRefs[8] = {NULL};

static NSWindow *GhostexGpuiCaptureWindowOfView(void *native_view) {
  if (!native_view) {
    return nil;
  }
  NSView *view = (__bridge NSView *)native_view;
  return view.window;
}

/// CDXC:GhostexCapture 2026-10-16 DECISION:
/// User: "pls remove the traffic lights on mac". GPUI opens a window without a title bar as a titled
/// window whose content fills it, so macOS still draws its close, minimize and zoom buttons over the
/// top-left of the content (the editor's first tools). Every Ghostex Capture window hides them; they
/// close with their own buttons and Escape. Changing a window's style mask brings them back, so
/// `GhostexGpuiCaptureMakeResizable` hides them again.
static void GhostexGpuiCaptureHideWindowButtons(NSWindow *window) {
  for (NSNumber *kind in @[ @(NSWindowCloseButton), @(NSWindowMiniaturizeButton),
                            @(NSWindowZoomButton) ]) {
    [window standardWindowButton:(NSWindowButton)kind.integerValue].hidden = YES;
  }
}

/// CDXC:GhostexCapture 2026-10-22 WHY:
/// A titled window is clipped by macOS to its own corner, rounder than the panel and editor GPUI
/// draws inside it, so the drawn 1px border vanished at every corner. Capture windows go
/// borderless so the drawn card's corner is the only one. The shared
/// `GhostexGpuiStripPopupWindowFrame` is not used: it drops the non-activating panel bit, and these
/// windows take the keyboard over other apps without activating Ghostex.
static void GhostexGpuiCaptureMakeBorderless(NSWindow *window) {
  NSWindowStyleMask mask =
      NSWindowStyleMaskBorderless |
      (window.styleMask & (NSWindowStyleMaskNonactivatingPanel | NSWindowStyleMaskResizable));
  if (window.styleMask == mask) {
    return;
  }
  NSResponder *responder = window.firstResponder;
  BOOL wasKey = window.isKeyWindow;
  window.styleMask = mask;
  if (responder != nil && window.firstResponder != responder) {
    [window makeFirstResponder:responder];
  }
  if (wasKey && !window.isKeyWindow) {
    [window makeKeyWindow];
  }
}

/// Keeps a Ghostex Capture window visible over other apps, in every Space and over fullscreen
/// apps. `keyable` windows (the panel, the editor, the prompt box, the capture overlay) take the
/// keyboard without activating Ghostex; the button never does.
void GhostexGpuiCapturePrepareFloatingWindow(void *native_view, bool keyable) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  if (!window) {
    return;
  }
  window.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                              NSWindowCollectionBehaviorStationary |
                              NSWindowCollectionBehaviorFullScreenAuxiliary |
                              NSWindowCollectionBehaviorIgnoresCycle;
  window.hidesOnDeactivate = NO;
  window.hasShadow = NO;
  GhostexGpuiCaptureMakeBorderless(window);
  GhostexGpuiCaptureHideWindowButtons(window);
  if ([window isKindOfClass:[NSPanel class]]) {
    ((NSPanel *)window).becomesKeyOnlyIfNeeded = !keyable;
    ((NSPanel *)window).floatingPanel = YES;
  }
  if (keyable) {
    [window makeKeyAndOrderFront:nil];
  } else {
    [window orderFrontRegardless];
  }
}

/// Moves a window to a frame given in GPUI's global space (points from the top-left corner of the
/// primary display, y down).
void GhostexGpuiCaptureSetWindowFrame(void *native_view, double x, double y,
                                      double width, double height) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  NSScreen *primary = NSScreen.screens.firstObject;
  if (!window || !primary) {
    return;
  }
  CGFloat top = NSMaxY(primary.frame);
  [window setFrame:NSMakeRect(x, top - y - height, width, height)
           display:YES
           animate:NO];
}

/// Lays the area-selection overlay over a whole screen, menu bar, notch and Dock included, from a
/// frame in GPUI's global space.
///
/// CDXC:GhostexCapture 2026-10-23 WHY:
/// User: area selection "shows the whole screen shifted down". GPUI opens the overlay as a titled
/// panel, which macOS keeps below the menu bar when it is shown, and `floatingPanel` drops it to the
/// floating level, under the menu bar and the Dock. Once borderless it is placed again (macOS leaves
/// a borderless window's frame alone) at the screen-saver level screenshot tools use, so the frozen
/// picture lines up with the screen and a drag can start anywhere on it.
void GhostexGpuiCaptureCoverScreen(void *native_view, double x, double y, double width,
                                   double height) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  if (!window) {
    return;
  }
  window.level = NSScreenSaverWindowLevel;
  GhostexGpuiCaptureSetWindowFrame(native_view, x, y, width, height);
}

/// Lets a borderless Ghostex Capture window be resized from its edges, no smaller than
/// `min_width` x `min_height` points. GPUI only adds the resizable style to windows with a title bar.
void GhostexGpuiCaptureMakeResizable(void *native_view, double min_width,
                                     double min_height) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  if (!window) {
    return;
  }
  window.styleMask |= NSWindowStyleMaskResizable;
  window.minSize = NSMakeSize(min_width, min_height);
  GhostexGpuiCaptureHideWindowButtons(window);
}

/// Where a window is and how big, in the space `GhostexGpuiCaptureSetWindowFrame` places it in
/// (top-left origin from the primary screen). GPUI's own bounds are measured from the window's
/// screen, so a frame read from them and set here moved a window that was on another screen, and
/// GPUI cannot read a window from inside that window's own event handlers at all.
bool GhostexGpuiCaptureWindowFrame(void *native_view, double *x, double *y,
                                   double *width, double *height) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  NSScreen *primary = NSScreen.screens.firstObject;
  if (!window || !primary) {
    return false;
  }
  NSRect frame = window.frame;
  *x = NSMinX(frame);
  *y = NSMaxY(primary.frame) - NSMaxY(frame);
  *width = NSWidth(frame);
  *height = NSHeight(frame);
  return true;
}

/// Gives a keyable Ghostex Capture window the keyboard again (after a capture hid and re-showed it).
void GhostexGpuiCaptureFocusWindow(void *native_view) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  [window makeKeyAndOrderFront:nil];
}

/// Takes a window off screen for a capture; true when it had the keyboard.
bool GhostexGpuiCaptureHideWindow(void *native_view) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  if (!window) {
    return false;
  }
  bool was_key = window.isKeyWindow;
  [window orderOut:nil];
  return was_key;
}

/// Brings back a window `GhostexGpuiCaptureHideWindow` hid, with the keyboard when `focus`.
void GhostexGpuiCaptureShowWindow(void *native_view, bool focus) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  if (focus) {
    [window makeKeyAndOrderFront:nil];
  } else {
    [window orderFrontRegardless];
  }
}

/// The pid of the app the user is in; 0 when there is none.
int32_t GhostexGpuiCaptureFrontmostPid(void) {
  NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
  return app ? app.processIdentifier : 0;
}

bool GhostexGpuiCaptureScreenAccess(bool request) {
  if (@available(macOS 10.15, *)) {
    if (CGPreflightScreenCaptureAccess()) {
      return true;
    }
    return request ? CGRequestScreenCaptureAccess() : false;
  }
  return true;
}

static bool GhostexGpuiCaptureWritePng(CGImageRef image, const char *path) {
  if (!image || !path) {
    return false;
  }
  NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithCGImage:image];
  NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG
                                  properties:@{}];
  if (!png) {
    return false;
  }
  NSString *target = [NSString stringWithUTF8String:path];
  return [png writeToFile:target atomically:YES];
}

/// Writes everything on screen inside a rect in GPUI's global space (which is also CoreGraphics'
/// global display space) to a PNG at full resolution.
bool GhostexGpuiCaptureRectPng(double x, double y, double width, double height,
                               const char *path) {
  @autoreleasepool {
    CGImageRef image = CGWindowListCreateImage(
        CGRectMake(x, y, width, height), kCGWindowListOptionOnScreenOnly,
        kCGNullWindowID, kCGWindowImageBestResolution);
    bool written = GhostexGpuiCaptureWritePng(image, path);
    if (image) {
      CFRelease(image);
    }
    return written;
  }
}

/// Writes the screen inside the front window of the app with `pid` to a PNG and reports that
/// window's frame in GPUI's global space and the app's name.
///
/// CDXC:GhostexCapture 2026-09-30 DECISION:
/// User: Capture App should "put the rectangle around that window's dimensions and capture instead
/// of capturing using api", because the single-window image left out Ghostex's CEF panes (they
/// are separate child windows). The pixels on screen inside the window's frame are read instead,
/// the way Windows and X11 already do it.
bool GhostexGpuiCaptureAppWindowPng(int32_t pid, const char *path,
                                    char *app_name, size_t app_name_capacity,
                                    double *frame) {
  @autoreleasepool {
    NSArray *windows = CFBridgingRelease(CGWindowListCopyWindowInfo(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID));
    for (NSDictionary *info in windows) {
      NSNumber *owner = info[(__bridge NSString *)kCGWindowOwnerPID];
      NSNumber *layer = info[(__bridge NSString *)kCGWindowLayer];
      NSNumber *alpha = info[(__bridge NSString *)kCGWindowAlpha];
      if (!owner || owner.intValue != pid || layer.integerValue != 0) {
        continue;
      }
      if (alpha && alpha.doubleValue <= 0.0) {
        continue;
      }
      CGRect bounds = CGRectZero;
      NSDictionary *boundsDictionary = info[(__bridge NSString *)kCGWindowBounds];
      if (!boundsDictionary ||
          !CGRectMakeWithDictionaryRepresentation(
              (__bridge CFDictionaryRef)boundsDictionary, &bounds) ||
          CGRectGetWidth(bounds) < 160.0 || CGRectGetHeight(bounds) < 120.0) {
        // An app's small helper windows (Ghostex's own pop-ups and hidden hosts among them) come
        // before its real window in the list; the capture wants the window the user sees.
        continue;
      }
      CGImageRef image = CGWindowListCreateImage(
          bounds, kCGWindowListOptionOnScreenOnly, kCGNullWindowID,
          kCGWindowImageBestResolution);
      bool written = GhostexGpuiCaptureWritePng(image, path);
      if (image) {
        CFRelease(image);
      }
      if (!written) {
        return false;
      }
      if (frame) {
        frame[0] = bounds.origin.x;
        frame[1] = bounds.origin.y;
        frame[2] = bounds.size.width;
        frame[3] = bounds.size.height;
      }
      NSString *name = info[(__bridge NSString *)kCGWindowOwnerName] ?: @"";
      if (app_name && app_name_capacity > 0) {
        strlcpy(app_name, name.UTF8String ?: "", app_name_capacity);
      }
      return true;
    }
    return false;
  }
}

static OSStatus GhostexGpuiCaptureHotkeyEvent(EventHandlerCallRef next,
                                              EventRef event, void *data) {
  EventHotKeyID hotkey = {0, 0};
  if (GetEventParameter(event, kEventParamDirectObject, typeEventHotKeyID, NULL,
                        sizeof(hotkey), NULL, &hotkey) == noErr &&
      hotkey.signature == GhostexGpuiCaptureHotkeySignature) {
    GhostexGpuiCaptureHotkeyPressed(hotkey.id);
  }
  return noErr;
}

void GhostexGpuiCaptureUnregisterHotkeys(void) {
  for (size_t index = 0; index < 8; index++) {
    if (GhostexGpuiCaptureHotkeyRefs[index]) {
      UnregisterEventHotKey(GhostexGpuiCaptureHotkeyRefs[index]);
      GhostexGpuiCaptureHotkeyRefs[index] = NULL;
    }
  }
}

/// Registers Cmd+Ctrl+Shift plus each key code; hotkey ids are the indexes into `key_codes`.
/// Returns how many registered (another app may own a combination already).
uint32_t GhostexGpuiCaptureRegisterHotkeys(const uint32_t *key_codes,
                                           uint32_t count) {
  GhostexGpuiCaptureUnregisterHotkeys();
  if (!GhostexGpuiCaptureHotkeyHandler) {
    EventTypeSpec spec = {kEventClassKeyboard, kEventHotKeyPressed};
    InstallApplicationEventHandler(&GhostexGpuiCaptureHotkeyEvent, 1, &spec,
                                   NULL, &GhostexGpuiCaptureHotkeyHandler);
  }
  uint32_t registered = 0;
  for (uint32_t index = 0; index < count && index < 8; index++) {
    EventHotKeyID hotkey = {GhostexGpuiCaptureHotkeySignature, index};
    if (RegisterEventHotKey(key_codes[index], cmdKey | controlKey | shiftKey,
                            hotkey, GetApplicationEventTarget(), 0,
                            &GhostexGpuiCaptureHotkeyRefs[index]) == noErr) {
      registered++;
    }
  }
  return registered;
}

/// Whether each display has its own Spaces (System Settings > Desktop & Dock > Displays have
/// separate Spaces). There a child window cannot sit on a display other than its parent's: macOS
/// puts it back in the parent's Space at the frame it was dropped at, mostly off that screen.
bool GhostexGpuiScreensHaveSeparateSpaces(void) {
  return NSScreen.screensHaveSeparateSpaces;
}

/// Whether the primary mouse button is held down anywhere, so a window drag is still going.
bool GhostexGpuiPrimaryMouseButtonDown(void) {
  return (NSEvent.pressedMouseButtons & 1) != 0;
}

/// Takes a child window off its parent, so ordering the parent front never shows it again while it
/// is put aside (a hidden Ghostex page, app/visual_page_hidden.rs).
void GhostexGpuiDetachChildWindow(void *native_view) {
  NSWindow *window = GhostexGpuiCaptureWindowOfView(native_view);
  NSWindow *parent = window.parentWindow;
  if (window && parent) {
    [parent removeChildWindow:window];
  }
}
