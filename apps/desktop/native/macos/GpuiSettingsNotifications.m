#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <UserNotifications/UserNotifications.h>
#import <dispatch/dispatch.h>
#import <stddef.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

extern void
GhostexGpuiSessionAttentionNotificationClicked(const char *sessionId);

typedef NS_ENUM(int32_t, GhostexGpuiNotificationAuthorizationStatus) {
  GhostexGpuiNotificationAuthorizationUnsupported = -1,
  GhostexGpuiNotificationAuthorizationUnknown = 0,
  GhostexGpuiNotificationAuthorizationNotDetermined = 1,
  GhostexGpuiNotificationAuthorizationDenied = 2,
  GhostexGpuiNotificationAuthorizationAuthorized = 3,
  GhostexGpuiNotificationAuthorizationProvisional = 4,
};

typedef NS_ENUM(int32_t, GhostexGpuiNotificationDeliveryResult) {
  GhostexGpuiNotificationDeliveryUnsupported = -1,
  GhostexGpuiNotificationDeliveryUnknown = 0,
  GhostexGpuiNotificationDeliveryPermissionNotDetermined = 1,
  GhostexGpuiNotificationDeliveryPermissionDenied = 2,
  GhostexGpuiNotificationDeliverySent = 3,
  GhostexGpuiNotificationDeliveryFailed = 4,
};

static NSString *const GhostexGpuiRemoteSshPasswordKeychainService =
    @"com.madda.ghostex.remote-ssh-password";
static NSString *const GhostexGpuiRemoteGxserverTokenKeychainService =
    @"com.madda.ghostex.remote-gxserver-token";
static NSString *const GhostexGpuiSessionAttentionNotificationCategory =
    @"ghostex.gpui.session.attention";
static const NSTimeInterval
    GhostexGpuiSessionAttentionNotificationRemovalDelay = 12.0;
static const NSUInteger GhostexGpuiSessionAttentionIconDataUrlMaxLength =
    700000;
static const NSUInteger GhostexGpuiSessionAttentionIconRawDataMaxLength =
    512000;

static NSMutableDictionary *
GhostexGpuiRemoteSshPasswordKeychainQuery(NSString *remoteMachineId) {
  return [@{
    (__bridge id)kSecAttrAccount : remoteMachineId,
    (__bridge id)kSecAttrService : GhostexGpuiRemoteSshPasswordKeychainService,
    (__bridge id)kSecClass : (__bridge id)kSecClassGenericPassword,
  } mutableCopy];
}

int32_t GhostexGpuiSaveRemoteSshPassword(const char *remoteMachineId,
                                         const uint8_t *passwordBytes,
                                         size_t passwordLength) {
  /*
   CDXC:RemoteMachines 2026-06-24-13:36:
   GPUI Remote Machine password parity uses the same macOS Keychain
   service/account contract as Swift: service
   `com.madda.ghostex.remote-ssh-password`, account `remoteMachineId`, and
   generic-password data. The raw password crosses only this native boundary,
   never through shell arguments, persistent logs, settings JSON, stdout/stderr,
   URLs, paths, hostnames, usernames, or command text.

   CDXC:RemoteMachines 2026-06-24-13:36:
   Non-empty saves must match `RemoteGxserverClient.storeSshPasswordInKeychain`:
   delete the existing service/account generic-password item first, treat
   missing items as a clean pre-add state, then add a new item with
   `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` and `kSecValueData`. Do
   not use `SecItemUpdate` because the GPUI path must not diverge from Swift
   Keychain replacement semantics.
   */
  @autoreleasepool {
    if (remoteMachineId == NULL) {
      return 0;
    }
    NSString *account = [NSString stringWithUTF8String:remoteMachineId];
    if (account.length == 0) {
      return 0;
    }
    NSMutableDictionary *query =
        GhostexGpuiRemoteSshPasswordKeychainQuery(account);
    if (passwordLength == 0) {
      OSStatus status = SecItemDelete((__bridge CFDictionaryRef)query);
      return (status == errSecSuccess || status == errSecItemNotFound) ? 1 : 0;
    }
    if (passwordBytes == NULL) {
      return 0;
    }

    NSData *passwordData = [NSData dataWithBytes:passwordBytes
                                          length:passwordLength];
    OSStatus deleteStatus = SecItemDelete((__bridge CFDictionaryRef)query);
    if (deleteStatus != errSecSuccess && deleteStatus != errSecItemNotFound) {
      return 0;
    }

    NSMutableDictionary *addQuery = [query mutableCopy];
    addQuery[(__bridge id)kSecAttrAccessible] =
        (__bridge id)kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly;
    addQuery[(__bridge id)kSecValueData] = passwordData;

    OSStatus status = SecItemAdd((__bridge CFDictionaryRef)addQuery, NULL);
    return status == errSecSuccess ? 1 : 0;
  }
}

