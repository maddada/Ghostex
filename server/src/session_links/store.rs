//! A session's stored links, `runtimeSettings.sessionLinks`: the links that joined it (from an
//! earlier branch, what the agent opened or reported, the issues a PR closes, the Linear tickets
//! the chat mentioned, the ones a person added) and the links a person removed.
//!
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "x on an entry, need to click twice to remove it (to confirm removal)", "also if removed": a removed link goes onto `removed` and never joins the session again by itself, whatever names it later (the branch, the agent, the chat). Only adding it again by hand (or `ghostex link add`), or restoring it, takes it off the list.
//! CDXC:SessionLinks 2026-10-24 DECISION:
//! User: "when a link is gone i see that i can't bring it back why not?", "i mean when i hit x button twice on it". A removed link keeps what it was (`removedItems`: where it came from), the lists show it under a collapsed "Removed (N)" group with Restore, and right after a removal its row reads "Removed · Undo" for a few seconds. Restoring takes it off `removed`, puts it back as it was, and from then on it can join by itself again.

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::SessionLinkRef;

/// Where a stored link came from, which its second line says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionLinkSource {
    /// The branch the session is on, or was on before.
    Branch,
    /// The agent opened it (`gh pr create`, `gh issue create`).
    AgentOpened,
    /// The agent reported it with `ghostex link add`.
    AgentReported,
    /// A linked PR closes it.
    Closes,
    /// The user's own words in the chat named a Linear ticket of the workspace's Linear teams
    /// (`via` is `MENTIONED_BY_USER`; one stored before 2026-10-27 has none, `recheck_mentions`).
    Mentioned,
    /// A person added it.
    User,
}

impl SessionLinkSource {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::AgentOpened => "agentOpened",
            Self::AgentReported => "agentReported",
            Self::Closes => "closes",
            Self::Mentioned => "mentioned",
            Self::User => "user",
        }
    }

    pub(crate) fn from_wire(text: &str) -> Option<Self> {
        Some(match text {
            "branch" => Self::Branch,
            "agentOpened" => Self::AgentOpened,
            "agentReported" => Self::AgentReported,
            "closes" => Self::Closes,
            "mentioned" => Self::Mentioned,
            "user" => Self::User,
            _ => return None,
        })
    }
}

/// The `via` of a `Mentioned` link that joined from the user's own words.
pub(crate) const MENTIONED_BY_USER: &str = "user";

/// The `via` of a removed `Mentioned` item that was a suggestion a person dismissed with its ×
/// (`dismiss_suggestion`): Restore puts it back under Suggested, not among the links.
pub(crate) const DISMISSED_SUGGESTION: &str = "suggestion";

/// One stored link.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredSessionLink {
    pub(crate) link: SessionLinkRef,
    pub(crate) source: SessionLinkSource,
    /// The PR a `Closes` link came from (`#318`), or the branch a `Branch` link came from.
    pub(crate) via: Option<String>,
    /// A Linear issue's own URL, when it was linked by URL.
    pub(crate) url: Option<String>,
    pub(crate) added_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StoredSessionLinks {
    pub(crate) items: Vec<StoredSessionLink>,
    /// Keys of links a person removed.
    pub(crate) removed: Vec<String>,
    /// What a removed stored link was, so Restore puts it back with its source.
    pub(crate) removed_items: Vec<StoredSessionLink>,
    /// Keys of links a person pinned, in the order they were pinned.
    pub(crate) pinned: Vec<String>,
    /// What was last read about each linked link (title, state, checks, review, url, finished),
    /// by key, so the lists show it right after gxserver starts while the pass reads it again.
    pub(crate) read: Map<String, Value>,
}

impl StoredSessionLinks {
    pub(crate) fn is_removed(&self, key: &str) -> bool {
        self.removed.iter().any(|removed| removed == key)
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.items.iter().any(|item| item.link.key() == key)
    }

    /// Adds a link found automatically. Returns whether the list changed: a link already there or
    /// removed by a person is left alone.
    pub(crate) fn add_found(&mut self, link: StoredSessionLink) -> bool {
        let key = link.link.key();
        if self.is_removed(&key) || self.contains(&key) {
            return false;
        }
        self.items.push(link);
        true
    }

