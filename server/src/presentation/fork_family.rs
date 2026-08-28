use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::*;

/*
CDXC:SessionForkFamilies 2026-08-28:
One conversation family can own several registry session rows. `codex fork` and
`codex resume` both write a NEW rollout file with a NEW agent session uuid, so
the row that was tailing the old rollout keeps pointing at a dead file while the
continuation lives on under a different id. Ghostex-initiated forks record the
edge directly (`forkedFromSessionId` / `restoredFromSessionId`); out-of-band
forks only become visible once the chat follower adopts the successor id and the
row's previous id is appended to `runtimeSettings.previousAgentSessionIds`.

This derivation is registry-only and O(n) over the rows SQLite already handed us:
no rollout file is opened while a list is being built. Two rules come out of it:

- A row is SUPERSEDED, and therefore hidden from Previous Sessions, only when it
  is closed AND some other row descends from it. A deliberate fork leaves two
  living leaves, and both stay visible because neither has a descendant.
- Every row that survives publishes how many VISIBLE siblings share its history,
  so a card can say "this conversation has branches" without a second round trip.
*/

/// The registry ids a session claims as its parent, most trustworthy first.
/// Direct registry ids come from Ghostex's own fork/restore writes; agent
/// session ids are resolved through the id map because only the transcript
/// knows them.
struct ParentClaims {
    registry_ids: Vec<String>,
    agent_session_ids: Vec<String>,
}

pub(crate) struct SessionForkFamilies {
    parents: HashMap<String, String>,
    hidden: HashSet<String>,
    /// Family index per session id, and the members of each family.
    family_of: HashMap<String, usize>,
    families: Vec<Vec<String>>,
}

impl SessionForkFamilies {
    pub(crate) fn build(sessions: &[Value]) -> Self {
        let mut known: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();
        for session in sessions {
            let Some(session_id) = string_field(session, "sessionId") else {
                continue;
            };
            if known.insert(session_id.clone()) {
                order.push(session_id);
            }
        }
        order.sort();

        /*
        An agent session id that two rows both claim cannot resolve to a single
        parent, so it resolves to none at all: inventing an edge here would hide
        a living row behind a coin flip.
        */
        let mut agent_owners: HashMap<String, Vec<String>> = HashMap::new();
        for session in sessions {
            let (Some(session_id), Some(agent_session_id)) = (
                string_field(session, "sessionId"),
                read_runtime_text(session, "agentSessionId"),
            ) else {
                continue;
            };
            let owners = agent_owners.entry(agent_session_id).or_default();
            if !owners.contains(&session_id) {
                owners.push(session_id);
            }
        }
        let agent_owner: HashMap<String, String> = agent_owners
            .into_iter()
            .filter(|(_, owners)| owners.len() == 1)
            .map(|(agent_session_id, mut owners)| (agent_session_id, owners.remove(0)))
            .collect();

        let by_id: HashMap<String, &Value> = sessions
            .iter()
            .filter_map(|session| {
                string_field(session, "sessionId").map(|session_id| (session_id, session))
            })
            .collect();

        let mut parents: HashMap<String, String> = HashMap::new();
        for session_id in &order {
            let Some(session) = by_id.get(session_id) else {
                continue;
            };
            let claims = parent_claims(session);
            let resolved = claims
                .registry_ids
                .into_iter()
                .chain(
                    claims
                        .agent_session_ids
                        .into_iter()
                        .filter_map(|agent_session_id| agent_owner.get(&agent_session_id).cloned()),
                )
                .find(|candidate| {
                    candidate != session_id
                        && known.contains(candidate)
                        && !creates_cycle(&parents, session_id, candidate)
                });
            if let Some(parent) = resolved {
                parents.insert(session_id.clone(), parent);
            }
        }

        let has_descendant: HashSet<String> = parents.values().cloned().collect();
        let hidden: HashSet<String> = has_descendant
            .iter()
            .filter(|session_id| {
                by_id
                    .get(*session_id)
                    .is_some_and(|session| !is_active(session))
            })
            .cloned()
            .collect();

        let (family_of, families) = group_families(&order, &parents);
        Self {
            parents,
            hidden,
            family_of,
            families,
        }
    }

    pub(crate) fn parent_of(&self, session_id: &str) -> Option<&str> {
        self.parents.get(session_id).map(String::as_str)
    }

