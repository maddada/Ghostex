#import "GhostexCEFBridge.h"

#import <QuartzCore/QuartzCore.h>

#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>

#include <atomic>
#include <cmath>
#include <cstring>
#include <functional>
#include <map>
#include <memory>
#include <string>
#include <vector>

#include "include/base/cef_logging.h"
#include "include/cef_app.h"
#include "include/cef_application_mac.h"
#include "include/cef_browser.h"
#include "include/cef_client.h"
#include "include/cef_command_ids.h"
#include "include/cef_command_line.h"
#include "include/cef_context_menu_handler.h"
#include "include/cef_cookie.h"
#include "include/cef_display_handler.h"
#include "include/cef_find_handler.h"
#include "include/cef_life_span_handler.h"
#include "include/cef_load_handler.h"
#include "include/cef_permission_handler.h"
#include "include/cef_request_context.h"
#include "include/wrapper/cef_helpers.h"
#include "include/wrapper/cef_library_loader.h"

static int g_remoteDebuggingPort = 9333;
static bool g_cefInitialized = false;
static CefRefPtr<CefApp> g_cefApp;
static std::map<std::string, CefRefPtr<CefRequestContext>> g_persistentRequestContexts;
static NSString* const GhostexCEFBuiltInDefaultProfileIdentifier = @"52B43C05-4A1D-45D3-8FD5-9EF94952E445";
static NSString* const GhostexCEFViewportDiagnosticConsolePrefix = @"__GHOSTEX_CEF_VIEWPORT_DIAGNOSTIC__";
using GhostexCEFCompletionBlock = void (^)(void);

struct GhostexCEFProfileFlushState {
  explicit GhostexCEFProfileFlushState(GhostexCEFCompletionBlock completionBlock)
      : completion([completionBlock copy]) {}

  ~GhostexCEFProfileFlushState() {
    completion = nil;
  }

  std::atomic<int> pending{0};
  GhostexCEFCompletionBlock completion;
};

struct GhostexCEFCookieImportState {
  explicit GhostexCEFCookieImportState(void (^completionBlock)(NSInteger))
      : completion([completionBlock copy]) {}

  ~GhostexCEFCookieImportState() {
    completion = nil;
  }

  std::atomic<int> pending{0};
  std::atomic<int> imported{0};
  void (^completion)(NSInteger);
};

static void GhostexCEFFinishCookieImport(std::shared_ptr<GhostexCEFCookieImportState> state,
                                         bool success) {
  if (!state) {
    return;
  }
  if (success) {
    state->imported.fetch_add(1);
  }
  if (state->pending.fetch_sub(1) != 1) {
    return;
  }
  auto importedCount = state->imported.load();
  void (^completion)(NSInteger) = [state->completion copy];
  dispatch_async(dispatch_get_main_queue(), ^{
    if (completion) {
      completion(importedCount);
    }
  });
}

static void GhostexCEFFinishProfileFlush(std::shared_ptr<GhostexCEFProfileFlushState> state) {
  if (!state || state->pending.fetch_sub(1) != 1) {
    return;
  }
  GhostexCEFCompletionBlock completion = [state->completion copy];
  dispatch_async(dispatch_get_main_queue(), ^{
    if (completion) {
      completion();
    }
  });
}

class GhostexCEFCookieFlushCallback : public CefCompletionCallback {
 public:
  explicit GhostexCEFCookieFlushCallback(std::shared_ptr<GhostexCEFProfileFlushState> state)
      : state_(std::move(state)) {}

  void OnComplete() override {
    GhostexCEFFinishProfileFlush(state_);
  }

 private:
  std::shared_ptr<GhostexCEFProfileFlushState> state_;

  IMPLEMENT_REFCOUNTING(GhostexCEFCookieFlushCallback);
  DISALLOW_COPY_AND_ASSIGN(GhostexCEFCookieFlushCallback);
};

class GhostexCEFCookieImportCallback : public CefSetCookieCallback {
 public:
  explicit GhostexCEFCookieImportCallback(std::shared_ptr<GhostexCEFCookieImportState> state)
      : state_(std::move(state)) {}

  void OnComplete(bool success) override {
    GhostexCEFFinishCookieImport(state_, success);
  }

 private:
  std::shared_ptr<GhostexCEFCookieImportState> state_;

  IMPLEMENT_REFCOUNTING(GhostexCEFCookieImportCallback);
  DISALLOW_COPY_AND_ASSIGN(GhostexCEFCookieImportCallback);
};

static bool GhostexCEFFlushCookieManager(CefRefPtr<CefCookieManager> manager,
                                      std::shared_ptr<GhostexCEFProfileFlushState> state) {
  if (!manager || !state) {
    return false;
  }
  state->pending.fetch_add(1);
  if (!manager->FlushStore(new GhostexCEFCookieFlushCallback(state))) {
    GhostexCEFFinishProfileFlush(state);
    return false;
  }
  return true;
}

static bool IsPortAvailable(int port) {
  int sock = socket(AF_INET, SOCK_STREAM, 0);
  if (sock < 0) {
    return false;
  }
  int opt = 1;
  setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

  sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
  addr.sin_port = htons(static_cast<uint16_t>(port));

  int result = bind(sock, reinterpret_cast<sockaddr*>(&addr), sizeof(addr));
  close(sock);
  return result == 0;
}

static int FindAvailableRemoteDebuggingPort(void) {
  for (int port = 9333; port <= 9343; ++port) {
    if (IsPortAvailable(port)) {
      return port;
    }
  }
  return 9333;
}

static NSString* GhostexCEFHomeDirectoryName(void) {
  NSString* value = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"GHOSTEXHomeDirectoryName"];
  if ([value isKindOfClass:[NSString class]] && value.length > 0) {
    return value;
  }
  return @".ghostex";
}

static NSString* GhostexCEFSharedHomeDirectoryName(void) {
  NSString* value = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"GHOSTEXSharedHomeDirectoryName"];
  if ([value isKindOfClass:[NSString class]] && value.length > 0) {
    return value;
  }
  return @".ghostex";
}

static NSString* GhostexCEFStorageDirectory(void) {
  NSString* root = [NSHomeDirectory() stringByAppendingPathComponent:GhostexCEFHomeDirectoryName()];
  NSString* path = [root stringByAppendingPathComponent:@"cef"];
  [[NSFileManager defaultManager] createDirectoryAtPath:path
                            withIntermediateDirectories:YES
                                             attributes:nil
                                                  error:nil];
  return path;
}

static bool GhostexCEFNativeDebugLoggingEnabled(void) {
  static bool hasCachedValue = false;
  static bool cachedValue = false;
  static NSTimeInterval cachedValueReadAt = 0;
  NSTimeInterval now = [[NSProcessInfo processInfo] systemUptime];
  if (hasCachedValue && now - cachedValueReadAt < 0.25) {
    return cachedValue;
  }

  NSString* settingsPath = [[[NSHomeDirectory() stringByAppendingPathComponent:GhostexCEFSharedHomeDirectoryName()]
    stringByAppendingPathComponent:@"state"]
    stringByAppendingPathComponent:@"native-sidebar-settings.json"];
  NSData* data = [NSData dataWithContentsOfFile:settingsPath];
  bool isEnabled = false;
  if (data) {
    NSError* error = nil;
    id json = [NSJSONSerialization JSONObjectWithData:data options:0 error:&error];
    if (!error && [json isKindOfClass:[NSDictionary class]]) {
      id value = [(NSDictionary*)json objectForKey:@"debuggingMode"];
      if ([value respondsToSelector:@selector(boolValue)]) {
        isEnabled = [value boolValue];
      }
    }
  }

  cachedValue = isEnabled;
  cachedValueReadAt = now;
  hasCachedValue = true;
  return cachedValue;
}

static NSDateFormatter* GhostexCEFDiagnosticLogDateFormatter(void) {
  static NSDateFormatter* formatter = nil;
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    formatter = [[NSDateFormatter alloc] init];
    formatter.dateFormat = @"yyyy-MM-dd HH:mm:ss.SSS ZZZZ";
    formatter.locale = [NSLocale localeWithLocaleIdentifier:@"en_US_POSIX"];
    formatter.timeZone = [NSTimeZone localTimeZone];
  });
  return formatter;
}

static NSDictionary* GhostexCEFDescribeRect(NSRect rect) {
  return @{
    @"height": @(rect.size.height),
    @"maxX": @(NSMaxX(rect)),
    @"maxY": @(NSMaxY(rect)),
    @"minX": @(NSMinX(rect)),
    @"minY": @(NSMinY(rect)),
    @"width": @(rect.size.width),
    @"x": @(rect.origin.x),
    @"y": @(rect.origin.y),
  };
}

static NSDictionary* GhostexCEFDescribePoint(NSPoint point) {
  return @{
    @"x": @(point.x),
    @"y": @(point.y),
  };
}

static NSDictionary* GhostexCEFDescribeAffineTransform(CGAffineTransform transform) {
  return @{
    @"a": @(transform.a),
    @"b": @(transform.b),
    @"c": @(transform.c),
    @"d": @(transform.d),
    @"tx": @(transform.tx),
    @"ty": @(transform.ty),
  };
}

static NSDictionary* GhostexCEFDescribeTransform3D(CATransform3D transform) {
  return @{
    @"m11": @(transform.m11),
    @"m12": @(transform.m12),
    @"m13": @(transform.m13),
    @"m14": @(transform.m14),
    @"m21": @(transform.m21),
    @"m22": @(transform.m22),
    @"m23": @(transform.m23),
    @"m24": @(transform.m24),
    @"m31": @(transform.m31),
    @"m32": @(transform.m32),
    @"m33": @(transform.m33),
    @"m34": @(transform.m34),
    @"m41": @(transform.m41),
    @"m42": @(transform.m42),
    @"m43": @(transform.m43),
    @"m44": @(transform.m44),
  };
}

static NSDictionary* GhostexCEFDescribeLayer(CALayer* layer) {
  if (!layer) {
    return @{};
  }
  return @{
    @"affineTransform": GhostexCEFDescribeAffineTransform([layer affineTransform]),
    @"anchorPoint": GhostexCEFDescribePoint(NSPointFromCGPoint(layer.anchorPoint)),
    @"bounds": GhostexCEFDescribeRect(NSRectFromCGRect(layer.bounds)),
    @"class": NSStringFromClass([layer class]) ?: @"",
    @"contentsGravity": layer.contentsGravity ?: @"",
    @"contentsScale": @(layer.contentsScale),
    @"frame": GhostexCEFDescribeRect(NSRectFromCGRect(layer.frame)),
    @"geometryFlipped": @(layer.geometryFlipped),
    @"hidden": @(layer.hidden),
    @"masksToBounds": @(layer.masksToBounds),
    @"opacity": @(layer.opacity),
    @"position": GhostexCEFDescribePoint(NSPointFromCGPoint(layer.position)),
    @"sublayerCount": @(layer.sublayers.count),
    @"sublayerTransform": GhostexCEFDescribeTransform3D(layer.sublayerTransform),
    @"transform": GhostexCEFDescribeTransform3D(layer.transform),
    @"zPosition": @(layer.zPosition),
  };
}

static id GhostexCEFDescribeBoundsInWindow(NSView* view) {
  if (!view || !view.window) {
    return [NSNull null];
  }
  return GhostexCEFDescribeRect([view convertRect:view.bounds toView:nil]);
}

static id GhostexCEFDescribeFrameInWindow(NSView* view) {
  if (!view || !view.window || !view.superview) {
    return [NSNull null];
  }
  return GhostexCEFDescribeRect([view.superview convertRect:view.frame toView:nil]);
}

static NSString* GhostexCEFJSONStringLiteral(NSString* value) {
  if (!value) {
    return @"null";
  }
  NSData* data = [NSJSONSerialization dataWithJSONObject:value options:NSJSONWritingFragmentsAllowed error:nil];
  if (!data) {
    return @"null";
  }
  return [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding] ?: @"null";
}

