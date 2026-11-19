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
extern void GhostexGpuiSparkleUpdateVersionFound(const char *version);
extern void GhostexGpuiSparkleUnattendedUpdateEvent(int32_t kind,
                                                    const char *text);
/* go_back/keep.rs: keeps the running app so the user can go back to it. */
extern void GhostexGpuiKeepAppBeforeUpdate(void);

/*
 CDXC:RemoteMachines 2026-10-16 WHY:
 Another device can ask this computer to update (gxserver `/api/appUpdateInstall`,
 app_update_remote_control.rs). Nobody is in front of this screen, so while
 `gGhostexGpuiSparkleUnattended` is set the user driver gives Sparkle the
 answers the user would click (Install on the update-found alert and on Install
 and Relaunch) and skips the checking/release-notes windows; signature checks,
 download, extraction and the relaunch stay Sparkle's own. Every other session
 forwards to SPUStandardUserDriver unchanged.
*/
enum {
  GhostexGpuiSparkleUnattendedFound = 1,
  GhostexGpuiSparkleUnattendedNotFound = 2,
  GhostexGpuiSparkleUnattendedFailed = 3,
  GhostexGpuiSparkleUnattendedInstalling = 4,
  GhostexGpuiSparkleUnattendedEnded = 5,
};
/* SPUUserUpdateChoiceInstall in Sparkle 2's SPUUserUpdateChoice. */
static const NSInteger GhostexGpuiSparkleChoiceInstall = 1;
static BOOL gGhostexGpuiSparkleUnattended = NO;
static BOOL gGhostexGpuiSparkleUnattendedSupported = NO;
static Class gGhostexGpuiSparkleStandardDriverClass = Nil;

@interface NSObject (GhostexGpuiSparkleDynamicMessaging)
- (id)initWithHostBundle:(NSBundle *)hostBundle delegate:(id)delegate;
- (id)initWithHostBundle:(NSBundle *)hostBundle
       applicationBundle:(NSBundle *)applicationBundle
              userDriver:(id)userDriver
                delegate:(id)delegate;
- (BOOL)startUpdater:(NSError **)error;
- (void)checkForUpdates;
- (void)checkForUpdateInformation;
- (BOOL)sessionInProgress;
@end

static IMP GhostexGpuiSparkleSuperIMP(SEL selector) {
  if (gGhostexGpuiSparkleStandardDriverClass == Nil) {
    return NULL;
  }
  return class_getMethodImplementation(gGhostexGpuiSparkleStandardDriverClass,
                                       selector);
}

static NSString *GhostexGpuiSparkleItemVersion(id item) {
  @try {
    id version = [item valueForKey:@"displayVersionString"];
    if ([version isKindOfClass:[NSString class]]) {
      return version;
    }
  } @catch (NSException *exception) {
  }
  return nil;
}

static void GhostexGpuiSparkleEmitUnattended(int32_t kind, NSString *text) {
  GhostexGpuiSparkleUnattendedUpdateEvent(kind, text.UTF8String);
}

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

static void GhostexGpuiSparkleShowUserInitiatedCheck(id self, SEL _cmd,
                                                    id cancellation) {
  if (gGhostexGpuiSparkleUnattended) {
    return;
  }
  ((void (*)(id, SEL, id))GhostexGpuiSparkleSuperIMP(_cmd))(self, _cmd,
                                                           cancellation);
}

static void GhostexGpuiSparkleShowUpdateFound(id self, SEL _cmd, id item,
                                              id state, id reply) {
  NSString *version = GhostexGpuiSparkleItemVersion(item);
  if (version != nil) {
    GhostexGpuiSparkleUpdateVersionFound(version.UTF8String);
  }
  if (gGhostexGpuiSparkleUnattended) {
    GhostexGpuiSparkleEmitUnattended(GhostexGpuiSparkleUnattendedFound,
                                     version);
    ((void (^)(NSInteger))reply)(GhostexGpuiSparkleChoiceInstall);
    return;
  }
  ((void (*)(id, SEL, id, id, id))GhostexGpuiSparkleSuperIMP(_cmd))(
      self, _cmd, item, state, reply);
}

