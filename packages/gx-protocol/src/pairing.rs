//! Pairing v2 (GitHub #495, 11.0.1): device keys, a Pending step and three matching words.
//!
//! A phone that scans the computer's Show QR sheet sends its own public device key with the
//! one-time nonce from the QR. gxserver keeps it Pending and shows three words on the computer
//! (and on the phones already paired); the phone works out the same three words itself from the
//! nonce and its own key. The owner checks the words match and approves with a signed approval
//! (`SensitiveAction::PairingApproveDevice`); only then does gxserver write the phone's SSH key
//! and issue its device token, bound to the device key.
//!
//! Byte formats here are pinned by tests; the phone's TypeScript port
//! (`apps/mobile/app/src/machines/pairingWords.ts`) must produce the same bytes and words.
//! This crate carries no hash function (serde only, see Cargo.toml), so callers hash with
//! SHA-256 themselves and pass the digest in.

use serde::{Deserialize, Serialize};

/// The device key algorithm every phone uses: P-256 ECDSA with SHA-256 (Secure Enclave on iOS,
/// Android Keystore, StrongBox when present). Public key: the 65-byte uncompressed X9.63 point,
/// base64 standard with padding; signature: ASN.1 DER, base64 standard.
pub const DEVICE_KEY_ALGORITHM: &str = "ecdsaP256Sha256";

/// First line of the bytes hashed into the match words.
pub const PAIRING_WORDS_DOMAIN: &str = "ghostex-pairing-words-v1";
/// First line of the payload a pending phone signs to read its pairing status.
pub const PAIRING_STATUS_DOMAIN: &str = "ghostex-pairing-status-v1";
/// First line of the payload a paired phone signs to renew its device token.
pub const DEVICE_TOKEN_RENEW_DOMAIN: &str = "ghostex-device-token-v1";
/// First line of [`easy_connect_client_keys_payload`].
pub const EASY_CONNECT_CLIENT_KEYS_DOMAIN: &str = "ghostex-easy-connect-client-keys-v1";

/// How many words both screens show.
pub const PAIRING_WORD_COUNT: usize = 3;

/// The bytes whose SHA-256 picks the match words:
///
/// ```text
/// ghostex-pairing-words-v1
/// <the QR's one-time nonce, as scanned>
/// <the phone's device public key, base64 as sent>
/// ```
///
/// UTF-8, `\n` between lines, no trailing newline. The nonce never leaves the QR, the phone and
/// this computer, so nobody else can work out the words for a key of their own.
pub fn pairing_words_input(nonce: &str, device_public_key: &str) -> Vec<u8> {
    format!("{PAIRING_WORDS_DOMAIN}\n{nonce}\n{device_public_key}").into_bytes()
}

/// The three words for `digest` (SHA-256 of [`pairing_words_input`]): its first three bytes,
/// each an index into [`PAIRING_WORDS`].
pub fn pairing_match_words(digest: &[u8]) -> Vec<&'static str> {
    digest
        .iter()
        .take(PAIRING_WORD_COUNT)
        .map(|byte| PAIRING_WORDS[usize::from(*byte)])
        .collect()
}

/// The payload a pending phone signs with its device key to ask for its status:
/// `ghostex-pairing-status-v1\n<pendingId>\n<timestampMs>`.
pub fn pairing_status_payload(pending_id: &str, timestamp_ms: u64) -> Vec<u8> {
    format!("{PAIRING_STATUS_DOMAIN}\n{pending_id}\n{timestamp_ms}").into_bytes()
}

/// The payload a paired phone signs with its device key to get a fresh device token:
/// `ghostex-device-token-v1\n<deviceId>\n<timestampMs>`.
pub fn device_token_renew_payload(device_id: &str, timestamp_ms: u64) -> Vec<u8> {
    format!("{DEVICE_TOKEN_RENEW_DOMAIN}\n{device_id}\n{timestamp_ms}").into_bytes()
}

/// The payload a paired computer signs with its pairing device key to register Easy Connect client
/// keys of its own on the computer it paired with: `ghostex-easy-connect-client-keys-v1\n<device
/// public key>\n<timestampMs>` and then one `<purpose>=<client key>` line per key, sorted by purpose.
pub fn easy_connect_client_keys_payload(
    device_public_key: &str,
    timestamp_ms: u64,
    keys: &[(String, String)],
) -> Vec<u8> {
    let mut sorted: Vec<&(String, String)> = keys.iter().collect();
    sorted.sort();
    let mut payload =
        format!("{EASY_CONNECT_CLIENT_KEYS_DOMAIN}\n{device_public_key}\n{timestamp_ms}");
    for (purpose, key) in sorted {
        payload.push('\n');
        payload.push_str(purpose);
        payload.push('=');
        payload.push_str(key);
    }
    payload.into_bytes()
}

