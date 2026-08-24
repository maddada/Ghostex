#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <dispatch/dispatch.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <stdint.h>

/*
 Sparkle is resolved at runtime from the packaged app bundle instead of being a
 build-time link dependency so `cargo check`/`cargo build` succeed on machines
 without a Sparkle SDK. `build-macos-app.sh` stages Sparkle.framework into
 Contents/Frameworks; unpackaged cargo runs report "unavailable" (start
 result 0) and the app runs without an updater.

 The update UX contract mirrors the macOS host exactly
 (AppDelegate.swift + GhostexSparkleUserDriver.swift):
 - Keep Sparkle's standard release-notes / install / relaunch / error UI.
 - Suppress only the download and extraction status windows.
 - Publish download-active state and a normalized 0...1 progress ratio so the
   native titlebar can render the ring without archive sizes or byte counts.
 - Scheduled/probed availability surfaces as quiet titlebar chrome, never as
   an unprompted modal.
*/

extern void GhostexGpuiSparkleUpdateAvailableChanged(int32_t available);
extern void GhostexGpuiSparkleUpdateDownloadingChanged(int32_t downloading);
extern void GhostexGpuiSparkleUpdateDownloadProgressChanged(int32_t hasProgress,
                                                            double progress);

@interface NSObject (GhostexGpuiSparkleDynamicMessaging)
- (id)initWithHostBundle:(NSBundle *)hostBundle delegate:(id)delegate;
- (id)initWithHostBundle:(NSBundle *)hostBundle
       applicationBundle:(NSBundle *)applicationBundle
              userDriver:(id)userDriver
                delegate:(id)delegate;
- (BOOL)startUpdater:(NSError **)error;
- (void)checkForUpdates;
- (void)checkForUpdateInformation;
@end

static id gGhostexGpuiSparkleUpdater = nil;
static id gGhostexGpuiSparkleUserDriver = nil;
static id gGhostexGpuiSparkleDelegate = nil;
static int32_t gGhostexGpuiSparkleStartResult = 0;
static BOOL gGhostexGpuiSparkleStartAttempted = NO;

static uint64_t gGhostexGpuiSparkleDownloadExpectedLength = 0;
static uint64_t gGhostexGpuiSparkleDownloadReceivedLength = 0;

static void GhostexGpuiSparkleRunOnMain(dispatch_block_t block) {
  if ([NSThread isMainThread]) {
    block();
  } else {
    dispatch_sync(dispatch_get_main_queue(), block);
  }
}

static void GhostexGpuiSparkleResetDownloadProgress(void) {
  gGhostexGpuiSparkleDownloadExpectedLength = 0;
  gGhostexGpuiSparkleDownloadReceivedLength = 0;
}

static void GhostexGpuiSparkleEmitDownloadProgress(void) {
  if (gGhostexGpuiSparkleDownloadExpectedLength == 0) {
    GhostexGpuiSparkleUpdateDownloadProgressChanged(0, 0.0);
    return;
  }
  uint64_t received = gGhostexGpuiSparkleDownloadReceivedLength;
  if (received > gGhostexGpuiSparkleDownloadExpectedLength) {
    received = gGhostexGpuiSparkleDownloadExpectedLength;
  }
  double progress =
      (double)received / (double)gGhostexGpuiSparkleDownloadExpectedLength;
  if (progress < 0.0) {
    progress = 0.0;
  }
  if (progress > 1.0) {
    progress = 1.0;
  }
  GhostexGpuiSparkleUpdateDownloadProgressChanged(1, progress);
}

/*
 Runtime overrides for the SPUStandardUserDriver subclass. None of them call
 super: the whole point is that Sparkle's download/extraction status windows
 never appear (they would expose the archive size), matching
 GhostexSparkleUserDriver.swift.
*/
static void GhostexGpuiSparkleShowDownloadInitiated(id self, SEL _cmd,
                                                    id cancellation) {
  (void)self;
  (void)_cmd;
  (void)cancellation;
  GhostexGpuiSparkleResetDownloadProgress();
  GhostexGpuiSparkleUpdateDownloadingChanged(1);
  GhostexGpuiSparkleUpdateDownloadProgressChanged(0, 0.0);
}

static void GhostexGpuiSparkleShowDownloadDidReceiveExpectedContentLength(
    id self, SEL _cmd, uint64_t expectedContentLength) {
  (void)self;
  (void)_cmd;
  gGhostexGpuiSparkleDownloadExpectedLength = expectedContentLength;
  GhostexGpuiSparkleEmitDownloadProgress();
}

static void GhostexGpuiSparkleShowDownloadDidReceiveData(id self, SEL _cmd,
                                                         uint64_t length) {
  (void)self;
  (void)_cmd;
  uint64_t sum = gGhostexGpuiSparkleDownloadReceivedLength + length;
  if (sum < gGhostexGpuiSparkleDownloadReceivedLength) {
    sum = UINT64_MAX;
  }
  gGhostexGpuiSparkleDownloadReceivedLength = sum;
  GhostexGpuiSparkleEmitDownloadProgress();
}