static NSDictionary* GhostexCEFDescribeView(NSView* view, NSUInteger depth, NSString* path) {
  CALayer* layer = view.layer;
  NSMutableDictionary* details = [@{
    @"alphaValue": @(view.alphaValue),
    @"autoresizingMask": @(view.autoresizingMask),
    @"bounds": GhostexCEFDescribeRect(view.bounds),
    @"boundsInWindow": GhostexCEFDescribeBoundsInWindow(view),
    @"boundsRotation": @(view.boundsRotation),
    @"canDrawSubviewsIntoLayer": @(view.canDrawSubviewsIntoLayer),
    @"class": NSStringFromClass([view class]) ?: @"",
    @"depth": @(depth),
    @"frame": GhostexCEFDescribeRect(view.frame),
    @"frameInWindow": GhostexCEFDescribeFrameInWindow(view),
    @"frameRotation": @(view.frameRotation),
    @"hidden": @(view.hidden),
    @"hiddenOrHasHiddenAncestor": @([view isHiddenOrHasHiddenAncestor]),
    @"inLiveResize": @(view.inLiveResize),
    @"isFlipped": @([view isFlipped]),
    @"isOpaque": @([view isOpaque]),
    @"layerContentsRedrawPolicy": @(view.layerContentsRedrawPolicy),
    @"needsDisplay": @(view.needsDisplay),
    @"needsLayout": @(view.needsLayout),
    @"path": path ?: @"",
    @"subviewCount": @(view.subviews.count),
    @"translatesAutoresizingMaskIntoConstraints": @(view.translatesAutoresizingMaskIntoConstraints),
    @"visibleRect": GhostexCEFDescribeRect(view.visibleRect),
    @"wantsLayer": @(view.wantsLayer),
  } mutableCopy];
  if (view.superview) {
    details[@"superviewClass"] = NSStringFromClass([view.superview class]) ?: @"";
  }
  if (layer) {
    details[@"layer"] = GhostexCEFDescribeLayer(layer);
  }
  return details;
}

static void GhostexCEFCollectViewHierarchy(NSView* view,
                                           NSMutableArray* nodes,
                                           NSUInteger depth,
                                           NSUInteger maxDepth,
                                           NSUInteger maxCount,
                                           NSString* path) {
  if (!view || nodes.count >= maxCount) {
    return;
  }
  [nodes addObject:GhostexCEFDescribeView(view, depth, path)];
  if (depth >= maxDepth) {
    return;
  }
  NSUInteger index = 0;
  for (NSView* subview in view.subviews) {
    if (nodes.count >= maxCount) {
      return;
    }
    NSString* subviewPath = [NSString stringWithFormat:@"%@.%lu", path ?: @"0", (unsigned long)index];
    GhostexCEFCollectViewHierarchy(subview, nodes, depth + 1, maxDepth, maxCount, subviewPath);
    index += 1;
  }
}

static NSArray* GhostexCEFCollectAncestorChain(NSView* view, NSUInteger maxCount) {
  NSMutableArray* ancestors = [NSMutableArray array];
  NSView* cursor = view;
  NSUInteger depth = 0;
  while (cursor && ancestors.count < maxCount) {
    [ancestors addObject:GhostexCEFDescribeView(cursor, depth, [NSString stringWithFormat:@"%lu", (unsigned long)depth])];
    cursor = cursor.superview;
    depth += 1;
  }
  return ancestors;
}

static void GhostexCEFCollectLayerHierarchy(CALayer* layer,
                                            NSMutableArray* nodes,
                                            NSUInteger depth,
                                            NSUInteger maxDepth,
                                            NSUInteger maxCount,
                                            NSString* path) {
  if (!layer || nodes.count >= maxCount) {
    return;
  }
  NSMutableDictionary* details = [NSMutableDictionary dictionaryWithDictionary:GhostexCEFDescribeLayer(layer)];
  details[@"depth"] = @(depth);
  details[@"path"] = path ?: @"";
  [nodes addObject:details];
  if (depth >= maxDepth) {
    return;
  }
  NSUInteger index = 0;
  for (CALayer* sublayer in layer.sublayers) {
    if (nodes.count >= maxCount) {
      return;
    }
    NSString* sublayerPath = [NSString stringWithFormat:@"%@.%lu", path ?: @"0", (unsigned long)index];
    GhostexCEFCollectLayerHierarchy(sublayer, nodes, depth + 1, maxDepth, maxCount, sublayerPath);
    index += 1;
  }
}

static bool GhostexCEFDiagnosticRectChangedEnough(NSRect lhs, NSRect rhs, CGFloat threshold) {
  return fabs(lhs.origin.x - rhs.origin.x) >= threshold ||
    fabs(lhs.origin.y - rhs.origin.y) >= threshold ||
    fabs(lhs.size.width - rhs.size.width) >= threshold ||
    fabs(lhs.size.height - rhs.size.height) >= threshold;
}

static bool GhostexCEFRectAlmostEqual(NSRect lhs, NSRect rhs) {
  const CGFloat epsilon = 0.001;
  return fabs(lhs.origin.x - rhs.origin.x) < epsilon &&
    fabs(lhs.origin.y - rhs.origin.y) < epsilon &&
    fabs(lhs.size.width - rhs.size.width) < epsilon &&
    fabs(lhs.size.height - rhs.size.height) < epsilon;
}

static bool GhostexCEFPointAlmostEqual(NSPoint lhs, NSPoint rhs) {
  const CGFloat epsilon = 0.001;
  return fabs(lhs.x - rhs.x) < epsilon &&
    fabs(lhs.y - rhs.y) < epsilon;
}

static NSString* GhostexCEFRedactedText(void) {
  return @"[redacted]";
}

static NSString* GhostexCEFRedactedPath(void) {
  return @"[redacted:path]";
}

static NSString* GhostexCEFRedactedURL(void) {
  return @"[redacted:url]";
}

static NSString* GhostexCEFRedactedSecret(void) {
  return @"[redacted:secret]";
}

static BOOL GhostexCEFKeyHasSuffix(NSString* key, NSString* suffix) {
  return [key hasSuffix:suffix];
}

static BOOL GhostexCEFIsIdentifierKey(NSString* key) {
  return [key isEqualToString:@"id"] ||
    GhostexCEFKeyHasSuffix(key, @"id") ||
    GhostexCEFKeyHasSuffix(key, @"ids") ||
    GhostexCEFKeyHasSuffix(key, @"ref") ||
    GhostexCEFKeyHasSuffix(key, @"refs");
}

static BOOL GhostexCEFIsSecretKey(NSString* key) {
  return [key containsString:@"token"] ||
    [key containsString:@"bearer"] ||
    [key containsString:@"secret"] ||
    [key containsString:@"credential"] ||
    [key containsString:@"password"] ||
    [key containsString:@"cookie"] ||
    [key containsString:@"authorization"] ||
    [key containsString:@"auth"];
}

static BOOL GhostexCEFIsURLKey(NSString* key) {
  return [key isEqualToString:@"url"] ||
    GhostexCEFKeyHasSuffix(key, @"url") ||
    [key containsString:@"uri"] ||
    [key isEqualToString:@"href"] ||
    [key isEqualToString:@"origin"];
}

static BOOL GhostexCEFIsPathKey(NSString* key) {
  return [key isEqualToString:@"path"] ||
    [key isEqualToString:@"cwd"] ||
    GhostexCEFKeyHasSuffix(key, @"path") ||
    GhostexCEFKeyHasSuffix(key, @"dir") ||
    GhostexCEFKeyHasSuffix(key, @"directory") ||
    GhostexCEFKeyHasSuffix(key, @"root") ||
    GhostexCEFKeyHasSuffix(key, @"file") ||
    GhostexCEFKeyHasSuffix(key, @"filename") ||
    [key containsString:@"workspace"];
}

static BOOL GhostexCEFIsSensitiveTextKey(NSString* key) {
  return [key isEqualToString:@"title"] ||
    GhostexCEFKeyHasSuffix(key, @"title") ||
    [key isEqualToString:@"name"] ||
    GhostexCEFKeyHasSuffix(key, @"name") ||
    [key isEqualToString:@"message"] ||
    [key isEqualToString:@"details"] ||
    GhostexCEFKeyHasSuffix(key, @"details") ||
    [key isEqualToString:@"input"] ||
    [key isEqualToString:@"text"] ||
    GhostexCEFKeyHasSuffix(key, @"text") ||
    [key isEqualToString:@"comment"] ||
    [key isEqualToString:@"description"] ||
    [key isEqualToString:@"label"] ||
    [key isEqualToString:@"command"] ||
    GhostexCEFKeyHasSuffix(key, @"command") ||
    [key isEqualToString:@"stdout"] ||
    [key isEqualToString:@"stderr"] ||
    [key isEqualToString:@"body"] ||
    GhostexCEFKeyHasSuffix(key, @"body");
}

static BOOL GhostexCEFIsSensitiveCollectionKey(NSString* key) {
  return [key isEqualToString:@"args"] ||
    GhostexCEFKeyHasSuffix(key, @"args") ||
    [key isEqualToString:@"arguments"] ||
    GhostexCEFKeyHasSuffix(key, @"arguments");
}

static NSString* GhostexCEFRegexReplace(NSString* value, NSString* pattern, NSString* replacement) {
  NSError* error = nil;
  NSRegularExpression* regex = [NSRegularExpression regularExpressionWithPattern:pattern options:0 error:&error];
  if (error || !regex) {
    return value;
  }
  return [regex stringByReplacingMatchesInString:value
                                         options:0
                                           range:NSMakeRange(0, value.length)
                                    withTemplate:replacement];
}

static BOOL GhostexCEFStringMatches(NSString* value, NSString* pattern) {
  NSError* error = nil;
  NSRegularExpression* regex = [NSRegularExpression regularExpressionWithPattern:pattern options:0 error:&error];
  if (error || !regex) {
    return NO;
  }
  return [regex firstMatchInString:value options:0 range:NSMakeRange(0, value.length)] != nil;
}

static BOOL GhostexCEFIsSafeIdentifier(NSString* value) {
  return GhostexCEFStringMatches(value, @"^[A-Za-z0-9._:-]{1,128}$");
}

static BOOL GhostexCEFLooksLikeURL(NSString* value) {
  return GhostexCEFStringMatches(value, @"(?i)^https?://");
}

static BOOL GhostexCEFLooksLikePath(NSString* value) {
  return GhostexCEFStringMatches(value, @"^(~/|/Users/|/Volumes/|/private/|/tmp/|/var/folders/)");
}

static NSDictionary* GhostexCEFSummarizeURL(NSString* value) {
  NSURLComponents* components = [NSURLComponents componentsWithString:value];
  NSMutableDictionary* summary = [NSMutableDictionary dictionaryWithDictionary:@{
    @"redacted": @(YES),
    @"type": @"url",
  }];
  if (components.scheme.length > 0) {
    summary[@"protocol"] = components.scheme;
  }
  if (components.host.length > 0) {
    summary[@"host"] = components.port
      ? [NSString stringWithFormat:@"%@:%@", components.host, components.port]
      : components.host;
  }
  return summary;
}

static NSString* GhostexCEFRedactSensitiveText(NSString* value) {
  NSString* redacted = value ?: @"";
  redacted = GhostexCEFRegexReplace(
    redacted,
    @"(?i)\"(title|name|projectName|sessionName|cwd|path|projectPath|workspaceRoot|worktreePath|url|input|comment|description|command|text|message|details|token|authToken|bearer|credential|password|secret)\"\\s*:\\s*\"[^\"]*\"",
    @"\"$1\":\"[redacted]\"");
  redacted = GhostexCEFRegexReplace(
    redacted,
    @"(?i)\\b(bearer|token|authorization|password|secret|credential)=?[^\\s\"']+",
    GhostexCEFRedactedSecret());
  redacted = GhostexCEFRegexReplace(redacted, @"https?://[^\\s\"')]+", GhostexCEFRedactedURL());
  redacted = GhostexCEFRegexReplace(redacted, @"(~|/Users/[^\\s/\"']+|/(private/)?tmp|/var/folders|/Volumes)/[^\\s\"']+", GhostexCEFRedactedPath());
  return redacted;
}

