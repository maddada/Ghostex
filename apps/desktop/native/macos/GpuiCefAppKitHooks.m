#import <AppKit/AppKit.h>
#import "GpuiKeyboardShortcuts.h"
#import "GpuiNavigationGestures.h"
#import "GpuiWindowCorners.h"
#import <Carbon/Carbon.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <QuartzCore/QuartzCore.h>
#import <os/lock.h>
#import <stdbool.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

// Read pointer state in the same top-left coordinates as GPUI pane bounds.
// Native children consume mouse moves, so GPUI's last event position can be stale.
bool GhostexGpuiCommandPanePointerActive(void *rootView, double x, double y,
                                        double width, double height) {
  NSView *root = (__bridge NSView *)rootView;
  if (!root || !root.window) return true;
  if (NSEvent.pressedMouseButtons != 0) return true;
  NSPoint screenPoint = NSEvent.mouseLocation;
  NSPoint windowPoint = [root.window convertPointFromScreen:screenPoint];
  NSPoint point = [root convertPoint:windowPoint fromView:nil];
  if (!root.isFlipped) point.y = NSMaxY(root.bounds) - point.y;
  return NSPointInRect(point, NSMakeRect(x, y, width, height));
}

void GhostexGpuiCEFRunMessageLoop(void);
int GhostexGpuiCEFHandleSelectAllForNativeView(void *nativeView);
int GhostexGpuiCEFHandleSelectAllForActiveNativeView(void);
int GhostexGpuiCEFHandleEditCommandForNativeView(void *nativeView, int command);
void GhostexGpuiCEFLogClipboardRoute(int command, int bridgedDuringDispatch,
                                     int responderWalkHandled,
                                     const char *responderClass);
void GhostexGpuiCEFLogDevToolsWindowActivation(int windowIsKey,
                                               int responderInsideNativeView,
                                               const char *responderClass);
int GhostexGpuiCEFHandleZoomCommandForNativeView(void *nativeView, int command);
int GhostexGpuiCEFMarkNativeViewFocused(void *nativeView);
int GhostexGpuiCEFTextIsStrayKeyCharacter(const uint16_t *units, size_t len);
void GhostexGpuiCEFLogNativeMouseDown(void *nativeView, double eventWindowX,
                                      double eventWindowY, double frameWindowX,
                                      double frameWindowY, double frameWidth,
                                      double frameHeight,
                                      double parentBoundsWidth,
                                      double parentBoundsHeight, int hidden,
                                      const char *responderClass);
void GhostexGpuiCEFClearActiveNativeView(void);
int GhostexGpuiCEFRefreshSystemPageAppearanceForNativeView(void *nativeView);
void GhostexGpuiCEFRefreshSystemPageAppearances(void);
void GhostexGpuiSidebarRevealFocusEditable(void *sidebar);
void GhostexGpuiFirstResponderDidChange(void *gpuiRootView, void *responder);
int GhostexGpuiKeyboardRouteNativeEvent(void *gpuiRootView, int action,
                                        uint32_t keyCode, uint64_t modifiers,
                                        const char *shortcutCharacters,
                                        const char *characters);
int GhostexGpuiKeyboardOwnerUsesRendererEditHotkeys(void *gpuiRootView);
int GhostexGpuiKeyboardOwnerUsesDocsEditorHotkeys(void *gpuiRootView);
int GhostexGpuiPerformTextInputHistoryCommand(void *gpuiRootView, int redo);
bool GhostexGpuiNativeViewContainsResponder(void *rootNativeView,
                                            void *responder);

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

static BOOL g_ghostexGpuiCEFApplicationHooksInstalled = NO;
static BOOL g_ghostexGpuiCEFHandlingSendEvent = NO;
static BOOL g_ghostexGpuiCEFEditCommandBridged = NO;
static BOOL g_ghostexGpuiCEFSelectAllBridged = NO;
static const void *GhostexGpuiFirstResponderObserverKey =
    &GhostexGpuiFirstResponderObserverKey;
static const void *GhostexGpuiRootPointerTrackingAreaKey =
    &GhostexGpuiRootPointerTrackingAreaKey;
static const void *GhostexGpuiCEFPinchZoomDisabledKey =
    &GhostexGpuiCEFPinchZoomDisabledKey;
static BOOL g_ghostexGpuiCEFMessageLoopStartPending = NO;
static BOOL g_ghostexGpuiCEFMessageLoopRunning = NO;

static BOOL GhostexGpuiCEFResizeDiagnosticsEnabled(void) {
  static dispatch_once_t onceToken;
  static BOOL enabled = NO;
  dispatch_once(&onceToken, ^{
    enabled = getenv("GHOSTEX_GPUI_CEF_RESIZE_DIAGNOSTICS") != NULL;
  });
  return enabled;
}

static void GhostexGpuiCEFInstallStandardEditMenu(void);
static void GhostexGpuiCEFBrowserViewMouseDown(id self, SEL _cmd,
                                               NSEvent *event);
static BOOL GhostexGpuiCEFBrowserViewAcceptsFirstResponder(id self, SEL _cmd);
static void GhostexGpuiCEFBrowserViewSelectAll(id self, SEL _cmd, id sender);
static BOOL GhostexGpuiCEFBrowserViewPerformKeyEquivalent(id self, SEL _cmd,
                                                          NSEvent *event);
static void GhostexGpuiCEFBrowserViewAddSubview(id self, SEL _cmd,
                                                NSView *subview);
static void GhostexGpuiCEFBrowserViewDidChangeEffectiveAppearance(id self,
                                                                  SEL _cmd);
static void GhostexGpuiCEFInstallBrowserViewFocusSubclass(NSView *view);
static void GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(NSView *view);
static Class GhostexGpuiCEFFocusSuperclass(id self);
static void GhostexGpuiCEFBrowserViewCut(id self, SEL _cmd, id sender);
static void GhostexGpuiCEFBrowserViewCopy(id self, SEL _cmd, id sender);
static void GhostexGpuiCEFBrowserViewPaste(id self, SEL _cmd, id sender);
static void GhostexGpuiCEFBrowserViewInsertText(id self, SEL _cmd, id string,
                                                NSRange replacementRange);
static BOOL GhostexGpuiCEFEventIsCommandA(NSEvent *event);
static BOOL GhostexGpuiCEFEventIsCommandF(NSEvent *event);
static BOOL GhostexGpuiCEFEventIsCommandOptionF(NSEvent *event);
static BOOL GhostexGpuiCEFEventIsCommandY(NSEvent *event);
static BOOL GhostexGpuiCEFEventIsHistoryChord(NSEvent *event);
static BOOL GhostexGpuiCEFEventIsWebTextEditingKey(NSEvent *event);
static GhostexGpuiCEFEditCommand
GhostexGpuiCEFClipboardEditCommandForEvent(NSEvent *event);
static GhostexGpuiCEFZoomCommand
GhostexGpuiCEFZoomCommandForEvent(NSEvent *event);
static BOOL GhostexGpuiCEFHandleSelectAllForResponder(id responder);
static BOOL
GhostexGpuiCEFHandleEditCommandForResponder(id responder,
                                            GhostexGpuiCEFEditCommand command);
static BOOL
GhostexGpuiCEFHandleZoomCommandForResponder(id responder,
                                            GhostexGpuiCEFZoomCommand command);
static void GhostexGpuiCEFBrowserViewForwardEditActionToSuper(id self, SEL _cmd,
                                                              id sender);
static NSView *GhostexGpuiCEFMarkFocusedResponder(id responder);
static BOOL GhostexGpuiCEFRefreshSystemPageAppearanceForView(NSView *view);
static NSEvent *GhostexGpuiNormalizedNavigationKeyEvent(NSEvent *event);
static void GhostexGpuiFirstResponderReportWindow(NSWindow *window);
static void GhostexGpuiInstallRootPointerTrackingArea(NSView *view);
static void GhostexGpuiSidebarPointerTrackingObserveEvent(NSEvent *event);
static void GhostexGpuiSidebarPointerTrackingReport(BOOL inside);
static BOOL
GhostexGpuiSidebarPointerTrackingContainsScreenPoint(NSPoint screenPoint);

/*
 CDXC:Sidebar 2026-08-02:
 The sidebar CEF surface is a native sibling of GPUI chrome, Ghostty terminal
 hosts, and other CEF panes, so its renderer never receives a reliable
 mouse-leave when the pointer crosses into one of those siblings: Chromium can
 keep the last :hover row (and its hover-only Close button) painted, and an
 open sidebar context menu cannot see clicks that land outside its own
 document. NSApplication's sendEvent: is the app's single mouse-event entry
 point for every window, so observe — never reroute — mouse moves and downs
 here, compare the location against the registered sidebar view's frame, and
 report inside/outside transitions plus outside mouse-downs to Rust. Rust
 forwards both into the sidebar page (the existing
 data-native-pointer-inside CSS contract and context-menu dismissal).
*/
static __weak NSView *g_ghostexGpuiSidebarPointerTrackingView = nil;
static BOOL g_ghostexGpuiSidebarUsesNativeBounds = NO;
static NSRect g_ghostexGpuiSidebarNativeBounds;

static NSRect GhostexGpuiSidebarTrackingRect(NSView *view) {
  return g_ghostexGpuiSidebarUsesNativeBounds
      ? NSIntersectionRect(view.visibleRect, g_ghostexGpuiSidebarNativeBounds)
      : view.visibleRect;
}

/*
 CDXC:Sidebar 2026-08-20:
 This is "what the sidebar page was last told", and it exists to keep the
 high-frequency mouse-moved path from running a script per event. It must
 therefore have an honest `Unknown` value: any moment the page's copy can no
 longer be derived from it (the tracking view was re-registered for a new CEF
 surface, the window went inactive so crossings stop being observable) has to
 be representable, or the next observed event compares against a stale value,
 suppresses the write as redundant, and the page keeps a flag that contradicts
 the real pointer position for as long as the pointer stays on that side of
 the boundary.

 That is exactly what used to happen: window deactivation reported "outside"
 straight into the page while this cache still said "inside", so moving back
 over a session row reported nothing, `data-native-pointer-inside` stayed
 "false", and every hover suppressor in the sidebar stylesheet stayed armed —
 no row background, no hover-only Close button — even though Chromium's own
 :hover was tracking the pointer correctly. Every writer now goes through
 GhostexGpuiSidebarPointerTrackingReport so the cache cannot drift.
*/
typedef NS_ENUM(NSInteger, GhostexGpuiSidebarPointerTrackingState) {
  GhostexGpuiSidebarPointerTrackingStateUnknown = 0,
  GhostexGpuiSidebarPointerTrackingStateOutside,
  GhostexGpuiSidebarPointerTrackingStateInside,
};

static GhostexGpuiSidebarPointerTrackingState g_ghostexGpuiSidebarPointerState =
    GhostexGpuiSidebarPointerTrackingStateUnknown;

extern void GhostexGpuiSidebarPointerInsideChanged(bool inside);
extern void GhostexGpuiSidebarOutsideMouseDown(void);

