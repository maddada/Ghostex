use serde_json::{json, Map, Value};

use super::*;
use crate::work_mode::{IssuesWorkflowParts, WorkSourceId};

fn fields(linear: bool, github: bool, tracker: WorkSourceId) -> Value {
    let mut output = Map::new();
    insert_issues_workflow_fields(
        &mut output,
        IssuesWorkflowParts::from_setup(linear, github),
        tracker,
    );
    Value::Object(output)
}

#[test]
fn issues_workflow_is_off_until_linear_or_gh_is_set_up() {
    assert_eq!(fields(false, false, WorkSourceId::Github), json!({}));
}

#[test]
fn issues_workflow_with_gh_only_has_no_linear_parts() {
    assert_eq!(
        fields(false, true, WorkSourceId::Github),
        json!({ "workMode": true, "workTracker": "github" })
    );
}

#[test]
fn issues_workflow_with_a_linear_key_only_turns_github_parts_off() {
    assert_eq!(
        fields(true, false, WorkSourceId::Linear),
        json!({ "workMode": true, "workLinear": true, "workGithubOff": true, "workTracker": "linear" })
    );
}

#[test]
fn issues_workflow_with_both_set_up_has_every_part() {
    assert_eq!(
        fields(true, true, WorkSourceId::Linear),
        json!({ "workMode": true, "workLinear": true, "workTracker": "linear" })
    );
}