static void GhostexGpuiSparkleShowReleaseNotes(id self, SEL _cmd, id data) {
  if (gGhostexGpuiSparkleUnattended) {
    return;
  }
  ((void (*)(id, SEL, id))GhostexGpuiSparkleSuperIMP(_cmd))(self, _cmd, data);
}

static void GhostexGpuiSparkleShowReleaseNotesFailed(id self, SEL _cmd,
                                                     id error) {
  if (gGhostexGpuiSparkleUnattended) {
    return;
  }
  ((void (*)(id, SEL, id))GhostexGpuiSparkleSuperIMP(_cmd))(self, _cmd, error);
}

static void GhostexGpuiSparkleShowNotFound(id self, SEL _cmd, NSError *error,
                                           id acknowledgement) {
  if (gGhostexGpuiSparkleUnattended) {
    gGhostexGpuiSparkleUnattended = NO;
    GhostexGpuiSparkleEmitUnattended(GhostexGpuiSparkleUnattendedNotFound,
                                     nil);
    ((void (^)(void))acknowledgement)();
    return;
  }
  ((void (*)(id, SEL, id, id))GhostexGpuiSparkleSuperIMP(_cmd))(
      self, _cmd, error, acknowledgement);
}

static void GhostexGpuiSparkleShowUpdaterError(id self, SEL _cmd,
                                               NSError *error,
                                               id acknowledgement) {
  if (gGhostexGpuiSparkleUnattended) {
    gGhostexGpuiSparkleUnattended = NO;
    GhostexGpuiSparkleEmitUnattended(GhostexGpuiSparkleUnattendedFailed,
                                     error.localizedDescription);
    ((void (^)(void))acknowledgement)();
    return;
  }
  ((void (*)(id, SEL, id, id))GhostexGpuiSparkleSuperIMP(_cmd))(
      self, _cmd, error, acknowledgement);
}

static void GhostexGpuiSparkleShowReadyToInstall(id self, SEL _cmd,
                                                 id reply) {
  if (gGhostexGpuiSparkleUnattended) {
    GhostexGpuiSparkleEmitUnattended(GhostexGpuiSparkleUnattendedInstalling,
                                     nil);
    ((void (^)(NSInteger))reply)(GhostexGpuiSparkleChoiceInstall);
    return;
  }
  ((void (*)(id, SEL, id))GhostexGpuiSparkleSuperIMP(_cmd))(self, _cmd, reply);
}

static void GhostexGpuiSparkleDismissInstallation(id self, SEL _cmd) {
  if (gGhostexGpuiSparkleUnattended) {
    gGhostexGpuiSparkleUnattended = NO;
    GhostexGpuiSparkleEmitUnattended(GhostexGpuiSparkleUnattendedEnded, nil);
  }
  ((void (*)(id, SEL))GhostexGpuiSparkleSuperIMP(_cmd))(self, _cmd);
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
    // The unattended answers are all-or-nothing: without every one of them a
    // remote install could leave an alert waiting on a screen nobody sees,
    // so a Sparkle that lacks any refuses remote installs instead.
    BOOL unattended = YES;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(showUserInitiatedUpdateCheckWithCancellation:),
                     (IMP)GhostexGpuiSparkleShowUserInitiatedCheck) &&
                 unattended;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(showUpdateFoundWithAppcastItem:state:reply:),
                     (IMP)GhostexGpuiSparkleShowUpdateFound) &&
                 unattended;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(showUpdateReleaseNotesWithDownloadData:),
                     (IMP)GhostexGpuiSparkleShowReleaseNotes) &&
                 unattended;
    unattended =
        GhostexGpuiSparkleAddOverride(
            allocated, standardUserDriverClass,
            @selector(showUpdateReleaseNotesFailedToDownloadWithError:),
            (IMP)GhostexGpuiSparkleShowReleaseNotesFailed) &&
        unattended;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(showUpdateNotFoundWithError:acknowledgement:),
                     (IMP)GhostexGpuiSparkleShowNotFound) &&
                 unattended;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(showUpdaterError:acknowledgement:),
                     (IMP)GhostexGpuiSparkleShowUpdaterError) &&
                 unattended;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(showReadyToInstallAndRelaunch:),
                     (IMP)GhostexGpuiSparkleShowReadyToInstall) &&
                 unattended;
    unattended = GhostexGpuiSparkleAddOverride(
                     allocated, standardUserDriverClass,
                     @selector(dismissUpdateInstallation),
                     (IMP)GhostexGpuiSparkleDismissInstallation) &&
                 unattended;
    gGhostexGpuiSparkleStandardDriverClass = standardUserDriverClass;
    gGhostexGpuiSparkleUnattendedSupported = unattended;
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
  NSString *version = GhostexGpuiSparkleItemVersion(item);
  if (version != nil) {
    GhostexGpuiSparkleUpdateVersionFound(version.UTF8String);
  }
  GhostexGpuiSparkleUpdateAvailableChanged(1);
}

