//! Labels read off real tool calls from each agent's transcript (Codex 0.163 code mode, Claude
//! Code, Copilot, Grok Build, Hermes, OMP, Antigravity, and the OpenCode-style names ZCode and
//! Pi share), all recorded on 2026-10-24 running the same small task.

use serde_json::{json, Value};

use super::tool_label;

const WD: &str = "/Users/madda/.local/share/agent-work/ghostex/chat-parity/sb";

fn label(name: &str, input: Value) -> String {
    tool_label(name, &input, Some(WD), true)
        .map(|label| label.text)
        .unwrap_or_default()
}

fn glyph(name: &str, input: Value) -> Option<&'static str> {
    tool_label(name, &input, Some(WD), true).and_then(|label| label.glyph)
}

#[test]
fn codex_exec_reads_a_file() {
    let input = json!("const r = await tools.exec_command({cmd:\"cat ~/.agents/main.md\",workdir:\"/Users/madda/.local/share/agent-work/ghostex/chat-parity/sb\",max_output_tokens:20000}); text(r.output);\n");
    assert_eq!(label("exec", input.clone()), "Read ~/.agents/main.md");
    assert_eq!(glyph("exec", input), Some("file"));
}

#[test]
fn codex_exec_maps_over_a_command_list() {
    let input = json!("const cmds=[\"cat README.md\",\"cat src/math.js\",\"rg -n 'TODO' . -g '!node_modules/**' -g '!.git/**'\"]; const rs=await Promise.allSettled(cmds.map(cmd=>tools.exec_command({cmd,workdir:\"/x\",max_output_tokens:10000}))); rs.forEach((r,i)=>text(JSON.stringify({cmd:cmds[i],result:r.status===\"fulfilled\"?r.value:r.reason})));");
    assert_eq!(
        label("exec", input),
        "Read README.md and src/math.js, searched for TODO"
    );
}

#[test]
fn codex_exec_runs_a_test() {
    let input = json!("const r=await tools.exec_command({cmd:\"node test.js\",workdir:\"/x\",max_output_tokens:10000}); text(r);\n");
    assert_eq!(label("exec", input), "Ran node test.js");
}

#[test]
fn codex_exec_applies_a_patch() {
    let input = json!("text(await tools.apply_patch(\"*** Begin Patch\\n*** Update File: /Users/madda/.local/share/agent-work/ghostex/chat-parity/sb/src/math.js\\n@@\\n-  return 1;\\n+  return 0;\\n }\\n*** End Patch\"));\n");
    assert_eq!(label("exec", input.clone()), "Edited src/math.js");
    assert_eq!(glyph("exec", input), Some("edit"));
    // The same folder written in another case still reads relative to the session's folder.
    assert_eq!(
        tool_label(
            "apply_patch",
            &json!("*** Begin Patch\n*** Update File: /Users/madda/.local/share/agent-work/ghostex/chat-parity/sb/src/math.js\n*** End Patch"),
            Some("/Users/madda/.local/share/agent-work/Ghostex/chat-parity/sb"),
            true,
        )
        .map(|label| label.text),
        Some("Edited src/math.js".to_string())
    );
}

#[test]
fn codex_exec_opens_a_page_and_views_an_image() {
    let open = json!("const r=await tools.web__run({open:[{ref_id:\"https://ghostex.dev\"}],response_length:\"medium\"}); text(r);\n");
    assert_eq!(label("exec", open.clone()), "Opened ghostex.dev");
    assert_eq!(glyph("exec", open), Some("web"));
    let search = json!(
        "const r=await tools.web__run({search_query:[{q:\"cua driver browser\"}]}); text(r);"
    );
    assert_eq!(
        label("exec", search),
        "Searched the web for cua driver browser"
    );
    let image = json!("const r=await tools.view_image({path:\"/Users/madda/Movies/GhostX demo thumbnails/work/contact.jpg\",detail:\"original\"}); image(r.image_url);\n");
    assert_eq!(
        label("exec", image),
        "Viewed ~/Movies/GhostX demo thumbnails/work/contact.jpg"
    );
    let wait = json!("const r=await tools.write_stdin({session_id:39644,chars:\"\",yield_time_ms:1000,max_output_tokens:1000}); text(r);\n");
    assert_eq!(label("exec", wait), "Waited for a running command");
    let deep = json!("const r=await tools.view_image({path:\"/Users/madda/Movies/GhostX demo thumbnails/work/frames/upscaled/0420-full-resolution.png\"}); image(r.image_url);");
    assert_eq!(
        label("exec", deep),
        "Viewed …/upscaled/0420-full-resolution.png"
    );
}

