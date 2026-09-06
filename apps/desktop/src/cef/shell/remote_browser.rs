//! Dedicated request contexts for browsing through a remote computer.
use anyhow::{Context as _, Result};
use cef::rc::Rc as _;
use cef::{
    CefString, ImplDictionaryValue as _, ImplPreferenceManager as _, ImplRequestContextHandler,
    ImplValue as _, RequestContext, RequestContextHandler, WrapRequestContextHandler,
    wrap_request_context_handler,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

struct RemoteContext {
    context: RequestContext,
    status: Rc<Cell<u8>>,
}
thread_local! {
    static REMOTE_CONTEXTS: RefCell<HashMap<String, RemoteContext>> = RefCell::new(HashMap::new());
}

wrap_request_context_handler! {
    struct RemoteContextHandler { port: u16, status: Rc<Cell<u8>>, }
    impl RequestContextHandler {
        fn on_request_context_initialized(&self, context: Option<&mut RequestContext>) {
            self.status.set(if context.is_some_and(|context| configure_proxy(context, self.port).is_ok()) { 1 } else { 2 });
        }
    }
}

/// CDXC:Browser 2026-09-05 WHY:
/// Set the proxy only after CEF initializes the request context, and admit no page loads until it succeeds.
/// Chromium's implicit localhost bypass is disabled only in this dedicated context; app UI traffic retains its local context.
fn configure_proxy(context: &RequestContext, port: u16) -> Result<()> {
    let mut proxy = cef::dictionary_value_create().context("create remote proxy settings")?;
    proxy.set_string(
        Some(&CefString::from("mode")),
        Some(&CefString::from("fixed_servers")),
    );
    proxy.set_string(
        Some(&CefString::from("server")),
        Some(&CefString::from(
            format!("socks5://127.0.0.1:{port}").as_str(),
        )),
    );
    proxy.set_string(
        Some(&CefString::from("bypass_list")),
        Some(&CefString::from("<-loopback>")),
    );
    let mut value = cef::value_create().context("create remote proxy value")?;
    value.set_dictionary(Some(&mut proxy));
    let mut error = CefString::default();
    anyhow::ensure!(
        context.set_preference(
            Some(&CefString::from("proxy")),
            Some(&mut value),
            Some(&mut error)
        ) != 0,
        "Could not configure the remote browser proxy"
    );
    Ok(())
}

pub(crate) fn prepare_remote_browser_context(profile: &str, port: u16) -> Result<bool> {
    if let Some(status) = REMOTE_CONTEXTS.with(|contexts| {
        contexts
            .borrow()
            .get(profile)
            .map(|entry| entry.status.get())
    }) {
        anyhow::ensure!(status != 2, "Could not configure the remote browser proxy");
        return Ok(status == 1);
    }
    let status = Rc::new(Cell::new(0));
    let mut handler = RemoteContextHandler::new(port, status.clone());
    let settings = cef::RequestContextSettings::default();
    let context = cef::request_context_create_context(Some(&settings), Some(&mut handler))
        .context("Could not create the remote browser context")?;
    let ready = status.get() == 1;
    REMOTE_CONTEXTS.with(|contexts| {
        contexts
            .borrow_mut()
            .insert(profile.to_string(), RemoteContext { context, status })
    });
    Ok(ready)
}

pub(crate) fn remote_browser_request_context(profile: &str) -> Result<RequestContext> {
    REMOTE_CONTEXTS.with(|contexts| {
        let contexts = contexts.borrow();
        let entry = contexts
            .get(profile)
            .context("Remote browser network is not prepared")?;
        anyhow::ensure!(
            entry.status.get() == 1,
            "Remote browser network is not ready"
        );
        Ok(entry.context.clone())
    })
}
