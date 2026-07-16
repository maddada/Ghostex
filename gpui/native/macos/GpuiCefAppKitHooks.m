#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <stdbool.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <dispatch/dispatch.h>
#import <os/lock.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

void GhostexGpuiCEFDoMessageLoopWork(void);
int GhostexGpuiCEFHandleSelectAllForNativeView(void* nativeView);
int GhostexGpuiCEFHandleSelectAllForActiveNativeView(void);
int GhostexGpuiCEFHandleEditCommandForNativeView(void* nativeView, int command);
int GhostexGpuiCEFHandleZoomCommandForNativeView(void* nativeView, int command);
int GhostexGpuiCEFMarkNativeViewFocused(void* nativeView);
void GhostexGpuiCEFClearActiveNativeView(void);
int GhostexGpuiCEFRefreshSystemPageAppearanceForNativeView(void* nativeView);
void GhostexGpuiFirstResponderDidChange(void* responder);

// ABI contract with cef/shell.rs CefEditCommand::from_raw.
typedef enum {
  GhostexGpuiCEFEditCommandNone = 0,
  GhostexGpuiCEFEditCommandCut = 1,
  GhostexGpuiCEFEditCommandCopy = 2,
  GhostexGpuiCEFEditCommandPaste = 3,
} GhostexGpuiCEFEditCommand;

// ABI contract with cef/shell.rs CefZoomCommand::from_raw.
typedef enum {
  GhostexGpuiCEFZoomCommandNone = 0,
  GhostexGpuiCEFZoomCommandIn = 1,
  GhostexGpuiCEFZoomCommandOut = 2,
  GhostexGpuiCEFZoomCommandReset = 3,
} GhostexGpuiCEFZoomCommand;

static BOOL g_ghostexGpuiCEFMessagePumpInstalled = NO;
static BOOL g_ghostexGpuiCEFApplicationHooksInstalled = NO;
static BOOL g_ghostexGpuiCEFHandlingSendEvent = NO;
static BOOL g_ghostexGpuiCEFEditCommandBridged = NO;
static BOOL g_ghostexGpuiCEFMessagePumpWorkPending = NO;
static BOOL g_ghostexGpuiCEFMessagePumpWorkActive = NO;
static BOOL g_ghostexGpuiCEFMessagePumpReentrancyDetected = NO;
static uint64_t g_ghostexGpuiCEFMessagePumpGeneration = 0;
static os_unfair_lock g_ghostexGpuiCEFMessagePumpDispatchLock = OS_UNFAIR_LOCK_INIT;
static BOOL g_ghostexGpuiCEFMessagePumpDispatchPending = NO;
static int64_t g_ghostexGpuiCEFMessagePumpDispatchDelayMs = INT64_MAX;
static uint64_t g_ghostexGpuiCEFMessagePumpDispatchEpoch = 0;
static uint64_t g_ghostexGpuiCEFMessagePumpDispatchRequests = 0;
static uint64_t g_ghostexGpuiCEFMessagePumpDispatchBlocks = 0;

static BOOL GhostexGpuiCEFResizeDiagnosticsEnabled(void) {
  static dispatch_once_t onceToken;
  static BOOL enabled = NO;
  dispatch_once(&onceToken, ^{
    enabled = getenv("GHOSTEX_GPUI_CEF_RESIZE_DIAGNOSTICS") != NULL;
  });
  return enabled;
}

static const int64_t GhostexGpuiCEFMessagePumpPlaceholderDelayMs = INT32_MAX;
static const int64_t GhostexGpuiCEFMessagePumpImmediateTimerDelayMs = 1000 / 120;
static const int64_t GhostexGpuiCEFMessagePumpMaxTimerDelayMs = 1000 / 30;

static void GhostexGpuiCEFRunScheduledMessagePumpWork(void);
static void GhostexGpuiCEFOnScheduleMessagePumpWork(int64_t delayMs);
static void GhostexGpuiCEFInstallStandardEditMenu(void);
static void GhostexGpuiCEFBrowserViewMouseDown(id self, SEL _cmd, NSEvent* event);
static BOOL GhostexGpuiCEFBrowserViewAcceptsFirstResponder(id self, SEL _cmd);
static void GhostexGpuiCEFBrowserViewSelectAll(id self, SEL _cmd, id sender);
static BOOL GhostexGpuiCEFBrowserViewPerformKeyEquivalent(id self, SEL _cmd, NSEvent* event);
static void GhostexGpuiCEFBrowserViewAddSubview(id self, SEL _cmd, NSView* subview);
static void GhostexGpuiCEFBrowserViewDidChangeEffectiveAppearance(id self, SEL _cmd);
static void GhostexGpuiCEFInstallBrowserViewFocusSubclass(NSView* view);
static void GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(NSView* view);
static void GhostexGpuiCEFBrowserViewCut(id self, SEL _cmd, id sender);
static void GhostexGpuiCEFBrowserViewCopy(id self, SEL _cmd, id sender);
static void GhostexGpuiCEFBrowserViewPaste(id self, SEL _cmd, id sender);
static BOOL GhostexGpuiCEFEventIsCommandA(NSEvent* event);
static GhostexGpuiCEFEditCommand GhostexGpuiCEFClipboardEditCommandForEvent(NSEvent* event);
static GhostexGpuiCEFZoomCommand GhostexGpuiCEFZoomCommandForEvent(NSEvent* event);
static BOOL GhostexGpuiCEFHandleSelectAllForResponder(id responder);
static BOOL GhostexGpuiCEFHandleEditCommandForResponder(
  id responder,
  GhostexGpuiCEFEditCommand command);
static BOOL GhostexGpuiCEFHandleZoomCommandForResponder(
  id responder,
  GhostexGpuiCEFZoomCommand command);
static void GhostexGpuiCEFBrowserViewForwardEditActionToSuper(id self, SEL _cmd, id sender);
static void GhostexGpuiCEFMarkFocusedResponder(id responder);
static BOOL GhostexGpuiCEFRefreshSystemPageAppearanceForView(NSView* view);
static NSEvent* GhostexGpuiNormalizedNavigationKeyEvent(NSEvent* event);
static void GhostexGpuiFirstResponderReportWindow(NSWindow* window);

@interface GhostexGpuiFirstResponderObserver : NSObject
@property(nonatomic, weak) NSWindow* window;
- (instancetype)initWithWindow:(NSWindow*)window;
@end