#[test]
fn codex_exec_batches_several_commands() {
    let input = json!("const rs=await Promise.allSettled([\n  tools.exec_command({cmd:\"python3 - <<'PY'\\nimport PIL\\nprint(PIL.__version__)\\nPY\",max_output_tokens:1000}),\n  tools.exec_command({cmd:\"curl -L -s https://ghostex.dev | rg -o '<link[^>]+>' | head -50\",max_output_tokens:4000}),\n  tools.exec_command({cmd:\"fc-list | rg -i 'Gelion|Inter' | head -50\",max_output_tokens:4000})\n]); rs.forEach((r,i)=>text(JSON.stringify({i,result:r})));\n");
    assert_eq!(
        label("exec", input),
        "Ran a script in Python, fetched ghostex.dev, ran fc-list"
    );
}

#[test]
fn codex_shell_function_call_unwraps_bash() {
    assert_eq!(
        label(
            "shell",
            json!({"command": ["bash", "-lc", "rg -n foo src"]})
        ),
        "Searched for foo in src"
    );
    assert_eq!(
        label("exec_command", json!({"cmd": "sed -n '1,80p' a.rs"})),
        "Read a.rs"
    );
    assert_eq!(
        label(
            "exec_command",
            json!({"cmd": "cargo test -p ghostex-gx-chat-core"})
        ),
        "Ran cargo test -p ghostex-gx-chat-core"
    );
}

#[test]
fn claude_bash_without_a_description() {
    assert_eq!(
        label(
            "Bash",
            json!({"command": "cat README.md && echo ---- && cat -n src/math.js && echo ---- && rg -n TODO; echo ---- && node test.js"})
        ),
        "Read README.md and src/math.js, searched for TODO, ran node test.js"
    );
    assert_eq!(
        label(
            "Bash",
            json!({"command": "cd server && git status --short | head -20"})
        ),
        "Ran git status --short"
    );
}

#[test]
fn claude_named_tools() {
    assert_eq!(
        label("Read", json!({"file_path": format!("{WD}/src/math.js")})),
        "Read src/math.js"
    );
    assert_eq!(
        label(
            "Grep",
            json!({"pattern": "TODO", "path": format!("{WD}/src")})
        ),
        "Searched for TODO in src"
    );
    assert_eq!(
        label("Glob", json!({"pattern": "**/*.rs"})),
        "Found files matching **/*.rs"
    );
    assert_eq!(
        label(
            "WebFetch",
            json!({"url": "https://docs.rs/serde/latest/serde/", "prompt": "x"})
        ),
        "Fetched docs.rs/serde/latest/serde"
    );
    assert_eq!(
        label("WebSearch", json!({"query": "gpui accessibility"})),
        "Searched the web for gpui accessibility"
    );
    assert_eq!(label("TodoWrite", json!({"todos": []})), "Updated the plan");
    assert_eq!(
        label("mcp__spaceo__spaceo_click", json!({"element": "w20"})),
        "Used spaceo click"
    );
}

#[test]
fn copilot_tools() {
    assert_eq!(
        label(
            "Grep",
            json!({"pattern": "TODO", "paths": [WD], "output_mode": "content"})
        ),
        "Searched for TODO"
    );
    assert_eq!(
        label(
            "apply_patch",
            json!("*** Begin Patch\n*** Update File: /Users/madda/.local/share/agent-work/ghostex/chat-parity/sb/src/math.js\n@@\n-a\n+b\n*** End Patch")
        ),
        "Edited src/math.js"
    );
}

#[test]
fn grok_and_antigravity_file_spellings() {
    assert_eq!(
        label("Read", json!({"target_file": format!("{WD}/README.md")})),
        "Read README.md"
    );
    assert_eq!(
        label("read_file", json!({"Path": format!("{WD}/src/math.js")})),
        "Read src/math.js"
    );
    assert_eq!(
        label(
            "edit_file",
            json!({"Path": format!("{WD}/src/math.js"), "Replacement": "x"})
        ),
        "Edited src/math.js"
    );
}

#[test]
fn hermes_tools() {
    assert_eq!(
        label(
            "read_file",
            json!({"path": "README.md", "offset": 1, "limit": 2000})
        ),
        "Read README.md"
    );
    assert_eq!(
        label(
            "terminal",
            json!({"command": "rg -n \"TODO\" . -g '!node_modules/**'", "workdir": WD})
        ),
        "Searched for TODO"
    );
    assert_eq!(
        label(
            "patch",
            json!({"mode": "patch", "patch": "*** Begin Patch\n*** Update File: src/math.js\n@@\n*** End Patch"})
        ),
        "Edited src/math.js"
    );
    assert_eq!(
        label("skill_view", json!({"name": "shell-command-execution"})),
        "Used the skill shell-command-execution"
    );
}

