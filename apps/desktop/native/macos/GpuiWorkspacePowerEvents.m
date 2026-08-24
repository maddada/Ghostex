#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>

extern void GhostexGpuiWorkspaceDidWake(void);

static BOOL GhostexGpuiWorkspacePowerEventsMonitorInstalled = NO;

static void GhostexGpuiRunWorkspacePowerEventsOnMain(dispatch_block_t block) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

@interface GhostexGpuiWorkspacePowerEventsObserver : NSObject
@end

@implementation GhostexGpuiWorkspacePowerEventsObserver

+ (instancetype)sharedObserver {
  static GhostexGpuiWorkspacePowerEventsObserver *observer = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    observer = [[GhostexGpuiWorkspacePowerEventsObserver alloc] init];
  });
  return observer;
}

- (void)workspaceDidWake:(NSNotification *)notification {
  (void)notification;
  GhostexGpuiWorkspaceDidWake();
}

@end

void GhostexGpuiInstallWorkspacePowerEventsMonitor(void) {
  @autoreleasepool {
    GhostexGpuiRunWorkspacePowerEventsOnMain(^{
      if (GhostexGpuiWorkspacePowerEventsMonitorInstalled) {
        return;
      }
      [[[NSWorkspace sharedWorkspace] notificationCenter]
          addObserver:[GhostexGpuiWorkspacePowerEventsObserver sharedObserver]
             selector:@selector(workspaceDidWake:)
                 name:NSWorkspaceDidWakeNotification
               object:nil];
      GhostexGpuiWorkspacePowerEventsMonitorInstalled = YES;
    });
  }
}

void GhostexGpuiRemoveWorkspacePowerEventsMonitor(void) {
  @autoreleasepool {
    GhostexGpuiRunWorkspacePowerEventsOnMain(^{
      if (!GhostexGpuiWorkspacePowerEventsMonitorInstalled) {
        return;
      }
      [[[NSWorkspace sharedWorkspace] notificationCenter]
          removeObserver:[GhostexGpuiWorkspacePowerEventsObserver
                             sharedObserver]
                    name:NSWorkspaceDidWakeNotification
                  object:nil];
      GhostexGpuiWorkspacePowerEventsMonitorInstalled = NO;
    });
  }
}
