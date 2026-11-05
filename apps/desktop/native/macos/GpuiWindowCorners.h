#pragma once

#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>

/*
 CDXC:Workarea 2026-09-06 WHY:
 CEF and Ghostty are native children above GPUI, so their rectangular content would cover the curved pane border even after GPUI rounds it.
 Clip their existing layers at the window's bottom corners; their frames and input ownership remain with normal layout.
 SEE-ALSO: MACOS_WINDOW_CORNER_RADIUS in src/app/element/window_corner_pane.rs, the measured
 radius of the window's system mask, which this must equal.
 */
/// The mask both clips below share, so either one replaces or clears what the other set.
static char GhostexGpuiNativeCornerMaskKey;

static inline void GhostexGpuiClipNativeViewWindowCorners(NSView *view) {
  NSView *root = view.window.contentView;
  CAShapeLayer *mask = objc_getAssociatedObject(view, &GhostexGpuiNativeCornerMaskKey);
  if (!root || !view.superview || view.window.parentWindow ||
      !view.window.titlebarAppearsTransparent ||
      (view.window.styleMask & NSWindowStyleMaskFullScreen)) {
    if (mask && view.layer.mask == mask) view.layer.mask = nil;
    return;
  }

  const CGFloat radius = 16.5;
  NSRect frame = [view convertRect:view.bounds toView:root];
  NSRect viewport = root.bounds;
  CGFloat bottom = root.isFlipped ? NSMaxY(viewport) - NSMaxY(frame)
                                 : NSMinY(frame) - NSMinY(viewport);
  CGFloat left = NSMinX(frame) - NSMinX(viewport);
  CGFloat right = NSMaxX(viewport) - NSMaxX(frame);
  BOOL roundLeft = bottom >= 0 && bottom < radius && left >= 0 && left < radius;
  BOOL roundRight = bottom >= 0 && bottom < radius && right >= 0 && right < radius;
  if (!roundLeft && !roundRight) {
    if (mask && view.layer.mask == mask) view.layer.mask = nil;
    return;
  }

  // A border-inset child clips against the inset window curve, exposing the
  // GPUI stroke beneath it. Taller bottom bars and interior split panes do
  // not intersect either corner and therefore receive no mask.
  CGFloat inset = bottom;
  if (roundLeft) inset = MIN(inset, left);
  if (roundRight) inset = MIN(inset, right);
  NSRect clip = [root convertRect:NSInsetRect(viewport, inset, inset) toView:view];
  CGFloat r = radius - inset;
  CGMutablePathRef path = CGPathCreateMutable();
  CGPathAddRoundedRect(path, NULL, NSRectToCGRect(clip), r, r);
  if (mask && view.layer.mask == mask && NSEqualRects(mask.frame, view.bounds) &&
      mask.path && CGPathEqualToPath(mask.path, path)) {
    CGPathRelease(path);
    return;
  }
  if (!mask) {
    mask = [CAShapeLayer layer];
    objc_setAssociatedObject(view, &GhostexGpuiNativeCornerMaskKey, mask,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  view.wantsLayer = YES;
  [CATransaction begin];
  [CATransaction setDisableActions:YES];
  mask.frame = view.bounds;
  mask.path = path;
  view.layer.mask = mask;
  [CATransaction commit];
  CGPathRelease(path);
}

/*
 CDXC:Workarea 2026-11-05 WHY:
 With the work area frame on, a web page that reaches one of the card's corners would cover the card's rounded corner with its square one, because CEF's view sits above GPUI. Clip it to the card instead of the window: the card's inside (x, y, width, height in GPUI's top-left window points, its border left out) with `radius` on all four corners, in full screen too. The rectangle comes with every refresh, so each window clips to its own card and nothing is kept per process.
 SEE-ALSO: src/app/element/work_area_frame.rs (`refresh_native_work_area_frame_clip`) and src/app/helpers/work_area_frame.rs (`WORK_AREA_FRAME_INNER_RADIUS`).
 */
static inline void GhostexGpuiClipNativeViewToWorkAreaCard(NSView *view, CGFloat x, CGFloat y,
                                                           CGFloat width, CGFloat height,
                                                           CGFloat radius) {
  NSView *parent = view.superview;
  CAShapeLayer *mask = objc_getAssociatedObject(view, &GhostexGpuiNativeCornerMaskKey);
  if (!parent || !view.window || view.window.parentWindow) {
    if (mask && view.layer.mask == mask) view.layer.mask = nil;
    return;
  }
  // The same conversion GhostexGpuiCEFSetNativeViewFrame uses for the view's own frame.
  CGFloat parentY = parent.isFlipped ? y : NSHeight(parent.bounds) - y - height;
  NSRect card = NSMakeRect(x, parentY, MAX(0.0, width), MAX(0.0, height));
  NSRect frame = view.frame;
  CGFloat left = NSMinX(frame) - NSMinX(card);
  CGFloat right = NSMaxX(card) - NSMaxX(frame);
  CGFloat lower = NSMinY(frame) - NSMinY(card);
  CGFloat upper = NSMaxY(card) - NSMaxY(frame);
  BOOL nearSide = left < radius || right < radius;
  BOOL nearEnd = lower < radius || upper < radius;
  // Pages away from every corner of the card need no mask.
  if (!nearSide || !nearEnd) {
    if (mask && view.layer.mask == mask) view.layer.mask = nil;
    return;
  }
  NSRect clip = [view convertRect:card fromView:parent];
  CGMutablePathRef path = CGPathCreateMutable();
  CGPathAddRoundedRect(path, NULL, NSRectToCGRect(clip), radius, radius);
  if (mask && view.layer.mask == mask && NSEqualRects(mask.frame, view.bounds) && mask.path &&
      CGPathEqualToPath(mask.path, path)) {
    CGPathRelease(path);
    return;
  }
  if (!mask) {
    mask = [CAShapeLayer layer];
    objc_setAssociatedObject(view, &GhostexGpuiNativeCornerMaskKey, mask,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  view.wantsLayer = YES;
  [CATransaction begin];
  [CATransaction setDisableActions:YES];
  mask.frame = view.bounds;
  mask.path = path;
  view.layer.mask = mask;
  [CATransaction commit];
  CGPathRelease(path);
}