#[test]
fn omp_edit_header() {
    assert_eq!(
        label(
            "edit",
            json!({"input": "[src/math.js#55AD]\nPUT 9.=11:\n+x"})
        ),
        "Edited src/math.js"
    );
}

#[test]
fn opencode_style_tools() {
    assert_eq!(
        label("read", json!({"filePath": format!("{WD}/src/math.js")})),
        "Read src/math.js"
    );
    assert_eq!(
        label("list", json!({"path": format!("{WD}/src")})),
        "Listed src"
    );
    assert_eq!(
        label("grep", json!({"pattern": "average", "include": "*.js"})),
        "Searched for average in *.js"
    );
    assert_eq!(
        label(
            "webfetch",
            json!({"url": "https://ghostex.dev/docs?x=1", "format": "markdown"})
        ),
        "Fetched ghostex.dev/docs"
    );
}

#[test]
fn unreadable_calls_keep_their_name() {
    assert!(tool_label("SomeTool", &json!({"x": 1}), Some(WD), true).is_none());
    assert!(tool_label("exec", &json!(""), Some(WD), true).is_none());
}

#[test]
fn described_rows_keep_the_agents_words() {
    use crate::transcript::tool_summary::tool_description;
    assert_eq!(
        tool_description(&json!({"command": "node test.js", "description": "Re-run tests"})),
        "Re-run tests"
    );
    assert_eq!(
        tool_description(&json!({"path": "README.md", "i": "Reading project README"})),
        "Reading project README"
    );
}

#[test]
fn a_call_that_has_not_finished_well_does_not_read_as_done() {
    // The Codex call recorded on 2026-10-27: a `touch` blocked in Read Only, waiting for approval
    // and then declined ("aborted by user after 27.9s"). No file was made.
    let input = json!("const r = await tools.exec_command({cmd:\"touch proof-web-readonly.txt\",workdir:\"/Users/madda/.local/share/agent-work/ghostex/chat-parity/sb\",sandbox_permissions:\"require_escalated\",justification:\"Do you want to allow creating proof-web-readonly.txt in this read-only workspace?\"}); text(r.output);\n");
    let text = |finished| {
        tool_label("exec", &input, Some(WD), finished)
            .map(|label| label.text)
            .unwrap_or_default()
    };
    assert_eq!(text(false), "Create proof-web-readonly.txt");
    assert_eq!(text(true), "Created proof-web-readonly.txt");
    // The glyph says what kind of call it is either way.
    assert_eq!(
        tool_label("exec", &input, Some(WD), false).and_then(|label| label.glyph),
        Some("edit")
    );
    // Every verb of a joined label changes, not just the first.
    let command = json!({"command": "npm test && rm -f build.log"});
    assert_eq!(
        tool_label("Bash", &command, Some(WD), true).map(|label| label.text),
        Some("Ran npm test, deleted build.log".to_string())
    );
    assert_eq!(
        tool_label("Bash", &command, Some(WD), false).map(|label| label.text),
        Some("Run npm test, delete build.log".to_string())
    );
}

#[test]
fn transcript_rows_name_a_call_by_whether_it_finished_well() {
    use crate::transcript::tool_fold::ToolPair;
    use crate::transcript::transcript_rows::tool_rows;
    use ghostex_gx_protocol::ChatBlock;
    let call: ChatBlock = serde_json::from_value(json!({
        "type": "tool-call",
        "name": "Bash",
        "input": {"command": "touch proof.txt"},
        "callId": "c1",
    }))
    .unwrap();
    // gxserver marks a Codex call the user declined ("aborted by user after 27.9s") as failed.
    let declined: ChatBlock = serde_json::from_value(
        json!({"type": "tool-result", "output": "aborted by user after 27.9s", "isError": true, "callId": "c1"}),
    )
    .unwrap();
    let ran: ChatBlock =
        serde_json::from_value(json!({"type": "tool-result", "output": "", "callId": "c1"}))
            .unwrap();
    let label = |result: Option<&ChatBlock>| {
        tool_rows(
            &[ToolPair {
                call: Some(&call),
                result,
            }],
            "",
            Some(WD),
            &std::collections::HashMap::new(),
        )[0]["label"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(label(None), "Create proof.txt");
    assert_eq!(label(Some(&declined)), "Create proof.txt");
    assert_eq!(label(Some(&ran)), "Created proof.txt");
}
