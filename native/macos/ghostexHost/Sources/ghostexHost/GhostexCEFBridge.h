#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

#ifdef __cplusplus
extern "C" {
#endif

bool GhostexCEFPrepareApplication(void);
bool GhostexCEFIsRuntimeAvailable(void);
bool GhostexCEFInitialize(int argc, char* _Nullable argv[_Nonnull]);
void GhostexCEFRunMessageLoop(void);
void GhostexCEFFlushBrowserState(void (^_Nullable completion)(void));
void GhostexCEFImportCookiesForProfile(
  NSString* profileIdentifier,
  NSArray<NSHTTPCookie*>* cookies,
  void (^_Nullable completion)(NSInteger importedCount));
void GhostexCEFShutdown(void);
int GhostexCEFRemoteDebuggingPort(void);

#ifdef __cplusplus
}
#endif

@interface GhostexCEFBrowserView : NSView

@property(nonatomic, copy, nullable) void (^titleChangedHandler)(NSString* title);
@property(nonatomic, copy, nullable) void (^urlChangedHandler)(NSString* url);
@property(nonatomic, copy, nullable) void (^faviconURLChangedHandler)(NSString* faviconURL);
@property(nonatomic, copy, nullable) void (^navigationStateChangedHandler)(BOOL canGoBack, BOOL canGoForward, BOOL isLoading);
@property(nonatomic, copy, nullable) void (^consoleMessageHandler)(NSString* message, NSString* source, NSInteger line);
@property(nonatomic, copy, nullable) void (^newWindowRequestedHandler)(NSString* url);
@property(nonatomic, copy, nullable) void (^findResultHandler)(
  NSInteger matchCount,
  NSInteger activeMatchOrdinal,
  BOOL finalUpdate);
@property(nonatomic, copy, nullable) void (^loadEventHandler)(
  NSString* event,
  NSString* url,
  NSInteger httpStatusCode,
  NSInteger errorCode,
  NSString* errorText);
@property(nonatomic, copy, nullable) NSString* trustedClipboardOrigin;

- (instancetype)initWithFrame:(NSRect)frameRect
                   initialURL:(NSString*)initialURL
            profileIdentifier:(NSString*)profileIdentifier NS_DESIGNATED_INITIALIZER;
- (instancetype)initWithFrame:(NSRect)frameRect NS_UNAVAILABLE;
- (nullable instancetype)initWithCoder:(NSCoder*)coder NS_UNAVAILABLE;

@property(nonatomic, readonly, nullable) NSString* currentURLString;
@property(nonatomic, readonly, nullable) NSString* pageTitle;
@property(nonatomic, readonly) NSInteger browserIdentifier;
@property(nonatomic, readonly) BOOL canGoBack;
@property(nonatomic, readonly) BOOL canGoForward;
@property(nonatomic, readonly) BOOL isLoading;
@property(nonatomic, readonly) double zoomLevel;

- (void)loadURLString:(NSString*)urlString;
- (void)goBack;
- (void)goForward;
- (void)reload;
- (void)stopLoading;
- (void)zoomIn;
- (void)zoomOut;
- (void)resetZoom;
- (void)findText:(NSString*)searchText
         forward:(BOOL)forward
        findNext:(BOOL)findNext NS_SWIFT_NAME(findText(_:forward:findNext:));
- (void)stopFindingWithClearSelection:(BOOL)clearSelection NS_SWIFT_NAME(stopFinding(clearSelection:));
- (void)executeJavaScript:(NSString*)javaScript;
- (void)emitToolbarActionDiagnosticsWithAction:(NSString*)action
                                         phase:(NSString*)phase NS_SWIFT_NAME(emitToolbarActionDiagnostics(action:phase:));
- (void)pinHostedViewToBounds;
- (void)completeCurrentDragAtWindowPoint:(NSPoint)windowPoint;
- (void)toggleDevTools;
- (void)closeBrowser;

@end

NS_ASSUME_NONNULL_END
