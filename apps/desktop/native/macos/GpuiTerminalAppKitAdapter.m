#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>
#import <QuartzCore/QuartzCore.h>
#if __has_include(<UniformTypeIdentifiers/UniformTypeIdentifiers.h>)
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#endif
#import <stdbool.h>
#import <stdint.h>

enum {
  GhostexGpuiGhosttyModsNone = 0,
  GhostexGpuiGhosttyModsShift = 1 << 0,
  GhostexGpuiGhosttyModsCtrl = 1 << 1,
  GhostexGpuiGhosttyModsAlt = 1 << 2,
  GhostexGpuiGhosttyModsSuper = 1 << 3,
  GhostexGpuiGhosttyModsCaps = 1 << 4,
  GhostexGpuiGhosttyModsShiftRight = 1 << 6,
  GhostexGpuiGhosttyModsCtrlRight = 1 << 7,
  GhostexGpuiGhosttyModsAltRight = 1 << 8,
  GhostexGpuiGhosttyModsSuperRight = 1 << 9,
};

enum {
  GhostexGpuiGhosttyActionRelease = 0,
  GhostexGpuiGhosttyActionPress = 1,
  GhostexGpuiGhosttyActionRepeat = 2,
};

extern int GhostexGpuiTerminalNativeViewKeyTranslationMods(void *nativeView,
                                                           int mods);
extern int GhostexGpuiTerminalHandleNativeKeyEvent(
    void *nativeView, int action, int mods, int consumedMods, uint32_t keycode,
    const char *text, uint32_t unshiftedCodepoint, int composing);
extern int GhostexGpuiTerminalNativeKeyEventIsBinding(
    void *nativeView, int action, int mods, int consumedMods, uint32_t keycode,
    const char *text, uint32_t unshiftedCodepoint, int composing);
extern int GhostexGpuiTerminalHandlePromptEditorShortcut(void *nativeView);
extern int GhostexGpuiTerminalInsertDroppedText(void *nativeView,
                                                const char *bytes,
                                                uintptr_t len);
extern int GhostexGpuiTerminalInsertCommittedText(void *nativeView,
                                                  const char *bytes,
                                                  uintptr_t len);
extern int GhostexGpuiTerminalSetPreeditText(void *nativeView,
                                             const char *bytes, uintptr_t len);
extern int GhostexGpuiTerminalGetImePoint(void *nativeView, double *x,
                                          double *y, double *width,
                                          double *height);

/*
 CDXC:GPUTerminalAppKitAdapter 2026-06-22-20:58:
 Future real terminal adapters must supply the existing AppKit terminal NSView.
 This GPUI-local boundary may only position, show, hide, or focus that non-null
 view using exact terminal body bounds and the parent view's flipped-coordinate
 convention; it must not create terminal views, transparent overlays, hit-test
 routing, synthetic input routing, terminal processes, GhosttyKit calls, or
 persistent logs.

 CDXC:GPUTerminalAppKitAdapter 2026-06-22-21:42:
 Slice 108 creates only the terminal host NSView ownership boundary: an explicit
 parent NSView receives one normal hidden black child inside GPUI's measured
 terminal body bounds. The child view must remain ordinary AppKit layout, with
 no overlays, broad hitTest overrides, synthetic event routing, transparent
 hidden hit regions, Ghostty/libghostty calls, process lifecycle, logging, or
 app wiring.

 CDXC:GPUTerminalAppKitAdapter 2026-06-22-23:11:
 Slice 115 first-responder handoff may call `makeFirstResponder` only for the
 exact App-owned terminal host NSView supplied by Rust after a real focused
 Agents Ghostty surface is mounted. Do not expand this shim into hit-test
 overrides, pre-dispatch routing, synthetic input, transparent overlays,
 terminal lifecycle, logging, or fallback view creation.

 CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
 The GPUI host view is the exact AppKit responder for mounted Ghostty terminals
 because GPUI key events do not expose the native macOS keycode required for
 Return, Backspace, arrows, modifiers, and bindings. Forward only synchronous
 key primitives from this child view to Rust; do not add root/window routing,
 transparent overlays, hit-test overrides, command text logging,
 terminal-content capture, or persistent state.

 CDXC:GPUITerminalFileDrop 2026-06-27-03:32:
 Direct terminal file and image drops for Agents and command-pane terminals must
 use the real mounted host view as the drag destination and insert only
 transient formatted text through Rust. Keep this path free of overlays,
 hit-test routing, persistent logging, and file persistence so drag/drop matches
 native Swift terminal pane behavior.

 CDXC:GPUITerminalIME 2026-06-27-03:46:
 Agents and command-pane Ghostty host views must use AppKit NSTextInputClient
 for printable, Space, dead-key, and CJK composition while command/control
 shortcuts remain raw only when no marked text exists. Keep marked text and
 per-key committed text runtime-only, route committed/preedit bytes and
 candidate geometry synchronously through the exact host-view Rust callbacks,
 and do not add overlays, hit-test routing, Escape sidebands during composition,
 logs, persistence, command-text storage, terminal content capture, or
 focused-surface fallback.
 */
