#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>
#import <stdint.h>

typedef NS_ENUM(int32_t, GhostexGpuiMenuBarStatusKind) {
  GhostexGpuiMenuBarStatusKindAttention = 0,
  GhostexGpuiMenuBarStatusKindWorking = 1,
  GhostexGpuiMenuBarStatusKindAvailable = 2,
};

typedef struct {
  GhostexGpuiMenuBarStatusKind kind;
  uint64_t count;
} GhostexGpuiMenuBarStatusEntry;

typedef struct {
  const char *session_id;
  const char *title;
  const char *last_active_at;
  GhostexGpuiMenuBarStatusKind status;
  uint64_t order;
} GhostexGpuiMenuBarStatusSessionEntry;

typedef struct {
  const char *project_id;
  const char *title;
  const GhostexGpuiMenuBarStatusSessionEntry *sessions;
  uintptr_t session_count;
} GhostexGpuiMenuBarStatusProjectEntry;

extern void GhostexGpuiMenuBarStatusProjectClicked(const char *project_id);
extern void GhostexGpuiMenuBarStatusSessionClicked(const char *project_id,
                                                   const char *session_id);

static NSStatusItem *GhostexGpuiMenuBarStatusItem = nil;

@interface GhostexGpuiMenuBarStatusSessionModel : NSObject
@property(nonatomic, copy) NSString *sessionId;
@property(nonatomic, copy) NSString *title;
@property(nonatomic, copy) NSString *lastActiveAt;
@property(nonatomic, assign) GhostexGpuiMenuBarStatusKind status;
@property(nonatomic, assign) uint64_t order;
@end

@implementation GhostexGpuiMenuBarStatusSessionModel
@end

@interface GhostexGpuiMenuBarStatusProjectModel : NSObject
@property(nonatomic, copy) NSString *projectId;
@property(nonatomic, copy) NSString *title;
@property(nonatomic, copy)
    NSArray<GhostexGpuiMenuBarStatusSessionModel *> *sessions;
@end

@implementation GhostexGpuiMenuBarStatusProjectModel
@end

static NSFont *GhostexGpuiMenuBarStatusFont(void) {
  return [NSFont monospacedDigitSystemFontOfSize:15.0
                                          weight:NSFontWeightSemibold];
}

static NSColor *GhostexGpuiMenuBarStatusAvailableTextColor(void) {
  return [NSColor
        colorWithName:@"GhostexGpuiMenuBarAvailableText"
      dynamicProvider:^NSColor *(NSAppearance *appearance) {
        NSAppearanceName bestMatch =
            [appearance bestMatchFromAppearancesWithNames:@[
              NSAppearanceNameDarkAqua,
              NSAppearanceNameAccessibilityHighContrastDarkAqua,
              NSAppearanceNameAqua,
              NSAppearanceNameAccessibilityHighContrastAqua,
            ]];
        if ([bestMatch isEqualToString:NSAppearanceNameDarkAqua] ||
            [bestMatch isEqualToString:
                           NSAppearanceNameAccessibilityHighContrastDarkAqua]) {
          return [NSColor colorWithCalibratedRed:0xE5 / 255.0
                                           green:0xE6 / 255.0
                                            blue:0xE6 / 255.0
                                           alpha:1.0];
        }
        return [NSColor colorWithCalibratedWhite:0.08 alpha:1.0];
      }];
}

static NSColor *
GhostexGpuiMenuBarStatusTextColor(GhostexGpuiMenuBarStatusKind kind) {
  switch (kind) {
  case GhostexGpuiMenuBarStatusKindAttention:
    return [NSColor colorWithCalibratedRed:0x00 / 255.0
                                     green:0x93 / 255.0
                                      blue:0xFE / 255.0
                                     alpha:1.0];
  case GhostexGpuiMenuBarStatusKindWorking:
    return [NSColor colorWithCalibratedRed:0xC9 / 255.0
                                     green:0x96 / 255.0
                                      blue:0x43 / 255.0
                                     alpha:1.0];
  case GhostexGpuiMenuBarStatusKindAvailable:
    return GhostexGpuiMenuBarStatusAvailableTextColor();
  }
}

static NSString *GhostexGpuiMenuBarStatusCountLabel(uint64_t count) {
  return [NSString stringWithFormat:@"%llu", (unsigned long long)count];
}

static NSDictionary<NSAttributedStringKey, id> *
GhostexGpuiMenuBarStatusTextAttributes(GhostexGpuiMenuBarStatusKind kind) {
  return @{
    NSFontAttributeName : GhostexGpuiMenuBarStatusFont(),
    NSForegroundColorAttributeName : GhostexGpuiMenuBarStatusTextColor(kind),
  };
}

static NSSize
GhostexGpuiMenuBarStatusTextSize(GhostexGpuiMenuBarStatusEntry entry) {
  NSAttributedString *label = [[NSAttributedString alloc]
      initWithString:GhostexGpuiMenuBarStatusCountLabel(entry.count)
          attributes:@{NSFontAttributeName : GhostexGpuiMenuBarStatusFont()}];
  return label.size;
}

static NSSize
GhostexGpuiMenuBarStatusBadgeSize(GhostexGpuiMenuBarStatusEntry entry) {
  NSSize labelSize = GhostexGpuiMenuBarStatusTextSize(entry);
  return NSMakeSize(ceil(MAX(26.0, labelSize.width + 10.0)),
                    ceil(MAX(20.0, labelSize.height + 2.0)));
}

static NSSize GhostexGpuiMenuBarStatusPreferredSize(
    const GhostexGpuiMenuBarStatusEntry *entries, NSUInteger count) {
  CGFloat width = 2.0;
  CGFloat height = 22.0;
  for (NSUInteger index = 0; index < count; index += 1) {
    NSSize badgeSize = GhostexGpuiMenuBarStatusBadgeSize(entries[index]);
    width += badgeSize.width;
    height = MAX(height, badgeSize.height + 2.0);
    if (index + 1 < count) {
      width += 2.0;
    }
  }
  return NSMakeSize(ceil(width), ceil(height));
}

static void GhostexGpuiMenuBarStatusDrawEntries(
    const GhostexGpuiMenuBarStatusEntry *entries, NSUInteger count,
    NSRect bounds) {
  CGFloat groupWidth = 0.0;
  for (NSUInteger index = 0; index < count; index += 1) {
    groupWidth += GhostexGpuiMenuBarStatusBadgeSize(entries[index]).width;
    if (index + 1 < count) {
      groupWidth += 2.0;
    }
  }

  CGFloat x = floor((bounds.size.width - groupWidth) / 2.0);
  for (NSUInteger index = 0; index < count; index += 1) {
    GhostexGpuiMenuBarStatusEntry entry = entries[index];
    NSSize badgeSize = GhostexGpuiMenuBarStatusBadgeSize(entry);
    NSRect badgeRect =
        NSMakeRect(x, floor(NSMidY(bounds) - badgeSize.height / 2.0),
                   badgeSize.width, badgeSize.height);

    [NSColor.controlBackgroundColor setFill];
    [[NSBezierPath bezierPathWithRoundedRect:badgeRect xRadius:6.0
                                     yRadius:6.0] fill];

    NSAttributedString *label = [[NSAttributedString alloc]
        initWithString:GhostexGpuiMenuBarStatusCountLabel(entry.count)
            attributes:GhostexGpuiMenuBarStatusTextAttributes(entry.kind)];
    NSSize labelSize = label.size;
    [label drawAtPoint:NSMakePoint(
                           floor(NSMidX(badgeRect) - labelSize.width / 2.0),
                           floor(NSMidY(badgeRect) - labelSize.height / 2.0))];
    x += badgeSize.width + 2.0;
  }
}

static NSImage *
GhostexGpuiMenuBarStatusImage(const GhostexGpuiMenuBarStatusEntry *entries,
                              NSUInteger count) {
  NSSize size = GhostexGpuiMenuBarStatusPreferredSize(entries, count);
  NSImage *image = [[NSImage alloc] initWithSize:size];
  [image lockFocus];
  GhostexGpuiMenuBarStatusDrawEntries(
      entries, count, NSMakeRect(0.0, 0.0, size.width, size.height));
  [image unlockFocus];
  image.template = NO;
  return image;
}

