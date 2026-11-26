#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>

/*
 CDXC:RemoteScreen 2026-11-27 WHY:
 The remote screen's viewer (the screen helper's own page, through gxserver's ticket proxy) decodes H.264 with WebCodecs, and Ghostex's CEF is a stock build without H.264 (measured in a test copy on 2026-11-27: VideoDecoder.isConfigSupported is false for every avc1 profile, true for VP8, VP9 and AV1). WKWebView decodes H.264 in hardware, so the window shows the viewer in the system web view, a plain child of the window's GPUI view that fills it. It stays on the viewer's own address: any other navigation or new window is refused, so a page that went wrong cannot become a browser here. It takes the keyboard when made and whenever its window becomes key, since every key belongs to the remote computer.
 SEE-ALSO: apps/desktop/src/app/window/remote_screen/web_view.rs (the Rust side and the Windows WebView2 twin), native/macos/GpuiIntroVideoWebView.m (the same shape for the intro video).
*/
@interface GhostexRemoteScreenWebView : WKWebView <WKNavigationDelegate, WKUIDelegate>
@property(nonatomic, copy) NSString *allowedPrefix;
@end

@implementation GhostexRemoteScreenWebView

- (void)webView:(WKWebView *)webView
    decidePolicyForNavigationAction:(WKNavigationAction *)navigationAction
                    decisionHandler:(void (^)(WKNavigationActionPolicy))decisionHandler {
  NSString *address = navigationAction.request.URL.absoluteString;
  // Subframes and the page itself must stay under the viewer's own path.
  if (address != nil && [address hasPrefix:self.allowedPrefix]) {
    decisionHandler(WKNavigationActionPolicyAllow);
    return;
  }
  decisionHandler(WKNavigationActionPolicyCancel);
}

- (WKWebView *)webView:(WKWebView *)webView
    createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration
               forNavigationAction:(WKNavigationAction *)navigationAction
                    windowFeatures:(WKWindowFeatures *)windowFeatures {
  return nil;
}

- (void)windowBecameKey:(NSNotification *)notification {
  [self.window makeFirstResponder:self];
}

@end

void *GhostexGpuiRemoteScreenWebViewCreate(void *parentView, const char *url) {
  NSView *parent = (__bridge NSView *)parentView;
  if (parent == nil || url == NULL) {
    return NULL;
  }
  NSString *address = [NSString stringWithUTF8String:url];
  NSURL *pageURL = address == nil ? nil : [NSURL URLWithString:address];
  if (pageURL == nil) {
    return NULL;
  }
  WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
  configuration.mediaTypesRequiringUserActionForPlayback = WKAudiovisualMediaTypeNone;
  GhostexRemoteScreenWebView *webView =
      [[GhostexRemoteScreenWebView alloc] initWithFrame:NSZeroRect configuration:configuration];
  webView.allowedPrefix = address;
  webView.navigationDelegate = webView;
  webView.UIDelegate = webView;
  webView.autoresizingMask = NSViewNotSizable;
  webView.allowsBackForwardNavigationGestures = NO;
  webView.allowsMagnification = NO;
  [webView setValue:@NO forKey:@"drawsBackground"];
  webView.hidden = YES;
  [webView loadRequest:[NSURLRequest requestWithURL:pageURL]];
  [parent addSubview:webView positioned:NSWindowAbove relativeTo:nil];
  if (parent.window != nil) {
    [[NSNotificationCenter defaultCenter] addObserver:webView
                                             selector:@selector(windowBecameKey:)
                                                 name:NSWindowDidBecomeKeyNotification
                                               object:parent.window];
    [parent.window makeFirstResponder:webView];
  }
  return (__bridge_retained void *)webView;
}

void GhostexGpuiRemoteScreenWebViewSetFrame(void *handle, double x, double y, double width,
                                            double height) {
  GhostexRemoteScreenWebView *webView = (__bridge GhostexRemoteScreenWebView *)handle;
  if (webView == nil) {
    return;
  }
  NSView *parent = webView.superview;
  CGFloat nativeY = y;
  if (parent != nil && !parent.isFlipped) {
    nativeY = NSHeight(parent.bounds) - y - height;
  }
  webView.frame = NSMakeRect(x, nativeY, MAX(0.0, width), MAX(0.0, height));
  if (webView.hidden) {
    webView.hidden = NO;
    [webView.window makeFirstResponder:webView];
  }
}

void GhostexGpuiRemoteScreenWebViewDestroy(void *handle) {
  GhostexRemoteScreenWebView *webView = (__bridge_transfer GhostexRemoteScreenWebView *)handle;
  if (webView == nil) {
    return;
  }
  [[NSNotificationCenter defaultCenter] removeObserver:webView];
  NSWindow *window = webView.window;
  NSResponder *firstResponder = window.firstResponder;
  if ([firstResponder isKindOfClass:[NSView class]] &&
      [(NSView *)firstResponder isDescendantOf:webView]) {
    [window makeFirstResponder:webView.superview];
  }
  [webView stopLoading];
  webView.navigationDelegate = nil;
  webView.UIDelegate = nil;
  [webView removeFromSuperview];
}