    /// Judges the Linear tickets that joined by a mention before the 2026-10-27 rule (no
    /// `via`) again: one the user wrote (`user_mentions`) stays and is marked
    /// `MENTIONED_BY_USER`, one anyone pinned stays, the rest leave without going onto `removed`,
    /// so one an agent named shows under Suggested again and the user writing it later still links
    /// it. Returns whether the list changed.
    ///
    /// CDXC:SessionLinks 2026-10-27 WHY:
    /// The SPX tickets on the orchestrator came from other agents' reports and the agent's own replies (CDXC:SessionLinks 2026-10-27 in transcript_scan.rs). A ticket linked under the new rule is marked and never judged again, so a transcript read only in part (the first read stops after 20,000 rows) cannot drop one the user named long ago; only the unmarked ones get this one second look.
    pub(crate) fn recheck_mentions(&mut self, user_mentions: &[String]) -> bool {
        let pinned = self.pinned.clone();
        let mut changed = false;
        self.items.retain_mut(|item| {
            let SessionLinkRef::Linear { identifier } = &item.link else {
                return true;
            };
            if item.source != SessionLinkSource::Mentioned || item.via.is_some() {
                return true;
            }
            if user_mentions.contains(identifier) {
                item.via = Some(MENTIONED_BY_USER.to_string());
                changed = true;
                return true;
            }
            if pinned.contains(&item.link.key()) {
                return true;
            }
            changed = true;
            false
        });
        changed
    }

    /// A suggestion a person dismissed with its ×: it goes under Removed (so Restore brings it
    /// back as a suggestion) and the chat never suggests it again. Returns whether anything
    /// changed.
    ///
    /// CDXC:SessionLinks 2026-10-27 DECISION:
    /// User: "i should be able to press x on suggested ones to remove them also please". A Suggested row has the same × as a linked one (click twice to confirm). The dismissed suggestion is kept like a removed link, on `removed` with a `removedItems` entry marked `DISMISSED_SUGGESTION`, so it leaves for good, shows under Removed ("Mentioned in chat") and Restore puts it back under Suggested. A Linear ticket the user later types themselves still links (`relink_typed`).
    pub(crate) fn dismiss_suggestion(&mut self, link: SessionLinkRef) -> bool {
        let key = link.key();
        if self.contains(&key) {
            return self.remove(&key);
        }
        let changed = self.remove(&key);
        self.removed_items.retain(|item| item.link.key() != key);
        self.removed_items.push(StoredSessionLink {
            link,
            source: SessionLinkSource::Mentioned,
            via: Some(DISMISSED_SUGGESTION.to_string()),
            url: None,
            added_at: Some(now_iso()),
        });
        changed
    }

    /// Whether `key` is on Removed only because a person dismissed it as a suggestion.
    pub(crate) fn is_dismissed_suggestion(&self, key: &str) -> bool {
        self.is_removed(key)
            && self.removed_items.iter().any(|item| {
                item.link.key() == key && item.via.as_deref() == Some(DISMISSED_SUGGESTION)
            })
    }

    /// A Linear ticket the user typed after dismissing it as a suggestion: off Removed, so the
    /// pass links it like any ticket the user names. Returns whether anything changed.
    pub(crate) fn relink_typed(&mut self, key: &str) -> bool {
        if !self.is_dismissed_suggestion(key) {
            return false;
        }
        self.removed.retain(|removed| removed != key);
        self.removed_items.retain(|item| item.link.key() != key);
        true
    }

    /// Adds a link a person (or `ghostex link add`) asked for, taking it off the removed list.
    pub(crate) fn add_by_hand(&mut self, link: StoredSessionLink) -> bool {
        let key = link.link.key();
        let was_removed = self.is_removed(&key);
        self.removed.retain(|removed| removed != &key);
        if self.contains(&key) {
            return was_removed;
        }
        self.items.push(link);
        true
    }