static NSString *GhostexGpuiMenuBarStatusStringFromCString(const char *value) {
  if (value == NULL) {
    return @"";
  }
  NSString *text = [NSString stringWithUTF8String:value];
  return text ?: @"";
}

static void GhostexGpuiMenuBarStatusDrawPaddedTitle(NSString *title,
                                                    NSFont *font,
                                                    NSColor *textColor,
                                                    CGFloat horizontalPadding,
                                                    NSRect bounds) {
  NSMutableParagraphStyle *paragraphStyle =
      [[NSMutableParagraphStyle alloc] init];
  paragraphStyle.alignment = NSTextAlignmentLeft;
  paragraphStyle.lineBreakMode = NSLineBreakByTruncatingTail;
  NSDictionary<NSAttributedStringKey, id> *attributes = @{
    NSFontAttributeName : font,
    NSForegroundColorAttributeName : textColor,
    NSParagraphStyleAttributeName : paragraphStyle,
  };
  CGFloat textHeight = ceil([title sizeWithAttributes:attributes].height);
  NSRect textRect = NSMakeRect(
      NSMinX(bounds) + horizontalPadding,
      floor(NSMidY(bounds) - textHeight / 2.0),
      MAX(0.0, NSWidth(bounds) - horizontalPadding * 2.0), textHeight);
  [title drawInRect:textRect withAttributes:attributes];
}

static NSString *GhostexGpuiMenuBarStatusRelativeTime(NSString *timestamp) {
  if (timestamp.length == 0) {
    return @"";
  }

  static NSISO8601DateFormatter *fractionalFormatter = nil;
  static NSISO8601DateFormatter *secondFormatter = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    fractionalFormatter = [[NSISO8601DateFormatter alloc] init];
    fractionalFormatter.formatOptions =
        NSISO8601DateFormatWithInternetDateTime |
        NSISO8601DateFormatWithFractionalSeconds;
    secondFormatter = [[NSISO8601DateFormatter alloc] init];
    secondFormatter.formatOptions = NSISO8601DateFormatWithInternetDateTime;
  });

  NSDate *date = [fractionalFormatter dateFromString:timestamp]
                     ?: [secondFormatter dateFromString:timestamp];
  if (date == nil) {
    return @"";
  }

  NSTimeInterval seconds = MAX(0.0, -[date timeIntervalSinceNow]);
  if (seconds < 60.0) {
    return @"now";
  }
  if (seconds < 3600.0) {
    return [NSString stringWithFormat:@"%.0fm", floor(seconds / 60.0)];
  }
  if (seconds < 86400.0) {
    return [NSString stringWithFormat:@"%.0fh", floor(seconds / 3600.0)];
  }
  return [NSString stringWithFormat:@"%.0fd", floor(seconds / 86400.0)];
}

static NSArray<GhostexGpuiMenuBarStatusProjectModel *> *
GhostexGpuiCopyMenuBarStatusProjects(
    const GhostexGpuiMenuBarStatusProjectEntry *projects,
    uintptr_t projectCount) {
  if (projects == NULL || projectCount == 0) {
    return @[];
  }

  NSMutableArray<GhostexGpuiMenuBarStatusProjectModel *> *copiedProjects =
      [NSMutableArray array];
  uintptr_t safeProjectCount = MIN(projectCount, (uintptr_t)32);
  for (uintptr_t projectIndex = 0; projectIndex < safeProjectCount;
       projectIndex += 1) {
    GhostexGpuiMenuBarStatusProjectEntry projectEntry = projects[projectIndex];
    GhostexGpuiMenuBarStatusProjectModel *project =
        [[GhostexGpuiMenuBarStatusProjectModel alloc] init];
    project.projectId =
        GhostexGpuiMenuBarStatusStringFromCString(projectEntry.project_id);
    project.title =
        GhostexGpuiMenuBarStatusStringFromCString(projectEntry.title);

    NSMutableArray<GhostexGpuiMenuBarStatusSessionModel *> *sessions =
        [NSMutableArray array];
    if (projectEntry.sessions != NULL) {
      uintptr_t safeSessionCount =
          MIN(projectEntry.session_count, (uintptr_t)16);
      for (uintptr_t sessionIndex = 0; sessionIndex < safeSessionCount;
           sessionIndex += 1) {
        GhostexGpuiMenuBarStatusSessionEntry sessionEntry =
            projectEntry.sessions[sessionIndex];
        GhostexGpuiMenuBarStatusSessionModel *session =
            [[GhostexGpuiMenuBarStatusSessionModel alloc] init];
        session.sessionId =
            GhostexGpuiMenuBarStatusStringFromCString(sessionEntry.session_id);
        session.title =
            GhostexGpuiMenuBarStatusStringFromCString(sessionEntry.title);
        session.lastActiveAt = GhostexGpuiMenuBarStatusStringFromCString(
            sessionEntry.last_active_at);
        session.status = sessionEntry.status;
        session.order = sessionEntry.order;
        if (session.sessionId.length > 0 && session.title.length > 0) {
          [sessions addObject:session];
        }
      }
    }

    project.sessions = sessions;
    if (project.projectId.length > 0 && project.title.length > 0) {
      [copiedProjects addObject:project];
    }
  }
  return copiedProjects;
}

static void GhostexGpuiMenuBarStatusActivateApplication(void) {
  [NSApp unhide:nil];
  for (NSWindow *window in NSApp.windows) {
    if (window.isMiniaturized) {
      [window deminiaturize:nil];
    }
    if (window.canBecomeMainWindow) {
      [window makeKeyAndOrderFront:nil];
      break;
    }
  }
  [NSApp activateIgnoringOtherApps:YES];
}

@interface GhostexGpuiMenuBarStatusPanel : NSPanel
@end

@implementation GhostexGpuiMenuBarStatusPanel
- (BOOL)canBecomeKeyWindow {
  return YES;
}
@end

@interface GhostexGpuiMenuBarStatusFlippedView : NSView
@end

@implementation GhostexGpuiMenuBarStatusFlippedView
- (BOOL)isFlipped {
  return YES;
}
@end

@interface GhostexGpuiMenuBarStatusFocusSink : NSView
@end

@implementation GhostexGpuiMenuBarStatusFocusSink
- (BOOL)acceptsFirstResponder {
  return YES;
}
@end

@interface GhostexGpuiMenuBarStatusThinScrollbar : NSView
@property(nonatomic, assign) CGFloat knobHeightFraction;
@property(nonatomic, assign) CGFloat knobOffsetFraction;
@end

@implementation GhostexGpuiMenuBarStatusThinScrollbar
- (instancetype)initWithFrame:(NSRect)frameRect {
  self = [super initWithFrame:frameRect];
  if (self) {
    _knobHeightFraction = 1.0;
    _knobOffsetFraction = 0.0;
  }
  return self;
}

- (BOOL)isOpaque {
  return NO;
}

- (void)setKnobHeightFraction:(CGFloat)knobHeightFraction {
  CGFloat clampedFraction = MIN(1.0, MAX(0.0, knobHeightFraction));
  if (_knobHeightFraction == clampedFraction) {
    return;
  }
  _knobHeightFraction = clampedFraction;
  self.needsDisplay = YES;
}

- (void)setKnobOffsetFraction:(CGFloat)knobOffsetFraction {
  CGFloat clampedFraction = MIN(1.0, MAX(0.0, knobOffsetFraction));
  if (_knobOffsetFraction == clampedFraction) {
    return;
  }
  _knobOffsetFraction = clampedFraction;
  self.needsDisplay = YES;
}

- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];
  CGFloat trackHeight = NSHeight(self.bounds);
  if (trackHeight <= 0.0) {
    return;
  }
  CGFloat minKnobHeight = 24.0;
  CGFloat knobHeight = MAX(minKnobHeight, trackHeight * _knobHeightFraction);
  CGFloat maxOffset = MAX(0.0, trackHeight - knobHeight);
  CGFloat y =
      NSMaxY(self.bounds) - knobHeight - maxOffset * _knobOffsetFraction;
  [[NSColor.tertiaryLabelColor colorWithAlphaComponent:0.8] setFill];
  [[NSBezierPath
      bezierPathWithRoundedRect:NSMakeRect(0.0, y, NSWidth(self.bounds),
                                           knobHeight)
                        xRadius:NSWidth(self.bounds) / 2.0
                        yRadius:NSWidth(self.bounds) / 2.0] fill];
}
@end