int32_t GhostexGpuiCopyRemoteSshPassword(const char *remoteMachineId,
                                         uint8_t *passwordBytes,
                                         size_t passwordCapacity,
                                         size_t *passwordLength) {
  /*
   CDXC:RemotePairing 2026-08-03:
   Read the SSH password through Security.framework in the signed Ghostex
   process that owns the Keychain item. The SSH askpass child must not shell
   out to `/usr/bin/security`: that executable has a different Keychain access
   identity and can fail or wait for consent even though Ghostex saved the
   credential successfully. Copy only into caller-owned transient memory; the
   Rust boundary clears it after the one-shot askpass exchange.
   */
  @autoreleasepool {
    if (remoteMachineId == NULL || passwordBytes == NULL ||
        passwordCapacity == 0 || passwordLength == NULL) {
      return 0;
    }
    *passwordLength = 0;
    NSString *account = [NSString stringWithUTF8String:remoteMachineId];
    if (account.length == 0) {
      return 0;
    }

    NSMutableDictionary *query =
        GhostexGpuiRemoteSshPasswordKeychainQuery(account);
    query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
    query[(__bridge id)kSecReturnData] = @YES;
    CFTypeRef result = NULL;
    OSStatus status =
        SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
    if (status != errSecSuccess || result == NULL) {
      if (result != NULL) {
        CFRelease(result);
      }
      return status == errSecItemNotFound ? -1 : 0;
    }

    NSData *passwordData = CFBridgingRelease(result);
    if (passwordData.length == 0 || passwordData.length > passwordCapacity) {
      return 0;
    }
    memcpy(passwordBytes, passwordData.bytes, passwordData.length);
    *passwordLength = passwordData.length;
    return 1;
  }
}

static NSMutableDictionary *
GhostexGpuiRemoteGxserverTokenKeychainQuery(NSString *remoteMachineId) {
  return [@{
    (__bridge id)kSecAttrAccount : remoteMachineId,
    (__bridge id)
    kSecAttrService : GhostexGpuiRemoteGxserverTokenKeychainService,
    (__bridge id)kSecClass : (__bridge id)kSecClassGenericPassword,
  } mutableCopy];
}

int32_t GhostexGpuiSaveRemoteGxserverToken(const char *remoteMachineId,
                                           const uint8_t *tokenBytes,
                                           size_t tokenLength) {
  /*
   CDXC:RemoteMachines 2026-06-24-14:34:
   GPUI Remote gxserver reconnect stores the daemon token in the same macOS
   Keychain service/account contract as Swift: service
   `com.madda.ghostex.remote-gxserver-token`, account `remoteMachineId`,
   generic-password data. The token may live only in Keychain and transient
   runtime memory, never Settings JSON, persistent logs, app-modal payloads
   beyond connect status, stdout/stderr, URLs, paths, hostnames, usernames, or
   command text.

   CDXC:RemoteMachines 2026-07-21-03:20:
   Replace an existing token in place so Keychain preserves the item's access
   control owner. Local GPUI builds moved from `/Applications/GhostexGPUI.app`
   to `/Applications/Ghostex.app`; deleting an item created at the former path
   fails with errSecInvalidOwnerEdit before a replacement can be added. Add a
   new item only when this service/account does not exist yet.
   */
  @autoreleasepool {
    if (remoteMachineId == NULL) {
      return 0;
    }
    NSString *account = [NSString stringWithUTF8String:remoteMachineId];
    if (account.length == 0) {
      return 0;
    }
    NSMutableDictionary *query =
        GhostexGpuiRemoteGxserverTokenKeychainQuery(account);
    if (tokenLength == 0) {
      OSStatus status = SecItemDelete((__bridge CFDictionaryRef)query);
      return (status == errSecSuccess || status == errSecItemNotFound) ? 1 : 0;
    }
    if (tokenBytes == NULL) {
      return 0;
    }

    NSData *tokenData = [NSData dataWithBytes:tokenBytes length:tokenLength];
    NSDictionary *updateAttributes = @{
      (__bridge id)kSecValueData : tokenData,
    };
    OSStatus updateStatus =
        SecItemUpdate((__bridge CFDictionaryRef)query,
                      (__bridge CFDictionaryRef)updateAttributes);
    if (updateStatus == errSecSuccess) {
      return 1;
    }
    if (updateStatus != errSecItemNotFound) {
      return 0;
    }

    NSMutableDictionary *addQuery = [query mutableCopy];
    addQuery[(__bridge id)kSecAttrAccessible] =
        (__bridge id)kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly;
    addQuery[(__bridge id)kSecValueData] = tokenData;

    OSStatus status = SecItemAdd((__bridge CFDictionaryRef)addQuery, NULL);
    return status == errSecSuccess ? 1 : 0;
  }
}

