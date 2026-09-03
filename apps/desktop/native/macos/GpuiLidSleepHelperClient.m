#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <dispatch/dispatch.h>
#import <stdint.h>

@protocol GhostexLidSleepHelperProtocol
- (void)setLidSleepPreventionEnabled:(BOOL)enabled
                            ownerPID:(int32_t)ownerPID
                           withReply:(void (^)(BOOL ok, NSString *error))reply;
- (void)heartbeatWithOwnerPID:(int32_t)ownerPID
                    withReply:(void (^)(BOOL ok, NSString *error))reply;
- (void)statusWithReply:(void (^)(BOOL ok, BOOL enabled, NSString *error))reply;
@end

typedef NS_ENUM(int32_t, GhostexGpuiLidSleepHelperResult) {
  GhostexGpuiLidSleepHelperFailed = 0,
  GhostexGpuiLidSleepHelperOK = 1,
};

static NSString *GhostexGpuiLidSleepHelperLabel(void) {
  NSString *bundleIdentifier = NSBundle.mainBundle.bundleIdentifier;
  if (bundleIdentifier.length == 0) {
    /*
     CDXC:Release 2026-06-28-16:18:
     The fallback GPUI bundle id must match the packager's stable product
     identity because the privileged lid-sleep helper label is derived from this
     value when Bundle.main lacks metadata.
     */
    bundleIdentifier = @"com.madda.ghostex.gpui";
  }
  return [bundleIdentifier stringByAppendingString:@".LidSleepHelper"];
}

static NSString *GhostexGpuiLidSleepShellQuote(NSString *value) {
  return [NSString
      stringWithFormat:@"'%@'",
                       [value stringByReplacingOccurrencesOfString:@"'"
                                                        withString:@"'\\''"]];
}

static NSString *GhostexGpuiLidSleepAppleScriptString(NSString *value) {
  NSString *escaped = [value stringByReplacingOccurrencesOfString:@"\\"
                                                       withString:@"\\\\"];
  escaped = [escaped stringByReplacingOccurrencesOfString:@"\""
                                               withString:@"\\\""];
  return [NSString stringWithFormat:@"\"%@\"", escaped];
}

static NSString *GhostexGpuiLidSleepEscapePlist(NSString *value) {
  NSString *escaped = [value stringByReplacingOccurrencesOfString:@"&"
                                                       withString:@"&amp;"];
  escaped = [escaped stringByReplacingOccurrencesOfString:@"<"
                                               withString:@"&lt;"];
  escaped = [escaped stringByReplacingOccurrencesOfString:@">"
                                               withString:@"&gt;"];
  escaped = [escaped stringByReplacingOccurrencesOfString:@"\""
                                               withString:@"&quot;"];
  escaped = [escaped stringByReplacingOccurrencesOfString:@"'"
                                               withString:@"&apos;"];
  return escaped;
}

static NSString *
GhostexGpuiLidSleepDesignatedRequirementString(NSURL *appBundleURL) {
  SecStaticCodeRef staticCode = NULL;
  SecRequirementRef requirement = NULL;
  CFStringRef requirementText = NULL;
  NSString *result = nil;
  if (SecStaticCodeCreateWithPath((__bridge CFURLRef)appBundleURL, 0,
                                  &staticCode) != errSecSuccess ||
      staticCode == NULL) {
    goto cleanup;
  }
  if (SecCodeCopyDesignatedRequirement(staticCode, 0, &requirement) !=
          errSecSuccess ||
      requirement == NULL) {
    goto cleanup;
  }
  if (SecRequirementCopyString(requirement, 0, &requirementText) !=
          errSecSuccess ||
      requirementText == NULL) {
    goto cleanup;
  }
  result = CFBridgingRelease(requirementText);
  requirementText = NULL;

cleanup:
  if (requirementText != NULL) {
    CFRelease(requirementText);
  }
  if (requirement != NULL) {
    CFRelease(requirement);
  }
  if (staticCode != NULL) {
    CFRelease(staticCode);
  }
  return result;
}

