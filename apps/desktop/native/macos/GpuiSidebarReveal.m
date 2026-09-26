#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <stdbool.h>
#import <stdint.h>

void GhostexGpuiCEFRefreshSidebarPointerInside(void);

// CDXC:Sidebar 2026-09-09 DECISION:
// User: the collapsed sessions sidebar should slide in fluidly from the left edge on hover and use the same animation in reverse when the pointer leaves.
// User: narrow the reveal region from 30px to 10px at the sidebar edge to avoid triggering it too easily.
// User: while the companion is hidden, the top half reveals Sessions and the bottom half reveals the companion floating, with the same animation and dismissal; neither hover docks a pane. (Superseded 2026-09-20: the companion is gone and one panel carries the sidebar and the sessions column together; see the decision on GhostexGpuiNativeSidebarRevealRequest below.)
// User: Reveal Active Session opens the floating sidebar without changing its saved collapsed state; if it is not hovered within five seconds, animate it closed.
// The existing CEF view moves into a native child panel. Pointer observation does not intercept or reroute page input.

@interface GhostexGpuiSidebarRevealPanel : NSPanel
@end

@implementation GhostexGpuiSidebarRevealPanel
- (BOOL)canBecomeKeyWindow { return YES; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

@interface GhostexGpuiSidebarReveal : NSObject
@property(nonatomic, weak) NSView *sidebar;
@property(nonatomic, weak) NSView *root;
@property(nonatomic, weak) NSView *originalSuperview;
@property(nonatomic) NSRect originalFrame;
@property(nonatomic, strong) GhostexGpuiSidebarRevealPanel *panel;
@property(nonatomic) NSTimeInterval outsideSince;
@property(nonatomic) BOOL attached;
@property(nonatomic) BOOL companion;
@property(nonatomic) BOOL companionTriggerLatched;
@property(nonatomic) NSTimeInterval requestedRevealDeadline;
@property(nonatomic) NSRect targetFrame;
@property(nonatomic) double revealProgress;
@property(nonatomic) double revealStartProgress;
@property(nonatomic) double revealTarget;
@property(nonatomic) NSTimeInterval revealStartedAt;
/// Seconds for one slide, in or out. The GPUI panel is handed the Collapse animation speed
/// setting on every update; the legacy CEF panel keeps its 0.22s.
@property(nonatomic) double slideDuration;
@property(nonatomic, strong) NSTimer *animationTimer;
/// The GPUI panel's own view, whose content size is held while the panel slides.
@property(nonatomic, weak) NSView *hostView;
- (void)layoutReveal;
- (void)animateIn;
- (void)animateTo:(double)target;
- (void)hide;
@end

/// CSS `ease-out`, `cubic-bezier(0, 0, 0.58, 1)`, at `t`: the curve the docked panels slide on.
/// The ends are exact: the bisection alone lands a hair short of 1, which left a slid-out panel at
/// a progress just above 0 that the hosts never recognised as closed, so the next hover did nothing.
static double GhostexGpuiRevealEaseOut(double t) {
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

@implementation GhostexGpuiSidebarReveal
// CDXC:Sidebar 2026-09-23 WHY:
// The GPUI panel slides by resizing its child window, and every intermediate width used to reach
// GPUI as a real resize: a new drawable, a full layout and a repaint of the sidebar and sessions
// column, several times a frame, with AppKit showing the resized window before GPUI had painted
// into it. That is what made the floating slide stutter and flash. While it slides the panel's
// view now holds its full content size (`ghostexSetContentSizeHeld:` in the GPUI macOS crate):
// GPUI keeps the frame it already drew, the layer shows it pinned to the window's right edge, and
// the pointer is mapped the same way, so the slide costs no rendering and cannot flash. Moving a
// full-width window instead would have pushed the panel past the main window's left edge.
- (void)setContentSizeHeld:(BOOL)held {
  NSView *view = self.hostView;
  if (self.companion && [view respondsToSelector:@selector(ghostexSetContentSizeHeld:)]) {
    ((void (*)(id, SEL, BOOL))objc_msgSend)(view, @selector(ghostexSetContentSizeHeld:), held);
  }
}

- (void)layoutReveal {
  NSRect frame = self.targetFrame;
  CGFloat fullWidth = frame.size.width;
  frame.size.width = MAX(1, fullWidth * self.revealProgress);
  if (!NSEqualRects(self.panel.frame, frame)) [self.panel setFrame:frame display:NO];
  if (self.companion) return;
  // Keep Chromium's viewport full-sized while the child window clips the
  // entering page. Resizing the page itself would reflow every session row.
  NSRect sidebarFrame = NSMakeRect(frame.size.width - fullWidth, 0, fullWidth, frame.size.height);
  if (!NSEqualRects(self.sidebar.frame, sidebarFrame)) self.sidebar.frame = sidebarFrame;
  GhostexGpuiCEFRefreshSidebarPointerInside();
}

- (void)animateIn {
  self.revealProgress = 0;
  // Held before the first narrow frame, so GPUI never lays out at a sliver.
  if (self.slideDuration > 0 && !(!self.companion && NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion)) {
    [self setContentSizeHeld:YES];
  }
  [self layoutReveal];
  [self.panel orderFront:nil];
  if (!self.companion) GhostexGpuiCEFRefreshSidebarPointerInside();
  [self animateTo:1];
}

- (void)animateTo:(double)target {
  if (self.revealTarget == target && (self.animationTimer || self.revealProgress == target)) return;
  [self.animationTimer invalidate];
  self.animationTimer = nil;
  self.revealTarget = target;
  self.revealStartProgress = self.revealProgress;
  // The GPUI panel's duration comes from `floating_reveal_slide_duration` in
  // app/floating_reveal/model.rs, already 0 under Off or Reduce Motion, so it is not re-checked here.
  BOOL instant = self.slideDuration <= 0 ||
                 (!self.companion && NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion);
  if (instant) {
    self.revealProgress = target;
    [self setContentSizeHeld:NO];
    [self layoutReveal];
    if (target == 0) {
      [self.panel orderOut:nil];
      if (!self.companion) GhostexGpuiCEFRefreshSidebarPointerInside();
    }
    return;
  }
  self.revealStartedAt = NSProcessInfo.processInfo.systemUptime;
  [self setContentSizeHeld:YES];
  __weak GhostexGpuiSidebarReveal *weakSelf = self;
  self.animationTimer = [NSTimer timerWithTimeInterval:1.0 / 120.0 repeats:YES block:^(NSTimer *timer) {
    GhostexGpuiSidebarReveal *state = weakSelf;
    if (!state || !state.attached) {
      [timer invalidate];
      [state setContentSizeHeld:NO];
      return;
    }
    double elapsed = NSProcessInfo.processInfo.systemUptime - state.revealStartedAt;
    double t = MIN(1, MAX(0, elapsed / state.slideDuration));
    double eased = GhostexGpuiRevealEaseOut(t);
    state.revealProgress = state.revealStartProgress + (state.revealTarget - state.revealStartProgress) * eased;
    [state layoutReveal];
    if (t == 1) {
      [timer invalidate];
      state.animationTimer = nil;
      [state setContentSizeHeld:NO];
      // Keep the CEF view in the hidden panel until Rust's next visibility
      // update detaches it and hides the browser together, avoiding a flash
      // in its old docked frame between animation completion and that update.
      if (state.revealTarget == 0) {
        [state.panel orderOut:nil];
        if (!state.companion) GhostexGpuiCEFRefreshSidebarPointerInside();
      }
    }
  }];
  [NSRunLoop.mainRunLoop addTimer:self.animationTimer forMode:NSRunLoopCommonModes];
}

- (void)hide {
  self.requestedRevealDeadline = 0;
  [self.animationTimer invalidate];
  self.animationTimer = nil;
  [self setContentSizeHeld:NO];
  if (!self.attached) return;
  NSWindow *parent = self.panel.parentWindow;
  BOOL wasKey = self.panel.keyWindow;
  [self.panel orderOut:nil];
  if (!self.companion) GhostexGpuiCEFRefreshSidebarPointerInside();
  [parent removeChildWindow:self.panel];
  if (!self.companion) {
    [self.sidebar removeFromSuperview];
    [self.originalSuperview addSubview:self.sidebar];
    self.sidebar.frame = self.originalFrame;
  }
  self.attached = NO;
  if (wasKey && parent.visible && NSApp.active) [parent makeKeyWindow];
  self.outsideSince = 0;
}
@end

static const void *GhostexGpuiSidebarRevealKey = &GhostexGpuiSidebarRevealKey;

bool GhostexGpuiSidebarRevealUpdate(void *sidebarPtr, void *rootPtr,
                                  bool enabled, double width, double titlebarHeight,
                                  uint32_t backgroundColor,
                                  bool companionHidden, bool requested, bool keepUnderPointer,
                                  bool *expandCompanion) {
  *expandCompanion = false;
  NSView *sidebar = (__bridge NSView *)sidebarPtr;
  NSView *root = (__bridge NSView *)rootPtr;
  if (!sidebar || !root) return false;
  GhostexGpuiSidebarReveal *state = objc_getAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey);
  if (state.attached && state.revealTarget == 0 && state.revealProgress == 0 && !state.animationTimer) {
    [state hide];
  }
  if (!enabled) {
    [state hide];
    return false;
  }
  NSWindow *parent = root.window;
  if (!parent || !parent.visible || parent.miniaturized || !NSApp.active) {
    [state hide];
    return false;
  }
  NSWindow *keyWindow = NSApp.keyWindow;
  NSWindow *keyRoot = keyWindow;
  while (keyRoot.parentWindow) keyRoot = keyRoot.parentWindow;
  if (keyRoot != parent) {
    [state hide];
    return false;
  }
  NSRect body = root.bounds;
  body.size.height = MAX(0, body.size.height - titlebarHeight);
  if (root.flipped) body.origin.y += titlebarHeight;
  body = [parent convertRectToScreen:[root convertRect:body toView:nil]];
  NSPoint pointer = NSEvent.mouseLocation;
  NSRect edge = body;
  edge.size.width = MIN(10, body.size.width);
  BOOL overEdge = NSPointInRect(pointer, edge);
  if (!overEdge) state.companionTriggerLatched = NO;
  if (!state && !overEdge && !requested && !keepUnderPointer) return false;
  if (!state) {
    state = [GhostexGpuiSidebarReveal new];
    state.sidebar = sidebar;
    state.root = root;
    state.slideDuration = 0.22;
    state.panel = [[GhostexGpuiSidebarRevealPanel alloc]
        initWithContentRect:NSZeroRect
                  styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
                    backing:NSBackingStoreBuffered defer:NO];
    state.panel.releasedWhenClosed = NO;
    state.panel.hasShadow = YES;
    state.panel.hidesOnDeactivate = YES;
    state.panel.becomesKeyOnlyIfNeeded = YES;
    state.panel.acceptsMouseMovedEvents = YES;
    state.panel.collectionBehavior = NSWindowCollectionBehaviorFullScreenAuxiliary;
    state.panel.contentView.autoresizesSubviews = NO;
    state.panel.contentView.wantsLayer = YES;
    state.panel.contentView.layer.masksToBounds = YES;
    objc_setAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey, state, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  // CDXC:Theming 2026-09-15 WHY:
  // Chromium can leave the panel backing exposed while the sidebar slides or is reparented, so use the live sidebar color before showing it, including after a theme change.
  NSColor *sidebarBackground = [NSColor
      colorWithSRGBRed:((backgroundColor >> 16) & 0xff) / 255.0
                green:((backgroundColor >> 8) & 0xff) / 255.0
                 blue:(backgroundColor & 0xff) / 255.0
                alpha:1];
  if (![state.panel.backgroundColor isEqual:sidebarBackground]) {
    state.panel.backgroundColor = sidebarBackground;
  }
  if (requested) {
    state.companionTriggerLatched = NO;
    state.requestedRevealDeadline = NSProcessInfo.processInfo.systemUptime + 5;
    state.outsideSince = 0;
  }
  if (!state.attached && state.companionTriggerLatched) return false;
  if (!requested && !keepUnderPointer && !state.attached && overEdge && companionHidden &&
      pointer.y < NSMidY(body)) {
    if (NSEvent.pressedMouseButtons == 0) {
      state.companionTriggerLatched = YES;
      *expandCompanion = true;
    }
    return false;
  }
  NSRect panelFrame = body;
  panelFrame.size.width = MIN(width, body.size.width);
  state.targetFrame = panelFrame;
  if (!state.attached) {
    // CDXC:Sidebar 2026-09-12 DECISION:
    // User: a project or view switch that hides the docked sidebar must not pull it out from under the pointer.
    // While the pointer is over the sidebar's slot it stays as the floating panel at full width, with no slide, and leaves through the ordinary pointer-leave dismissal below.
    BOOL sticky = keepUnderPointer && NSPointInRect(pointer, panelFrame);
    if (!requested && !sticky && (!overEdge || NSEvent.pressedMouseButtons != 0)) return false;
    state.originalSuperview = sidebar.superview;
    state.originalFrame = sidebar.frame;
    [sidebar removeFromSuperview];
    [state.panel.contentView addSubview:sidebar];
    [parent addChildWindow:state.panel ordered:NSWindowAbove];
    state.attached = YES;
    if (sticky) {
      state.requestedRevealDeadline = 0;
      state.outsideSince = 0;
      state.revealTarget = 1;
      state.revealProgress = 1;
      [state layoutReveal];
      [state.panel orderFront:nil];
      GhostexGpuiCEFRefreshSidebarPointerInside();
    } else {
      [state animateIn];
    }
  } else {
    [state layoutReveal];
  }
  if (state.requestedRevealDeadline > 0) {
    if (NSPointInRect(pointer, state.panel.frame)) {
      state.requestedRevealDeadline = 0;
    } else if (NSProcessInfo.processInfo.systemUptime < state.requestedRevealDeadline) {
      [state animateTo:1];
      return true;
    } else {
      state.requestedRevealDeadline = 0;
      [state animateTo:0];
      return state.attached;
    }
  }
  BOOL inside = NSPointInRect(pointer, state.revealTarget == 0 ? state.panel.frame : panelFrame);
  // Native menus and dialogs launched from the sidebar own their input in
  // separate child windows. Keep the sidebar while those controls are used.
  for (NSWindow *child in parent.childWindows) {
    if (child != state.panel && child.visible && NSPointInRect(pointer, child.frame)) {
      inside = YES;
    }
  }
  if (inside || NSEvent.pressedMouseButtons != 0) {
    state.outsideSince = 0;
    [state animateTo:1];
  } else {
    NSTimeInterval now = NSDate.timeIntervalSinceReferenceDate;
    if (state.outsideSince == 0) state.outsideSince = now;
    if (now - state.outsideSince >= 0.2) {
      [state animateTo:0];
    }
  }
  if (state.revealTarget == 0 && state.revealProgress == 0 && !state.animationTimer) {
    [state hide];
    return false;
  }
  return true;
}

// The sidebar's editable-focus bridge is the only reason this passive
// panel should become key. Normal session/project clicks keep typing in the workspace.
void GhostexGpuiSidebarRevealFocusEditable(void *sidebarPtr) {
  NSView *sidebar = (__bridge NSView *)sidebarPtr;
  GhostexGpuiSidebarReveal *state = objc_getAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey);
  if (state.attached) [state.panel makeKeyWindow];
}

void GhostexGpuiSidebarRevealDispose(void *sidebarPtr) {
  NSView *sidebar = (__bridge NSView *)sidebarPtr;
  GhostexGpuiSidebarReveal *state = objc_getAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey);
  [state hide];
  [state.panel close];
  objc_setAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

// A GPUI companion owns a separate window because its header, split controls,
// terminals, and chat pages all need their normal window-local layout and input.
static bool GhostexGpuiNativeRevealUpdate(void *rootPtr, void *popupPtr, bool enabled,
                                         double width, double titlebarHeight, double leftInset,
                                         bool requested, bool sticky, double slideSeconds) {
  NSView *root = (__bridge NSView *)rootPtr;
  NSView *popup = (__bridge NSView *)popupPtr;
  NSWindow *parent = root.window;
  GhostexGpuiSidebarReveal *state = objc_getAssociatedObject(popup, GhostexGpuiSidebarRevealKey);
  NSWindow *keyRoot = NSApp.keyWindow;
  while (keyRoot.parentWindow) keyRoot = keyRoot.parentWindow;
  if (!enabled || !parent.visible || parent.miniaturized || !NSApp.active || keyRoot != parent) {
    [state hide];
    objc_setAssociatedObject(popup, GhostexGpuiSidebarRevealKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    return false;
  }
  if (state && (!state.attached || (state.revealTarget == 0 && state.revealProgress == 0 && !state.animationTimer))) {
    [state hide];
    objc_setAssociatedObject(popup, GhostexGpuiSidebarRevealKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    return false;
  }
  NSRect body = root.bounds;
  body.size.height = MAX(0, body.size.height - titlebarHeight);
  if (root.flipped) body.origin.y += titlebarHeight;
  body = [parent convertRectToScreen:[root convertRect:body toView:nil]];
  // A docked sidebar keeps the window's left edge, so the panel starts where the workarea does.
  CGFloat inset = MIN(MAX(0, leftInset), body.size.width);
  NSRect frame = body;
  frame.origin.x += inset;
  frame.size.width = MIN(width, body.size.width - inset);
  if (!state) {
    state = [GhostexGpuiSidebarReveal new];
    state.companion = YES;
    state.root = root;
    state.hostView = popup;
    state.panel = (GhostexGpuiSidebarRevealPanel *)popup.window;
    state.panel.level = parent.level;
    state.panel.styleMask = NSWindowStyleMaskNonactivatingPanel;
    state.panel.hasShadow = YES;
    state.panel.hidesOnDeactivate = YES;
    [parent addChildWindow:state.panel ordered:NSWindowAbove];
    state.attached = YES;
    state.targetFrame = frame;
    state.slideDuration = slideSeconds;
    objc_setAssociatedObject(popup, GhostexGpuiSidebarRevealKey, state, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    if (sticky) {
      state.revealTarget = 1;
      state.revealProgress = 1;
      [state layoutReveal];
      [state.panel orderFront:nil];
    } else {
      [state animateIn];
    }
  } else {
    state.targetFrame = frame;
    // A settings change reaches the next slide; one already running keeps its clock.
    if (!state.animationTimer) state.slideDuration = slideSeconds;
    [state layoutReveal];
  }
  NSPoint pointer = NSEvent.mouseLocation;
  if (requested) state.requestedRevealDeadline = NSProcessInfo.processInfo.systemUptime + 5;
  if (state.requestedRevealDeadline > 0) {
    if (NSPointInRect(pointer, state.panel.frame)) state.requestedRevealDeadline = 0;
    else if (NSProcessInfo.processInfo.systemUptime < state.requestedRevealDeadline) {
      [state animateTo:1];
      return true;
    } else {
      state.requestedRevealDeadline = 0;
      [state animateTo:0];
      return true;
    }
  }
  // Beside a docked sidebar the pointer that opened the panel is still over the sidebar (its left
  // edge, or the session row that was clicked), so the sidebar's slot keeps the panel open too.
  NSRect keep = frame;
  keep.origin.x -= inset;
  keep.size.width += inset;
  BOOL inside = NSPointInRect(pointer, state.revealTarget == 0 ? state.panel.frame : keep);
  for (NSWindow *child in parent.childWindows) {
    if (child != state.panel && child.visible && NSPointInRect(pointer, child.frame)) inside = YES;
  }
  if (inside || NSEvent.pressedMouseButtons != 0) {
    state.outsideSince = 0;
    [state animateTo:1];
  } else {
    NSTimeInterval now = NSProcessInfo.processInfo.systemUptime;
    if (state.outsideSince == 0) state.outsideSince = now;
    if (now - state.outsideSince >= 0.2) [state animateTo:0];
  }
  return true;
}

bool GhostexGpuiCompanionRevealUpdate(void *root, void *popup, bool enabled, double width, double titlebarHeight) {
  return GhostexGpuiNativeRevealUpdate(root, popup, enabled, width, titlebarHeight, 0, false, false, 0.22);
}

bool GhostexGpuiNativeSidebarRevealUpdate(void *root, void *popup, bool enabled, double width, double titlebarHeight, double leftInset, bool requested, bool sticky, double slideSeconds) {
  return GhostexGpuiNativeRevealUpdate(root, popup, enabled, width, titlebarHeight, leftInset, requested, sticky, slideSeconds);
}

// True while the panel slides out, so Rust can take down the chat windows that sit over it.
bool GhostexGpuiNativeSidebarRevealLeaving(void *popupPtr) {
  NSView *popup = (__bridge NSView *)popupPtr;
  GhostexGpuiSidebarReveal *state = objc_getAssociatedObject(popup, GhostexGpuiSidebarRevealKey);
  return state.attached && state.revealTarget == 0;
}

// CDXC:Sidebar 2026-09-20 DECISION:
// User (ruling 7B, screen 10): edge-hover floating works on macOS, Windows and Linux, and its hot
// zone is a real edge strip rather than an invisible layer over the content. The strip is a GPUI
// sibling in the main window's body row and it is the only trigger, so this no longer measures the
// pointer against a 10px rectangle of its own: Wayland cannot answer where the pointer is, and two
// measurements would have made one gesture mean two things. It supersedes the 2026-09-09 clause at
// the top of this file that split the edge into a Sessions half and a companion half; phase 3
// deleted the companion, and one panel carries both columns in its place.
// The pointer is still read for the sticky case, where a layout change must not pull the sidebar
// out from under it. This observes the pointer; each GPUI child window owns its normal input.
int GhostexGpuiNativeSidebarRevealRequest(void *rootPtr, double width, double titlebarHeight, double leftInset, bool edgeHovered, bool requested, bool keepUnderPointer) {
  NSView *root = (__bridge NSView *)rootPtr;
  NSWindow *parent = root.window;
  NSWindow *keyRoot = NSApp.keyWindow;
  while (keyRoot.parentWindow) keyRoot = keyRoot.parentWindow;
  if (!parent || !parent.visible || parent.miniaturized || !NSApp.active || keyRoot != parent) return 0;
  NSRect body = root.bounds;
  body.size.height = MAX(0, body.size.height - titlebarHeight);
  if (root.flipped) body.origin.y += titlebarHeight;
  body = [parent convertRectToScreen:[root convertRect:body toView:nil]];
  NSPoint pointer = NSEvent.mouseLocation;
  CGFloat inset = MIN(MAX(0, leftInset), body.size.width);
  NSRect slot = body;
  slot.origin.x += inset;
  slot.size.width = MIN(width, body.size.width - inset);
  if (requested || (keepUnderPointer && NSPointInRect(pointer, slot))) return 1;
  if (!edgeHovered || NSEvent.pressedMouseButtons != 0) return 0;
  return 1;
}

// The collapsed sidebar's hot zone: a transparent view over the left edge of the main window's
// content, kept above the CEF pages so a page reaching the window edge cannot hide it (the decision
// is on `floating_reveal_edge_want` in app/floating_reveal/edge_strip.rs). It takes the clicks in
// its frame and records the pointer for the reveal's poll: movement arms the reveal and leaving
// disarms it, as the GPUI zone does on the other platforms.
@interface GhostexGpuiRevealEdgeView : NSView
/// 0 nothing new, 1 the pointer moved in the zone (at `pendingYFraction` from the top), 2 it left.
@property(nonatomic) int pending;
@property(nonatomic) double pendingYFraction;
@end

@implementation GhostexGpuiRevealEdgeView
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstMouse:(NSEvent *)event { return YES; }
- (void)updateTrackingAreas {
  for (NSTrackingArea *area in [self.trackingAreas copy]) [self removeTrackingArea:area];
  [self addTrackingArea:[[NSTrackingArea alloc]
      initWithRect:NSZeroRect
           options:NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved |
                   NSTrackingActiveInActiveApp | NSTrackingInVisibleRect
             owner:self
          userInfo:nil]];
  [super updateTrackingAreas];
}
- (void)recordPointer:(NSEvent *)event {
  NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
  CGFloat height = self.bounds.size.height;
  self.pendingYFraction = height > 0 ? point.y / height : 0;
  self.pending = 1;
}
- (void)mouseEntered:(NSEvent *)event { [self recordPointer:event]; }
- (void)mouseMoved:(NSEvent *)event { [self recordPointer:event]; }
- (void)mouseExited:(NSEvent *)event { self.pending = 2; }
- (void)mouseDown:(NSEvent *)event {}
- (void)mouseUp:(NSEvent *)event {}
- (void)mouseDragged:(NSEvent *)event {}
- (void)rightMouseDown:(NSEvent *)event {}
- (void)rightMouseUp:(NSEvent *)event {}
- (void)otherMouseDown:(NSEvent *)event {}
- (void)otherMouseUp:(NSEvent *)event {}
@end

static const void *GhostexGpuiRevealEdgeKey = &GhostexGpuiRevealEdgeKey;

void GhostexGpuiRevealEdgeSync(void *rootPtr, bool visible, double width) {
  NSView *root = (__bridge NSView *)rootPtr;
  if (!root) return;
  GhostexGpuiRevealEdgeView *edge = objc_getAssociatedObject(root, GhostexGpuiRevealEdgeKey);
  if (!visible) {
    if (edge) {
      [edge removeFromSuperview];
      objc_setAssociatedObject(root, GhostexGpuiRevealEdgeKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    }
    return;
  }
  if (!edge) {
    edge = [[GhostexGpuiRevealEdgeView alloc] initWithFrame:NSZeroRect];
    objc_setAssociatedObject(root, GhostexGpuiRevealEdgeKey, edge, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  NSRect frame = NSMakeRect(0, 0, MIN(width, root.bounds.size.width), root.bounds.size.height);
  if (!NSEqualRects(edge.frame, frame)) edge.frame = frame;
  // A CEF page created or reparented since the last sweep is added above this view.
  if (edge.superview != root || root.subviews.lastObject != edge) {
    [root addSubview:edge positioned:NSWindowAbove relativeTo:nil];
  }
}

int GhostexGpuiRevealEdgeTake(void *rootPtr, double *yFraction) {
  NSView *root = (__bridge NSView *)rootPtr;
  if (!root) return 0;
  GhostexGpuiRevealEdgeView *edge = objc_getAssociatedObject(root, GhostexGpuiRevealEdgeKey);
  if (!edge) return 0;
  int pending = edge.pending;
  if (yFraction) *yFraction = edge.pendingYFraction;
  edge.pending = 0;
  return pending;
}

bool GhostexGpuiReparentPaneNativeView(void *viewPtr, void *parentPtr, void *fromPtr) {
  NSView *view = (__bridge NSView *)viewPtr;
  NSView *parent = (__bridge NSView *)parentPtr;
  NSView *from = (__bridge NSView *)fromPtr;
  if (!view || !parent || view.superview == parent) return false;
  if (from && view.window != from.window) return false;
  [view removeFromSuperview];
  [parent addSubview:view];
  return true;
}
