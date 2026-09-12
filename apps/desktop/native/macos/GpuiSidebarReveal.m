#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>
#import <stdbool.h>

void GhostexGpuiCEFClearActiveNativeView(void);
void GhostexGpuiCEFRefreshSidebarPointerInside(void);

// CDXC:Sidebar 2026-09-09 DECISION:
// User: the collapsed sessions sidebar should slide in fluidly from its configured side on hover and use the same animation in reverse when the pointer leaves.
// User: narrow the reveal region from 30px to 10px at the configured sidebar edge to avoid triggering it too easily.
// User: while the companion is hidden, the top half reveals Sessions and the bottom half reveals the companion floating, with the same animation and dismissal; neither hover docks a pane.
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
@property(nonatomic) BOOL onRight;
@property(nonatomic) BOOL companionTriggerLatched;
@property(nonatomic) NSTimeInterval requestedRevealDeadline;
@property(nonatomic) NSRect targetFrame;
@property(nonatomic) double revealProgress;
@property(nonatomic) double revealStartProgress;
@property(nonatomic) double revealTarget;
@property(nonatomic) NSTimeInterval revealStartedAt;
@property(nonatomic, strong) NSTimer *animationTimer;
- (void)layoutReveal;
- (void)animateIn;
- (void)animateTo:(double)target;
- (void)hide;
@end

@implementation GhostexGpuiSidebarReveal
- (void)layoutReveal {
  NSRect frame = self.targetFrame;
  CGFloat fullWidth = frame.size.width;
  frame.size.width = MAX(1, fullWidth * self.revealProgress);
  if (self.onRight) frame.origin.x = NSMaxX(self.targetFrame) - frame.size.width;
  if (!NSEqualRects(self.panel.frame, frame)) [self.panel setFrame:frame display:NO];
  if (self.companion) return;
  // Keep Chromium's viewport full-sized while the child window clips the
  // entering page. Resizing the page itself would reflow every session row.
  NSRect sidebarFrame = NSMakeRect(self.onRight ? 0 : frame.size.width - fullWidth,
                                  0, fullWidth, frame.size.height);
  if (!NSEqualRects(self.sidebar.frame, sidebarFrame)) self.sidebar.frame = sidebarFrame;
  GhostexGpuiCEFRefreshSidebarPointerInside();
}

- (void)animateIn {
  self.revealProgress = 0;
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
  if (NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion) {
    self.revealProgress = target;
    [self layoutReveal];
    if (target == 0) {
      [self.panel orderOut:nil];
      if (!self.companion) GhostexGpuiCEFRefreshSidebarPointerInside();
    }
    return;
  }
  self.revealStartedAt = NSProcessInfo.processInfo.systemUptime;
  __weak GhostexGpuiSidebarReveal *weakSelf = self;
  self.animationTimer = [NSTimer timerWithTimeInterval:1.0 / 120.0 repeats:YES block:^(NSTimer *timer) {
    GhostexGpuiSidebarReveal *state = weakSelf;
    if (!state || !state.attached) {
      [timer invalidate];
      return;
    }
    double elapsed = NSProcessInfo.processInfo.systemUptime - state.revealStartedAt;
    double t = MIN(1, MAX(0, elapsed / 0.22));
    double remaining = 1 - t;
    double eased = 1 - remaining * remaining * remaining;
    state.revealProgress = state.revealStartProgress + (state.revealTarget - state.revealStartProgress) * eased;
    [state layoutReveal];
    if (t == 1) {
      [timer invalidate];
      state.animationTimer = nil;
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
                                  bool enabled, double width, double titlebarHeight, bool onRight,
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
  if (onRight) edge.origin.x = NSMaxX(body) - edge.size.width;
  BOOL overEdge = NSPointInRect(pointer, edge);
  if (!overEdge) state.companionTriggerLatched = NO;
  if (!state && !overEdge && !requested && !keepUnderPointer) return false;
  if (!state) {
    state = [GhostexGpuiSidebarReveal new];
    state.sidebar = sidebar;
    state.root = root;
    state.panel = [[GhostexGpuiSidebarRevealPanel alloc]
        initWithContentRect:NSZeroRect
                  styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
                    backing:NSBackingStoreBuffered defer:NO];
    state.panel.releasedWhenClosed = NO;
    state.panel.hasShadow = YES;
    state.panel.hidesOnDeactivate = YES;
    state.panel.becomesKeyOnlyIfNeeded = YES;
    state.panel.backgroundColor = [NSColor colorWithWhite:0.08 alpha:1];
    state.panel.acceptsMouseMovedEvents = YES;
    state.panel.collectionBehavior = NSWindowCollectionBehaviorFullScreenAuxiliary;
    state.panel.contentView.autoresizesSubviews = NO;
    state.panel.contentView.wantsLayer = YES;
    state.panel.contentView.layer.masksToBounds = YES;
    objc_setAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey, state, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  if (state.attached && state.onRight != onRight) [state hide];
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
  if (onRight) panelFrame.origin.x = NSMaxX(body) - panelFrame.size.width;
  state.targetFrame = panelFrame;
  state.onRight = onRight;
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

bool GhostexGpuiSidebarRevealReturnFocus(void *sidebarPtr) {
  NSView *sidebar = (__bridge NSView *)sidebarPtr;
  GhostexGpuiSidebarReveal *state = objc_getAssociatedObject(sidebar, GhostexGpuiSidebarRevealKey);
  if (!state.attached || !state.root.window) return false;
  GhostexGpuiCEFClearActiveNativeView();
  [state.root.window makeKeyWindow];
  [state.root.window makeFirstResponder:state.root];
  return true;
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
bool GhostexGpuiCompanionRevealUpdate(void *rootPtr, void *popupPtr, bool enabled,
                                     bool onRight, double width, double titlebarHeight) {
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
  NSRect frame = body;
  frame.size.width = MIN(width, body.size.width);
  if (onRight) frame.origin.x = NSMaxX(body) - frame.size.width;
  if (!state) {
    state = [GhostexGpuiSidebarReveal new];
    state.companion = YES;
    state.root = root;
    state.panel = (GhostexGpuiSidebarRevealPanel *)popup.window;
    state.panel.level = parent.level;
    state.panel.styleMask = NSWindowStyleMaskNonactivatingPanel;
    state.panel.hasShadow = YES;
    state.panel.hidesOnDeactivate = YES;
    [parent addChildWindow:state.panel ordered:NSWindowAbove];
    state.attached = YES;
    state.targetFrame = frame;
    state.onRight = onRight;
    objc_setAssociatedObject(popup, GhostexGpuiSidebarRevealKey, state, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    [state animateIn];
  } else {
    state.targetFrame = frame;
    state.onRight = onRight;
    [state layoutReveal];
  }
  NSPoint pointer = NSEvent.mouseLocation;
  BOOL inside = NSPointInRect(pointer, state.revealTarget == 0 ? state.panel.frame : frame);
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