static NSURL *GhostexGpuiLidSleepWriteInstallerScript(
    NSString *appBundlePath, NSString *appBundleIdentifier,
    NSString *appRequirement, NSString *helperSourcePath) {
  NSString *helperLabel = GhostexGpuiLidSleepHelperLabel();
  NSString *scriptName =
      [NSString stringWithFormat:@"ghostex-gpui-lid-sleep-helper-%@.sh",
                                 NSUUID.UUID.UUIDString];
  NSURL *scriptURL =
      [NSURL fileURLWithPath:[NSTemporaryDirectory()
                                 stringByAppendingPathComponent:scriptName]];
  NSString *helperDestination = [@"/Library/PrivilegedHelperTools"
      stringByAppendingPathComponent:helperLabel];
  NSString *configDestination =
      [helperDestination stringByAppendingString:@".config.plist"];
  NSString *plistDestination = [NSString
      stringWithFormat:@"/Library/LaunchDaemons/%@.plist", helperLabel];

  /*
   CDXC:KeepAwake 2026-06-26-00:09:
   GPUI closed-lid Keep Awake uses the same root-owned installer contract as the
   Swift app: install the staged helper, write a helper config with bundle
   id/path/designated requirement, then bootstrap the LaunchDaemon. Keep all raw
   paths inside the installer boundary and return only generic success/failure
   to Rust.
   */
  NSString *script = [NSString
      stringWithFormat:
          @"#!/bin/sh\n"
           "set -eu\n"
           "/usr/bin/install -o root -g wheel -m 755 %@ %@\n"
           "/bin/cat > %@ <<'EOF_CONFIG'\n"
           "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
           "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" "
           "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n"
           "<plist version=\"1.0\">\n"
           "<dict>\n"
           "  <key>AuthorizedClientBundleIdentifiers</key>\n"
           "  <array>\n"
           "    <string>%@</string>\n"
           "  </array>\n"
           "  <key>AuthorizedClientBundlePath</key>\n"
           "  <string>%@</string>\n"
           "  <key>AuthorizedClientRequirement</key>\n"
           "  <string>%@</string>\n"
           "</dict>\n"
           "</plist>\n"
           "EOF_CONFIG\n"
           "/usr/sbin/chown root:wheel %@\n"
           "/bin/chmod 644 %@\n"
           "/bin/cat > %@ <<'EOF_PLIST'\n"
           "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
           "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" "
           "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n"
           "<plist version=\"1.0\">\n"
           "<dict>\n"
           "  <key>Label</key>\n"
           "  <string>%@</string>\n"
           "  <key>MachServices</key>\n"
           "  <dict>\n"
           "    <key>%@</key>\n"
           "    <true/>\n"
           "  </dict>\n"
           "  <key>ProgramArguments</key>\n"
           "  <array>\n"
           "    <string>%@</string>\n"
           "  </array>\n"
           "  <key>RunAtLoad</key>\n"
           "  <true/>\n"
           "</dict>\n"
           "</plist>\n"
           "EOF_PLIST\n"
           "/usr/sbin/chown root:wheel %@\n"
           "/bin/chmod 644 %@\n"
           "/bin/launchctl bootout system %@ >/dev/null 2>&1 || true\n"
           "/bin/launchctl bootstrap system %@\n"
           "/bin/launchctl kickstart -k system/%@ >/dev/null 2>&1 || true\n",
          GhostexGpuiLidSleepShellQuote(helperSourcePath),
          GhostexGpuiLidSleepShellQuote(helperDestination),
          GhostexGpuiLidSleepShellQuote(configDestination),
          GhostexGpuiLidSleepEscapePlist(appBundleIdentifier),
          GhostexGpuiLidSleepEscapePlist(appBundlePath),
          GhostexGpuiLidSleepEscapePlist(appRequirement),
          GhostexGpuiLidSleepShellQuote(configDestination),
          GhostexGpuiLidSleepShellQuote(configDestination),
          GhostexGpuiLidSleepShellQuote(plistDestination),
          GhostexGpuiLidSleepEscapePlist(helperLabel),
          GhostexGpuiLidSleepEscapePlist(helperLabel),
          GhostexGpuiLidSleepEscapePlist(helperDestination),
          GhostexGpuiLidSleepShellQuote(plistDestination),
          GhostexGpuiLidSleepShellQuote(plistDestination),
          GhostexGpuiLidSleepShellQuote(plistDestination),
          GhostexGpuiLidSleepShellQuote(plistDestination),
          GhostexGpuiLidSleepShellQuote(helperLabel)];

  NSError *error = nil;
  if (![script writeToURL:scriptURL
               atomically:YES
                 encoding:NSUTF8StringEncoding
                    error:&error]) {
    return nil;
  }
  if (![NSFileManager.defaultManager setAttributes:@{
        NSFilePosixPermissions : @(0700)
      }
                                      ofItemAtPath:scriptURL.path
                                             error:&error]) {
    [NSFileManager.defaultManager removeItemAtURL:scriptURL error:nil];
    return nil;
  }
  return scriptURL;
}

