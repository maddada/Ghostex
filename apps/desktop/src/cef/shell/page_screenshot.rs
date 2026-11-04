//! A screenshot of the top of a page a windowed browser is showing, taken over the DevTools
//! protocol (`Page.captureScreenshot`) the appearance and keep-awake code already drive, and the
//! wait before it until the page has laid itself out. Used for the chat's page card pictures
//! (`app/visual_page_thumbnails.rs`).
//!
//! CDXC:CefRuntime 2026-11-02 WHY:
//! The picture comes from a windowed browser: the floating page window the user opened, or, for a page published since 2026-11-02, a hidden browser made only to picture it (`app/visual_page_background_pictures.rs`, the user's decision is on server/src/visual_pages/thumbnail.rs). Never from a windowless (off-screen rendering) browser, which needs CEF's windowless rendering switched on for every web view; the user rejected that on 2026-10-11. The answers arrive on the observer while CEF's message pump runs, which can be inside a gpui update, so it only fills a channel.
use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, WrapDevToolsMessageObserver,
    wrap_dev_tools_message_observer,
};
use futures::channel::oneshot;

/// The answer slot of one DevTools request.
type ScreenshotSlot = Arc<Mutex<Option<(c_int, oneshot::Sender<Option<String>>)>>>;

/// A screenshot on its way. Keep it until `picture` resolves: dropping it stops listening.
pub struct PageScreenshot {
    /// Base64 PNG, or `None` when the page refused.
    pub picture: oneshot::Receiver<Option<String>>,
    _registration: Option<cef::Registration>,
}

/// The page's answer to `wait_for_page_settled` on its way. Keep it until `settled` resolves:
/// dropping it stops listening.
pub struct PageSettled {
    /// `"top"` when the page is laid out (or ran out of time) at its top, `"scrolled"` when it is
    /// scrolled away from it; `None` when the page could not answer.
    pub settled: oneshot::Receiver<Option<String>>,
    _registration: Option<cef::Registration>,
}

/// Waits, inside the page, for the document to load, its web fonts and its pictures (each given up
/// on after `__TIMEOUT__` ms in all), then says whether the page is at its top. Timers, not
/// animation frames, because a hidden page gets no frames.
const PAGE_SETTLED_SCRIPT: &str = r#"(async () => {
  const late = new Promise((done) => setTimeout(done, __TIMEOUT__));
  const loaded = (async () => {
    if (document.readyState !== 'complete') await new Promise((done) => addEventListener('load', done, { once: true }));
    if (document.fonts) await document.fonts.ready;
    await Promise.all(Array.from(document.images, (img) => (img.complete ? null : img.decode().catch(() => null))));
    await new Promise((done) => setTimeout(done, 60));
  })();
  await Promise.race([loaded, late]);
  return scrollX === 0 && scrollY === 0 ? 'top' : 'scrolled';
})()"#;

/// What an answer's `result` turns into on the channel.
#[derive(Clone, Copy)]
enum AnswerKind {
    /// `Page.captureScreenshot`'s `data`.
    ScreenshotData,
    /// `Runtime.evaluate`'s string value.
    EvaluatedString,
}

impl AnswerKind {
    fn read(self, value: &serde_json::Value) -> Option<String> {
        let found = match self {
            Self::ScreenshotData => value.get("data"),
            Self::EvaluatedString => value.get("result")?.get("value"),
        };
        found?.as_str().map(str::to_string)
    }
}

wrap_dev_tools_message_observer! {
    struct PageScreenshotObserver {
        slot: ScreenshotSlot,
        kind: AnswerKind,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut cef::Browser>,
            message_id: c_int,
            success: c_int,
            result: Option<&[u8]>,
        ) {
            let mut slot = self.slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.as_ref().map(|(id, _)| *id) != Some(message_id) {
                return;
            }
            let Some((_, sender)) = slot.take() else {
                return;
            };
            let picture = (success != 0)
                .then_some(result)
                .flatten()
                .and_then(|result| serde_json::from_slice::<serde_json::Value>(result).ok())
                .and_then(|value| self.kind.read(&value));
            let _ = sender.send(picture);
        }
    }
}

impl CefBrowser {
    /// Asks the page for a PNG of the area `width`×`height` CSS pixels at its top left, drawn at
    /// `scale`. `None` when the request could not be sent.
    pub fn capture_page_top(&self, width: f64, height: f64, scale: f64) -> Option<PageScreenshot> {
        let mut clip = cef::dictionary_value_create()?;
        for (key, value) in [
            ("x", 0.0),
            ("y", 0.0),
            ("width", width),
            ("height", height),
            ("scale", scale),
        ] {
            clip.set_double(Some(&CefString::from(key)), value);
        }
        let params = cef::dictionary_value_create()?;
        params.set_string(
            Some(&CefString::from("format")),
            Some(&CefString::from("png")),
        );
        params.set_dictionary(Some(&CefString::from("clip")), Some(&mut clip));
        let (picture, registration) =
            self.ask_dev_tools("Page.captureScreenshot", params, AnswerKind::ScreenshotData)?;
        Some(PageScreenshot {
            picture,
            _registration: registration,
        })
    }

    /// Asks the page to answer once it has loaded with its web fonts and pictures, or after
    /// `timeout_ms`, whichever comes first (`PAGE_SETTLED_SCRIPT`). `None` when the request could
    /// not be sent.
    pub fn wait_for_page_settled(&self, timeout_ms: u64) -> Option<PageSettled> {
        let params = cef::dictionary_value_create()?;
        let script = PAGE_SETTLED_SCRIPT.replace("__TIMEOUT__", &timeout_ms.to_string());
        params.set_string(
            Some(&CefString::from("expression")),
            Some(&CefString::from(script.as_str())),
        );
        params.set_bool(Some(&CefString::from("awaitPromise")), 1);
        params.set_bool(Some(&CefString::from("returnByValue")), 1);
        let (settled, registration) =
            self.ask_dev_tools("Runtime.evaluate", params, AnswerKind::EvaluatedString)?;
        Some(PageSettled {
            settled,
            _registration: registration,
        })
    }

    fn ask_dev_tools(
        &self,
        method: &str,
        mut params: cef::DictionaryValue,
        kind: AnswerKind,
    ) -> Option<(oneshot::Receiver<Option<String>>, Option<cef::Registration>)> {
        let browser = self.browser.borrow().clone();
        let host = browser.host()?;
        let (sender, answer) = oneshot::channel();
        let slot: ScreenshotSlot = Arc::new(Mutex::new(None));
        let mut observer = PageScreenshotObserver::new(slot.clone(), kind);
        let registration = host.add_dev_tools_message_observer(Some(&mut observer));
        let message_id = next_page_appearance_devtools_message_id();
        // Filled before the call: CEF may answer while it executes.
        *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((message_id, sender));
        let sent = host.execute_dev_tools_method(
            message_id,
            Some(&CefString::from(method)),
            Some(&mut params),
        ) != 0;
        sent.then_some((answer, registration))
    }
}