@interface GhostexGpuiSettingsNotificationDelegate
    : NSObject <UNUserNotificationCenterDelegate>
@end

@implementation GhostexGpuiSettingsNotificationDelegate

+ (instancetype)sharedDelegate {
  static GhostexGpuiSettingsNotificationDelegate *delegate = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    delegate = [[GhostexGpuiSettingsNotificationDelegate alloc] init];
  });
  return delegate;
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       willPresentNotification:(UNNotification *)notification
         withCompletionHandler:
             (void (^)(UNNotificationPresentationOptions options))
                 completionHandler {
  (void)center;
  (void)notification;
  if (@available(macOS 11.0, *)) {
    completionHandler(UNNotificationPresentationOptionBanner);
  } else {
    completionHandler(UNNotificationPresentationOptionAlert);
  }
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
    didReceiveNotificationResponse:(UNNotificationResponse *)response
             withCompletionHandler:(void (^)(void))completionHandler {
  /*
   CDXC:Notifications 2026-06-26-06:56:
   GPUI session attention banner clicks pass only the notification-owned session
   id back to Rust. Copy the C string for the synchronous callback and do not
   persist or log notification titles, bodies, project names, paths, URLs,
   command text, terminal content, tokens, raw payloads, or settings JSON.
   */
  (void)center;
  UNNotificationContent *content = response.notification.request.content;
  if ([content.categoryIdentifier
          isEqualToString:GhostexGpuiSessionAttentionNotificationCategory]) {
    id sessionIdValue = content.userInfo[@"sessionId"];
    if ([sessionIdValue isKindOfClass:[NSString class]]) {
      const char *sessionIdUtf8 = [(NSString *)sessionIdValue UTF8String];
      if (sessionIdUtf8 != NULL) {
        char *sessionIdCopy = strdup(sessionIdUtf8);
        if (sessionIdCopy != NULL) {
          GhostexGpuiSessionAttentionNotificationClicked(sessionIdCopy);
          free(sessionIdCopy);
        }
      }
    }
  }
  completionHandler();
}

@end

static BOOL GhostexGpuiNotificationsAvailable(void) {
  if (@available(macOS 10.14, *)) {
    return YES;
  }
  return NO;
}

static UNUserNotificationCenter *GhostexGpuiNotificationCenter(void) {
  if (!GhostexGpuiNotificationsAvailable()) {
    return nil;
  }
  UNUserNotificationCenter *center =
      [UNUserNotificationCenter currentNotificationCenter];
  center.delegate = [GhostexGpuiSettingsNotificationDelegate sharedDelegate];
  return center;
}

static NSString *GhostexGpuiNotificationStringFromCString(const char *value) {
  if (value == NULL) {
    return @"";
  }
  NSString *text = [NSString stringWithUTF8String:value];
  return text ?: @"";
}

static NSString *GhostexGpuiTrimmedNotificationString(NSString *value,
                                                      NSString *fallback) {
  NSString *trimmed = [value
      stringByTrimmingCharactersInSet:[NSCharacterSet
                                          whitespaceAndNewlineCharacterSet]];
  return trimmed.length > 0 ? trimmed : fallback;
}

static BOOL GhostexGpuiNotificationStringHasControlCharacters(NSString *value) {
  for (NSUInteger index = 0; index < value.length; index += 1) {
    unichar character = [value characterAtIndex:index];
    if (character < 0x20 || character == 0x7f) {
      return YES;
    }
  }
  return NO;
}

static NSData *GhostexGpuiPngDataForSessionAttentionIcon(NSImage *image) {
  NSSize targetSize = NSMakeSize(128.0, 128.0);
  NSSize sourceSize = image.size.width > 0.0 && image.size.height > 0.0
                          ? image.size
                          : targetSize;
  CGFloat scale = MIN(targetSize.width / sourceSize.width,
                      targetSize.height / sourceSize.height);
  NSSize drawSize =
      NSMakeSize(sourceSize.width * scale, sourceSize.height * scale);
  NSRect drawRect = NSMakeRect((targetSize.width - drawSize.width) / 2.0,
                               (targetSize.height - drawSize.height) / 2.0,
                               drawSize.width, drawSize.height);
  NSImage *output = [[NSImage alloc] initWithSize:targetSize];
  [output lockFocus];
  [[NSColor clearColor] setFill];
  NSRectFill(NSMakeRect(0.0, 0.0, targetSize.width, targetSize.height));
  [image drawInRect:drawRect
           fromRect:NSZeroRect
          operation:NSCompositingOperationSourceOver
           fraction:1.0];
  [output unlockFocus];
  NSData *tiffData = [output TIFFRepresentation];
  if (!tiffData) {
    return nil;
  }
  NSBitmapImageRep *bitmap = [NSBitmapImageRep imageRepWithData:tiffData];
  return [bitmap representationUsingType:NSBitmapImageFileTypePNG
                              properties:@{}];
}

static NSURL *
GhostexGpuiWriteSessionAttentionIconAttachment(NSString *iconDataUrl,
                                               NSString *identifier) {
  NSString *trimmed = GhostexGpuiTrimmedNotificationString(iconDataUrl, @"");
  if (trimmed.length == 0 ||
      trimmed.length > GhostexGpuiSessionAttentionIconDataUrlMaxLength ||
      GhostexGpuiNotificationStringHasControlCharacters(trimmed)) {
    return nil;
  }

  NSRange commaRange = [trimmed rangeOfString:@","];
  if (commaRange.location == NSNotFound) {
    return nil;
  }
  NSString *header =
      [[trimmed substringToIndex:commaRange.location] lowercaseString];
  if (![header hasPrefix:@"data:image/"] ||
      [header rangeOfString:@";base64"].location == NSNotFound) {
    return nil;
  }
  NSString *payload =
      [trimmed substringFromIndex:commaRange.location + commaRange.length];
  if (payload.length == 0) {
    return nil;
  }

  NSData *rawData = [[NSData alloc] initWithBase64EncodedString:payload
                                                        options:0];
  if (!rawData ||
      rawData.length > GhostexGpuiSessionAttentionIconRawDataMaxLength) {
    return nil;
  }
  NSImage *image = [[NSImage alloc] initWithData:rawData];
  NSData *pngData =
      image ? GhostexGpuiPngDataForSessionAttentionIcon(image) : nil;
  if (!pngData) {
    return nil;
  }

  NSURL *directory = [[[NSFileManager defaultManager] temporaryDirectory]
      URLByAppendingPathComponent:@"ghostex-gpui-notification-icons"
                      isDirectory:YES];
  NSError *createError = nil;
  if (![[NSFileManager defaultManager] createDirectoryAtURL:directory
                                withIntermediateDirectories:YES
                                                 attributes:nil
                                                      error:&createError]) {
    return nil;
  }
  (void)createError;

  NSString *fileName = [[identifier stringByReplacingOccurrencesOfString:@"/"
                                                              withString:@"_"]
      stringByAppendingString:@".png"];
  NSURL *fileURL = [directory URLByAppendingPathComponent:fileName
                                              isDirectory:NO];
  NSError *writeError = nil;
  if (![pngData writeToURL:fileURL
                   options:NSDataWritingAtomic
                     error:&writeError]) {
    return nil;
  }
  (void)writeError;
  return fileURL;
}

static NSURL *GhostexGpuiApplySessionAttentionIconAttachment(
    UNMutableNotificationContent *content, const char *iconDataUrl,
    NSString *identifier) {
  /*
   CDXC:Notifications 2026-06-26-07:22:
   Match Swift session-attention icon parity for GPUI: only a bounded data:image
   base64 URL may become a temporary 128x128 PNG notification attachment.
   Attachment failures still deliver the banner without fabricating a fallback
   icon, and the temp file is removed with the delivered-notification cleanup.
   */
  NSURL *attachmentURL = GhostexGpuiWriteSessionAttentionIconAttachment(
      GhostexGpuiNotificationStringFromCString(iconDataUrl), identifier);
  if (!attachmentURL) {
    return nil;
  }
  NSError *attachmentError = nil;
  UNNotificationAttachment *attachment = [UNNotificationAttachment
      attachmentWithIdentifier:@"projectIcon"
                           URL:attachmentURL
                       options:@{
                         UNNotificationAttachmentOptionsTypeHintKey :
                             @"public.png"
                       }
                         error:&attachmentError];
  if (!attachment) {
    [[NSFileManager defaultManager] removeItemAtURL:attachmentURL error:nil];
    return nil;
  }
  (void)attachmentError;
  content.attachments = @[ attachment ];
  return attachmentURL;
}

static void GhostexGpuiRemoveDeliveredSessionAttentionNotificationLater(
    NSString *identifier, NSURL *attachmentURL) {
  /*
   CDXC:Notifications 2026-06-26-06:56:
   Session attention banners should be temporary like the Swift host: after
   successful delivery, remove the delivered notification and any temp
   project-icon attachment after 12 seconds so ignored or swiped banners do not
   accumulate in Notification Center. The cleanup uses only the request
   identifier and GPUI-owned temp attachment URL; it does not log or inspect
   title, body, project names, user paths, external URLs, command text,
   stdout/stderr, tokens, raw payloads, or settings content.
   */
  NSString *identifierCopy = [identifier copy];
  dispatch_after(
      dispatch_time(
          DISPATCH_TIME_NOW,
          (int64_t)(GhostexGpuiSessionAttentionNotificationRemovalDelay *
                    NSEC_PER_SEC)),
      dispatch_get_main_queue(), ^{
        [[UNUserNotificationCenter currentNotificationCenter]
            removeDeliveredNotificationsWithIdentifiers:@[ identifierCopy ]];
        if (attachmentURL) {
          [[NSFileManager defaultManager] removeItemAtURL:attachmentURL
                                                    error:nil];
        }
      });
}

static GhostexGpuiNotificationAuthorizationStatus
GhostexGpuiNotificationAuthorizationStatusFromSettings(
    UNNotificationSettings *settings) {
  if (!settings) {
    return GhostexGpuiNotificationAuthorizationUnknown;
  }

  switch (settings.authorizationStatus) {
  case UNAuthorizationStatusNotDetermined:
    return GhostexGpuiNotificationAuthorizationNotDetermined;
  case UNAuthorizationStatusDenied:
    return GhostexGpuiNotificationAuthorizationDenied;
  case UNAuthorizationStatusAuthorized:
    return GhostexGpuiNotificationAuthorizationAuthorized;
  case UNAuthorizationStatusProvisional:
    return GhostexGpuiNotificationAuthorizationProvisional;
  default:
    return GhostexGpuiNotificationAuthorizationUnknown;
  }
}

int32_t GhostexGpuiGetNotificationAuthorizationStatus(void) {
  /*
   CDXC:Notifications 2026-06-24-12:44:
   GPUI Settings reads macOS notification authorization through
   UserNotifications instead of reporting a stubbed unavailable state. Keep this
   shim status-only and privacy-neutral: no persistent logs, no raw errors, and
   no project/session/path/title content crosses the boundary.
   */
  UNUserNotificationCenter *center = GhostexGpuiNotificationCenter();
  if (!center) {
    return GhostexGpuiNotificationAuthorizationUnsupported;
  }

  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  __block GhostexGpuiNotificationAuthorizationStatus result =
      GhostexGpuiNotificationAuthorizationUnknown;
  [center getNotificationSettingsWithCompletionHandler:^(
              UNNotificationSettings *settings) {
    result = GhostexGpuiNotificationAuthorizationStatusFromSettings(settings);
    dispatch_semaphore_signal(semaphore);
  }];

  dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC);
  if (dispatch_semaphore_wait(semaphore, timeout) != 0) {
    return GhostexGpuiNotificationAuthorizationUnknown;
  }
  return result;
}

