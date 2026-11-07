#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <stdbool.h>
#import <stdint.h>

void GhostexGpuiCEFRefreshSidebarPointerInside(void);
void GhostexGpuiShowChildWindowAboveParent(NSWindow *child, NSWindow *parent);

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

// CDXC:Sidebar 2026-11-07 DECISION:
// User approved PR #455 ("OK after your notes are addressed"), including this overlap, as the
// hit-testing rule in AGENTS.md requires; GitHub #432 asked for the idea. With the sidebar hidden, the floating sidebar shows the main window's
// own traffic lights in its top-left, where the window keeps them. The window's lights hide with
// the sidebar (Rust asks through `GhostexGpuiSetWindowControlsHidden`); once the panel has slid
// all the way in, the panel lends them back (`lent`) and leaves a hole the size of each light in
// its shape, so the real buttons show through with AppKit's own colours, hover glyphs, the green
// button's menu, Option-zoom and VoiceOver, and a click there reaches them: AppKit passes clicks
// through the fully transparent pixels of a non-opaque window. Holes, not drawn copies of the
// lights, are the one place the panel overlaps the window's own controls; copies of the buttons in the panel would draw grey, since the panel never becomes the main window.
static const NSWindowButton GhostexGpuiRevealWindowButtons[] = {
    NSWindowCloseButton, NSWindowMiniaturizeButton, NSWindowZoomButton};
static const void *GhostexGpuiWindowLightsHiddenKey = &GhostexGpuiWindowLightsHiddenKey;
static const void *GhostexGpuiWindowLightsLentKey = &GhostexGpuiWindowLightsLentKey;

/// Hidden while Rust asks for it and no floating panel has lent the lights back.
///
/// Hiding or showing a standard button makes AppKit lay the title bar out again at its default
/// spot, which loses the position GPUI gave the lights (`traffic_light_position`, 11 by 11.5 or one
/// frame gutter more) until the window next resizes. So AppKit's layout runs at once and the frames
/// the buttons and their container had a moment before are put back.
static void GhostexGpuiApplyWindowLights(NSWindow *window) {
  BOOL requested = [objc_getAssociatedObject(window, GhostexGpuiWindowLightsHiddenKey) boolValue];
  BOOL lent = [objc_getAssociatedObject(window, GhostexGpuiWindowLightsLentKey) boolValue];
  BOOL hidden = requested && !lent;
  NSButton *buttons[3];
  NSRect frames[3];
  BOOL changes = NO;
  for (size_t i = 0; i < 3; i++) {
    buttons[i] = [window standardWindowButton:GhostexGpuiRevealWindowButtons[i]];
    frames[i] = buttons[i].frame;
    if (buttons[i] && buttons[i].hidden != hidden) changes = YES;
  }
  if (!changes) return;
  NSView *container = buttons[0].superview.superview;
  NSRect containerFrame = container.frame;
  for (size_t i = 0; i < 3; i++) buttons[i].hidden = hidden;
  [container.superview layoutSubtreeIfNeeded];
  if (container && !NSEqualRects(container.frame, containerFrame)) container.frame = containerFrame;
  for (size_t i = 0; i < 3; i++) {
    if (!buttons[i]) continue;
    if (!NSEqualRects(buttons[i].frame, frames[i])) buttons[i].frame = frames[i];
    [buttons[i] updateTrackingAreas];
  }
  [container updateTrackingAreas];
}