// CDXC:Sidebar 2026-10-18 WHY:
// Never set acceptsMouseMovedEvents on the sidebar's window. AppKit then hands the window pointer moves for points another window covers, so with Settings open over the main window its left menu lit up the sidebar rows behind it. GPUI's own tracking area already delivers moves only while the window is the topmost one under the pointer, and this observer sees those same moves in sendEvent.
void GhostexGpuiCEFSetSidebarPointerTrackingView(void *view) {
  NSView *sidebarView = (__bridge NSView *)view;
  g_ghostexGpuiSidebarUsesNativeBounds = NO;
  g_ghostexGpuiSidebarPointerTrackingView = sidebarView;
  // Unknown until the next mouse event recomputes it against the new view.
  g_ghostexGpuiSidebarPointerState =
      GhostexGpuiSidebarPointerTrackingStateUnknown;
}

// CDXC:Sidebar 2026-09-17 WHY:
// Embedded browser panes consume clicks before GPUI receives them. Track the rendered sidebar rectangle in its GPUI view so those clicks still dismiss native menus and clear stale hover.
void GhostexGpuiNativeSidebarSetTrackingBounds(void *view, double x, double y, double width, double height) {
  NSView *nativeView = (__bridge NSView *)view;
  if (nativeView != g_ghostexGpuiSidebarPointerTrackingView) {
    GhostexGpuiCEFSetSidebarPointerTrackingView(view);
  }
  g_ghostexGpuiSidebarUsesNativeBounds = YES;
  g_ghostexGpuiSidebarNativeBounds = NSMakeRect(x, nativeView.isFlipped ? y : NSHeight(nativeView.bounds) - y - height, width, height);
}

static void GhostexGpuiSidebarPointerTrackingReport(BOOL inside) {
  GhostexGpuiSidebarPointerTrackingState next =
      inside ? GhostexGpuiSidebarPointerTrackingStateInside
             : GhostexGpuiSidebarPointerTrackingStateOutside;
  if (next == g_ghostexGpuiSidebarPointerState) {
    return;
  }
  g_ghostexGpuiSidebarPointerState = next;
  GhostexGpuiSidebarPointerInsideChanged(inside);
}

/*
 CDXC:Sidebar 2026-09-09 DECISION:
 User: retain native stale-hover protection, but determine pointer presence from the visible sidebar's screen position and refresh it as the floating panel opens, animates, and closes.
 A floating sidebar lives in a nonactivating child window; the event's window identity must not gate hover presence.
 SEE-ALSO: GpuiSidebarReveal.m refreshes this same reporter when the panel moves under a stationary pointer.
*/
static BOOL
GhostexGpuiSidebarPointerTrackingContainsScreenPoint(NSPoint screenPoint) {
  NSView *sidebarView = g_ghostexGpuiSidebarPointerTrackingView;
  NSWindow *window = sidebarView.window;
  if (!sidebarView || !window || !window.isVisible || window.miniaturized ||
      !NSApp.active || sidebarView.isHiddenOrHasHiddenAncestor) {
    return NO;
  }
  NSRect frameInWindow = [sidebarView convertRect:GhostexGpuiSidebarTrackingRect(sidebarView)
                                           toView:nil];
  NSRect contentInWindow = [window.contentView convertRect:window.contentView.bounds
                                                   toView:nil];
  NSRect visibleFrame = NSIntersectionRect(frameInWindow, contentInWindow);
  if (NSIsEmptyRect(visibleFrame) ||
      !NSPointInRect(screenPoint, [window convertRectToScreen:visibleFrame])) return NO;
  // A menu or another window covering this rectangle must still clear stale hover.
  return [NSWindow windowNumberAtPoint:screenPoint belowWindowWithWindowNumber:0] == window.windowNumber;
}

/*
 CDXC:Sidebar 2026-08-20:
 The workspace window stopped being active, so mouse-moved events stop
 arriving and a crossing out of the sidebar can no longer be observed. Report
 the pointer as outside — the same thing leaving for another app does to a
 native menu — through the shared reporter so the cache records that the page
 now holds "false".
*/
void GhostexGpuiCEFReportSidebarPointerOutside(void) {
  GhostexGpuiSidebarPointerTrackingReport(NO);
}

/*
 CDXC:Sidebar 2026-08-20:
 The workspace window became active again. The pointer may already be parked
 over a session row, and a pointer that never moves produces no event to
 recompute from, so resolve the crossing from the real pointer location rather
 than waiting for the user to jiggle the mouse.
*/
void GhostexGpuiCEFRefreshSidebarPointerInside(void) {
  GhostexGpuiSidebarPointerTrackingReport(
      GhostexGpuiSidebarPointerTrackingContainsScreenPoint(
          NSEvent.mouseLocation));
}

static void GhostexGpuiSidebarPointerTrackingObserveEvent(NSEvent *event) {
  NSEventType type = event.type;
  BOOL isMove = type == NSEventTypeMouseMoved ||
                type == NSEventTypeLeftMouseDragged ||
                type == NSEventTypeRightMouseDragged ||
                type == NSEventTypeOtherMouseDragged;
  BOOL isDown = type == NSEventTypeLeftMouseDown ||
                type == NSEventTypeRightMouseDown ||
                type == NSEventTypeOtherMouseDown;
  if (!isMove && !isDown) {
    return;
  }
  NSView *sidebarView = g_ghostexGpuiSidebarPointerTrackingView;
  if (!sidebarView) {
    return;
  }
  NSWindow *window = event.window;
  BOOL inside = GhostexGpuiSidebarPointerTrackingContainsScreenPoint(NSEvent.mouseLocation);
  GhostexGpuiSidebarPointerTrackingReport(inside);
  if (isDown) {
    /*
     CDXC:Sidebar 2026-09-10 WHY:
     Hover uses the live pointer, but dismissal must use the mouse-down event's location: the pointer can already be back over the sidebar when a queued Agents-area click is processed.
     The event's window also distinguishes clicks in child windows covering the sidebar, without depending on current hover or application activation state.
    */
    BOOL clickedSidebar = window && window == sidebarView.window &&
        !sidebarView.isHiddenOrHasHiddenAncestor &&
        NSPointInRect([sidebarView convertPoint:event.locationInWindow fromView:nil],
                      GhostexGpuiSidebarTrackingRect(sidebarView));
    /*
     CDXC:Theming 2026-09-25 WHY:
     Under window glass the sidebar's own menu draws in a frosted child window over the sidebar
     (app/window/frosted_host.rs). A press on it is a press on the menu, not outside it: dismissing
     here closed the menu on mouse-down, before its row could see the click.
    */
    BOOL clickedSidebarMenu =
        [window.identifier isEqualToString:@"ghostex.frostedSidebarMenu"];
    if (!clickedSidebar && !clickedSidebarMenu) {
      GhostexGpuiSidebarOutsideMouseDown();
    }
  }
}

@interface GhostexGpuiFirstResponderObserver : NSObject
@property(nonatomic, weak) NSWindow *window;
@property(nonatomic, weak) NSView *gpuiRootView;
- (instancetype)initWithWindow:(NSWindow *)window
                  gpuiRootView:(NSView *)gpuiRootView;
@end

NSView *GhostexGpuiNavigationRootForView(NSView *view) {
  NSWindow *window = view.window;
  GhostexGpuiFirstResponderObserver *observer =
      objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey);
  NSView *root = observer.gpuiRootView;
  return root && root.window == window ? root : nil;
}

@implementation GhostexGpuiFirstResponderObserver
- (instancetype)initWithWindow:(NSWindow *)window
                  gpuiRootView:(NSView *)gpuiRootView {
  self = [super init];
  if (!self) {
    return nil;
  }
  _window = window;
  _gpuiRootView = gpuiRootView;
  [window addObserver:self
           forKeyPath:@"firstResponder"
              options:NSKeyValueObservingOptionInitial |
                      NSKeyValueObservingOptionNew
              context:NULL];
  return self;
}

- (void)dealloc {
  @try {
    [_window removeObserver:self forKeyPath:@"firstResponder"];
  } @catch (__unused NSException *exception) {
  }
}

- (void)observeValueForKeyPath:(NSString *)keyPath
                      ofObject:(id)object
                        change:(NSDictionary<NSKeyValueChangeKey, id> *)change
                       context:(void *)context {
  (void)change;
  (void)context;
  if ([keyPath isEqualToString:@"firstResponder"] &&
      [object isKindOfClass:NSWindow.class]) {
    GhostexGpuiFirstResponderReportWindow((NSWindow *)object);
    return;
  }
  [super observeValueForKeyPath:keyPath
                       ofObject:object
                         change:change
                        context:context];
}
@end

static BOOL
GhostexGpuiCEFRendererEditHotkeysOwnKeyboardInWindow(NSWindow *window) {
  GhostexGpuiFirstResponderObserver *observer =
      objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey);
  NSView *gpuiRootView = observer.gpuiRootView;
  return gpuiRootView && gpuiRootView.window == window &&
         GhostexGpuiKeyboardOwnerUsesRendererEditHotkeys(
             (__bridge void *)gpuiRootView) != 0;
}

static BOOL
GhostexGpuiCEFDocsEditorHotkeysOwnKeyboardInWindow(NSWindow *window) {
  GhostexGpuiFirstResponderObserver *observer =
      objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey);
  NSView *gpuiRootView = observer.gpuiRootView;
  return gpuiRootView && gpuiRootView.window == window &&
         GhostexGpuiKeyboardOwnerUsesDocsEditorHotkeys(
             (__bridge void *)gpuiRootView) != 0;
}

static BOOL GhostexGpuiResponderIsChromiumContent(id responder) {
  static Class contentClass = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    contentClass = NSClassFromString(@"RenderWidgetHostViewCocoa");
  });
  return contentClass && [responder isKindOfClass:contentClass];
}

/*
 CDXC:UndoRedo 2026-09-25 DECISION:
 User: Edit > Undo and Redo in the menu bar must work everywhere. A nil-target undo: reaches NSWindow before GPUI's app delegate, and NSWindow's undo manager is always empty, so the items were greyed out for every native text field. These items target this object instead: a focused web page gets Chromium's own undo, anything else goes to the focused GPUI text field.
 */
@interface GhostexGpuiEditHistoryMenuTarget : NSObject
@end

@implementation GhostexGpuiEditHistoryMenuTarget
- (void)ghostexPerformHistoryCommand:(BOOL)redo sender:(id)sender {
  NSWindow *window = NSApp.keyWindow;
  id responder = window.firstResponder;
  SEL selector = redo ? @selector(redo:) : @selector(undo:);
  if (GhostexGpuiResponderIsChromiumContent(responder)) {
    if ([responder respondsToSelector:selector]) {
      ((void (*)(id, SEL, id))objc_msgSend)(responder, selector, sender);
    }
    return;
  }
  GhostexGpuiFirstResponderObserver *observer =
      window ? objc_getAssociatedObject(window,
                                        GhostexGpuiFirstResponderObserverKey)
             : nil;
  GhostexGpuiPerformTextInputHistoryCommand(
      (__bridge void *)observer.gpuiRootView, redo ? 1 : 0);
}

- (void)undo:(id)sender {
  [self ghostexPerformHistoryCommand:NO sender:sender];
}

- (void)redo:(id)sender {
  [self ghostexPerformHistoryCommand:YES sender:sender];
}

- (BOOL)validateMenuItem:(NSMenuItem *)item {
  (void)item;
  return NSApp.keyWindow != nil;
}
@end