@implementation GhostexGpuiFirstResponderObserver
- (instancetype)initWithWindow:(NSWindow*)window {
  self = [super init];
  if (!self) {
    return nil;
  }
  _window = window;
  [window addObserver:self
           forKeyPath:@"firstResponder"
              options:NSKeyValueObservingOptionInitial | NSKeyValueObservingOptionNew
              context:NULL];
  return self;
}

- (void)dealloc {
  @try {
    [_window removeObserver:self forKeyPath:@"firstResponder"];
  } @catch (__unused NSException* exception) {
  }
}

- (void)observeValueForKeyPath:(NSString*)keyPath
                      ofObject:(id)object
                        change:(NSDictionary<NSKeyValueChangeKey, id>*)change
                       context:(void*)context {
  (void)change;
  (void)context;
  if ([keyPath isEqualToString:@"firstResponder"] && [object isKindOfClass:NSWindow.class]) {
    GhostexGpuiFirstResponderReportWindow((NSWindow*)object);
    return;
  }
  [super observeValueForKeyPath:keyPath ofObject:object change:change context:context];
}
@end

/*
 CDXC:GPUICefAppProtocol 2026-06-14-16:14:
 CEF's macOS external-run-loop path requires NSApplication to conform to CefAppProtocol before Chromium installs its CFRunLoop observers. Mirror the protocol definitions from cef_application_mac.h locally so this lightweight cef-rs shim can register the Objective-C category at load time without restoring a direct CEF C++ header dependency.
 */
@protocol CrAppProtocol
- (BOOL)isHandlingSendEvent;
@end

@protocol CrAppControlProtocol <CrAppProtocol>
- (void)setHandlingSendEvent:(BOOL)handlingSendEvent;
@end

@protocol CefAppProtocol <CrAppControlProtocol>
@end

@interface NSApplication (GhostexGpuiCEFApplication) <CefAppProtocol>
- (BOOL)isHandlingSendEvent;
- (void)setHandlingSendEvent:(BOOL)handlingSendEvent;
- (void)ghostexGpuiCEFSendEvent:(NSEvent*)event;
@end

@implementation NSApplication (GhostexGpuiCEFApplication)
+ (void)load {
  Method originalSendEvent = class_getInstanceMethod(self, @selector(sendEvent:));
  Method cefSendEvent = class_getInstanceMethod(self, @selector(ghostexGpuiCEFSendEvent:));
  if (originalSendEvent && cefSendEvent) {
    method_exchangeImplementations(originalSendEvent, cefSendEvent);
  }
}

- (BOOL)isHandlingSendEvent {
  return g_ghostexGpuiCEFHandlingSendEvent;
}

- (void)setHandlingSendEvent:(BOOL)handlingSendEvent {
  g_ghostexGpuiCEFHandlingSendEvent = handlingSendEvent;
}

- (void)ghostexGpuiCEFSendEvent:(NSEvent*)event {
  /*
   CDXC:GPUINavKeyEventNormalization 2026-07-04:
   CGEvent-synthesized keyboards (Karabiner's virtual HID, BetterTouchTool,
   CGEvent-posting automation) deliver arrow/Home/End/PageUp/PageDown and
   backspace/forward-delete key events whose underlying CGEvent unicode
   payload is the raw layout translation of those keys: legacy control
   codes (0x1C-0x1F for arrows, 0x08/0x7F for the delete keys). The NSEvent fields still read as the normal F700-range
   function-key characters, but macOS's async TSM input-method path reads
   the raw CGEvent payload and commits it as literal text to the focused
   NSTextInputClient without any keyDown dispatch — inserting invisible
   control characters into terminals, CEF inputs, and GPUI inputs alike.
   Normalize once here at the app's single event entry point so every
   downstream view sees the payload-free event shape a hardware key
   produces.
   */
  event = GhostexGpuiNormalizedNavigationKeyEvent(event);

  /*
   CDXC:GPUICefEditCommands 2026-06-14-17:25:
   GPUI can keep its address-input focus handle after Chromium has accepted a page click, so AppKit command-key dispatch may never invoke selectAll: on CEF's responder chain. When the active native target is a registered CEF view, mirror only Cmd+A in the existing CEF NSApplication sendEvent hook and call Chromium's Frame::select_all after normal dispatch; GPUI chrome clicks clear that active target before their own text shortcuts run.
   */
  BOOL shouldSelectAllInActiveCEF = GhostexGpuiCEFEventIsCommandA(event);

  /*
   CDXC:GPUICefEditCommands 2026-07-09:
   Cmd+X/C/V need the same post-dispatch mirror as Cmd+A because GPUI's
   window key handling consumes command chords before AppKit menu or
   responder-chain dispatch can reach CEF child views. Two differences from
   the Cmd+A path keep clipboard state safe: the mirror resolves its target
   from the event window's actual first responder (never the last-active
   CEF view registry, which can be stale after focus moved to a native
   terminal), and a per-event bridged flag skips the mirror whenever normal
   dispatch already delivered the command to Chromium through the CEF view
   subclass, so no path can double-cut or double-paste.
   */
  GhostexGpuiCEFEditCommand clipboardCommand = GhostexGpuiCEFClipboardEditCommandForEvent(event);

  BOOL wasHandlingSendEvent = g_ghostexGpuiCEFHandlingSendEvent;
  BOOL wasEditCommandBridged = g_ghostexGpuiCEFEditCommandBridged;
  g_ghostexGpuiCEFHandlingSendEvent = YES;
  g_ghostexGpuiCEFEditCommandBridged = NO;
  @try {
    [self ghostexGpuiCEFSendEvent:event];
  } @finally {
    g_ghostexGpuiCEFHandlingSendEvent = wasHandlingSendEvent;
  }

  if (shouldSelectAllInActiveCEF) {
    GhostexGpuiCEFHandleSelectAllForActiveNativeView();
  }

  if (clipboardCommand != GhostexGpuiCEFEditCommandNone &&
      !g_ghostexGpuiCEFEditCommandBridged) {
    NSWindow* window = event.window ?: NSApp.keyWindow;
    GhostexGpuiCEFHandleEditCommandForResponder(window.firstResponder, clipboardCommand);
  }
  g_ghostexGpuiCEFEditCommandBridged = wasEditCommandBridged;
}
@end