static void GhostexGpuiSetWindowLightsLent(NSWindow *window, BOOL lent) {
  objc_setAssociatedObject(window, GhostexGpuiWindowLightsLentKey, @(lent), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  GhostexGpuiApplyWindowLights(window);
}

static const void *GhostexGpuiRevealPanelNeverMainKey = &GhostexGpuiRevealPanelNeverMainKey;
static IMP GhostexGpuiRevealPanelCanBecomeMainOriginal;

static BOOL GhostexGpuiRevealPanelCanBecomeMain(NSWindow *window, SEL selector) {
  if ([objc_getAssociatedObject(window, GhostexGpuiRevealPanelNeverMainKey) boolValue]) return NO;
  return ((BOOL (*)(id, SEL))GhostexGpuiRevealPanelCanBecomeMainOriginal)(window, selector);
}

/// CDXC:Sidebar 2026-11-07 WHY:
/// GPUI's window classes answer YES to `canBecomeMainWindow`, panels included, where a plain NSPanel
/// answers NO. So a click into the floating sidebar made it the main window, the workspace window
/// lost main, and the traffic lights the panel shows for it (GhostexGpuiSetWindowLightsLent) drew
/// in their faint inactive style, through which the header's toggles under them showed (GitHub #432
/// follow-up). The panel takes keys as before but never becomes main, as an NSPanel normally can't.
/// Only this panel is marked: GPUI's other windows of the same class keep its answer. The method is
/// replaced on GPUI's own class and the instance keeps its class; changing the class of a window
/// AppKit observes with KVO made GPUI panic when it was freed (2026-11-06).
static void GhostexGpuiRevealPanelNeverMain(NSWindow *panel) {
  objc_setAssociatedObject(panel, GhostexGpuiRevealPanelNeverMainKey, @YES, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    Class owner = [panel class];
    unsigned count = 0;
    Method *methods = class_copyMethodList(owner, &count);
    Method own = NULL;
    for (unsigned i = 0; i < count; i++) {
      if (method_getName(methods[i]) == @selector(canBecomeMainWindow)) own = methods[i];
    }
    free(methods);
    // Only GPUI's own answer is replaced, never AppKit's NSPanel or NSWindow one.
    if (own) GhostexGpuiRevealPanelCanBecomeMainOriginal = method_setImplementation(own, (IMP)GhostexGpuiRevealPanelCanBecomeMain);
  });
  if (panel.isMainWindow) [panel.parentWindow makeMainWindow];
}

@interface GhostexGpuiSidebarReveal : NSObject
@property(nonatomic, weak) NSView *sidebar;
@property(nonatomic, weak) NSView *root;
@property(nonatomic, weak) NSView *originalSuperview;
@property(nonatomic) NSRect originalFrame;
@property(nonatomic, strong) GhostexGpuiSidebarRevealPanel *panel;
@property(nonatomic) NSTimeInterval outsideSince;
@property(nonatomic) BOOL attached;
@property(nonatomic) BOOL companion;
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
/// The panel shows its main window's own traffic lights in its top-left (`wantWindowLights:`).
@property(nonatomic) BOOL windowLightsWanted;
/// The lights are lent back to the window and show through the panel's holes now.
@property(nonatomic) BOOL windowLightsOpen;
@property(nonatomic, weak) NSWindow *windowLightsOwner;
/// The panel's left corner radii where they sit on the main window's rounded corners; 0 leaves
/// the corners to the radius `GhostexGpuiRoundFloatingPanel` gave the GPUI view.
@property(nonatomic) CGFloat topLeftRadius;
@property(nonatomic) CGFloat bottomLeftRadius;
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

- (void)wantWindowLights:(BOOL)wanted {
  if (self.windowLightsWanted == wanted) return;
  self.windowLightsWanted = wanted;
  // A hole in a window's shape lets its shadow through too, which would shade the lights; the panel
  // that shows them casts none.
  self.panel.hasShadow = !wanted;
  [self.panel invalidateShadow];
  [self syncWindowLights];
}

/// The lights show only while the panel is all the way in: they belong to the main window and stay
/// where they are, so during a slide they would sit over the header instead of the panel. Shown,
/// the window draws them before the holes open; hidden, the holes close before they go, so neither
/// order shows what the window draws under them.
- (void)syncWindowLights {
  NSWindow *owner = self.windowLightsOwner ?: self.panel.parentWindow;
  BOOL open = self.attached && owner && self.windowLightsWanted && self.revealTarget == 1 &&
              self.revealProgress >= 1;
  if (open == self.windowLightsOpen) return;
  self.windowLightsOpen = open;
  if (open) {
    self.windowLightsOwner = owner;
    GhostexGpuiSetWindowLightsLent(owner, YES);
    [owner displayIfNeeded];
    [CATransaction flush];
    [self layoutCornerMask];
  } else {
    [self layoutCornerMask];
    [CATransaction flush];
    if (owner) GhostexGpuiSetWindowLightsLent(owner, NO);
    self.windowLightsOwner = nil;
  }
}

