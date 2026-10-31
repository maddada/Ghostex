use super::*;
use crate::session_chat::{SessionChatBlock, SessionChatInteractivePrompt, SessionChatRole};
use serde_json::{json, Value};

#[test]
fn large_opencode_image_is_shrunk_to_send_budget() {
    let image = image::RgbImage::from_fn(1800, 1800, |x, y| {
        let value = x
            .wrapping_mul(747_796_405)
            .wrapping_add(y.wrapping_mul(2_891_336_453));
        image::Rgb([value as u8, (value >> 8) as u8, (value >> 16) as u8])
    });
    let mut original = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut original),
            image::ImageFormat::Png,
        )
        .unwrap();
    assert!(original.len() > OPENCODE_IMAGE_TARGET_BYTES);
    let (prepared, encoding) = opencode_image_bytes(original).unwrap();
    assert!(prepared.len() <= OPENCODE_IMAGE_TARGET_BYTES);
    assert!(matches!(encoding, "png" | "jpg"));
    let decoded = image::load_from_memory(&prepared).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1800, 1800));
}

fn form(fields: Value) -> Snapshot {
    Snapshot {
        info: json!({}),
        messages: vec![],
        permissions: vec![],
        working: false,
        forms: vec![json!({"id":"frm_test","title":"Choose","fields":fields})],
    }
}

#[test]
fn rejects_session_ids_that_escape_api_or_mirror_directory() {
    for id in [
        "",
        "../credentials",
        "ses_a/b",
        "ses_a?x",
        "ses_a%2f",
        "ses_a\\b",
        "ses_a\n",
    ] {
        assert!(!safe_id(id), "{id}");
    }
    assert!(safe_id("ses_f29725ec3ffeKdTeyuiZsbly2t"));
}

#[test]
fn pasted_image_references_survive_windows_paths_and_queue_storage() {
    let draft = r"Compare [Image #1](C:\test folder\shot (2).png) and [Image #2](/tmp/second.png). [Image #3](C:\test folder\shot (2).png) [File #4](/tmp/readme.md)";
    assert_eq!(
        super::attachments::image_references(draft),
        vec![r"C:\test folder\shot (2).png", "/tmp/second.png"]
    );
    assert!(super::attachments::image_references(
        "[Image #no](ignored.png) [Image #1](broken\n.png)"
    )
    .is_empty());
}

#[test]
fn mutable_text_reasoning_and_tools_keep_stable_identity() {
    let mut messages = vec![
        json!({"id":"msg_user","type":"user","text":"Run test","time":{"created":12}}),
        json!({"id":"msg_reply","type":"assistant","time":{"created":14},"content":[
            {"type":"reasoning","text":"Thinking"},{"type":"text","text":"Starting"},
            {"type":"tool","id":"call_1","name":"bash","state":{"status":"streaming","input":"{\"command\":\"echo ok\"}"}}
        ]}),
    ];
    let first = decode_messages(&messages);
    assert_eq!(first.len(), 4);
    assert_eq!(first[1].role, SessionChatRole::Reasoning);
    assert_eq!(first[3].turn_id.as_deref(), Some("msg_user"));
    messages[1]["content"][1]["text"] = json!("Starting now");
    messages[1]["content"][2]["state"] = json!({"status":"completed","input":{"command":"echo ok"},"content":[{"type":"text","text":"ok"}]});
    let second = decode_messages(&messages);
    assert_eq!(first[2].id, second[2].id);
    assert_eq!(first[3].id, second[3].id);
    assert_eq!(second[4].role, SessionChatRole::Tool);
    assert!(
        matches!(&second[4].blocks[0],SessionChatBlock::ToolResult{output,is_error:Some(false), call_id: None, patch: None } if output=="ok")
    );
    for row in second {
        assert_eq!(
            decode_line(&serde_json::to_string(&row).unwrap(), "").unwrap(),
            row
        );
    }
}

#[test]
fn errors_compaction_and_shell_output_remain_visible() {
    let rows = decode_messages(&[
        json!({"id":"msg_error","type":"assistant","content":[],"error":{"type":"provider","message":"Rate limit"}}),
        json!({"id":"msg_compact","type":"compaction","status":"completed"}),
        json!({"id":"msg_shell","type":"shell","command":"echo hello","output":{"output":"hello"},"exit":0}),
    ]);
    assert_eq!(rows.len(), 4);
    assert!(matches!(&rows[0].blocks[0],SessionChatBlock::Text{text} if text=="Rate limit"));
    assert!(
        matches!(&rows[3].blocks[0],SessionChatBlock::ToolResult{output,..} if output=="hello")
    );
}

