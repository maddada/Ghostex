//! File links (`/open-file/`) as a browser reads them: a page served from one runs sandboxed, and
//! its `null` origin gets no cross-origin read of the link's other files.

use super::*;

fn get_from_null_origin(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("origin", "null")
        .body(Body::empty())
        .expect("request")
}

fn header_text(response: &Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

#[tokio::test]
async fn a_sandboxed_link_page_cannot_read_the_links_other_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_app_state(get_gxserver_paths(Some(temp.path().join("gx"))));
    let root = fs::canonicalize(temp.path()).expect("root").join("project");
    fs::create_dir_all(root.join("docs")).expect("docs");
    fs::create_dir_all(root.join("notes")).expect("notes");
    fs::write(
        root.join("docs/page.html"),
        "<script>fetch('../notes/private.txt')</script>",
    )
    .expect("page");
    fs::write(root.join("notes/private.txt"), "private").expect("note");
    fs::write(root.join("docs/font.woff2"), "font").expect("font");

    let page = handle_http_request(
        state.clone(),
        get_from_null_origin(&crate::file_links::link_path(&root, "docs/page.html")),
    )
    .await
    .expect("page answer");
    assert_eq!(page.status(), StatusCode::OK);
    let policy = header_text(&page, "content-security-policy").unwrap_or_default();
    assert!(policy.starts_with("sandbox "), "{policy}");
    assert!(!policy.contains("allow-same-origin"), "{policy}");

    let note = handle_http_request(
        state.clone(),
        get_from_null_origin(&crate::file_links::link_path(&root, "notes/private.txt")),
    )
    .await
    .expect("note answer");
    assert_eq!(note.status(), StatusCode::OK);
    assert_eq!(header_text(&note, "access-control-allow-origin"), None);

    let font = handle_http_request(
        state.clone(),
        get_from_null_origin(&crate::file_links::link_path(&root, "docs/font.woff2")),
    )
    .await
    .expect("font answer");
    assert_eq!(
        header_text(&font, "access-control-allow-origin").as_deref(),
        Some("*")
    );

    // The API keeps answering the app's own file:// pages, whose origin is `null` as well.
    let api = handle_http_request(state.clone(), get_from_null_origin("/api/health/server"))
        .await
        .expect("api answer");
    assert_eq!(
        header_text(&api, "access-control-allow-origin").as_deref(),
        Some("null")
    );
}
