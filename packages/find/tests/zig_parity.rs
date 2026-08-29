//! Cross-implementation parity: every vector in `tests/fixtures/fuzzy-zig-vectors.tsv`
//! was produced by the original Zig `fuzzy.zig` matcher. The Rust port must agree
//! on match/no-match, on the score, and on the highlight positions.

use std::path::PathBuf;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex pair"))
        .collect()
}

#[test]
fn rust_matcher_agrees_with_zig_vectors() {
    let path = std::env::var("ZEHN_FUZZY_VECTORS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fuzzy-zig-vectors.tsv")
        });
    let data = std::fs::read_to_string(&path).expect("fuzzy vector fixture");
    let mut matcher = ghostex_find::fuzzy::Matcher::new();
    let mut checked = 0usize;
    for (line_no, line) in data.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let needle = unhex(parts.next().expect("needle"));
        let hay = unhex(parts.next().expect("hay"));
        let score = parts.next().expect("score");
        let positions = parts.next().unwrap_or("");
        let got = matcher.matches(&needle, &hay);
        if score == "none" {
            assert!(
                got.is_none(),
                "line {}: zig rejected but rust matched with {:?}",
                line_no + 1,
                got.map(|m| m.score)
            );
            checked += 1;
            continue;
        }
        let m = got.unwrap_or_else(|| panic!("line {}: zig matched but rust did not", line_no + 1));
        assert_eq!(m.score.to_string(), score, "line {} score", line_no + 1);
        let want: Vec<u16> = if positions.is_empty() {
            Vec::new()
        } else {
            positions
                .split(',')
                .map(|p| p.parse().expect("position"))
                .collect()
        };
        assert_eq!(
            m.highlights(),
            want.as_slice(),
            "line {} positions",
            line_no + 1
        );
        checked += 1;
    }
    assert!(
        checked > 100,
        "expected a meaningful vector count, got {checked}"
    );
}