static BOOL GhostexGpuiLidSleepInstallHelper(void) {
  NSURL *appBundleURL = NSBundle.mainBundle.bundleURL;
  NSString *appBundleIdentifier = NSBundle.mainBundle.bundleIdentifier;
  if (appBundleIdentifier.length == 0) {
    return NO;
  }
  NSString *appRequirement =
      GhostexGpuiLidSleepDesignatedRequirementString(appBundleURL);
  if (appRequirement.length == 0) {
    return NO;
  }
  NSURL *helperSourceURL = [[[appBundleURL
      URLByAppendingPathComponent:@"Contents/Library/LaunchServices"
                      isDirectory:YES]
      URLByAppendingPathComponent:GhostexGpuiLidSleepHelperLabel()
                      isDirectory:NO] standardizedURL];
  if (![NSFileManager.defaultManager
          isExecutableFileAtPath:helperSourceURL.path]) {
    return NO;
  }

  NSURL *scriptURL = GhostexGpuiLidSleepWriteInstallerScript(
      appBundleURL.path, appBundleIdentifier, appRequirement,
      helperSourceURL.path);
  if (!scriptURL) {
    return NO;
  }

  NSTask *task = [[NSTask alloc] init];
  task.executableURL = [NSURL fileURLWithPath:@"/usr/bin/osascript"];
  NSString *scriptCommand =
      [NSString stringWithFormat:@"/bin/sh %@",
                                 GhostexGpuiLidSleepShellQuote(scriptURL.path)];
  task.arguments = @[
    @"-e",
    [NSString
        stringWithFormat:@"do shell script %@ with administrator privileges",
                         GhostexGpuiLidSleepAppleScriptString(scriptCommand)]
  ];
  task.standardInput = NSFileHandle.fileHandleWithNullDevice;
  task.standardOutput = NSFileHandle.fileHandleWithNullDevice;
  task.standardError = NSFileHandle.fileHandleWithNullDevice;

  NSError *error = nil;
  BOOL launched = [task launchAndReturnError:&error];
  if (launched) {
    [task waitUntilExit];
  }
  [NSFileManager.defaultManager removeItemAtURL:scriptURL error:nil];
  return launched && task.terminationStatus == 0;
}

static NSXPCConnection *GhostexGpuiLidSleepConnection(void) {
  NSXPCConnection *connection = [[NSXPCConnection alloc]
      initWithMachServiceName:GhostexGpuiLidSleepHelperLabel()
                      options:NSXPCConnectionPrivileged];
  connection.remoteObjectInterface = [NSXPCInterface
      interfaceWithProtocol:@protocol(GhostexLidSleepHelperProtocol)];
  return connection;
}

