//! What of a command's answer may reach a client when a secret credential is set. The contract
//! parser asks the guard about each field as it decodes it, before any trim, normalization or cut,
//! so a secret straddling a cut can never leave its prefix behind:
//! - display text (titles, names, labels, descriptions, comments) is masked, then cut to the
//!   field's own cap;
//! - a structural field (an id, a key, a state group, a project or repo it names, a credential
//!   name) that contains a secret refuses the row or the answer, since masking it would change
//!   what it names;
//! - a link, a branch name or a time that contains one is dropped.
//!
//! Without a secret set the guard changes nothing.

use super::*;

/// The guard for one answer; `AnswerGuard::NONE` when nothing is secret.
#[derive(Clone, Copy)]
pub(crate) struct AnswerGuard<'a>(Option<&'a CommandEnv>);

impl AnswerGuard<'static> {
    pub(crate) const NONE: Self = Self(None);
}

impl<'a> AnswerGuard<'a> {
    pub(crate) fn new(env: &'a CommandEnv) -> Self {
        Self(Some(env))
    }

    fn reveals(&self, decoded: &str) -> bool {
        self.0.is_some_and(|env| env.reveals_secret(decoded))
    }

    /// Display text as decoded: masked whole (never through `shown`, whose message cut would
    /// shorten a 500-character title to 300), then trimmed; `None` when blank; cut to `cap`.
    pub(crate) fn display(&self, decoded: &str, cap: usize) -> Option<String> {
        let masked = self.masked(decoded);
        let trimmed = masked.trim();
        (!trimmed.is_empty()).then(|| cut(trimmed, cap))
    }

    /// Display text kept as sent apart from the mask and the cut (a label).
    pub(crate) fn display_as_sent(&self, decoded: &str, cap: usize) -> String {
        cut(&self.masked(decoded), cap)
    }

    fn masked(&self, decoded: &str) -> String {
        match self.0 {
            Some(env) => env.redact(decoded),
            None => decoded.to_string(),
        }
    }

    /// A structural field: refused when its decoded value, untrimmed, contains a secret.
    pub(crate) fn structural(&self, key: &str, decoded: &str) -> Result<(), String> {
        if self.reveals(decoded) {
            return Err(format!("{key} contains a credential"));
        }
        Ok(())
    }

    /// A link, a branch name or a time: dropped when its decoded value contains a secret.
    pub(crate) fn droppable<'s>(&self, decoded: &'s str) -> Option<&'s str> {
        (!self.reveals(decoded)).then_some(decoded)
    }
}
