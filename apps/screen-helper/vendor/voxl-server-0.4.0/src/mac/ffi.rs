//! C APIs of CoreGraphics, CoreVideo, CoreMedia, CoreFoundation, Carbon and CoreAudio
//! that the macOS backend calls directly (Objective-C APIs go through objc2).

#![allow(non_upper_case_globals, non_snake_case, dead_code)]

use std::ffi::{c_char, c_void};

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFDataRef = *const c_void;
pub type CGEventRef = *mut c_void;
pub type CGEventSourceRef = *mut c_void;
pub type CGDisplayModeRef = *mut c_void;
pub type CGDisplayConfigRef = *mut c_void;
pub type CGImageRef = *const c_void;
pub type CGContextRef = *mut c_void;
pub type CGColorSpaceRef = *mut c_void;
pub type CVPixelBufferRef = *mut c_void;
pub type CMSampleBufferRef = *const c_void;
pub type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFArrayGetCount(a: CFArrayRef) -> isize;
    pub fn CFArrayGetValueAtIndex(a: CFArrayRef, i: isize) -> *const c_void;
    pub fn CFDictionaryGetValue(d: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub fn CFNumberGetValue(n: CFTypeRef, ty: isize, out: *mut c_void) -> bool;
    pub fn CFDataGetBytePtr(d: CFDataRef) -> *const u8;
    pub fn CFDataGetLength(d: CFDataRef) -> isize;
    pub fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, enc: u32) -> bool;
    pub fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> bool;
}
pub const kCFNumberSInt64Type: isize = 4;
pub const kCFNumberDoubleType: isize = 13;
pub const kCFStringEncodingUTF8: u32 = 0x0800_0100;

/// A CFString as a Rust string.
pub unsafe fn cf_string(s: CFStringRef) -> String {
    if s.is_null() {
        return String::new();
    }
    let mut buf = [0 as c_char; 512];
    if CFStringGetCString(
        s,
        buf.as_mut_ptr(),
        buf.len() as isize,
        kCFStringEncodingUTF8,
    ) {
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGMainDisplayID() -> u32;
    pub fn CGGetActiveDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    pub fn CGDisplayIsMain(id: u32) -> u32;
    pub fn CGDisplayBounds(id: u32) -> CGRect;
    pub fn CGDisplayPixelsWide(id: u32) -> usize;
    pub fn CGDisplayPixelsHigh(id: u32) -> usize;
    pub fn CGDisplayCopyDisplayMode(id: u32) -> CGDisplayModeRef;
    pub fn CGDisplayCopyAllDisplayModes(id: u32, options: CFDictionaryRef) -> CFArrayRef;
    pub fn CGDisplayModeGetWidth(m: CGDisplayModeRef) -> usize;
    pub fn CGDisplayModeGetHeight(m: CGDisplayModeRef) -> usize;
    pub fn CGDisplayModeGetPixelWidth(m: CGDisplayModeRef) -> usize;
    pub fn CGDisplayModeGetPixelHeight(m: CGDisplayModeRef) -> usize;
    pub fn CGDisplayModeGetRefreshRate(m: CGDisplayModeRef) -> f64;
    pub fn CGDisplayModeIsUsableForDesktopGUI(m: CGDisplayModeRef) -> bool;
    pub fn CGDisplayModeGetIODisplayModeID(m: CGDisplayModeRef) -> i32;
    pub fn CGDisplayModeRelease(m: CGDisplayModeRef);
    pub fn CGBeginDisplayConfiguration(config: *mut CGDisplayConfigRef) -> i32;
    pub fn CGConfigureDisplayWithDisplayMode(
        config: CGDisplayConfigRef,
        display: u32,
        mode: CGDisplayModeRef,
        options: CFDictionaryRef,
    ) -> i32;
    pub fn CGCompleteDisplayConfiguration(config: CGDisplayConfigRef, option: u32) -> i32;
    pub fn CGCancelDisplayConfiguration(config: CGDisplayConfigRef) -> i32;
    pub static kCGDisplayShowDuplicateLowResolutionModes: CFStringRef;
    pub static kCGDisplayStreamYCbCrMatrix_ITU_R_601_4: CFStringRef;
    pub static kCGColorSpaceSRGB: CFStringRef;

    pub fn CGRectMakeWithDictionaryRepresentation(d: CFDictionaryRef, rect: *mut CGRect) -> bool;

    pub fn CGPreflightScreenCaptureAccess() -> bool;
    pub fn CGRequestScreenCaptureAccess() -> bool;

    pub fn CGEventSourceCreate(state: i32) -> CGEventSourceRef;
    pub fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        ty: u32,
        pos: CGPoint,
        button: u32,
    ) -> CGEventRef;
    pub fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, key: u16, down: bool)
        -> CGEventRef;
    pub fn CGEventCreateScrollWheelEvent2(
        source: CGEventSourceRef,
        units: u32,
        count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    pub fn CGEventSetType(e: CGEventRef, ty: u32);
    pub fn CGEventSetFlags(e: CGEventRef, flags: u64);
    pub fn CGEventSetIntegerValueField(e: CGEventRef, field: u32, value: i64);
    pub fn CGEventKeyboardSetUnicodeString(e: CGEventRef, len: usize, s: *const u16);
    pub fn CGEventPost(tap: u32, e: CGEventRef);

    pub fn CGColorSpaceCreateDeviceRGB() -> CGColorSpaceRef;
    pub fn CGColorSpaceRelease(cs: CGColorSpaceRef);
    pub fn CGBitmapContextCreate(
        data: *mut c_void,
        w: usize,
        h: usize,
        bits: usize,
        bpr: usize,
        cs: CGColorSpaceRef,
        info: u32,
    ) -> CGContextRef;
    pub fn CGContextDrawImage(c: CGContextRef, r: CGRect, image: CGImageRef);
    pub fn CGContextRelease(c: CGContextRef);
}

