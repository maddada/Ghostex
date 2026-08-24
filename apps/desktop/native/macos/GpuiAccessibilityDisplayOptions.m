#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>
#import <stdint.h>

extern void
GhostexGpuiAccessibilityDisplayOptionsChanged(int32_t shouldReduceMotion);

static BOOL GhostexGpuiAccessibilityDisplayOptionsMonitorInstalled = NO;

static void
GhostexGpuiRunAccessibilityDisplayOptionsOnMain(dispatch_block_t block) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static int32_t GhostexGpuiAccessibilityDisplayShouldReduceMotionOnMain(void) {
  /*
   CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
   GPUI Pet Overlay Reduce Motion follows macOS accessibility display options
   from NSWorkspace at runtime. Return only a boolean-like primitive across FFI
   and do not persist or log settings payloads, paths, titles, commands, URLs,
   terminal content, tokens, or raw system preference data.
   */
  if (@available(macOS 10.12, *)) {
    return NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion
               ? 1
               : 0;
  }
  return -1;
}

@interface GhostexGpuiAccessibilityDisplayOptionsObserver : NSObject
@end

@implementation GhostexGpuiAccessibilityDisplayOptionsObserver

+ (instancetype)sharedObserver {
  static GhostexGpuiAccessibilityDisplayOptionsObserver *observer = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    observer = [[GhostexGpuiAccessibilityDisplayOptionsObserver alloc] init];
  });
  return observer;
}

- (void)displayOptionsChanged:(NSNotification *)notification {
  (void)notification;
  GhostexGpuiAccessibilityDisplayOptionsChanged(
      GhostexGpuiAccessibilityDisplayShouldReduceMotionOnMain());
}

@end

int32_t GhostexGpuiAccessibilityDisplayShouldReduceMotion(void) {
  @autoreleasepool {
    if ([NSThread isMainThread]) {
      return GhostexGpuiAccessibilityDisplayShouldReduceMotionOnMain();
    }
    __block int32_t result = -1;
    dispatch_sync(dispatch_get_main_queue(), ^{
      result = GhostexGpuiAccessibilityDisplayShouldReduceMotionOnMain();
    });
    return result;
  }
}

void GhostexGpuiInstallAccessibilityDisplayOptionsMonitor(void) {
  @autoreleasepool {
    GhostexGpuiRunAccessibilityDisplayOptionsOnMain(^{
      if (GhostexGpuiAccessibilityDisplayOptionsMonitorInstalled) {
        return;
      }
      /*
       CDXC:GPUIStatusPetOverlay 2026-06-26-07:31:
       Runtime Reduce Motion changes must notify Rust from the NSWorkspace
       accessibility display-options notification instead of using an animation
       polling loop. The callback carries only the current boolean state so the
       pet ticker can stop or restart without broad settings IPC or hidden UI.
       */
      [[[NSWorkspace sharedWorkspace] notificationCenter]
          addObserver:[GhostexGpuiAccessibilityDisplayOptionsObserver
                          sharedObserver]
             selector:@selector(displayOptionsChanged:)
                 name:
                     NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification
               object:nil];
      GhostexGpuiAccessibilityDisplayOptionsMonitorInstalled = YES;
    });
  }
}

void GhostexGpuiRemoveAccessibilityDisplayOptionsMonitor(void) {
  @autoreleasepool {
    GhostexGpuiRunAccessibilityDisplayOptionsOnMain(^{
      if (!GhostexGpuiAccessibilityDisplayOptionsMonitorInstalled) {
        return;
      }
      [[[NSWorkspace sharedWorkspace] notificationCenter]
          removeObserver:[GhostexGpuiAccessibilityDisplayOptionsObserver
                             sharedObserver]
                    name:
                        NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification
                  object:nil];
      GhostexGpuiAccessibilityDisplayOptionsMonitorInstalled = NO;
    });
  }
}
