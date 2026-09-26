/// The composer stores pasted pictures as numbered references in its text, including queued
/// drafts. OpenCode's API needs the bytes as attachments; it does not interpret those links.
pub(super) fn image_references(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[Image #") {
        rest = &rest[start + 8..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        let label = rest[digits..].strip_prefix('·').unwrap_or(&rest[digits..]);
        let Some(destination) = label.strip_prefix("](") else {
            continue;
        };
        let mut depth = 1;
        let mut end = None;
        for (index, ch) in destination.char_indices() {
            match ch {
                '\n' | '\r' => break,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { continue };
        let path = destination[..end].trim();
        let path = path
            .strip_prefix('<')
            .and_then(|p| p.strip_suffix('>'))
            .unwrap_or(path);
        if !path.is_empty() && !paths.iter().any(|existing| existing == path) {
            paths.push(path.to_string());
        }
        rest = &destination[end + 1..];
    }
    paths
}