@interface GhostexGpuiMenuBarStatusContentView : NSView
@property(nonatomic, copy) void (^hoverChanged)(BOOL hovered);
@end

@implementation GhostexGpuiMenuBarStatusContentView
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  for (NSTrackingArea *trackingArea in self.trackingAreas.copy) {
    [self removeTrackingArea:trackingArea];
  }
  [self addTrackingArea:[[NSTrackingArea alloc]
                            initWithRect:self.bounds
                                 options:NSTrackingMouseEnteredAndExited |
                                         NSTrackingActiveAlways |
                                         NSTrackingInVisibleRect
                                   owner:self
                                userInfo:nil]];
}

- (void)mouseEntered:(NSEvent *)event {
  if (self.hoverChanged) {
    self.hoverChanged(YES);
  }
}

- (void)mouseExited:(NSEvent *)event {
  if (self.hoverChanged) {
    self.hoverChanged(NO);
  }
}
@end

@interface GhostexGpuiMenuBarStatusProjectCardView : NSView
@end

@implementation GhostexGpuiMenuBarStatusProjectCardView
- (instancetype)initWithFrame:(NSRect)frameRect {
  self = [super initWithFrame:frameRect];
  if (self) {
    self.wantsLayer = YES;
    self.layer.backgroundColor =
        [NSColor colorWithCalibratedWhite:0x16 / 255.0 alpha:1.0].CGColor;
    self.layer.borderColor =
        [NSColor colorWithCalibratedWhite:0x3a / 255.0 alpha:0.72].CGColor;
    self.layer.borderWidth = 1.0;
    self.layer.cornerRadius = 8.0;
    self.layer.masksToBounds = YES;
  }
  return self;
}
@end

@interface GhostexGpuiMenuBarStatusProjectButton : NSControl
@property(nonatomic, copy, readonly) NSString *projectId;
@property(nonatomic, copy) NSString *title;
@property(nonatomic, strong) NSFont *displayFont;
@property(nonatomic, strong) NSColor *textColor;
- (instancetype)initWithProjectId:(NSString *)projectId;
@end

@implementation GhostexGpuiMenuBarStatusProjectButton
- (instancetype)initWithProjectId:(NSString *)projectId {
  self = [super initWithFrame:NSZeroRect];
  if (self) {
    _projectId = [projectId copy];
    _title = @"";
    _displayFont = [NSFont systemFontOfSize:16.0 weight:NSFontWeightLight];
    _textColor = [NSColor colorWithCalibratedWhite:0xa5 / 255.0 alpha:1.0];
    self.focusRingType = NSFocusRingTypeNone;
  }
  return self;
}

- (BOOL)acceptsFirstResponder {
  return NO;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
  return YES;
}

- (void)setTitle:(NSString *)title {
  _title = [title copy] ?: @"";
  self.needsDisplay = YES;
}

- (void)setDisplayFont:(NSFont *)displayFont {
  _displayFont = displayFont;
  self.needsDisplay = YES;
}

- (void)setTextColor:(NSColor *)textColor {
  _textColor = textColor;
  self.needsDisplay = YES;
}

- (void)mouseUp:(NSEvent *)event {
  if (event.type != NSEventTypeLeftMouseUp ||
      (event.modifierFlags & NSEventModifierFlagControl) != 0 ||
      !NSPointInRect([self convertPoint:event.locationInWindow fromView:nil],
                     self.bounds) ||
      self.action == NULL) {
    return;
  }
  [NSApp sendAction:self.action to:self.target from:self];
}

- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];
  GhostexGpuiMenuBarStatusDrawPaddedTitle(self.title, self.displayFont,
                                          self.textColor, 10.0, self.bounds);
}
@end

@interface GhostexGpuiMenuBarStatusActionButton : NSControl
@property(nonatomic, copy) NSString *title;
@property(nonatomic, strong) NSFont *displayFont;
@property(nonatomic, strong) NSColor *textColor;
- (void)setHovered:(BOOL)hovered;
@end

@implementation GhostexGpuiMenuBarStatusActionButton {
  BOOL _hovered;
}

- (instancetype)initWithFrame:(NSRect)frameRect {
  self = [super initWithFrame:frameRect];
  if (self) {
    _title = @"";
    _displayFont = [NSFont systemFontOfSize:15.55 weight:NSFontWeightLight];
    _textColor = [NSColor colorWithCalibratedRed:0xb4 / 255.0
                                           green:0xb8 / 255.0
                                            blue:0xc0 / 255.0
                                           alpha:1.0];
    self.focusRingType = NSFocusRingTypeNone;
  }
  return self;
}

- (BOOL)acceptsFirstResponder {
  return NO;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
  return YES;
}

- (void)setTitle:(NSString *)title {
  _title = [title copy] ?: @"";
  self.needsDisplay = YES;
}

- (void)setDisplayFont:(NSFont *)displayFont {
  _displayFont = displayFont;
  self.needsDisplay = YES;
}

- (void)setTextColor:(NSColor *)textColor {
  _textColor = textColor;
  self.needsDisplay = YES;
}

- (void)setHovered:(BOOL)hovered {
  if (_hovered == hovered) {
    return;
  }
  _hovered = hovered;
  self.needsDisplay = YES;
}

- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  for (NSTrackingArea *trackingArea in self.trackingAreas.copy) {
    [self removeTrackingArea:trackingArea];
  }
  [self addTrackingArea:[[NSTrackingArea alloc]
                            initWithRect:self.bounds
                                 options:NSTrackingMouseEnteredAndExited |
                                         NSTrackingActiveAlways |
                                         NSTrackingInVisibleRect
                                   owner:self
                                userInfo:nil]];
}

- (void)mouseEntered:(NSEvent *)event {
  [self setHovered:YES];
}

- (void)mouseExited:(NSEvent *)event {
  [self setHovered:NO];
}

- (void)mouseUp:(NSEvent *)event {
  if (event.type != NSEventTypeLeftMouseUp ||
      (event.modifierFlags & NSEventModifierFlagControl) != 0 ||
      !NSPointInRect([self convertPoint:event.locationInWindow fromView:nil],
                     self.bounds) ||
      self.action == NULL) {
    return;
  }
  [NSApp sendAction:self.action to:self.target from:self];
}

- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];
  if (_hovered) {
    [[NSColor colorWithCalibratedWhite:0x18 / 255.0 alpha:1.0] setFill];
    [[NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.0, 2.0)
                                     xRadius:6.0
                                     yRadius:6.0] fill];
  }
  GhostexGpuiMenuBarStatusDrawPaddedTitle(self.title, self.displayFont,
                                          self.textColor, 10.0, self.bounds);
}
@end

@interface GhostexGpuiMenuBarStatusSessionRow : NSControl
@property(nonatomic, copy, readonly) NSString *projectId;
@property(nonatomic, copy, readonly) NSString *sessionId;
@property(nonatomic, copy) void (^hoverChanged)
    (GhostexGpuiMenuBarStatusSessionRow *row);
- (instancetype)initWithProjectId:(NSString *)projectId
                          session:
                              (GhostexGpuiMenuBarStatusSessionModel *)session;
- (void)setHovered:(BOOL)hovered;
@end

@implementation GhostexGpuiMenuBarStatusSessionRow {
  NSString *_title;
  NSString *_trailingText;
  GhostexGpuiMenuBarStatusKind _status;
  BOOL _hovered;
}

- (instancetype)initWithProjectId:(NSString *)projectId
                          session:
                              (GhostexGpuiMenuBarStatusSessionModel *)session {
  self = [super initWithFrame:NSZeroRect];
  if (self) {
    _projectId = [projectId copy];
    _sessionId = [session.sessionId copy];
    _title = [session.title copy];
    _status = session.status;
    _trailingText =
        session.status == GhostexGpuiMenuBarStatusKindWorking
            ? @""
            : GhostexGpuiMenuBarStatusRelativeTime(session.lastActiveAt);
    self.focusRingType = NSFocusRingTypeNone;
  }
  return self;
}

- (BOOL)acceptsFirstResponder {
  return NO;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
  return YES;
}

- (void)setHovered:(BOOL)hovered {
  if (_hovered == hovered) {
    return;
  }
  _hovered = hovered;
  self.needsDisplay = YES;
}

- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  for (NSTrackingArea *trackingArea in self.trackingAreas.copy) {
    [self removeTrackingArea:trackingArea];
  }
  [self addTrackingArea:[[NSTrackingArea alloc]
                            initWithRect:self.bounds
                                 options:NSTrackingMouseEnteredAndExited |
                                         NSTrackingActiveAlways |
                                         NSTrackingInVisibleRect
                                   owner:self
                                userInfo:nil]];
}

- (void)mouseEntered:(NSEvent *)event {
  if (self.hoverChanged) {
    self.hoverChanged(self);
  }
}

- (void)mouseExited:(NSEvent *)event {
  if (_hovered && self.hoverChanged) {
    self.hoverChanged(nil);
  }
}

- (void)mouseUp:(NSEvent *)event {
  if (event.type != NSEventTypeLeftMouseUp ||
      (event.modifierFlags & NSEventModifierFlagControl) != 0 ||
      !NSPointInRect([self convertPoint:event.locationInWindow fromView:nil],
                     self.bounds) ||
      self.action == NULL) {
    return;
  }
  [NSApp sendAction:self.action to:self.target from:self];
}

- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];
  if (_hovered) {
    [[NSColor colorWithCalibratedWhite:0x20 / 255.0 alpha:1.0] setFill];
    [[NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.0, 2.0)
                                     xRadius:6.0
                                     yRadius:6.0] fill];
  }

  CGFloat iconSize = _status == GhostexGpuiMenuBarStatusKindWorking ? 8.0 : 9.0;
  CGFloat iconX = NSMinX(self.bounds) + 13.0 + floor((18.0 - iconSize) / 2.0);
  NSRect iconRect = NSMakeRect(
      iconX, floor(NSMidY(self.bounds) - iconSize / 2.0), iconSize, iconSize);
  [GhostexGpuiMenuBarStatusTextColor(_status) setFill];
  if (_status == GhostexGpuiMenuBarStatusKindWorking) {
    [[NSBezierPath bezierPathWithRect:iconRect] fill];
  } else {
    [[NSBezierPath bezierPathWithOvalInRect:iconRect] fill];
  }

  NSMutableParagraphStyle *titleStyle = [[NSMutableParagraphStyle alloc] init];
  titleStyle.lineBreakMode = NSLineBreakByTruncatingTail;
  NSDictionary<NSAttributedStringKey, id> *titleAttributes = @{
    NSFontAttributeName : [NSFont systemFontOfSize:15.55
                                            weight:NSFontWeightLight],
    NSForegroundColorAttributeName :
        [NSColor colorWithCalibratedRed:0xb4 / 255.0
                                  green:0xb8 / 255.0
                                   blue:0xc0 / 255.0
                                  alpha:1.0],
    NSParagraphStyleAttributeName : titleStyle,
  };
  NSDictionary<NSAttributedStringKey, id> *timeAttributes = @{
    NSFontAttributeName :
        [NSFont monospacedDigitSystemFontOfSize:13.55 weight:NSFontWeightLight],
    NSForegroundColorAttributeName :
        [NSColor colorWithCalibratedWhite:0x4f / 255.0 alpha:1.0],
  };
  CGFloat titleX = NSMinX(self.bounds) + 41.0;
  CGFloat trailingWidth = 82.0;
  NSRect titleRect = NSMakeRect(
      titleX, floor(NSMidY(self.bounds) - 10.0),
      MAX(0.0, NSWidth(self.bounds) - titleX - trailingWidth - 14.0), 20.0);
  [_title drawInRect:titleRect withAttributes:titleAttributes];

  if (_status == GhostexGpuiMenuBarStatusKindWorking) {
    NSRect squareRect = NSMakeRect(NSMaxX(self.bounds) - 21.0,
                                   floor(NSMidY(self.bounds) - 4.0), 8.0, 8.0);
    [[NSColor colorWithCalibratedRed:0xC9 / 255.0
                               green:0x96 / 255.0
                                blue:0x43 / 255.0
                               alpha:1.0] setFill];
    [[NSBezierPath bezierPathWithRect:squareRect] fill];
  } else if (_trailingText.length > 0) {
    NSSize timeSize = [_trailingText sizeWithAttributes:timeAttributes];
    [_trailingText
           drawAtPoint:NSMakePoint(
                           NSMaxX(self.bounds) - 13.0 - timeSize.width,
                           floor(NSMidY(self.bounds) - timeSize.height / 2.0))
        withAttributes:timeAttributes];
  }
}
@end

@interface GhostexGpuiMenuBarStatusPanelController : NSObject <NSWindowDelegate>
@property(nonatomic, copy)
    NSArray<GhostexGpuiMenuBarStatusProjectModel *> *projects;
- (void)showFromStatusButton:(NSStatusBarButton *)button;
- (void)dismissPanel;
@end

@implementation GhostexGpuiMenuBarStatusPanelController {
  GhostexGpuiMenuBarStatusPanel *_panel;
  GhostexGpuiMenuBarStatusFocusSink *_focusSink;
  NSView *_rowsContainerView;
  GhostexGpuiMenuBarStatusFlippedView *_rowsContentView;
  NSScrollView *_scrollView;
  GhostexGpuiMenuBarStatusThinScrollbar *_scrollbarView;
  NSStackView *_rowsStack;
  NSLayoutConstraint *_rowsHeightConstraint;
  NSMutableArray<GhostexGpuiMenuBarStatusActionButton *> *_footerActionButtons;
  GhostexGpuiMenuBarStatusSessionRow *_hoveredSessionRow;
  BOOL _isMouseInsidePanel;
  id _localDismissEventMonitor;
  id _globalDismissEventMonitor;
}

static const CGFloat GhostexGpuiMenuBarStatusPanelWidth = 370.0;
static const CGFloat GhostexGpuiMenuBarStatusMaxPanelHeight = 520.0;
static const CGFloat GhostexGpuiMenuBarStatusMinPanelHeight = 180.0;
static const CGFloat GhostexGpuiMenuBarStatusRowHeight = 34.0;
static const CGFloat GhostexGpuiMenuBarStatusProjectHeaderHeight = 28.0;
static const CGFloat GhostexGpuiMenuBarStatusFooterHeight = 66.0;
static const CGFloat GhostexGpuiMenuBarStatusFooterRowHeight = 30.0;
static const CGFloat GhostexGpuiMenuBarStatusEmptyHeight = 44.0;
static const CGFloat GhostexGpuiMenuBarStatusContentHorizontalPadding = 16.0;
static const CGFloat GhostexGpuiMenuBarStatusContentVerticalPadding = 8.0;
static const CGFloat GhostexGpuiMenuBarStatusScrollbarWidth = 2.0;
static const CGFloat GhostexGpuiMenuBarStatusProjectSectionSpacing = 10.0;
static const CGFloat GhostexGpuiMenuBarStatusProjectTitleCardGap = 4.0;
static const CGFloat GhostexGpuiMenuBarStatusProjectCardHorizontalPadding = 6.0;
static const CGFloat GhostexGpuiMenuBarStatusProjectCardVerticalPadding = 6.0;

- (instancetype)init {
  self = [super init];
  if (self) {
    _projects = @[];
    _footerActionButtons = [NSMutableArray array];
    _panel = [[GhostexGpuiMenuBarStatusPanel alloc]
        initWithContentRect:NSMakeRect(0.0, 0.0,
                                       GhostexGpuiMenuBarStatusPanelWidth,
                                       GhostexGpuiMenuBarStatusMinPanelHeight)
                  styleMask:NSWindowStyleMaskBorderless |
                            NSWindowStyleMaskNonactivatingPanel
                    backing:NSBackingStoreBuffered
                      defer:NO];
    /*
     CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
     The GPUI menu-bar primary click opens a native Running Agents dropdown, not
     a CEF or GPUI overlay. Keep it as a borderless non-activating AppKit panel
     with normal visible row controls, click-away dismissal, project/session
     callbacks, and Restart/Quit footer rows so opening it does not raise the
     main app.

     CDXC:GPUIMenuBarStatusItem 2026-06-26-06:29:
     Pixel parity with the native menu-bar dropdown requires a hover-only 2px
     scrollbar pinned to the panel's right edge. Keep it as visible AppKit
     chrome with the system scroller disabled; it must not introduce hidden hit
     regions or input rerouting.
     */
    _panel.delegate = self;
    _panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                                NSWindowCollectionBehaviorFullScreenAuxiliary |
                                NSWindowCollectionBehaviorTransient;
    _panel.hidesOnDeactivate = YES;
    _panel.floatingPanel = YES;
    _panel.opaque = NO;
    _panel.releasedWhenClosed = NO;
    _panel.level = NSFloatingWindowLevel;
    _panel.backgroundColor = NSColor.clearColor;
    _panel.hasShadow = YES;
    [self configureContent];
    [self rebuildRows];
  }
  return self;
}

