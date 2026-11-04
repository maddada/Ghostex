#[cfg(test)]
use serde_json::{json, Value};

#[cfg(test)]
use crate::ghostex_cli::args::parse_args;

#[cfg(test)]
use super::*;

// ---------------------------------------------------------------------------
// Page-side scripts. These are the VERBATIM sources of the Node CLI's
// ghostexSnapshotScript / ghostexClickScript / ghostexFillScript functions
// (which it serialized with fn.toString()); they run in the page's JS engine
// via Runtime.evaluate as `(<source>)(...<jsonArgs>)`.
// ---------------------------------------------------------------------------

pub(super) const GHOSTEX_SNAPSHOT_SCRIPT: &str = r##"function ghostexSnapshotScript(limit) {
  const selectors = [
    "a[href]",
    "button",
    "input",
    "textarea",
    "select",
    "[role='button']",
    "[role='link']",
    "[role='textbox']",
    "[contenteditable='true']",
    "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  const cssPath = (element) => {
    if (!element || element.nodeType !== 1) return "";
    const parts = [];
    let cursor = element;
    while (cursor && cursor.nodeType === 1 && cursor !== document.documentElement) {
      let part = cursor.nodeName.toLowerCase();
      if (cursor.id) {
        part += `#${CSS.escape(cursor.id)}`;
        parts.unshift(part);
        break;
      }
      const parent = cursor.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter((child) => child.nodeName === cursor.nodeName);
        if (siblings.length > 1) {
          part += `:nth-of-type(${siblings.indexOf(cursor) + 1})`;
        }
      }
      parts.unshift(part);
      cursor = parent;
    }
    return parts.join(" > ");
  };
  const labelFor = (element) => {
    const aria = element.getAttribute("aria-label");
    if (aria) return aria.trim();
    if (element.id) {
      const label = document.querySelector(`label[for="${CSS.escape(element.id)}"]`);
      if (label?.innerText) return label.innerText.trim();
    }
    return (element.innerText || element.value || element.placeholder || element.title || "").trim().replace(/\s+/g, " ");
  };
  const elements = [];
  for (const element of Array.from(document.querySelectorAll(selectors))) {
    const rect = element.getBoundingClientRect();
    const style = window.getComputedStyle(element);
    if (rect.width <= 0 || rect.height <= 0 || style.visibility === "hidden" || style.display === "none") {
      continue;
    }
    elements.push({
      bounds: { height: Math.round(rect.height), width: Math.round(rect.width), x: Math.round(rect.x), y: Math.round(rect.y) },
      disabled: Boolean(element.disabled || element.getAttribute("aria-disabled") === "true"),
      label: labelFor(element).slice(0, 240),
      placeholder: element.getAttribute("placeholder") || "",
      ref: `@e${elements.length + 1}`,
      role: element.getAttribute("role") || element.nodeName.toLowerCase(),
      selector: cssPath(element),
      tag: element.nodeName.toLowerCase(),
      type: element.getAttribute("type") || "",
      value: "value" in element ? String(element.value ?? "").slice(0, 240) : "",
    });
    if (elements.length >= limit) break;
  }
  return { elements, title: document.title, url: location.href };
}"##;

pub(super) const GHOSTEX_CLICK_SCRIPT: &str = r##"function ghostexClickScript(selector) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`Element not found: ${selector}`);
  element.scrollIntoView({ block: "center", inline: "center" });
  element.focus?.();
  element.click();
  const rect = element.getBoundingClientRect();
  return { bounds: { height: rect.height, width: rect.width, x: rect.x, y: rect.y }, selector };
}"##;

