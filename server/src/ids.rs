use rand::{distributions::Uniform, Rng};

const DIGITS: &[u8] = b"0123456789";
const LOWERCASE_OR_DIGIT: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/*
CDXC:ServerApi 2026-06-14-20:37:
Server identity is stable daemon identity, not per-process runtime metadata. Match the TypeScript S[0-9][a-z0-9] shape so client refs stay compatible when Rust owns the control plane.
*/
pub fn create_server_id() -> String {
    format!(
        "S{}{}",
        random_char(DIGITS),
        random_char(LOWERCASE_OR_DIGIT)
    )
}

pub fn create_project_id() -> String {
    format!("P{}{}", random_char(DIGITS), random_body(3))
}

pub fn create_session_id() -> String {
    format!("G{}{}", random_char(DIGITS), random_body(3))
}

pub fn is_gxserver_server_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 3
        && bytes[0] == b'S'
        && bytes[1].is_ascii_digit()
        && (bytes[2].is_ascii_lowercase() || bytes[2].is_ascii_digit())
}

pub fn is_gxserver_project_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 5
        && bytes[0] == b'P'
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

pub fn is_gxserver_session_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 5
        && bytes[0] == b'G'
        && bytes[1].is_ascii_digit()
        && bytes[2..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

pub fn create_global_session_ref(server_id: &str, project_id: &str, session_id: &str) -> String {
    format!("{server_id}:{project_id}:{session_id}")
}

pub fn create_zmx_session_name(server_id: &str, project_id: &str, session_id: &str) -> String {
    format!("{server_id}-{project_id}-{session_id}")
}

fn random_body(length: usize) -> String {
    (0..length)
        .map(|_| random_char(LOWERCASE_OR_DIGIT))
        .collect()
}

fn random_char(chars: &[u8]) -> char {
    let range = Uniform::new(0, chars.len());
    chars[rand::thread_rng().sample(range)] as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_id_matches_protocol_shape() {
        assert!(is_gxserver_server_id(&create_server_id()));
        assert!(!is_gxserver_server_id("server"));
    }

    #[test]
    fn project_and_session_ids_match_protocol_shape() {
        assert!(is_gxserver_project_id(&create_project_id()));
        assert!(is_gxserver_session_id(&create_session_id()));
        assert_eq!(
            create_global_session_ref("S7k", "P3a91", "G8v20"),
            "S7k:P3a91:G8v20"
        );
        assert_eq!(
            create_zmx_session_name("S7k", "P3a91", "G8v20"),
            "S7k-P3a91-G8v20"
        );
    }
}