static int GhostexGpuiTerminalGhosttyMods(NSEventModifierFlags flags) {
  int mods = GhostexGpuiGhosttyModsNone;

  if ((flags & NSEventModifierFlagShift) != 0)
    mods |= GhostexGpuiGhosttyModsShift;
  if ((flags & NSEventModifierFlagControl) != 0)
    mods |= GhostexGpuiGhosttyModsCtrl;
  if ((flags & NSEventModifierFlagOption) != 0)
    mods |= GhostexGpuiGhosttyModsAlt;
  if ((flags & NSEventModifierFlagCommand) != 0)
    mods |= GhostexGpuiGhosttyModsSuper;
  if ((flags & NSEventModifierFlagCapsLock) != 0)
    mods |= GhostexGpuiGhosttyModsCaps;

  if ((flags & NX_DEVICERSHIFTKEYMASK) != 0)
    mods |= GhostexGpuiGhosttyModsShiftRight;
  if ((flags & NX_DEVICERCTLKEYMASK) != 0)
    mods |= GhostexGpuiGhosttyModsCtrlRight;
  if ((flags & NX_DEVICERALTKEYMASK) != 0)
    mods |= GhostexGpuiGhosttyModsAltRight;
  if ((flags & NX_DEVICERCMDKEYMASK) != 0)
    mods |= GhostexGpuiGhosttyModsSuperRight;

  return mods;
}

static NSEventModifierFlags
GhostexGpuiTerminalTranslatedModifierFlags(NSEventModifierFlags originalFlags,
                                           int translatedMods) {
  NSEventModifierFlags flags = originalFlags;

  if ((translatedMods & GhostexGpuiGhosttyModsShift) != 0) {
    flags |= NSEventModifierFlagShift;
  } else {
    flags &= ~NSEventModifierFlagShift;
  }
  if ((translatedMods & GhostexGpuiGhosttyModsCtrl) != 0) {
    flags |= NSEventModifierFlagControl;
  } else {
    flags &= ~NSEventModifierFlagControl;
  }
  if ((translatedMods & GhostexGpuiGhosttyModsAlt) != 0) {
    flags |= NSEventModifierFlagOption;
  } else {
    flags &= ~NSEventModifierFlagOption;
  }
  if ((translatedMods & GhostexGpuiGhosttyModsSuper) != 0) {
    flags |= NSEventModifierFlagCommand;
  } else {
    flags &= ~NSEventModifierFlagCommand;
  }

  return flags;
}

static uint32_t GhostexGpuiTerminalFirstUnicodeScalar(NSString *value) {
  if (value.length == 0) {
    return 0;
  }

  unichar first = [value characterAtIndex:0];
  if (CFStringIsSurrogateHighCharacter(first) && value.length > 1) {
    unichar second = [value characterAtIndex:1];
    if (CFStringIsSurrogateLowCharacter(second)) {
      return (uint32_t)CFStringGetLongCharacterForSurrogatePair(first, second);
    }
  }

  return (uint32_t)first;
}

static uint32_t GhostexGpuiTerminalUnshiftedCodepoint(NSEvent *event) {
  if (event.type != NSEventTypeKeyDown && event.type != NSEventTypeKeyUp) {
    return 0;
  }

  NSString *characters = [event charactersByApplyingModifiers:0];
  return GhostexGpuiTerminalFirstUnicodeScalar(characters);
}

static NSString *
GhostexGpuiTerminalCharactersForEvent(NSEvent *event,
                                      NSEventModifierFlags translationFlags) {
  NSString *characters = nil;
  if (translationFlags == event.modifierFlags) {
    characters = event.characters;
  } else {
    characters = [event charactersByApplyingModifiers:translationFlags];
  }
  if (characters.length == 0) {
    return nil;
  }

  uint32_t scalar = GhostexGpuiTerminalFirstUnicodeScalar(characters);
  if (characters.length == 1 && scalar < 0x20) {
    return [event charactersByApplyingModifiers:(translationFlags &
                                                 ~NSEventModifierFlagControl)];
  }
  if (characters.length == 1 && scalar >= 0xF700 && scalar <= 0xF8FF) {
    return nil;
  }

  return characters;
}

static const char *GhostexGpuiTerminalKeyTextCString(NSString *text) {
  if (text.length == 0) {
    return NULL;
  }

  const char *value = text.UTF8String;
  if (!value || ((unsigned char)value[0]) < 0x20) {
    return NULL;
  }
  return value;
}

