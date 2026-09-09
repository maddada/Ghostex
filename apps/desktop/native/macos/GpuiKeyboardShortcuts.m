#import "GpuiKeyboardShortcuts.h"
#import <Carbon/Carbon.h>
#import <objc/runtime.h>

static const void *GhostexGpuiShortcutCharactersKey =
    &GhostexGpuiShortcutCharactersKey;

// Translates a physical key through a keyboard layout with the given Carbon
// modifier bits (cmdKey, shiftKey), exactly as AppKit derives event text.
static NSString *GhostexGpuiTranslateKeyCode(TISInputSourceRef source,
                                             unsigned short keyCode,
                                             UInt32 carbonModifiers) {
  if (!source) {
    return nil;
  }
  CFDataRef layoutData =
      TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
  if (!layoutData) {
    return nil;
  }
  const UCKeyboardLayout *layout =
      (const UCKeyboardLayout *)CFDataGetBytePtr(layoutData);
  UInt32 deadKeyState = 0;
  UniChar buffer[4];
  UniCharCount length = 0;
  OSStatus status = UCKeyTranslate(
      layout, keyCode, kUCKeyActionDown, (carbonModifiers >> 8) & 0xFF,
      LMGetKbdType(), kUCKeyTranslateNoDeadKeysMask, &deadKeyState,
      sizeof(buffer) / sizeof(buffer[0]), &length, buffer);
  if (status != noErr || length == 0) {
    return nil;
  }
  return [NSString stringWithCharacters:buffer length:length];
}

static BOOL GhostexGpuiIsPrintableASCII(NSString *characters) {
  if (characters.length != 1) {
    return NO;
  }
  unichar character = [characters characterAtIndex:0];
  return character > 0x20 && character < 0x7F;
}

static BOOL GhostexGpuiInputSourceIsASCIICapable(TISInputSourceRef source) {
  CFBooleanRef capable =
      source ? TISGetInputSourceProperty(source,
                                         kTISPropertyInputSourceIsASCIICapable)
             : NULL;
  return capable && CFBooleanGetValue(capable);
}

static NSString *GhostexGpuiComputeShortcutCharacters(NSEvent *event) {
  NSString *ignoringModifiers = event.charactersIgnoringModifiers ?: @"";
  NSEventModifierFlags flags =
      event.modifierFlags & NSEventModifierFlagDeviceIndependentFlagsMask;
  if ((flags & (NSEventModifierFlagCommand | NSEventModifierFlagControl |
                NSEventModifierFlagOption)) == 0) {
    return ignoringModifiers;
  }

  TISInputSourceRef layout = TISCopyCurrentKeyboardLayoutInputSource();
  BOOL asciiCapable = GhostexGpuiInputSourceIsASCIICapable(layout);
  if ((flags & NSEventModifierFlagCommand) == 0 && asciiCapable) {
    // Control and Option chords on a Latin layout mean the printed key; only
    // Command switches layouts such as "Dvorak - QWERTY ⌘" to their Command
    // layer, matching AppKit and GPUI.
    if (layout) {
      CFRelease(layout);
    }
    return ignoringModifiers;
  }

  UInt32 shift = (flags & NSEventModifierFlagShift) ? shiftKey : 0;
  NSString *commandLayer =
      GhostexGpuiTranslateKeyCode(layout, event.keyCode, cmdKey | shift);
  if (layout) {
    CFRelease(layout);
  }
  if (GhostexGpuiIsPrintableASCII(commandLayer)) {
    return commandLayer;
  }

  if (!asciiCapable) {
    TISInputSourceRef asciiLayout =
        TISCopyCurrentASCIICapableKeyboardLayoutInputSource();
    NSString *asciiLayer =
        GhostexGpuiTranslateKeyCode(asciiLayout, event.keyCode, shift);
    if (asciiLayout) {
      CFRelease(asciiLayout);
    }
    if (GhostexGpuiIsPrintableASCII(asciiLayer)) {
      return asciiLayer;
    }
  }
  return ignoringModifiers;
}

NSString *GhostexGpuiShortcutCharactersForEvent(NSEvent *event) {
  if (!event ||
      (event.type != NSEventTypeKeyDown && event.type != NSEventTypeKeyUp)) {
    return @"";
  }
  // Several matchers inspect the same event during one sendEvent: pass;
  // resolve the layout once per event instance.
  NSString *cached =
      objc_getAssociatedObject(event, GhostexGpuiShortcutCharactersKey);
  if (cached) {
    return cached;
  }
  NSString *characters = GhostexGpuiComputeShortcutCharacters(event);
  objc_setAssociatedObject(event, GhostexGpuiShortcutCharactersKey, characters,
                           OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  return characters;
}