static id GhostexCEFSanitizeDiagnosticValue(id value, NSString* key);

static NSDictionary* GhostexCEFSanitizeDiagnosticPayload(NSDictionary* payload) {
  NSMutableDictionary* sanitized = [NSMutableDictionary dictionary];
  for (id rawKey in payload) {
    NSString* key = [rawKey isKindOfClass:[NSString class]]
      ? (NSString*)rawKey
      : [rawKey description];
    sanitized[key] = GhostexCEFSanitizeDiagnosticValue(payload[rawKey], key);
  }
  return sanitized;
}

static id GhostexCEFSanitizeDiagnosticString(NSString* value, NSString* key) {
  NSString* normalizedKey = [[key lowercaseString] copy] ?: @"";
  if (GhostexCEFIsSecretKey(normalizedKey)) {
    return GhostexCEFRedactedSecret();
  }
  if (GhostexCEFIsIdentifierKey(normalizedKey) && GhostexCEFIsSafeIdentifier(value)) {
    return value;
  }
  if (GhostexCEFIsURLKey(normalizedKey) || GhostexCEFLooksLikeURL(value)) {
    return GhostexCEFSummarizeURL(value);
  }
  if (GhostexCEFIsPathKey(normalizedKey) || GhostexCEFLooksLikePath(value)) {
    return GhostexCEFRedactedPath();
  }
  if (GhostexCEFIsSensitiveTextKey(normalizedKey)) {
    return GhostexCEFRedactedText();
  }
  return GhostexCEFRedactSensitiveText(value);
}

static id GhostexCEFSanitizeDiagnosticValue(id value, NSString* key) {
  if (!value || value == [NSNull null]) {
    return [NSNull null];
  }
  NSString* normalizedKey = [[key lowercaseString] copy] ?: @"";
  if ([value isKindOfClass:[NSString class]]) {
    return GhostexCEFSanitizeDiagnosticString((NSString*)value, normalizedKey);
  }
  if ([value isKindOfClass:[NSNumber class]]) {
    return value;
  }
  if ([value isKindOfClass:[NSArray class]]) {
    NSArray* array = (NSArray*)value;
    if (GhostexCEFIsSensitiveCollectionKey(normalizedKey)) {
      return @{@"count": @(array.count), @"redacted": @(YES)};
    }
    NSMutableArray* sanitizedArray = [NSMutableArray arrayWithCapacity:array.count];
    for (id item in array) {
      [sanitizedArray addObject:GhostexCEFSanitizeDiagnosticValue(item, key) ?: [NSNull null]];
    }
    return sanitizedArray;
  }
  if ([value isKindOfClass:[NSDictionary class]]) {
    if (GhostexCEFIsSensitiveCollectionKey(normalizedKey)) {
      return @{@"redacted": @(YES)};
    }
    return GhostexCEFSanitizeDiagnosticPayload((NSDictionary*)value);
  }
  return GhostexCEFRedactSensitiveText([value description]);
}

