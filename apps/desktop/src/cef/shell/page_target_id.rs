//! The id the DevTools endpoint lists a page under (`targetId`), asked of the page's own DevTools
//! session (`Target.getTargetInfo` with no id answers for the page itself), so the app can say which
//! of the endpoint's pages is which Browser tab (`listBrowserPages`, app/browser_agent_pages.rs).
//! Same request and answer path as `page_screenshot.rs`.
use super::*;
use cef::{
    DevToolsMessageObserver, ImplDevToolsMessageObserver, WrapDevToolsMessageObserver,
    wrap_dev_tools_message_observer,
};
use futures::channel::oneshot;

/// The answer slot of one target id request.
type TargetIdSlot = Arc<Mutex<Option<(c_int, oneshot::Sender<Option<String>>)>>>;

/// A target id on its way. Keep it until `target_id` resolves: dropping it stops listening.
pub struct PageTargetId {
    pub target_id: oneshot::Receiver<Option<String>>,
    _registration: Option<cef::Registration>,
}

wrap_dev_tools_message_observer! {
    struct PageTargetIdObserver {
        slot: TargetIdSlot,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            browser: Option<&mut cef::Browser>,
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
            let target_id = (success != 0)
                .then_some(result)
                .flatten()
                .and_then(|result| serde_json::from_slice::<serde_json::Value>(result).ok())
                .and_then(|value| {
                    value
                        .get("targetInfo")?
                        .get("targetId")?
                        .as_str()
                        .map(str::to_string)
                });
            // The DevTools relay lists the page under the same id (devtools_relay.rs).
            if let (Some(browser), Some(target_id)) = (browser, target_id.as_deref()) {
                cef_devtools_relay_note_target_id(browser.identifier(), target_id);
            }
            let _ = sender.send(target_id);
        }
    }
}

impl CefBrowser {
    /// Asks the page for its DevTools target id. `None` when the request could not be sent.
    pub fn request_devtools_target_id(&self) -> Option<PageTargetId> {
        let browser = self.browser.borrow().clone();
        let host = browser.host()?;
        let (sender, target_id) = oneshot::channel();
        let slot: TargetIdSlot = Arc::new(Mutex::new(None));
        let mut observer = PageTargetIdObserver::new(slot.clone());
        let registration = host.add_dev_tools_message_observer(Some(&mut observer));
        let mut params = cef::dictionary_value_create()?;
        let message_id = next_page_appearance_devtools_message_id();
        // Filled before the call: CEF may answer while it executes.
        *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((message_id, sender));
        let sent = host.execute_dev_tools_method(
            message_id,
            Some(&CefString::from("Target.getTargetInfo")),
            Some(&mut params),
        ) != 0;
        sent.then_some(PageTargetId {
            target_id,
            _registration: registration,
        })
    }
}
