//! Which PR links of a Linear issue are kept, and what is left of them. A kept link can become the
//! selector of `gh pr view`, so it is exactly `https://github.com/<owner>/<repo>/pull/<number>`:
//! a real link is trimmed to that, and anything else stays out of `pullRequestUrls`.

use serde_json::{json, Map, Value};

use crate::paths::get_gxserver_paths;

use super::*;

const CANONICAL: &str = "https://github.com/acme/app/pull/40";

/// Links people and Linear's GitHub integration really attach, all of the one PR.
const REAL_SHAPES: [&str; 9] = [
    "https://github.com/acme/app/pull/40",
    "https://github.com/acme/app/pull/40/",
    "https://github.com/acme/app/pull/40/files",
    "https://github.com/acme/app/pull/40/commits",
    "https://github.com/acme/app/pull/40/commits/9f8e7d6",
    "https://github.com/acme/app/pull/40#discussion_r1234567",
    "https://github.com/acme/app/pull/40#issuecomment-1234567",
    "https://github.com/acme/app/pull/40?w=1",
    "https://github.com/acme/app/pull/40/files?diff=unified&w=1#diff-abc123",
];

/// Links that name `github.com/` and `/pull/` without being a GitHub PR. The first two would read
/// as a flag of `gh pr view`.
const NOT_PULL_REQUESTS: [&str; 11] = [
    "--repo=a/github.com/acme/app/pull/1",
    "-R/github.com/acme/app/pull/1",
    "https://evil.example/github.com/acme/app/pull/7",
    "https://github.com.evil.example/acme/app/pull/7",
    "javascript:alert(1)//github.com/acme/app/pull/7",
    "http://github.com/acme/app/pull/7",
    "https://github.com/acme/app/pull/",
    "https://github.com/acme/app/pull/x",
    "https://github.com/acme/app/pull/7x/files",
    "https://github.com/acme/../pull/7",
    "https://github.com/acme/pull/7",
];

fn attachments(urls: &[&str]) -> Value {
    urls.iter()
        .map(|url| json!({ "title": "Link", "url": url }))
        .collect()
}

fn issue_node(identifier: &str, urls: &[&str]) -> Value {
    let mut node = linear_node(
        identifier,
        ("started", "In Progress"),
        "2026-10-20T09:00:00.000Z",
    );
    node["attachments"]["nodes"] = attachments(urls);
    node
}

/// A Linear issue's details as `fetch_linear_issue_detail` reads them, with these attachments.
fn linear_detail(identifier: &str, urls: &[&str]) -> Value {
    let node = issue_node(identifier, urls);
    let _linear = answer_linear_for_test(move |_query, _variables| {
        Ok(json!({ "data": { "issue": node.clone() } }))
    });
    fetch_linear_issue_detail("lin-key-pr-links", identifier).unwrap()
}

