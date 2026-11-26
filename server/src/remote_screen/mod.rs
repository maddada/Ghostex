//! Remote screen: watch and control this computer from another computer or a phone. The screen
//! itself is streamed by the separate `ghostex-screen` helper (apps/screen-helper); gxserver
//! starts it, stops it and carries its page and stream through the link the viewer already has.

mod endpoint;
mod gate;
mod helper;
mod proxy;

pub(crate) use endpoint::{handle_remote_screen_http, REMOTE_SCREEN_ENDPOINTS};
pub(crate) use gate::remote_screen_enabled;
pub(crate) use proxy::handle_remote_screen_proxy;
