//! How a [`FoldedSnapshot`] is written: exactly the object the derived, flattened form wrote, with
//! the shared rows in the `messages` the result itself keeps empty.
//!
//! CDXC:SessionChat 2026-11-10 WHY: the fold holds its rows as shared `ChatRow`s (one copy with the authoritative list) rather than inside the wire type's `messages`, so writing it cannot be derived. `messages` is the result's first field, so it is written first from `rows`; every other entry of the result is passed through as the derived code writes it (`EntriesInto` forwards them and drops the empty `messages`), then the frame keys, which gives the record the same bytes as before (memharness replay: every saved record identical).

use serde::ser::{Error, Impossible, SerializeMap, SerializeStruct, Serializer};
use serde::Serialize;

use crate::session::fold::FoldedSnapshot;

impl Serialize for FoldedSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // CDXC:SessionChat 2026-11-11 WHY: every field is named (no `..`) so a field added to
        // `FoldedSnapshot` fails the build here: write it below in the place the derived,
        // flattened form wrote it, and read it in `fold.rs`'s `Deserialize`. The result's own
        // fields need nothing here (`EntriesInto` forwards each one); the test below compares
        // this writer with the derived one for a value with every field set.
        let FoldedSnapshot {
            result,
            rows,
            extra,
        } = self;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(ROWS_KEY, rows)?;
        result.serialize(EntriesInto {
            map: &mut map,
            skipping: false,
        })?;
        for (key, value) in extra {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// The key the result writes its own (empty) rows under.
const ROWS_KEY: &str = "messages";

/// A serializer that writes the entries of the map or struct it is given into `map`, leaving out
/// [`ROWS_KEY`].
struct EntriesInto<'a, M> {
    map: &'a mut M,
    /// The key just seen was [`ROWS_KEY`], so its value is dropped too.
    skipping: bool,
}

/// Whether `key` is [`ROWS_KEY`]. Keys here are the derived code's field names, plain strings.
fn is_rows_key<K: Serialize + ?Sized>(key: &K) -> bool {
    matches!(serde_json::to_value(key), Ok(serde_json::Value::String(name)) if name == ROWS_KEY)
}

impl<M: SerializeMap> SerializeMap for EntriesInto<'_, M> {
    type Ok = ();
    type Error = M::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), M::Error> {
        self.skipping = is_rows_key(key);
        if self.skipping {
            return Ok(());
        }
        self.map.serialize_key(key)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), M::Error> {
        if std::mem::take(&mut self.skipping) {
            return Ok(());
        }
        self.map.serialize_value(value)
    }

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), M::Error> {
        if is_rows_key(key) {
            return Ok(());
        }
        self.map.serialize_entry(key, value)
    }

    fn end(self) -> Result<(), M::Error> {
        Ok(())
    }
}

impl<M: SerializeMap> SerializeStruct for EntriesInto<'_, M> {
    type Ok = ();
    type Error = M::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), M::Error> {
        if key == ROWS_KEY {
            return Ok(());
        }
        self.map.serialize_entry(key, value)
    }

    fn end(self) -> Result<(), M::Error> {
        Ok(())
    }
}

/// Anything but a map or a struct is not a result.
fn not_an_object<E: Error>() -> E {
    E::custom("a folded snapshot's result writes as an object")
}

impl<'a, M: SerializeMap> Serializer for EntriesInto<'a, M> {
    type Ok = ();
    type Error = M::Error;
    type SerializeSeq = Impossible<(), M::Error>;
    type SerializeTuple = Impossible<(), M::Error>;
    type SerializeTupleStruct = Impossible<(), M::Error>;
    type SerializeTupleVariant = Impossible<(), M::Error>;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<(), M::Error>;