static GhostexGpuiEditHistoryMenuTarget *
GhostexGpuiEditHistoryMenuTargetShared(void) {
  static GhostexGpuiEditHistoryMenuTarget *target = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    target = [[GhostexGpuiEditHistoryMenuTarget alloc] init];
  });
  return target;
}

/*
 CDXC:CefRuntime 2026-06-14-16:14:
 CEF's macOS external-run-loop path requires NSApplication to conform to
 CefAppProtocol before Chromium installs its CFRunLoop observers. Mirror the
 protocol definitions from cef_application_mac.h locally so this lightweight
 cef-rs shim can register the Objective-C category at load time without
 restoring a direct CEF C++ header dependency.
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
- (void)ghostexGpuiCEFSendEvent:(NSEvent *)event;
@end

@implementation NSApplication (GhostexGpuiCEFApplication)
+ (void)load {
  Method originalSendEvent =
      class_getInstanceMethod(self, @selector(sendEvent:));
  Method cefSendEvent =
      class_getInstanceMethod(self, @selector(ghostexGpuiCEFSendEvent:));
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

- (void)ghostexGpuiCEFSendEvent:(NSEvent *)event {
  /*
   CDXC:Hotkeys 2026-07-04:
   CGEvent-synthesized keyboards (Karabiner's virtual HID, BetterTouchTool,
   CGEvent-posting automation) deliver arrow/Home/End/PageUp/PageDown and
   backspace/forward-delete key events whose underlying CGEvent unicode
   payload is the raw layout translation of those keys: legacy control
   codes (0x1C-0x1F for arrows, 0x08/0x7F for the delete keys, 0x10 for
   F1-F20). The NSEvent fields still read as the normal F700-range
   function-key characters, but macOS's async TSM input-method path reads
   the raw CGEvent payload and commits it as literal text to the focused
   NSTextInputClient without any keyDown dispatch — inserting invisible
   control characters into terminals, CEF inputs, and GPUI inputs alike.
   Normalize once here at the app's single event entry point so every
   downstream view sees the payload-free event shape a hardware key
   produces.
   */
  event = GhostexGpuiNormalizedNavigationKeyEvent(event);

  // Observe-only sidebar pointer tracking (see GPUISidebarPointerTracking
  // above); the event continues unchanged to its normal target.
  GhostexGpuiSidebarPointerTrackingObserveEvent(event);

  /*
   CDXC:Hotkeys 2026-07-24:
   GPUI, native Ghostty, and CEF use separate responder systems. Offer key
   press/repeat/release events once to the window-scoped Rust router before
   AppKit key-equivalent traversal. The router claims only an application
   command, a configured Ghostex hotkey allowed for the exact owner, or the
   composited-terminal Tab lifecycle that AppKit otherwise removes before
   GPUI sees it. Every unclaimed event continues unchanged through the normal
   responder chain.
   */
  if (event.type == NSEventTypeKeyDown || event.type == NSEventTypeKeyUp) {
    NSWindow *window = event.window ?: NSApp.keyWindow;
    GhostexGpuiFirstResponderObserver *observer =
        objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey);
    NSView *gpuiRootView = observer.gpuiRootView;
    int action = event.type == NSEventTypeKeyUp ? 3 : (event.isARepeat ? 2 : 1);
    NSString *shortcutCharacters = GhostexGpuiShortcutCharactersForEvent(event);
    NSString *characters = event.characters ?: @"";
    if (gpuiRootView && gpuiRootView.window == window &&
        GhostexGpuiKeyboardRouteNativeEvent(
            (__bridge void *)gpuiRootView, action, (uint32_t)event.keyCode,
            (uint64_t)event.modifierFlags, shortcutCharacters.UTF8String,
            characters.UTF8String) != 0) {
      return;
    }
  }

  /*
   CDXC:Hotkeys 2026-08-09:
   GPUI and AppKit both have process-level meanings for editor chords before
   Chromium's renderer sees them. When the exact Docs workarea is the proven
   native keyboard owner, deliver its VS Code-compatible Command+F Find,
   Option+Command+F Replace, and Command+Y Redo chords through the focused
   Chromium responder's normal keyDown path before key-equivalent traversal.
   The Rust router explicitly leaves these same chords unclaimed for Docs.
   Other surfaces keep their normal application and browser shortcuts,
   including macOS Command+H Hide.
   */
  NSWindow *docsShortcutWindow = event.window ?: NSApp.keyWindow;
  if (event.type == NSEventTypeKeyDown &&
      GhostexGpuiCEFDocsEditorHotkeysOwnKeyboardInWindow(docsShortcutWindow) &&
      (GhostexGpuiCEFEventIsCommandF(event) ||
       GhostexGpuiCEFEventIsCommandOptionF(event) ||
       GhostexGpuiCEFEventIsCommandY(event))) {
    id responder = docsShortcutWindow.firstResponder;
    if (responder && [responder respondsToSelector:@selector(keyDown:)]) {
      [responder keyDown:event];
      return;
    }
  }

  /*
   CDXC:UndoRedo 2026-09-25 DECISION:
   User: when a web page has focus, Cmd+Z and Cmd+Shift+Z go to the page first, so web apps with their own undo (Google Docs, Figma) get them, and Chromium's own undo runs only when the page leaves the key alone. AppKit fires the Edit menu's key equivalent before Chromium sees the key, and GPUI can still hold focus on a hidden native field, so hand the chord straight to the focused Chromium view. An unhandled chord comes back through CEF to the main menu, whose Undo/Redo target runs Chromium's undo.
   */
  NSWindow *historyWindow = event.window ?: NSApp.keyWindow;
  if (GhostexGpuiCEFEventIsHistoryChord(event) &&
      GhostexGpuiResponderIsChromiumContent(historyWindow.firstResponder)) {
    [historyWindow.firstResponder keyDown:event];
    return;
  }

  /*
   CDXC:Hotkeys 2026-10-25 WHY:
   AppKit offers an arrow, Home/End, PageUp/PageDown, Delete or Forward Delete press to performKeyEquivalent: first, and the window's GPUIView, the parent of every CEF view, hands any key that types no text and has no Control or Command to its own input context, which reports it handled. Chromium's keyDown never ran, and the input method committed the key as a blank character into the focused web text field: in the page window a question form's note box got a space for every arrow, Option+Left or Option+Delete instead of moving the caret or deleting a word. With a Chromium view as first responder these keys, bare or with Shift and Option, go straight to its keyDown, the path a key that is no key equivalent takes, so Chromium and any input method composing in the page edit as usual. This supersedes the 2026-10-24 fix that covered only the bare keys. The router above has already claimed any configured Ghostex hotkey; Control and Command variants never reach GPUIView's input context and keep their normal path, and Escape and the F-keys stay with GPUI, which binds them.
   */
  NSWindow *navigationWindow = event.window ?: NSApp.keyWindow;
  if (GhostexGpuiCEFEventIsWebTextEditingKey(event) &&
      GhostexGpuiResponderIsChromiumContent(navigationWindow.firstResponder)) {
    [navigationWindow.firstResponder keyDown:event];
    return;
  }

  /*
   CDXC:Hotkeys 2026-06-14-17:25:
   GPUI can keep its address-input focus handle after Chromium has accepted a
   page click, so AppKit command-key dispatch may never invoke selectAll: on
   CEF's responder chain. When the active native target is a registered CEF
   view, mirror only Cmd+A in the existing CEF NSApplication sendEvent hook and
   call Chromium's Frame::select_all after normal dispatch; GPUI chrome clicks
   clear that active target before their own text shortcuts run.
   */
  /*
   CDXC:Hotkeys 2026-08-09:
   Renderer editors such as Monaco own Cmd+A as a renderer keybinding.
   Chromium's generic Frame::select_all edit command does not execute that
   keybinding, so the app-wide CEF Select All mirror must stay out of those
   events once the window keyboard router confirms that exact surface owns
   first responder. Other CEF surfaces retain the mirror used by ordinary web
   text controls.
   */
  BOOL shouldSelectAllInActiveCEF =
      GhostexGpuiCEFEventIsCommandA(event) &&
      !GhostexGpuiCEFRendererEditHotkeysOwnKeyboardInWindow(
          event.window ?: NSApp.keyWindow);

  /*
   CDXC:Hotkeys 2026-07-09:
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
  GhostexGpuiCEFEditCommand clipboardCommand =
      GhostexGpuiCEFClipboardEditCommandForEvent(event);

  BOOL wasHandlingSendEvent = g_ghostexGpuiCEFHandlingSendEvent;
  BOOL wasEditCommandBridged = g_ghostexGpuiCEFEditCommandBridged;
  BOOL wasSelectAllBridged = g_ghostexGpuiCEFSelectAllBridged;
  g_ghostexGpuiCEFHandlingSendEvent = YES;
  g_ghostexGpuiCEFEditCommandBridged = NO;
  g_ghostexGpuiCEFSelectAllBridged = NO;
  @try {
    [self ghostexGpuiCEFSendEvent:event];
  } @finally {
    g_ghostexGpuiCEFHandlingSendEvent = wasHandlingSendEvent;
  }

  /*
   CDXC:Hotkeys 2026-08-18:
   The mirror exists for the case where GPUI chrome still holds the focus
   handle, so it resolves its target from the last-active CEF registry rather
   than the real first responder. Repeating it after normal dispatch already
   bridged Select All would re-issue the command against a possibly different
   surface and move native focus there, which blurs the field the user was
   typing in (a sidebar rename commits and closes on that blur). Skip the
   mirror whenever the responder chain already delivered the command.
   */
  BOOL selectAllBridgedDuringDispatch = g_ghostexGpuiCEFSelectAllBridged;
  g_ghostexGpuiCEFSelectAllBridged = wasSelectAllBridged;
  if (shouldSelectAllInActiveCEF && !selectAllBridgedDuringDispatch) {
    GhostexGpuiCEFHandleSelectAllForActiveNativeView();
  }

  BOOL bridgedDuringDispatch = g_ghostexGpuiCEFEditCommandBridged;
  BOOL responderWalkHandled = NO;
  NSWindow *clipboardWindow = event.window ?: NSApp.keyWindow;
  if (clipboardCommand != GhostexGpuiCEFEditCommandNone &&
      !bridgedDuringDispatch) {
    responderWalkHandled = GhostexGpuiCEFHandleEditCommandForResponder(
        clipboardWindow.firstResponder, clipboardCommand);
  }
  if (clipboardCommand != GhostexGpuiCEFEditCommandNone) {
    id responder = clipboardWindow.firstResponder;
    GhostexGpuiCEFLogClipboardRoute(
        (int)clipboardCommand, bridgedDuringDispatch ? 1 : 0,
        responderWalkHandled ? 1 : 0,
        responder ? object_getClassName(responder) : NULL);
  }
  g_ghostexGpuiCEFEditCommandBridged = wasEditCommandBridged;
}
@end