static BOOL GhostexGpuiTerminalShouldBypassTextInput(NSEvent *event,
                                                     BOOL hasMarkedText) {
  if (hasMarkedText) {
    return NO;
  }

  NSEventModifierFlags flags =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  return (flags & NSEventModifierFlagCommand) != 0 ||
         (flags & NSEventModifierFlagControl) != 0;
}

static NSEvent *GhostexGpuiTerminalTranslatedTextInputEvent(
    NSEvent *event, NSEventModifierFlags translationFlags) {
  if (translationFlags == event.modifierFlags) {
    return event;
  }

  NSEvent *translatedEvent =
      [NSEvent keyEventWithType:event.type
                             location:event.locationInWindow
                        modifierFlags:translationFlags
                            timestamp:event.timestamp
                         windowNumber:event.windowNumber
                              context:nil
                           characters:[event charactersByApplyingModifiers:
                                                 translationFlags]
                                          ?: @""
          charactersIgnoringModifiers:event.charactersIgnoringModifiers ?: @""
                            isARepeat:event.isARepeat
                              keyCode:event.keyCode];
  return translatedEvent ?: event;
}

static int
GhostexGpuiTerminalConsumedTextInputMods(NSEventModifierFlags flags) {
  return GhostexGpuiTerminalGhosttyMods(
      flags & ~(NSEventModifierFlagControl | NSEventModifierFlagCommand));
}

static BOOL
GhostexGpuiTerminalShouldSuppressComposingControlInput(NSString *text,
                                                       BOOL composing) {
  if (!composing || text.length == 0) {
    return NO;
  }

  if (text.length == 1) {
    return [text characterAtIndex:0] < 0x20;
  }

  return NO;
}

static NSString *GhostexGpuiTerminalTextInputString(id string) {
  /*
   CDXC:GPUITerminalNativeImeBridge 2026-07-03-00:58:
   AppKit's hardware text-input pipeline can pass insertText:/setMarkedText: a
   mutable string it reuses and empties after the callback returns. Accumulated
   key text and marked text must own their characters, so copy here; otherwise
   keyDown later reads an empty string and forwards printable keys to Ghostty
   with no text, which encodes to nothing outside the kitty keyboard protocol.
   */
  if ([string isKindOfClass:[NSString class]]) {
    return [(NSString *)string copy];
  }
  if ([string isKindOfClass:[NSAttributedString class]]) {
    return [[(NSAttributedString *)string string] copy] ?: @"";
  }
  return @"";
}

static NSPasteboardType GhostexGpuiTerminalFileURLPasteboardType(void) {
  return NSPasteboardTypeFileURL;
}

static NSPasteboardType GhostexGpuiTerminalLegacyFilenamesPasteboardType(void) {
  return (NSPasteboardType) @"NSFilenamesPboardType";
}

static NSArray<NSString *> *GhostexGpuiTerminalRegisteredDragTypes(void) {
  return @[
    GhostexGpuiTerminalFileURLPasteboardType(),
    NSPasteboardTypeString,
    GhostexGpuiTerminalLegacyFilenamesPasteboardType(),
  ];
}

static BOOL
GhostexGpuiTerminalAppendUniquePath(NSMutableArray<NSString *> *paths,
                                    NSMutableSet<NSString *> *seen,
                                    NSString *path) {
  if (path.length == 0 || [seen containsObject:path]) {
    return NO;
  }
  [paths addObject:path];
  [seen addObject:path];
  return YES;
}

static NSArray<NSString *> *
GhostexGpuiTerminalStringDroppedPaths(NSString *value) {
  if (value.length == 0) {
    return @[];
  }

  NSMutableArray<NSString *> *paths = [NSMutableArray array];
  NSArray<NSString *> *candidates =
      [value componentsSeparatedByCharactersInSet:[NSCharacterSet
                                                      newlineCharacterSet]];
  NSFileManager *fileManager = [NSFileManager defaultManager];

  for (NSString *candidate in candidates) {
    NSString *trimmed =
        [candidate stringByTrimmingCharactersInSet:[NSCharacterSet
                                                       whitespaceCharacterSet]];
    if (trimmed.length == 0) {
      continue;
    }

    if ([trimmed hasPrefix:@"file://"]) {
      NSURL *url = [NSURL URLWithString:trimmed];
      if (url.isFileURL) {
        [paths addObject:url.path];
        continue;
      }
      return @[];
    }

    if ([trimmed hasPrefix:@"/"] && [fileManager fileExistsAtPath:trimmed]) {
      [paths addObject:trimmed];
      continue;
    }

    return @[];
  }

  return paths;
}