- (void)dealloc {
  [[NSNotificationCenter defaultCenter] removeObserver:self];
  [self removeDismissEventMonitors];
}

- (void)setProjects:
    (NSArray<GhostexGpuiMenuBarStatusProjectModel *> *)projects {
  _projects = [projects copy] ?: @[];
  [self rebuildRows];
}

- (CGFloat)contentWidth {
  return GhostexGpuiMenuBarStatusPanelWidth -
         GhostexGpuiMenuBarStatusContentHorizontalPadding * 2.0;
}

- (CGFloat)projectCardInnerWidth {
  return self.contentWidth -
         GhostexGpuiMenuBarStatusProjectCardHorizontalPadding * 2.0;
}

- (void)configureContent {
  GhostexGpuiMenuBarStatusContentView *contentView =
      [[GhostexGpuiMenuBarStatusContentView alloc] initWithFrame:NSZeroRect];
  __weak typeof(self) weakSelf = self;
  contentView.hoverChanged = ^(BOOL hovered) {
    GhostexGpuiMenuBarStatusPanelController *strongSelf = weakSelf;
    if (strongSelf == nil) {
      return;
    }
    strongSelf->_isMouseInsidePanel = hovered;
    [strongSelf updateScrollbar];
  };
  contentView.wantsLayer = YES;
  contentView.layer.backgroundColor =
      [NSColor colorWithCalibratedWhite:0x1e / 255.0 alpha:1.0].CGColor;
  contentView.layer.borderColor =
      [NSColor colorWithCalibratedWhite:0x4f / 255.0 alpha:0.72].CGColor;
  contentView.layer.borderWidth = 1.0;
  contentView.layer.cornerRadius = 18.0;
  contentView.layer.masksToBounds = YES;
  _panel.contentView = contentView;

  _focusSink = [[GhostexGpuiMenuBarStatusFocusSink alloc]
      initWithFrame:NSMakeRect(-8.0, -8.0, 1.0, 1.0)];
  _focusSink.translatesAutoresizingMaskIntoConstraints = NO;
  [contentView addSubview:_focusSink];

  NSStackView *rootStack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  rootStack.orientation = NSUserInterfaceLayoutOrientationVertical;
  rootStack.alignment = NSLayoutAttributeLeading;
  rootStack.distribution = NSStackViewDistributionFill;
  rootStack.spacing = 0.0;
  rootStack.translatesAutoresizingMaskIntoConstraints = NO;
  [contentView addSubview:rootStack];

  _rowsContainerView = [[NSView alloc] initWithFrame:NSZeroRect];
  _rowsContainerView.translatesAutoresizingMaskIntoConstraints = NO;
  _scrollView = [[NSScrollView alloc] initWithFrame:NSZeroRect];
  _scrollView.drawsBackground = NO;
  _scrollView.borderType = NSNoBorder;
  _scrollView.hasVerticalScroller = NO;
  _scrollView.autohidesScrollers = YES;
  _scrollView.translatesAutoresizingMaskIntoConstraints = NO;
  _rowsContentView = [[GhostexGpuiMenuBarStatusFlippedView alloc]
      initWithFrame:NSMakeRect(0.0, 0.0, self.contentWidth,
                               GhostexGpuiMenuBarStatusEmptyHeight)];
  _scrollView.documentView = _rowsContentView;
  _scrollView.contentView.postsBoundsChangedNotifications = YES;
  [[NSNotificationCenter defaultCenter]
      addObserver:self
         selector:@selector(scrollBoundsDidChange:)
             name:NSViewBoundsDidChangeNotification
           object:_scrollView.contentView];
  [_rowsContainerView addSubview:_scrollView];
  _scrollbarView =
      [[GhostexGpuiMenuBarStatusThinScrollbar alloc] initWithFrame:NSZeroRect];
  _scrollbarView.translatesAutoresizingMaskIntoConstraints = NO;
  _scrollbarView.hidden = YES;
  [contentView addSubview:_scrollbarView];

  _rowsStack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  _rowsStack.orientation = NSUserInterfaceLayoutOrientationVertical;
  _rowsStack.alignment = NSLayoutAttributeLeading;
  _rowsStack.distribution = NSStackViewDistributionFill;
  _rowsStack.spacing = GhostexGpuiMenuBarStatusProjectSectionSpacing;
  _rowsStack.translatesAutoresizingMaskIntoConstraints = NO;
  [_rowsContentView addSubview:_rowsStack];

  NSBox *separator = [[NSBox alloc] initWithFrame:NSZeroRect];
  separator.boxType = NSBoxSeparator;
  separator.translatesAutoresizingMaskIntoConstraints = NO;

  NSStackView *footerStack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  footerStack.orientation = NSUserInterfaceLayoutOrientationVertical;
  footerStack.alignment = NSLayoutAttributeLeading;
  footerStack.distribution = NSStackViewDistributionFill;
  footerStack.edgeInsets = NSEdgeInsetsMake(4.0, 0.0, 2.0, 0.0);
  footerStack.spacing = 0.0;
  footerStack.translatesAutoresizingMaskIntoConstraints = NO;
  [footerStack addArrangedSubview:[self actionButtonWithTitle:@"Restart Ghostex"
                                                       action:@selector
                                                       (restartGhostex:)]];
  [footerStack
      addArrangedSubview:[self actionButtonWithTitle:@"Quit Ghostex"
                                              action:@selector(quitGhostex:)]];

  [rootStack addArrangedSubview:_rowsContainerView];
  [rootStack addArrangedSubview:separator];
  [rootStack addArrangedSubview:footerStack];

  _rowsHeightConstraint = [_rowsContainerView.heightAnchor
      constraintEqualToConstant:GhostexGpuiMenuBarStatusEmptyHeight];
  [NSLayoutConstraint activateConstraints:@[
    [rootStack.leadingAnchor
        constraintEqualToAnchor:contentView.leadingAnchor
                       constant:
                           GhostexGpuiMenuBarStatusContentHorizontalPadding],
    [rootStack.trailingAnchor
        constraintEqualToAnchor:contentView.trailingAnchor
                       constant:
                           -GhostexGpuiMenuBarStatusContentHorizontalPadding],
    [rootStack.topAnchor
        constraintEqualToAnchor:contentView.topAnchor
                       constant:GhostexGpuiMenuBarStatusContentVerticalPadding],
    [rootStack.bottomAnchor
        constraintEqualToAnchor:contentView.bottomAnchor
                       constant:-
                                GhostexGpuiMenuBarStatusContentVerticalPadding],

    [_focusSink.leadingAnchor constraintEqualToAnchor:contentView.leadingAnchor
                                             constant:-8.0],
    [_focusSink.topAnchor constraintEqualToAnchor:contentView.topAnchor
                                         constant:-8.0],
    [_focusSink.widthAnchor constraintEqualToConstant:1.0],
    [_focusSink.heightAnchor constraintEqualToConstant:1.0],

    [_rowsContainerView.widthAnchor
        constraintEqualToConstant:self.contentWidth],
    _rowsHeightConstraint,
    [_scrollView.leadingAnchor
        constraintEqualToAnchor:_rowsContainerView.leadingAnchor],
    [_scrollView.trailingAnchor
        constraintEqualToAnchor:_rowsContainerView.trailingAnchor],
    [_scrollView.topAnchor
        constraintEqualToAnchor:_rowsContainerView.topAnchor],
    [_scrollView.bottomAnchor
        constraintEqualToAnchor:_rowsContainerView.bottomAnchor],
    [_scrollbarView.trailingAnchor
        constraintEqualToAnchor:contentView.trailingAnchor],
    [_scrollbarView.topAnchor
        constraintEqualToAnchor:_rowsContainerView.topAnchor],
    [_scrollbarView.bottomAnchor
        constraintEqualToAnchor:_rowsContainerView.bottomAnchor],
    [_scrollbarView.widthAnchor
        constraintEqualToConstant:GhostexGpuiMenuBarStatusScrollbarWidth],

    [_rowsStack.leadingAnchor
        constraintEqualToAnchor:_rowsContentView.leadingAnchor],
    [_rowsStack.trailingAnchor
        constraintEqualToAnchor:_rowsContentView.trailingAnchor],
    [_rowsStack.topAnchor constraintEqualToAnchor:_rowsContentView.topAnchor],
    [_rowsStack.widthAnchor constraintEqualToConstant:self.contentWidth],

    [separator.widthAnchor constraintEqualToConstant:self.contentWidth],
    [footerStack.widthAnchor constraintEqualToConstant:self.contentWidth],
    [footerStack.heightAnchor
        constraintEqualToConstant:GhostexGpuiMenuBarStatusFooterHeight],
  ]];
}

