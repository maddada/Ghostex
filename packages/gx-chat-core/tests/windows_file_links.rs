//! Chat file links written the way agents write Windows file URIs (`file:///C:/...`, `/C:/...`)
//! reach the host as a drive path it can open (GitHub issue #391), in the transcript and in the
//! composer alike, while WSL and POSIX paths stay as they were.

use ghostex_gx_chat_core::composer::links as composer;
use ghostex_gx_chat_core::transcript::links as transcript;

fn transcript_file(href: &str) -> String {
    match transcript::classify_link_href(href) {
        transcript::LinkTarget::File(path) => path,
        other => panic!("{href} is not a file link: {other:?}"),
    }
}

fn composer_file(href: &str) -> String {
    match composer::classify_link_href(href) {
        composer::LinkTarget::File(path) => path,
        other => panic!("{href} is not a file link: {other:?}"),
    }
}

fn assert_both(href: &str, expected: &str) {
    assert_eq!(transcript_file(href), expected, "transcript: {href}");
    assert_eq!(composer_file(href), expected, "composer: {href}");
}

#[test]
fn file_uri_with_drive_letter_becomes_drive_path() {
    assert_both(
        "file:///C:/development/project/report.md",
        "C:/development/project/report.md",
    );
    assert_both("FILE:///d:/x/y.md", "d:/x/y.md");
}

#[test]
fn slash_before_drive_letter_is_dropped() {
    assert_both(
        "/C:/development/project/report.md",
        "C:/development/project/report.md",
    );
    assert_both("/c:\\Users\\dave\\notes.md", "c:\\Users\\dave\\notes.md");
    assert_both("/z:/a.txt", "z:/a.txt");
}

#[test]
fn localhost_host_names_the_same_file() {
    assert_both("file://localhost/C:/x/report.md", "C:/x/report.md");
    assert_both(
        "file://LOCALHOST/Users/dave/report.md",
        "/Users/dave/report.md",
    );
}

#[test]
fn percent_encoded_spaces_and_drive_colon_decode() {
    assert_both(
        "file:///C:/My%20Projects/report%20v2.md",
        "C:/My Projects/report v2.md",
    );
    assert_both("/C:/My%20Projects/a.md", "C:/My Projects/a.md");
    assert_both("file:///c%3A/Users/dave/a.md", "c:/Users/dave/a.md");
}

#[test]
fn editor_coordinates_survive_on_drive_paths() {
    assert_both("file:///C:/x/main.rs:42", "C:/x/main.rs");
    let position = transcript::file_position_from_href("file:///C:/x/main.rs:42:7")
        .expect("transcript position");
    assert_eq!((position.line, position.column), (42, Some(7)));
    let position =
        composer::file_position_from_href("/C:/x/main.rs:42").expect("composer position");
    assert_eq!(position.line, 42);
}

#[test]
fn wsl_and_posix_paths_are_unchanged() {
    assert_both("/mnt/c/Users/dave/report.md", "/mnt/c/Users/dave/report.md");
    assert_both(
        "file:///mnt/c/Users/dave/report.md",
        "/mnt/c/Users/dave/report.md",
    );
    assert_both("/Users/dave/report.md", "/Users/dave/report.md");
    assert_both("file:///home/dave/a%20b.md", "/home/dave/a b.md");
    assert_both("C:/already/a/drive/path.md", "C:/already/a/drive/path.md");
    // Not a drive: one letter and a colon with no slash after it.
    assert_both("/C:notes.md", "/C:notes.md");
    assert_both("/ab:/x", "/ab:/x");
}