void GhostexGpuiCEFPrepareApplication(void) {
  @autoreleasepool {
    NSUserDefaults* defaults = [NSUserDefaults standardUserDefaults];
    NSMutableDictionary* argumentDefaults =
      [[defaults volatileDomainForName:NSArgumentDomain] mutableCopy] ?: [NSMutableDictionary dictionary];
    /*
     CDXC:GPUICefCrashRestore 2026-06-14-15:25:
     The GPUI CEF shell is launched repeatedly while Chromium embedding is under construction. Disable AppKit's crash-state restoration prompts in the process argument domain so a saved-state modal cannot block the first GPUI frame or the deferred CEF initialization path.
     */
    argumentDefaults[@"ApplePersistenceIgnoreState"] = @YES;
    argumentDefaults[@"NSQuitAlwaysKeepsWindows"] = @NO;
    [defaults setVolatileDomain:argumentDefaults forName:NSArgumentDomain];
  }
}

bool GhostexGpuiCEFSystemUsesDarkPageAppearance(void) {
  NSAppearance* appearance = NSApp.effectiveAppearance ?: NSAppearance.currentAppearance;
  NSAppearanceName match = [appearance bestMatchFromAppearancesWithNames:@[
    NSAppearanceNameAqua,
    NSAppearanceNameDarkAqua,
  ]];
  return [match isEqualToString:NSAppearanceNameDarkAqua];
}

void GhostexGpuiCEFInstallMessagePump(void) {
  if (g_ghostexGpuiCEFMessagePumpInstalled) {
    return;
  }

  /*
   CDXC:GPUICefMessagePump 2026-06-14-15:25:
   GPUI owns the AppKit run loop, while cef-rs exposes a single-step CefDoMessageLoopWork pump. Let CEF's BrowserProcessHandler schedule each required step onto the main queue instead of handing the process to CefRunMessageLoop, matching Ghostex's GPUI-safe external-pump model without replacing GPUI's application loop.

   CDXC:GPUICefMessagePump 2026-06-14-17:38:
   The cef-rs/Tauri external pump does not fire only once. It cancels stale work, caps placeholder delays to a short timer, and reschedules idle work so CEF renderers continue painting React sidebar content and browser pages after startup.
   */
  g_ghostexGpuiCEFMessagePumpInstalled = YES;
  g_ghostexGpuiCEFMessagePumpWorkPending = NO;
  g_ghostexGpuiCEFMessagePumpWorkActive = NO;
  g_ghostexGpuiCEFMessagePumpReentrancyDetected = NO;
  g_ghostexGpuiCEFMessagePumpGeneration += 1;
  os_unfair_lock_lock(&g_ghostexGpuiCEFMessagePumpDispatchLock);
  g_ghostexGpuiCEFMessagePumpDispatchPending = NO;
  g_ghostexGpuiCEFMessagePumpDispatchDelayMs = INT64_MAX;
  g_ghostexGpuiCEFMessagePumpDispatchEpoch += 1;
  os_unfair_lock_unlock(&g_ghostexGpuiCEFMessagePumpDispatchLock);
}

void GhostexGpuiCEFInvalidateMessagePump(void) {
  g_ghostexGpuiCEFMessagePumpInstalled = NO;
  g_ghostexGpuiCEFMessagePumpWorkPending = NO;
  g_ghostexGpuiCEFMessagePumpGeneration += 1;
  os_unfair_lock_lock(&g_ghostexGpuiCEFMessagePumpDispatchLock);
  g_ghostexGpuiCEFMessagePumpDispatchPending = NO;
  g_ghostexGpuiCEFMessagePumpDispatchDelayMs = INT64_MAX;
  g_ghostexGpuiCEFMessagePumpDispatchEpoch += 1;
  os_unfair_lock_unlock(&g_ghostexGpuiCEFMessagePumpDispatchLock);
}

void GhostexGpuiCEFScheduleMessagePumpWork(int64_t delayMs) {
  /*
   CDXC:GPUICefMessagePumpCoalescing 2026-07-12:
   CEF may request message-pump work from any thread, including synchronously
   while the main thread is inside CefDoMessageLoopWork. Marshalling every
   callback with dispatch_async lets the post-pump placeholder race ahead of
   the still-queued zero-delay callback, creating a two-for-one feedback loop
   that floods the main queue during browser resize. Coalesce at this
   cross-thread boundary so one main-queue block observes all callbacks that
   arrived before it runs. New real CEF requests replace the pending delay,
   matching the external-pump contract; only the adapter's placeholder must
   not overwrite real work that is already queued.
  */
  BOOL shouldDispatch = NO;
  uint64_t epoch = 0;
  uint64_t firstRequest = 0;
  os_unfair_lock_lock(&g_ghostexGpuiCEFMessagePumpDispatchLock);
  uint64_t request = ++g_ghostexGpuiCEFMessagePumpDispatchRequests;
  if (!g_ghostexGpuiCEFMessagePumpDispatchPending) {
    g_ghostexGpuiCEFMessagePumpDispatchPending = YES;
    g_ghostexGpuiCEFMessagePumpDispatchDelayMs = delayMs;
    epoch = g_ghostexGpuiCEFMessagePumpDispatchEpoch;
    firstRequest = request;
    shouldDispatch = YES;
  } else if (delayMs != GhostexGpuiCEFMessagePumpPlaceholderDelayMs) {
    g_ghostexGpuiCEFMessagePumpDispatchDelayMs = delayMs;
  }
  os_unfair_lock_unlock(&g_ghostexGpuiCEFMessagePumpDispatchLock);

  if (!shouldDispatch) {
    return;
  }

  CFAbsoluteTime enqueuedAt = CFAbsoluteTimeGetCurrent();
  dispatch_async(dispatch_get_main_queue(), ^{
    int64_t requestedDelayMs = INT64_MAX;
    uint64_t coalescedRequests = 0;
    uint64_t block = 0;
    BOOL currentEpoch = NO;
    os_unfair_lock_lock(&g_ghostexGpuiCEFMessagePumpDispatchLock);
    currentEpoch = epoch == g_ghostexGpuiCEFMessagePumpDispatchEpoch;
    if (currentEpoch) {
      requestedDelayMs = g_ghostexGpuiCEFMessagePumpDispatchDelayMs;
      coalescedRequests = g_ghostexGpuiCEFMessagePumpDispatchRequests - firstRequest;
      g_ghostexGpuiCEFMessagePumpDispatchPending = NO;
      g_ghostexGpuiCEFMessagePumpDispatchDelayMs = INT64_MAX;
      block = ++g_ghostexGpuiCEFMessagePumpDispatchBlocks;
    }
    os_unfair_lock_unlock(&g_ghostexGpuiCEFMessagePumpDispatchLock);

    if (!currentEpoch) {
      return;
    }

    if (GhostexGpuiCEFResizeDiagnosticsEnabled()) {
      double queueMs = (CFAbsoluteTimeGetCurrent() - enqueuedAt) * 1000.0;
      if (coalescedRequests >= 4 || block % 120 == 0) {
        NSLog(
          @"[gpui-cef-pump] block=%llu coalesced=%llu delay_ms=%lld queue_ms=%.3f",
          (unsigned long long)block,
          (unsigned long long)coalescedRequests,
          (long long)requestedDelayMs,
          queueMs);
      }
    }
    GhostexGpuiCEFOnScheduleMessagePumpWork(requestedDelayMs);
  });
}

