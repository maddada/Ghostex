#pragma once

#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>

/// The mask the work area card's clip below keeps on a view.
static char GhostexGpuiNativeCornerMaskKey;

/*
 CDXC:Workarea 2026-11-05 WHY:
 A web page that reaches one of the work area card's corners would cover the card's rounded corner with its square one, because CEF's view sits above GPUI. Clip it to the card: the card's inside (x, y, width, height in GPUI's top-left window points, its border left out) with `radius` on all four corners, in full screen too. The rectangle comes with every refresh, so each window clips to its own card and nothing is kept per process.
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