    /// Drops a link and remembers that it was removed, and what it was.
    pub(crate) fn remove(&mut self, key: &str) -> bool {
        let before = self.items.len();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|item| item.link.key() == key);
        self.items = kept;
        self.removed_items.retain(|item| item.link.key() != key);
        self.removed_items.extend(gone);
        let newly_removed = !self.is_removed(key);
        if newly_removed {
            self.removed.push(key.to_string());
        }
        self.pinned.retain(|pinned| pinned != key);
        newly_removed || self.items.len() != before
    }

    pub(crate) fn is_pinned(&self, key: &str) -> bool {
        self.pinned.iter().any(|pinned| pinned == key)
    }

    /// Pins or unpins a link. Returns whether anything changed.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "for the linked issues/prs/linear i want to be able to pin a few so they appear on top under "pinned" and they always appear in the sidebar. this is so i can pin the main pr/issue for each thread. we should always show the pinned issue as 1st thing (gh/linear) then the pinned pr after it in the sidebar order and in the floating and top of the chat pane so it's consistent". Pins are kept per session with its links (`sessionLinks.pinned`), so every client and `ghostex link pin|unpin` agree; removing a link unpins it.
    pub(crate) fn set_pinned(&mut self, key: &str, pinned: bool) -> bool {
        let was = self.is_pinned(key);
        if pinned && !was {
            self.pinned.push(key.to_string());
        } else if !pinned && was {
            self.pinned.retain(|known| known != key);
        }
        was != pinned
    }

    /// Takes a link off the removed list and puts back the stored link it was, when it had one.
    /// Returns whether anything changed.
    pub(crate) fn restore(&mut self, key: &str) -> bool {
        let was_removed = self.is_removed(key);
        self.removed.retain(|removed| removed != key);
        let (back, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.removed_items)
            .into_iter()
            .partition(|item| item.link.key() == key);
        self.removed_items = kept;
        let mut changed = was_removed;
        for mut item in back {
            // A dismissed suggestion goes back under Suggested, not among the links.
            if item.via.as_deref() == Some(DISMISSED_SUGGESTION) {
                continue;
            }
            // A person brought it back, so `recheck_mentions` leaves it alone.
            if item.source == SessionLinkSource::Mentioned && item.via.is_none() {
                item.via = Some(MENTIONED_BY_USER.to_string());
            }
            if !self.contains(key) {
                self.items.push(item);
                changed = true;
            }
        }
        changed
    }

    fn to_value(&self) -> Value {
        let item_value = |item: &StoredSessionLink| {
            let mut object = Map::new();
            object.insert("key".to_string(), json!(item.link.key()));
            object.insert("source".to_string(), json!(item.source.as_wire()));
            if let Some(via) = &item.via {
                object.insert("via".to_string(), json!(via));
            }
            if let Some(url) = &item.url {
                object.insert("url".to_string(), json!(url));
            }
            if let Some(added_at) = &item.added_at {
                object.insert("addedAt".to_string(), json!(added_at));
            }
            Value::Object(object)
        };
        let removed_items: Vec<Value> = self.removed_items.iter().map(item_value).collect();
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|item| {
                let mut object = Map::new();
                object.insert("key".to_string(), json!(item.link.key()));
                object.insert("source".to_string(), json!(item.source.as_wire()));
                if let Some(via) = &item.via {
                    object.insert("via".to_string(), json!(via));
                }
                if let Some(url) = &item.url {
                    object.insert("url".to_string(), json!(url));
                }
                if let Some(added_at) = &item.added_at {
                    object.insert("addedAt".to_string(), json!(added_at));
                }
                Value::Object(object)
            })
            .collect();
        json!({
            "items": items,
            "removed": self.removed,
            "removedItems": removed_items,
            "pinned": self.pinned,
            "read": self.read,
        })
    }
}