void GhostexGpuiCEFPrepareApplication(void) {
  @autoreleasepool {
    NSUserDefaults *defaults = [NSUserDefaults standardUserDefaults];
    NSMutableDictionary *argumentDefaults =
        [[defaults volatileDomainForName:NSArgumentDomain] mutableCopy]
            ?: [NSMutableDictionary dictionary];
    /*
     CDXC:CefRuntime 2026-06-14-15:25:
     The GPUI CEF shell is launched repeatedly while Chromium embedding is under
     construction. Disable AppKit's crash-state restoration prompts in the
     process argument domain so a saved-state modal cannot block the first GPUI
     frame or the deferred CEF initialization path.
     */
    argumentDefaults[@"ApplePersistenceIgnoreState"] = @YES;
    argumentDefaults[@"NSQuitAlwaysKeepsWindows"] = @NO;
    [defaults setVolatileDomain:argumentDefaults forName:NSArgumentDomain];

    /*
     CDXC:Browser 2026-09-08 WHY:
     A fixed host appearance can suppress viewDidChangeEffectiveAppearance when the OS theme changes.
     Observe the system theme directly so every Browser tab, including hidden tabs, gets the new media preference.
     */
    static dispatch_once_t appearanceObserverOnce;
    dispatch_once(&appearanceObserverOnce, ^{
      [[NSDistributedNotificationCenter defaultCenter]
          addObserverForName:@"AppleInterfaceThemeChangedNotification"
                      object:nil
                       queue:NSOperationQueue.mainQueue
                  usingBlock:^(NSNotification *notification) {
                    (void)notification;
                    GhostexGpuiCEFRefreshSystemPageAppearances();
                  }];
    });
  }
}

bool GhostexGpuiCEFSystemUsesDarkPageAppearance(void) {
  NSDictionary *systemDefaults = [[NSUserDefaults standardUserDefaults]
      persistentDomainForName:NSGlobalDomain];
  return [systemDefaults[@"AppleInterfaceStyle"] isEqual:@"Dark"];
}

static void GhostexGpuiCEFScheduleMessageLoopStart(CFTimeInterval delay) {
  CFRunLoopTimerRef timer = CFRunLoopTimerCreateWithHandler(
      kCFAllocatorDefault, CFAbsoluteTimeGetCurrent() + delay, 0, 0, 0,
      ^(CFRunLoopTimerRef firedTimer) {
        (void)firedTimer;
        if (!g_ghostexGpuiCEFMessageLoopStartPending) {
          return;
        }
        if (NSApp.modalWindow != nil) {
          GhostexGpuiCEFScheduleMessageLoopStart(0.25);
          return;
        }
        g_ghostexGpuiCEFMessageLoopStartPending = NO;
        g_ghostexGpuiCEFMessageLoopRunning = YES;
        GhostexGpuiCEFRunMessageLoop();
        g_ghostexGpuiCEFMessageLoopRunning = NO;
      });
  CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopDefaultMode);
  CFRelease(timer);
}

/*
 CDXC:CefRuntime 2026-10-17 WHY:
 Chromium's message loop runs inside GPUI's NSApplication loop for the rest of
 the process: it dispatches the app's events itself (the same
 nextEventMatchingMask/sendEvent loop Chrome uses for nested loops) and does
 its work from its own run-loop sources, timers and before-wait observer.
 The timer's callout (GhostexGpuiCEFRunMessageLoop in cef/macos.rs) first runs
 cef::initialize and then enters the loop, so the loop pass that was running
 when Chromium's pump installed its observers never ends while the process
 lives (an unmatched exit is a CHECK crash; see start_cef in cef/macos.rs).
 Start it from a default-mode run-loop timer, never from a main-queue block:
 GPUI's foreground tasks are main-queue blocks, and CFRunLoop does not drain
 the main queue in a nested loop that a main-queue block started, so GPUI
 would stop. Default mode also keeps it out of menu tracking, live resize and
 modal panels, which would otherwise never end.
 */
void GhostexGpuiCEFStartMessageLoop(void) {
  if (g_ghostexGpuiCEFMessageLoopStartPending ||
      g_ghostexGpuiCEFMessageLoopRunning) {
    return;
  }
  g_ghostexGpuiCEFMessageLoopStartPending = YES;
  GhostexGpuiCEFScheduleMessageLoopStart(0);
  CFRunLoopWakeUp(CFRunLoopGetMain());
}

void GhostexGpuiCEFCancelMessageLoopStart(void) {
  g_ghostexGpuiCEFMessageLoopStartPending = NO;
}

// CDXC:AppWindows 2026-10-01 WHY:
// GPUI makes the menu named "Window" the app's windows menu, and AppKit then lists every titled
// window in it on its own, beside the rows Ghostex builds for its workspace windows
// (helpers/os_cli/main_menus.rs): each window twice, plus Settings and the other dialogs.
// Ghostex keeps its own rows and takes the menu back from AppKit after every menu install,
// removing what AppKit had already added to it.
static void GhostexGpuiDetachSystemWindowsMenu(void) {
  NSMenu *windowsMenu = NSApp.windowsMenu;
  if (windowsMenu == nil) {
    return;
  }
  NSApp.windowsMenu = nil;
  for (NSMenuItem *item in [windowsMenu.itemArray copy]) {
    if (item.action == @selector(makeKeyAndOrderFront:) ||
        item.action == @selector(arrangeInFront:)) {
      [windowsMenu removeItem:item];
    }
  }
  while (windowsMenu.numberOfItems > 0 &&
         windowsMenu.itemArray.lastObject.isSeparatorItem) {
    [windowsMenu removeItemAtIndex:windowsMenu.numberOfItems - 1];
  }
}

void GhostexGpuiCEFInstallApplicationHooks(void) {
  GhostexGpuiCEFInstallStandardEditMenu();
  GhostexGpuiDetachSystemWindowsMenu();
  if (g_ghostexGpuiCEFApplicationHooksInstalled || !NSApp) {
    return;
  }

  Class appClass = [NSApp class];
  if (!appClass) {
    return;
  }

  /*
   CDXC:CefRuntime 2026-06-14-15:25:
   Tauri's CEF runtime makes its NSApplication subclass conform to
   CefAppProtocol and toggles isHandlingSendEvent during sendEvent:. GPUI must
   keep GPUIApplication as the concrete app class, so install the same protocol
   surface and send-event state on GPUIApplication at runtime without changing
   window layout or input routing.

   CDXC:CefRuntime 2026-06-14-16:14:
   Chromium's message_pump_mac.mm traps if CefAppProtocol is missing when
   NSApplication's run loop is already active. Register the protocol through the
   NSApplication category above before main, then add the same protocol chain to
   GPUIApplication for direct conformance checks while leaving the early
   swizzled sendEvent implementation in place.
   */
  class_addProtocol(appClass, @protocol(CrAppProtocol));
  class_addProtocol(appClass, @protocol(CrAppControlProtocol));
  class_addProtocol(appClass, @protocol(CefAppProtocol));
  g_ghostexGpuiCEFApplicationHooksInstalled = YES;
}

static NSMenuItem *GhostexGpuiCEFStandardEditMenuItem(NSString *title,
                                                      SEL action,
                                                      NSString *keyEquivalent) {
  NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title
                                                action:action
                                         keyEquivalent:keyEquivalent];
  item.target = nil;
  item.keyEquivalentModifierMask = NSEventModifierFlagCommand;
  return item;
}

static BOOL GhostexGpuiCEFMenuContainsAction(NSMenu *menu, SEL action) {
  for (NSMenuItem *item in menu.itemArray) {
    if (item.action == action) {
      return YES;
    }
  }
  return NO;
}

static void GhostexGpuiCEFSetEditMenuKeyEquivalent(
    NSMenu *menu, SEL action, NSString *keyEquivalent,
    NSEventModifierFlags modifierMask, BOOL sourceHotkeyPassthrough) {
  for (NSMenuItem *item in menu.itemArray) {
    if (item.action != action) {
      continue;
    }
    item.keyEquivalent = sourceHotkeyPassthrough ? @"" : keyEquivalent;
    item.keyEquivalentModifierMask = modifierMask;
  }
}

static void GhostexGpuiCEFInstallStandardEditMenu(void) {
  if (!NSApp) {
    return;
  }

  NSMenu *mainMenu = NSApp.mainMenu;
  if (!mainMenu) {
    mainMenu = [[NSMenu alloc] initWithTitle:@""];
    NSApp.mainMenu = mainMenu;
  }

  NSMenu *editMenu = nil;
  for (NSMenuItem *item in mainMenu.itemArray) {
    if ([item.title isEqualToString:@"Edit"] ||
        [item.submenu.title isEqualToString:@"Edit"]) {
      editMenu = item.submenu;
      break;
    }
  }

  if (!editMenu) {
    NSMenuItem *editItem = [[NSMenuItem alloc] initWithTitle:@"Edit"
                                                      action:nil
                                               keyEquivalent:@""];
    editMenu = [[NSMenu alloc] initWithTitle:@"Edit"];
    editItem.submenu = editMenu;
    NSInteger insertionIndex = mainMenu.numberOfItems > 0 ? 1 : 0;
    [mainMenu insertItem:editItem atIndex:insertionIndex];
  }

  /*
   CDXC:Hotkeys 2026-06-14-16:31:
   Web-page inputs inside the embedded CEF browser need macOS standard Edit
   commands, including Cmd+A Select All. Install first-responder menu actions
   instead of synthesizing web-specific fallbacks so CEF, AppKit text views, and
   future browser surfaces receive the platform's normal text-command dispatch.
   */
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(undo:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(
                          @"Undo", @selector(undo:), @"z")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(redo:))) {
    NSMenuItem *redo =
        GhostexGpuiCEFStandardEditMenuItem(@"Redo", @selector(redo:), @"Z");
    redo.keyEquivalentModifierMask =
        NSEventModifierFlagCommand | NSEventModifierFlagShift;
    [editMenu addItem:redo];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(cut:)) ||
      !GhostexGpuiCEFMenuContainsAction(editMenu, @selector(selectAll:))) {
    [editMenu addItem:[NSMenuItem separatorItem]];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(cut:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(
                          @"Cut", @selector(cut:), @"x")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(copy:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(
                          @"Copy", @selector(copy:), @"c")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(paste:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(
                          @"Paste", @selector(paste:), @"v")];
  }
  if (!GhostexGpuiCEFMenuContainsAction(editMenu, @selector(selectAll:))) {
    [editMenu addItem:GhostexGpuiCEFStandardEditMenuItem(
                          @"Select All", @selector(selectAll:), @"a")];
  }

  for (NSMenuItem *item in editMenu.itemArray) {
    if (item.action == @selector(undo:) || item.action == @selector(redo:)) {
      item.target = GhostexGpuiEditHistoryMenuTargetShared();
    }
  }
  // Undo and Redo keep their key equivalents everywhere: a focused web page
  // receives the chord before the menu (see the UndoRedo route in sendEvent).
  GhostexGpuiCEFSetEditMenuKeyEquivalent(editMenu, @selector(undo:), @"z",
                                         NSEventModifierFlagCommand, NO);
  GhostexGpuiCEFSetEditMenuKeyEquivalent(editMenu, @selector(redo:), @"Z",
                                         NSEventModifierFlagCommand |
                                             NSEventModifierFlagShift,
                                         NO);

  /*
   CDXC:Hotkeys 2026-09-25 WHY:
   Source and Docs own renderer-level editing commands (Monaco, CodeMirror, and Excalidraw). While either exact workarea is the proven window keyboard owner, keep the standard Edit menu actions clickable but remove their key equivalents so AppKit cannot translate Cmd+X/C/V or Cmd+A into responder selectors before Chromium receives the original trusted key event. Reinstalling the normal menu after focus leaves those workareas restores every standard equivalent for Browser and native text controls. Supersedes the 2026-08-08 version, which also stripped Cmd+Z and Cmd+Shift+Z; those now reach every focused web page first through the UndoRedo route in sendEvent.
   */
  BOOL rendererEditHotkeyPassthrough =
      GhostexGpuiCEFRendererEditHotkeysOwnKeyboardInWindow(NSApp.keyWindow);
  GhostexGpuiCEFSetEditMenuKeyEquivalent(editMenu, @selector(cut:), @"x",
                                         NSEventModifierFlagCommand,
                                         rendererEditHotkeyPassthrough);
  GhostexGpuiCEFSetEditMenuKeyEquivalent(editMenu, @selector(copy:), @"c",
                                         NSEventModifierFlagCommand,
                                         rendererEditHotkeyPassthrough);
  GhostexGpuiCEFSetEditMenuKeyEquivalent(editMenu, @selector(paste:), @"v",
                                         NSEventModifierFlagCommand,
                                         rendererEditHotkeyPassthrough);
  GhostexGpuiCEFSetEditMenuKeyEquivalent(editMenu, @selector(selectAll:), @"a",
                                         NSEventModifierFlagCommand,
                                         rendererEditHotkeyPassthrough);
}

void GhostexGpuiCEFSetNativeViewFrame(void *nativeView, double x, double y,
                                      double width, double height) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  /*
   CDXC:CefRuntime 2026-07-07:
   GPUI layout is the only owner of this CEF child view's frame. CEF creates
   the child with a width/height-sizable autoresizing mask, so AppKit resizes
   it on every window resize before GPUI's next layout pass; the Rust
   set_bounds cache then skips the correcting frame write whenever the logical
   bounds are unchanged (e.g. the fixed-width sidebar during a width-only
   window narrow), leaving the AppKit-adjusted frame in place. Pin the mask to
   not-sizable so the frame only ever changes through this setter.
  */
  view.autoresizingMask = NSViewNotSizable;

  NSView *parent = [view superview];
  CGFloat nativeY = y;
  if (parent && ![parent isFlipped]) {
    nativeY = NSHeight(parent.bounds) - y - height;
  }
  CFAbsoluteTime startedAt = CFAbsoluteTimeGetCurrent();
  view.frame = NSMakeRect(x, nativeY, MAX(0.0, width), MAX(0.0, height));
  if (GhostexGpuiCEFResizeDiagnosticsEnabled()) {
    double frameMs = (CFAbsoluteTimeGetCurrent() - startedAt) * 1000.0;
    if (frameMs >= 1.0) {
      NSLog(@"[gpui-cef-native-frame] frame_ms=%.3f rect=%.0f,%.0f,%.0fx%.0f",
            frameMs, x, nativeY, width, height);
    }
  }
}