/// CDXC:Sidebar 2026-11-06 WHY:
/// The panel's left corners take the window's radius where they sit on its corners (the decision
/// and the radii are `floating_panel_left_corner_radii` in app/floating_reveal/host_macos.rs). The GPUI view, its glass and wallpaper views each round all
/// four corners alike, so the panel's content view is masked instead: the left corners at their own
/// radii, the right ones at the GPUI view's. The panel's left edge stays on the window's left edge
/// while it slides, so the mask follows the panel's live bounds.
- (void)layoutCornerMask {
  NSView *content = self.panel.contentView;
  CGFloat panelRadius = self.hostView.layer.cornerRadius;
  if (!content) return;
  BOOL masked = self.topLeftRadius > panelRadius || self.bottomLeftRadius > panelRadius ||
                self.windowLightsOpen;
  if (!masked) {
    if (content.layer.mask) content.layer.mask = nil;
    return;
  }
  content.wantsLayer = YES;
  CAShapeLayer *mask = [content.layer.mask isKindOfClass:CAShapeLayer.class] ? (CAShapeLayer *)content.layer.mask : nil;
  NSRect bounds = content.bounds;
  CGFloat top = MAX(self.topLeftRadius, panelRadius);
  CGFloat bottom = MAX(self.bottomLeftRadius, panelRadius);
  CGFloat limit = MIN(bounds.size.width, bounds.size.height) / 2;
  top = MIN(top, limit);
  bottom = MIN(bottom, limit);
  CGFloat right = MIN(panelRadius, limit);
  // In the view's coordinates, which the layer's sublayers share: the top is the low y when flipped.
  CGFloat minX = NSMinX(bounds), maxX = NSMaxX(bounds);
  CGFloat topY = content.flipped ? NSMinY(bounds) : NSMaxY(bounds);
  CGFloat bottomY = content.flipped ? NSMaxY(bounds) : NSMinY(bounds);
  CGMutablePathRef path = CGPathCreateMutable();
  CGPathMoveToPoint(path, NULL, minX, (topY + bottomY) / 2);
  CGPathAddArcToPoint(path, NULL, minX, topY, maxX, topY, top);
  CGPathAddArcToPoint(path, NULL, maxX, topY, maxX, bottomY, right);
  CGPathAddArcToPoint(path, NULL, maxX, bottomY, minX, bottomY, right);
  CGPathAddArcToPoint(path, NULL, minX, bottomY, minX, topY, bottom);
  CGPathCloseSubpath(path);
  // A hole over each of the window's lights, one device pixel inside its edge (AppKit draws the
  // light across its whole 14pt frame), so the hole's own anti-aliased edge falls on the light and
  // nothing the window draws around it shows through.
  NSWindow *owner = self.windowLightsOwner;
  if (self.windowLightsOpen && owner) {
    for (size_t i = 0; i < 3; i++) {
      NSButton *button = [owner standardWindowButton:GhostexGpuiRevealWindowButtons[i]];
      if (!button || button.hidden) continue;
      NSRect rect = [owner convertRectToScreen:[button convertRect:button.bounds toView:nil]];
      rect = [content convertRect:[self.panel convertRectFromScreen:rect] fromView:nil];
      CGPathAddEllipseInRect(path, NULL, NSRectToCGRect(NSInsetRect(rect, 0.5, 0.5)));
    }
  }
  [CATransaction begin];
  [CATransaction setDisableActions:YES];
  BOOL fresh = mask == nil;
  if (fresh) mask = [CAShapeLayer layer];
  mask.frame = content.layer.bounds;
  mask.fillRule = kCAFillRuleEvenOdd;
  mask.path = path;
  if (fresh) content.layer.mask = mask;
  [CATransaction commit];
  CGPathRelease(path);
  if (fresh) [self.panel invalidateShadow];
}