    /// A closed row that some other row descends from. Superseded ancestors are
    /// dropped from Previous Sessions; running and sleeping rows never are.
    pub(crate) fn is_superseded(&self, session_id: &str) -> bool {
        self.hidden.contains(session_id)
    }

    pub(crate) fn family_session_ids(&self, session_id: &str) -> &[String] {
        self.family_of
            .get(session_id)
            .and_then(|index| self.families.get(*index))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn visible_family_session_ids(&self, session_id: &str) -> Vec<String> {
        self.family_session_ids(session_id)
            .iter()
            .filter(|member| !self.hidden.contains(*member))
            .cloned()
            .collect()
    }

    /// Publishes the fork shape onto an already-projected row. All three keys
    /// are present-only: a family of one, and a daemon that predates fork
    /// awareness, both send nothing.
    pub(crate) fn insert_fork_fields(&self, session_id: &str, output: &mut Map<String, Value>) {
        insert_optional_string(
            output,
            "forkedFromSessionId",
            self.parent_of(session_id).map(str::to_string),
        );
        let visible = self.visible_family_session_ids(session_id);
        if visible.len() < 2 {
            return;
        }
        output.insert(
            "forkBranchCount".to_string(),
            Value::Number(visible.len().into()),
        );
        output.insert(
            "forkFamilySessionIds".to_string(),
            Value::Array(visible.into_iter().map(Value::String).collect()),
        );
    }
}

fn parent_claims(session: &Value) -> ParentClaims {
    let mut registry_ids = Vec::new();
    let mut push_registry = |value: Option<String>| {
        if let Some(value) = value.map(|value| value.trim().to_string()) {
            if !value.is_empty() && !registry_ids.contains(&value) {
                registry_ids.push(value);
            }
        }
    };
    push_registry(read_runtime_text(session, "forkedFromSessionId"));
    push_registry(read_launch_text(session, "forkedFromSessionId"));
    push_registry(read_hidden_metadata_text(session, "restoredFromSessionId"));

    let agent_session_ids = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("previousAgentSessionIds"))
        .and_then(Value::as_array)
        .map(|items| {
            let mut ids: Vec<String> = Vec::new();
            /*
            Newest first: the id this row carried immediately before its current
            one is the closest ancestor, and the list is appended to in order.
            */
            for item in items.iter().rev() {
                let Some(value) = item
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                if !ids.iter().any(|existing| existing == value) {
                    ids.push(value.to_string());
                }
            }
            ids
        })
        .unwrap_or_default();

    ParentClaims {
        registry_ids,
        agent_session_ids,
    }
}

fn read_launch_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_hidden_metadata_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("hiddenMetadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// True when making `candidate` the parent of `session_id` would close a loop.
/// Restore chains can be long, so the walk is bounded by the edge count already
/// accepted rather than trusting the data to terminate.
fn creates_cycle(parents: &HashMap<String, String>, session_id: &str, candidate: &str) -> bool {
    let mut cursor = candidate;
    for _ in 0..=parents.len() {
        if cursor == session_id {
            return true;
        }
        match parents.get(cursor) {
            Some(next) => cursor = next.as_str(),
            None => return false,
        }
    }
    true
}

/// Connected components over the parent edges, walked from each root so members
/// come out in a stable order without a union-find structure.
fn group_families(
    order: &[String],
    parents: &HashMap<String, String>,
) -> (HashMap<String, usize>, Vec<Vec<String>>) {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (child, parent) in parents {
        children
            .entry(parent.as_str())
            .or_default()
            .push(child.as_str());
    }
    for entries in children.values_mut() {
        entries.sort_unstable();
    }

    let mut family_of: HashMap<String, usize> = HashMap::new();
    let mut families: Vec<Vec<String>> = Vec::new();
    for session_id in order {
        if family_of.contains_key(session_id) {
            continue;
        }
        // Climb to the family root, then collect the whole subtree beneath it.
        let mut root = session_id.as_str();
        let mut guard = 0usize;
        while let Some(parent) = parents.get(root) {
            root = parent.as_str();
            guard += 1;
            if guard > parents.len() {
                break;
            }
        }
        let index = families.len();
        let mut members: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(current) = stack.pop() {
            if family_of.contains_key(current) {
                continue;
            }
            family_of.insert(current.to_string(), index);
            members.push(current.to_string());
            if let Some(entries) = children.get(current) {
                stack.extend(entries.iter().copied());
            }
        }
        members.sort();
        families.push(members);
    }
    (family_of, families)
}