static NSArray<NSString *> *
GhostexGpuiTerminalDroppedPaths(NSPasteboard *pasteboard) {
  NSMutableArray<NSString *> *paths = [NSMutableArray array];
  NSMutableSet<NSString *> *seen = [NSMutableSet set];

  NSArray *urlObjects = [pasteboard readObjectsForClasses:@[ [NSURL class] ]
                                                  options:nil];
  for (id object in urlObjects) {
    if (![object isKindOfClass:[NSURL class]]) {
      continue;
    }
    NSURL *url = (NSURL *)object;
    if (url.isFileURL) {
      GhostexGpuiTerminalAppendUniquePath(paths, seen, url.path);
    }
  }

  for (NSPasteboardItem *item in pasteboard.pasteboardItems ?: @[]) {
    NSString *fileURLString =
        [item stringForType:GhostexGpuiTerminalFileURLPasteboardType()];
    if (fileURLString.length == 0) {
      fileURLString =
          [item stringForType:(NSPasteboardType) @"public.file-url"];
    }
    NSURL *url =
        fileURLString.length > 0 ? [NSURL URLWithString:fileURLString] : nil;
    if (url.isFileURL) {
      GhostexGpuiTerminalAppendUniquePath(paths, seen, url.path);
    }
  }

  id filenames = [pasteboard
      propertyListForType:GhostexGpuiTerminalLegacyFilenamesPasteboardType()];
  if ([filenames isKindOfClass:[NSArray class]]) {
    for (id value in (NSArray *)filenames) {
      if ([value isKindOfClass:[NSString class]]) {
        GhostexGpuiTerminalAppendUniquePath(paths, seen, (NSString *)value);
      }
    }
  }

  if (paths.count > 0) {
    return paths;
  }

  NSArray<NSString *> *stringPaths = GhostexGpuiTerminalStringDroppedPaths(
      [pasteboard stringForType:NSPasteboardTypeString]);
  for (NSString *path in stringPaths) {
    GhostexGpuiTerminalAppendUniquePath(paths, seen, path);
  }
  return paths;
}

static NSSet<NSString *> *GhostexGpuiTerminalImageExtensions(void) {
  static NSSet<NSString *> *extensions = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    extensions = [NSSet setWithArray:@[
      @"avif",
      @"gif",
      @"heic",
      @"heif",
      @"jpg",
      @"jpeg",
      @"png",
      @"svg",
      @"tif",
      @"tiff",
      @"webp",
    ]];
  });
  return extensions;
}

static BOOL GhostexGpuiTerminalIsImageFilePath(NSString *path) {
  if (path.length == 0 ||
      ![[NSFileManager defaultManager] fileExistsAtPath:path]) {
    return NO;
  }

  NSString *fileExtension = path.pathExtension.lowercaseString;
  if ([GhostexGpuiTerminalImageExtensions() containsObject:fileExtension]) {
    return YES;
  }

#if __has_include(<UniformTypeIdentifiers/UniformTypeIdentifiers.h>)
  if (@available(macOS 11.0, *)) {
    UTType *type = [UTType typeWithFilenameExtension:fileExtension];
    if ([type conformsToType:UTTypeImage]) {
      return YES;
    }
  }
#endif

  return NO;
}

static NSString *
GhostexGpuiTerminalDropInsertionText(NSArray<NSString *> *paths) {
  NSMutableArray<NSString *> *entries =
      [NSMutableArray arrayWithCapacity:paths.count];
  NSUInteger imageNumber = 1;

  for (NSString *path in paths) {
    if (GhostexGpuiTerminalIsImageFilePath(path)) {
      [entries addObject:[NSString stringWithFormat:@"[Image #%lu](%@)",
                                                    (unsigned long)imageNumber,
                                                    path]];
      imageNumber += 1;
    } else {
      [entries addObject:path];
    }
  }

  return [entries componentsJoinedByString:@" "];
}

@interface GhostexGpuiTerminalHostView : NSView <NSTextInputClient> {
  NSString *_markedText;
  NSRange _markedTextRange;
  NSRange _selectedTextRange;
  NSMutableArray<NSString *> *_keyTextAccumulator;
  NSNumber *_lastPerformKeyEventTimestamp;
}
@end

@implementation GhostexGpuiTerminalHostView

- (instancetype)initWithFrame:(NSRect)frame {
  self = [super initWithFrame:frame];
  if (self) {
    _markedText = @"";
    _markedTextRange = NSMakeRange(NSNotFound, 0);
    _selectedTextRange = NSMakeRange(0, 0);
  }
  return self;
}

- (BOOL)acceptsFirstResponder {
  return YES;
}

- (BOOL)canBecomeKeyView {
  return YES;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
  (void)event;
  return YES;
}

- (NSDragOperation)draggingEntered:(id<NSDraggingInfo>)sender {
  return GhostexGpuiTerminalDroppedPaths(sender.draggingPasteboard).count > 0
             ? NSDragOperationCopy
             : NSDragOperationNone;
}