static void GhostexCEFAppendDiagnosticLog(NSString* event, NSDictionary* details) {
  if (!GhostexCEFNativeDebugLoggingEnabled()) {
    return;
  }
  NSMutableDictionary* payload = [NSMutableDictionary dictionaryWithDictionary:details ? details : @{}];
  [payload setObject:event forKey:@"event"];
  /*
   CDXC:DiagnosticsPrivacy 2026-05-31-00:33:
   CEF diagnostics write directly from Objective-C++ into the same support-bundle log as Swift pane diagnostics. Sanitize the payload before JSON serialization so raw browser URLs, page titles, source paths, console text, and credentials cannot bypass NativeLogPrivacy when users zip and send logs.
   */
  NSDictionary* sanitizedPayload = GhostexCEFSanitizeDiagnosticPayload(payload);
  NSError* serializationError = nil;
  NSData* jsonData = [NSJSONSerialization dataWithJSONObject:sanitizedPayload
                                                     options:NSJSONWritingSortedKeys
                                                       error:&serializationError];
  NSString* json = serializationError || !jsonData
    ? @"{\"event\":\"serializationFailed\"}"
    : [[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding];
  NSString* line = [NSString stringWithFormat:@"[%@] %@\n",
    [GhostexCEFDiagnosticLogDateFormatter() stringFromDate:[NSDate date]],
    json ?: @"{\"event\":\"serializationFailed\"}"];

  NSString* logsDirectory = [[NSHomeDirectory() stringByAppendingPathComponent:GhostexCEFHomeDirectoryName()]
    stringByAppendingPathComponent:@"logs"];
  NSString* logPath = [logsDirectory stringByAppendingPathComponent:@"native-t3-code-pane-repro.log"];
  [[NSFileManager defaultManager] createDirectoryAtPath:logsDirectory
                            withIntermediateDirectories:YES
                                             attributes:nil
                                                  error:nil];
  NSFileHandle* handle = [NSFileHandle fileHandleForWritingAtPath:logPath];
  NSData* lineData = [line dataUsingEncoding:NSUTF8StringEncoding];
  if (handle) {
    [handle seekToEndOfFile];
    [handle writeData:lineData];
    [handle closeFile];
    return;
  }
  [lineData writeToFile:logPath atomically:YES];
}

static CefRefPtr<CefRequestContext> GhostexCEFRequestContextForProfile(NSString* profileIdentifier) {
  NSString* identifier = profileIdentifier.length > 0 ? profileIdentifier : @"default";
  if ([identifier isEqualToString:@"default"] || [identifier isEqualToString:GhostexCEFBuiltInDefaultProfileIdentifier]) {
    return CefRequestContext::GetGlobalContext();
  }

  std::string key([identifier UTF8String]);
  auto existing = g_persistentRequestContexts.find(key);
  if (existing != g_persistentRequestContexts.end()) {
    return existing->second;
  }

  /**
   CDXC:ChromiumBrowserPanes 2026-05-04-17:09
   Electrobun keeps CEF custom profiles outside Chromium's own `Default` profile
   folder and reuses each request context. Chrome runtime can reject duplicate or
   colliding cache paths, so the built-in default profile stays on Chromium's
   global context while named ghostex profiles get their own cached CEF contexts.

   Chromium only allows Chrome profile directories directly under the CEF
   user-data root. Keep named profile cache paths as direct
   `~/.ghostex/cef/<profile-id>` children; nested `cef/partitions/<profile-id>`
   paths are rejected and create non-persistent profiles.
   */
  NSString* profilePath = [GhostexCEFStorageDirectory() stringByAppendingPathComponent:identifier];
  [[NSFileManager defaultManager] createDirectoryAtPath:profilePath
                            withIntermediateDirectories:YES
                                             attributes:nil
                                                  error:nil];

  CefRequestContextSettings contextSettings;
  contextSettings.persist_session_cookies = true;
  CefString(&contextSettings.cache_path) = [profilePath UTF8String];
  CefRefPtr<CefRequestContext> context = CefRequestContext::CreateContext(contextSettings, nullptr);
  if (!context) {
    NSLog(@"[CEF] Failed to create persistent request context for profile <redacted>; using global context.");
    return CefRequestContext::GetGlobalContext();
  }
  g_persistentRequestContexts[key] = context;
  return context;
}

void GhostexCEFImportCookiesForProfile(
  NSString* profileIdentifier,
  NSArray<NSHTTPCookie*>* cookies,
  void (^completion)(NSInteger importedCount)) {
  if (!g_cefInitialized || cookies.count == 0) {
    if (completion) {
      dispatch_async(dispatch_get_main_queue(), ^{
        completion(0);
      });
    }
    return;
  }

  CefRefPtr<CefRequestContext> requestContext = GhostexCEFRequestContextForProfile(profileIdentifier);
  CefRefPtr<CefCookieManager> cookieManager = requestContext ? requestContext->GetCookieManager(nullptr) : nullptr;
  if (!cookieManager) {
    if (completion) {
      dispatch_async(dispatch_get_main_queue(), ^{
        completion(0);
      });
    }
    return;
  }

  /*
   CDXC:BrowserImport 2026-06-04-11:40:
   Browser data import must populate the selected Ghostex browser profile through CEF's cookie manager instead of copying another browser's raw profile database. Chromium profile stores include browser-specific encryption, locks, and schema state; importing compatible cookies at the writer boundary keeps the destination profile owned by Ghostex and lets CEF decide which cookies are valid.
   */
  auto state = std::make_shared<GhostexCEFCookieImportState>(completion);
  for (NSHTTPCookie* cookie in cookies) {
    NSString* name = cookie.name ?: @"";
    NSString* value = cookie.value ?: @"";
    NSString* domain = cookie.domain ?: @"";
    if (name.length == 0 || domain.length == 0) {
      continue;
    }
    NSString* normalizedHost = domain;
    while ([normalizedHost hasPrefix:@"."]) {
      normalizedHost = [normalizedHost substringFromIndex:1];
    }
    if (normalizedHost.length == 0) {
      continue;
    }

    NSString* path = cookie.path.length > 0 ? cookie.path : @"/";
    NSString* scheme = cookie.isSecure ? @"https" : @"http";
    NSString* url = [NSString stringWithFormat:@"%@://%@%@", scheme, normalizedHost, path];

    CefCookie cefCookie;
    CefString(&cefCookie.name) = [name UTF8String];
    CefString(&cefCookie.value) = [value UTF8String];
    CefString(&cefCookie.domain) = [domain UTF8String];
    CefString(&cefCookie.path) = [path UTF8String];
    cefCookie.secure = cookie.isSecure;
    cefCookie.httponly = cookie.isHTTPOnly;
    if (cookie.expiresDate) {
      cefCookie.has_expires = true;
      cefCookie.expires.val = static_cast<int64_t>(
        (cookie.expiresDate.timeIntervalSince1970 + 11644473600.0) * 1000000.0);
    }

    state->pending.fetch_add(1);
    if (!cookieManager->SetCookie(
          CefString([url UTF8String]),
          cefCookie,
          new GhostexCEFCookieImportCallback(state))) {
      GhostexCEFFinishCookieImport(state, false);
    }
  }

  if (state->pending.load() == 0) {
    if (completion) {
      dispatch_async(dispatch_get_main_queue(), ^{
        completion(0);
      });
    }
  }
}

static NSString* GhostexCEFFrameworkExecutablePath(void) {
  return [[[NSBundle mainBundle] privateFrameworksPath]
    stringByAppendingPathComponent:@"Chromium Embedded Framework.framework/Chromium Embedded Framework"];
}

static NSString* GhostexCEFFrameworkBundlePath(void) {
  return [[[NSBundle mainBundle] privateFrameworksPath]
    stringByAppendingPathComponent:@"Chromium Embedded Framework.framework"];
}

static NSString* GhostexCEFHelperExecutablePath(void) {
  return [[[NSBundle mainBundle] privateFrameworksPath]
    stringByAppendingPathComponent:@"ghostex Helper.app/Contents/MacOS/ghostex Helper"];
}

static NSString* EscapeDevToolsWebSocketURL(NSString* url) {
  return [url stringByReplacingOccurrencesOfString:@"ws://" withString:@""];
}

@interface GhostexCEFApplication : NSApplication<CefAppProtocol> {
 @private
  BOOL handlingSendEvent_;
}
@end

@implementation GhostexCEFApplication
- (BOOL)isHandlingSendEvent {
  return handlingSendEvent_;
}

- (void)setHandlingSendEvent:(BOOL)handlingSendEvent {
  handlingSendEvent_ = handlingSendEvent;
}

- (void)sendEvent:(NSEvent*)event {
  CefScopedSendingEvent sendingEventScoper;
  [super sendEvent:event];
}
@end

class GhostexCEFApp : public CefApp, public CefBrowserProcessHandler {
 public:
  GhostexCEFApp() = default;

  CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override {
    return this;
  }

  void OnBeforeCommandLineProcessing(const CefString& process_type, CefRefPtr<CefCommandLine> command_line) override {
    /**
     CDXC:ChromiumBrowserPanes 2026-05-04-16:38
     Embedded Chromium panes must start in the correct CEF mode instead of
     attempting WebKit first. Keep the command line minimal and local-dev
     oriented: localhost cert exceptions support ghostex/T3 tooling, while the
     remote DevTools frontend remains bound to the selected loopback port.
     */
    command_line->AppendSwitch("use-mock-keychain");
    command_line->AppendSwitch("enable-fullscreen");
    command_line->AppendSwitch("allow-insecure-localhost");
    command_line->AppendSwitchWithValue("remote-allow-origins", "*");
  }

  void OnBeforeChildProcessLaunch(CefRefPtr<CefCommandLine> command_line) override {
    command_line->AppendSwitch("disable-background-mode");
    command_line->AppendSwitch("disable-backgrounding-occluded-windows");
  }

 private:
  IMPLEMENT_REFCOUNTING(GhostexCEFApp);
  DISALLOW_COPY_AND_ASSIGN(GhostexCEFApp);
};

class GhostexRemoteDevToolsClient : public CefClient, public CefLifeSpanHandler {
 public:
  explicit GhostexRemoteDevToolsClient(std::function<void()> onClose) : onClose_(std::move(onClose)) {}

  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override {
    return this;
  }

  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override {
    if (onClose_) {
      dispatch_async(dispatch_get_main_queue(), ^{
        onClose_();
      });
    }
  }

 private:
  std::function<void()> onClose_;

  IMPLEMENT_REFCOUNTING(GhostexRemoteDevToolsClient);
  DISALLOW_COPY_AND_ASSIGN(GhostexRemoteDevToolsClient);
};

@class GhostexCEFBrowserView;

class GhostexCEFBrowserClient : public CefClient,
                             public CefDisplayHandler,
                             public CefFindHandler,
                             public CefLoadHandler,
                             public CefLifeSpanHandler,
                             public CefContextMenuHandler,
                             public CefPermissionHandler {
 public:
  explicit GhostexCEFBrowserClient(GhostexCEFBrowserView* owner) : owner_(owner) {}

  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefFindHandler> GetFindHandler() override { return this; }
  CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefContextMenuHandler> GetContextMenuHandler() override { return this; }
  CefRefPtr<CefPermissionHandler> GetPermissionHandler() override { return this; }

  void OnTitleChange(CefRefPtr<CefBrowser> browser, const CefString& title) override;
  void OnAddressChange(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, const CefString& url) override;
  void OnFaviconURLChange(CefRefPtr<CefBrowser> browser, const std::vector<CefString>& icon_urls) override;
  void OnLoadStart(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, TransitionType transition_type) override;
  void OnLoadEnd(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int httpStatusCode) override;
  void OnLoadError(CefRefPtr<CefBrowser> browser,
                   CefRefPtr<CefFrame> frame,
                   ErrorCode errorCode,
                   const CefString& errorText,
                   const CefString& failedUrl) override;
  void OnLoadingStateChange(CefRefPtr<CefBrowser> browser, bool isLoading, bool canGoBack, bool canGoForward) override;
  bool OnConsoleMessage(CefRefPtr<CefBrowser> browser,
                        cef_log_severity_t level,
                        const CefString& message,
                        const CefString& source,
                        int line) override;
  void OnFindResult(CefRefPtr<CefBrowser> browser,
                    int identifier,
                    int count,
                    const CefRect& selectionRect,
                    int activeMatchOrdinal,
                    bool finalUpdate) override;
  void OnAfterCreated(CefRefPtr<CefBrowser> browser) override;
  bool DoClose(CefRefPtr<CefBrowser> browser) override;
  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;
  bool OnBeforePopup(CefRefPtr<CefBrowser> browser,
                     CefRefPtr<CefFrame> frame,
                     int popup_id,
                     const CefString& target_url,
                     const CefString& target_frame_name,
                     CefLifeSpanHandler::WindowOpenDisposition target_disposition,
                     bool user_gesture,
                     const CefPopupFeatures& popupFeatures,
                     CefWindowInfo& windowInfo,
                     CefRefPtr<CefClient>& client,
                     CefBrowserSettings& settings,
                     CefRefPtr<CefDictionaryValue>& extra_info,
                     bool* no_javascript_access) override;
  void OnBeforeContextMenu(CefRefPtr<CefBrowser> browser,
                           CefRefPtr<CefFrame> frame,
                           CefRefPtr<CefContextMenuParams> params,
                           CefRefPtr<CefMenuModel> model) override;
  bool OnContextMenuCommand(CefRefPtr<CefBrowser> browser,
                            CefRefPtr<CefFrame> frame,
                            CefRefPtr<CefContextMenuParams> params,
                            int command_id,
                            CefContextMenuHandler::EventFlags event_flags) override;
  bool OnShowPermissionPrompt(CefRefPtr<CefBrowser> browser,
                              uint64_t prompt_id,
                              const CefString& requesting_origin,
                              uint32_t requested_permissions,
                              CefRefPtr<CefPermissionPromptCallback> callback) override;

  void MarkClosingFromGhostex();
  void ToggleRemoteDevTools(CefRefPtr<CefBrowser> browser);
  void CloseRemoteDevTools();

 private:
  static constexpr int kInspectElementCommandId = 26001;

  bool OpenRequestedURLInGhostexTab(const CefString& target_url);
  void OpenRemoteDevToolsFrontend(CefRefPtr<CefBrowser> browser);
  void CreateRemoteDevToolsWindow(NSString* frontendURL);

  __weak GhostexCEFBrowserView* owner_;
  NSWindow* devToolsWindow_ = nil;
  CefRefPtr<CefBrowser> devToolsBrowser_;
  CefRefPtr<GhostexRemoteDevToolsClient> devToolsClient_;
  bool devToolsOpen_ = false;
  bool closingFromGhostex_ = false;
  std::string lastTitle_;

  IMPLEMENT_REFCOUNTING(GhostexCEFBrowserClient);
  DISALLOW_COPY_AND_ASSIGN(GhostexCEFBrowserClient);
};

static NSString* StringFromCefString(const CefString& value) {
  std::string stringValue = value.ToString();
  return [NSString stringWithUTF8String:stringValue.c_str()] ?: @"";
}

static NSString* GhostexCEFNormalizedOrigin(NSString* value) {
  NSURLComponents* components = [NSURLComponents componentsWithString:value ?: @""];
  NSString* scheme = components.scheme.lowercaseString;
  NSString* host = components.host.lowercaseString;
  if (scheme.length == 0 || host.length == 0) {
    return nil;
  }

  NSNumber* port = components.port;
  NSInteger normalizedPort = port != nil ? port.integerValue : -1;
  if (normalizedPort < 0) {
    if ([scheme isEqualToString:@"http"]) {
      normalizedPort = 80;
    } else if ([scheme isEqualToString:@"https"]) {
      normalizedPort = 443;
    }
  }

  if (normalizedPort >= 0) {
    return [NSString stringWithFormat:@"%@://%@:%ld", scheme, host, (long)normalizedPort];
  }
  return [NSString stringWithFormat:@"%@://%@", scheme, host];
}

static bool GhostexCEFOriginsMatch(NSString* lhs, NSString* rhs) {
  NSString* leftOrigin = GhostexCEFNormalizedOrigin(lhs);
  NSString* rightOrigin = GhostexCEFNormalizedOrigin(rhs);
  return leftOrigin.length > 0 && [leftOrigin isEqualToString:rightOrigin];
}

static void GhostexCEFGrantTrustedClipboardContentSetting(CefRefPtr<CefRequestContext> requestContext,
                                                          NSString* trustedOrigin) {
  NSString* normalizedOrigin = GhostexCEFNormalizedOrigin(trustedOrigin);
  if (!requestContext || normalizedOrigin.length == 0) {
    return;
  }

  /*
  CDXC:EditorClipboard 2026-05-21-07:23:
  VS Code Explorer file-copy reads Chromium's async clipboard before writing its file resource payload. CEF Alloy does not show Chrome's site-settings UI, so the embedded code-server origin must receive Chromium's clipboard read/write content setting up front instead of leaving navigator.clipboard.readText behind the default ask state.
  */
  CefString origin([normalizedOrigin UTF8String]);
  requestContext->SetContentSetting(
    origin,
    origin,
    CEF_CONTENT_SETTING_TYPE_CLIPBOARD_READ_WRITE,
    CEF_CONTENT_SETTING_VALUE_ALLOW);
}

@interface GhostexCEFBrowserView () {
 @private
  NSString* initialURL_;
  NSString* profileIdentifier_;
  CefRefPtr<CefBrowser> browser_;
  CefRefPtr<GhostexCEFBrowserClient> client_;
  NSView* cefView_;
  NSString* currentURLString_;
  NSString* pageTitle_;
  BOOL canGoBack_;
  BOOL canGoForward_;
  BOOL isLoading_;
  BOOL didCreateBrowser_;
  NSUInteger layoutPass_;
  NSUInteger internalGeometryDiagnosticSequence_;
  NSUInteger viewportDiagnosticSequence_;
  NSTimeInterval lastInternalGeometryDiagnosticAt_;
  NSTimeInterval lastViewportDiagnosticAt_;
  NSRect lastInternalGeometryDiagnosticBounds_;
  NSRect lastViewportDiagnosticBounds_;
}
- (void)ghostexCEFPinHostedViewToBoundsWithReason:(NSString*)reason;
@end

@implementation GhostexCEFBrowserView

- (instancetype)initWithFrame:(NSRect)frameRect
                   initialURL:(NSString*)initialURL
            profileIdentifier:(NSString*)profileIdentifier {
  self = [super initWithFrame:frameRect];
  if (self) {
    initialURL_ = [initialURL copy];
    profileIdentifier_ = [profileIdentifier copy];
    currentURLString_ = [initialURL copy];
    self.wantsLayer = YES;
    self.layer.backgroundColor = [NSColor colorWithCalibratedWhite:0.086 alpha:1].CGColor;
    self.layer.masksToBounds = YES;
  }
  return self;
}

- (BOOL)isFlipped {
  return YES;
}

- (BOOL)acceptsFirstResponder {
  return YES;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
  return YES;
}

- (NSView *)hitTest:(NSPoint)point {
  /*
  CDXC:EditorPanes 2026-05-14-08:50:
  The embedded VS Code CEF view must own secondary-click hit testing so VS Code can open its in-editor context menus.
  Route wrapper hits into the native CEF child view instead of letting AppKit treat the wrapper as the event target.

  CDXC:EditorPanes 2026-05-15-10:54:
  Left-edge project-editor clicks can land inside the CEF view frame while Chromium's internal render-widget hit-test returns nil.
  The visible CEF child still owns those pixels; return it instead of the wrapper so VS Code receives the click instead of only focusing the host.
  */
  if (!NSPointInRect(point, self.bounds)) {
    return nil;
  }

  if (cefView_ && !cefView_.hidden && cefView_.alphaValue > 0.0) {
    NSPoint cefPoint = [self convertPoint:point toView:cefView_];
    if (NSPointInRect(cefPoint, cefView_.bounds)) {
      NSView *hitView = [cefView_ hitTest:cefPoint];
      if (hitView) {
        return hitView;
      }
      return cefView_;
    }
  }

  return [super hitTest:point];
}

- (BOOL)becomeFirstResponder {
  if (cefView_ && self.window) {
    [self.window makeFirstResponder:cefView_];
  }
  return [super becomeFirstResponder];
}

- (void)viewDidMoveToWindow {
  [super viewDidMoveToWindow];
  if (self.window && !didCreateBrowser_) {
    [self createBrowserIfNeeded];
  }
}

- (void)layout {
  [super layout];
  [self ghostexCEFPinHostedViewToBoundsWithReason:@"layout"];
}

- (void)pinHostedViewToBounds {
  [self ghostexCEFPinHostedViewToBoundsWithReason:@"explicit"];
}

- (void)ghostexCEFPinHostedViewToBoundsWithReason:(NSString*)reason {
  NSRect targetFrame = self.bounds;
  if (!cefView_) {
    return;
  }
  NSRect cefFrameBefore = cefView_.frame;
  NSRect cefBoundsBefore = cefView_.bounds;
  NSPoint targetBoundsOrigin = NSMakePoint(0, 0);
  BOOL needsFrameUpdate = !GhostexCEFRectAlmostEqual(cefFrameBefore, targetFrame);
  BOOL needsBoundsOriginUpdate = !GhostexCEFPointAlmostEqual(cefBoundsBefore.origin, targetBoundsOrigin);
  /*
  CDXC:ChromiumBrowserPanes 2026-05-16-22:37:
  Disabled browser-toolbar clicks reproduced upward visual drift with unchanged AppKit and DOM geometry: the CEF wrapper stayed fixed while Chromium's flipped compositor layer grew after every redundant same-frame layout.
  Treat the native CEF frame assignment as an idempotent resize boundary, and only notify CEF when the hosted frame actually changes.

  CDXC:ChromiumBrowserPanes 2026-05-21-11:09:
  Bottom command-pane show/hide legitimately resizes the Code/Git CEF wrapper to reserve visible space; that resize must not let Chromium's hosted NSView keep a nonzero local bounds origin.
  Pin the hosted CEF view synchronously to the wrapper bounds instead of overlaying the command pane or reloading the page.
  */
  if (!needsFrameUpdate && !needsBoundsOriginUpdate) {
    return;
  }
  if (!GhostexCEFNativeDebugLoggingEnabled()) {
    if (needsFrameUpdate) {
      cefView_.frame = targetFrame;
    }
    if (needsBoundsOriginUpdate) {
      [cefView_ setBoundsOrigin:targetBoundsOrigin];
    }
    return;
  }

  id cefFrameInWindowBefore = GhostexCEFDescribeFrameInWindow(cefView_);
  id cefBoundsInWindowBefore = GhostexCEFDescribeBoundsInWindow(cefView_);
  if (needsFrameUpdate) {
    cefView_.frame = targetFrame;
  }
  if (needsBoundsOriginUpdate) {
    [cefView_ setBoundsOrigin:targetBoundsOrigin];
  }
  layoutPass_ += 1;
  NSString* cefClass = NSStringFromClass([cefView_ class]);
  NSString* wrapperClass = NSStringFromClass([self class]);
  id currentURL = currentURLString_ ? currentURLString_ : (id)[NSNull null];
  id pageTitle = pageTitle_ ? pageTitle_ : (id)[NSNull null];
  /*
  CDXC:ChromiumBrowserPanes 2026-05-15-09:51:
  Browser panes can visually drift upward while their native host frame remains pinned during split resize.
  Log the wrapper and CEF child NSView geometry at the CEF boundary so the repro can distinguish upstream pane-host movement from internal CEF child-view coordinate drift without changing resize behavior.
  */
  GhostexCEFAppendDiagnosticLog(@"nativeWorkspace.chromiumBrowserPane.cef.layout", @{
    @"cefBoundsAfter": GhostexCEFDescribeRect(cefView_.bounds),
    @"cefBoundsBefore": GhostexCEFDescribeRect(cefBoundsBefore),
    @"cefBoundsInWindowAfter": GhostexCEFDescribeBoundsInWindow(cefView_),
    @"cefBoundsInWindowBefore": cefBoundsInWindowBefore,
    @"cefClass": cefClass ? cefClass : @"",
    @"cefFrameAfter": GhostexCEFDescribeRect(cefView_.frame),
    @"cefFrameBefore": GhostexCEFDescribeRect(cefFrameBefore),
    @"cefFrameInWindowAfter": GhostexCEFDescribeFrameInWindow(cefView_),
    @"cefFrameInWindowBefore": cefFrameInWindowBefore,
    @"cefHidden": @(cefView_.hidden),
    @"cefWantsLayer": @(cefView_.wantsLayer),
    @"currentURL": currentURL,
    @"layoutPass": @(layoutPass_),
    @"needsBoundsOriginUpdate": @(needsBoundsOriginUpdate),
    @"needsFrameUpdate": @(needsFrameUpdate),
    @"pageTitle": pageTitle,
    @"reason": reason ? reason : @"",
    @"targetFrame": GhostexCEFDescribeRect(targetFrame),
    @"wrapperBounds": GhostexCEFDescribeRect(self.bounds),
    @"wrapperBoundsInWindow": GhostexCEFDescribeBoundsInWindow(self),
    @"wrapperClass": wrapperClass ? wrapperClass : @"",
    @"wrapperFlipped": @([self isFlipped]),
    @"wrapperFrame": GhostexCEFDescribeRect(self.frame),
    @"wrapperFrameInWindow": GhostexCEFDescribeFrameInWindow(self),
    @"wrapperWantsLayer": @(self.wantsLayer),
    @"windowNumber": self.window ? @(self.window.windowNumber) : (id)[NSNull null],
  });
  [self ghostexCEFEmitTargetedResizeDiagnosticsIfNeededAt:[[NSProcessInfo processInfo] systemUptime]
                                               layoutPass:layoutPass_
                                             currentFrame:targetFrame
                                               currentURL:currentURL
                                                pageTitle:pageTitle];
}

- (void)ghostexCEFAppendInternalGeometryDiagnosticForLayoutPass:(NSUInteger)layoutPass
                                                  currentFrame:(NSRect)currentFrame
                                                    currentURL:(id)currentURL
                                                     pageTitle:(id)pageTitle
                                                        reason:(NSString*)reason
                                                         phase:(NSString*)phase {
  if (!cefView_ || !GhostexCEFNativeDebugLoggingEnabled()) {
    return;
  }
  internalGeometryDiagnosticSequence_ += 1;
  NSMutableArray* cefViewHierarchy = [NSMutableArray array];
  GhostexCEFCollectViewHierarchy(cefView_, cefViewHierarchy, 0, 5, 80, @"0");
  NSMutableArray* cefLayerHierarchy = [NSMutableArray array];
  if (cefView_.layer) {
    GhostexCEFCollectLayerHierarchy(cefView_.layer, cefLayerHierarchy, 0, 5, 120, @"0");
  }
  /*
  CDXC:ChromiumBrowserPanes 2026-05-15-15:43:
  The first resize repro showed wrapper and top-level CEF frames staying aligned while the rendered page appeared offset.
  Add targeted Debugging Mode diagnostics for CEF's internal NSView/CALayer tree and the ancestor flip chain so the fix can target the exact Cocoa or compositor layer that moves.

  CDXC:ChromiumBrowserPanes 2026-05-16-15:48:
  Toolbar button clicks can reproduce the same upward CEF content drift without resizing the pane, while a full reload restores the content position.
  Reuse the internal geometry snapshot on demand around browser-toolbar actions so the repro can distinguish button hit/focus effects from resize-only layout effects.
  */
  GhostexCEFAppendDiagnosticLog(@"nativeWorkspace.chromiumBrowserPane.cef.internalGeometry", @{
    @"ancestorChain": GhostexCEFCollectAncestorChain(cefView_, 16),
    @"cefLayerHierarchy": cefLayerHierarchy,
    @"cefSubviewHierarchy": cefViewHierarchy,
    @"currentURL": currentURL,
    @"diagnosticPhase": phase ?: @"",
    @"diagnosticReason": reason ?: @"",
    @"diagnosticSequence": @(internalGeometryDiagnosticSequence_),
    @"layoutPass": @(layoutPass),
    @"pageTitle": pageTitle,
    @"windowNumber": self.window ? @(self.window.windowNumber) : (id)[NSNull null],
    @"wrapperClass": NSStringFromClass([self class]) ?: @"",
    @"wrapperFrame": GhostexCEFDescribeRect(self.frame),
    @"wrapperFrameInWindow": GhostexCEFDescribeFrameInWindow(self),
  });
}

- (void)ghostexCEFEmitTargetedResizeDiagnosticsIfNeededAt:(NSTimeInterval)now
                                               layoutPass:(NSUInteger)layoutPass
                                             currentFrame:(NSRect)currentFrame
                                               currentURL:(id)currentURL
                                                pageTitle:(id)pageTitle {
  BOOL shouldEmitInternalGeometry = lastInternalGeometryDiagnosticAt_ <= 0 ||
    now - lastInternalGeometryDiagnosticAt_ >= 0.25 ||
    GhostexCEFDiagnosticRectChangedEnough(lastInternalGeometryDiagnosticBounds_, currentFrame, 48);
  if (shouldEmitInternalGeometry) {
    lastInternalGeometryDiagnosticAt_ = now;
    lastInternalGeometryDiagnosticBounds_ = currentFrame;
    [self ghostexCEFAppendInternalGeometryDiagnosticForLayoutPass:layoutPass
                                                    currentFrame:currentFrame
                                                      currentURL:currentURL
                                                       pageTitle:pageTitle
                                                          reason:@"layout"
                                                           phase:@"resize"];
  }

  BOOL shouldEmitViewport = browser_ &&
    (lastViewportDiagnosticAt_ <= 0 ||
      now - lastViewportDiagnosticAt_ >= 0.25 ||
      GhostexCEFDiagnosticRectChangedEnough(lastViewportDiagnosticBounds_, currentFrame, 64));
  if (shouldEmitViewport) {
    lastViewportDiagnosticAt_ = now;
    lastViewportDiagnosticBounds_ = currentFrame;
    viewportDiagnosticSequence_ += 1;
    [self ghostexCEFEmitPageViewportDiagnosticForLayoutPass:layoutPass
                                                   sequence:viewportDiagnosticSequence_
                                               currentFrame:currentFrame
                                                     reason:@"layout"
                                                      phase:@"resize"];
  }
}

- (void)ghostexCEFEmitPageViewportDiagnosticForLayoutPass:(NSUInteger)layoutPass
                                                 sequence:(NSUInteger)sequence
                                             currentFrame:(NSRect)currentFrame
                                                   reason:(NSString*)reason
                                                    phase:(NSString*)phase {
  if (!browser_ || !GhostexCEFNativeDebugLoggingEnabled()) {
    return;
  }
  CefRefPtr<CefFrame> frame = browser_->GetMainFrame();
  if (!frame) {
    return;
  }
  NSString* reasonLiteral = GhostexCEFJSONStringLiteral(reason ?: @"");
  NSString* phaseLiteral = GhostexCEFJSONStringLiteral(phase ?: @"");
  NSString* script = [NSString stringWithFormat:
    @"(() => {"
      "const prefix = '%@';"
      "const diagnosticReason = %@;"
      "const diagnosticPhase = %@;"
      "const number = (value) => Number.isFinite(value) ? value : null;"
      "const rectOf = (rect) => rect ? ({"
        "x: number(rect.x), y: number(rect.y), width: number(rect.width), height: number(rect.height),"
        "top: number(rect.top), right: number(rect.right), bottom: number(rect.bottom), left: number(rect.left)"
      "}) : null;"
      "const describe = (el) => {"
        "if (!el) return null;"
        "const rect = el.getBoundingClientRect ? el.getBoundingClientRect() : null;"
        "let className = '';"
        "try { className = typeof el.className === 'string' ? el.className : String(el.className || ''); } catch (_) {}"
        "return {"
          "tagName: el.tagName || null,"
          "id: el.id || '',"
          "className: className.slice(0, 180),"
          "rect: rectOf(rect),"
          "scrollTop: number(el.scrollTop), scrollLeft: number(el.scrollLeft),"
          "clientWidth: number(el.clientWidth), clientHeight: number(el.clientHeight),"
          "scrollWidth: number(el.scrollWidth), scrollHeight: number(el.scrollHeight),"
          "offsetTop: number(el.offsetTop), offsetLeft: number(el.offsetLeft)"
        "};"
      "};"
      "try {"
        "const doc = document;"
        "const docEl = doc.documentElement;"
        "const body = doc.body;"
        "const scrollingElement = doc.scrollingElement;"
        "const visualViewport = window.visualViewport;"
        "const sampleX = Math.max(0, Math.min(Math.max(0, window.innerWidth - 1), Math.floor(window.innerWidth / 2)));"
        "const topY = Math.max(0, Math.min(Math.max(0, window.innerHeight - 1), 1));"
        "const midY = Math.max(0, Math.min(Math.max(0, window.innerHeight - 1), Math.floor(window.innerHeight / 2)));"
        "const payload = {"
          "layoutPass: %lu,"
          "diagnosticSequence: %lu,"
          "diagnosticReason: diagnosticReason,"
          "diagnosticPhase: diagnosticPhase,"
          "nativeFrame: {x: %.3f, y: %.3f, width: %.3f, height: %.3f},"
          "href: String(location.href),"
          "readyState: doc.readyState,"
          "devicePixelRatio: number(window.devicePixelRatio),"
          "innerWidth: number(window.innerWidth), innerHeight: number(window.innerHeight),"
          "outerWidth: number(window.outerWidth), outerHeight: number(window.outerHeight),"
          "scrollX: number(window.scrollX), scrollY: number(window.scrollY),"
          "pageXOffset: number(window.pageXOffset), pageYOffset: number(window.pageYOffset),"
          "visualViewport: visualViewport ? {"
            "offsetLeft: number(visualViewport.offsetLeft), offsetTop: number(visualViewport.offsetTop),"
            "pageLeft: number(visualViewport.pageLeft), pageTop: number(visualViewport.pageTop),"
            "width: number(visualViewport.width), height: number(visualViewport.height),"
            "scale: number(visualViewport.scale)"
          "} : null,"
          "documentElement: describe(docEl),"
          "body: describe(body),"
          "scrollingElementTag: scrollingElement ? scrollingElement.tagName : null,"
          "scrollingElement: describe(scrollingElement),"
          "activeElement: describe(doc.activeElement),"
          "centerTopElement: describe(doc.elementFromPoint(sampleX, topY)),"
          "centerMiddleElement: describe(doc.elementFromPoint(sampleX, midY)),"
          "performanceNow: performance && performance.now ? number(performance.now()) : null"
        "};"
        "console.info(prefix + JSON.stringify(payload));"
      "} catch (error) {"
        "console.info(prefix + JSON.stringify({"
          "layoutPass: %lu,"
          "diagnosticSequence: %lu,"
          "diagnosticReason: diagnosticReason,"
          "diagnosticPhase: diagnosticPhase,"
          "nativeFrame: {x: %.3f, y: %.3f, width: %.3f, height: %.3f},"
          "error: String(error && (error.stack || error.message || error))"
        "}));"
      "}"
    "})();",
    GhostexCEFViewportDiagnosticConsolePrefix,
    reasonLiteral,
    phaseLiteral,
    (unsigned long)layoutPass,
    (unsigned long)sequence,
    currentFrame.origin.x,
    currentFrame.origin.y,
    currentFrame.size.width,
    currentFrame.size.height,
    (unsigned long)layoutPass,
    (unsigned long)sequence,
    currentFrame.origin.x,
    currentFrame.origin.y,
    currentFrame.size.width,
    currentFrame.size.height];
  frame->ExecuteJavaScript(CefString([script UTF8String]), frame->GetURL(), 0);
}

- (NSString*)currentURLString {
  return currentURLString_;
}

- (NSString*)pageTitle {
  return pageTitle_;
}

- (NSInteger)browserIdentifier {
  /**
   CDXC:TitlebarResources 2026-05-17-01:25:
   The Resources dropdown needs to group Chromium renderer processes under the
   visible Browser tab or project editor view. Expose CEF's browser identifier
   so Swift can correlate renderer `--renderer-client-id` process arguments with the
   title and URL already tracked by GhostexCEFBrowserView.
   */
  return browser_ ? browser_->GetIdentifier() : -1;
}

- (BOOL)canGoBack {
  return canGoBack_;
}

- (BOOL)canGoForward {
  return canGoForward_;
}

- (BOOL)isLoading {
  return isLoading_;
}

- (double)zoomLevel {
  return browser_ ? browser_->GetHost()->GetZoomLevel() : 0.0;
}

- (void)createBrowserIfNeeded {
  if (didCreateBrowser_ || !g_cefInitialized) {
    return;
  }
  didCreateBrowser_ = YES;

  CefWindowInfo windowInfo;
  /**
   CDXC:ChromiumBrowserPanes 2026-05-07-05:18
   code-server panes need VS Code's live drag indicators during in-page view
   movement. CEF's Chrome runtime cannot be used for ghostex's embedded child
   NSView panes because CEF forces Alloy style whenever `parent_view` is set;
   keep child panes on Alloy and let TerminalWorkspaceView handle only the
   active-drag hover/drop retargeting gap.
  */
  windowInfo.runtime_style = CEF_RUNTIME_STYLE_ALLOY;
  CefRect rect(0, 0, static_cast<int>(MAX(1, self.bounds.size.width)), static_cast<int>(MAX(1, self.bounds.size.height)));
  windowInfo.SetAsChild((__bridge void*)self, rect);

  CefBrowserSettings browserSettings;
  browserSettings.background_color = CefColorSetARGB(255, 22, 22, 22);
  if (self.trustedClipboardOrigin.length > 0) {
    /*
    CDXC:EditorClipboard 2026-05-14-10:08:
    Embedded VS Code runs in CEF Alloy, whose default permission handling ignores browser clipboard prompts.
    Project editor panes must grant JavaScript clipboard access only for their owned code-server origin so Explorer file copy can read/write VS Code's browser clipboard without enabling clipboard access for arbitrary Chromium browser panes.
    */
    browserSettings.javascript_access_clipboard = STATE_ENABLED;
    browserSettings.javascript_dom_paste = STATE_ENABLED;
  }

  CefRefPtr<CefRequestContext> requestContext = GhostexCEFRequestContextForProfile(profileIdentifier_);
  if (self.trustedClipboardOrigin.length > 0) {
    GhostexCEFGrantTrustedClipboardContentSetting(requestContext, self.trustedClipboardOrigin);
  }

  client_ = new GhostexCEFBrowserClient(self);
  browser_ = CefBrowserHost::CreateBrowserSync(
    windowInfo,
    client_,
    CefString("about:blank"),
    browserSettings,
    nullptr,
    requestContext);
  if (!browser_) {
    return;
  }

  CefWindowHandle handle = browser_->GetHost()->GetWindowHandle();
  cefView_ = (__bridge NSView*)handle;
  cefView_.autoresizingMask = NSViewNotSizable;
  [self ghostexCEFPinHostedViewToBoundsWithReason:@"createBrowser"];
  [self setNeedsLayout:YES];

  if (initialURL_.length > 0) {
    [self loadURLString:initialURL_];
  }
}

- (void)loadURLString:(NSString*)urlString {
  currentURLString_ = [urlString copy];
  if (self.urlChangedHandler) {
    self.urlChangedHandler(currentURLString_);
  }
  if (browser_ && urlString.length > 0) {
    browser_->GetMainFrame()->LoadURL(CefString([urlString UTF8String]));
  }
}

- (void)goBack {
  if (browser_ && browser_->CanGoBack()) {
    browser_->GoBack();
  }
}

- (void)goForward {
  if (browser_ && browser_->CanGoForward()) {
    browser_->GoForward();
  }
}

- (void)reload {
  if (browser_) {
    browser_->Reload();
  }
}

- (void)stopLoading {
  if (browser_) {
    browser_->StopLoad();
  }
}

- (void)zoomIn {
  if (!browser_) {
    return;
  }
  /*
   CDXC:ChromiumBrowserPanes 2026-06-10-15:47:
   Browser zoom shortcuts must use Chromium's own focused-browser zoom command instead of CSS or JavaScript scaling. This keeps Cmd+=, Cmd+-, and Cmd+0 aligned with Chrome behavior while leaving any same-site persistence to CEF's profile storage.
   */
  browser_->GetHost()->Zoom(CEF_ZOOM_COMMAND_IN);
}

- (void)zoomOut {
  if (!browser_) {
    return;
  }
  browser_->GetHost()->Zoom(CEF_ZOOM_COMMAND_OUT);
}

- (void)resetZoom {
  if (!browser_) {
    return;
  }
  browser_->GetHost()->Zoom(CEF_ZOOM_COMMAND_RESET);
}

- (void)findText:(NSString*)searchText forward:(BOOL)forward findNext:(BOOL)findNext {
  if (!browser_) {
    return;
  }
  NSString* text = searchText ?: @"";
  if (text.length == 0) {
    browser_->GetHost()->StopFinding(true);
    [self ghostexCEFHandleFindResultWithCount:0 activeMatchOrdinal:0 finalUpdate:YES];
    return;
  }
  /**
   CDXC:BrowserSearch 2026-06-13-00:00:
   Cmd+F in an embedded CEF pane should use Chromium page search, not the terminal or app-wide find path.
   Route the native find bar through CEF's Find API so matches, selection, and next/previous navigation stay inside the focused browser renderer.
   */
  browser_->GetHost()->Find(CefString([text UTF8String]), forward, false, findNext);
}

- (void)stopFindingWithClearSelection:(BOOL)clearSelection {
  if (browser_) {
    browser_->GetHost()->StopFinding(clearSelection);
  }
  [self ghostexCEFHandleFindResultWithCount:0 activeMatchOrdinal:0 finalUpdate:YES];
}

- (void)executeJavaScript:(NSString*)javaScript {
  if (!browser_ || javaScript.length == 0) {
    return;
  }
  CefRefPtr<CefFrame> frame = browser_->GetMainFrame();
  if (frame) {
    frame->ExecuteJavaScript(CefString([javaScript UTF8String]), frame->GetURL(), 0);
  }
}

- (void)emitToolbarActionDiagnosticsWithAction:(NSString*)action phase:(NSString*)phase {
  if (!GhostexCEFNativeDebugLoggingEnabled()) {
    return;
  }
  NSString* reason = action.length > 0
    ? [NSString stringWithFormat:@"toolbar.%@", action]
    : @"toolbar";
  id currentURL = currentURLString_ ? currentURLString_ : (id)[NSNull null];
  id pageTitle = pageTitle_ ? pageTitle_ : (id)[NSNull null];
  NSRect currentFrame = self.bounds;
  GhostexCEFAppendDiagnosticLog(@"nativeWorkspace.chromiumBrowserPane.cef.toolbarAction", @{
    @"action": action ?: @"",
    @"cefBounds": cefView_ ? GhostexCEFDescribeRect(cefView_.bounds) : (id)[NSNull null],
    @"cefFrame": cefView_ ? GhostexCEFDescribeRect(cefView_.frame) : (id)[NSNull null],
    @"cefFrameInWindow": cefView_ ? GhostexCEFDescribeFrameInWindow(cefView_) : (id)[NSNull null],
    @"currentURL": currentURL,
    @"hasBrowser": @(browser_ != nullptr),
    @"hasCEFView": @(cefView_ != nil),
    @"layoutPass": @(layoutPass_),
    @"pageTitle": pageTitle,
    @"phase": phase ?: @"",
    @"wrapperBounds": GhostexCEFDescribeRect(self.bounds),
    @"wrapperFrame": GhostexCEFDescribeRect(self.frame),
    @"wrapperFrameInWindow": GhostexCEFDescribeFrameInWindow(self),
    @"windowNumber": self.window ? @(self.window.windowNumber) : (id)[NSNull null],
  });
  [self ghostexCEFAppendInternalGeometryDiagnosticForLayoutPass:layoutPass_
                                                  currentFrame:currentFrame
                                                    currentURL:currentURL
                                                     pageTitle:pageTitle
                                                        reason:reason
                                                         phase:phase ?: @""];
  viewportDiagnosticSequence_ += 1;
  [self ghostexCEFEmitPageViewportDiagnosticForLayoutPass:layoutPass_
                                                 sequence:viewportDiagnosticSequence_
                                             currentFrame:currentFrame
                                                   reason:reason
                                                    phase:phase ?: @""];
}

- (void)completeCurrentDragAtWindowPoint:(NSPoint)windowPoint {
  if (!browser_) {
    return;
  }
  NSPoint localPoint = [self convertPoint:windowPoint fromView:nil];
  if (!NSPointInRect(localPoint, self.bounds)) {
    return;
  }
  /**
   CDXC:ChromiumBrowserPanes 2026-05-07-05:18
   Embedded CEF panes must keep Chromium's live drag/drop target feedback. When
   macOS does not deliver CEF's final source-ended callback for an in-page drag,
   complete the native CEF drag source after TerminalWorkspaceView has sent any
   scoped in-page hover/drop retargeting required for VS Code's DnD state.
   */
  browser_->GetHost()->DragSourceEndedAt(
    static_cast<int>(std::round(localPoint.x)),
    static_cast<int>(std::round(localPoint.y)),
    DRAG_OPERATION_MOVE);
  browser_->GetHost()->DragSourceSystemDragEnded();
}

- (void)toggleDevTools {
  if (browser_ && client_) {
    client_->ToggleRemoteDevTools(browser_);
  }
}

- (void)closeBrowser {
  if (client_) {
    client_->CloseRemoteDevTools();
  }
  if (browser_) {
    /**
     CDXC:ChromiumBrowserPanes 2026-05-07-07:31
     Sidebar middle-click closes only the embedded browser pane. CEF Alloy's
     default DoClose path sends `performClose:` to the top-level NSWindow when
     CloseBrowser(false) is allowed to fall through, which made the app quit at
     2026-05-07 07:08:56. Keep the app-owned close flag on the CEF client
     because DoClose can arrive after Swift has removed the view owner; DoClose
     must still suppress the top-level window close and remove only this pane.
     */
    client_->MarkClosingFromGhostex();
    browser_->GetHost()->CloseBrowser(true);
    browser_ = nullptr;
  }
  if (cefView_) {
    [cefView_ removeFromSuperview];
    cefView_ = nil;
  }
}

- (void)dealloc {
  [self closeBrowser];
}

- (void)ghostexCEFSetTitle:(NSString*)title {
  pageTitle_ = [title copy];
  if (self.titleChangedHandler) {
    self.titleChangedHandler(pageTitle_ ?: @"");
  }
}

- (void)ghostexCEFSetURL:(NSString*)url {
  currentURLString_ = [url copy];
  if (self.urlChangedHandler) {
    self.urlChangedHandler(currentURLString_ ?: @"");
  }
}

- (void)ghostexCEFHandleFindResultWithCount:(NSInteger)count
                         activeMatchOrdinal:(NSInteger)activeMatchOrdinal
                                finalUpdate:(BOOL)finalUpdate {
  if (self.findResultHandler) {
    self.findResultHandler(count, activeMatchOrdinal, finalUpdate);
  }
}

- (void)ghostexCEFSetFaviconURL:(NSString*)url {
  if (self.faviconURLChangedHandler) {
    self.faviconURLChangedHandler(url ?: @"");
  }
}

- (void)ghostexCEFSetLoading:(BOOL)isLoading canGoBack:(BOOL)canGoBack canGoForward:(BOOL)canGoForward {
  isLoading_ = isLoading;
  canGoBack_ = canGoBack;
  canGoForward_ = canGoForward;
  if (self.navigationStateChangedHandler) {
    self.navigationStateChangedHandler(canGoBack_, canGoForward_, isLoading_);
  }
}

- (void)ghostexCEFHandleLoadEvent:(NSString*)event
                              url:(NSString*)url
                   httpStatusCode:(NSInteger)httpStatusCode
                         errorCode:(NSInteger)errorCode
                         errorText:(NSString*)errorText {
  /*
  CDXC:EditorPanes 2026-05-15-18:53:
  The code editor reconnect/page-not-found repro needs CEF main-frame load boundaries separate from VS Code console messages and the code-server process exit log.
  Forward load start, HTTP completion, and load error state to Swift diagnostics without changing navigation behavior.
  */
  if (self.loadEventHandler) {
    self.loadEventHandler(
      event ?: @"",
      url ?: @"",
      httpStatusCode,
      errorCode,
      errorText ?: @"");
  }
}

- (void)ghostexCEFHandleConsoleMessage:(NSString*)message source:(NSString*)source line:(NSInteger)line {
  if ([message hasPrefix:GhostexCEFViewportDiagnosticConsolePrefix]) {
    NSString* json = [message substringFromIndex:GhostexCEFViewportDiagnosticConsolePrefix.length];
    NSData* data = [json dataUsingEncoding:NSUTF8StringEncoding];
    NSMutableDictionary* details = [NSMutableDictionary dictionary];
    if (data) {
      NSError* error = nil;
      id payload = [NSJSONSerialization JSONObjectWithData:data options:0 error:&error];
      if (!error && [payload isKindOfClass:[NSDictionary class]]) {
        [details addEntriesFromDictionary:(NSDictionary*)payload];
      } else if (error) {
        details[@"parseError"] = error.localizedDescription ?: @"unknown";
        details[@"rawMessage"] = json ?: @"";
      }
    }
    /*
    CDXC:ChromiumBrowserPanes 2026-05-15-15:43:
    Page-scroll drift must be separated from native CEF compositor drift before changing resize behavior.
    Capture document, visualViewport, and scrollingElement metrics through the page console bridge so a resize repro proves whether Chromium moved the web document or only its native backing surface.
    */
    details[@"currentURL"] = currentURLString_ ?: (id)[NSNull null];
    details[@"pageTitle"] = pageTitle_ ?: (id)[NSNull null];
    details[@"source"] = source ?: @"";
    details[@"line"] = @(line);
    GhostexCEFAppendDiagnosticLog(@"nativeWorkspace.chromiumBrowserPane.pageViewport", details);
    return;
  }
  if (self.consoleMessageHandler) {
    self.consoleMessageHandler(message ?: @"", source ?: @"", line);
  }
}

@end

void GhostexCEFBrowserClient::OnTitleChange(CefRefPtr<CefBrowser> browser, const CefString& title) {
  lastTitle_ = title.ToString();
  NSString* titleString = StringFromCefString(title);
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFSetTitle:titleString];
  });
}

