//! The boot ticket: `BoxBootTicket` JSON, base64url without padding, on `ghostex box boot`'s
//! command line. It carries no secret (exec command lines can reach a provider's logs).

use ghostex_gx_protocol::cloud_box::BoxBootTicket;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode_ticket(ticket: &BoxBootTicket) -> String {
    let json = serde_json::to_vec(ticket).unwrap_or_default();
    base64url_encode(&json)
}

pub fn decode_ticket(text: &str) -> Result<BoxBootTicket, String> {
    let bytes = base64url_decode(text.trim()).ok_or("The boot ticket is not base64url.")?;
    serde_json::from_slice(&bytes).map_err(|error| format!("The boot ticket is not valid: {error}"))
}

fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let chars = chunk.len() + 1;
        for index in 0..chars {
            out.push(ALPHABET[((triple >> (18 - index * 6)) & 63) as usize] as char);
        }
    }
    out
}

fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for byte in text.bytes() {
        let value = ALPHABET.iter().position(|candidate| *candidate == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}