int32_t GhostexGpuiRequestNotificationAuthorization(void) {
  /*
   CDXC:Notifications 2026-06-24-12:44:
   The Settings permission button may request only alert authorization and only
   when macOS reports notDetermined. Denied permission remains a system-settings
   repair flow; GPUI must not fake success or attempt to override Notification
   Settings.
   */
  UNUserNotificationCenter *center = GhostexGpuiNotificationCenter();
  if (!center) {
    return GhostexGpuiNotificationAuthorizationUnsupported;
  }

  int32_t currentStatus = GhostexGpuiGetNotificationAuthorizationStatus();
  if (currentStatus != GhostexGpuiNotificationAuthorizationNotDetermined) {
    return currentStatus;
  }

  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  __block BOOL callbackReceived = NO;
  [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert
                        completionHandler:^(BOOL granted, NSError *error) {
                          (void)granted;
                          (void)error;
                          callbackReceived = YES;
                          dispatch_semaphore_signal(semaphore);
                        }];

  dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);
  if (!callbackReceived) {
    return GhostexGpuiNotificationAuthorizationUnknown;
  }
  return GhostexGpuiGetNotificationAuthorizationStatus();
}

int32_t GhostexGpuiDeliverSettingsTestNotification(void) {
  /*
   CDXC:Notifications 2026-06-24-12:44:
   Test agent task completion should emit exactly one generic macOS banner with
   no notification sound when Settings enables macOS attention notifications.
   This is not session notification routing: do not attach project icons,
   session ids, terminal text, command content, URLs, paths, or click-to-focus
   state.
   */
  UNUserNotificationCenter *center = GhostexGpuiNotificationCenter();
  if (!center) {
    return GhostexGpuiNotificationDeliveryUnsupported;
  }

  int32_t status = GhostexGpuiRequestNotificationAuthorization();
  switch (status) {
  case GhostexGpuiNotificationAuthorizationAuthorized:
  case GhostexGpuiNotificationAuthorizationProvisional:
    break;
  case GhostexGpuiNotificationAuthorizationNotDetermined:
    return GhostexGpuiNotificationDeliveryPermissionNotDetermined;
  case GhostexGpuiNotificationAuthorizationDenied:
    return GhostexGpuiNotificationDeliveryPermissionDenied;
  case GhostexGpuiNotificationAuthorizationUnsupported:
    return GhostexGpuiNotificationDeliveryUnsupported;
  default:
    return GhostexGpuiNotificationDeliveryUnknown;
  }

  UNMutableNotificationContent *content =
      [[UNMutableNotificationContent alloc] init];
  content.title = @"Agent task complete";
  content.body = @"This is a Ghostex notification test.";
  content.categoryIdentifier = @"ghostex.gpui.settings.test";
  content.threadIdentifier = @"ghostex.gpui.settings.test";
  content.sound = nil;

  NSString *identifier =
      [NSString stringWithFormat:@"ghostex.gpui.settings.test.%@",
                                 [NSUUID UUID].UUIDString];
  UNNotificationRequest *request =
      [UNNotificationRequest requestWithIdentifier:identifier
                                           content:content
                                           trigger:nil];

  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  __block BOOL delivered = NO;
  [center addNotificationRequest:request
           withCompletionHandler:^(NSError *error) {
             delivered = error == nil;
             dispatch_semaphore_signal(semaphore);
           }];

  dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC);
  if (dispatch_semaphore_wait(semaphore, timeout) != 0) {
    return GhostexGpuiNotificationDeliveryFailed;
  }

  return delivered ? GhostexGpuiNotificationDeliverySent
                   : GhostexGpuiNotificationDeliveryFailed;
}