void GhostexGpuiCEFRefreshNativeViewWindowCorners(void *nativeView) {
  GhostexGpuiClipNativeViewWindowCorners((__bridge NSView *)nativeView);
}

void GhostexGpuiCEFLogResizeDiagnostic(int browserId, int width, int height,
                                       uint64_t frameUs, uint64_t wasResizedUs,
                                       uint64_t totalUs) {
  if (!GhostexGpuiCEFResizeDiagnosticsEnabled()) {
    return;
  }
  NSLog(@"[gpui-cef-resize] browser=%d size=%dx%d frame_us=%llu "
        @"was_resized_us=%llu total_us=%llu",
        browserId, width, height, (unsigned long long)frameUs,
        (unsigned long long)wasResizedUs, (unsigned long long)totalUs);
}

/*
 CDXC:Workarea 2026-09-23 WHY:
 While a panel slides, a web page whose frame would move or poke outside its
 clip is made transparent instead of hidden: hiding a view that holds the first
 responder hands the window's keyboard focus to the window itself. The alpha is
 set on the view and on its layer because Chromium's view may host its own layer.
*/
void GhostexGpuiCEFSetNativeViewMotionHidden(void *nativeView, bool hidden,
                                             double fadeInSeconds) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }
  static NSString *const fadeKey = @"ghostexMotionFade";
  CGFloat alpha = hidden ? 0.0 : 1.0;
  [CATransaction begin];
  [CATransaction setDisableActions:YES];
  view.alphaValue = alpha;
  if (view.layer) {
    [view.layer removeAnimationForKey:fadeKey];
    view.layer.opacity = (float)alpha;
  }
  [CATransaction commit];
  // Coming back after a slide, the page fades in the way the panel's GPUI
  // content does; Core Animation runs it, so nothing drives it per frame.
  if (!hidden && fadeInSeconds > 0.0 && view.layer) {
    CABasicAnimation *fade = [CABasicAnimation animationWithKeyPath:@"opacity"];
    fade.fromValue = @0.0;
    fade.toValue = @1.0;
    fade.duration = fadeInSeconds;
    fade.timingFunction =
        [CAMediaTimingFunction functionWithControlPoints:0.0:0.0:0.58:1.0];
    [view.layer addAnimation:fade forKey:fadeKey];
  }
}

void GhostexGpuiCEFSetNativeViewVisible(void *nativeView, bool visible) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }
  view.hidden = visible ? NO : YES;
}

void GhostexGpuiCEFOrderNativeViewFront(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  NSView *parent = view.superview;
  if (!view || !parent) {
    return;
  }

  /*
   CDXC:Titlebar 2026-07-09:
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

static uintptr_t GhostexGpuiCEFLayerContentIdentity(CALayer *layer) {
  if (!layer) {
    return 0;
  }
  // Chromium shows the GPU process's frames through a CALayerHost (its contextId) or, when it
  // draws in this process, as an IOSurface set as a layer's contents.
  static Class hostClass;
  static SEL contextIdSelector;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    hostClass = NSClassFromString(@"CALayerHost");
    contextIdSelector = NSSelectorFromString(@"contextId");
  });
  // A private class: `valueForKey:` throws if a later macOS drops the key, so ask first.
  if (hostClass && [layer isKindOfClass:hostClass] && [layer respondsToSelector:contextIdSelector]) {
    NSNumber *contextId = [layer valueForKey:@"contextId"];
    if (contextId.unsignedIntValue != 0) {
      return ((uintptr_t)1 << 63) | contextId.unsignedIntValue;
    }
  }
  if (layer.contents) {
    return (uintptr_t)(__bridge void *)layer.contents;
  }
  for (CALayer *sublayer in layer.sublayers) {
    uintptr_t identity = GhostexGpuiCEFLayerContentIdentity(sublayer);
    if (identity) {
      return identity;
    }
  }
  return 0;
}

/// CDXC:CodeEditor 2026-10-24 WHY:
/// What a CEF page's view shows right now, as an identity: it changes when the page's compositor hands the view a new surface, which is how a Code window switch learns that the incoming page is on screen (app/workarea/code_windows.rs) without the page's own script, which keeps drawing while hidden and so cannot tell. 0 while the view shows nothing.
uint64_t GhostexGpuiCEFNativeViewContentIdentity(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return 0;
  }
  uintptr_t identity = GhostexGpuiCEFLayerContentIdentity(view.layer);
  if (identity) {
    return identity;
  }
  for (NSView *subview in view.subviews) {
    identity = GhostexGpuiCEFNativeViewContentIdentity((__bridge void *)subview);
    if (identity) {
      return identity;
    }
  }
  return 0;
}

void GhostexGpuiCEFRemoveNativeViewFromSuperview(void *nativeView) {
  if (!nativeView) {
    return;
  }

  /*
   CDXC:CefRuntime 2026-08-24:
   Completes CEF's close contract for browsers whose DoClose returns handled
   (cef_life_span_handler.h: the app must still finish the close by proceeding
   with window/view-hierarchy tear-down, otherwise the browser stays partially
   closed). CEF creates this child NSView, adds it to the GPUI content view and
   releases it, so the superview holds its only strong reference: removing it
   deallocs the view, CEF observes the window destruction, fires OnBeforeClose
   and the renderer process exits. Without this every closed CEF surface left a
   live renderer subprocess parented to the long-lived GPUI window.
   `__bridge` (never `__bridge_transfer`): Rust holds a borrowed pointer to a
   CEF-owned view, so ARC must not consume a reference here.
  */
  NSView *view = (__bridge NSView *)nativeView;
  [view removeFromSuperview];
}

void GhostexGpuiCEFPrepareNativeViewForFocus(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  /*
   CDXC:FocusRouting 2026-06-14-16:45:
   Browser clicks land on CEF's native child view, not always on GPUI's hitbox
   tree. Make the exact CEF NSView accept first responder and claim it on
   mouseDown before forwarding the event, so macOS command-key text actions
   route to Chromium after the user leaves the GPUI address bar.
  */
  GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(view);
}

