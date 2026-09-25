//! Presentation deltas (`presentationDelta.delta`).

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::presentation::{PresentationGroup, PresentationProject, PresentationSession};

/// One change to the presentation.
///
/// The Rust server emits five types today: `projectAdded`, `projectUpdated`, `projectRemoved`,
/// `sessionPresentationChanged`, and `sessionRemoved`. Eleven more names exist only in the
/// TypeScript union (older and remote daemons, and the local patches of the QuickJS runtime);
/// they are accepted as aliases because daemons of other versions are merged into one client.
/// Serialization always writes the canonical name.
///
/// A session variant always carries the whole session: apply it as a replacement, never a merge.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PresentationDelta {
    ProjectAdded {
        project: Box<PresentationProject>,
        /// The full domain project row; kept loose until a narrow typed view is needed.
        #[serde(skip_serializing_if = "Option::is_none")]
        domain_project: Option<Value>,
    },
    ProjectUpdated {
        project: Box<PresentationProject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        domain_project: Option<Value>,
    },
    /// Also what a requested add or update becomes when the project is gone or no longer
    /// presentable (parked, hidden, carrier).
    ProjectRemoved { project_id: String },
    /// Upsert of a whole session. Legacy aliases: `sessionAdded`, `sessionUpdated`,
    /// `sessionMoved`, `sessionTitleChanged`, `sessionActivityChanged`,
    /// `sessionLifecycleChanged`, `sessionSurfaceChanged`.
    SessionPresentationChanged { session: Box<PresentationSession> },
    /// Also sent when the session no longer qualifies for the presentation (stopped and not
    /// pinned, parked, favorite, or tagged).
    SessionRemoved {
        project_id: String,
        session_id: String,
    },
    /// Legacy only (`groupAdded`, `groupUpdated`, `groupOrderChanged`).
    #[serde(rename = "groupUpdated")]
    GroupUpserted { group: PresentationGroup },
    /// Legacy only. Also drops the sessions of that group.
    GroupRemoved {
        project_id: String,
        group_id: String,
    },
    /// A delta type this client does not know: advance the revision, change nothing.
    #[serde(rename = "unknown")]
    Unknown { delta_type: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectBody {
    project: Box<PresentationProject>,
    #[serde(default)]
    domain_project: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRemovedBody {
    project_id: String,
}

#[derive(Deserialize)]
struct SessionBody {
    session: Box<PresentationSession>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRemovedBody {
    project_id: String,
    session_id: String,
}

#[derive(Deserialize)]
struct GroupBody {
    group: PresentationGroup,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupRemovedBody {
    project_id: String,
    group_id: String,
}

impl PresentationDelta {
    /// The canonical wire `type` of this delta (the original name for an unknown one).
    pub fn delta_type(&self) -> &str {
        match self {
            Self::ProjectAdded { .. } => "projectAdded",
            Self::ProjectUpdated { .. } => "projectUpdated",
            Self::ProjectRemoved { .. } => "projectRemoved",
            Self::SessionPresentationChanged { .. } => "sessionPresentationChanged",
            Self::SessionRemoved { .. } => "sessionRemoved",
            Self::GroupUpserted { .. } => "groupUpdated",
            Self::GroupRemoved { .. } => "groupRemoved",
            Self::Unknown { delta_type } => delta_type.as_str(),
        }
    }

    /// Dispatches on `type` by hand so an unknown type keeps its name for diagnostics, which
    /// `#[serde(other)]` cannot do.
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        let delta_type = match value.get("type").and_then(Value::as_str) {
            Some(delta_type) => delta_type.to_string(),
            None => {
                return Err(serde_json::Error::custom(
                    "presentation delta has no string `type`",
                ))
            }
        };
        Ok(match delta_type.as_str() {
            "projectAdded" => {
                let body: ProjectBody = serde_json::from_value(value)?;
                Self::ProjectAdded {
                    project: body.project,
                    domain_project: body.domain_project,
                }
            }
            "projectUpdated" => {
                let body: ProjectBody = serde_json::from_value(value)?;
                Self::ProjectUpdated {
                    project: body.project,
                    domain_project: body.domain_project,
                }
            }
            "projectRemoved" => {
                let body: ProjectRemovedBody = serde_json::from_value(value)?;
                Self::ProjectRemoved {
                    project_id: body.project_id,
                }
            }
            "sessionPresentationChanged"
            | "sessionAdded"
            | "sessionUpdated"
            | "sessionMoved"
            | "sessionTitleChanged"
            | "sessionActivityChanged"
            | "sessionLifecycleChanged"
            | "sessionSurfaceChanged" => {
                let body: SessionBody = serde_json::from_value(value)?;
                Self::SessionPresentationChanged {
                    session: body.session,
                }
            }
            "sessionRemoved" => {
                let body: SessionRemovedBody = serde_json::from_value(value)?;
                Self::SessionRemoved {
                    project_id: body.project_id,
                    session_id: body.session_id,
                }
            }
            "groupAdded" | "groupUpdated" | "groupOrderChanged" => {
                let body: GroupBody = serde_json::from_value(value)?;
                Self::GroupUpserted { group: body.group }
            }
            "groupRemoved" => {
                let body: GroupRemovedBody = serde_json::from_value(value)?;
                Self::GroupRemoved {
                    project_id: body.project_id,
                    group_id: body.group_id,
                }
            }
            _ => Self::Unknown { delta_type },
        })
    }
}

impl<'de> Deserialize<'de> for PresentationDelta {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(D::Error::custom)
    }
}