- (NSDragOperation)draggingUpdated:(id<NSDraggingInfo>)sender {
  return GhostexGpuiTerminalDroppedPaths(sender.draggingPasteboard).count > 0
             ? NSDragOperationCopy
             : NSDragOperationNone;
}

- (BOOL)performDragOperation:(id<NSDraggingInfo>)sender {
  NSArray<NSString *> *paths =
      GhostexGpuiTerminalDroppedPaths(sender.draggingPasteboard);
  if (paths.count == 0) {
    return NO;
  }

  NSString *insertionText = GhostexGpuiTerminalDropInsertionText(paths);
  NSData *data = [insertionText dataUsingEncoding:NSUTF8StringEncoding];
  if (data.length == 0) {
    return NO;
  }

  NSWindow *window = self.window;
  if (window) {
    [window makeFirstResponder:self];
  }

  return GhostexGpuiTerminalInsertDroppedText((__bridge void *)self, data.bytes,
                                              (uintptr_t)data.length) != 0;
}

- (BOOL)sendKeyEvent:(NSEvent *)event
              action:(int)action
         includeText:(BOOL)includeText
           composing:(BOOL)composing
        consumedMods:(int)consumedMods
                text:(NSString *)text {
  const char *textValue =
      includeText ? GhostexGpuiTerminalKeyTextCString(text) : NULL;
  return GhostexGpuiTerminalHandleNativeKeyEvent(
             (__bridge void *)self, action,
             GhostexGpuiTerminalGhosttyMods(event.modifierFlags), consumedMods,
             (uint32_t)event.keyCode, textValue,
             GhostexGpuiTerminalUnshiftedCodepoint(event),
             composing ? 1 : 0) != 0;
}

- (BOOL)insertCommittedText:(NSString *)text {
  if (text.length == 0) {
    return NO;
  }

  NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
  if (data.length == 0) {
    return NO;
  }

  return GhostexGpuiTerminalInsertCommittedText(
             (__bridge void *)self, data.bytes, (uintptr_t)data.length) != 0;
}

- (BOOL)setPreeditText:(NSString *)text {
  if (text.length == 0) {
    return GhostexGpuiTerminalSetPreeditText((__bridge void *)self, NULL, 0) !=
           0;
  }

  NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
  if (data.length == 0) {
    return NO;
  }

  return GhostexGpuiTerminalSetPreeditText((__bridge void *)self, data.bytes,
                                           (uintptr_t)data.length) != 0;
}

- (void)syncPreeditClearIfNeeded:(BOOL)clearIfNeeded {
  if ([self hasMarkedText] && _markedText.length > 0) {
    [self setPreeditText:_markedText];
  } else if (clearIfNeeded) {
    [self setPreeditText:@""];
  }
}

- (NSRange)clampedMarkedTextRange:(NSRange)range {
  if (range.location == NSNotFound) {
    return NSMakeRange(0, 0);
  }

  NSUInteger length = _markedText.length;
  NSUInteger location = MIN(range.location, length);
  return NSMakeRange(location, MIN(range.length, length - location));
}

- (NSRange)intersectionOfRange:(NSRange)range
          withMarkedRangeFound:(BOOL *)found {
  if (range.location == NSNotFound || _markedTextRange.location == NSNotFound) {
    if (found) {
      *found = NO;
    }
    return NSMakeRange(NSNotFound, 0);
  }

  NSUInteger start = MAX(range.location, _markedTextRange.location);
  NSUInteger end = MIN(NSMaxRange(range), NSMaxRange(_markedTextRange));
  if (start > end) {
    if (found) {
      *found = NO;
    }
    return NSMakeRange(NSNotFound, 0);
  }

  if (found) {
    *found = YES;
  }
  return NSMakeRange(start, end - start);
}