/// The device key a phone sends with `/api/pairDevice`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceKey {
    /// [`DEVICE_KEY_ALGORITHM`].
    pub algorithm: String,
    /// The 65-byte uncompressed P-256 point, base64 standard.
    pub public_key: String,
}

/// Where a pairing stands, as `/api/pairingStatus` answers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PairingStatus {
    /// Waiting for the owner to compare the words and approve.
    Pending,
    /// Approved: the reply carries the device id and token.
    Approved,
    /// The owner said no.
    Denied,
    /// Nobody answered in time, or the computer restarted.
    Expired,
}

/// A device waiting for approval, as `/api/pairedDevices` lists it under `pending`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub words: Vec<String>,
    /// The approval request (`SensitiveAction::PairingApproveDevice`) to approve or deny.
    pub approval_id: String,
    pub requested_at_ms: u64,
    pub expires_at_ms: u64,
    /// A phone that signs in over SSH (Tailscale, the local network) and asked through that
    /// login (`/api/pairOverSsh`), not with the Show QR code.
    #[serde(default)]
    pub over_ssh: bool,
}

/// 256 short, distinct, everyday English words: one per byte value. Never reorder or edit: the
/// phone's copy must match index for index (pinned by `words_match_the_shared_vector`).
pub const PAIRING_WORDS: [&str; 256] = [
    "acorn", "actor", "agent", "alarm", "album", "alley", "amber", "anchor", "angle", "ankle",
    "apple", "apron", "arch", "arrow", "aspen", "atlas", "attic", "autumn", "avocado", "badge",
    "bagel", "baker", "bamboo", "banana", "banjo", "barn", "basket", "beach", "beacon", "beard",
    "beaver", "bell", "berry", "bicycle", "bison", "blanket", "blossom", "boat", "bonfire", "book",
    "bottle", "bread", "brick", "bridge", "broom", "bubble", "bucket", "buffalo", "butter",
    "button", "cabin", "cactus", "camel", "camera", "candle", "canoe", "canyon", "carpet",
    "carrot", "castle", "cedar", "cello", "chair", "cherry", "chess", "chimney", "circle", "clock",
    "cloud", "clover", "cobalt", "coconut", "comet", "compass", "copper", "coral", "cotton",
    "cougar", "cradle", "crane", "crayon", "cricket", "crown", "cup", "daisy", "dancer", "desert",
    "diamond", "dolphin", "donkey", "dragon", "drum", "eagle", "easel", "echo", "elbow",
    "elephant", "elm", "ember", "engine", "falcon", "feather", "fern", "fiddle", "field", "flag",
    "flamingo", "flute", "forest", "fossil", "fountain", "fox", "garden", "garlic", "gecko",
    "ginger", "giraffe", "glacier", "globe", "goat", "grape", "guitar", "hammer", "harbor", "harp",
    "hazel", "helmet", "heron", "hill", "honey", "horizon", "hornet", "igloo", "iris", "island",
    "ivory", "jacket", "jaguar", "jasmine", "jelly", "jigsaw", "juniper", "kayak", "kettle",
    "kitten", "kiwi", "koala", "ladder", "lagoon", "lamp", "lantern", "lemon", "leopard",
    "lettuce", "lily", "lion", "lizard", "lobster", "locket", "lotus", "magnet", "mango", "maple",
    "marble", "meadow", "melon", "meteor", "mirror", "mitten", "monkey", "moose", "mosaic",
    "muffin", "mushroom", "napkin", "nectar", "needle", "nest", "nutmeg", "oasis", "ocean",
    "olive", "onion", "orange", "orbit", "orchid", "otter", "owl", "oyster", "paddle", "palace",
    "panda", "parrot", "peach", "peanut", "pearl", "pebble", "pelican", "pencil", "pepper",
    "piano", "pillow", "pine", "planet", "plum", "pocket", "pony", "poppy", "potato", "pumpkin",
    "puzzle", "quartz", "quilt", "rabbit", "radish", "rainbow", "raven", "ribbon", "river",
    "robin", "rocket", "saddle", "salmon", "sapphire", "scarf", "seagull", "shell", "silver",
    "sparrow", "spider", "spoon", "squirrel", "star", "stone", "sunset", "swan", "teapot", "tiger",
    "tomato", "tractor", "trumpet", "tulip", "turtle", "umbrella", "valley", "velvet", "violin",
    "volcano", "wagon", "walnut", "whale", "willow", "window", "wizard", "yogurt", "zebra",
];
