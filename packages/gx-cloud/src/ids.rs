//! Ghostex's own ids for boxes and snapshots: `gx_<12 lowercase base32>`.

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
pub const GX_ID_PREFIX: &str = "gx_";
const BODY_LEN: usize = 12;

/// A `gx_…` id from at least 8 bytes of randomness the caller supplies (this crate reads no
/// clock and no random source).
pub fn gx_id_from_entropy(entropy: &[u8]) -> String {
    let mut bits: u128 = 0;
    for byte in entropy.iter().take(16) {
        bits = (bits << 8) | u128::from(*byte);
    }
    let mut body = String::with_capacity(BODY_LEN);
    for index in 0..BODY_LEN {
        body.push(ALPHABET[((bits >> (index * 5)) & 31) as usize] as char);
    }
    format!("{GX_ID_PREFIX}{body}")
}

/// A well-formed `gx_…` id (safe to put in a container name, a label or a file name).
pub fn is_gx_id(value: &str) -> bool {
    value.strip_prefix(GX_ID_PREFIX).is_some_and(|body| {
        body.len() == BODY_LEN && body.bytes().all(|byte| ALPHABET.contains(&byte))
    })
}

/// The short name the chip and lists show, `box-<first 3 of the id>`.
pub fn box_display_name(box_id: &str) -> String {
    let body = box_id.strip_prefix(GX_ID_PREFIX).unwrap_or(box_id);
    format!("box-{}", body.chars().take(3).collect::<String>())
}