pub const kCGEventSourceStateHIDSystemState: i32 = 1;
pub const kCGHIDEventTap: u32 = 0;
pub const kCGConfigureForAppOnly: u32 = 0;

pub const kCGEventLeftMouseDown: u32 = 1;
pub const kCGEventLeftMouseUp: u32 = 2;
pub const kCGEventRightMouseDown: u32 = 3;
pub const kCGEventRightMouseUp: u32 = 4;
pub const kCGEventMouseMoved: u32 = 5;
pub const kCGEventLeftMouseDragged: u32 = 6;
pub const kCGEventRightMouseDragged: u32 = 7;
pub const kCGEventFlagsChanged: u32 = 12;
pub const kCGEventOtherMouseDown: u32 = 25;
pub const kCGEventOtherMouseUp: u32 = 26;
pub const kCGEventOtherMouseDragged: u32 = 27;

pub const kCGMouseEventClickState: u32 = 1;
pub const kCGMouseEventButtonNumber: u32 = 3;
pub const kCGKeyboardEventAutorepeat: u32 = 8;
pub const kCGScrollWheelEventIsContinuous: u32 = 88;

pub const kCGScrollEventUnitPixel: u32 = 0;
pub const kCGScrollEventUnitLine: u32 = 1;

pub const kCGEventFlagMaskShift: u64 = 0x0002_0000;
pub const kCGEventFlagMaskControl: u64 = 0x0004_0000;
pub const kCGEventFlagMaskAlternate: u64 = 0x0008_0000;
pub const kCGEventFlagMaskCommand: u64 = 0x0010_0000;
/// Device-dependent bits telling left and right modifiers apart (IOKit's NX_DEVICE*KEYMASK).
pub const NX_DEVICELCTLKEYMASK: u64 = 0x0001;
pub const NX_DEVICELSHIFTKEYMASK: u64 = 0x0002;
pub const NX_DEVICERSHIFTKEYMASK: u64 = 0x0004;
pub const NX_DEVICELCMDKEYMASK: u64 = 0x0008;
pub const NX_DEVICERCMDKEYMASK: u64 = 0x0010;
pub const NX_DEVICELALTKEYMASK: u64 = 0x0020;
pub const NX_DEVICERALTKEYMASK: u64 = 0x0040;
pub const NX_DEVICERCTLKEYMASK: u64 = 0x2000;
/// Set on every event a real keyboard or mouse makes.
pub const NX_NONCOALSESCEDMASK: u64 = 0x0100;

