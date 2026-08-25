use axum::http::{header, HeaderMap};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use subtle::ConstantTimeEq;

const EXTENSION_TOKEN_BYTES: usize = 32;

pub(crate) fn new_extension_token() -> String {
    let mut random = [0_u8; EXTENSION_TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut random);
    URL_SAFE_NO_PAD.encode(random)
}

pub(crate) fn request_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let mut parts = value.split(' ');
    let scheme = parts.next()?;
    let token = parts.next()?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

pub(crate) fn tokens_equal(left: &str, right: &str) -> bool {
    left.len() == right.len() && left.as_bytes().ct_eq(right.as_bytes()).into()
}
