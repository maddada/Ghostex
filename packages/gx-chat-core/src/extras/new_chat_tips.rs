//! The tips a new chat shows under "What should we build with …?": the list (one data file,
//! `packages/gx-chat-core/tips/new-chat-tips.json`) and the order the card walks them in.
//!
//! CDXC:Onboarding 2026-11-11 DECISION:
//! User (first-run page, step 14): "show friendly tips about ghostex that teach about it below the what should we build with X thing (should be a card which shows a different tip every time you open and has ability to click next prev and has TONS of nice things about ghostex explained there. image icon with title next to it then subtitle text below max 2 lines." The first ones a new user sees are how to report a bug with "Collect more data with an agent" on, dragging a session from the sidebar to split, coordinators and Connect a Device (the tips with `first`). Step 19: "let's remove tips and tricks modal from the whole app, we should the help on new chats instead and user can go through them and if he marks any as read we don't show it again ever". The rest walk in an order shuffled once a day, so Next and Previous stay put while a new chat opens on the next one.
//! SEE-ALSO: apps/desktop/src/app/native_chat/new_chat_tips.rs (the card), the phone's new chat screen (not built yet: it can read this same file through the WebAssembly core).

use std::sync::OnceLock;

use serde::Deserialize;

/// The data file. Edit tips there; a tip's `id` is what Got it remembers, so keep an id when only
/// the words change and give a tip that says something new a new id. A hotkey is written
/// `{key:<hotkey id>}` (the ids of `ghostex_settings_catalog`'s hotkey catalog), so each person
/// reads the key they really have ([`fill_new_chat_tip_keys`]).
const NEW_CHAT_TIPS_JSON: &str = include_str!("../../tips/new-chat-tips.json");

#[derive(Clone, Debug, Deserialize)]
pub struct NewChatTip {
    pub id: String,
    /// A Tabler icon name the desktop has as `titlebar/<icon>.svg`.
    pub icon: String,
    pub title: String,
    pub body: String,
    /// Shown before every other tip, in this order, until it is read.
    #[serde(default)]
    pub first: Option<u32>,
    /// A built-in extension id (`ghostex_settings_catalog::built_in_extensions`) that must be on.
    #[serde(default)]
    pub requires: Option<String>,
    /// The systems the tip is true on (`macos`, `windows`, `linux`); every system when absent.
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct NewChatTipsFile {
    tips: Vec<NewChatTip>,
}

/// Every tip in the data file, in its order.
pub fn new_chat_tips() -> &'static [NewChatTip] {
    static TIPS: OnceLock<Vec<NewChatTip>> = OnceLock::new();
    TIPS.get_or_init(|| {
        serde_json::from_str::<NewChatTipsFile>(NEW_CHAT_TIPS_JSON)
            .map(|file| file.tips)
            .unwrap_or_default()
    })
}

/// Whether `id` names a tip in the data file (read ids of retired tips are dropped on the next save).
pub fn is_new_chat_tip_id(id: &str) -> bool {
    new_chat_tips().iter().any(|tip| tip.id == id)
}

/// Whether a tip is true here: its system matches and the feature it needs is on.
pub fn new_chat_tip_applies(
    tip: &NewChatTip,
    platform: &str,
    feature_on: impl Fn(&str) -> bool,
) -> bool {
    tip.platforms
        .as_ref()
        .is_none_or(|platforms| platforms.iter().any(|name| name == platform))
        && tip.requires.as_deref().is_none_or(feature_on)
}

/// The unread tips that apply, in the order the card walks them on day `day` (days since the Unix
/// epoch, local): the `first` tips in their order, then the others shuffled with the day as seed.
pub fn new_chat_tip_order(
    applies: impl Fn(&NewChatTip) -> bool,
    is_read: impl Fn(&str) -> bool,
    day: i64,
) -> Vec<&'static NewChatTip> {
    let unread = new_chat_tips()
        .iter()
        .filter(|tip| !is_read(&tip.id) && applies(tip));
    let (mut first, mut rest): (Vec<_>, Vec<_>) = unread.partition(|tip| tip.first.is_some());
    first.sort_by_key(|tip| tip.first);
    let mut seed = (day as u64) ^ 0x9e37_79b9_7f4a_7c15;
    for index in (1..rest.len()).rev() {
        let pick = (splitmix64(&mut seed) % (index as u64 + 1)) as usize;
        rest.swap(index, pick);
    }
    first.extend(rest);
    first
}

/// `text` with each `{key:<hotkey id>}` replaced by `label(<hotkey id>)`, the key that person has
/// for it on their system; `None` when one of them has no key (unbound, or a host without hotkeys,
/// such as the phone), so the tip is skipped rather than shown with a hole in it.
pub fn fill_new_chat_tip_keys(
    text: &str,
    label: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{key:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 5..];
        let end = after.find('}')?;
        let key = label(&after[..end]).filter(|key| !key.is_empty())?;
        out.push_str(&key);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