/// What each Linear route keeps of these links, by route: the details page, the Work list's feed
/// and the card pass (`refresh_linear_issues`), whose links reach the same `gh pr view`.
fn kept_by_each_route(identifier: &str, urls: &[&str]) -> Vec<(&'static str, Vec<String>)> {
    let details = linear_detail(identifier, urls)["pullRequestUrls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|url| url.as_str().unwrap().to_string())
        .collect();
    let list = parse_linear_list_issue(&issue_node(identifier, urls))
        .unwrap()
        .pull_request_urls;
    let api_key = format!("lin-key-pr-links-{identifier}");
    let node = issue_node(identifier, urls);
    let _linear = answer_linear_for_test(move |_query, _variables| {
        Ok(json!({ "data": { "issues": { "nodes": [node.clone()] } } }))
    });
    refresh_linear_issues(&api_key, &[identifier.to_string()], &[], 1);
    let card = cached_linear_issue(linear_key_fingerprint(&api_key), identifier)
        .unwrap()
        .pull_request_urls;
    vec![("details", details), ("list", list), ("card", card)]
}

#[test]
fn every_real_shape_of_a_pr_link_is_trimmed_to_its_canonical_url_on_every_route() {
    for (index, shape) in REAL_SHAPES.iter().enumerate() {
        for (route, kept) in kept_by_each_route(&format!("SHP-{}", index + 1), &[shape]) {
            assert_eq!(kept, vec![CANONICAL], "{route}: {shape}");
        }
    }
}

#[test]
fn links_of_one_pr_are_kept_once_in_the_order_they_came() {
    let other = "https://github.com/acme/web/pull/9";
    let urls = [REAL_SHAPES[2], other, REAL_SHAPES[5], CANONICAL];
    for (route, kept) in kept_by_each_route("ORD-1", &urls) {
        assert_eq!(kept, vec![CANONICAL, other], "{route}");
    }
}

#[test]
fn a_link_that_is_not_a_github_pr_is_left_out_on_every_route() {
    for (index, url) in NOT_PULL_REQUESTS.iter().enumerate() {
        for (route, kept) in kept_by_each_route(&format!("NOT-{}", index + 1), &[url]) {
            assert_eq!(kept, Vec::<String>::new(), "{route}: {url}");
        }
    }
}

#[test]
fn a_link_is_trimmed_only_to_a_github_pr_it_really_names() {
    for shape in REAL_SHAPES {
        assert_eq!(
            canonical_github_pull_request_url(shape).as_deref(),
            Some(CANONICAL),
            "{shape}"
        );
    }
    for url in NOT_PULL_REQUESTS {
        assert_eq!(canonical_github_pull_request_url(url), None, "{url}");
    }
    // What comes before the number is never trimmed into a PR it doesn't name.
    for url in [
        "https://github.com/acme/app?next=/pull/7",
        "https://github.com/acme/app#/pull/7",
        "https://github.com/acme/app/issues/7/pull/8",
        "https://github.com/acme/app/pull/+7",
        "https://github.com/ac%6De/app/pull/7",
    ] {
        assert_eq!(canonical_github_pull_request_url(url), None, "{url}");
    }
    // A long tail goes; a long PR link is not one.
    let long_query = format!("{CANONICAL}?w={}", "1".repeat(1_000_000));
    assert_eq!(
        canonical_github_pull_request_url(&long_query).as_deref(),
        Some(CANONICAL)
    );
    let long_owner = format!("https://github.com/{}/app/pull/40", "a".repeat(3000));
    assert_eq!(canonical_github_pull_request_url(&long_owner), None);
    // Only an exact link is canonical, as a command source's must be.
    assert!(is_canonical_github_pull_request_url(CANONICAL));
    for shape in &REAL_SHAPES[1..] {
        assert!(!is_canonical_github_pull_request_url(shape), "{shape}");
    }
}

/// The details of one Linear ticket, its issue read a moment ago with these attachments. Nothing
/// asks Linear. `gh` is taken as signed in: a test that lets a wrong selector through runs it.
fn details_with(project_id: &str, cwd: &str, urls: &[&str]) -> Value {
    let _gh = crate::session_git_status::assume_gh_cli_available_for_test();
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
    let api_key = format!("lin-key-{project_id}");
    let detail = linear_detail("ACM-40", urls);
    cached_detail(
        format!("linear:{}:ACM-40", linear_key_fingerprint(&api_key)),
        true,
        || Ok(detail),
    )
    .unwrap();
    let projects = vec![work_project(
        project(project_id, cwd),
        Vec::new(),
        Some(&api_key),
        "acme/app",
        WorkSourceId::Linear,
    )];
    read_work_item(
        &paths,
        &projects,
        &WorkFeedPlan::default(),
        Some(project_id),
        &WorkItemRef::Ticket(TicketRef::new(WorkSourceId::Linear, "ACM-40")),
        false,
    )
}

fn link_urls(read: &Value) -> Vec<&str> {
    read["links"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|link| link["url"].as_str())
        .collect()
}

#[test]
fn a_real_link_reads_its_pr_by_the_canonical_url_and_is_not_listed_as_a_link() {
    let cwd = "/gx-char/pr-links/real";
    // What the page read a moment ago under the canonical selector, so `gh` isn't asked.
    let pull_request = json!({ "number": 40, "title": "Read by its canonical URL" });
    let cached: Map<String, Value> = pull_request.as_object().unwrap().clone();
    cached_detail(format!("pr:{cwd}:{CANONICAL}"), true, || {
        Ok(Value::Object(cached))
    })
    .unwrap();
    let read = details_with(
        "PPrLinksReal",
        cwd,
        &["https://github.com/acme/app/pull/40/files#diff-abc123"],
    );
    assert_eq!(read["pullRequest"], pull_request);
    assert_eq!(read["errors"], json!([]));
    assert_eq!(link_urls(&read), Vec::<&str>::new());
}

#[test]
fn a_link_that_could_read_as_a_flag_never_becomes_the_selector_and_stays_a_plain_link() {
    for (index, url) in NOT_PULL_REQUESTS.iter().enumerate() {
        let read = details_with(
            &format!("PPrLinksNot{index}"),
            &format!("/gx-char/pr-links/not-{index}"),
            &[url],
        );
        assert_eq!(read["pullRequest"], Value::Null, "{url}");
        assert_eq!(read["errors"], json!([]), "{url}");
        assert_eq!(link_urls(&read), vec![*url], "{url}");
    }
}