    fn serialize_map(self, _len: Option<usize>) -> Result<Self, M::Error> {
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self, M::Error> {
        Ok(self)
    }

    fn serialize_bool(self, _: bool) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i8(self, _: i8) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i16(self, _: i16) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i32(self, _: i32) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i64(self, _: i64) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u8(self, _: u8) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u16(self, _: u16) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u32(self, _: u32) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u64(self, _: u64) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_f32(self, _: f32) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_f64(self, _: f64) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_char(self, _: char) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_str(self, _: &str) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_none(self) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_unit(self) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, M::Error> {
        Err(not_an_object())
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, M::Error> {
        Err(not_an_object())
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, M::Error> {
        Err(not_an_object())
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, M::Error> {
        Err(not_an_object())
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, M::Error> {
        Err(not_an_object())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use ghostex_gx_protocol::{
        ChatForkInfo, ChatLookSettings, ChatMessage, ChatQuietViewSettings, ChatSideState,
        ChatStatus, ChatViewField, ReadSessionChatResult, Tri, TurnLifecycle, TurnLifecycleState,
        UnavailableMode,
    };
    use serde::Serialize;
    use serde_json::{json, Map, Value};

    use crate::session::fold::FoldedSnapshot;

    /// What the fold wrote before its rows were shared: the result and the frame keys, flattened.
    #[derive(Serialize)]
    struct Derived<'a> {
        #[serde(flatten)]
        result: &'a ReadSessionChatResult,
        #[serde(flatten)]
        extra: &'a Map<String, Value>,
    }

    fn some(tag: &str, value: &str) -> Option<Value> {
        Some(json!({ "set": value, "tag": tag }))
    }

    /// A result with every field set, its values marked with `tag`. Built without `..`, so a
    /// field added to the result or the side state fails this test's build until it is set here
    /// too.
    pub(crate) fn every_field_set(tag: &str) -> ReadSessionChatResult {
        // The rows are written by their own derived code, so any row does.
        let message = |id: &str| -> ChatMessage {
            serde_json::from_value(json!({
                "id": id,
                "role": "assistant",
                "blocks": [{ "type": "text", "text": format!("row {id} {tag}") }],
                "timestamp": 1_700_000_000_000_i64,
                "source": "transcript",
                "turnId": "turn",
            }))
            .unwrap()
        };
        // Values that differ between tags, so two results built with different tags differ in
        // every field a fold can take from either.
        let flag = tag.len() % 2 == 0;
        let number = tag.len() as i64;
        let state = ChatSideState {
            prompt: some(tag, "prompt"),
            selected_options: some(tag, "selectedOptions"),
            terminal_notice: some(tag, "terminalNotice"),
            terminal_activity: some(tag, "terminalActivity"),
            agent_fleet: some(tag, "agentFleet"),
            agent_tasks: some(tag, "agentTasks"),
            workflows: some(tag, "workflows"),
            screen_probed: Some(flag),
            agent_gone: Some(!flag),
            queue: Some(vec![json!(tag)]),
            draft: some(tag, "draft").map(Into::into),
            app_commands: Some(vec![json!(tag)]),
            returned_prompt: some(tag, "returnedPrompt"),
            coordinator_threads: some(tag, "coordinatorThreads").map(Into::into),
            coordinator_threads_patch: some(tag, "coordinatorThreadsPatch"),
            side_chats: some(tag, "sideChats"),
            side_chat_of: some(tag, "sideChatOf"),
            secret_requests: some(tag, "secretRequests"),
            relayed_answers: some(tag, "relayedAnswers"),
            autopilot: some(tag, "autopilot"),
            collab: some(tag, "collab"),
            agent_message_deliveries: some(tag, "agentMessageDeliveries"),
            agent_session_id: Some(format!("agent-session {tag}")),
            account_switch: Tri::Value(json!(tag)),
            api_retry: if flag {
                Tri::Null
            } else {
                Tri::Value(json!(tag))
            },
            pending_model_selection: Tri::Value(json!(tag)),
            async_questions_since: Tri::Value(number),
            forked_at: if flag { Tri::Value(number) } else { Tri::Null },
            retired_async_question_ids: Some(vec![format!("retired {tag}")]),
            later_question_ids: Some(vec![format!("later {tag}")]),
            unavailable_modes: Some(vec![UnavailableMode {
                value: "plan".into(),
                why: tag.into(),
            }]),
            removed_question_ids: Some(vec![format!("removed {tag}")]),
            resource_index: Some(vec![json!(tag)].into()),
        };
        ReadSessionChatResult {
            messages: vec![message("a"), message("b")],
            has_more: true,
            has_more_exact: Some(true),
            before_offset: 9,
            epoch: 3,
            seq: 11,
            status: ChatStatus::Ready,
            working: Some(true),
            fingerprint: Some("fingerprint".into()),
            agent: Some("claude".into()),
            session_agent_id: Some("claude".into()),
            available_agents: some(tag, "availableAgents"),
            switchable_agents: some(tag, "switchableAgents"),
            uses_own_login: Some(false),
            catch_up: Some(true),
            session_details: Some(false),
            send_on_pick: Some(true),
            answers_wait_for_idle: Some(false),
            all_orchestrator_efforts: Some(true),
            enabled_agent_icons: Some(vec!["claude".into()]),
            marked_heading_strength: Some(40.0),
            chat_text_strength: Some(72.5),
            chat_look: Some(ChatLookSettings {
                message_layout: Some("bubbles".into()),
                user_style: Some("plain".into()),
                agent_work: Some("folded".into()),
                status_in_transcript: Some(true),
                spacing: Some("compact".into()),
            }),
            chat_view: Some(
                [(
                    "quietView".to_string(),
                    ChatViewField {
                        value: Some(flag),
                        at: number,
                    },
                )]
                .into(),
            ),
            quiet_view: Some(ChatQuietViewSettings {
                coordinators: Some(flag),
                chats: Some(!flag),
                expand_last: Some(true),
            }),
            status_line_pins: Some(
                [(
                    "claudeContext".to_string(),
                    json!({ "starred": { "repo": flag }, "starredOrder": [tag] }),
                )]
                .into(),
            ),
            fork_info: Some(ChatForkInfo {
                forked_from_id: Some("parent".into()),
                ancestor_ids: vec!["root".into()],
            }),
            subagent: some(tag, "subagent"),
            error: Some("error".into()),
            lifecycle: Some(TurnLifecycle {
                state: TurnLifecycleState::Working,
                turn_id: format!("turn {tag}"),
                timestamp: Some(number),
            }),
            state,
        }
    }

    #[test]
    fn writes_what_the_derived_form_wrote_for_every_field() {
        let mut folded = FoldedSnapshot::new(every_field_set("written"));
        folded
            .extra
            .insert("type".into(), json!("sessionChatSnapshot"));
        folded.extra.insert("sessionId".into(), json!("session"));
        let written = serde_json::to_string(&folded).unwrap();
        let derived = serde_json::to_string(&Derived {
            result: &folded.read_result(),
            extra: &folded.extra,
        })
        .unwrap();
        assert_eq!(written, derived);
        assert_eq!(
            serde_json::from_str::<FoldedSnapshot>(&written).unwrap(),
            folded
        );
    }
}