static void GhostexGpuiSparkleShowDownloadDidStartExtractingUpdate(id self,
                                                                   SEL _cmd) {
  (void)self;
  (void)_cmd;
  GhostexGpuiSparkleUpdateDownloadingChanged(0);
  GhostexGpuiSparkleResetDownloadProgress();
  GhostexGpuiSparkleUpdateDownloadProgressChanged(0, 0.0);
}

static void GhostexGpuiSparkleShowExtractionReceivedProgress(id self, SEL _cmd,
                                                             double progress) {
  (void)self;
  (void)_cmd;
  (void)progress;
  GhostexGpuiSparkleUpdateDownloadingChanged(0);
  GhostexGpuiSparkleResetDownloadProgress();
  GhostexGpuiSparkleUpdateDownloadProgressChanged(0, 0.0);
}

static BOOL GhostexGpuiSparkleAddOverride(Class subclass, Class superclass,
                                          SEL selector, IMP imp) {
  Method superMethod = class_getInstanceMethod(superclass, selector);
  if (superMethod == NULL) {
    return NO;
  }
  return class_addMethod(subclass, selector, imp,
                         method_getTypeEncoding(superMethod));
}

static Class GhostexGpuiSparkleUserDriverClass(Class standardUserDriverClass) {
  static Class driverClass = Nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    Class allocated = objc_allocateClassPair(standardUserDriverClass,
                                             "GhostexGpuiSparkleUserDriver", 0);
    if (allocated == Nil) {
      return;
    }
    BOOL added = YES;
    added = GhostexGpuiSparkleAddOverride(
                allocated, standardUserDriverClass,
                @selector(showDownloadInitiatedWithCancellation:),
                (IMP)GhostexGpuiSparkleShowDownloadInitiated) &&
            added;
    added =
        GhostexGpuiSparkleAddOverride(
            allocated, standardUserDriverClass,
            @selector(showDownloadDidReceiveExpectedContentLength:),
            (IMP)
                GhostexGpuiSparkleShowDownloadDidReceiveExpectedContentLength) &&
        added;
    added = GhostexGpuiSparkleAddOverride(
                allocated, standardUserDriverClass,
                @selector(showDownloadDidReceiveDataOfLength:),
                (IMP)GhostexGpuiSparkleShowDownloadDidReceiveData) &&
            added;
    added = GhostexGpuiSparkleAddOverride(
                allocated, standardUserDriverClass,
                @selector(showDownloadDidStartExtractingUpdate),
                (IMP)GhostexGpuiSparkleShowDownloadDidStartExtractingUpdate) &&
            added;
    added = GhostexGpuiSparkleAddOverride(
                allocated, standardUserDriverClass,
                @selector(showExtractionReceivedProgress:),
                (IMP)GhostexGpuiSparkleShowExtractionReceivedProgress) &&
            added;
    if (!added) {
      // The bundled Sparkle no longer exposes the standard download
      // callbacks this compact driver suppresses. Refuse the subclass so the
      // caller reports a start error instead of shipping unknown update UI.
      objc_disposeClassPair(allocated);
      return;
    }
    objc_registerClassPair(allocated);
    driverClass = allocated;
  });
  return driverClass;
}

/*
 Delegate for both SPUUpdaterDelegate and SPUStandardUserDriverDelegate.
 Sparkle probes delegate capabilities with respondsToSelector:, so a plain
 NSObject subclass implementing the selectors is sufficient.
*/
@interface GhostexGpuiSparkleDelegate : NSObject
@end

@implementation GhostexGpuiSparkleDelegate

- (BOOL)supportsGentleScheduledUpdateReminders {
  return YES;
}

- (BOOL)standardUserDriverShouldHandleShowingScheduledUpdate:(id)update
                                         andInImmediateFocus:
                                             (BOOL)immediateFocus {
  // Scheduled availability surfaces as the quiet titlebar affordance, never
  // as Sparkle's own scheduled alert (AppDelegate.swift parity).
  (void)update;
  (void)immediateFocus;
  GhostexGpuiSparkleUpdateAvailableChanged(1);
  return NO;
}

- (void)standardUserDriverWillHandleShowingUpdate:(BOOL)handleShowingUpdate
                                        forUpdate:(id)update
                                            state:(id)state {
  (void)handleShowingUpdate;
  (void)update;
  BOOL userInitiated = NO;
  @try {
    userInitiated = [[state valueForKey:@"userInitiated"] boolValue];
  } @catch (NSException *exception) {
    userInitiated = NO;
  }
  if (!userInitiated) {
    GhostexGpuiSparkleUpdateAvailableChanged(1);
  }
}

