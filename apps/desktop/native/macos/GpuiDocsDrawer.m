#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <stdbool.h>

// CDXC:Docs 2026-09-25 DECISION:
// User: "the animation when the files list shows is not smooth like the sessions list". The Docs
// view's floating files list slides the way the floating sessions sidebar does
// (GpuiSidebarReveal.m): AppKit resizes the list's child window from the view's right edge on its
// own 120Hz timer, on the panels' ease-out curve, while GPUI keeps the frame it already drew
// pinned to the window's left edge (`ghostexSetContentSizeHeldFromLeft:`), so the slide costs no
// rendering. Moving the window step by step from Rust re-rendered the list at every width.

@interface GhostexGpuiDocsDrawer : NSObject
@property(nonatomic, weak) NSView *hostView;
@property(nonatomic, weak) NSWindow *parent;
@property(nonatomic) NSRect targetFrame;
@property(nonatomic) double progress;
@property(nonatomic) double startProgress;
@property(nonatomic) double target;
@property(nonatomic) double duration;
@property(nonatomic) NSTimeInterval startedAt;
@property(nonatomic, strong) NSTimer *timer;
/// Watches the main window's clicks while the list is out.
@property(nonatomic, strong) id clickMonitor;
/// A click landed in the main window (on the document, a browser page, the header) since Docs last
/// asked. The page under an HTML file takes its clicks natively, so Docs would never hear of them.
@property(nonatomic) BOOL outsideClick;
@end

static char GhostexGpuiDocsDrawerKey;

/// CSS `ease-out`, the curve the sessions sidebar slides on (`GhostexGpuiRevealEaseOut`).
static double GhostexGpuiDocsDrawerEaseOut(double t) {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  double low = 0, high = 1;
  for (int i = 0; i < 20; i++) {
    double u = (low + high) / 2;
    double x = 3 * (1 - u) * u * u * 0.58 + u * u * u;
    if (x < t) low = u; else high = u;
  }
  double u = (low + high) / 2;
  return 3 * (1 - u) * u * u + u * u * u;
}

@implementation GhostexGpuiDocsDrawer
- (NSWindow *)window {
  return self.hostView.window;
}

- (void)setHeld:(BOOL)held {
  NSView *view = self.hostView;
  if ([view respondsToSelector:@selector(ghostexSetContentSizeHeldFromLeft:)]) {
    ((void (*)(id, SEL, BOOL))objc_msgSend)(view, @selector(ghostexSetContentSizeHeldFromLeft:), held);
  }
}

- (void)layout {
  NSRect frame = self.targetFrame;
  CGFloat full = frame.size.width;
  CGFloat width = MAX(1, full * self.progress);
  frame.origin.x = NSMaxX(frame) - width;
  frame.size.width = width;
  NSWindow *window = self.window;
  if (!NSEqualRects(window.frame, frame)) [window setFrame:frame display:NO];
}

// CDXC:Docs 2026-09-25 WHY:
// The window is kept between uses, unlike the sessions panel, which opens a new one each time.
// Releasing the hold at the end of a slide-out, while the window was a 1pt sliver, made GPUI lay
// the list out and draw it 1pt wide, and the next slide-in held that empty frame, so the list
// came in as bare glass and appeared only once the slide ended. The window goes back to its full
// width after it is ordered out and before the hold is released, so GPUI never lays the list out
// at a sliver.
- (void)finish {
  [self.timer invalidate];
  self.timer = nil;
  if (self.target == 0) {
    [self.window orderOut:nil];
    self.progress = 1;
    [self layout];
    [self setHeld:NO];
    self.progress = 0;
    if (self.clickMonitor) [NSEvent removeMonitor:self.clickMonitor];
    self.clickMonitor = nil;
    self.outsideClick = NO;
  } else {
    self.progress = 1;
    [self layout];
    [self setHeld:NO];
    [self.window invalidateShadow];
  }
}

- (void)animateTo:(double)target {
  if (self.target == target && (self.timer || self.progress == target)) return;
  [self.timer invalidate];
  self.timer = nil;
  self.target = target;
  if (self.duration <= 0) {
    [self finish];
    return;
  }
  self.startProgress = self.progress;
  self.startedAt = NSProcessInfo.processInfo.systemUptime;
  [self setHeld:YES];
  __weak GhostexGpuiDocsDrawer *weakSelf = self;
  self.timer = [NSTimer timerWithTimeInterval:1.0 / 120.0
                                      repeats:YES
                                        block:^(NSTimer *timer) {
                                          GhostexGpuiDocsDrawer *state = weakSelf;
                                          if (!state || !state.window) {
                                            [timer invalidate];
                                            return;
                                          }
                                          double elapsed = NSProcessInfo.processInfo.systemUptime -
                                                           state.startedAt;
                                          double t = MIN(1, elapsed / state.duration);
                                          if (t >= 1) {
                                            [state finish];
                                            return;
                                          }
                                          double eased = GhostexGpuiDocsDrawerEaseOut(t);
                                          state.progress = state.startProgress +
                                                           (state.target - state.startProgress) * eased;
                                          [state layout];
                                        }];
  [[NSRunLoop mainRunLoop] addTimer:self.timer forMode:NSRunLoopCommonModes];
}
@end

