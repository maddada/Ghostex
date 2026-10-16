/// CDXC:ServerDaemon 2026-10-06 WHY:
/// WSL health failures can contain 4096 characters of stderr, overwhelming the toast and first-run setup message; keep those readable while startup diagnostics retain the full detail.
pub(super) fn bounded_health_detail(detail: &str) -> String {
    match detail.char_indices().nth(300) {
        Some((end, _)) => format!("{}…", &detail[..end]),
        None => detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_detail_preserves_short_and_exact_limit_messages() {
        for detail in [
            String::new(),
            "Connection refused".to_string(),
            "é".repeat(300),
        ] {
            assert_eq!(bounded_health_detail(&detail), detail);
        }
    }

    #[test]
    fn health_detail_truncates_on_a_char_boundary() {
        for character in ["a", "é", "界", "🦀"] {
            let detail = character.repeat(4096);
            let shortened = bounded_health_detail(&detail);
            assert_eq!(shortened, format!("{}…", character.repeat(300)));
            assert_eq!(shortened.chars().count(), 301);
            assert_eq!(detail.chars().count(), 4096);
        }
    }
}