void GhostexGpuiInstallFirstResponderObserverForNativeView(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view || !view.window) {
    return;
  }

  GhostexGpuiInstallRootPointerTrackingArea(view);
  GhostexGpuiInstallNavigationGestureMethods(object_getClass(view));

  NSWindow *window = view.window;
  GhostexGpuiFirstResponderObserver *observer =
      objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey);
  if (observer) {
    observer.gpuiRootView = view;
    GhostexGpuiFirstResponderReportWindow(window);
    return;
  }

  observer = [[GhostexGpuiFirstResponderObserver alloc] initWithWindow:window
                                                          gpuiRootView:view];
  objc_setAssociatedObject(window, GhostexGpuiFirstResponderObserverKey,
                           observer, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

static void GhostexGpuiInstallRootPointerTrackingArea(NSView *view) {
  if (!view ||
      objc_getAssociatedObject(view, GhostexGpuiRootPointerTrackingAreaKey)) {
    return;
  }

  /*
   CDXC:CefRuntime 2026-08-23:
   A normal GPUI macOS window asks NSWindow to distribute mouse-moved events,
   but AppKit sends those generic moves to the first responder. Clicking a CEF
   child correctly makes Chromium first responder for keyboard input, which
   otherwise leaves the real GPUI root without pointer moves and freezes hover
   state across the titlebar and pane tabs until a GPUI click restores it.

   Give the actual GPUI root view its standard AppKit tracking area. The owner
   then receives mouseMoved: while the pointer is inside its own visible bounds
   regardless of which normal-layout child owns keyboard focus. This observes
   the existing view geometry only; it adds no overlay, hit-test override, or
   event rerouting, and CEF keeps first-responder ownership for page input.
  */
  NSTrackingArea *trackingArea = [[NSTrackingArea alloc]
      initWithRect:NSZeroRect
           options:NSTrackingMouseMoved | NSTrackingActiveInKeyWindow |
                   NSTrackingInVisibleRect
             owner:view
          userInfo:nil];
  [view addTrackingArea:trackingArea];
  objc_setAssociatedObject(view, GhostexGpuiRootPointerTrackingAreaKey,
                           trackingArea, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

static void GhostexGpuiFirstResponderReportWindow(NSWindow *window) {
  if (!window) {
    GhostexGpuiFirstResponderDidChange(NULL, NULL);
    return;
  }
  GhostexGpuiFirstResponderObserver *observer =
      objc_getAssociatedObject(window, GhostexGpuiFirstResponderObserverKey);
  NSView *gpuiRootView = observer.gpuiRootView;
  id responder = window.firstResponder;
  /*
   CDXC:FocusRouting 2026-07-11:
   The Rust side defers responder classification onto the gpui foreground
   executor, and responder churn is often CAUSED by the very teardown that
   deallocates the outgoing responder view (browser/terminal host drops). A
   raw pointer would be dangling by the time the deferred task walks its
   superview chain — pass a +1 retained reference instead, released by Rust
   via GhostexGpuiReleaseRetainedResponder after classification. A retained
   view that was removed from its window classifies as no known surface,
   which is the correct answer for a dying responder.
   */
  GhostexGpuiFirstResponderDidChange(
      (__bridge void *)gpuiRootView,
      responder ? (void *)CFBridgingRetain(responder) : NULL);
}

void GhostexGpuiReleaseRetainedResponder(void *responder) {
  if (responder) {
    CFRelease((CFTypeRef)responder);
  }
}

bool GhostexGpuiNativeViewContainsResponder(void *rootNativeView,
                                            void *responder) {
  NSView *root = (__bridge NSView *)rootNativeView;
  id candidate = (__bridge id)responder;
  if (!root || ![candidate isKindOfClass:NSView.class]) {
    return false;
  }

  for (NSView *view = (NSView *)candidate; view; view = view.superview) {
    if (view == root) {
      return true;
    }
  }
  return false;
}

bool GhostexGpuiCEFNativeViewOwnsFirstResponder(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return false;
  }
  NSWindow *window = view.window;
  if (!window) {
    return false;
  }
  id responder = window.firstResponder;
  if (!responder) {
    return false;
  }
  return GhostexGpuiNativeViewContainsResponder(nativeView,
                                                (__bridge void *)responder);
}

static void GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(NSView *view) {
  if (!view) {
    return;
  }

  GhostexGpuiCEFInstallBrowserViewFocusSubclass(view);
  for (NSView *subview in view.subviews) {
    GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(subview);
  }
}

/*
 CDXC:Sidebar 2026-09-09 WHY:
 Chromium 148's RenderWidgetHostViewCocoa already tracks mouse movement with NSTrackingActiveAlways, but shouldIgnoreMouseEvent rejects movement when its window is neither main nor key.
 Moving the sidebar into a nonactivating child panel therefore disables CSS hover even with the native-pointer-inside gate set correctly.
 Use Chromium's kWhenInActiveApp policy for the floating sidebar's mouse and tooltip acceptance so hovering does not need to take keyboard focus from the workspace.
 Both AcceptMouseEvents and AcceptTooltipEvents define kWhenInActiveApp as the C++ int enum value 1 in content/public/browser/render_widget_host_view_mac_delegate.h.
*/
static int GhostexGpuiCEFBrowserViewHoverAcceptance(id self, SEL _cmd) {
  NSView *sidebar = g_ghostexGpuiSidebarPointerTrackingView;
  if (sidebar.window.parentWindow &&
      GhostexGpuiNativeViewContainsResponder((__bridge void *)sidebar,
                                             (__bridge void *)self)) {
    return 1;
  }
  struct objc_super superInfo = {
      .receiver = self,
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  int (*sendSuper)(struct objc_super *, SEL) = (void *)objc_msgSendSuper;
  return sendSuper(&superInfo, _cmd);
}

void GhostexGpuiCEFSetNativeViewPinchZoomDisabled(void *nativeView,
                                                 bool disabled) {
  NSView *view = (__bridge NSView *)nativeView;
  if (view) {
    objc_setAssociatedObject(view, GhostexGpuiCEFPinchZoomDisabledKey,
                             @(disabled), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
}

/*
 CDXC:CefRuntime 2026-09-09 WHY:
 Stop magnification on the receiving CEF view before Chromium scales the page, including embedded editor frames and descendants created after navigation.
 The policy lives on the browser root and is set by cef/shell/browser.rs; ordinary scrolling and keyboard zoom use separate event paths.
*/
static void GhostexGpuiCEFBrowserViewMagnify(id self, SEL _cmd, NSEvent *event) {
  for (NSView *view = self; view; view = view.superview) {
    NSNumber *disabled =
        objc_getAssociatedObject(view, GhostexGpuiCEFPinchZoomDisabledKey);
    if (disabled) {
      if (disabled.boolValue) {
        return;
      }
      break;
    }
  }
  struct objc_super superInfo = {
      .receiver = self,
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  void (*sendSuper)(struct objc_super *, SEL, NSEvent *) =
      (void *)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd, event);
}

/*
 CDXC:CefRuntime 2026-10-24 WHY:
 GitHub #291: the app quit on an uncaught "Cannot remove an observer <_NSTouchBarFinderObservation> for the key path nextResponder" exception. AppKit's Touch Bar finder (it runs on Macs without a Touch Bar too) KVO-observes nextResponder on every responder from the key window's first responder up, and KVO does that by switching the view's class to NSKVONotifying_<class>. The focus subclass below switches the class too, so running it on a view the finder was observing put our class on top of KVO's: KVO lost its own class info (`-class` returned nil), and the finder's next removal of that observer, when focus moved (a chat menu or the feedback window closing), threw or crashed.
 So a view whose hierarchy already holds the focus subclass is left alone, even under KVO's class (the old prefix check compared one byte too many and never matched, so every repeat swizzled again), and a view KVO currently owns is swizzled once KVO has let go. The forwarding calls take their superclass from the focus subclass, not from the object's class: with KVO's class on top, the class above the focus subclass is the focus subclass itself, so every forwarded call looped back into the same hook.
*/
/*
 CDXC:CefRuntime 2026-10-25 SEE-ALSO:
 The same exception had a second cause in the GPUI fork: its accessibility adapter (accesskit's SubclassingAdapter) swapped every GPUI window's content-view class when the window opened and swapped it back when the window closed, which stripped KVO's class whenever a key child window closed (Add to Chat from the chat's menu, restoring the maximized composer). `.dependencies/zed` crates/gpui_macos/src/window.rs (`A11yState`) now gives GPUI windows a declared `GPUIContentView` instead. Never change the class of a view in a GPUI window, there or here, without the KVO check above.
*/
static const char GhostexGpuiCEFFocusClassPrefix[] = "GhostexGpuiCEFFocus_";
static const void *GhostexGpuiCEFFocusSubclassRetryKey =
    &GhostexGpuiCEFFocusSubclassRetryKey;
static const int64_t GhostexGpuiCEFFocusSubclassRetryDelay =
    500 * NSEC_PER_MSEC;

static Class GhostexGpuiCEFFocusSubclassInHierarchy(Class cls) {
  size_t prefixLength = sizeof(GhostexGpuiCEFFocusClassPrefix) - 1;
  for (; cls; cls = class_getSuperclass(cls)) {
    if (strncmp(class_getName(cls), GhostexGpuiCEFFocusClassPrefix,
                prefixLength) == 0) {
      return cls;
    }
  }
  return Nil;
}

static Class GhostexGpuiCEFFocusSuperclass(id self) {
  Class objectClass = object_getClass(self);
  Class focusClass = GhostexGpuiCEFFocusSubclassInHierarchy(objectClass);
  return class_getSuperclass(focusClass ?: objectClass);
}

static void GhostexGpuiCEFRetryBrowserViewFocusSubclass(NSView *view) {
  if (objc_getAssociatedObject(view, GhostexGpuiCEFFocusSubclassRetryKey)) {
    return;
  }
  objc_setAssociatedObject(view, GhostexGpuiCEFFocusSubclassRetryKey, @YES,
                           OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  __weak NSView *weakView = view;
  dispatch_after(
      dispatch_time(DISPATCH_TIME_NOW, GhostexGpuiCEFFocusSubclassRetryDelay),
      dispatch_get_main_queue(), ^{
        NSView *retryView = weakView;
        if (!retryView) {
          return;
        }
        objc_setAssociatedObject(retryView,
                                 GhostexGpuiCEFFocusSubclassRetryKey, nil,
                                 OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        GhostexGpuiCEFInstallBrowserViewFocusSubclass(retryView);
      });
}

static void GhostexGpuiCEFInstallBrowserViewFocusSubclass(NSView *view) {
  Class originalClass = object_getClass(view);
  if (!originalClass ||
      GhostexGpuiCEFFocusSubclassInHierarchy(originalClass)) {
    return;
  }
  if (originalClass != [view class]) {
    GhostexGpuiCEFRetryBrowserViewFocusSubclass(view);
    return;
  }

  const char *originalName = class_getName(originalClass);
  NSString *subclassName =
      [NSString stringWithFormat:@"%s%s", GhostexGpuiCEFFocusClassPrefix,
                                 originalName];
  Class subclass = NSClassFromString(subclassName);
  if (!subclass) {
    subclass =
        objc_allocateClassPair(originalClass, subclassName.UTF8String, 0);
    if (!subclass) {
      return;
    }

    // These selectors belong to Chromium's render widget, not its container views.
    for (NSString *name in @[@"acceptsMouseEventsOption", @"acceptsTooltipEvents"]) {
      SEL selector = NSSelectorFromString(name);
      Method method = class_getInstanceMethod(originalClass, selector);
      if (method) {
        class_addMethod(subclass, selector,
                        (IMP)GhostexGpuiCEFBrowserViewHoverAcceptance,
                        method_getTypeEncoding(method));
      }
    }
    class_addMethod(subclass, @selector(mouseDown:),
                    (IMP)GhostexGpuiCEFBrowserViewMouseDown, "v@:@");
    class_addMethod(subclass, @selector(magnifyWithEvent:),
                    (IMP)GhostexGpuiCEFBrowserViewMagnify, "v@:@");
    class_addMethod(subclass, @selector(smartMagnifyWithEvent:),
                    (IMP)GhostexGpuiCEFBrowserViewMagnify, "v@:@");
    class_addMethod(subclass, @selector(acceptsFirstResponder),
                    (IMP)GhostexGpuiCEFBrowserViewAcceptsFirstResponder, "c@:");
    class_addMethod(subclass, @selector(selectAll:),
                    (IMP)GhostexGpuiCEFBrowserViewSelectAll, "v@:@");
    class_addMethod(subclass, @selector(cut:),
                    (IMP)GhostexGpuiCEFBrowserViewCut, "v@:@");
    class_addMethod(subclass, @selector(copy:),
                    (IMP)GhostexGpuiCEFBrowserViewCopy, "v@:@");
    class_addMethod(subclass, @selector(paste:),
                    (IMP)GhostexGpuiCEFBrowserViewPaste, "v@:@");
    class_addMethod(subclass, @selector(performKeyEquivalent:),
                    (IMP)GhostexGpuiCEFBrowserViewPerformKeyEquivalent, "c@:@");
    // Only Chromium's render widget is the page's text input client.
    Method insertText = class_getInstanceMethod(
        originalClass, @selector(insertText:replacementRange:));
    if (insertText && GhostexGpuiResponderIsChromiumContent(view)) {
      class_addMethod(subclass, @selector(insertText:replacementRange:),
                      (IMP)GhostexGpuiCEFBrowserViewInsertText,
                      method_getTypeEncoding(insertText));
    }
    class_addMethod(subclass, @selector(addSubview:),
                    (IMP)GhostexGpuiCEFBrowserViewAddSubview, "v@:@");
    class_addMethod(subclass, @selector(viewDidChangeEffectiveAppearance),
                    (IMP)GhostexGpuiCEFBrowserViewDidChangeEffectiveAppearance,
                    "v@:");
    objc_registerClassPair(subclass);
    GhostexGpuiInstallNavigationGestureMethods(subclass);
  }

  object_setClass(view, subclass);
}

static void GhostexGpuiCEFBrowserViewMouseDown(id self, SEL _cmd,
                                               NSEvent *event) {
  NSView *browserRoot = GhostexGpuiCEFMarkFocusedResponder(self);
  NSWindow *window = [self window];
  if (window) {
    [window makeFirstResponder:self];
  }
  if (browserRoot && event) {
    NSRect frameInWindow = [browserRoot convertRect:browserRoot.bounds
                                             toView:nil];
    NSView *parent = browserRoot.superview;
    GhostexGpuiCEFLogNativeMouseDown(
        (__bridge void *)browserRoot, event.locationInWindow.x,
        event.locationInWindow.y, frameInWindow.origin.x,
        frameInWindow.origin.y, frameInWindow.size.width,
        frameInWindow.size.height, parent ? NSWidth(parent.bounds) : 0.0,
        parent ? NSHeight(parent.bounds) : 0.0,
        browserRoot.hidden || browserRoot.isHiddenOrHasHiddenAncestor ? 1 : 0,
        object_getClassName(self));
  }
  /*
   CDXC:Titlebar 2026-07-15:
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
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  void (*sendSuper)(struct objc_super *, SEL, NSEvent *) =
      (void *)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd, event);
}

static BOOL GhostexGpuiCEFBrowserViewAcceptsFirstResponder(id self, SEL _cmd) {
  (void)self;
  (void)_cmd;
  return YES;
}

static void GhostexGpuiCEFBrowserViewSelectAll(id self, SEL _cmd, id sender) {
  /*
   CDXC:Hotkeys 2026-06-14-17:25:
   Cmd+A in focused CEF page text fields must stay inside Chromium after the
   GPUI address bar has previously owned focus. Implement the standard AppKit
   selectAll: command on the exact CEF NSView and delegate to cef-rs
   Frame::select_all, so macOS command dispatch uses Chromium selection
   semantics without a hidden hit-test layer or page-specific fallback.

   CDXC:Hotkeys 2026-06-14-17:25:
   CEF can deliver page clicks to descendant NSViews below the browser host
   returned by cef-rs. Install the focus subclass on the CEF view tree and
   resolve selectAll: by walking ancestor views back to the registered browser
   root, so command-key focus follows the actual Chromium child that received
   the click.
   */
  if (GhostexGpuiCEFHandleSelectAllForResponder(self)) {
    return;
  }

  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static void GhostexGpuiCEFBrowserViewForwardEditActionToSuper(id self, SEL _cmd,
                                                              id sender) {
  Class superClass = GhostexGpuiCEFFocusSuperclass(self);
  if (superClass && class_getInstanceMethod(superClass, _cmd)) {
    struct objc_super superInfo = {
        .receiver = self,
        .super_class = superClass,
    };
    void (*sendSuper)(struct objc_super *, SEL, id) = (void *)objc_msgSendSuper;
    sendSuper(&superInfo, _cmd, sender);
  }
}

/*
 CDXC:Hotkeys 2026-07-09:
 Standard Edit-menu actions (menu-bar clicks and nil-target responder-chain
 dispatch) must reach Chromium's frame edit commands on the exact CEF view
 tree, the same way selectAll: already does. Routing through the bridge —
 instead of relying on Chromium's own copy:/paste: responders deeper in the
 tree — also marks the per-event bridged flag so the app-level sendEvent
 mirror never issues the same clipboard command twice.
 */
static void GhostexGpuiCEFBrowserViewCut(id self, SEL _cmd, id sender) {
  if (GhostexGpuiCEFHandleEditCommandForResponder(
          self, GhostexGpuiCEFEditCommandCut)) {
    return;
  }
  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static void GhostexGpuiCEFBrowserViewCopy(id self, SEL _cmd, id sender) {
  if (GhostexGpuiCEFHandleEditCommandForResponder(
          self, GhostexGpuiCEFEditCommandCopy)) {
    return;
  }
  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static void GhostexGpuiCEFBrowserViewPaste(id self, SEL _cmd, id sender) {
  if (GhostexGpuiCEFHandleEditCommandForResponder(
          self, GhostexGpuiCEFEditCommandPaste)) {
    return;
  }
  GhostexGpuiCEFBrowserViewForwardEditActionToSuper(self, _cmd, sender);
}

static BOOL GhostexGpuiCEFBrowserViewPerformKeyEquivalent(id self, SEL _cmd,
                                                          NSEvent *event) {
  GhostexGpuiCEFZoomCommand zoomCommand =
      GhostexGpuiCEFZoomCommandForEvent(event);
  if (zoomCommand != GhostexGpuiCEFZoomCommandNone &&
      GhostexGpuiCEFHandleZoomCommandForResponder(self, zoomCommand)) {
    return YES;
  }

  if (GhostexGpuiCEFEventIsCommandA(event)) {
    if (!GhostexGpuiCEFRendererEditHotkeysOwnKeyboardInWindow([self window]) &&
        GhostexGpuiCEFHandleSelectAllForResponder(self)) {
      return YES;
    }
  }

  struct objc_super superInfo = {
      .receiver = self,
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  BOOL (*sendSuper)(struct objc_super *, SEL, NSEvent *) =
      (void *)objc_msgSendSuper;
  BOOL handled = sendSuper(&superInfo, _cmd, event);

  GhostexGpuiCEFEditCommand clipboardCommand =
      GhostexGpuiCEFClipboardEditCommandForEvent(event);
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

/*
 CDXC:Hotkeys 2026-10-27 WHY:
 GitHub #326: an arrow, Home/End or Page Up/Down press in the Code view also typed its layout code (0x1E for Up) into the file, and into the Command Palette's box, which re-filtered the list so the arrows never moved its selection. The macOS input method committed the key event's raw character as text, and Chromium's insertText: hands committed text to the page as is, past Blink's rule that keys never type control characters. A commit made only of control or AppKit function-key characters is dropped here (the rule and its tests are GhostexGpuiCEFTextIsStrayKeyCharacter in cef/macos.rs); Enter, Tab, Backspace, Escape and Ctrl+letter keep reaching the page through Chromium's key event, and every CEF page (Code, Browser, Docs, Settings) shares this view.
 */
static void GhostexGpuiCEFBrowserViewInsertText(id self, SEL _cmd, id string,
                                                NSRange replacementRange) {
  NSString *text =
      [string isKindOfClass:NSAttributedString.class] ? [string string] : string;
  if ([text isKindOfClass:NSString.class] && text.length > 0 &&
      text.length <= 16) {
    unichar units[16];
    [text getCharacters:units range:NSMakeRange(0, text.length)];
    if (GhostexGpuiCEFTextIsStrayKeyCharacter(units, text.length) != 0) {
      return;
    }
  }
  struct objc_super superInfo = {
      .receiver = self,
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  void (*sendSuper)(struct objc_super *, SEL, id, NSRange) =
      (void *)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd, string, replacementRange);
}

static void GhostexGpuiCEFBrowserViewAddSubview(id self, SEL _cmd,
                                                NSView *subview) {
  struct objc_super superInfo = {
      .receiver = self,
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  void (*sendSuper)(struct objc_super *, SEL, NSView *) =
      (void *)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd, subview);
  GhostexGpuiCEFInstallBrowserViewFocusSubclassInTree(subview);
}

static void GhostexGpuiCEFBrowserViewDidChangeEffectiveAppearance(id self,
                                                                  SEL _cmd) {
  struct objc_super superInfo = {
      .receiver = self,
      .super_class = GhostexGpuiCEFFocusSuperclass(self),
  };
  void (*sendSuper)(struct objc_super *, SEL) = (void *)objc_msgSendSuper;
  sendSuper(&superInfo, _cmd);
  if ([self isKindOfClass:NSView.class]) {
    GhostexGpuiCEFRefreshSystemPageAppearanceForView((NSView *)self);
  }
}

static BOOL GhostexGpuiCEFRefreshSystemPageAppearanceForView(NSView *view) {
  for (NSView *candidate = view; candidate; candidate = candidate.superview) {
    if (GhostexGpuiCEFRefreshSystemPageAppearanceForNativeView(
            (__bridge void *)candidate)) {
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

static NSEvent *GhostexGpuiNormalizedNavigationKeyEvent(NSEvent *event) {
  if (!event ||
      (event.type != NSEventTypeKeyDown && event.type != NSEventTypeKeyUp)) {
    return event;
  }

  // These physical keycodes are reserved navigation keys on every macOS
  // keyboard layout, so matching by keycode alone is safe. Cleanliness is
  // judged by the CGEvent unicode payload — the field TSM reads — because
  // the NSEvent-level characters always look correct for these keys.
  // Dirty events are copied from their CGEvent with the payload replaced by
  // the canonical function-key character (removed for Backspace): the shape
  // TSM treats as a normal function key (doCommand dispatch) instead of
  // committable text.
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
      /*
       CDXC:Hotkeys 2026-08-18:
       F1-F20 carry the same defect in the same place: their raw layout
       translation is the single control code 0x10, so a CGEvent-synthesized
       F1 gets committed by TSM as a literal DLE character into whatever text
       client has focus — Monaco's chat composer showed a control-character box
       instead of opening its F1 command palette. Their canonical NSEvent
       characters are the sequential F700-range function-key codes with the
       function modifier set, which is exactly what a hardware F-key reports.
       */
      {122, NSF1FunctionKey, YES, NO},
      {120, NSF2FunctionKey, YES, NO},
      {99, NSF3FunctionKey, YES, NO},
      {118, NSF4FunctionKey, YES, NO},
      {96, NSF5FunctionKey, YES, NO},
      {97, NSF6FunctionKey, YES, NO},
      {98, NSF7FunctionKey, YES, NO},
      {100, NSF8FunctionKey, YES, NO},
      {101, NSF9FunctionKey, YES, NO},
      {109, NSF10FunctionKey, YES, NO},
      {103, NSF11FunctionKey, YES, NO},
      {111, NSF12FunctionKey, YES, NO},
      {105, NSF13FunctionKey, YES, NO},
      {107, NSF14FunctionKey, YES, NO},
      {113, NSF15FunctionKey, YES, NO},
      {106, NSF16FunctionKey, YES, NO},
      {64, NSF17FunctionKey, YES, NO},
      {79, NSF18FunctionKey, YES, NO},
      {80, NSF19FunctionKey, YES, NO},
      {90, NSF20FunctionKey, YES, NO},
  };

  for (size_t i = 0; i < sizeof(normalizations) / sizeof(normalizations[0]);
       i++) {
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
    /*
     CDXC:Hotkeys 2026-09-28 WHY:
     A Backspace translated through the keyboard layout carries a 0x08
     payload. Rebuilding it
     with keyEventWithType: as a DEL (0x7F) event made the macOS Pinyin input
     method decline it (doCommandBySelector:deleteBackward: instead of
     editing its marked text), so every Backspace during a pinyin
     composition was swallowed (GitHub #112 follow-up). The original CGEvent
     copied with its payload removed is edited normally, so a dirty event is
     never rebuilt, and a DEL payload is not treated as clean for Backspace.
     */
    BOOL payloadClean =
        payloadLength == 0 ||
        (entry.functionModifier && payloadLength == 1 &&
         payload[0] == (UniChar)canonicalCharacter);
    if (payloadClean || !cgEvent) {
      return event;
    }
    /*
     CDXC:Hotkeys 2026-10-22 WHY:
     Every arrow, Home/End, PageUp/PageDown, Forward Delete and F-key event the
     system makes (a real keyboard, Karabiner's virtual HID, any CGEventSource)
     carries the layout's classic character code (0x1C-0x1F for the arrows)
     together with the Fn flag, plus NumPad for the arrows. That is the shape
     input methods read, so it is not dirty: rewriting it with an F700-range
     payload made the Japanese input method decline Up/Down
     (doCommandBySelector: instead of moving through its candidates), so
     neither the chat box nor the terminal could pick a conversion candidate
     from the keyboard. Only an event missing those flags (a posting tool that
     set its own flags) is the 2026-07-04 control-code case and is rewritten.
     */
    if (entry.functionModifier) {
      CGEventFlags systemFlags = kCGEventFlagMaskSecondaryFn;
      if (entry.numericPad) {
        systemFlags |= kCGEventFlagMaskNumericPad;
      }
      if ((CGEventGetFlags(cgEvent) & systemFlags) == systemFlags) {
        return event;
      }
    }

    CGEventRef stripped = CGEventCreateCopy(cgEvent);
    if (!stripped) {
      return event;
    }
    /*
     CDXC:Hotkeys 2026-10-05 WHY:
     An arrow, Home/End, PageUp/PageDown, Forward Delete or F-key copy with an empty payload got its characters from the keyboard layout again (0x1C-0x1F for the arrows), so Chromium inserted them as text and VS Code showed red FS/GS/RS/US marks instead of moving the cursor. Those keys get their canonical F700-range character, the hardware shape the clean check above accepts; only Backspace keeps the empty payload the Pinyin fix needs.
     */
    if (entry.functionModifier) {
      UniChar canonicalPayload = (UniChar)canonicalCharacter;
      CGEventKeyboardSetUnicodeString(stripped, 1, &canonicalPayload);
    } else {
      CGEventKeyboardSetUnicodeString(stripped, 0, NULL);
    }
    CGEventFlags flags = CGEventGetFlags(stripped);
    if (entry.functionModifier) {
      flags |= kCGEventFlagMaskSecondaryFn;
    }
    if (entry.numericPad) {
      flags |= kCGEventFlagMaskNumericPad;
    }
    CGEventSetFlags(stripped, flags);
    NSEvent *normalized = [NSEvent eventWithCGEvent:stripped];
    CFRelease(stripped);
    return normalized ?: event;
  }

  return event;
}

static BOOL GhostexGpuiCEFEventIsCommandA(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return NO;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  if (modifiers != 0) {
    return NO;
  }

  return
      [GhostexGpuiShortcutCharactersForEvent(event).lowercaseString
          isEqualToString:@"a"];
}

static BOOL GhostexGpuiCEFEventIsCommandF(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return NO;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  if (modifiers != 0) {
    return NO;
  }

  return
      [GhostexGpuiShortcutCharactersForEvent(event).lowercaseString
          isEqualToString:@"f"];
}

static BOOL GhostexGpuiCEFEventIsCommandOptionF(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  NSEventModifierFlags requiredModifiers =
      NSEventModifierFlagCommand | NSEventModifierFlagOption;
  if ((modifiers & requiredModifiers) != requiredModifiers) {
    return NO;
  }

  modifiers &= ~requiredModifiers;
  if (modifiers != 0) {
    return NO;
  }

  return
      [GhostexGpuiShortcutCharactersForEvent(event).lowercaseString
          isEqualToString:@"f"];
}

static BOOL GhostexGpuiCEFEventIsHistoryChord(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return NO;
  }

  modifiers &= ~(NSEventModifierFlagCommand | NSEventModifierFlagShift);
  if (modifiers != 0) {
    return NO;
  }

  return
      [GhostexGpuiShortcutCharactersForEvent(event).lowercaseString
          isEqualToString:@"z"];
}

static BOOL GhostexGpuiCEFEventIsWebTextEditingKey(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  // Shift and Option are the modifiers GPUIView still feeds to its input
  // context (Option+Left moves by word, Option+Delete deletes a word).
  NSEventModifierFlags modifiers =
      event.modifierFlags &
      (NSEventModifierFlagShift | NSEventModifierFlagControl |
       NSEventModifierFlagOption | NSEventModifierFlagCommand);
  if ((modifiers & (NSEventModifierFlagControl | NSEventModifierFlagCommand)) !=
      0) {
    return NO;
  }

  // Arrows, Home, End, PageUp, PageDown, Delete and Forward Delete.
  switch (event.keyCode) {
  case 123:
  case 124:
  case 125:
  case 126:
  case 115:
  case 119:
  case 116:
  case 121:
  case 51:
  case 117:
    return YES;
  default:
    return NO;
  }
}

static BOOL GhostexGpuiCEFEventIsCommandY(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return NO;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return NO;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  if (modifiers != 0) {
    return NO;
  }

  return
      [GhostexGpuiShortcutCharactersForEvent(event).lowercaseString
          isEqualToString:@"y"];
}

static GhostexGpuiCEFEditCommand
GhostexGpuiCEFClipboardEditCommandForEvent(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return GhostexGpuiCEFEditCommandNone;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return GhostexGpuiCEFEditCommandNone;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  if (modifiers != 0) {
    return GhostexGpuiCEFEditCommandNone;
  }

  NSString *key = GhostexGpuiShortcutCharactersForEvent(event).lowercaseString;
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

static GhostexGpuiCEFZoomCommand
GhostexGpuiCEFZoomCommandForEvent(NSEvent *event) {
  if (!event || event.type != NSEventTypeKeyDown) {
    return GhostexGpuiCEFZoomCommandNone;
  }

  NSEventModifierFlags modifiers =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((modifiers & NSEventModifierFlagCommand) == 0) {
    return GhostexGpuiCEFZoomCommandNone;
  }

  modifiers &= ~NSEventModifierFlagCommand;
  NSString *key = GhostexGpuiShortcutCharactersForEvent(event);
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

  for (NSView *view = (NSView *)responder; view; view = view.superview) {
    if (GhostexGpuiCEFHandleSelectAllForNativeView((__bridge void *)view)) {
      g_ghostexGpuiCEFSelectAllBridged = YES;
      return YES;
    }
  }
  return NO;
}

static BOOL
GhostexGpuiCEFHandleEditCommandForResponder(id responder,
                                            GhostexGpuiCEFEditCommand command) {
  if (command == GhostexGpuiCEFEditCommandNone ||
      ![responder isKindOfClass:NSView.class]) {
    return NO;
  }

  for (NSView *view = (NSView *)responder; view; view = view.superview) {
    if (GhostexGpuiCEFHandleEditCommandForNativeView((__bridge void *)view,
                                                     (int)command)) {
      g_ghostexGpuiCEFEditCommandBridged = YES;
      return YES;
    }
  }
  return NO;
}

static BOOL
GhostexGpuiCEFHandleZoomCommandForResponder(id responder,
                                            GhostexGpuiCEFZoomCommand command) {
  if (command == GhostexGpuiCEFZoomCommandNone ||
      ![responder isKindOfClass:NSView.class]) {
    return NO;
  }

  /*
   CDXC:Hotkeys 2026-07-14:
   Resolve page zoom only by walking from the exact CEF responder to its
   registered Browser/project-workarea root. This keeps Source, Browser,
   Kanban, Automate, and Docs zoom local to the focused pane without window
   event rerouting, overlays, hit-test changes, or stale active-view fallback.
   */
  for (NSView *view = (NSView *)responder; view; view = view.superview) {
    if (GhostexGpuiCEFHandleZoomCommandForNativeView((__bridge void *)view,
                                                     (int)command)) {
      return YES;
    }
  }
  return NO;
}

static NSView *GhostexGpuiCEFMarkFocusedResponder(id responder) {
  if (![responder isKindOfClass:NSView.class]) {
    return nil;
  }

  for (NSView *view = (NSView *)responder; view; view = view.superview) {
    if (GhostexGpuiCEFMarkNativeViewFocused((__bridge void *)view)) {
      return view;
    }
  }
  return nil;
}

void GhostexGpuiCEFFocusNativeView(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  NSWindow *window = view.window;
  if (!window) {
    return;
  }

  /*
   CDXC:FocusRouting 2026-06-14-18:05:
   CEF child views can remain the AppKit first responder after browser
   interaction. When the GPUI-owned address bar is clicked, return
   first-responder ownership to the exact GPUI parent view before focusing the
   GPUI input so typed keys edit the address field instead of continuing into
   Chromium.
   */
  if (!GhostexGpuiCEFMarkNativeViewFocused(nativeView)) {
    GhostexGpuiCEFClearActiveNativeView();
  }
  GhostexGpuiSidebarRevealFocusEditable(nativeView);
  [window makeFirstResponder:view];
}

void GhostexGpuiCEFActivateNativeViewWindow(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  NSWindow *window = view.window;
  if (!view || !window) {
    return;
  }

  /*
   CDXC:FocusRouting 2026-07-15:
   CEF DevTools owns a separate native window. Making its browser view first
   responder is insufficient while the GPUI window remains key: AppKit sends
   application edit shortcuts through the key window's GPUIView instead.
   Activate the real DevTools window before focusing its registered CEF root
   so normal responder-chain Copy/Paste reaches the popup browser.
  */
  if (!GhostexGpuiCEFMarkNativeViewFocused(nativeView)) {
    GhostexGpuiCEFClearActiveNativeView();
  }
  [window makeKeyAndOrderFront:nil];
  [window makeFirstResponder:view];
  id responder = window.firstResponder;
  BOOL responderInsideNativeView = [responder isKindOfClass:NSView.class] &&
                                   GhostexGpuiNativeViewContainsResponder(
                                       nativeView, (__bridge void *)responder);
  GhostexGpuiCEFLogDevToolsWindowActivation(
      window.isKeyWindow ? 1 : 0, responderInsideNativeView ? 1 : 0,
      responder ? object_getClassName(responder) : NULL);
}

void GhostexGpuiCEFFocusGpuiRootView(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view || !view.window) {
    return;
  }

  /*
   CDXC:FocusRouting 2026-07-15:
   A caller that owns the GPUI root already knows this target is not a CEF
   surface. Do not run it through the generic CEF registry probe: clear the
   active Chromium grant unconditionally before AppKit transfers first
   responder to GPUI, so a renderer SYSTEM focus callback from the outgoing
   sidebar cannot reuse stale explicit ownership during the same event.
  */
  GhostexGpuiCEFClearActiveNativeView();
  [view.window makeFirstResponder:view];
}