- (GhostexGpuiMenuBarStatusActionButton *)actionButtonWithTitle:
                                              (NSString *)title
                                                         action:(SEL)action {
  GhostexGpuiMenuBarStatusActionButton *button =
      [[GhostexGpuiMenuBarStatusActionButton alloc] initWithFrame:NSZeroRect];
  button.title = title;
  button.target = self;
  button.action = action;
  button.translatesAutoresizingMaskIntoConstraints = NO;
  [button.heightAnchor
      constraintEqualToConstant:GhostexGpuiMenuBarStatusFooterRowHeight]
      .active = YES;
  [button.widthAnchor constraintEqualToConstant:self.contentWidth].active = YES;
  [_footerActionButtons addObject:button];
  return button;
}

- (NSArray<GhostexGpuiMenuBarStatusProjectModel *> *)nonEmptyProjects {
  NSMutableArray<GhostexGpuiMenuBarStatusProjectModel *> *nonEmptyProjects =
      [NSMutableArray array];
  for (GhostexGpuiMenuBarStatusProjectModel *project in self.projects) {
    if (project.sessions.count > 0) {
      [nonEmptyProjects addObject:project];
    }
  }
  return nonEmptyProjects;
}

- (void)rebuildRows {
  [self setHoveredSessionRow:nil];
  for (NSView *view in _rowsStack.arrangedSubviews.copy) {
    [_rowsStack removeArrangedSubview:view];
    [view removeFromSuperview];
  }

  NSArray<GhostexGpuiMenuBarStatusProjectModel *> *projects =
      [self nonEmptyProjects];
  if (projects.count == 0) {
    [_rowsStack addArrangedSubview:[self emptyLabel]];
  } else {
    for (GhostexGpuiMenuBarStatusProjectModel *project in projects) {
      [_rowsStack addArrangedSubview:[self projectSection:project]];
    }
  }
  [_rowsContentView
      setFrameSize:NSMakeSize(self.contentWidth, [self preferredRowsHeight])];
  [_rowsStack layoutSubtreeIfNeeded];
  [self updateScrollbar];
}

- (NSTextField *)emptyLabel {
  NSTextField *label = [NSTextField labelWithString:@"No running agents"];
  label.font = [NSFont systemFontOfSize:13.0 weight:NSFontWeightMedium];
  label.textColor = NSColor.secondaryLabelColor;
  label.alignment = NSTextAlignmentCenter;
  label.translatesAutoresizingMaskIntoConstraints = NO;
  [label.heightAnchor
      constraintEqualToConstant:GhostexGpuiMenuBarStatusEmptyHeight]
      .active = YES;
  [label.widthAnchor constraintEqualToConstant:self.contentWidth].active = YES;
  return label;
}

- (NSStackView *)projectSection:
    (GhostexGpuiMenuBarStatusProjectModel *)project {
  NSStackView *sectionStack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  sectionStack.orientation = NSUserInterfaceLayoutOrientationVertical;
  sectionStack.alignment = NSLayoutAttributeLeading;
  sectionStack.distribution = NSStackViewDistributionFill;
  sectionStack.spacing = GhostexGpuiMenuBarStatusProjectTitleCardGap;
  sectionStack.translatesAutoresizingMaskIntoConstraints = NO;
  [sectionStack addArrangedSubview:[self projectButton:project]];
  [sectionStack addArrangedSubview:[self projectCard:project]];
  [sectionStack.widthAnchor constraintEqualToConstant:self.contentWidth]
      .active = YES;
  [sectionStack.heightAnchor
      constraintEqualToConstant:[self projectSectionHeight:project]]
      .active = YES;
  return sectionStack;
}

- (NSView *)projectCard:(GhostexGpuiMenuBarStatusProjectModel *)project {
  GhostexGpuiMenuBarStatusProjectCardView *card =
      [[GhostexGpuiMenuBarStatusProjectCardView alloc]
          initWithFrame:NSZeroRect];
  card.translatesAutoresizingMaskIntoConstraints = NO;
  NSStackView *cardStack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  cardStack.orientation = NSUserInterfaceLayoutOrientationVertical;
  cardStack.alignment = NSLayoutAttributeLeading;
  cardStack.distribution = NSStackViewDistributionFill;
  cardStack.spacing = 0.0;
  cardStack.translatesAutoresizingMaskIntoConstraints = NO;
  [card addSubview:cardStack];

  NSArray<GhostexGpuiMenuBarStatusSessionModel *> *sortedSessions =
      [project.sessions sortedArrayUsingComparator:^NSComparisonResult(
                            GhostexGpuiMenuBarStatusSessionModel *left,
                            GhostexGpuiMenuBarStatusSessionModel *right) {
        if (left.order < right.order) {
          return NSOrderedAscending;
        }
        if (left.order > right.order) {
          return NSOrderedDescending;
        }
        return NSOrderedSame;
      }];
  for (GhostexGpuiMenuBarStatusSessionModel *session in sortedSessions) {
    [cardStack addArrangedSubview:[self sessionRowWithProject:project
                                                      session:session]];
  }

  [NSLayoutConstraint activateConstraints:@[
    [card.widthAnchor constraintEqualToConstant:self.contentWidth],
    [card.heightAnchor
        constraintEqualToConstant:[self projectSessionsCardHeight:project]],
    [cardStack.leadingAnchor
        constraintEqualToAnchor:card.leadingAnchor
                       constant:
                           GhostexGpuiMenuBarStatusProjectCardHorizontalPadding],
    [cardStack.trailingAnchor
        constraintEqualToAnchor:card.trailingAnchor
                       constant:
                           -
                           GhostexGpuiMenuBarStatusProjectCardHorizontalPadding],
    [cardStack.topAnchor
        constraintEqualToAnchor:card.topAnchor
                       constant:
                           GhostexGpuiMenuBarStatusProjectCardVerticalPadding],
    [cardStack.bottomAnchor
        constraintEqualToAnchor:card.bottomAnchor
                       constant:
                           -GhostexGpuiMenuBarStatusProjectCardVerticalPadding],
  ]];
  return card;
}

- (GhostexGpuiMenuBarStatusProjectButton *)projectButton:
    (GhostexGpuiMenuBarStatusProjectModel *)project {
  GhostexGpuiMenuBarStatusProjectButton *button =
      [[GhostexGpuiMenuBarStatusProjectButton alloc]
          initWithProjectId:project.projectId];
  button.title = project.title;
  button.target = self;
  button.action = @selector(projectClicked:);
  button.translatesAutoresizingMaskIntoConstraints = NO;
  [button.heightAnchor
      constraintEqualToConstant:GhostexGpuiMenuBarStatusProjectHeaderHeight]
      .active = YES;
  [button.widthAnchor constraintEqualToConstant:self.contentWidth].active = YES;
  return button;
}