pub(super) const GHOSTEX_FILL_SCRIPT: &str = r##"function ghostexFillScript(selector, text) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`Element not found: ${selector}`);
  element.scrollIntoView({ block: "center", inline: "center" });
  element.focus?.();
  if (element.isContentEditable) {
    element.textContent = text;
  } else if (element.tagName === "SELECT") {
    element.value = text;
  } else if ("value" in element) {
    element.value = text;
  } else {
    throw new Error(`Element cannot be filled: ${selector}`);
  }
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
  return { selector, value: text };
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> McpState {
        McpState::new(McpServerOptions {
            port: None,
            target: None,
            timeout_ms: 10_000,
            scope: AgentScope::default(),
        })
    }

    #[test]
    fn key_event_maps_special_keys() {
        assert_eq!(
            key_event_for_browser_mcp("Enter"),
            json!({ "code": "Enter", "key": "Enter", "windowsVirtualKeyCode": 13 })
        );
        assert_eq!(
            key_event_for_browser_mcp("ArrowLeft"),
            json!({ "code": "ArrowLeft", "key": "ArrowLeft", "windowsVirtualKeyCode": 37 })
        );
        assert_eq!(
            key_event_for_browser_mcp("Escape"),
            json!({ "code": "Escape", "key": "Escape", "windowsVirtualKeyCode": 27 })
        );
    }

    #[test]
    fn key_event_maps_printable_and_multi_char_keys() {
        assert_eq!(
            key_event_for_browser_mcp("a"),
            json!({ "code": "KeyA", "key": "a", "text": "a", "windowsVirtualKeyCode": 65 })
        );
        assert_eq!(
            key_event_for_browser_mcp("Z"),
            json!({ "code": "KeyZ", "key": "Z", "text": "Z", "windowsVirtualKeyCode": 90 })
        );
        // Multi-character non-special keys keep the key as code, empty text, vk 0.
        assert_eq!(
            key_event_for_browser_mcp("F5"),
            json!({ "code": "F5", "key": "F5", "text": "", "windowsVirtualKeyCode": 0 })
        );
    }

    #[test]
    fn tools_list_shape_matches_node_cli() {
        let tools = browser_mcp_tools();
        let tools = tools.as_array().expect("tools array");
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool.get("name").and_then(Value::as_str).unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "ghostex_list_pages",
                "ghostex_select_page",
                "ghostex_navigate",
                "ghostex_evaluate",
                "ghostex_console_logs",
                "ghostex_snapshot",
                "ghostex_click",
                "ghostex_fill",
                "ghostex_press_key",
                "ghostex_screenshot",
            ]
        );
        for tool in tools {
            assert!(tool.get("description").and_then(Value::as_str).is_some());
            assert_eq!(
                tool.pointer("/inputSchema/type").and_then(Value::as_str),
                Some("object")
            );
        }
        // list_pages has no page-selector properties; every other tool does.
        assert_eq!(
            tools[0].pointer("/inputSchema/properties"),
            Some(&json!({}))
        );
        for tool in &tools[1..] {
            assert!(tool
                .pointer("/inputSchema/properties/pageId/description")
                .is_some());
        }
        assert_eq!(
            tools[2].pointer("/inputSchema/required"),
            Some(&json!(["url"]))
        );
        assert_eq!(
            tools[3].pointer("/inputSchema/required"),
            Some(&json!(["script"]))
        );
        assert_eq!(
            tools[7].pointer("/inputSchema/required"),
            Some(&json!(["text"]))
        );
        assert_eq!(
            tools[8].pointer("/inputSchema/required"),
            Some(&json!(["key"]))
        );
    }

    #[test]
    fn handle_initialize_and_tools_list() {
        let mut state = test_state();
        let response = handle_browser_mcp_message(
            &json!({ "id": 1, "jsonrpc": "2.0", "method": "initialize", "params": {} }),
            &mut state,
        )
        .expect("initialize response");
        assert_eq!(response.get("id"), Some(&json!(1)));
        assert_eq!(response.get("jsonrpc"), Some(&json!("2.0")));
        assert_eq!(
            response.pointer("/result/protocolVersion"),
            Some(&json!("2024-11-05"))
        );
        assert_eq!(
            response.pointer("/result/serverInfo/name"),
            Some(&json!("ghostex-browser-devtools"))
        );
        assert_eq!(
            response.pointer("/result/serverInfo/version"),
            Some(&json!("1.0.0"))
        );
        assert_eq!(
            response.pointer("/result/capabilities/tools"),
            Some(&json!({}))
        );

        let echoed = handle_browser_mcp_message(
            &json!({ "id": 2, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
            &mut state,
        )
        .expect("initialize response");
        assert_eq!(
            echoed.pointer("/result/protocolVersion"),
            Some(&json!("2025-03-26"))
        );

        let listed =
            handle_browser_mcp_message(&json!({ "id": 3, "method": "tools/list" }), &mut state)
                .expect("tools/list response");
        assert_eq!(listed.pointer("/result/tools"), Some(&browser_mcp_tools()));
    }

    #[test]
    fn handle_ignores_notifications_and_id_less_messages() {
        let mut state = test_state();
        assert!(handle_browser_mcp_message(
            &json!({ "method": "notifications/initialized" }),
            &mut state
        )
        .is_none());
        assert!(handle_browser_mcp_message(
            &json!({ "id": 9, "method": "notifications/initialized" }),
            &mut state
        )
        .is_none());
        assert!(
            handle_browser_mcp_message(&json!({ "method": "tools/list" }), &mut state).is_none()
        );
        assert!(handle_browser_mcp_message(
            &json!({ "id": null, "method": "tools/list" }),
            &mut state
        )
        .is_none());
        assert!(handle_browser_mcp_message(&json!("not an object"), &mut state).is_none());
    }

    #[test]
    fn handle_unknown_method_and_unknown_tool() {
        let mut state = test_state();
        let response =
            handle_browser_mcp_message(&json!({ "id": 4, "method": "resources/list" }), &mut state)
                .expect("error response");
        assert_eq!(response.pointer("/error/code"), Some(&json!(-32601)));
        assert_eq!(
            response.pointer("/error/message"),
            Some(&json!("Unknown MCP method: resources/list"))
        );

        let response = handle_browser_mcp_message(
            &json!({ "id": 5, "method": "tools/call", "params": { "name": "nope" } }),
            &mut state,
        )
        .expect("error response");
        assert_eq!(response.pointer("/error/code"), Some(&json!(-32000)));
        assert_eq!(
            response.pointer("/error/message"),
            Some(&json!("Unknown Ghostex browser MCP tool: nope"))
        );

        let response = handle_browser_mcp_message(
            &json!({ "id": 6, "method": "tools/call", "params": {} }),
            &mut state,
        )
        .expect("error response");
        assert_eq!(
            response.pointer("/error/message"),
            Some(&json!("Unknown Ghostex browser MCP tool: undefined"))
        );
    }

    #[test]
    fn normalize_browser_navigation_url_adds_https_when_missing_scheme() {
        assert_eq!(
            normalize_browser_navigation_url("https://x.test/a"),
            "https://x.test/a"
        );
        assert_eq!(
            normalize_browser_navigation_url("chrome-extension://abc"),
            "chrome-extension://abc"
        );
        assert_eq!(
            normalize_browser_navigation_url("about:blank"),
            "about:blank"
        );
        assert_eq!(
            normalize_browser_navigation_url("example.com/path"),
            "https://example.com/path"
        );
        assert_eq!(
            normalize_browser_navigation_url("127.0.0.1:8080"),
            "https://127.0.0.1:8080"
        );
    }

    #[test]
    fn string_flag_and_normalize_positive_integer_match_js() {
        assert_eq!(string_flag(Some(&json!("  x  "))), Some("x".to_string()));
        assert_eq!(string_flag(Some(&json!("   "))), None);
        assert_eq!(string_flag(Some(&json!(true))), Some("true".to_string()));
        assert_eq!(string_flag(Some(&json!(7))), Some("7".to_string()));
        assert_eq!(string_flag(Some(&Value::Null)), None);
        assert_eq!(string_flag(None), None);

        assert_eq!(
            normalize_positive_integer_opt(Some(json!(9333))),
            Some(9333)
        );
        assert_eq!(
            normalize_positive_integer_opt(Some(json!("9334"))),
            Some(9334)
        );
        assert_eq!(normalize_positive_integer_opt(Some(json!(1.5))), None);
        assert_eq!(normalize_positive_integer_opt(Some(json!(0))), None);
        assert_eq!(normalize_positive_integer_opt(Some(json!(-2))), None);
        assert_eq!(normalize_positive_integer_opt(Some(json!("abc"))), None);
        assert_eq!(normalize_positive_integer_opt(Some(Value::Null)), None);
        assert_eq!(normalize_positive_integer_opt(None), None);
    }

    #[test]
    fn normalize_remote_object_prefers_value_then_unserializable_then_description() {
        assert_eq!(
            normalize_remote_object(Some(&json!({ "value": 5, "description": "5" }))),
            json!(5)
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "value": null }))),
            Value::Null
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "unserializableValue": "Infinity" }))),
            json!("Infinity")
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "description": "Object", "type": "object" }))),
            json!("Object")
        );
        assert_eq!(
            normalize_remote_object(Some(&json!({ "type": "undefined" }))),
            json!("undefined")
        );
        assert_eq!(normalize_remote_object(Some(&Value::Null)), Value::Null);
        assert_eq!(normalize_remote_object(None), Value::Null);
        assert_eq!(
            remote_object_text(&json!({ "value": { "a": 1 } })),
            "{\"a\":1}"
        );
        assert_eq!(remote_object_text(&json!({ "value": "hi" })), "hi");
    }

    #[test]
    fn unique_numbers_dedupes_and_keeps_order() {
        assert_eq!(
            unique_numbers(&[None, Some(9334), Some(9333), Some(9334), Some(9335)]),
            vec![9334, 9333, 9335]
        );
    }

    #[test]
    fn parse_browser_open_defaults_match_js() {
        let parsed = parse_args(&[
            "--url".to_string(),
            "example.com".to_string(),
            "--new".to_string(),
            "--active-project".to_string(),
        ]);
        let payload = parse_browser_open(&parsed.rest, &parsed.flags);
        assert_eq!(payload.get("url"), Some(&json!("example.com")));
        assert_eq!(payload.get("reuse"), Some(&json!("none")));
        // --active-project boolean true suppresses the cwd projectPath default.
        assert_eq!(payload.get("projectPath"), None);

        let parsed = parse_args(&["http://x.test".to_string()]);
        let payload = parse_browser_open(&parsed.rest, &parsed.flags);
        assert_eq!(payload.get("url"), Some(&json!("http://x.test")));
        assert_eq!(payload.get("reuse"), Some(&json!("similar")));
        assert!(payload.get("projectPath").and_then(Value::as_str).is_some());
    }

    #[test]
    fn parse_ws_host_port_handles_devtools_urls() {
        assert_eq!(
            parse_ws_host_port("ws://127.0.0.1:9333/devtools/page/AB12"),
            Some(("127.0.0.1".to_string(), 9333))
        );
        assert_eq!(
            parse_ws_host_port("ws://localhost/devtools/page/AB12"),
            Some(("localhost".to_string(), 80))
        );
        assert_eq!(parse_ws_host_port("http://x.test/"), None);
    }
}
