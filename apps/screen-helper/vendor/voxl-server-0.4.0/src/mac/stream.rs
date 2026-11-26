//! ScreenCaptureKit streams: one display, with a Rust callback for every sample buffer
//! (video frames for capture, system audio for the audio module).

use super::ffi::CMSampleBufferRef;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_core_media::CMSampleBuffer;
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

pub use objc2_screen_capture_kit::SCStreamOutputType as OutputType;

type Handler = Box<dyn Fn(CMSampleBufferRef, SCStreamOutputType) + Send + Sync>;

pub struct Ivars {
    handler: Handler,
    stopped: Arc<AtomicBool>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and Output doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "VoxlStreamOutput"]
    #[ivars = Ivars]
    struct Output;

    unsafe impl NSObjectProtocol for Output {}

    unsafe impl SCStreamOutput for Output {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, ty: SCStreamOutputType) {
            (self.ivars().handler)(sample as *const CMSampleBuffer as CMSampleBufferRef, ty);
        }
    }

    unsafe impl SCStreamDelegate for Output {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _stream: &SCStream, error: &NSError) {
            log::warn!("screen capture stopped: {}", error.localizedDescription());
            self.ivars().stopped.store(true, Ordering::Relaxed);
        }
    }
);

impl Output {
    fn new(handler: Handler, stopped: Arc<AtomicBool>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { handler, stopped });
        unsafe { msg_send![super(this), init] }
    }
}

/// Objective-C objects handed between threads: ScreenCaptureKit objects may be used
/// from any thread.
struct SendBox<T>(T);
unsafe impl<T> Send for SendBox<T> {}

/// A running capture; stops when dropped.
pub struct Stream {
    stream: SendBox<Retained<SCStream>>,
    _output: SendBox<Retained<Output>>,
    _queue: SendBox<dispatch2::DispatchRetained<DispatchQueue>>,
    /// Set when ScreenCaptureKit ends the stream (display gone, permission revoked).
    pub stopped: Arc<AtomicBool>,
}

fn shareable_display(id: u32) -> Result<Retained<SCDisplay>, String> {
    let (tx, rx) = mpsc::channel::<Result<SendBox<Retained<SCDisplay>>, String>>();
    let block = block2::RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        let r = match unsafe { Retained::retain(content) } {
            Some(c) => unsafe { c.displays() }
                .iter()
                .find(|d| unsafe { d.displayID() } == id)
                .map(SendBox)
                .ok_or_else(|| format!("display {id} is not available for capture")),
            None => Err(unsafe { err.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "no shareable content".into())),
        };
        let _ = tx.send(r);
    });
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(r) => r.map(|b| b.0).map_err(|e| {
            format!("{e} (is Screen Recording allowed in System Settings > Privacy & Security?)")
        }),
        Err(_) => Err("timed out listing displays".into()),
    }
}

/// Starts capturing `display` with the outputs in `types`; `configure` sets up the
/// stream configuration and `handler` gets every sample buffer, on a capture queue.
pub fn start(
    display: u32,
    name: &str,
    types: &[SCStreamOutputType],
    configure: impl FnOnce(&SCStreamConfiguration),
    handler: impl Fn(CMSampleBufferRef, SCStreamOutputType) + Send + Sync + 'static,
) -> Result<Stream, String> {
    let d = shareable_display(display)?;
    let stopped = Arc::new(AtomicBool::new(false));
    let output = Output::new(Box::new(handler), stopped.clone());
    let queue = DispatchQueue::new(&format!("voxl.{name}"), None);
    let stream = unsafe {
        let filter = SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            &d,
            &NSArray::new(),
        );
        let cfg = SCStreamConfiguration::new();
        configure(&cfg);
        let stream = SCStream::initWithFilter_configuration_delegate(
            SCStream::alloc(),
            &filter,
            &cfg,
            Some(ProtocolObject::from_ref(&*output)),
        );
        for &ty in types {
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    ty,
                    Some(&queue),
                )
                .map_err(|e| format!("adding a stream output: {}", e.localizedDescription()))?;
        }
        stream
    };
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let done = block2::RcBlock::new(move |err: *mut NSError| {
        let _ = tx.send(unsafe { err.as_ref() }.map(|e| e.localizedDescription().to_string()));
    });
    unsafe { stream.startCaptureWithCompletionHandler(Some(&done)) };
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(None) => {}
        Ok(Some(e)) => return Err(format!("starting capture: {e}")),
        Err(_) => return Err("timed out starting capture".into()),
    }
    Ok(Stream {
        stream: SendBox(stream),
        _output: SendBox(output),
        _queue: SendBox(queue),
        stopped,
    })
}

impl Drop for Stream {
    fn drop(&mut self) {
        // Wait for the stop, so a stream started right after (new size) doesn't overlap.
        let (tx, rx) = mpsc::channel::<()>();
        let done = block2::RcBlock::new(move |_err: *mut NSError| {
            let _ = tx.send(());
        });
        unsafe { self.stream.0.stopCaptureWithCompletionHandler(Some(&done)) };
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
}