- (void)standardUserDriverDidReceiveUserAttentionForUpdate:(id)update {
  // Clicking into the update dialog must not consume the titlebar affordance
  // while the installed build remains behind the appcast.
  (void)update;
  GhostexGpuiSparkleUpdateAvailableChanged(1);
}

- (void)standardUserDriverWillFinishUpdateSession {
  // Closing Sparkle's dialog is not proof the app is current; only a
  // confirmed no-update probe clears the affordance.
  GhostexGpuiSparkleUpdateDownloadingChanged(0);
}

- (void)updater:(id)updater didFindValidUpdate:(id)item {
  (void)updater;
  (void)item;
  GhostexGpuiSparkleUpdateAvailableChanged(1);
}

- (void)updaterDidNotFindUpdate:(id)updater {
  (void)updater;
  GhostexGpuiSparkleUpdateDownloadingChanged(0);
  GhostexGpuiSparkleUpdateAvailableChanged(0);
}

- (void)updater:(id)updater didAbortWithError:(NSError *)error {
  (void)updater;
  (void)error;
  GhostexGpuiSparkleUpdateDownloadingChanged(0);
}

@end

static int32_t GhostexGpuiSparkleUpdaterStartOnMain(void) {
  if (gGhostexGpuiSparkleStartAttempted) {
    return gGhostexGpuiSparkleStartResult;
  }
  gGhostexGpuiSparkleStartAttempted = YES;
  gGhostexGpuiSparkleStartResult = 0;

  NSURL *frameworksURL = [[NSBundle mainBundle] privateFrameworksURL];
  if (frameworksURL == nil) {
    return gGhostexGpuiSparkleStartResult;
  }
  NSURL *sparkleURL =
      [frameworksURL URLByAppendingPathComponent:@"Sparkle.framework"];
  NSBundle *sparkleBundle = [NSBundle bundleWithURL:sparkleURL];
  if (sparkleBundle == nil || ![sparkleBundle load]) {
    return gGhostexGpuiSparkleStartResult;
  }

  Class updaterClass = NSClassFromString(@"SPUUpdater");
  Class standardUserDriverClass = NSClassFromString(@"SPUStandardUserDriver");
  if (updaterClass == Nil || standardUserDriverClass == Nil) {
    gGhostexGpuiSparkleStartResult = -1;
    return gGhostexGpuiSparkleStartResult;
  }
  Class driverClass =
      GhostexGpuiSparkleUserDriverClass(standardUserDriverClass);
  if (driverClass == Nil) {
    gGhostexGpuiSparkleStartResult = -1;
    return gGhostexGpuiSparkleStartResult;
  }

  GhostexGpuiSparkleDelegate *delegate =
      [[GhostexGpuiSparkleDelegate alloc] init];
  id userDriver = [[driverClass alloc] initWithHostBundle:[NSBundle mainBundle]
                                                 delegate:delegate];
  if (userDriver == nil) {
    gGhostexGpuiSparkleStartResult = -1;
    return gGhostexGpuiSparkleStartResult;
  }
  id updater = [[updaterClass alloc] initWithHostBundle:[NSBundle mainBundle]
                                      applicationBundle:[NSBundle mainBundle]
                                             userDriver:userDriver
                                               delegate:delegate];
  if (updater == nil) {
    gGhostexGpuiSparkleStartResult = -1;
    return gGhostexGpuiSparkleStartResult;
  }
  NSError *error = nil;
  if (![updater startUpdater:&error]) {
    gGhostexGpuiSparkleStartResult = -1;
    return gGhostexGpuiSparkleStartResult;
  }

  gGhostexGpuiSparkleDelegate = delegate;
  gGhostexGpuiSparkleUserDriver = userDriver;
  gGhostexGpuiSparkleUpdater = updater;
  gGhostexGpuiSparkleStartResult = 1;
  return gGhostexGpuiSparkleStartResult;
}

int32_t GhostexGpuiSparkleUpdaterStart(void) {
  @autoreleasepool {
    __block int32_t result = 0;
    GhostexGpuiSparkleRunOnMain(^{
      result = GhostexGpuiSparkleUpdaterStartOnMain();
    });
    return result;
  }
}

void GhostexGpuiSparkleCheckForUpdates(void) {
  @autoreleasepool {
    GhostexGpuiSparkleRunOnMain(^{
      if (gGhostexGpuiSparkleUpdater == nil) {
        return;
      }
      [gGhostexGpuiSparkleUpdater checkForUpdates];
    });
  }
}

void GhostexGpuiSparkleProbeForUpdateInformation(void) {
  @autoreleasepool {
    GhostexGpuiSparkleRunOnMain(^{
      if (gGhostexGpuiSparkleUpdater == nil) {
        return;
      }
      // Informational probe: never offers, downloads, or shows UI on its own;
      // availability arrives through the updater delegate callbacks.
      [gGhostexGpuiSparkleUpdater checkForUpdateInformation];
    });
  }
}
