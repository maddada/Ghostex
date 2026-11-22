use super::input::parse_sgr_mouse;
use super::*;
use crate::index::{day_key, SearchIndex, SECONDS_PER_DAY, UNKNOWN_DAY_KEY};
use crate::unicode as uni;

fn visible_cols_no_ansi(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut cols = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && (bytes[i] < b'@' || bytes[i] > b'~') {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            continue;
        }
        if bytes[i] == b'\r' || bytes[i] == b'\n' {
            i += 1;
            continue;
        }
        let (cp, len) = uni::decode(&bytes[i..]);
        cols += uni::char_width(cp);
        i += len;
    }
    cols
}

fn empty_index() -> SearchIndex {
    SearchIndex::build(
        "/nonexistent-home",
        std::path::Path::new("/nonexistent-cache"),
        "/nonexistent/favorites".into(),
    )
}

#[test]
fn query_render_keeps_typed_text_visible_with_cursor() {
    let mut index = empty_index();
    let mut tui = Tui::new(&mut index);
    tui.cols = 24;
    tui.query = b"abcdef".to_vec();
    tui.query_cursor = tui.query.len();
    let mut out = String::new();
    tui.write_query_with_cursor(&mut out, 20);
    assert!(out.contains("abcdef"));
    assert!(out.contains("\x1b[7m \x1b[0m"));
}

#[test]
fn prompt_line_keeps_a_spare_column_to_avoid_autowrap() {
    for cols in [20u16, 64, 80, 120] {
        let mut index = empty_index();
        let mut tui = Tui::new(&mut index);
        tui.cols = cols;
        tui.query =
            b"a deliberately long search that used to push the help text past the edge".to_vec();
        tui.query_cursor = tui.query.len();
        let mut out = String::new();
        tui.write_prompt_line(&mut out);
        assert!(
            visible_cols_no_ansi(&out) < cols as usize,
            "prompt line overflowed at {cols} cols"
        );
        assert!(out.ends_with("\r\n"));
    }
}

#[test]
fn prompt_line_advertises_the_remapped_hotkeys() {
    let mut index = empty_index();
    let mut tui = Tui::new(&mut index);
    tui.cols = 120;
    let mut out = String::new();
    tui.write_prompt_line(&mut out);
    assert!(out.contains("^g agents"));
    assert!(out.contains("^j projects"));
    assert!(!out.contains("^t agents"));
    assert!(!out.contains("^r projects"));
}

#[test]
fn query_supports_readline_style_editing() {
    let mut index = empty_index();
    let mut tui = Tui::new(&mut index);
    tui.query = b"hello world".to_vec();
    tui.query_cursor = tui.query.len();

    tui.move_left();
    tui.move_left();
    assert_eq!(tui.query_cursor, 9);
    tui.insert_query_byte(b'!');
    assert_eq!(tui.query, b"hello wor!ld");

    tui.backspace();
    assert_eq!(tui.query, b"hello world");
    assert_eq!(tui.query_cursor, 9);

    tui.kill_to_beginning();
    assert_eq!(tui.query, b"ld");
    assert_eq!(tui.query_cursor, 0);

    tui.kill_to_end();
    assert!(tui.query.is_empty());
}

#[test]
fn query_supports_word_movement_and_word_deletion() {
    let mut index = empty_index();
    let mut tui = Tui::new(&mut index);
    tui.query = b"alpha beta gamma".to_vec();
    tui.query_cursor = tui.query.len();

    tui.move_word_left();
    assert_eq!(tui.query_cursor, 11);
    tui.move_word_left();
    assert_eq!(tui.query_cursor, 6);
    tui.move_word_right();
    assert_eq!(tui.query_cursor, 10);

    tui.query_cursor = tui.query.len();
    tui.delete_word_backward();
    assert_eq!(tui.query, b"alpha beta ");
}

#[test]
fn day_and_last_active_labels_match_the_original_wording() {
    let now = 20_000 * SECONDS_PER_DAY;
    assert_eq!(format_day_header(day_key(now), now), "Today");
    assert_eq!(format_day_header(day_key(now) - 1, now), "Yesterday");
    assert_eq!(format_day_header(day_key(now) - 3, now), "3 days ago");
    assert_eq!(format_day_header(UNKNOWN_DAY_KEY, now), "Unknown day");

    assert_eq!(format_last_active_compact(0, now), "unknown");
    assert_eq!(format_last_active_compact(now - 5, now), "now");
    assert_eq!(format_last_active_compact(now - 120, now), "2m ago");
    assert_eq!(format_last_active_compact(now - 7_200, now), "2h ago");
    assert_eq!(
        format_last_active_compact(now - 2 * SECONDS_PER_DAY, now),
        "2d ago"
    );
    assert!(format_last_active_full(now).starts_with("last active "));
}

#[test]
fn sgr_mouse_wheel_events_parse() {
    let (ev, consumed) = parse_sgr_mouse(b"\x1b[<65;10;20M").expect("wheel event");
    assert_eq!(consumed, 12);
    assert_eq!(ev.button & 64, 64);
    assert_eq!(ev.button & 3, 1);
    assert!(parse_sgr_mouse(b"\x1b[<65;10").is_none());
}