pub(crate) fn stored_session_links(session: &Value) -> StoredSessionLinks {
    let Some(links) = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get("sessionLinks"))
    else {
        return StoredSessionLinks::default();
    };
    let parse_items = |field: &str| -> Vec<StoredSessionLink> {
        links
            .get(field)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let text = |key: &str| {
                            item.get(key)
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|text| !text.is_empty())
                                .map(str::to_string)
                        };
                        Some(StoredSessionLink {
                            link: SessionLinkRef::from_key(&text("key")?)?,
                            source: text("source")
                                .as_deref()
                                .and_then(SessionLinkSource::from_wire)
                                .unwrap_or(SessionLinkSource::User),
                            via: text("via"),
                            url: text("url"),
                            added_at: text("addedAt"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let items = parse_items("items");
    let removed_items = parse_items("removedItems");
    let keys = |field: &str| -> Vec<String> {
        links
            .get(field)
            .and_then(Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    StoredSessionLinks {
        items,
        removed: keys("removed"),
        removed_items,
        pinned: keys("pinned"),
        read: links
            .get("read")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
    }
}

/// Writes `runtimeSettings.sessionLinks` in one statement, patched in place like `workLinks`
/// (hooks, titles and activity rewrite the same JSON all the time).
pub(crate) fn write_session_links(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    links: &StoredSessionLinks,
) -> Result<bool, DomainStateError> {
    let changed = db
        .execute(
            "UPDATE sessions SET runtimeSettingsJson = json_set(COALESCE(runtimeSettingsJson, '{}'), '$.sessionLinks', json(?1)) \
             WHERE projectId = ?2 AND sessionId = ?3",
            params![links.to_value().to_string(), project_id, session_id],
        )
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite session links error: {error}"),
        })?;
    Ok(changed > 0)
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(identifier: &str, source: SessionLinkSource, via: Option<&str>) -> StoredSessionLink {
        StoredSessionLink {
            link: SessionLinkRef::Linear {
                identifier: identifier.to_string(),
            },
            source,
            via: via.map(str::to_string),
            url: None,
            added_at: None,
        }
    }

    /// CDXC:SessionLinks 2026-10-27: the tickets linked from other agents' text leave; the user's,
    /// pinned and hand-added ones stay.
    #[test]
    fn recheck_mentions_drops_tickets_the_user_never_wrote() {
        let mut stored = StoredSessionLinks {
            items: vec![
                linear("SPX-13211", SessionLinkSource::Mentioned, None),
                linear("SPX-123", SessionLinkSource::Mentioned, None),
                linear("SPX-12599", SessionLinkSource::Mentioned, None),
                linear("SPX-1", SessionLinkSource::User, None),
                linear(
                    "GHX-9",
                    SessionLinkSource::Mentioned,
                    Some(MENTIONED_BY_USER),
                ),
            ],
            pinned: vec!["linear:SPX-12599".to_string()],
            ..Default::default()
        };
        assert!(stored.recheck_mentions(&["SPX-123".to_string()]));
        let kept: Vec<(String, Option<String>)> = stored
            .items
            .iter()
            .map(|item| (item.link.key(), item.via.clone()))
            .collect();
        assert_eq!(
            kept,
            vec![
                ("linear:SPX-123".to_string(), Some("user".to_string())),
                ("linear:SPX-12599".to_string(), None),
                ("linear:SPX-1".to_string(), None),
                ("linear:GHX-9".to_string(), Some("user".to_string())),
            ]
        );
        // Dropped, not removed: the user naming it later still links it.
        assert!(stored.removed.is_empty());
        assert!(!stored.recheck_mentions(&[]));
    }

    #[test]
    fn a_dismissed_suggestion_stays_out_until_restored_or_typed() {
        let link = SessionLinkRef::Linear {
            identifier: "SPX-7".to_string(),
        };
        let mut stored = StoredSessionLinks::default();
        assert!(stored.dismiss_suggestion(link.clone()));
        assert!(stored.is_removed("linear:SPX-7"));
        assert!(stored.is_dismissed_suggestion("linear:SPX-7"));
        // The pass cannot bring it back by itself.
        assert!(!stored.add_found(linear("SPX-7", SessionLinkSource::Mentioned, None)));
        // Restore puts it back under Suggested: off Removed, not linked.
        assert!(stored.restore("linear:SPX-7"));
        assert!(!stored.is_removed("linear:SPX-7") && !stored.contains("linear:SPX-7"));
        // Dismissed again, then typed by the user: it may link.
        stored.dismiss_suggestion(link);
        assert!(stored.relink_typed("linear:SPX-7"));
        assert!(stored.add_found(linear("SPX-7", SessionLinkSource::Mentioned, Some("user"))));
        // A link a person removed is not one the user's typing brings back.
        stored.remove("linear:SPX-7");
        assert!(!stored.relink_typed("linear:SPX-7"));
    }

    #[test]
    fn a_restored_mention_is_not_rechecked() {
        let mut stored = StoredSessionLinks {
            items: vec![linear("SPX-5", SessionLinkSource::Mentioned, None)],
            ..Default::default()
        };
        stored.remove("linear:SPX-5");
        assert!(stored.restore("linear:SPX-5"));
        assert!(!stored.recheck_mentions(&[]));
        assert!(stored.contains("linear:SPX-5"));
    }
}