/// Shows the drawer at `x, y, width, height` in the main window's content coordinates (top-left
/// origin), sliding in from the right when it was hidden. Called again with a new frame while it
/// is out, it follows the view without a slide.
void GhostexGpuiDocsDrawerShow(void *drawerView, void *mainView, double x, double y, double width,
                               double height, double slideSeconds) {
  @autoreleasepool {
    if (drawerView == NULL || mainView == NULL) return;
    NSView *host = (__bridge NSView *)drawerView;
    NSWindow *window = host.window;
    NSWindow *parent = ((__bridge NSView *)mainView).window;
    if (window == nil || parent == nil || window == parent) return;
    NSRect content = [parent contentRectForFrameRect:parent.frame];
    NSRect frame = NSMakeRect(content.origin.x + x,
                              content.origin.y + content.size.height - y - height, width, height);
    GhostexGpuiDocsDrawer *state = objc_getAssociatedObject(host, &GhostexGpuiDocsDrawerKey);
    if (!state) {
      state = [GhostexGpuiDocsDrawer new];
      state.hostView = host;
      objc_setAssociatedObject(host, &GhostexGpuiDocsDrawerKey, state,
                               OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    }
    state.targetFrame = frame;
    state.duration = slideSeconds;
    BOOL hidden = !window.visible || (state.progress == 0 && state.target == 0);
    if (hidden) {
      window.level = parent.level;
      window.hasShadow = YES;
      // A panel hides itself when the app deactivates, behind Docs' back; the list stays out
      // until Docs closes it.
      window.hidesOnDeactivate = NO;
      if (window.parentWindow != parent) [parent addChildWindow:window ordered:NSWindowAbove];
      state.progress = 0;
      state.target = 0;
      state.parent = parent;
      state.outsideClick = NO;
      if (!state.clickMonitor) {
        __weak GhostexGpuiDocsDrawer *weakState = state;
        state.clickMonitor = [NSEvent
            addLocalMonitorForEventsMatchingMask:(NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown)
                                         handler:^NSEvent *(NSEvent *event) {
                                           GhostexGpuiDocsDrawer *watched = weakState;
                                           if (watched && watched.target == 1 &&
                                               event.window == watched.parent) {
                                             watched.outsideClick = YES;
                                           }
                                           return event;
                                         }];
      }
      // Full width first, then held before the first narrow frame, so the frame GPUI holds is
      // the whole list and it is never laid out at a sliver.
      state.progress = 1;
      [state layout];
      if (slideSeconds > 0) [state setHeld:YES];
      state.progress = 0;
      [state layout];
      [window orderFront:nil];
      // Present the list now, in this transaction, so the first frame of the slide already shows
      // it; GPUI would otherwise draw only once the window's display link starts, after the
      // system reports the window visible, which can be the whole slide later. The caller has
      // marked the window dirty and calls this outside any app update, which GPUI's synchronous
      // `displayLayer:` needs.
      [host.layer setNeedsDisplay];
      [host.layer displayIfNeeded];
      [state animateTo:1];
      return;
    }
    if (state.target == 0) {
      [state animateTo:1];
      return;
    }
    if (!state.timer) [state layout];
  }
}

/// Takes the main-window click recorded since the last call.
bool GhostexGpuiDocsDrawerTakeOutsideClick(void *drawerView) {
  if (drawerView == NULL) return false;
  NSView *host = (__bridge NSView *)drawerView;
  GhostexGpuiDocsDrawer *state = objc_getAssociatedObject(host, &GhostexGpuiDocsDrawerKey);
  BOOL clicked = state.outsideClick;
  state.outsideClick = NO;
  return clicked;
}

/// Whether the drawer's window is on screen (or sliding in).
bool GhostexGpuiDocsDrawerVisible(void *drawerView) {
  if (drawerView == NULL) return false;
  NSView *host = (__bridge NSView *)drawerView;
  GhostexGpuiDocsDrawer *state = objc_getAssociatedObject(host, &GhostexGpuiDocsDrawerKey);
  return host.window.visible && state.target == 1;
}

/// Slides the drawer out and orders it out at the end. The window is kept for the next show.
void GhostexGpuiDocsDrawerHide(void *drawerView, double slideSeconds) {
  @autoreleasepool {
    if (drawerView == NULL) return;
    NSView *host = (__bridge NSView *)drawerView;
    GhostexGpuiDocsDrawer *state = objc_getAssociatedObject(host, &GhostexGpuiDocsDrawerKey);
    if (!state || !host.window.visible) {
      [host.window orderOut:nil];
      return;
    }
    state.duration = slideSeconds;
    [state animateTo:0];
  }
}

// CDXC:Sidebar 2026-09-25 DECISION:
// User: "i like how you're rounding the files list from all 4 corners, apply that on the files list
// and the floating agent chat pls". The Docs view's floating files list and the floating sessions
// panel (sidebar and sessions column) are rounded on all four corners. GPUI clips only to
// rectangles, so the corners are cut from the window's GPUI layer here, and the glass backdrop
// behind it takes the same radius (`set_background_corner_radius`).
void GhostexGpuiRoundFloatingPanel(void *nativeView, double radius) {
  @autoreleasepool {
    if (nativeView == NULL) return;
    NSView *view = (__bridge NSView *)nativeView;
    NSWindow *window = view.window;
    view.wantsLayer = YES;
    CALayer *layer = view.layer;
    if (layer.cornerRadius == radius && layer.masksToBounds) return;
    layer.cornerRadius = radius;
    if (@available(macOS 10.15, *)) layer.cornerCurve = kCACornerCurveContinuous;
    layer.masksToBounds = YES;
    window.opaque = NO;
    window.backgroundColor = NSColor.clearColor;
    [window invalidateShadow];
  }
}