- (GhostexGpuiMenuBarStatusSessionRow *)
    sessionRowWithProject:(GhostexGpuiMenuBarStatusProjectModel *)project
                  session:(GhostexGpuiMenuBarStatusSessionModel *)session {
  GhostexGpuiMenuBarStatusSessionRow *row =
      [[GhostexGpuiMenuBarStatusSessionRow alloc]
          initWithProjectId:project.projectId
                    session:session];
  row.target = self;
  row.action = @selector(sessionClicked:);
  __weak typeof(self) weakSelf = self;
  row.hoverChanged = ^(GhostexGpuiMenuBarStatusSessionRow *hoveredRow) {
    [weakSelf setHoveredSessionRow:hoveredRow];
  };
  row.translatesAutoresizingMaskIntoConstraints = NO;
  [row.heightAnchor constraintEqualToConstant:GhostexGpuiMenuBarStatusRowHeight]
      .active = YES;
  [row.widthAnchor constraintEqualToConstant:self.projectCardInnerWidth]
      .active = YES;
  return row;
}

- (CGFloat)preferredRowsHeight {
  NSArray<GhostexGpuiMenuBarStatusProjectModel *> *projects =
      [self nonEmptyProjects];
  if (projects.count == 0) {
    return GhostexGpuiMenuBarStatusEmptyHeight;
  }
  CGFloat height = 0.0;
  for (GhostexGpuiMenuBarStatusProjectModel *project in projects) {
    height += [self projectSectionHeight:project];
  }
  height += MAX((NSInteger)projects.count - 1, 0) *
            GhostexGpuiMenuBarStatusProjectSectionSpacing;
  return height;
}

- (CGFloat)projectSectionHeight:
    (GhostexGpuiMenuBarStatusProjectModel *)project {
  return GhostexGpuiMenuBarStatusProjectHeaderHeight +
         GhostexGpuiMenuBarStatusProjectTitleCardGap +
         [self projectSessionsCardHeight:project];
}

- (CGFloat)projectSessionsCardHeight:
    (GhostexGpuiMenuBarStatusProjectModel *)project {
  return GhostexGpuiMenuBarStatusProjectCardVerticalPadding * 2.0 +
         project.sessions.count * GhostexGpuiMenuBarStatusRowHeight;
}

- (CGFloat)preferredPanelHeight {
  CGFloat requestedHeight =
      [self preferredRowsHeight] + GhostexGpuiMenuBarStatusFooterHeight +
      GhostexGpuiMenuBarStatusContentVerticalPadding * 2.0;
  return MIN(GhostexGpuiMenuBarStatusMaxPanelHeight,
             MAX(GhostexGpuiMenuBarStatusMinPanelHeight, requestedHeight));
}

- (void)setHoveredSessionRow:(GhostexGpuiMenuBarStatusSessionRow *)row {
  if (_hoveredSessionRow == row) {
    return;
  }
  [_hoveredSessionRow setHovered:NO];
  _hoveredSessionRow = row;
  [_hoveredSessionRow setHovered:YES];
}

- (void)showFromStatusButton:(NSStatusBarButton *)button {
  _isMouseInsidePanel = NO;
  [self rebuildRows];
  CGFloat panelHeight = [self preferredPanelHeight];
  _rowsHeightConstraint.constant =
      panelHeight - GhostexGpuiMenuBarStatusFooterHeight -
      GhostexGpuiMenuBarStatusContentVerticalPadding * 2.0;
  [_panel.contentView layoutSubtreeIfNeeded];
  [_panel
      setFrame:[self panelFrameWithSize:NSMakeSize(
                                            GhostexGpuiMenuBarStatusPanelWidth,
                                            panelHeight)
                             anchoredTo:button]
       display:YES];
  [_panel orderFrontRegardless];
  [_panel makeFirstResponder:_focusSink];
  [self installDismissEventMonitors];
  [self updateScrollbar];
}

- (NSRect)panelFrameWithSize:(NSSize)size
                  anchoredTo:(NSStatusBarButton *)button {
  NSScreen *fallbackScreen =
      NSScreen.mainScreen ?: NSScreen.screens.firstObject;
  NSRect buttonFrame =
      button.window
          ? [button.window convertRectToScreen:[button convertRect:button.bounds
                                                            toView:nil]]
          : NSMakeRect(NSEvent.mouseLocation.x, NSEvent.mouseLocation.y, 0.0,
                       0.0);
  NSRect screenFrame = fallbackScreen.visibleFrame;
  CGFloat x = MIN(
      MAX(NSMinX(screenFrame) + 8.0, NSMidX(buttonFrame) - size.width / 2.0),
      NSMaxX(screenFrame) - size.width - 8.0);
  CGFloat proposedY = NSMinY(buttonFrame) - size.height - 8.0;
  CGFloat y = proposedY >= NSMinY(screenFrame) + 8.0
                  ? proposedY
                  : NSMaxY(buttonFrame) + 8.0;
  return NSMakeRect(x, y, size.width, size.height);
}

- (void)installDismissEventMonitors {
  [self removeDismissEventMonitors];
  __weak typeof(self) weakSelf = self;
  _localDismissEventMonitor = [NSEvent
      addLocalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown |
                                           NSEventMaskRightMouseDown
                                   handler:^NSEvent *(NSEvent *event) {
                                     GhostexGpuiMenuBarStatusPanelController
                                         *strongSelf = weakSelf;
                                     if (strongSelf &&
                                         strongSelf->_panel.visible &&
                                         event.window != strongSelf->_panel) {
                                       [strongSelf dismissPanel];
                                     }
                                     return event;
                                   }];
  _globalDismissEventMonitor =
      [NSEvent addGlobalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown |
                                                     NSEventMaskRightMouseDown
                                             handler:^(NSEvent *event) {
                                               [weakSelf dismissPanel];
                                             }];
}

- (void)removeDismissEventMonitors {
  if (_localDismissEventMonitor != nil) {
    [NSEvent removeMonitor:_localDismissEventMonitor];
    _localDismissEventMonitor = nil;
  }
  if (_globalDismissEventMonitor != nil) {
    [NSEvent removeMonitor:_globalDismissEventMonitor];
    _globalDismissEventMonitor = nil;
  }
}

- (void)scrollBoundsDidChange:(NSNotification *)notification {
  [self updateScrollbar];
}

- (void)updateScrollbar {
  if (_scrollbarView == nil || _scrollView.superview == nil) {
    return;
  }
  CGFloat visibleHeight = MAX(0.0, NSHeight(_scrollView.contentView.bounds));
  CGFloat contentHeight = MAX(0.0, NSHeight(_rowsContentView.bounds));
  if (!_isMouseInsidePanel || visibleHeight <= 0.0 ||
      contentHeight <= visibleHeight + 1.0) {
    _scrollbarView.hidden = YES;
    return;
  }
  CGFloat maxOffset = MAX(1.0, contentHeight - visibleHeight);
  _scrollbarView.hidden = NO;
  _scrollbarView.knobHeightFraction = MIN(1.0, visibleHeight / contentHeight);
  _scrollbarView.knobOffsetFraction =
      MIN(1.0, MAX(0.0, _scrollView.contentView.bounds.origin.y / maxOffset));
}

- (void)dismissPanel {
  _isMouseInsidePanel = NO;
  _scrollbarView.hidden = YES;
  [self setHoveredSessionRow:nil];
  for (GhostexGpuiMenuBarStatusActionButton *button in _footerActionButtons) {
    [button setHovered:NO];
  }
  if (_panel.visible) {
    [_panel orderOut:nil];
  }
  [self removeDismissEventMonitors];
}

- (void)windowDidResignKey:(NSNotification *)notification {
  [self dismissPanel];
}

- (void)projectClicked:(GhostexGpuiMenuBarStatusProjectButton *)sender {
  [self dismissPanel];
  GhostexGpuiMenuBarStatusActivateApplication();
  GhostexGpuiMenuBarStatusProjectClicked(sender.projectId.UTF8String);
}

- (void)sessionClicked:(GhostexGpuiMenuBarStatusSessionRow *)sender {
  [self dismissPanel];
  GhostexGpuiMenuBarStatusActivateApplication();
  GhostexGpuiMenuBarStatusSessionClicked(sender.projectId.UTF8String,
                                         sender.sessionId.UTF8String);
}

- (void)restartGhostex:(GhostexGpuiMenuBarStatusActionButton *)sender {
  [self dismissPanel];
  NSWorkspaceOpenConfiguration *configuration =
      [NSWorkspaceOpenConfiguration configuration];
  configuration.createsNewApplicationInstance = YES;
  [NSWorkspace.sharedWorkspace
      openApplicationAtURL:NSBundle.mainBundle.bundleURL
             configuration:configuration
         completionHandler:^(NSRunningApplication *app, NSError *error) {
           [NSApp terminate:nil];
         }];
}