/*
 CDXC:Release 2026-11-17 WHY:
 Sparkle replaces this bundle once Ghostex quits; this is the last moment the running version is
 still on disk, so it is kept here for Go back (Q3729, apps/desktop/src/go_back/keep.rs). Called
 for an Install and Relaunch and for an update installed on quit alike.
*/
- (void)updater:(id)updater willInstallUpdate:(id)item {
  (void)updater;
  (void)item;
  GhostexGpuiKeepAppBeforeUpdate();
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

/*
 Sparkle's own scheduled checks: the app turns them off at start
 (os_integration/updater.rs `turn_off_sparkle_schedule`) and checks quietly
 itself as often as Settings > Updates > Checking for updates > Ghostex says,
 so Sparkle never holds a hidden update session of its own. Info.plist's
 SUEnableAutomaticChecks stays on so Sparkle never asks on second launch.
*/
void GhostexGpuiSparkleSetAutomaticChecks(int32_t enabled,
                                          double intervalSeconds) {
  @autoreleasepool {
    GhostexGpuiSparkleRunOnMain(^{
      id updater = gGhostexGpuiSparkleUpdater;
      if (updater == nil) {
        return;
      }
      @try {
        if (enabled && intervalSeconds > 0 &&
            [updater respondsToSelector:@selector(setUpdateCheckInterval:)]) {
          [updater setValue:@(intervalSeconds) forKey:@"updateCheckInterval"];
        }
        if ([updater
                respondsToSelector:@selector(setAutomaticallyChecksForUpdates:)]) {
          [updater setValue:@(enabled ? YES : NO)
                     forKey:@"automaticallyChecksForUpdates"];
        }
      } @catch (NSException *exception) {
        (void)exception;
      }
    });
  }
}

/*
 Starts Sparkle's user-initiated check in unattended mode: 1 started, 0 no
 updater, 2 a Sparkle session (an update window) is already open, -1 this
 Sparkle cannot be answered unattended.
*/
int32_t GhostexGpuiSparkleInstallUnattended(void) {
  @autoreleasepool {
    __block int32_t result = 0;
    GhostexGpuiSparkleRunOnMain(^{
      if (gGhostexGpuiSparkleUpdater == nil) {
        result = 0;
        return;
      }
      if (!gGhostexGpuiSparkleUnattendedSupported) {
        result = -1;
        return;
      }
      BOOL busy = NO;
      @try {
        busy = [gGhostexGpuiSparkleUpdater sessionInProgress];
      } @catch (NSException *exception) {
        busy = NO;
      }
      if (busy && !gGhostexGpuiSparkleUnattended) {
        result = 2;
        return;
      }
      gGhostexGpuiSparkleUnattended = YES;
      [gGhostexGpuiSparkleUpdater checkForUpdates];
      result = 1;
    });
    return result;
  }
}