- (void)keyDown:(NSEvent *)event {
  _lastPerformKeyEventTimestamp = nil;
  int mods = GhostexGpuiTerminalGhosttyMods(event.modifierFlags);
  int translatedMods = GhostexGpuiTerminalNativeViewKeyTranslationMods(
      (__bridge void *)self, mods);
  NSEventModifierFlags translationFlags =
      GhostexGpuiTerminalTranslatedModifierFlags(event.modifierFlags,
                                                 translatedMods);
  int consumedMods = GhostexGpuiTerminalConsumedTextInputMods(translationFlags);
  int action = event.isARepeat ? GhostexGpuiGhosttyActionRepeat
                               : GhostexGpuiGhosttyActionPress;

  if (GhostexGpuiTerminalShouldBypassTextInput(event, [self hasMarkedText])) {
    if ([self sendKeyEvent:event
                    action:action
               includeText:NO
                 composing:NO
              consumedMods:consumedMods
                      text:nil]) {
      return;
    }

    [super keyDown:event];
    return;
  }

  BOOL markedTextBefore = [self hasMarkedText];
  NSEvent *translationEvent =
      GhostexGpuiTerminalTranslatedTextInputEvent(event, translationFlags);
  _keyTextAccumulator = [NSMutableArray array];
  [self interpretKeyEvents:@[ translationEvent ]];
  NSArray<NSString *> *accumulatedText = [_keyTextAccumulator copy];
  _keyTextAccumulator = nil;
  [self syncPreeditClearIfNeeded:markedTextBefore];

  BOOL composing = [self hasMarkedText] || markedTextBefore;
  if (accumulatedText.count > 0) {
    for (NSString *text in accumulatedText) {
      if (GhostexGpuiTerminalShouldSuppressComposingControlInput(text,
                                                                 composing)) {
        continue;
      }
      if (markedTextBefore) {
        [self insertCommittedText:text];
      } else {
        [self sendKeyEvent:event
                    action:action
               includeText:YES
                 composing:NO
              consumedMods:consumedMods
                      text:text];
      }
    }
    return;
  }

  if (GhostexGpuiTerminalShouldSuppressComposingControlInput(event.characters,
                                                             composing)) {
    return;
  }

  NSString *text = GhostexGpuiTerminalCharactersForEvent(
      translationEvent, translationEvent.modifierFlags);
  [self sendKeyEvent:event
              action:action
         includeText:!composing
           composing:composing
        consumedMods:consumedMods
                text:text];
}

- (BOOL)performKeyEquivalent:(NSEvent *)event {
  /*
   CDXC:GPUITerminalNativeKeyBridge 2026-07-11:
   AppKit dispatches Command/Control key equivalents before keyDown and may
   turn standard text-navigation chords into responder commands instead. Ask
   the exact mounted libghostty surface whether the original native event is a
   binding; terminal bindings re-enter the normal keyDown/IME path, while
   non-bindings remain available to Ghostex menus and the responder chain.
   */
  if (event.type != NSEventTypeKeyDown || self.window.firstResponder != self) {
    return NO;
  }

  /*
   CDXC:GPUITerminalNativeTab 2026-07-13:
   An AppKit key window offers plain Tab to key-view traversal before the
   terminal host can receive keyDown. Since this exact mounted host is already
   first responder, claim plain Tab and Shift-Tab here and feed them through
   the same native Ghostty key path as every other terminal-owned key. Keep
   Option/Control/Command variants on the binding and menu paths below.
   */
  NSEventModifierFlags independentFlags =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  NSEventModifierFlags tabTraversalFlags =
      independentFlags &
      ~(NSEventModifierFlagShift | NSEventModifierFlagCapsLock);
  if (event.keyCode == kVK_Tab && tabTraversalFlags == 0) {
    [self keyDown:event];
    return YES;
  }

  NSString *characters = event.characters;
  const char *text = characters.length > 0 ? characters.UTF8String : NULL;
  int mods = GhostexGpuiTerminalGhosttyMods(event.modifierFlags);
  int consumedMods =
      GhostexGpuiTerminalConsumedTextInputMods(event.modifierFlags);
  if (GhostexGpuiTerminalNativeKeyEventIsBinding(
          (__bridge void *)self, GhostexGpuiGhosttyActionPress, mods,
          consumedMods, (uint32_t)event.keyCode, text,
          GhostexGpuiTerminalUnshiftedCodepoint(event), 0) != 0) {
    [self keyDown:event];
    return YES;
  }

  NSString *equivalent = nil;
  NSEventModifierFlags flags =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ([event.charactersIgnoringModifiers isEqualToString:@"\r"] &&
      (flags & NSEventModifierFlagControl) != 0) {
    equivalent = @"\r";
  } else if ([event.charactersIgnoringModifiers isEqualToString:@"/"] &&
             (flags & NSEventModifierFlagControl) != 0 &&
             (flags & (NSEventModifierFlagShift | NSEventModifierFlagCommand |
                       NSEventModifierFlagOption)) == 0) {
    equivalent = @"_";
  } else {
    if (event.timestamp == 0) {
      return NO;
    }
    if ((flags & (NSEventModifierFlagCommand | NSEventModifierFlagControl)) ==
        0) {
      _lastPerformKeyEventTimestamp = nil;
      return NO;
    }

    if (_lastPerformKeyEventTimestamp &&
        _lastPerformKeyEventTimestamp.doubleValue == event.timestamp) {
      _lastPerformKeyEventTimestamp = nil;
      equivalent = event.characters ?: @"";
    } else {
      _lastPerformKeyEventTimestamp = @(event.timestamp);
      return NO;
    }
  }

  NSEvent *finalEvent = [NSEvent keyEventWithType:NSEventTypeKeyDown
                                         location:event.locationInWindow
                                    modifierFlags:event.modifierFlags
                                        timestamp:event.timestamp
                                     windowNumber:event.windowNumber
                                          context:nil
                                       characters:equivalent
                      charactersIgnoringModifiers:equivalent
                                        isARepeat:event.isARepeat
                                          keyCode:event.keyCode];
  if (!finalEvent) {
    return NO;
  }

  [self keyDown:finalEvent];
  return YES;
}