static BOOL GhostexGpuiCEFPerformMessageLoopWork(void) {
  if (g_ghostexGpuiCEFMessagePumpWorkActive) {
    g_ghostexGpuiCEFMessagePumpReentrancyDetected = YES;
    return NO;
  }

  g_ghostexGpuiCEFMessagePumpReentrancyDetected = NO;
  g_ghostexGpuiCEFMessagePumpWorkActive = YES;
  GhostexGpuiCEFDoMessageLoopWork();
  g_ghostexGpuiCEFMessagePumpWorkActive = NO;

  return g_ghostexGpuiCEFMessagePumpReentrancyDetected;
}

static void GhostexGpuiCEFRunScheduledMessagePumpWork(void) {
  if (!g_ghostexGpuiCEFMessagePumpInstalled) {
    return;
  }

  BOOL wasReentrant = GhostexGpuiCEFPerformMessageLoopWork();
  if (wasReentrant) {
    GhostexGpuiCEFScheduleMessagePumpWork(0);
  } else if (!g_ghostexGpuiCEFMessagePumpWorkPending) {
    GhostexGpuiCEFScheduleMessagePumpWork(GhostexGpuiCEFMessagePumpPlaceholderDelayMs);
  }
}

static void GhostexGpuiCEFOnScheduleMessagePumpWork(int64_t delayMs) {
  if (!g_ghostexGpuiCEFMessagePumpInstalled) {
    return;
  }

  if (delayMs == GhostexGpuiCEFMessagePumpPlaceholderDelayMs &&
      g_ghostexGpuiCEFMessagePumpWorkPending) {
    return;
  }

  g_ghostexGpuiCEFMessagePumpGeneration += 1;
  g_ghostexGpuiCEFMessagePumpWorkPending = NO;

  /*
   CEF defines non-positive delays as "reasonably soon", not as permission to
   monopolize the main queue. The reference external-pump adapters schedule a
   platform timer after immediate work; GCD blocks have no equivalent minimum
   timer quantum and can therefore self-repost thousands of times per second.
   Use a one-shot 120 Hz timer for immediate work so Chromium receives two
   pump opportunities per 60 Hz frame while AppKit/GPUI input and layout keep
   normal run-loop ownership.
  */
  int64_t clampedDelayMs = delayMs <= 0
    ? GhostexGpuiCEFMessagePumpImmediateTimerDelayMs
    : delayMs;
  if (clampedDelayMs > GhostexGpuiCEFMessagePumpMaxTimerDelayMs) {
    clampedDelayMs = GhostexGpuiCEFMessagePumpMaxTimerDelayMs;
  }

  g_ghostexGpuiCEFMessagePumpWorkPending = YES;
  uint64_t generation = g_ghostexGpuiCEFMessagePumpGeneration;
  dispatch_time_t when = dispatch_time(DISPATCH_TIME_NOW, clampedDelayMs * NSEC_PER_MSEC);
  dispatch_after(when, dispatch_get_main_queue(), ^{
    if (!g_ghostexGpuiCEFMessagePumpInstalled ||
        !g_ghostexGpuiCEFMessagePumpWorkPending ||
        generation != g_ghostexGpuiCEFMessagePumpGeneration) {
      return;
    }

    g_ghostexGpuiCEFMessagePumpWorkPending = NO;
    GhostexGpuiCEFRunScheduledMessagePumpWork();
  });
}

void GhostexGpuiCEFInstallApplicationHooks(void) {
  GhostexGpuiCEFInstallStandardEditMenu();
  if (g_ghostexGpuiCEFApplicationHooksInstalled || !NSApp) {
    return;
  }

  Class appClass = [NSApp class];
  if (!appClass) {
    return;
  }

  /*
   CDXC:GPUICefAppProtocol 2026-06-14-15:25:
   Tauri's CEF runtime makes its NSApplication subclass conform to CefAppProtocol and toggles isHandlingSendEvent during sendEvent:. GPUI must keep GPUIApplication as the concrete app class, so install the same protocol surface and send-event state on GPUIApplication at runtime without changing window layout or input routing.

   CDXC:GPUICefAppProtocol 2026-06-14-16:14:
   Chromium's message_pump_mac.mm traps if CefAppProtocol is missing when NSApplication's run loop is already active. Register the protocol through the NSApplication category above before main, then add the same protocol chain to GPUIApplication for direct conformance checks while leaving the early swizzled sendEvent implementation in place.
   */
  class_addProtocol(appClass, @protocol(CrAppProtocol));
  class_addProtocol(appClass, @protocol(CrAppControlProtocol));
  class_addProtocol(appClass, @protocol(CefAppProtocol));
  g_ghostexGpuiCEFApplicationHooksInstalled = YES;
}

static NSMenuItem* GhostexGpuiCEFStandardEditMenuItem(NSString* title, SEL action, NSString* keyEquivalent) {
  NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:keyEquivalent];
  item.target = nil;
  item.keyEquivalentModifierMask = NSEventModifierFlagCommand;
  return item;
}

static BOOL GhostexGpuiCEFMenuContainsAction(NSMenu* menu, SEL action) {
  for (NSMenuItem* item in menu.itemArray) {
    if (item.action == action) {
      return YES;
    }
  }
  return NO;
}