int32_t GhostexGpuiDeliverSessionAttentionNotification(
    const char *sessionId, const char *title, const char *body,
    const char *iconDataUrl) {
  /*
   CDXC:Notifications 2026-06-26-06:56:
   Real GPUI session attention notifications use UserNotifications directly from
   the sanitized Rust status model. Content is limited to bounded session title
   and project-title-or-Ghostex body, sound remains nil, and the click payload
   is only the bounded session id used by the existing sidebar focus route.

   CDXC:Notifications 2026-06-26-07:22:
   The optional project icon is part of the same sanitized attention candidate
   and may only reach this delivery function as a nullable bounded data:image
   URL. It is converted to a temp PNG attachment by the GPUI-owned helper;
   failed or timed-out delivery removes the temp file instead of leaving
   Notification Center cleanup responsible for an undelivered request.
   */
  UNUserNotificationCenter *center = GhostexGpuiNotificationCenter();
  if (!center) {
    return GhostexGpuiNotificationDeliveryUnsupported;
  }

  int32_t status = GhostexGpuiRequestNotificationAuthorization();
  switch (status) {
  case GhostexGpuiNotificationAuthorizationAuthorized:
  case GhostexGpuiNotificationAuthorizationProvisional:
    break;
  case GhostexGpuiNotificationAuthorizationNotDetermined:
    return GhostexGpuiNotificationDeliveryPermissionNotDetermined;
  case GhostexGpuiNotificationAuthorizationDenied:
    return GhostexGpuiNotificationDeliveryPermissionDenied;
  case GhostexGpuiNotificationAuthorizationUnsupported:
    return GhostexGpuiNotificationDeliveryUnsupported;
  default:
    return GhostexGpuiNotificationDeliveryUnknown;
  }

  NSString *notificationSessionId = GhostexGpuiTrimmedNotificationString(
      GhostexGpuiNotificationStringFromCString(sessionId), @"");
  if (notificationSessionId.length == 0) {
    return GhostexGpuiNotificationDeliveryFailed;
  }

  UNMutableNotificationContent *content =
      [[UNMutableNotificationContent alloc] init];
  content.title = GhostexGpuiTrimmedNotificationString(
      GhostexGpuiNotificationStringFromCString(title),
      @"Session needs attention");
  content.body = GhostexGpuiTrimmedNotificationString(
      GhostexGpuiNotificationStringFromCString(body), @"Ghostex");
  content.categoryIdentifier = GhostexGpuiSessionAttentionNotificationCategory;
  content.threadIdentifier =
      [NSString stringWithFormat:@"ghostex.gpui.session.attention.%@",
                                 notificationSessionId];
  content.targetContentIdentifier = notificationSessionId;
  content.userInfo = @{@"sessionId" : notificationSessionId};
  content.sound = nil;

  NSString *identifier = [NSString
      stringWithFormat:@"ghostex.gpui.session.attention.%@.%@",
                       notificationSessionId, [NSUUID UUID].UUIDString];
  NSURL *attachmentURL = GhostexGpuiApplySessionAttentionIconAttachment(
      content, iconDataUrl, identifier);
  UNNotificationRequest *request =
      [UNNotificationRequest requestWithIdentifier:identifier
                                           content:content
                                           trigger:nil];

  dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
  __block BOOL delivered = NO;
  [center addNotificationRequest:request
           withCompletionHandler:^(NSError *error) {
             delivered = error == nil;
             if (delivered) {
               GhostexGpuiRemoveDeliveredSessionAttentionNotificationLater(
                   identifier, attachmentURL);
             } else if (attachmentURL) {
               [[NSFileManager defaultManager] removeItemAtURL:attachmentURL
                                                         error:nil];
             }
             dispatch_semaphore_signal(semaphore);
           }];

  dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC);
  if (dispatch_semaphore_wait(semaphore, timeout) != 0) {
    if (attachmentURL) {
      [[NSFileManager defaultManager] removeItemAtURL:attachmentURL error:nil];
    }
    return GhostexGpuiNotificationDeliveryFailed;
  }

  return delivered ? GhostexGpuiNotificationDeliverySent
                   : GhostexGpuiNotificationDeliveryFailed;
}