void GhostexCEFBrowserClient::OnAddressChange(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, const CefString& url) {
  if (!frame->IsMain()) {
    return;
  }
  NSString* urlString = StringFromCefString(url);
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFSetURL:urlString];
  });
}

void GhostexCEFBrowserClient::OnFaviconURLChange(CefRefPtr<CefBrowser> browser, const std::vector<CefString>& icon_urls) {
  if (icon_urls.empty()) {
    return;
  }
  NSString* faviconURL = StringFromCefString(icon_urls.front());
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFSetFaviconURL:faviconURL];
  });
}

void GhostexCEFBrowserClient::OnLoadStart(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, TransitionType transition_type) {
  if (!frame || !frame->IsMain()) {
    return;
  }
  NSString* urlString = StringFromCefString(frame->GetURL());
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFHandleLoadEvent:@"loadStart"
                                  url:urlString
                       httpStatusCode:0
                            errorCode:0
                            errorText:@""];
  });
}

void GhostexCEFBrowserClient::OnLoadEnd(CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int httpStatusCode) {
  if (!frame || !frame->IsMain()) {
    return;
  }
  NSString* urlString = StringFromCefString(frame->GetURL());
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFHandleLoadEvent:@"loadEnd"
                                  url:urlString
                       httpStatusCode:httpStatusCode
                            errorCode:0
                            errorText:@""];
  });
}