- (void)keyUp:(NSEvent *)event {
  if (GhostexGpuiTerminalHandleNativeKeyEvent(
          (__bridge void *)self, GhostexGpuiGhosttyActionRelease,
          GhostexGpuiTerminalGhosttyMods(event.modifierFlags),
          GhostexGpuiTerminalGhosttyMods(
              event.modifierFlags &
              ~(NSEventModifierFlagControl | NSEventModifierFlagCommand)),
          (uint32_t)event.keyCode, NULL,
          GhostexGpuiTerminalUnshiftedCodepoint(event), 0) != 0) {
    return;
  }

  [super keyUp:event];
}

- (void)flagsChanged:(NSEvent *)event {
  if ([self hasMarkedText]) {
    return;
  }

  int mod = GhostexGpuiGhosttyModsNone;
  switch (event.keyCode) {
  case 0x39:
    mod = GhostexGpuiGhosttyModsCaps;
    break;
  case 0x38:
  case 0x3C:
    mod = GhostexGpuiGhosttyModsShift;
    break;
  case 0x3B:
  case 0x3E:
    mod = GhostexGpuiGhosttyModsCtrl;
    break;
  case 0x3A:
  case 0x3D:
    mod = GhostexGpuiGhosttyModsAlt;
    break;
  case 0x37:
  case 0x36:
    mod = GhostexGpuiGhosttyModsSuper;
    break;
  default:
    [super flagsChanged:event];
    return;
  }

  int mods = GhostexGpuiTerminalGhosttyMods(event.modifierFlags);
  int action = GhostexGpuiGhosttyActionRelease;
  if ((mods & mod) != 0) {
    bool sidePressed = true;
    switch (event.keyCode) {
    case 0x3C:
      sidePressed = (event.modifierFlags & NX_DEVICERSHIFTKEYMASK) != 0;
      break;
    case 0x3E:
      sidePressed = (event.modifierFlags & NX_DEVICERCTLKEYMASK) != 0;
      break;
    case 0x3D:
      sidePressed = (event.modifierFlags & NX_DEVICERALTKEYMASK) != 0;
      break;
    case 0x36:
      sidePressed = (event.modifierFlags & NX_DEVICERCMDKEYMASK) != 0;
      break;
    default:
      break;
    }
    if (sidePressed) {
      action = GhostexGpuiGhosttyActionPress;
    }
  }

  if (GhostexGpuiTerminalHandleNativeKeyEvent(
          (__bridge void *)self, action, mods,
          GhostexGpuiTerminalGhosttyMods(
              event.modifierFlags &
              ~(NSEventModifierFlagControl | NSEventModifierFlagCommand)),
          (uint32_t)event.keyCode, NULL, 0, 0) != 0) {
    return;
  }

  [super flagsChanged:event];
}

- (void)insertText:(id)string replacementRange:(NSRange)replacementRange {
  (void)replacementRange;
  NSString *text = GhostexGpuiTerminalTextInputString(string);
  [self unmarkText];
  if (text.length == 0) {
    return;
  }

  if (_keyTextAccumulator) {
    [_keyTextAccumulator addObject:text];
    return;
  }

  [self insertCommittedText:text];
}

- (void)setMarkedText:(id)string
        selectedRange:(NSRange)selectedRange
     replacementRange:(NSRange)replacementRange {
  (void)replacementRange;
  _markedText = GhostexGpuiTerminalTextInputString(string);
  _markedTextRange = _markedText.length == 0
                         ? NSMakeRange(NSNotFound, 0)
                         : NSMakeRange(0, _markedText.length);
  _selectedTextRange = [self clampedMarkedTextRange:selectedRange];
  if (!_keyTextAccumulator) {
    [self syncPreeditClearIfNeeded:YES];
  }
}

- (void)unmarkText {
  if (![self hasMarkedText]) {
    return;
  }

  _markedText = @"";
  _markedTextRange = NSMakeRange(NSNotFound, 0);
  _selectedTextRange = NSMakeRange(0, 0);
  [self syncPreeditClearIfNeeded:YES];
}

- (BOOL)hasMarkedText {
  return _markedTextRange.location != NSNotFound;
}

- (NSRange)markedRange {
  return _markedTextRange;
}

- (NSRange)selectedRange {
  return _selectedTextRange;
}

- (NSArray<NSAttributedStringKey> *)validAttributesForMarkedText {
  return @[];
}