pub const kCGImageAlphaPremultipliedLast: u32 = 1;
pub const kCGBitmapByteOrder32Big: u32 = 4 << 12;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    pub fn CVPixelBufferRetain(pb: CVPixelBufferRef) -> CVPixelBufferRef;
    pub fn CVPixelBufferRelease(pb: CVPixelBufferRef);
    pub fn CVPixelBufferLockBaseAddress(pb: CVPixelBufferRef, flags: u64) -> i32;
    pub fn CVPixelBufferUnlockBaseAddress(pb: CVPixelBufferRef, flags: u64) -> i32;
    pub fn CVPixelBufferGetBaseAddressOfPlane(pb: CVPixelBufferRef, plane: usize) -> *const u8;
    pub fn CVPixelBufferGetBytesPerRowOfPlane(pb: CVPixelBufferRef, plane: usize) -> usize;
    pub fn CVPixelBufferGetWidth(pb: CVPixelBufferRef) -> usize;
    pub fn CVPixelBufferGetHeight(pb: CVPixelBufferRef) -> usize;
    pub fn CVPixelBufferGetPixelFormatType(pb: CVPixelBufferRef) -> u32;
}
pub const kCVPixelBufferLock_ReadOnly: u64 = 1;
/// '420v': NV12, video range.
pub const kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange: u32 = 0x3432_3076;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioStreamBasicDescription {
    pub mSampleRate: f64,
    pub mFormatID: u32,
    pub mFormatFlags: u32,
    pub mBytesPerPacket: u32,
    pub mFramesPerPacket: u32,
    pub mBytesPerFrame: u32,
    pub mChannelsPerFrame: u32,
    pub mBitsPerChannel: u32,
    pub mReserved: u32,
}
pub const kAudioFormatLinearPCM: u32 = 0x6c70_636d;
pub const kAudioFormatFlagIsFloat: u32 = 1;
pub const kAudioFormatFlagIsSignedInteger: u32 = 4;
pub const kAudioFormatFlagIsPacked: u32 = 8;
pub const kAudioFormatFlagIsNonInterleaved: u32 = 32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AudioBuffer {
    pub mNumberChannels: u32,
    pub mDataByteSize: u32,
    pub mData: *mut c_void,
}
/// An AudioBufferList with room for up to 8 buffers.
#[repr(C)]
pub struct AudioBufferList8 {
    pub mNumberBuffers: u32,
    pub mBuffers: [AudioBuffer; 8],
}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    pub fn CMSampleBufferGetImageBuffer(sb: CMSampleBufferRef) -> CVPixelBufferRef;
    pub fn CMSampleBufferGetSampleAttachmentsArray(
        sb: CMSampleBufferRef,
        create: bool,
    ) -> CFArrayRef;
    pub fn CMSampleBufferIsValid(sb: CMSampleBufferRef) -> bool;
    pub fn CMSampleBufferGetFormatDescription(sb: CMSampleBufferRef) -> CFTypeRef;
    pub fn CMAudioFormatDescriptionGetStreamBasicDescription(
        desc: CFTypeRef,
    ) -> *const AudioStreamBasicDescription;
    pub fn CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sb: CMSampleBufferRef,
        size_needed: *mut usize,
        list: *mut AudioBufferList8,
        list_size: usize,
        structure_alloc: CFTypeRef,
        block_alloc: CFTypeRef,
        flags: u32,
        block_out: *mut CFTypeRef,
    ) -> OSStatus;
}
pub const kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment: u32 = 1;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    pub fn TISGetInputSourceProperty(src: CFTypeRef, key: CFStringRef) -> *const c_void;
    pub static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    pub fn LMGetKbdType() -> u8;
    pub fn UCKeyTranslate(
        layout: *const u8,
        key: u16,
        action: u16,
        modifiers: u32,
        kbd_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_len: usize,
        len: *mut usize,
        chars: *mut u16,
    ) -> OSStatus;
}
pub const kUCKeyActionDown: u16 = 0;

#[repr(C)]
pub struct AudioObjectPropertyAddress {
    pub mSelector: u32,
    pub mScope: u32,
    pub mElement: u32,
}
pub const kAudioObjectSystemObject: u32 = 1;
pub const kAudioHardwarePropertyDevices: u32 = u32::from_be_bytes(*b"dev#");
pub const kAudioObjectPropertyName: u32 = u32::from_be_bytes(*b"lnam");
pub const kAudioDevicePropertyDeviceUID: u32 = u32::from_be_bytes(*b"uid ");
pub const kAudioDevicePropertyStreams: u32 = u32::from_be_bytes(*b"stm#");
pub const kAudioObjectPropertyScopeGlobal: u32 = u32::from_be_bytes(*b"glob");
pub const kAudioObjectPropertyScopeOutput: u32 = u32::from_be_bytes(*b"outp");
pub const kAudioObjectPropertyElementMain: u32 = 0;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    pub fn AudioObjectGetPropertyDataSize(
        obj: u32,
        addr: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> OSStatus;
    pub fn AudioObjectGetPropertyData(
        obj: u32,
        addr: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
}

pub type AudioQueueRef = *mut c_void;
#[repr(C)]
pub struct AudioQueueBuffer {
    pub mAudioDataBytesCapacity: u32,
    pub mAudioData: *mut c_void,
    pub mAudioDataByteSize: u32,
    pub mUserData: *mut c_void,
    pub mPacketDescriptionCapacity: u32,
    pub mPacketDescriptions: *mut c_void,
    pub mPacketDescriptionCount: u32,
}
pub type AudioQueueOutputCallback =
    extern "C" fn(user: *mut c_void, queue: AudioQueueRef, buffer: *mut AudioQueueBuffer);
pub const kAudioQueueProperty_CurrentDevice: u32 = u32::from_be_bytes(*b"aqcd");

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    pub fn AudioQueueNewOutput(
        format: *const AudioStreamBasicDescription,
        callback: AudioQueueOutputCallback,
        user: *mut c_void,
        run_loop: CFTypeRef,
        run_loop_mode: CFStringRef,
        flags: u32,
        queue: *mut AudioQueueRef,
    ) -> OSStatus;
    pub fn AudioQueueSetProperty(
        queue: AudioQueueRef,
        id: u32,
        data: *const c_void,
        size: u32,
    ) -> OSStatus;
    pub fn AudioQueueAllocateBuffer(
        queue: AudioQueueRef,
        size: u32,
        buffer: *mut *mut AudioQueueBuffer,
    ) -> OSStatus;
    pub fn AudioQueueEnqueueBuffer(
        queue: AudioQueueRef,
        buffer: *mut AudioQueueBuffer,
        npackets: u32,
        packets: *const c_void,
    ) -> OSStatus;
    pub fn AudioQueueStart(queue: AudioQueueRef, start: *const c_void) -> OSStatus;
    pub fn AudioQueueDispose(queue: AudioQueueRef, immediate: bool) -> OSStatus;
}