void GhostexCEFBrowserClient::OnLoadError(CefRefPtr<CefBrowser> browser,
                                       CefRefPtr<CefFrame> frame,
                                       ErrorCode errorCode,
                                       const CefString& errorText,
                                       const CefString& failedUrl) {
  if (!frame || !frame->IsMain()) {
    return;
  }
  NSString* urlString = StringFromCefString(failedUrl);
  NSString* errorTextString = StringFromCefString(errorText);
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFHandleLoadEvent:@"loadError"
                                  url:urlString
                       httpStatusCode:0
                            errorCode:static_cast<NSInteger>(errorCode)
                            errorText:errorTextString];
  });
}

void GhostexCEFBrowserClient::OnLoadingStateChange(CefRefPtr<CefBrowser> browser, bool isLoading, bool canGoBack, bool canGoForward) {
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFSetLoading:isLoading canGoBack:canGoBack canGoForward:canGoForward];
  });
}

bool GhostexCEFBrowserClient::OnConsoleMessage(CefRefPtr<CefBrowser> browser,
                                            cef_log_severity_t level,
                                            const CefString& message,
                                            const CefString& source,
                                            int line) {
  /**
   CDXC:ChromiumBrowserPanes 2026-05-07-05:18
   CEF console messages remain forwarded for browser/editor diagnostics, but
   ghostex does not install persistent page-level drag diagnostics into code-server.
   Drag behavior comes from Chromium plus the scoped native hover/drop bridge
   used only during active CEF drags.
   */
  NSString* messageString = StringFromCefString(message);
  NSString* sourceString = StringFromCefString(source);
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner_ ghostexCEFHandleConsoleMessage:messageString source:sourceString line:line];
  });
  return false;
}