- (NSAttributedString *)attributedSubstringForProposedRange:(NSRange)range
                                                actualRange:(NSRangePointer)
                                                                actualRange {
  if (![self hasMarkedText]) {
    if (actualRange) {
      *actualRange = NSMakeRange(0, 0);
    }
    return range.location == 0 && range.length == 0
               ? [[NSAttributedString alloc] initWithString:@""]
               : nil;
  }

  BOOL found = NO;
  NSRange safeRange = [self intersectionOfRange:range
                           withMarkedRangeFound:&found];
  if (!found) {
    return nil;
  }

  if (actualRange) {
    *actualRange = safeRange;
  }
  return [[NSAttributedString alloc]
      initWithString:[_markedText substringWithRange:safeRange]];
}

- (NSRect)firstRectForCharacterRange:(NSRange)range
                         actualRange:(NSRangePointer)actualRange {
  if (actualRange) {
    *actualRange = [self hasMarkedText] ? [self clampedMarkedTextRange:range]
                                        : NSMakeRange(0, 0);
  }

  double x = 0.0;
  double y = 0.0;
  double width = 0.0;
  double height = 0.0;
  if (GhostexGpuiTerminalGetImePoint((__bridge void *)self, &x, &y, &width,
                                     &height) != 0) {
    NSPoint viewPoint =
        NSMakePoint((CGFloat)x, NSHeight(self.bounds) - (CGFloat)y);
    NSPoint windowPoint = [self convertPoint:viewPoint toView:nil];
    NSPoint screenPoint = self.window
                              ? [self.window convertPointToScreen:windowPoint]
                              : windowPoint;
    return NSMakeRect(screenPoint.x, screenPoint.y - (CGFloat)height,
                      (CGFloat)width, (CGFloat)height);
  }

  NSRect localRect = NSMakeRect(NSMinX(self.bounds), NSMinY(self.bounds), 1, 1);
  NSRect windowRect = [self convertRect:localRect toView:nil];
  return self.window ? [self.window convertRectToScreen:windowRect]
                     : windowRect;
}

- (NSUInteger)characterIndexForPoint:(NSPoint)point {
  (void)point;
  return NSNotFound;
}

- (void)doCommandBySelector:(SEL)selector {
  (void)selector;
  if (_keyTextAccumulator) {
    return;
  }

  NSEvent *currentEvent = NSApp.currentEvent;
  if (_lastPerformKeyEventTimestamp && currentEvent &&
      _lastPerformKeyEventTimestamp.doubleValue == currentEvent.timestamp) {
    [NSApp sendEvent:currentEvent];
  }
}

@end

static NSRect GhostexGpuiTerminalFrameInParent(NSView *parent, double x,
                                               double y, double width,
                                               double height) {
  CGFloat nativeWidth = MAX((CGFloat)0.0, (CGFloat)width);
  CGFloat nativeHeight = MAX((CGFloat)0.0, (CGFloat)height);
  CGFloat nativeY = (CGFloat)y;
  if (parent && ![parent isFlipped]) {
    nativeY = NSHeight(parent.bounds) - (CGFloat)y - nativeHeight;
  }

  return NSMakeRect((CGFloat)x, nativeY, nativeWidth, nativeHeight);
}

void *GhostexGpuiTerminalCreateHostNativeView(void *parentView, double x,
                                              double y, double width,
                                              double height) {
  NSView *parent = (__bridge NSView *)parentView;
  if (!parent) {
    return NULL;
  }

  NSView *hostView = [[GhostexGpuiTerminalHostView alloc]
      initWithFrame:GhostexGpuiTerminalFrameInParent(parent, x, y, width,
                                                     height)];
  [hostView registerForDraggedTypes:GhostexGpuiTerminalRegisteredDragTypes()];
  hostView.hidden = YES;
  hostView.wantsLayer = YES;
  hostView.layer.backgroundColor = [NSColor blackColor].CGColor;
  [parent addSubview:hostView];

  return (__bridge_retained void *)hostView;
}

void GhostexGpuiTerminalDestroyHostNativeView(void *nativeView) {
  if (!nativeView) {
    return;
  }

  NSView *view = (__bridge_transfer NSView *)nativeView;
  [view removeFromSuperview];
}

void GhostexGpuiTerminalSetNativeViewFrame(void *nativeView, double x, double y,
                                           double width, double height) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  NSView *parent = [view superview];
  view.frame = GhostexGpuiTerminalFrameInParent(parent, x, y, width, height);
}

void GhostexGpuiTerminalShowNativeView(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  view.hidden = NO;
}

void GhostexGpuiTerminalHideNativeView(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  view.hidden = YES;
}

void GhostexGpuiTerminalFocusNativeView(void *nativeView) {
  NSView *view = (__bridge NSView *)nativeView;
  if (!view) {
    return;
  }

  NSWindow *window = [view window];
  if (!window) {
    return;
  }

  [window makeFirstResponder:view];
}