static int32_t GhostexGpuiLidSleepCallSetEnabled(BOOL enabled) {
  NSXPCConnection *connection = GhostexGpuiLidSleepConnection();
  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  NSLock *lock = [[NSLock alloc] init];
  __block BOOL completed = NO;
  __block int32_t result = GhostexGpuiLidSleepHelperFailed;
  void (^finish)(BOOL) = ^(BOOL ok) {
    [lock lock];
    if (!completed) {
      completed = YES;
      result =
          ok ? GhostexGpuiLidSleepHelperOK : GhostexGpuiLidSleepHelperFailed;
      dispatch_semaphore_signal(semaphore);
    }
    [lock unlock];
  };
  connection.invalidationHandler = ^{
    finish(NO);
  };
  connection.interruptionHandler = ^{
    finish(NO);
  };
  [connection resume];
  id<GhostexLidSleepHelperProtocol> helper =
      [connection remoteObjectProxyWithErrorHandler:^(NSError *error) {
        (void)error;
        finish(NO);
        [connection invalidate];
      }];
  if (!helper) {
    [connection invalidate];
    return GhostexGpuiLidSleepHelperFailed;
  }
  [helper setLidSleepPreventionEnabled:enabled
                              ownerPID:(int32_t)NSProcessInfo.processInfo
                                           .processIdentifier
                             withReply:^(BOOL ok, NSString *error) {
                               (void)error;
                               finish(ok);
                               [connection invalidate];
                             }];
  dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 20 * NSEC_PER_SEC);
  if (dispatch_semaphore_wait(semaphore, timeout) != 0) {
    [connection invalidate];
    return GhostexGpuiLidSleepHelperFailed;
  }
  return result;
}

int32_t GhostexGpuiSetLidSleepPreventionEnabled(int32_t enabled,
                                                int32_t installIfNeeded) {
  /*
   CDXC:KeepAwake 2026-06-26-00:09:
   Only GPUI's first closed-lid enable may request administrator-approved helper
   installation. Heartbeat and disable paths use the already-installed XPC
   helper and must never invoke the installer or prompt for credentials.
   */
  @autoreleasepool {
    BOOL shouldEnable = enabled != 0;
    int32_t firstResult = GhostexGpuiLidSleepCallSetEnabled(shouldEnable);
    if (firstResult == GhostexGpuiLidSleepHelperOK || installIfNeeded == 0 ||
        !shouldEnable) {
      return firstResult;
    }
    if (!GhostexGpuiLidSleepInstallHelper()) {
      return GhostexGpuiLidSleepHelperFailed;
    }
    return GhostexGpuiLidSleepCallSetEnabled(YES);
  }
}

int32_t GhostexGpuiHeartbeatLidSleepPrevention(void) {
  @autoreleasepool {
    NSXPCConnection *connection = GhostexGpuiLidSleepConnection();
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    NSLock *lock = [[NSLock alloc] init];
    __block BOOL completed = NO;
    __block int32_t result = GhostexGpuiLidSleepHelperFailed;
    void (^finish)(BOOL) = ^(BOOL ok) {
      [lock lock];
      if (!completed) {
        completed = YES;
        result =
            ok ? GhostexGpuiLidSleepHelperOK : GhostexGpuiLidSleepHelperFailed;
        dispatch_semaphore_signal(semaphore);
      }
      [lock unlock];
    };
    connection.invalidationHandler = ^{
      finish(NO);
    };
    connection.interruptionHandler = ^{
      finish(NO);
    };
    [connection resume];
    id<GhostexLidSleepHelperProtocol> helper =
        [connection remoteObjectProxyWithErrorHandler:^(NSError *error) {
          (void)error;
          finish(NO);
          [connection invalidate];
        }];
    if (!helper) {
      [connection invalidate];
      return GhostexGpuiLidSleepHelperFailed;
    }
    [helper
        heartbeatWithOwnerPID:(int32_t)
                                  NSProcessInfo.processInfo.processIdentifier
                    withReply:^(BOOL ok, NSString *error) {
                      (void)error;
                      finish(ok);
                      [connection invalidate];
                    }];
    dispatch_time_t timeout =
        dispatch_time(DISPATCH_TIME_NOW, 20 * NSEC_PER_SEC);
    if (dispatch_semaphore_wait(semaphore, timeout) != 0) {
      [connection invalidate];
      return GhostexGpuiLidSleepHelperFailed;
    }
    return result;
  }
}