void GhostexCEFBrowserClient::OnFindResult(CefRefPtr<CefBrowser> browser,
                                           int identifier,
                                           int count,
                                           const CefRect& selectionRect,
                                           int activeMatchOrdinal,
                                           bool finalUpdate) {
  GhostexCEFBrowserView* owner = owner_;
  if (!owner) {
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    [owner ghostexCEFHandleFindResultWithCount:count
                            activeMatchOrdinal:activeMatchOrdinal
                                   finalUpdate:finalUpdate];
  });
}

void GhostexCEFBrowserClient::OnAfterCreated(CefRefPtr<CefBrowser> browser) {}

bool GhostexCEFBrowserClient::DoClose(CefRefPtr<CefBrowser> browser) {
  if (closingFromGhostex_) {
    return true;
  }
  return false;
}

void GhostexCEFBrowserClient::OnBeforeClose(CefRefPtr<CefBrowser> browser) {
  CloseRemoteDevTools();
}

void GhostexCEFBrowserClient::MarkClosingFromGhostex() {
  closingFromGhostex_ = true;
}

bool GhostexCEFBrowserClient::OpenRequestedURLInGhostexTab(const CefString& target_url) {
  std::string url = target_url.ToString();
  GhostexCEFBrowserView* owner = owner_;
  if (url.empty() || !owner || !owner.newWindowRequestedHandler) {
    return false;
  }
  /**
   CDXC:BrowserTabs 2026-06-13-00:00:
   CEF middle-click, target-blank, and context-menu "Open Link in New Tab/Window" should become a Ghostex tab in the current surface.
   Dispatch the link intent to Swift instead of letting Chromium create an unmanaged native window or replacing the current page.
   */
  NSString* requestedURL = StringFromCefString(target_url);
  dispatch_async(dispatch_get_main_queue(), ^{
    if (owner.newWindowRequestedHandler) {
      owner.newWindowRequestedHandler(requestedURL);
    }
  });
  return true;
}

bool GhostexCEFBrowserClient::OnBeforePopup(CefRefPtr<CefBrowser> browser,
                                         CefRefPtr<CefFrame> frame,
                                         int popup_id,
                                         const CefString& target_url,
                                         const CefString& target_frame_name,
                                         CefLifeSpanHandler::WindowOpenDisposition target_disposition,
                                         bool user_gesture,
                                         const CefPopupFeatures& popupFeatures,
                                         CefWindowInfo& windowInfo,
                                         CefRefPtr<CefClient>& client,
                                         CefBrowserSettings& settings,
                                         CefRefPtr<CefDictionaryValue>& extra_info,
                                         bool* no_javascript_access) {
  std::string url = target_url.ToString();
  if (OpenRequestedURLInGhostexTab(target_url)) {
    return true;
  }
  if (browser && !url.empty()) {
    browser->GetMainFrame()->LoadURL(target_url);
  }
  return true;
}

void GhostexCEFBrowserClient::OnBeforeContextMenu(CefRefPtr<CefBrowser> browser,
                                               CefRefPtr<CefFrame> frame,
                                               CefRefPtr<CefContextMenuParams> params,
                                               CefRefPtr<CefMenuModel> model) {
  if (model->GetCount() > 0) {
    model->AddSeparator();
  }
  model->AddItem(kInspectElementCommandId, "Inspect Element");
}