static void GhostexGpuiCEFInstallStandardEditMenu(void) {
  if (!NSApp) {
    return;
  }

  NSMenu* mainMenu = NSApp.mainMenu;
  if (!mainMenu) {
    mainMenu = [[NSMenu alloc] initWithTitle:@""];
    NSApp.mainMenu = mainMenu;
  }

  NSMenu* editMenu = nil;
  for (NSMenuItem* item in mainMenu.itemArray) {
    if ([item.title isEqualToString:@"Edit"] || [item.submenu.title isEqualToString:@"Edit"]) {
      editMenu = item.submenu;
      break;
    }
  }

  if (!editMenu) {
    NSMenuItem* editItem = [[NSMenuItem alloc] initWithTitle:@"Edit" action:nil keyEquivalent:@""];
    editMenu = [[NSMenu alloc] initWithTitle:@"Edit"];
    editItem.submenu = editMenu;
    NSInteger insertionIndex = mainMenu.numberOfItems > 0 ? 1 : 0;
    [mainMenu insertItem:editItem atIndex:insertionIndex];
  }

  /*
   CDXC:GPUICefEditCommands 2026-06-14-16:31:
   Web-page inputs inside the embedded CEF browser need macOS standard Edit commands, including Cmd+A Select All. Install first-responder menu actions instead of synthesizing web-specific fallbacks so CEF, AppKit text views, and future browser surfaces receive the platform's normal text-command dispatch.
   */
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(undo:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(@"Undo", @selector(undo:), @"z")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(redo:))) {
    NSMenuItem* redo = GhostexGpuiCEFStandardEditMenuItem(@"Redo", @selector(redo:), @"Z");
    redo.keyEquivalentModifierMask = NSEventModifierFlagCommand | NSEventModifierFlagShift;
    [editMenu addItem:redo];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(cut:)) ||
      !GhostexGpuiCEFMenuContainsAction(editMenu, @selector(selectAll:))) {
    [editMenu addItem:[NSMenuItem separatorItem]];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(cut:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(@"Cut", @selector(cut:), @"x")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(copy:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(@"Copy", @selector(copy:), @"c")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(paste:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(@"Paste", @selector(paste:), @"v")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(selectAll:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(@"Select All", @selector(selectAll:), @"a")];
  }
}

void GhostexGpuiCEFSetNativeViewFrame(
  void* nativeView,
  double x,
  double y,
  double width,
  double height) {
  NSView* view = (__bridge NSView*)nativeView;
  if (!view) {
    return;
  }

  /*
   CDXC:GPUICefNativeViewFrame 2026-07-07:
   GPUI layout is the only owner of this CEF child view's frame. CEF creates
   the child with a width/height-sizable autoresizing mask, so AppKit resizes
   it on every window resize before GPUI's next layout pass; the Rust
   set_bounds cache then skips the correcting frame write whenever the logical
   bounds are unchanged (e.g. the fixed-width sidebar during a width-only
   window narrow), leaving the AppKit-adjusted frame in place. Pin the mask to
   not-sizable so the frame only ever changes through this setter.
  */
  view.autoresizingMask = NSViewNotSizable;

  NSView* parent = [view superview];
  CGFloat nativeY = y;
  if (parent && ![parent isFlipped]) {
    nativeY = NSHeight(parent.bounds) - y - height;
  }
  CFAbsoluteTime startedAt = CFAbsoluteTimeGetCurrent();
  view.frame = NSMakeRect(x, nativeY, MAX(0.0, width), MAX(0.0, height));
  if (GhostexGpuiCEFResizeDiagnosticsEnabled()) {
    double frameMs = (CFAbsoluteTimeGetCurrent() - startedAt) * 1000.0;
    if (frameMs >= 1.0) {
      NSLog(
        @"[gpui-cef-native-frame] frame_ms=%.3f rect=%.0f,%.0f,%.0fx%.0f",
        frameMs,
        x,
        nativeY,
        width,
        height);
    }
  }
}

void GhostexGpuiCEFLogResizeDiagnostic(
  int browserId,
  int width,
  int height,
  uint64_t frameUs,
  uint64_t wasResizedUs,
  uint64_t totalUs) {
  if (!GhostexGpuiCEFResizeDiagnosticsEnabled()) {
    return;
  }
  NSLog(
    @"[gpui-cef-resize] browser=%d size=%dx%d frame_us=%llu was_resized_us=%llu total_us=%llu",
    browserId,
    width,
    height,
    (unsigned long long)frameUs,
    (unsigned long long)wasResizedUs,
    (unsigned long long)totalUs);
}

void GhostexGpuiCEFSetNativeViewVisible(void* nativeView, bool visible) {
  NSView* view = (__bridge NSView*)nativeView;
  if (!view) {
    return;
  }
  view.hidden = visible ? NO : YES;
}

void GhostexGpuiCEFOrderNativeViewFront(void* nativeView) {
  NSView* view = (__bridge NSView*)nativeView;
  NSView* parent = view.superview;
  if (!view || !parent) {
    return;
  }

  /*
   CDXC:GPUITitlebarDropdownZOrder 2026-07-09:
   Sibling native children of the GPUI content view stack in creation order,
   and terminal host views are appended whenever a session mounts. A reused
   dropdown CEF panel created earlier would therefore reappear underneath
   newer terminal views, so showing a dropdown must explicitly re-order its
   native view above all current siblings.
  */
  if (parent.subviews.lastObject == view) {
    return;
  }
  [parent addSubview:view positioned:NSWindowAbove relativeTo:nil];
}

void GhostexGpuiCEFPrepareNativeViewForFocus(void* nativeView) {
  NSView* view = (__bridge NSView*)nativeView;
  if (!view) {
    return;
  }

  /*
   CDXC:GPUICefFocusRouting 2026-06-14-16:45:
   Browser clicks land on CEF's native child view, not always on GPUI's hitbox tree. Make the exact CEF NSView accept first responder and claim it on mouseDown before forwarding the event, so macOS command-key text actions route to Chromium after the user leaves the GPUI address bar.
  */
  GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(view);
}

void GhostexGpuiInstallFirstResponderObserverForNativeView(void* nativeView) {
  NSView* view = (__bridge NSView*)nativeView;
  if (!view || !view.window) {
    return;
  }

  static const void* GhostexGpuiFirstResponderObserverKey =
    &GhostexGpuiFirstResponderObserverKey;
  NSWindow* window = view.window;
  if (objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey)) {
    GhostexGpuiFirstResponderReportWindow(window);
    return;
  }

  GhostexGpuiFirstResponderObserver* observer =
    [[GhostexGpuiFirstResponderObserver alloc] initWithWindow:window];
  objc_setAssociatedObject(
    window,
    GhostexGpuiFirstResponderObserverKey,
    observer,
    OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

static void GhostexGpuiFirstResponderReportWindow(NSWindow* window) {
  if (!window) {
    GhostexGpuiFirstResponderDidChange(NULL);
    return;
  }
  id responder = window.firstResponder;
  /*
   CDXC:GPUIFirstResponderLifetime 2026-07-11:
   The Rust side defers responder classification onto the gpui foreground
   executor, and responder churn is often CAUSED by the very teardown that
   deallocates the outgoing responder view (browser/terminal host drops). A
   raw pointer would be dangling by the time the deferred task walks its
   superview chain — pass a +1 retained reference instead, released by Rust
   via GhostexGpuiReleaseRetainedResponder after classification. A retained
   view that was removed from its window classifies as no known surface,
   which is the correct answer for a dying responder.
   */
  GhostexGpuiFirstResponderDidChange(responder ? (void*)CFBridgingRetain(responder) : NULL);
}

void GhostexGpuiReleaseRetainedResponder(void* responder) {
  if (responder) {
    CFRelease((CFTypeRef)responder);
  }
}

bool GhostexGpuiNativeViewContainsResponder(void* rootNativeView, void* responder) {
  NSView* root = (__bridge NSView*)rootNativeView;
  id candidate = (__bridge id)responder;
  if (!root || ![candidate isKindOfClass:NSView.class]) {
    return false;
  }

  for (NSView* view = (NSView*)candidate; view; view = view.superview) {
    if (view == root) {
      return true;
    }
  }
  return false;
}

bool GhostexGpuiCEFNativeViewOwnsFirstResponder(void* nativeView) {
  NSView* view = (__bridge NSView*)nativeView;
  if (!view) {
    return false;
  }
  NSWindow* window = view.window;
  if (!window) {
    return false;
  }
  id responder = window.firstResponder;
  if (!responder) {
    return false;
  }
  return GhostexGpuiNativeViewContainsResponder(nativeView, (__bridge void*)responder);
}

static void GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(NSView* view) {
  if (!view) {
    return;
  }

  GhostexGpuiCEFInstallBrowserViewFocusSubclass(view);
  for (NSView* subview in view.subviews) {
    GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(subview);
  }
}

static void GhostexGpuiCEFInstallBrowserViewFocusSubclass(NSView* view) {
  Class originalClass = object_getClass(view);
  if (!originalClass) {
    return;
  }

  const char* originalName = class_getName(originalClass);
  if (strncmp(originalName, "GhostexGpuiCEFFocus_", 21) == 0) {
    return;
  }

  NSString* subclassName = [NSString stringWithFormat:@"GhostexGpuiCEFFocus_%s", originalName];
  Class subclass = NSClassFromString(subclassName);
  if (!subclass) {
    subclass = objc_allocateClassPair(originalClass, subclassName.UTF8String, 0);
    if (!subclass) {
      return;
    }

    class_addMethod(
      subclass,
      @selector(mouseDown:),
      (IMP)GhostexGpuiCEFBrowserViewMouseDown,
      "v@:@");
    class_addMethod(
      subclass,
      @selector(acceptsFirstResponder),
      (IMP)GhostexGpuiCEFBrowserViewAcceptsFirstResponder,
      "c@:");
    class_addMethod(
      subclass,
      @selector(selectAll:),
      (IMP)GhostexGpuiCEFBrowserViewSelectAll,
      "v@:@");
    class_addMethod(
      subclass,
      @selector(cut:),
      (IMP)GhostexGpuiCEFBrowserViewCut,
      "v@:@");
    class_addMethod(
      subclass,
      @selector(copy:),
      (IMP)GhostexGpuiCEFBrowserViewCopy,
      "v@:@");
    class_addMethod(
      subclass,
      @selector(paste:),
      (IMP)GhostexGpuiCEFBrowserViewPaste,
      "v@:@");
    class_addMethod(
      subclass,
      @selector(performKeyEquivalent:),
      (IMP)GhostexGpuiCEFBrowserViewPerformKeyEquivalent,
      "c@:@");
    class_addMethod(
      subclass,
      @selector(addSubview:),
      (IMP)GhostexGpuiCEFBrowserViewAddSubview,
      "v@:@");
    class_addMethod(
      subclass,
      @selector(viewDidChangeEffectiveAppearance),
      (IMP)GhostexGpuiCEFBrowserViewDidChangeEffectiveAppearance,
      "v@:");
    objc_registerClassPair(subclass);
  }

  object_setClass(view, subclass);
}

static void GhostexGpuiCEFBrowserViewMouseDown(id self, SEL _cmd, NSEvent* event) {
  NSWindow* window = [self window];
  if (window) {
    [window makeFirstResponder:self];
  }
  GhostexGpuiCEFMarkFocusedResponder(self);
  /*
   CDXC:GPUITitlebarDropdownCefDismissal 2026-07-15:
   CEF child views receive mouseDown before GPUI's main-window mouse capture,
   so clicking a Source/Browser/project-workarea pane cannot dismiss a GPUI
   titlebar popup through the normal outside-click route. Report the current
   first responder for every real CEF mouseDown, including when this view was
   already first responder and AppKit therefore emits no KVO transition. Rust
   uses the existing responder classification to dismiss app chrome; the
   original event continues unchanged to Chromium below.
  */
  if (window) {
    GhostexGpuiFirstResponderReportWindow(window);
  }

  struct objc_super superInfo = {
    .receiver = self,
    .super_class = class_getSuperclass(object_getClass(self)),
  };
  void (*sendSuper)(struct objc_super*, SEL, NSEvent*) = (void*)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd, event);
}

static BOOL GhostexGpuiCEFBrowserViewAcceptsFirstResponder(id self, SEL _cmd) {
  (void)self;
  (void)_cmd;
  return YES;
}

static void GhostexGpuiCEFBrowserViewSelectAll(id self, SEL _cmd, id sender) {
  /*
   CDXC:GPUICefEditCommands 2026-06-14-17:25:
   Cmd+A in focused CEF page text fields must stay inside Chromium after the GPUI address bar has previously owned focus. Implement the standard AppKit selectAll: command on the exact CEF NSView and delegate to cef-rs Frame::select_all, so macOS command dispatch uses Chromium selection semantics without a hidden hit-test layer or page-specific fallback.

   CDXC:GPUICefEditCommands 2026-06-14-17:25:
   CEF can deliver page clicks to descendant NSViews below the browser host returned by cef-rs. Install the focus subclass on the CEF view tree and resolve selectAll: by walking ancestor views back to the registered browser root, so command-key focus follows the actual Chromium child that received the click.
   */
  if (GhostexGpuiCEFHandleSelectAllForResponder(self)) {
    return;
  }

  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static void GhostexGpuiCEFBrowserViewForwardEditActionToSuper(id self, SEL _cmd, id sender) {
  Class superClass = class_getSuperclass(object_getClass(self));
  if (superClass && class_getInstanceMethod(superClass, _cmd)) {
    struct objc_super superInfo = {
      .receiver = self,
      .super_class = superClass,
    };
    void (*sendSuper)(struct objc_super*, SEL, id) = (void*)objc_msgSendSuper;
    sendSuper(&superInfo, _cmd, sender);
  }
}

/*
 CDXC:GPUICefEditCommands 2026-07-09:
 Standard Edit-menu actions (menu-bar clicks and nil-target responder-chain
 dispatch) must reach Chromium's frame edit commands on the exact CEF view
 tree, the same way selectAll: already does. Routing through the bridge —
 instead of relying on Chromium's own copy:/paste: responders deeper in the
 tree — also marks the per-event bridged flag so the app-level sendEvent
 mirror never issues the same clipboard command twice.
 */
static void GhostexGpuiCEFBrowserViewCut(id self, SEL _cmd, id sender) {
  if (GhostexGpuiCEFHandleEditCommandForResponder(self, GhostexGpuiCEFEditCommandCut)) {
    return;
  }
  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static void GhostexGpuiCEFBrowserViewCopy(id self, SEL _cmd, id sender) {
  if (GhostexGpuiCEFHandleEditCommandForResponder(self, GhostexGpuiCEFEditCommandCopy)) {
    return;
  }
  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static void GhostexGpuiCEFBrowserViewPaste(id self, SEL _cmd, id sender) {
  if (GhostexGpuiCEFHandleEditCommandForResponder(self, GhostexGpuiCEFEditCommandPaste)) {
    return;
  }
  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static BOOL GhostexGpuiCEFBrowserViewPerformKeyEquivalent(id self, SEL _cmd, NSEvent* event) {
  GhostexGpuiCEFZoomCommand zoomCommand = GhostexGpuiCEFZoomCommandForEvent(event);
  if (zoomCommand != GhostexGpuiCEFZoomCommandNone &&
      GhostexGpuiCEFHandleZoomCommandForResponder(self, zoomCommand)) {
    return YES;
  }

  if (GhostexGpuiCEFEventIsCommandA(event) &&
      GhostexGpuiCEFHandleSelectAllForResponder(self)) {
    return YES;
  }

  struct objc_super superInfo = {
    .receiver = self,
    .super_class = class_getSuperclass(object_getClass(self)),
  };
  BOOL (*sendSuper)(struct objc_super*, SEL, NSEvent*) = (void*)objc_msgSendSuper;
  BOOL handled = sendSuper(&superInfo, _cmd, event);

  GhostexGpuiCEFEditCommand clipboardCommand = GhostexGpuiCEFClipboardEditCommandForEvent(event);
  if (clipboardCommand == GhostexGpuiCEFEditCommandNone) {
    return handled;
  }
  if (handled) {
    // Chromium's own key-equivalent path already delivered this clipboard
    // chord to the renderer; mark it bridged so the sendEvent mirror does
    // not repeat the command.
    g_ghostexGpuiCEFEditCommandBridged = YES;
    return YES;
  }
  return GhostexGpuiCEFHandleEditCommandForResponder(self, clipboardCommand);
}

static void GhostexGpuiCEFBrowserViewAddSubview(id self, SEL _cmd, NSView* subview) {
  struct objc_super superInfo = {
    .receiver = self,
    .super_class = class_getSuperclass(object_getClass(self)),
  };
  void (*sendSuper)(struct objc_super*, SEL, NSView*) = (void*)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd, subview);
  GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(subview);
}

static void GhostexGpuiCEFBrowserViewDidChangeEffectiveAppearance(id self, SEL _cmd) {
  struct objc_super superInfo = {
    .receiver = self,
    .super_class = class_getSuperclass(object_getClass(self)),
  };
  void (*sendSuper)(struct objc_super*, SEL) = (void*)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd);
  if ([self isKindOfClass:NSView.class]) {
    GhostexGpuiCEFRefreshSystemPageAppearanceForView((NSView*)self);
  }
}

static BOOL GhostexGpuiCEFRefreshSystemPageAppearanceForView(NSView* view) {
  for (NSView* candidate = view; candidate; candidate = candidate.superview) {
    if (GhostexGpuiCEFRefreshSystemPageAppearanceForNativeView((__bridge void*)candidate)) {
      return YES;
    }
  }
  return NO;
}

typedef struct {
  unsigned short keyCode;
  unichar canonicalCharacter;
  BOOL functionModifier;
  BOOL numericPad;
} GhostexGpuiNavigationKeyNormalization;

static NSEvent* GhostexGpuiNormalizedNavigationKeyEvent(NSEvent* event) {
  if (!event || (event.type != NSEventTypeKeyDown && event.type != NSEventTypeKeyUp)) {
    return event;
  }

  // These physical keycodes are reserved navigation keys on every macOS
  // keyboard layout, so matching by keycode alone is safe. Cleanliness is
  // judged by the CGEvent unicode payload — the field TSM reads — because
  // the NSEvent-level characters always look correct for these keys.
  // Dirty events are rebuilt via keyEventWithType, whose derived CGEvent
  // carries an empty payload: the shape TSM treats as a normal function
  // key (doCommand dispatch) instead of committable text.
  static const GhostexGpuiNavigationKeyNormalization normalizations[] = {
    {123, NSLeftArrowFunctionKey, YES, YES},
    {124, NSRightArrowFunctionKey, YES, YES},
    {125, NSDownArrowFunctionKey, YES, YES},
    {126, NSUpArrowFunctionKey, YES, YES},
    {115, NSHomeFunctionKey, YES, NO},
    {119, NSEndFunctionKey, YES, NO},
    {116, NSPageUpFunctionKey, YES, NO},
    {121, NSPageDownFunctionKey, YES, NO},
    // Backspace/forward-delete carry raw layout payloads of 0x08/0x7F on
    // CGEvent-synthesized keyboards, which TSM commits as invisible text
    // instead of deleting. Backspace's canonical NSEvent character is DEL
    // (0x7F) with no function modifier.
    {51, 0x7F, NO, NO},
    {117, NSDeleteFunctionKey, YES, NO},
  };

  for (size_t i = 0; i < sizeof(normalizations) / sizeof(normalizations[0]); i++) {
    GhostexGpuiNavigationKeyNormalization entry = normalizations[i];
    if (event.keyCode != entry.keyCode) {
      continue;
    }

    unichar canonicalCharacter = entry.canonicalCharacter;
    UniChar payload[8];
    UniCharCount payloadLength = 0;
    CGEventRef cgEvent = event.CGEvent;
    if (cgEvent) {
      CGEventKeyboardGetUnicodeString(cgEvent, 8, &payloadLength, payload);
    }
    BOOL payloadClean =
      payloadLength == 0 ||
      (payloadLength == 1 && payload[0] == (UniChar)canonicalCharacter);
    if (payloadClean) {
      return event;
    }

    NSString* canonicalCharacters = [NSString stringWithCharacters:&canonicalCharacter
                                                             length:1];
    NSEventModifierFlags canonicalFlags = event.modifierFlags;
    if (entry.functionModifier) {
      canonicalFlags |= NSEventModifierFlagFunction;
    }
    if (entry.numericPad) {
      canonicalFlags |= NSEventModifierFlagNumericPad;
    }
    NSEvent* normalized = [NSEvent keyEventWithType:event.type
                                           location:event.locationInWindow
                                      modifierFlags:canonicalFlags
                                          timestamp:event.timestamp
                                       windowNumber:event.windowNumber
                                            context:nil
                                         characters:canonicalCharacters
                        charactersIgnoringModifiers:canonicalCharacters
                                          isARepeat:event.isARepeat
                                            keyCode:event.keyCode];
    return normalized ?: event;
  }

  return event;
}

static BOOL GhostexGpuiCEFEventIsCommandA(NSEvent* event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  NSEventModifierFlags modifiers = event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return NO;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  if (modifiers != 0) {
    return NO;
  }

  return [event.charactersIgnoringModifiers.lowercaseString isEqualToString:@"a"];
}

static GhostexGpuiCEFEditCommand GhostexGpuiCEFClipboardEditCommandForEvent(NSEvent* event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return GhostexGpuiCEFEditCommandNone;
  }

  NSEventModifierFlags modifiers = event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return GhostexGpuiCEFEditCommandNone;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  if (modifiers != 0) {
    return GhostexGpuiCEFEditCommandNone;
  }

  NSString* key = event.charactersIgnoringModifiers.lowercaseString;
  if ([key isEqualToString:@"x"]) {
    return GhostexGpuiCEFEditCommandCut;
  }
  if ([key isEqualToString:@"c"]) {
    return GhostexGpuiCEFEditCommandCopy;
  }
  if ([key isEqualToString:@"v"]) {
    return GhostexGpuiCEFEditCommandPaste;
  }
  return GhostexGpuiCEFEditCommandNone;
}

static GhostexGpuiCEFZoomCommand GhostexGpuiCEFZoomCommandForEvent(NSEvent* event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return GhostexGpuiCEFZoomCommandNone;
  }

  NSEventModifierFlags modifiers =
    event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return GhostexGpuiCEFZoomCommandNone;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  NSString* key = event.charactersIgnoringModifiers;
  if ((modifiers == 0 || modifiers == NSEventModifierFlagShift) &&
      ([key isEqualToString:@"="] || [key isEqualToString:@"+"])) {
    return GhostexGpuiCEFZoomCommandIn;
  }
  if (modifiers != 0) {
    return GhostexGpuiCEFZoomCommandNone;
  }
  if ([key isEqualToString:@"-"]) {
    return GhostexGpuiCEFZoomCommandOut;
  }
  if ([key isEqualToString:@"0"]) {
    return GhostexGpuiCEFZoomCommandReset;
  }
  return GhostexGpuiCEFZoomCommandNone;
}

static BOOL GhostexGpuiCEFHandleSelectAllForResponder(id responder) {
  if (![responder isKindOfClass:NSView.class]) {
    return NO;
  }

  for (NSView* view = (NSView*)responder; view; view = view.superview) {
    if (GhostexGpuiCEFHandleSelectAllForNativeView((__bridge void*)view)) {
      return YES;
    }
  }
  return NO;
}

static BOOL GhostexGpuiCEFHandleEditCommandForResponder(
  id responder,
  GhostexGpuiCEFEditCommand command) {
  if (command == GhostexGpuiCEFEditCommandNone || ![responder isKindOfClass:NSView.class]) {
    return NO;
  }

  for (NSView* view = (NSView*)responder; view; view = view.superview) {
    if (GhostexGpuiCEFHandleEditCommandForNativeView((__bridge void*)view, (int)command)) {
      g_ghostexGpuiCEFEditCommandBridged = YES;
      return YES;
    }
  }
  return NO;
}

static BOOL GhostexGpuiCEFHandleZoomCommandForResponder(
  id responder,
  GhostexGpuiCEFZoomCommand command) {
  if (command == GhostexGpuiCEFZoomCommandNone || ![responder isKindOfClass:NSView.class]) {
    return NO;
  }

  /*
   CDXC:GPUICefPaneZoomShortcuts 2026-07-14:
   Resolve page zoom only by walking from the exact CEF responder to its
   registered Browser/project-workarea root. This keeps Source, Browser,
   Kanban, Automate, and Docs zoom local to the focused pane without window
   event rerouting, overlays, hit-test changes, or stale active-view fallback.
   */
  for (NSView* view = (NSView*)responder; view; view = view.superview) {
    if (GhostexGpuiCEFHandleZoomCommandForNativeView((__bridge void*)view, (int)command)) {
      return YES;
    }
  }
  return NO;
}

static void GhostexGpuiCEFMarkFocusedResponder(id responder) {
  if (![responder isKindOfClass:NSView.class]) {
    return;
  }

  for (NSView* view = (NSView*)responder; view; view = view.superview) {
    if (GhostexGpuiCEFMarkNativeViewFocused((__bridge void*)view)) {
      return;
    }
  }
}

void GhostexGpuiCEFFocusNativeView(void* nativeView) {
  NSView* view = (__bridge NSView*)nativeView;
  if (!view) {
    return;
  }

  NSWindow* window = view.window;
  if (!window) {
    return;
  }

  /*
   CDXC:GPUICefFocusRouting 2026-06-14-18:05:
   CEF child views can remain the AppKit first responder after browser interaction. When the GPUI-owned address bar is clicked, return first-responder ownership to the exact GPUI parent view before focusing the GPUI input so typed keys edit the address field instead of continuing into Chromium.
   */
  if (!GhostexGpuiCEFMarkNativeViewFocused(nativeView)) {
    GhostexGpuiCEFClearActiveNativeView();
  }
  [window makeFirstResponder:view];
}