#[test]
fn images_preserve_mime_and_title() {
    let rows = decode_messages(&[
        json!({"id":"msg_img","type":"user","text":"See image","files":[{"mime":"image/png","data":"eA==","name":"sample.png"}]}),
    ]);
    assert!(
        matches!(&rows[0].blocks[1], SessionChatBlock::ImageRef{url:Some(url),alt:Some(alt),..} if url=="data:image/png;base64,eA==" && alt=="sample.png")
    );
    let linked = decode_messages(&[
        json!({"id":"msg_linked","type":"user","text":"See [Image #1](C:\\pictures\\sample.png)","files":[{"mime":"image/png","data":"eA==","name":"sample.png"}]}),
    ]);
    assert_eq!(
        linked[0].blocks.len(),
        1,
        "the numbered picture already renders inline"
    );
}

#[test]
fn permissions_take_precedence_and_preserve_exact_request_id() {
    let mut input = form(json!([{"key":"x","type":"string"}]));
    input
        .permissions
        .push(json!({"id":"per_exact","action":"edit","resources":["one.rs","two.rs"]}));
    match interactive_prompt(&input).unwrap() {
        SessionChatInteractivePrompt::Approval {
            tool,
            summary,
            tool_use_id,
        } => {
            assert_eq!(tool, "edit");
            assert_eq!(summary.as_deref(), Some("one.rs\ntwo.rs"));
            assert_eq!(tool_use_id.as_deref(), Some("per_exact"));
        }
        _ => panic!("expected permission"),
    }
}

#[test]
fn questions_offer_boolean_multiselect_and_free_text() {
    let input = form(
        json!([{"type":"boolean","key":"accept"},{"type":"multiselect","key":"features","custom":true,"options":[{"value":"machine-value","label":"Readable"}]},{"type":"string","key":"note"}]),
    );
    let SessionChatInteractivePrompt::Question {
        questions,
        tool_use_id,
    } = interactive_prompt(&input).unwrap()
    else {
        panic!()
    };
    assert_eq!(tool_use_id.as_deref(), Some("frm_test"));
    assert_eq!(questions[0].options[1].label, "No");
    assert!(questions[1].multi_select);
    assert_eq!(questions[1].options[0].label, "Readable");
    assert_eq!(questions[2].allow_custom, Some(true));
}

#[test]
fn external_and_conditional_forms_are_not_misrepresented() {
    assert!(interactive_prompt(&form(
        json!([{"type":"external","key":"auth","url":"https://example.com"}])
    ))
    .is_none());
    assert!(interactive_prompt(&form(
        json!([{"type":"string","key":"answer","when":[{"key":"earlier","op":"eq","value":"yes"}]}])
    ))
    .is_none());
}

#[test]
fn model_config_edits_preserve_comments_and_other_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opencode.jsonc");
    std::fs::write(
        &path,
        "{\n // keep this note\n \"model\": \"old/model\",\n \"shell\": \"pwsh\",\n}\n",
    )
    .unwrap();
    patch_config(
        &path,
        "model",
        &json!({"providerID":"opencode","model":"test","variant":"high"}),
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("// keep this note"));
    let value = read_config(&text).unwrap();
    assert_eq!(value["shell"], "pwsh");
    assert_eq!(value["model"]["variant"], "high");
    std::fs::write(&path, "[]").unwrap();
    assert!(patch_config(&path, "model", &json!("bad")).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[]");
}

#[test]
fn composer_model_follows_the_conversation_not_a_default() {
    let catalog = json!({"agents":{"opencode":{"models":[
        {"value":"opencode/big-pickle","label":"Big Pickle"},
        {"value":"deepseek/deepseek-flash","label":"DeepSeek Flash"}
    ]}}});
    let reply = |provider: &str, id: &str| json!({"type":"assistant","model":{"providerID":provider,"id":id}});
    let messages = vec![
        reply("opencode", "big-pickle"),
        json!({"type":"user"}),
        reply("deepseek", "deepseek-flash"),
        json!({"type":"idle"}),
    ];
    // No selection on the session (OpenCode leaves it unset, or null): the latest reply's model.
    for info in [json!({}), json!({"model":null})] {
        let model = mirror::detected_model(&info, &messages, Some(&catalog)).unwrap();
        assert_eq!(
            (model.value.as_str(), model.label.as_str()),
            ("deepseek/deepseek-flash", "DeepSeek Flash")
        );
    }
    // A switch made in the TUI or the picker is the session's selection and wins over older replies.
    let info = json!({"model":{"providerID":"openrouter","id":"deepseek/deepseek-v4.1-flash-1017","variant":"high"}});
    let model = mirror::detected_model(&info, &messages, Some(&catalog)).unwrap();
    assert_eq!(model.value, "openrouter/deepseek/deepseek-v4.1-flash-1017");
    // A model the catalog does not list keeps its own id, never another model's name.
    assert_eq!(model.label, "deepseek/deepseek-v4.1-flash-1017");
    assert_eq!(model.variant.as_deref(), Some("high"));
    // A new conversation with no selection and no reply has no model yet.
    assert!(mirror::detected_model(&json!({}), &[], Some(&catalog)).is_none());
}