- (void)setLeftCornerRadiiTop:(CGFloat)top bottom:(CGFloat)bottom {
  if (self.topLeftRadius == top && self.bottomLeftRadius == bottom) return;
  self.topLeftRadius = top;
  self.bottomLeftRadius = bottom;
  [self layoutCornerMask];
  [self.panel invalidateShadow];
}

- (void)layoutReveal {
  NSRect frame = self.targetFrame;
  CGFloat fullWidth = frame.size.width;
  frame.size.width = MAX(1, fullWidth * self.revealProgress);
  if (!NSEqualRects(self.panel.frame, frame)) [self.panel setFrame:frame display:NO];
  [self syncWindowLights];
  [self layoutCornerMask];
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
  GhostexGpuiShowChildWindowAboveParent(self.panel, self.panel.parentWindow);
  if (!self.companion) GhostexGpuiCEFRefreshSidebarPointerInside();
  [self animateTo:1];
}

- (void)animateTo:(double)target {
  if (self.revealTarget == target && (self.animationTimer || self.revealProgress == target)) return;
  [self.animationTimer invalidate];
  self.animationTimer = nil;
  self.revealTarget = target;
  self.revealStartProgress = self.revealProgress;
  // Leaving, the window takes its lights back before the panel starts to move.
  [self syncWindowLights];
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
  self.windowLightsWanted = NO;
  [self syncWindowLights];
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
                                         bool requested, bool sticky, double slideSeconds,
                                         bool windowControls, double topLeftRadius,
                                         double bottomLeftRadius) {
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
    GhostexGpuiRevealPanelNeverMain(state.panel);
    [state wantWindowLights:windowControls];
    [state setLeftCornerRadiiTop:topLeftRadius bottom:bottomLeftRadius];
    if (sticky) {
      state.revealTarget = 1;
      state.revealProgress = 1;
      [state layoutReveal];
      GhostexGpuiShowChildWindowAboveParent(state.panel, parent);
    } else {
      [state animateIn];
    }
  } else {
    state.targetFrame = frame;
    // A settings change reaches the next slide; one already running keeps its clock.
    if (!state.animationTimer) state.slideDuration = slideSeconds;
    [state wantWindowLights:windowControls];
    [state setLeftCornerRadiiTop:topLeftRadius bottom:bottomLeftRadius];
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
  return GhostexGpuiNativeRevealUpdate(root, popup, enabled, width, titlebarHeight, 0, false, false, 0.22, false, 0, 0);
}

bool GhostexGpuiNativeSidebarRevealUpdate(void *root, void *popup, bool enabled, double width, double titlebarHeight, double leftInset, bool requested, bool sticky, double slideSeconds, bool windowControls, double topLeftRadius, double bottomLeftRadius) {
  return GhostexGpuiNativeRevealUpdate(root, popup, enabled, width, titlebarHeight, leftInset, requested, sticky, slideSeconds, windowControls, topLeftRadius, bottomLeftRadius);
}

// Hides or shows the main window's own traffic lights as Rust asks: hidden while the sidebar is
// hidden. An open floating sidebar lends them back (GhostexGpuiSetWindowLightsLent). Rust calls it
// on every render of the window, so it only writes on a change.
void GhostexGpuiSetWindowControlsHidden(void *rootPtr, bool hidden) {
  NSWindow *window = ((__bridge NSView *)rootPtr).window;
  if (!window) return;
  NSNumber *requested = objc_getAssociatedObject(window, GhostexGpuiWindowLightsHiddenKey);
  if (!requested || requested.boolValue != hidden) {
    objc_setAssociatedObject(window, GhostexGpuiWindowLightsHiddenKey, @(hidden), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  GhostexGpuiApplyWindowLights(window);
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
  // CDXC:Sidebar 2026-09-12 DECISION:
  // User: a project or view switch that hides the docked sidebar must not pull it out from under the pointer.
  // While the pointer is over the sidebar's slot it stays as the floating panel at full width, with no slide, and leaves through the ordinary pointer-leave dismissal in GhostexGpuiNativeRevealUpdate.
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
