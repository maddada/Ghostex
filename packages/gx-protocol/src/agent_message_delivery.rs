//! How a message one agent sent another is recognised on both sides of the wire: gxserver's
//! delivery watch looks for it in the recipient's transcript, and the sender's chat matches the
//! watch's word to the card of a send whose result it could not read.
//!
//! The `agentMessageDeliveries` chat field (`ChatSideState::agent_message_deliveries`) maps each
//! settled send's `sendRequestId` to `{ "state": "delivered" }` or `{ "state": "failed", "failure":
//! "…" }`, and, from gxserver 2026-10-31 on, carries every send the sender's chat may have to
//! follow under [`SENDS_KEY`]: `[{ "sendRequestId", "projectId", "sessionId", "fingerprint",
//! "sentAtMs", "state", "failure"? }]`, where `state` is `waiting`, `delivered`, `failed` or
//! `untracked` (gxserver stopped watching it) and `fingerprint` is [`delivery_fingerprint`] of the
//! message.
//!
//! CDXC:SessionChat 2026-10-31 WHY:
//! A coordinator that filters its send's output (`ghostex agents send … --queue "…" | grep -m1 '"status"'`) keeps the status but loses the `sendRequestId` the card looked gxserver's outcome up by, so the card read "Queued until its turn ends" for good after the thread had acted on the message. gxserver now names every watched send by its recipient, message fingerprint and time too, so the card finds its send without the id, and a card it cannot find stops claiming Queued. The list rides under a key no send id can take, so a chat core that predates it reads the field exactly as before.
//! SEE-ALSO: server/src/agent_message_deliveries.rs (writes it), packages/gx-chat-core/src/transcript/agent_message_card.rs `apply_live_delivery` (reads it).

/// The key the list of watched sends rides under inside `agentMessageDeliveries`.
pub const SENDS_KEY: &str = "@sends";

/// Normalized characters from the start of a message a transcript row must contain...
const DELIVERY_HEAD_CHARS: usize = 80;
/// ...and from its end, for a message long enough to have a distinct end.
const DELIVERY_TAIL_CHARS: usize = 40;

/// Text minus whatever the trip to the transcript can change: whitespace, control characters,
/// and the Control Pictures signs gxserver writes in their place.
pub fn normalize_delivery_text(text: &str) -> String {
    text.chars()
        .filter(|character| {
            !character.is_whitespace()
                && !character.is_control()
                && !matches!(character, '\u{2400}'..='\u{243f}')
        })
        .collect()
}

/// What a transcript row must contain for `body` to count as delivered: the start of the body
/// and, when it is long enough, its end. Empty when the body has nothing to match.
/// CDXC:Coordinators 2026-10-04 WHY: the match is on the body, never on the "Message from another agent" header every agent message shares, and on both ends so a different message that opens the same way does not count.
pub fn delivery_needles(body: &str) -> Vec<String> {
    let normalized: Vec<char> = normalize_delivery_text(body).chars().collect();
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut needles = vec![normalized
        .iter()
        .take(DELIVERY_HEAD_CHARS)
        .collect::<String>()];
    if normalized.len() > DELIVERY_HEAD_CHARS + DELIVERY_TAIL_CHARS {
        needles.push(
            normalized[normalized.len() - DELIVERY_TAIL_CHARS..]
                .iter()
                .collect(),
        );
    }
    needles
}

/// A short stable name for a message's [`delivery_needles`] (64-bit FNV-1a, in hex); empty when
/// the message has nothing to match.
pub fn delivery_fingerprint(needles: &[String]) -> String {
    if needles.is_empty() {
        return String::new();
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (index, needle) in needles.iter().enumerate() {
        if index > 0 {
            hash ^= u64::from(b'\n');
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        for byte in needle.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// [`delivery_fingerprint`] of a message body.
pub fn message_fingerprint(body: &str) -> String {
    delivery_fingerprint(&delivery_needles(body))
}