#[test]
fn token_usage_and_tasks_follow_latest_assistant_state() {
    let mut snapshot = form(json!([]));
    snapshot.messages = vec![
        json!({"type":"assistant","tokens":{"input":10,"output":3,"reasoning":2,"cache":{"read":20,"write":5}},"content":[{"type":"tool","name":"todowrite","state":{"status":"completed","input":{"todos":[{"content":"Verify","status":"in_progress"}]}}}]}),
    ];
    assert_eq!(usage(&snapshot).unwrap().used_tokens, Some(40));
    assert_eq!(tasks(&snapshot).unwrap().tasks[0].subject, "Verify");
    snapshot
        .messages
        .push(json!({"type":"compaction","status":"completed"}));
    assert!(usage(&snapshot).is_none());
}

/// Exercises the installed OpenCode's public API; creates and removes only its own session.
#[test]
#[ignore = "requires the local OpenCode service"]
fn live_service_questions_permissions_and_controls() {
    let client = Client::discover().expect("OpenCode service");
    let directory = tempfile::tempdir().unwrap();
    let created=client.request("POST","/api/session",Some(json!({"title":"Ghostex adapter integration test","location":{"directory":directory.path()},"permissions":[{"action":"*","resource":"*","effect":"ask"}]}))).unwrap();
    let id = created["data"]["id"].as_str().unwrap().to_string();
    struct Cleanup<'a>(&'a Client, String);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self.0.session(&self.1, "", "DELETE", None);
        }
    }
    let _cleanup = Cleanup(&client, id.clone());
    assert!(client.messages(&id).unwrap().is_empty());
    let fields = json!([
        {"type":"string","key":"choice","required":true,"options":[{"value":"stored-value","label":"Friendly label"}]},
        {"type":"multiselect","key":"features","custom":true,"options":[{"value":"first-value","label":"First"}]},
        {"type":"boolean","key":"enabled"}, {"type":"integer","key":"count"}
    ]);
    let created = client
        .session(
            &id,
            "/form",
            "POST",
            Some(json!({"title":"Integration question","fields":fields})),
        )
        .unwrap();
    let form_id = created["data"]["id"].as_str().unwrap();
    let args = json!({"kind":"question","toolUseId":form_id,"selections":[{"indices":[0]},{"indices":[0],"other":"custom value"},{"indices":[1]},{"indices":[],"other":"3"}]});
    answer(&id, args.as_object().unwrap()).unwrap();
    let settled = client
        .session(&id, &format!("/form/{form_id}"), "GET", None)
        .unwrap();
    assert!(settled.to_string().contains("stored-value"), "{settled}");
    assert!(client.session(&id, "/form", "GET", None).unwrap()["data"]
        .as_array()
        .unwrap()
        .is_empty());
    let permission = client
        .session(
            &id,
            "/permission",
            "POST",
            Some(json!({"action":"edit","resources":["integration-only.txt"]})),
        )
        .unwrap();
    assert_eq!(permission["data"]["effect"], "ask");
    let permission_id = permission["data"]["id"].as_str().unwrap();
    assert!(answer(
        &id,
        json!({"kind":"approval","toolUseId":"per_stale","approvalSend":"1"})
            .as_object()
            .unwrap()
    )
    .is_err());
    answer(
        &id,
        json!({"kind":"approval","toolUseId":permission_id,"approvalSend":""})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert!(
        client.session(&id, "/permission", "GET", None).unwrap()["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let permission = client
        .session(
            &id,
            "/permission",
            "POST",
            Some(json!({"action":"edit","resources":["allow-once.txt"]})),
        )
        .unwrap();
    let permission_id = permission["data"]["id"].as_str().unwrap();
    answer(
        &id,
        json!({"kind":"approval","toolUseId":permission_id,"approvalSend":"1"})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    client.session(&id, "/interrupt", "POST", None).unwrap();
    let catalog = model_catalog(
        &client,
        &client.session(&id, "", "GET", None).unwrap()["data"],
    )
    .unwrap();
    assert!(catalog["agents"]["opencode"]["models"].is_array());
    assert!(catalog["agents"]["opencode"]["efforts"].is_array());
    let form = client
        .session(
            &id,
            "/form",
            "POST",
            Some(json!({"title":"Cancel test","fields":[{"type":"boolean","key":"cancel"}]})),
        )
        .unwrap();
    interrupt(&id, Some(form["data"]["id"].as_str().unwrap())).unwrap();
    assert!(client.session(&id, "/form", "GET", None).unwrap()["data"]
        .as_array()
        .unwrap()
        .is_empty());
    if let Some(model) = catalog["agents"]["opencode"]["models"]
        .as_array()
        .unwrap()
        .first()
    {
        select(
            &id,
            json!({"model":model["value"],"scope":"session","options":{"mode":"plan"}})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        let info = client.session(&id, "", "GET", None).unwrap();
        assert_eq!(info["data"]["agent"], "plan");
        assert!(info["data"]["model"].is_object());
    }
    let message_id = format!("msg_gx_{}", uuid::Uuid::new_v4().simple());
    let body = json!({"id":message_id,"text":"Image admission test","resume":false,"files":[{"uri":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAALSURBVBhXY2BABwAAEgABp3qZbgAAAABJRU5ErkJggg==","name":"pixel.png"}]});
    let first = client
        .session(&id, "/prompt", "POST", Some(body.clone()))
        .unwrap();
    let retry = client.session(&id, "/prompt", "POST", Some(body)).unwrap();
    assert_eq!(first["data"]["id"], retry["data"]["id"]);
    assert!(first.to_string().contains("image/png"));
}

#[test]
#[ignore = "requires the local OpenCode service"]
fn live_service_pagination_rewind_children_and_shell() {
    let client = Client::discover().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let create = |parent: Option<&str>| {
        client.request("POST","/api/session",Some(json!({"title":"Ghostex transcript integration test","location":{"directory":directory.path()},"parentID":parent}))).unwrap()["data"]["id"].as_str().unwrap().to_string()
    };
    let template = create(None);
    struct Cleanup<'a>(&'a Client, Vec<String>);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            for id in self.1.iter().rev() {
                let _ = self.0.session(id, "", "DELETE", None);
            }
        }
    }
    let mut cleanup = Cleanup(&client, vec![template.clone()]);
    let root = format!("ses_gx_{}", uuid::Uuid::new_v4().simple());
    let prefix = format!("msg_gx_{}", uuid::Uuid::new_v4().simple());
    let mut info = client.session(&template, "", "GET", None).unwrap()["data"].clone();
    info["id"] = json!(root);
    info["time"]["idle"] = json!(chrono::Utc::now().timestamp_millis());
    let messages:Vec<Value>=(0..205).map(|i|json!({"id":format!("{prefix}_{i}"),"type":"user","time":{"created":i+1},"text":format!("Pagination row {i}"),"files":[],"agents":[],"skills":[]})).collect();
    client
        .request(
            "POST",
            "/api/experimental/session/import",
            Some(json!({"info":info,"messages":messages})),
        )
        .unwrap();
    cleanup.1.push(root.clone());
    let rows = client.messages(&root).unwrap();
    assert_eq!(rows.len(), 205);
    assert_eq!(rows[204]["text"], "Pagination row 204");
    let child_id = format!("ses_gx_{}", uuid::Uuid::new_v4().simple());
    let mut child_info = client.session(&root, "", "GET", None).unwrap()["data"].clone();
    child_info["id"] = json!(child_id);
    child_info["parentID"] = json!(root);
    client
        .request(
            "POST",
            "/api/experimental/session/import",
            Some(json!({"info":child_info,"messages":[]})),
        )
        .unwrap();
    cleanup.1.push(child_id.clone());
    assert_eq!(child(&root, &child_id).unwrap()["id"], child_id);
    assert!(child("ses_unrelated", &child_id).is_err());
    assert_eq!(fleet(&root).unwrap().unwrap().agents.len(), 1);
    rewind(&root, &format!("{prefix}_203")).unwrap();
    assert_eq!(client.messages(&root).unwrap().len(), 203);
    client
        .send(
            &child_id,
            "!echo OPENCODE_SHELL_TEST",
            &[],
            Some(&format!("{prefix}_shell")),
        )
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        let messages = client.messages(&child_id).unwrap();
        if messages
            .iter()
            .any(|row| row["type"] == "shell" && row["status"] != "running")
        {
            assert!(decode_messages(&messages).iter().any(|row|row.blocks.iter().any(|block|matches!(block,SessionChatBlock::ToolResult{output,..} if output.contains("OPENCODE_SHELL_TEST")))));
            break;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(15));
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let command = if cfg!(windows) {
        "!powershell -NoProfile -Command Start-Sleep -Seconds 20"
    } else {
        "!sleep 20"
    };
    let start = std::time::Instant::now();
    client
        .send(
            &child_id,
            command,
            &[],
            Some(&format!("{prefix}_long_shell")),
        )
        .unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "chat must acknowledge a running shell before it finishes"
    );
    interrupt(&child_id, None).unwrap();
    let start = std::time::Instant::now();
    loop {
        if !client
            .messages(&child_id)
            .unwrap()
            .iter()
            .any(|m| m["type"] == "shell" && m["status"] == "running")
        {
            break;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