bool GhostexCEFBrowserClient::OnContextMenuCommand(CefRefPtr<CefBrowser> browser,
                                                CefRefPtr<CefFrame> frame,
                                                CefRefPtr<CefContextMenuParams> params,
                                                int command_id,
                                                CefContextMenuHandler::EventFlags event_flags) {
  if (command_id == kInspectElementCommandId) {
    OpenRemoteDevToolsFrontend(browser);
    return true;
  }
  if (command_id == IDC_CONTENT_CONTEXT_OPENLINKNEWTAB ||
      command_id == IDC_CONTENT_CONTEXT_OPENLINKNEWWINDOW) {
    CefString linkURL;
    if (params) {
      linkURL = params->GetUnfilteredLinkUrl();
      if (linkURL.empty()) {
        linkURL = params->GetLinkUrl();
      }
    }
    if (OpenRequestedURLInGhostexTab(linkURL)) {
      return true;
    }
  }
  return false;
}

bool GhostexCEFBrowserClient::OnShowPermissionPrompt(CefRefPtr<CefBrowser> browser,
                                                  uint64_t prompt_id,
                                                  const CefString& requesting_origin,
                                                  uint32_t requested_permissions,
                                                  CefRefPtr<CefPermissionPromptCallback> callback) {
  constexpr uint32_t clipboardPermission = static_cast<uint32_t>(CEF_PERMISSION_TYPE_CLIPBOARD);
  if ((requested_permissions & clipboardPermission) == 0) {
    return false;
  }

  GhostexCEFBrowserView* owner = owner_;
  NSString* trustedOrigin = owner.trustedClipboardOrigin;
  NSString* requestingOrigin = StringFromCefString(requesting_origin);
  uint32_t unsupportedPermissions = requested_permissions & ~clipboardPermission;
  bool shouldAccept = unsupportedPermissions == 0 && GhostexCEFOriginsMatch(requestingOrigin, trustedOrigin);
  callback->Continue(shouldAccept ? CEF_PERMISSION_RESULT_ACCEPT : CEF_PERMISSION_RESULT_DENY);
  return true;
}

void GhostexCEFBrowserClient::ToggleRemoteDevTools(CefRefPtr<CefBrowser> browser) {
  if (devToolsOpen_) {
    CloseRemoteDevTools();
    return;
  }
  OpenRemoteDevToolsFrontend(browser);
}

void GhostexCEFBrowserClient::CloseRemoteDevTools() {
  devToolsOpen_ = false;
  if (devToolsWindow_) {
    [devToolsWindow_ orderOut:nil];
  }
}

void GhostexCEFBrowserClient::OpenRemoteDevToolsFrontend(CefRefPtr<CefBrowser> browser) {
  NSString* baseURL = [NSString stringWithFormat:@"http://127.0.0.1:%d", g_remoteDebuggingPort];
  NSURL* targetsURL = [NSURL URLWithString:[baseURL stringByAppendingString:@"/json"]];
  NSString* currentURL = nil;
  if (browser && browser->GetMainFrame()) {
    currentURL = StringFromCefString(browser->GetMainFrame()->GetURL());
  }
  NSString* title = lastTitle_.empty() ? nil : [NSString stringWithUTF8String:lastTitle_.c_str()];

  NSURLSessionDataTask* task = [[NSURLSession sharedSession]
    dataTaskWithURL:targetsURL
  completionHandler:^(NSData* data, NSURLResponse* response, NSError* error) {
    if (error || !data) {
      NSLog(@"[CEF] Remote DevTools target lookup failed: %@", GhostexCEFRedactSensitiveText([error description]));
      return;
    }
    NSError* jsonError = nil;
    id json = [NSJSONSerialization JSONObjectWithData:data options:0 error:&jsonError];
    if (jsonError || ![json isKindOfClass:[NSArray class]]) {
      NSLog(@"[CEF] Remote DevTools target JSON was invalid: %@", GhostexCEFRedactSensitiveText([jsonError description]));
      return;
    }
    NSDictionary* selected = nil;
    for (NSDictionary* item in (NSArray*)json) {
      NSString* itemURL = item[@"url"];
      NSString* itemTitle = item[@"title"];
      if ((currentURL && [itemURL isKindOfClass:[NSString class]] && [itemURL isEqualToString:currentURL]) ||
          (title && [itemTitle isKindOfClass:[NSString class]] && [itemTitle isEqualToString:title])) {
        selected = item;
        break;
      }
    }
    if (!selected && [(NSArray*)json count] > 0) {
      selected = [(NSArray*)json firstObject];
    }
    NSString* webSocketURL = selected[@"webSocketDebuggerUrl"];
    if (![webSocketURL isKindOfClass:[NSString class]]) {
      NSLog(@"[CEF] Remote DevTools target did not include a webSocketDebuggerUrl");
      return;
    }
    NSString* frontendURL = [NSString stringWithFormat:@"%@/devtools/inspector.html?ws=%@&dockSide=undocked",
                                                       baseURL,
                                                       EscapeDevToolsWebSocketURL(webSocketURL)];
    dispatch_async(dispatch_get_main_queue(), ^{
      CreateRemoteDevToolsWindow(frontendURL);
    });
  }];
  [task resume];
}

void GhostexCEFBrowserClient::CreateRemoteDevToolsWindow(NSString* frontendURL) {
  if (!devToolsWindow_) {
    NSRect frame = NSMakeRect(160, 160, 1120, 820);
    devToolsWindow_ = [[NSWindow alloc] initWithContentRect:frame
                                                  styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable | NSWindowStyleMaskMiniaturizable
                                                    backing:NSBackingStoreBuffered
                                                      defer:NO];
    [devToolsWindow_ setTitle:@"Chromium DevTools"];
    [devToolsWindow_ setReleasedWhenClosed:NO];
  }
  [devToolsWindow_ makeKeyAndOrderFront:nil];
  devToolsOpen_ = true;

  if (!devToolsClient_) {
    CefRefPtr<GhostexCEFBrowserClient> selfRef(this);
    devToolsClient_ = new GhostexRemoteDevToolsClient([selfRef]() {
      selfRef->CloseRemoteDevTools();
    });
  }
  if (devToolsBrowser_) {
    devToolsBrowser_->GetMainFrame()->LoadURL(CefString([frontendURL UTF8String]));
    return;
  }
  NSView* contentView = [devToolsWindow_ contentView];
  CefWindowInfo windowInfo;
  windowInfo.runtime_style = CEF_RUNTIME_STYLE_ALLOY;
  windowInfo.SetAsChild((__bridge void*)contentView, CefRect(0, 0, static_cast<int>(contentView.bounds.size.width), static_cast<int>(contentView.bounds.size.height)));
  CefBrowserSettings settings;
  devToolsBrowser_ = CefBrowserHost::CreateBrowserSync(
    windowInfo,
    devToolsClient_,
    CefString([frontendURL UTF8String]),
    settings,
    nullptr,
    nullptr);
}

bool GhostexCEFPrepareApplication(void) {
  [GhostexCEFApplication sharedApplication];
  return [NSApp isKindOfClass:[GhostexCEFApplication class]];
}

bool GhostexCEFIsRuntimeAvailable(void) {
  return [[NSFileManager defaultManager] fileExistsAtPath:GhostexCEFFrameworkExecutablePath()]
    && [[NSFileManager defaultManager] fileExistsAtPath:GhostexCEFHelperExecutablePath()];
}

bool GhostexCEFInitialize(int argc, char* _Nullable argv[]) {
  if (g_cefInitialized) {
    return true;
  }
  if (![NSApp isKindOfClass:[GhostexCEFApplication class]]) {
    return false;
  }
  if (!GhostexCEFIsRuntimeAvailable()) {
    NSLog(@"[CEF] Runtime not available. Missing framework or helper app.");
    return false;
  }

  CefMainArgs mainArgs(argc, argv);
  g_cefApp = new GhostexCEFApp();

  CefSettings settings;
  settings.no_sandbox = true;
  settings.multi_threaded_message_loop = false;
  settings.windowless_rendering_enabled = false;
  g_remoteDebuggingPort = FindAvailableRemoteDebuggingPort();
  settings.remote_debugging_port = g_remoteDebuggingPort;

  NSString* bundlePath = [[NSBundle mainBundle] bundlePath];
  if (bundlePath) {
    CefString(&settings.main_bundle_path) = [bundlePath UTF8String];
  }
  CefString(&settings.framework_dir_path) = [GhostexCEFFrameworkBundlePath() UTF8String];
  CefString(&settings.browser_subprocess_path) = [GhostexCEFHelperExecutablePath() UTF8String];

  NSString* cachePath = GhostexCEFStorageDirectory();
  [[NSFileManager defaultManager] createDirectoryAtPath:cachePath
                            withIntermediateDirectories:YES
                                             attributes:nil
                                                  error:nil];
  /**
   CDXC:ChromiumBrowserPanes 2026-05-06-01:12
   Chrome embed panes must keep users logged in after ghostex restarts. CEF only
   persists global-profile cookies, session cookies, localStorage, and IndexedDB
   when the app-level cache_path is set; root_cache_path alone stores
   installation data. Use ~/.ghostex[-dev]/cef as both the CEF root and default
   browser profile cache so the built-in profile survives process restarts and
   named custom profile caches can live as direct children under the same
   required root.
   */
  CefString(&settings.cache_path) = [cachePath UTF8String];
  CefString(&settings.root_cache_path) = [cachePath UTF8String];
  settings.persist_session_cookies = true;
  CefString(&settings.log_file) = [[cachePath stringByAppendingPathComponent:@"debug.log"] UTF8String];
  /*
  CDXC:ChromiumBrowserPanes 2026-05-16-07:23:
  CEF may write routine browser diagnostics through its own log file. Keep
  non-error CEF logging behind Settings Debugging Mode while still allowing CEF
  error entries in normal app mode.
  */
  settings.log_severity = GhostexCEFNativeDebugLoggingEnabled()
    ? LOGSEVERITY_DEFAULT
    : LOGSEVERITY_ERROR;
  CefString(&settings.accept_language_list) = "en-US,en";

  if (!CefInitialize(mainArgs, settings, g_cefApp.get(), nullptr)) {
    NSLog(@"[CEF] CefInitialize failed.");
    return false;
  }
  g_cefInitialized = true;
  if (GhostexCEFNativeDebugLoggingEnabled()) {
    NSLog(@"[CEF] Initialized with remote debugging on 127.0.0.1:%d", g_remoteDebuggingPort);
  }
  return true;
}

void GhostexCEFRunMessageLoop(void) {
  CefRunMessageLoop();
}

void GhostexCEFFlushBrowserState(GhostexCEFCompletionBlock completion) {
  auto state = std::make_shared<GhostexCEFProfileFlushState>(completion);
  /**
   CDXC:ChromiumBrowserPanes 2026-05-06-01:12
   Login cookies must be durable when the user quits or restarts ghostex. CEF
   writes web storage with the profile cache_path, but cookie writes can remain
   buffered until the cookie managers are flushed. Flush the global default
   profile and every custom CEF request-context profile before app termination
   continues, so Google/auth cookies survive the next launch.
   */
  if (g_cefInitialized) {
    CefRefPtr<CefRequestContext> globalContext = CefRequestContext::GetGlobalContext();
    if (globalContext) {
      GhostexCEFFlushCookieManager(globalContext->GetCookieManager(nullptr), state);
    }
    for (const auto& entry : g_persistentRequestContexts) {
      if (entry.second) {
        GhostexCEFFlushCookieManager(entry.second->GetCookieManager(nullptr), state);
      }
    }
  }
  if (state->pending.load() == 0 && completion) {
    dispatch_async(dispatch_get_main_queue(), ^{
      completion();
    });
  }
}

void GhostexCEFShutdown(void) {
  if (g_cefInitialized) {
    CefShutdown();
    g_cefInitialized = false;
  }
}

int GhostexCEFRemoteDebuggingPort(void) {
  return g_remoteDebuggingPort;
}