- (void)quitGhostex:(GhostexGpuiMenuBarStatusActionButton *)sender {
  [self dismissPanel];
  [NSApp terminate:nil];
}
@end

@interface GhostexGpuiMenuBarStatusTarget : NSObject
@property(nonatomic, strong)
    GhostexGpuiMenuBarStatusPanelController *panelController;
- (void)statusItemClicked:(NSStatusBarButton *)sender;
- (void)updateProjects:
    (NSArray<GhostexGpuiMenuBarStatusProjectModel *> *)projects;
- (void)dismissPanel;
@end

@implementation GhostexGpuiMenuBarStatusTarget
- (instancetype)init {
  self = [super init];
  if (self) {
    _panelController = [[GhostexGpuiMenuBarStatusPanelController alloc] init];
  }
  return self;
}

- (void)statusItemClicked:(NSStatusBarButton *)sender {
  NSEvent *event = NSApp.currentEvent;
  /*
   CDXC:GPUIMenuBarStatusItem 2026-06-26-06:29:
   The status button is already registered for leftMouseUp, but AppKit may
   dispatch this action after NSApp.currentEvent is nil or has moved to another
   event. Match the Swift host by ignoring only explicit secondary-click events
   and Control-clicks.
   */
  if (event != nil && (event.type == NSEventTypeRightMouseDown ||
                       event.type == NSEventTypeRightMouseUp)) {
    return;
  }
  if (event != nil && (event.modifierFlags & NSEventModifierFlagControl) != 0) {
    return;
  }
  [self.panelController showFromStatusButton:sender];
}

- (void)updateProjects:
    (NSArray<GhostexGpuiMenuBarStatusProjectModel *> *)projects {
  self.panelController.projects = projects;
}

- (void)dismissPanel {
  [self.panelController dismissPanel];
}
@end

static GhostexGpuiMenuBarStatusTarget *GhostexGpuiMenuBarStatusClickTarget =
    nil;

static GhostexGpuiMenuBarStatusTarget *
GhostexGpuiEnsureMenuBarStatusTarget(void) {
  if (GhostexGpuiMenuBarStatusClickTarget == nil) {
    GhostexGpuiMenuBarStatusClickTarget =
        [[GhostexGpuiMenuBarStatusTarget alloc] init];
  }
  return GhostexGpuiMenuBarStatusClickTarget;
}

static NSUInteger
GhostexGpuiMenuBarStatusEntries(uint64_t attentionCount, uint64_t workingCount,
                                uint64_t availableCount,
                                GhostexGpuiMenuBarStatusEntry entries[3]) {
  NSUInteger count = 0;
  if (attentionCount > 0) {
    entries[count++] = (GhostexGpuiMenuBarStatusEntry){
        .kind = GhostexGpuiMenuBarStatusKindAttention,
        .count = attentionCount,
    };
  }
  if (workingCount > 0) {
    entries[count++] = (GhostexGpuiMenuBarStatusEntry){
        .kind = GhostexGpuiMenuBarStatusKindWorking,
        .count = workingCount,
    };
  }
  if (count == 0) {
    /*
     CDXC:GPUIMenuBarStatusItem 2026-06-26-05:44:
     When Rust applies instead of hides, the menu-bar item must remain visible
     as the Running Agents dropdown target. Render an available-style zero chip
     for the all-empty state; only the Rust hide path removes the NSStatusItem.
     */
    entries[count++] = (GhostexGpuiMenuBarStatusEntry){
        .kind = GhostexGpuiMenuBarStatusKindAvailable,
        .count = availableCount,
    };
  }
  return count;
}

static void GhostexGpuiHideMenuBarStatusItemOnMain(void) {
  [GhostexGpuiMenuBarStatusClickTarget dismissPanel];
  if (GhostexGpuiMenuBarStatusItem == nil) {
    return;
  }
  [NSStatusBar.systemStatusBar removeStatusItem:GhostexGpuiMenuBarStatusItem];
  GhostexGpuiMenuBarStatusItem = nil;
}

static NSStatusItem *GhostexGpuiEnsureMenuBarStatusItem(void) {
  if (GhostexGpuiMenuBarStatusItem == nil) {
    GhostexGpuiMenuBarStatusItem = [NSStatusBar.systemStatusBar
        statusItemWithLength:NSVariableStatusItemLength];
    GhostexGpuiMenuBarStatusItem.visible = NO;
  }
  return GhostexGpuiMenuBarStatusItem;
}

static void GhostexGpuiApplyMenuBarStatusItemOnMain(
    uint64_t attentionCount, uint64_t workingCount, uint64_t availableCount,
    NSArray<GhostexGpuiMenuBarStatusProjectModel *> *copiedProjects) {
  /*
   CDXC:GPUIMenuBarStatusItem 2026-06-26-05:42:
   The compact NSStatusItem badge is still driven by primitive counts selected
   by Rust; renderer-owned menu-bar payloads remain rejected.

   CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
   The Running Agents dropdown may receive only Rust-owned sanitized
   project/session ids, titles, status, order, and timestamps from the strict
   parser. AppKit copies that data at the FFI boundary and owns only
   presentation plus fixed callbacks, never paths, URLs, command text,
   stdout/stderr, tokens, environment, terminal content, renderer JSON, hidden
   hit regions, or overlay routing.
  */
  GhostexGpuiMenuBarStatusTarget *target =
      GhostexGpuiEnsureMenuBarStatusTarget();
  [target updateProjects:copiedProjects ?: @[]];

  GhostexGpuiMenuBarStatusEntry entries[3] = {0};
  NSUInteger count = GhostexGpuiMenuBarStatusEntries(
      attentionCount, workingCount, availableCount, entries);
  if (count == 0) {
    /*
     CDXC:GPUIMenuBarStatusItem 2026-06-26-05:44:
     Applying primitive counts is a visible-state operation; hiding is reserved
     for the explicit Rust None path. Keep a defensive available-style zero
     entry here so future edits to the entry builder cannot make an all-zero
     apply remove the dropdown target.
     */
    entries[count++] = (GhostexGpuiMenuBarStatusEntry){
        .kind = GhostexGpuiMenuBarStatusKindAvailable,
        .count = 0,
    };
  }

  NSStatusItem *item = GhostexGpuiEnsureMenuBarStatusItem();
  NSSize preferredSize = GhostexGpuiMenuBarStatusPreferredSize(entries, count);
  item.length = preferredSize.width;
  item.visible = YES;

  NSStatusBarButton *button = item.button;
  if (button == nil) {
    return;
  }
  button.action = @selector(statusItemClicked:);
  button.target = target;
  [button sendActionOn:NSEventMaskLeftMouseUp];
  button.image = GhostexGpuiMenuBarStatusImage(entries, count);
  button.imagePosition = NSImageOnly;
  button.toolTip = @"Ghostex session status";
}

void GhostexGpuiApplyMenuBarStatusItemWithProjects(
    uint64_t attentionCount, uint64_t workingCount, uint64_t availableCount,
    const GhostexGpuiMenuBarStatusProjectEntry *projects,
    uintptr_t projectCount) {
  NSArray<GhostexGpuiMenuBarStatusProjectModel *> *copiedProjects =
      GhostexGpuiCopyMenuBarStatusProjects(projects, projectCount);
  if (NSThread.isMainThread) {
    GhostexGpuiApplyMenuBarStatusItemOnMain(attentionCount, workingCount,
                                            availableCount, copiedProjects);
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    GhostexGpuiApplyMenuBarStatusItemOnMain(attentionCount, workingCount,
                                            availableCount, copiedProjects);
  });
}

void GhostexGpuiApplyMenuBarStatusItem(uint64_t attentionCount,
                                       uint64_t workingCount,
                                       uint64_t availableCount) {
  GhostexGpuiApplyMenuBarStatusItemWithProjects(attentionCount, workingCount,
                                                availableCount, NULL, 0);
}

void GhostexGpuiHideMenuBarStatusItem(void) {
  if (NSThread.isMainThread) {
    GhostexGpuiHideMenuBarStatusItemOnMain();
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    GhostexGpuiHideMenuBarStatusItemOnMain();
  });
}
