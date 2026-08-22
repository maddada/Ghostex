#import <AppKit/AppKit.h>
#import <math.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

static NSBezierPath* GhostexGpuiAppIconSquirclePath(NSRect rect) {
  const NSInteger steps = 256;
  const double exponent = 5.0;
  const double power = 2.0 / exponent;
  const double centerX = NSMidX(rect);
  const double centerY = NSMidY(rect);
  const double radiusX = NSWidth(rect) / 2.0;
  const double radiusY = NSHeight(rect) / 2.0;
  NSBezierPath* path = [NSBezierPath bezierPath];
  for (NSInteger index = 0; index <= steps; index += 1) {
    const double theta = (2.0 * M_PI * (double)index) / (double)steps;
    const double cosine = cos(theta);
    const double sine = sin(theta);
    const double x = centerX + radiusX * copysign(pow(fabs(cosine), power), cosine);
    const double y = centerY + radiusY * copysign(pow(fabs(sine), power), sine);
    const NSPoint point = NSMakePoint((CGFloat)x, (CGFloat)y);
    if (index == 0) {
      [path moveToPoint:point];
    } else {
      [path lineToPoint:point];
    }
  }
  [path closePath];
  return path;
}

static NSImage* GhostexGpuiAppIconMaskedImage(NSImage* source, CGFloat dimension) {
  if (!source || dimension <= 0.0 || source.size.width <= 0.0 || source.size.height <= 0.0) {
    return nil;
  }
  NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc]
    initWithBitmapDataPlanes:NULL
                  pixelsWide:(NSInteger)dimension
                  pixelsHigh:(NSInteger)dimension
               bitsPerSample:8
             samplesPerPixel:4
                    hasAlpha:YES
                    isPlanar:NO
              colorSpaceName:NSDeviceRGBColorSpace
                 bytesPerRow:0
                bitsPerPixel:0];
  if (!bitmap) {
    return nil;
  }
  NSGraphicsContext* context = [NSGraphicsContext graphicsContextWithBitmapImageRep:bitmap];
  if (!context) {
    return nil;
  }
  NSGraphicsContext* previous = NSGraphicsContext.currentContext;
  NSGraphicsContext.currentContext = context;
  CGContextClearRect(context.CGContext, CGRectMake(0.0, 0.0, dimension, dimension));
  [GhostexGpuiAppIconSquirclePath(NSMakeRect(0.0, 0.0, dimension, dimension)) addClip];

  const CGFloat sourceWidth = source.size.width;
  const CGFloat sourceHeight = source.size.height;
  const CGFloat sourceSide = MIN(sourceWidth, sourceHeight);
  const NSRect sourceRect = NSMakeRect(
    (sourceWidth - sourceSide) / 2.0,
    (sourceHeight - sourceSide) / 2.0,
    sourceSide,
    sourceSide);
  [source drawInRect:NSMakeRect(0.0, 0.0, dimension, dimension)
            fromRect:sourceRect
           operation:NSCompositingOperationSourceOver
            fraction:1.0
      respectFlipped:NO
               hints:@{NSImageHintInterpolation: @(NSImageInterpolationHigh)}];
  [context flushGraphics];
  NSGraphicsContext.currentContext = previous;
  return [[NSImage alloc] initWithCGImage:bitmap.CGImage
                                    size:NSMakeSize(dimension, dimension)];
}

static NSImage* GhostexGpuiAppIconImageForPath(const char* path, CGFloat dimension) {
  if (!path || path[0] == '\0') {
    return nil;
  }
  NSString* pathString = [NSString stringWithUTF8String:path];
  if (pathString.length == 0) {
    return nil;
  }
  NSImage* source = [[NSImage alloc] initWithContentsOfFile:pathString];
  return GhostexGpuiAppIconMaskedImage(source, dimension);
}

static NSData* GhostexGpuiAppIconPngData(NSImage* image) {
  if (!image) {
    return nil;
  }
  NSData* tiff = image.TIFFRepresentation;
  NSBitmapImageRep* bitmap = tiff ? [[NSBitmapImageRep alloc] initWithData:tiff] : nil;
  return [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
}

static void GhostexGpuiAppIconSetRuntimeImage(NSImage* image) {
  NSDockTile* dockTile = NSApp.dockTile;
  if (!image) {
    NSApp.applicationIconImage = nil;
    dockTile.contentView = nil;
    [dockTile display];
  } else {
    NSApp.applicationIconImage = image;
    NSSize tileSize = dockTile.size;
    if (tileSize.width <= 0.0 || tileSize.height <= 0.0) {
      tileSize = NSMakeSize(128.0, 128.0);
    }
    NSImageView* imageView = [[NSImageView alloc] initWithFrame:NSMakeRect(0.0, 0.0, tileSize.width, tileSize.height)];
    imageView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    imageView.image = image;
    imageView.imageAlignment = NSImageAlignCenter;
    imageView.imageScaling = NSImageScaleProportionallyUpOrDown;
    dockTile.contentView = imageView;
    [dockTile display];
  }

  NSString* bundlePath = NSBundle.mainBundle.bundlePath;
  if (bundlePath.length > 0 && [NSWorkspace.sharedWorkspace setIcon:image forFile:bundlePath options:0]) {
    [NSWorkspace.sharedWorkspace noteFileSystemChanged:bundlePath];
    [NSFileManager.defaultManager setAttributes:@{NSFileModificationDate: [NSDate date]}
                                   ofItemAtPath:bundlePath
                                          error:nil];
  }
}

int GhostexGpuiAppIconApplyFile(const char* path) {
  if (!path || path[0] == '\0') {
    GhostexGpuiAppIconSetRuntimeImage(nil);
    return 1;
  }
  NSImage* image = GhostexGpuiAppIconImageForPath(path, 1024.0);
  if (!image) {
    return 0;
  }
  GhostexGpuiAppIconSetRuntimeImage(image);
  return 1;
}

char* GhostexGpuiAppIconThumbnailDataUrl(const char* path) {
  NSImage* image = nil;
  if (!path || path[0] == '\0') {
    image = [NSWorkspace.sharedWorkspace iconForFile:NSBundle.mainBundle.bundlePath];
  } else {
    image = GhostexGpuiAppIconImageForPath(path, 128.0);
  }
  NSData* png = GhostexGpuiAppIconPngData(image);
  if (!png) {
    return strdup("");
  }
  NSString* value = [@"data:image/png;base64," stringByAppendingString:[png base64EncodedStringWithOptions:0]];
  return strdup(value.UTF8String ?: "");
}

void GhostexGpuiAppIconRevealDirectory(const char* path) {
  if (!path || path[0] == '\0') {
    return;
  }
  NSString* pathString = [NSString stringWithUTF8String:path];
  if (pathString.length > 0) {
    [NSWorkspace.sharedWorkspace openURL:[NSURL fileURLWithPath:pathString isDirectory:YES]];
  }
}

void GhostexGpuiAppIconFreeCString(char* value) {
  free(value);
}
