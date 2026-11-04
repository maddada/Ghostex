//! Every PR, GitHub issue and Linear ticket a session is linked to, as the work area header's
//! chips, the links list and the sidebar's hover line read them: which glyph and colour each entry
//! gets, its state word and its second line.
//!
//! The daemon publishes `PresentationSession.links` for every session (server/src/session_links/),
//! already sorted with open work first, so clients only draw it.
//!
//! SEE-ALSO: apps/mobile/app/src/chat/native/sessionLinks.ts (the phone's copy of these rules,
//! which must give the same glyphs, words and lines).

use ghostex_gx_protocol::presentation::PresentationSessionLinks;

/// Suggestions a links list shows before its "Show all" row.
///
/// CDXC:SessionLinks 2026-10-27 DECISION:
/// User (Q669): "5, then a Show all row". The Suggested group shows its five newest; when there are more, a "Show all N" row under them shows the rest in place and turns into "Show fewer", which folds them again. gxserver sends up to 40. Every list draws it the same way: the desktop's links list (the header's dropdown and the sidebar's card), the web build's (the same view) and the phone's sheet (apps/mobile/app/src/chat/native/sessionLinks.ts keeps the same number).
pub const SUGGESTED_SHOWN_FIRST: usize = 5;

/// Linked links up to which a links list opens with its Suggested group unfolded.
///
/// CDXC:SessionLinks 2026-10-28 DECISION:
/// User: "if there are 2 or fewer links, then expand Suggested in any case, so I can manage them without having to click on it manually. This is on desktop and on mobile. If there are 3 or more links already attached, no need to auto-expand Suggested." A list that opens with 2 or fewer linked links shows Suggested unfolded whatever the saved fold says; with 3 or more the saved fold (`sessionLinksFoldedGroups`) holds as before. A fold or unfold the person makes while the list is open wins until it closes, and the open-time decision holds while it stays open (linking a third suggestion does not fold the group under their finger). Removed keeps its own fold. The phone's sheet keeps the same number (apps/mobile/app/src/chat/native/sessionLinks.ts `SUGGESTED_OPEN_UP_TO_LINKED`).
pub const SUGGESTED_OPEN_UP_TO_LINKED: usize = 2;

/// A session's links, in the client's own vocabulary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLinks {
    pub items: Vec<SessionLink>,
    pub suggested: Vec<SuggestedSessionLink>,
    /// Links a person removed, newest first: the lists' "Removed (N)" group, each with Restore.
    pub removed: Vec<SuggestedSessionLink>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkKind {
    PullRequest,
    Issue,
    Linear,
    /// A ticket of a source this client does not know: drawn with the generic ticket icon.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLink {
    /// What removing it sends (`pr:owner/repo#318`).
    pub key: String,
    pub kind: SessionLinkKind,
    /// `#318`, `repo#12` or `GHX-142`.
    pub label: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub state_name: Option<String>,
    pub checks: Option<String>,
    pub review: Option<String>,
    pub url: Option<String>,
    /// Where it came from (`branch`, `closes`, `user`…); only `closes` shows, as `source_label`.
    pub source: String,
    /// "From branch", "Agent opened", "Closed by #318"…
    pub source_label: String,
    pub finished: bool,
    /// A person pinned it: gxserver lists it first, and every sidebar option but Off shows it.
    pub pinned: bool,
    /// A PR's branch (`headRefName`).
    pub branch: Option<String>,
    /// Never read yet, with a read on its way: the lists say "Loading…" and keep it out of Open.
    pub loading: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestedSessionLink {
    pub key: String,
    pub kind: SessionLinkKind,
    pub label: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub state: Option<String>,
    pub state_name: Option<String>,
    /// The second line: "Mentioned in the chat", or where a removed link came from.
    pub source_label: String,
    pub finished: bool,
    pub loading: bool,
    pub branch: Option<String>,
}

/// The icon an entry draws; each renderer maps it to its own SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkGlyph {
    /// Every pull request: a branch.
    Branch,
    /// Every GitHub issue: the GitHub mark.
    Github,
    /// Every Linear ticket: the Linear logo.
    Linear,
    /// A ticket of any other source.
    Ticket,
    ChecksPassing,
    ChecksFailing,
    ChecksPending,
}

/// The colour an icon or a word takes; each renderer maps it to its palette (the sidebar's work
/// chip colours: GitHub's green, grey, purple, red and amber, Linear's yellow, green and indigo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionLinkTone {
    Open,
    Draft,
    Merged,
    Closed,
    Pending,
    Started,
    Review,
    Done,
    Linear,
}

fn kind_of(text: &str) -> SessionLinkKind {
    match text {
        "pullRequest" => SessionLinkKind::PullRequest,
        "issue" => SessionLinkKind::Issue,
        "linear" => SessionLinkKind::Linear,
        _ => SessionLinkKind::Other,
    }
}

fn other_link(
    item: &ghostex_gx_protocol::presentation::PresentationSuggestedLink,
) -> SuggestedSessionLink {
    SuggestedSessionLink {
        key: item.key.clone(),
        kind: kind_of(&item.kind),
        label: item.label.clone(),
        url: item.url.clone(),
        title: item.title.clone().filter(|title| !title.trim().is_empty()),
        state: item.state.clone(),
        state_name: item.state_name.clone(),
        source_label: item.source_label.clone(),
        finished: item.finished,
        loading: item.loading,
        branch: item.branch.clone(),
    }
}

impl SessionLinks {
    pub fn from_presentation(links: &PresentationSessionLinks) -> Self {
        Self {
            items: links
                .items
                .iter()
                .map(|item| SessionLink {
                    key: item.key.clone(),
                    kind: kind_of(&item.kind),
                    label: item.label.clone(),
                    title: item.title.clone().filter(|title| !title.trim().is_empty()),
                    state: item.state.clone(),
                    state_name: item.state_name.clone(),
                    checks: item.checks.clone(),
                    review: item.review.clone(),
                    url: item.url.clone(),
                    source: item.source.clone(),
                    source_label: item.source_label.clone(),
                    finished: item.finished,
                    pinned: item.pinned,
                    loading: item.loading,
                    branch: item.branch.clone(),
                })
                .collect(),
            suggested: links.suggested.iter().map(other_link).collect(),
            removed: links.removed.iter().map(other_link).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.suggested.is_empty() && self.removed.is_empty()
    }

    /// Whether a list opening on these links shows Suggested unfolded whatever the saved fold says
    /// (`SUGGESTED_OPEN_UP_TO_LINKED`).
    pub fn suggested_opens_unfolded(&self) -> bool {
        self.items.len() <= SUGGESTED_OPEN_UP_TO_LINKED
    }

    /// The groups a list opening on these links shows folded: the saved folds, without Suggested
    /// when `suggested_opens_unfolded`.
    pub fn folded_groups_on_open(&self, saved: &[String]) -> Vec<String> {
        let unfold_suggested = self.suggested_opens_unfolded();
        saved
            .iter()
            .filter(|group| !(unfold_suggested && group.as_str() == "suggested"))
            .cloned()
            .collect()
    }

    /// The list as it stands once `keys` are removed, before gxserver publishes it: each leaves
    /// its group at once and leads Removed, where Restore brings it back.
    ///
    /// CDXC:SessionLinks 2026-10-27 DECISION:
    /// User: "please don't show undo here right away after i remove one. ther'es no need. i clicked twice and confiremd already man". A link removed with the two-click × leaves the list straight away, with no "Removed · Undo" row in its place; it stays restorable from the Removed group. This supersedes the 2026-10-24 Undo row ("when a link is gone i see that i can't bring it back why not?").
    pub fn without_removed(&self, keys: &[String]) -> SessionLinks {
        let mut links = self.clone();
        let (gone, kept): (Vec<SessionLink>, Vec<SessionLink>) = links
            .items
            .into_iter()
            .partition(|link| keys.contains(&link.key));
        links.items = kept;
        // A suggestion's × dismisses it, which also lands it under Removed.
        let (dismissed, suggested): (Vec<SuggestedSessionLink>, Vec<SuggestedSessionLink>) = links
            .suggested
            .into_iter()
            .partition(|link| keys.contains(&link.key));
        links.suggested = suggested;
        let mut removed: Vec<SuggestedSessionLink> = gone
            .iter()
            .map(SessionLink::as_removed)
            .chain(dismissed)
            .filter(|link| !links.removed.iter().any(|known| known.key == link.key))
            .collect();
        removed.append(&mut links.removed);
        links.removed = removed;
        links
    }

    /// Whether a link left out of the first `shown` chips has failing checks: the "+N" chip then
    /// carries the red dot, so a failing PR is never hidden.
    pub fn overflow_has_failing(&self, shown: usize) -> bool {
        self.items
            .iter()
            .skip(shown)
            .any(|item| item.checks.as_deref() == Some("failing"))
    }

    /// The Links pill on the chat's bar above the composer, or `None` with nothing to show.
    ///
    /// CDXC:SessionLinks 2026-10-28 DECISION:
    /// User: "I don't like how the top bar just says "3 suggested" there. I think it's better to move the links to always be in the bar above the composer. It should be between Tasks and Threads down there in a pill, so I just click on it and see all of them. The look of the pill needs to look like the one we have in the sidebar [the Badge option]. It just shows a link icon and the number of links we have. If it's just one link, you can just put the name or the ID of that link, like SPX-123, for example, or #123." So the pill is the sidebar badge's look and text (`badge_label`: the link icon, one link's ID and "+N", in the most pressing link's colour, grey once all are finished); with no linked links but suggestions it reads "3 suggested", quiet like the sidebar's suggestions badge (muted, dashed). A click opens the session's links list. This replaces the chips and the "+N" / "N suggested" / "List" pill in the chat's top bar (and the phone's strip under its header) and the setting that hid them.
    pub fn chat_pill(&self) -> Option<SessionLinksChatPill> {
        if self.items.is_empty() {
            let count = self.suggested.len();
            return (count > 0).then(|| SessionLinksChatPill {
                label: format!("{count} suggested"),
                more: 0,
                tone: SessionLinkTone::Draft,
                suggested: true,
                aria_label: format!("Links, {count} suggested"),
            });
        }
        let (label, more) = self.badge_label()?;
        let summary = self
            .badge_link()
            .map(SessionLink::summary)
            .unwrap_or_default();
        Some(SessionLinksChatPill {
            aria_label: if more > 0 {
                format!("Links, {summary} and {more} more")
            } else {
                format!("Links, {summary}")
            },
            label,
            more,
            tone: self.most_pressing_tone(),
            suggested: false,
        })
    }
}

/// The Links pill (`SessionLinks::chat_pill`): its text after the link icon and its colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLinksChatPill {
    /// The named link's ID (the badge's, `SessionLinks::badge_label`), or "3 suggested".
    pub label: String,
    /// The other linked links, drawn "+N" in a lighter weight.
    pub more: usize,
    pub tone: SessionLinkTone,
    /// Only suggestions: drawn quietly (muted, dashed).
    pub suggested: bool,
    pub aria_label: String,
}

impl SessionLink {
    /// The label without its repo name: `maestro_ed#151` is `#151`; `#151` and a Linear ID stay.
    pub fn badge_id(&self) -> String {
        match (self.kind, self.label.find('#')) {
            (SessionLinkKind::PullRequest | SessionLinkKind::Issue, Some(at)) => {
                self.label[at..].to_string()
            }
            _ => self.label.clone(),
        }
    }

    /// The icon says the kind (a branch for a PR, the GitHub mark for an issue, the Linear logo
    /// for a ticket) and its colour the state.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "Let's please not show text for issue and for Pull Request I think it's better to just show branch icon for PRs and show github icon for issues and a linear icon for linear issues." Each icon keeps its state colour (open green, merged purple, closed red, a closed issue purple, draft grey, Linear's state colours). This supersedes the same day's kind word at the start of the second line and the per-state PR and issue icons.
    pub fn glyph(&self) -> (SessionLinkGlyph, SessionLinkTone) {
        let state = self.state.as_deref();
        match self.kind {
            SessionLinkKind::PullRequest => (
                SessionLinkGlyph::Branch,
                match state {
                    Some("draft") => SessionLinkTone::Draft,
                    Some("merged") => SessionLinkTone::Merged,
                    Some("closed") => SessionLinkTone::Closed,
                    Some(_) => SessionLinkTone::Open,
                    None => SessionLinkTone::Draft,
                },
            ),
            SessionLinkKind::Issue => (
                SessionLinkGlyph::Github,
                match state {
                    Some("closed") => SessionLinkTone::Merged,
                    Some(_) => SessionLinkTone::Open,
                    None => SessionLinkTone::Draft,
                },
            ),
            SessionLinkKind::Linear => {
                let in_review = self
                    .state_name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("review"));
                (
                    SessionLinkGlyph::Linear,
                    match state {
                        Some("started") if in_review => SessionLinkTone::Review,
                        Some("started") => SessionLinkTone::Started,
                        Some("completed") => SessionLinkTone::Done,
                        Some(_) => SessionLinkTone::Draft,
                        None => SessionLinkTone::Linear,
                    },
                )
            }
            // A ticket command's ticket it can't ask about right now (`unavailable`, its last
            // title kept) is grey like one not read yet, not green like open work.
            SessionLinkKind::Other => (
                SessionLinkGlyph::Ticket,
                match state {
                    _ if self.finished => SessionLinkTone::Done,
                    Some("unavailable") | None => SessionLinkTone::Draft,
                    Some(_) => SessionLinkTone::Open,
                },
            ),
        }
    }

    /// The mark after a PR's number: its checks.
    pub fn checks_mark(&self) -> Option<(SessionLinkGlyph, SessionLinkTone)> {
        match self.checks.as_deref()? {
            "passing" => Some((SessionLinkGlyph::ChecksPassing, SessionLinkTone::Open)),
            "failing" => Some((SessionLinkGlyph::ChecksFailing, SessionLinkTone::Closed)),
            "pending" => Some((SessionLinkGlyph::ChecksPending, SessionLinkTone::Pending)),
            _ => None,
        }
    }

    /// The state in words: "Open", "Draft", "Merged", "Closed", or the Linear team's own name.
    pub fn state_label(&self) -> Option<String> {
        if matches!(self.kind, SessionLinkKind::Linear | SessionLinkKind::Other) {
            return self.state_name.clone();
        }
        let state = self.state.as_deref()?;
        let mut chars = state.chars();
        let first = chars.next()?.to_ascii_uppercase();
        Some(std::iter::once(first).chain(chars).collect())
    }

    /// The second line: only what is worth acting on, an open PR's checks and review and the PR
    /// that closes an issue ("Closed by #284"); empty draws a one-line row.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "i dont like the "You added"/"mentioned in chat" text there i think no need for that". Where a link came from ("You added", "Agent added", "From branch", "Mentioned in the chat") is not shown; only "Closed by #284" stays, with an open PR's checks and review.
    pub fn detail_line(&self) -> String {
        let (checks, review) = (self.checks_words(), self.review_words());
        let closed_by = (self.source == "closes").then_some(self.source_label.as_str());
        [checks, review, closed_by]
            .into_iter()
            .flatten()
            .filter(|part| !part.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn checks_words(&self) -> Option<&'static str> {
        match self.checks.as_deref() {
            Some("passing") => Some("Checks passing"),
            Some("failing") => Some("Checks failing"),
            Some("pending") => Some("Checks running"),
            _ => None,
        }
    }

    fn review_words(&self) -> Option<&'static str> {
        match self.review.as_deref() {
            Some("reviewRequired") => Some("Review requested"),
            Some("approved") => Some("Approved"),
            Some("changesRequested") => Some("Changes requested"),
            _ => None,
        }
    }

    /// What hovering a row's icon says: the state in words, then a PR's checks and review
    /// (`Open · Checks failing · Review requested`); "Loading…" before it was read.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "no need for the word merged and closed i should be able to hover their icon to get that word instead in tooltip". The lists' rows show no state word; the icon's colour carries the state and its tooltip says it, with a PR's checks and review.
    pub fn state_tooltip(&self) -> String {
        let state = self
            .state_label()
            .or_else(|| self.loading.then(|| "Loading…".to_string()));
        [state.as_deref(), self.checks_words(), self.review_words()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// What hovering a link says: its full title, and under it a PR's branch; nothing else (the
    /// row already shows the number, and the icon the state).
    ///
    /// CDXC:SessionLinks 2026-10-25 DECISION:
    /// User: "please make the tooltip just show the title and branch, nothing else, no need to show the #123 again or show the state 'merged' thing". The card is the title (wrapping), then the branch for a pull request; an issue or a Linear ticket shows its title alone. This supersedes the same day's card with the number, kind, state, checks and repo lines.
    pub fn hover_card_text(&self) -> String {
        let title = match (&self.title, self.loading) {
            (Some(title), _) => title.clone(),
            (None, true) => "Loading…".to_string(),
            (None, false) => self.label.clone(),
        };
        let branch = (self.kind == SessionLinkKind::PullRequest)
            .then(|| self.branch.clone())
            .flatten();
        [Some(title), branch]
            .into_iter()
            .flatten()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// How pressing an open link is, lowest first, and the tone it gives the badge: failing checks
    /// or changes asked for, then a review asked for or checks still running, then open work, then
    /// a draft.
    fn pressing_rank(&self) -> (u8, SessionLinkTone) {
        if self.checks.as_deref() == Some("failing")
            || self.review.as_deref() == Some("changesRequested")
        {
            (0, SessionLinkTone::Closed)
        } else if self.review.as_deref() == Some("reviewRequired")
            || self.checks.as_deref() == Some("pending")
        {
            (1, SessionLinkTone::Pending)
        } else if self.state.as_deref() != Some("draft") {
            (2, self.glyph().1)
        } else {
            (3, SessionLinkTone::Draft)
        }
    }

    /// The tooltip and the list's title fallback: `#318 · Port the Find screen · Open`.
    pub fn summary(&self) -> String {
        [
            Some(self.label.clone()),
            self.title.clone(),
            self.state_label(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

impl SessionLink {
    /// The entry as the Removed group lists it.
    fn as_removed(&self) -> SuggestedSessionLink {
        SuggestedSessionLink {
            key: self.key.clone(),
            kind: self.kind,
            label: self.label.clone(),
            url: self.url.clone(),
            title: self.title.clone(),
            state: self.state.clone(),
            state_name: self.state_name.clone(),
            source_label: self.source_label.clone(),
            finished: self.finished,
            loading: self.loading,
            branch: self.branch.clone(),
        }
    }
}

impl SuggestedSessionLink {
    /// The entry as a linked one, which the lists draw with the same row: its glyph and state come
    /// from what was read about it, and it has no second line.
    pub fn as_link(&self) -> SessionLink {
        SessionLink {
            key: self.key.clone(),
            kind: self.kind,
            label: self.label.clone(),
            title: self.title.clone(),
            state: self.state.clone(),
            state_name: self.state_name.clone(),
            checks: None,
            review: None,
            url: self.url.clone(),
            source: String::new(),
            source_label: self.source_label.clone(),
            finished: self.finished,
            pinned: false,
            loading: self.loading,
            branch: self.branch.clone(),
        }
    }
}

/// What a sidebar row shows of its session's links (`sidebarSessionLinks`).
///
/// CDXC:SessionLinks 2026-10-24 DECISION:
/// User, of the four ways to see a session's links without hovering: "i want a, b, c,d to be options in settings so people pick but they want (fully disabled should be option too)". Always (A), Selected session (B), Badge (C), Open work only (D) and Off.
/// CDXC:SessionLinks 2026-10-27 DECISION:
/// User: "and also please make the sidebar show as badge by default". Badge is the default; a person who picked an option keeps it. This replaces the 2026-10-24 Always default ("i want all the settings related to the sidebar links we added to match what i have set now").
/// CDXC:SessionLinks 2026-10-24 DECISION:
/// User: "i think i we don't need a separate thing called "work mode" and instead we can just do this". Work mode cards follow this setting too; their permanent chip line (CDXC:WorkMode 2026-10-09) is gone, except while a merged PR's Clean up / Keep offer waits. This supersedes the 2026-10-24 rule that kept the sidebar line Work-mode-only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionLinksDisplay {
    Always,
    Selected,
    #[default]
    Badge,
    OpenOnly,
    Off,
}

impl SessionLinksDisplay {
    pub const SETTING_KEY: &'static str = "sidebarSessionLinks";

    /// The setting's saved value; anything unknown is the default.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("always") => Self::Always,
            Some("selected") => Self::Selected,
            Some("open") => Self::OpenOnly,
            Some("off") => Self::Off,
            _ => Self::Badge,
        }
    }

    pub fn from_settings_json(settings: &serde_json::Value) -> Self {
        Self::from_setting(
            settings
                .get(Self::SETTING_KEY)
                .and_then(serde_json::Value::as_str),
        )
    }
}

/// The badge a row shows instead of a line: one link's ID and "+N" for the others, coloured by the
/// most pressing link (`SessionLinks::badge_label`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLinksBadge {
    /// Every linked link, or every suggestion.
    pub count: usize,
    pub tone: SessionLinkTone,
    /// The session has no links yet, only suggestions: the badge counts those and is drawn
    /// quietly (muted, dashed).
    pub suggested: bool,
    /// The named link's ID (`#123`, `SPX-123`), or the suggestion count.
    pub label: String,
    /// The other linked links, drawn "+N" in a lighter weight after the ID; 0 draws none.
    pub more: usize,
}

/// What one sidebar row shows of its links.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLinksRow {
    /// Indices into `SessionLinks::items` the row's second line shows, in order; empty draws no
    /// line. The renderer fits what it can and shows "+N" for the rest.
    pub line: Vec<usize>,
    pub badge: Option<SessionLinksBadge>,
    /// Hovering the row (or its badge) opens the floating links card.
    pub hover_card: bool,
}

impl SessionLinks {
    /// The colour of the most pressing link: failing checks or changes asked for, then a review
    /// asked for or checks still running, then open work, then a draft; grey once every link is
    /// merged or closed.
    pub fn most_pressing_tone(&self) -> SessionLinkTone {
        Self::most_pressing_tone_of(self.items.iter())
    }

    /// CDXC:SessionLinks 2026-10-27 DECISION:
    /// User: "make the badge gray if it's merged (not open)". A badge whose links are all merged or closed (none open) is grey (`Draft`, GitHub's grey), not merged purple; while any of them is open it keeps the most pressing link's colour.
    fn most_pressing_tone_of<'a>(links: impl Iterator<Item = &'a SessionLink>) -> SessionLinkTone {
        // Only open links colour it; none open (or none at all) is grey.
        links
            .filter(|link| !link.finished)
            .map(SessionLink::pressing_rank)
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, tone)| tone)
            .unwrap_or(SessionLinkTone::Draft)
    }

    /// The link the badge and the chat's Links pill name: the first pinned one; else the only one;
    /// else the most pressing open one (the one whose tone colours the badge), the first in the
    /// list on a tie (gxserver lists open work first); else, every link finished, the first.
    pub fn badge_link(&self) -> Option<&SessionLink> {
        if let Some(pinned) = self.items.iter().find(|link| link.pinned) {
            return Some(pinned);
        }
        self.items
            .iter()
            .filter(|link| !link.finished)
            // `min_by_key` keeps the first of equal keys.
            .min_by_key(|link| link.pressing_rank().0)
            .or_else(|| self.items.first())
    }

    /// The badge's text: the named link's ID and how many other links follow it as "+N".
    ///
    /// CDXC:SessionLinks 2026-10-28 DECISION:
    /// User: "If we pin one link, or if it's just one link, we should show the ID for that link instead of just showing the number of total links. Then we can put +3 or something like that if there are more links." From the mockup (Q702 "id", Q703 "yes, as shown"): with several links and none pinned, the most pressing link's ID and "+N". The same text on the sidebar badge (Badge option) and the chat's Links pill; a pinned link no longer has its own line under the name with Badge, it is the badge's ID. The colour stays the most pressing link's (grey once none is open). Supersedes the 2026-10-24 badge that counted the links the pinned chips left out.
    ///
    /// CDXC:SessionLinks 2026-10-28 DECISION:
    /// User: "let's not show the repo name here". gxserver labels a PR or issue from another repo than the session's `repo#151` (a folder `mastro_ed` on a repo `maestro_ed` made every PR read `maestro_ed#151`); the badge and the Links pill have no room for it and show `#151` (`#151 +2` with more links). A Linear ticket keeps its ID. The Links panel, the hover card and the sidebar's line under the name keep the full label so PRs from two repos stay apart. The phone's `sessionLinksBadgeLabel` does the same.
    pub fn badge_label(&self) -> Option<(String, usize)> {
        let link = self.badge_link()?;
        Some((link.badge_id(), self.items.len() - 1))
    }

    /// What a row shows under `display`; `selected` is whether the row is the session the window
    /// shows.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "why didn't we suggest #263 here for this session?" A session whose only links are suggestions shows a quiet badge (muted and dashed, counting the suggestions) that opens the floating card on hover, under every option but Off, instead of nothing.
    pub fn row(&self, display: SessionLinksDisplay, selected: bool) -> SessionLinksRow {
        if display == SessionLinksDisplay::Off {
            return SessionLinksRow::default();
        }
        if self.items.is_empty() {
            if self.suggested.is_empty() {
                return SessionLinksRow::default();
            }
            return SessionLinksRow {
                line: Vec::new(),
                badge: Some(SessionLinksBadge {
                    count: self.suggested.len(),
                    tone: SessionLinkTone::Draft,
                    suggested: true,
                    label: self.suggested.len().to_string(),
                    more: 0,
                }),
                hover_card: true,
            };
        }
        // Pinned links lead `items` (gxserver sorts them first) and show under every option but
        // Off (CDXC:SessionLinks 2026-10-24 in server/src/session_links/store.rs).
        let all = || (0..self.items.len()).collect::<Vec<_>>();
        let pinned = || {
            self.items
                .iter()
                .enumerate()
                .filter(|(_, link)| link.pinned)
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        };
        let (line, badge) = match display {
            SessionLinksDisplay::Always => (all(), None),
            SessionLinksDisplay::Selected if selected => (all(), None),
            SessionLinksDisplay::Selected => (pinned(), None),
            // No line: the badge names the pinned (or most pressing) link and counts the rest.
            SessionLinksDisplay::Badge => (
                Vec::new(),
                self.badge_label().map(|(label, more)| SessionLinksBadge {
                    count: self.items.len(),
                    tone: self.most_pressing_tone(),
                    suggested: false,
                    label,
                    more,
                }),
            ),
            SessionLinksDisplay::OpenOnly => (
                self.items
                    .iter()
                    .enumerate()
                    .filter(|(_, link)| link.pinned || !link.finished)
                    .map(|(index, _)| index)
                    .collect(),
                None,
            ),
            SessionLinksDisplay::Off => unreachable!(),
        };
        SessionLinksRow {
            line,
            badge,
            hover_card: true,
        }
    }
}

/// How many of a links line's chips fit in `available`, in order: the most that fit together with
/// the "+N" pill (sized by `pill_width` for its own count) whenever any link is left out. Links the
/// line never shows (`never_shown`, the finished ones under Open work only) count into "+N" too.
/// Widths are the renderer's own measurements, in one unit.
///
/// CDXC:SessionLinks 2026-10-24 WHY:
/// The line once guessed the room from the sidebar's width and a fixed 34px pill, and its "+11" ran past the card's edge ("stuff is cut off here fam"). Every renderer now measures its chips, its pill and the line's real width, and drops chips until both fit.
pub fn link_chips_that_fit(
    chip_widths: &[f32],
    never_shown: usize,
    available: f32,
    gap: f32,
    pill_width: impl Fn(usize) -> f32,
) -> usize {
    let total = chip_widths.len();
    (0..=total)
        .rev()
        .find(|&shown| {
            let chips =
                chip_widths[..shown].iter().sum::<f32>() + gap * shown.saturating_sub(1) as f32;
            let hidden = total - shown + never_shown;
            let pill = if hidden == 0 {
                0.0
            } else {
                pill_width(hidden) + if shown > 0 { gap } else { 0.0 }
            };
            chips + pill <= available
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(key: &str, state: &str, finished: bool) -> SessionLink {
        SessionLink {
            key: key.to_string(),
            kind: SessionLinkKind::PullRequest,
            label: key.to_string(),
            title: None,
            state: Some(state.to_string()),
            state_name: None,
            checks: None,
            review: None,
            url: None,
            source: String::new(),
            source_label: String::new(),
            finished,
            pinned: false,
            loading: false,
            branch: None,
        }
    }

    fn links() -> SessionLinks {
        let mut failing = link("#1", "open", false);
        failing.checks = Some("failing".to_string());
        SessionLinks {
            items: vec![
                failing,
                link("#2", "open", false),
                link("#3", "merged", true),
            ],
            suggested: Vec::new(),
            removed: Vec::new(),
        }
    }

    #[test]
    fn the_setting_reads_every_option_and_defaults_to_badge() {
        use SessionLinksDisplay::*;
        for (value, display) in [
            ("always", Always),
            ("selected", Selected),
            ("badge", Badge),
            ("open", OpenOnly),
            ("off", Off),
            ("nonsense", Badge),
        ] {
            assert_eq!(SessionLinksDisplay::from_setting(Some(value)), display);
        }
        // A person who picked Always keeps it; only an unset setting is Badge.
        assert_eq!(SessionLinksDisplay::from_setting(Some("always")), Always);
        assert_eq!(SessionLinksDisplay::from_setting(None), Badge);
        assert_eq!(
            SessionLinksDisplay::from_settings_json(&serde_json::json!({})),
            Badge
        );
        assert_eq!(SessionLinksDisplay::default(), Badge);
    }

    #[test]
    fn suggested_opens_unfolded_with_two_or_fewer_linked_links() {
        let saved = vec![
            "finished".to_string(),
            "suggested".to_string(),
            "removed".to_string(),
        ];
        for linked in 0..=2 {
            let mut list = links();
            list.items.truncate(linked);
            assert!(list.suggested_opens_unfolded(), "{linked} linked");
            assert_eq!(
                list.folded_groups_on_open(&saved),
                vec!["finished".to_string(), "removed".to_string()],
                "{linked} linked"
            );
        }
        // Three or more: the saved fold holds, folded or not.
        let list = links();
        assert!(!list.suggested_opens_unfolded());
        assert_eq!(list.folded_groups_on_open(&saved), saved);
        assert_eq!(
            list.folded_groups_on_open(&["removed".to_string()]),
            vec!["removed".to_string()]
        );
    }

    #[test]
    fn a_removed_link_leaves_its_group_at_once_and_leads_removed() {
        let older = links().items[2].as_removed();
        let mut list = links();
        list.removed = vec![older];
        let after = list.without_removed(&["#2".to_string()]);
        assert_eq!(
            after
                .items
                .iter()
                .map(|link| link.key.as_str())
                .collect::<Vec<_>>(),
            vec!["#1", "#3"]
        );
        assert_eq!(
            after
                .removed
                .iter()
                .map(|link| link.key.as_str())
                .collect::<Vec<_>>(),
            vec!["#2", "#3"]
        );
        // Once gxserver has published the removal the key changes nothing.
        let published = SessionLinks {
            items: after.items.clone(),
            suggested: Vec::new(),
            removed: after.removed.clone(),
        };
        assert_eq!(published.without_removed(&["#2".to_string()]), published);
        assert_eq!(links().without_removed(&[]), links());
    }

    #[test]
    fn a_dismissed_suggestion_leaves_suggested_at_once_and_leads_removed() {
        let mut list = links();
        list.suggested = vec![links().items[0].as_removed(), links().items[1].as_removed()];
        list.items.clear();
        let after = list.without_removed(&["#2".to_string()]);
        let keys = |links: &[SuggestedSessionLink]| -> Vec<String> {
            links.iter().map(|link| link.key.clone()).collect()
        };
        assert_eq!(keys(&after.suggested), vec!["#1"]);
        assert_eq!(keys(&after.removed), vec!["#2"]);
    }

    #[test]
    fn the_chat_pill_reads_like_the_badge_and_says_suggested_quietly() {
        let pill = links().chat_pill().expect("a pill");
        assert_eq!(
            (pill.label.as_str(), pill.more, pill.tone, pill.suggested),
            ("#1", 2, SessionLinkTone::Closed, false)
        );

        let mut one = links();
        one.items.truncate(1);
        one.items[0].label = "SPX-123".to_string();
        let pill = one.chat_pill().expect("a pill");
        assert_eq!((pill.label.as_str(), pill.more), ("SPX-123", 0));

        let suggested = SessionLinks {
            items: Vec::new(),
            suggested: links()
                .without_removed(&["#1".to_string(), "#2".to_string()])
                .removed,
            removed: Vec::new(),
        };
        let pill = suggested.chat_pill().expect("a pill");
        assert_eq!(
            (pill.label.as_str(), pill.tone, pill.suggested),
            ("2 suggested", SessionLinkTone::Draft, true)
        );

        assert_eq!(SessionLinks::default().chat_pill(), None);
        let only_removed = SessionLinks {
            removed: suggested.suggested.clone(),
            ..SessionLinks::default()
        };
        assert_eq!(
            only_removed.chat_pill(),
            None,
            "removed links alone show no pill"
        );
    }

    #[test]
    fn always_shows_every_link_on_every_row() {
        let row = links().row(SessionLinksDisplay::Always, false);
        assert_eq!(row.line, vec![0, 1, 2]);
        assert_eq!(row.badge, None);
        assert!(row.hover_card);
    }

    #[test]
    fn selected_shows_the_line_only_on_the_selected_row() {
        assert_eq!(
            links().row(SessionLinksDisplay::Selected, true).line,
            vec![0, 1, 2]
        );
        let other = links().row(SessionLinksDisplay::Selected, false);
        assert!(other.line.is_empty() && other.badge.is_none() && other.hover_card);
    }

    #[test]
    fn badge_names_the_most_pressing_link_and_counts_the_rest() {
        let row = links().row(SessionLinksDisplay::Badge, false);
        assert!(row.line.is_empty());
        assert_eq!(
            row.badge,
            Some(SessionLinksBadge {
                count: 3,
                tone: SessionLinkTone::Closed,
                suggested: false,
                label: "#1".to_string(),
                more: 2,
            })
        );
    }

    #[test]
    fn badge_label_one_link_pinned_ties_and_finished() {
        // One link: its ID alone.
        let one = SessionLinks {
            items: vec![link("SPX-123", "open", false)],
            ..SessionLinks::default()
        };
        assert_eq!(one.badge_label(), Some(("SPX-123".to_string(), 0)));
        // A pinned link is named whatever else is more pressing.
        let mut pinned = links();
        pinned.items[2].pinned = true;
        assert_eq!(pinned.badge_label(), Some(("#3".to_string(), 2)));
        // Several open links of the same rank: the first in the list.
        let tie = SessionLinks {
            items: vec![link("#7", "open", false), link("#8", "open", false)],
            ..SessionLinks::default()
        };
        assert_eq!(tie.badge_label(), Some(("#7".to_string(), 1)));
        // A review asked for beats plain open work, wherever it sits.
        let mut review = link("#9", "open", false);
        review.review = Some("reviewRequired".to_string());
        let ranked = SessionLinks {
            items: vec![link("#7", "open", false), review],
            ..SessionLinks::default()
        };
        assert_eq!(ranked.badge_label(), Some(("#9".to_string(), 1)));
        // Every link finished: the first.
        let done = SessionLinks {
            items: vec![link("#3", "merged", true), link("#4", "merged", true)],
            ..SessionLinks::default()
        };
        assert_eq!(done.badge_label(), Some(("#3".to_string(), 1)));
        assert_eq!(SessionLinks::default().badge_label(), None);
    }

    #[test]
    fn badge_and_pill_drop_the_repo_name_but_the_label_keeps_it() {
        let mut pr = link("pr:acme/maestro_ed#151", "open", false);
        pr.label = "maestro_ed#151".to_string();
        let mut issue = link("issue:acme/rust#57563", "open", false);
        issue.kind = SessionLinkKind::Issue;
        issue.label = "rust#57563".to_string();
        let mut linear = link("linear:SPX-123", "open", false);
        linear.kind = SessionLinkKind::Linear;
        linear.label = "SPX-123".to_string();

        let one = SessionLinks {
            items: vec![pr.clone()],
            ..SessionLinks::default()
        };
        assert_eq!(one.badge_label(), Some(("#151".to_string(), 0)));
        assert_eq!(one.chat_pill().expect("a pill").label, "#151");

        let several = SessionLinks {
            items: vec![pr.clone(), issue.clone(), linear.clone()],
            ..SessionLinks::default()
        };
        assert_eq!(several.badge_label(), Some(("#151".to_string(), 2)));
        let badge = several
            .row(SessionLinksDisplay::Badge, false)
            .badge
            .expect("a badge");
        assert_eq!((badge.label.as_str(), badge.more), ("#151", 2));
        // The full label stays for the places with room.
        assert_eq!(several.items[0].label, "maestro_ed#151");

        let mut pinned_issue = issue;
        pinned_issue.pinned = true;
        let other = SessionLinks {
            items: vec![pinned_issue, pr],
            ..SessionLinks::default()
        };
        assert_eq!(other.badge_label(), Some(("#57563".to_string(), 1)));

        let ticket = SessionLinks {
            items: vec![linear],
            ..SessionLinks::default()
        };
        assert_eq!(ticket.badge_label(), Some(("SPX-123".to_string(), 0)));
    }

    #[test]
    fn a_ticket_of_an_unknown_source_draws_the_generic_ticket_icon() {
        let mut ticket = link("jira:ABC-12", "todo", false);
        ticket.kind = kind_of("jira");
        ticket.state_name = Some("Todo".to_string());
        assert_eq!(
            ticket.glyph(),
            (SessionLinkGlyph::Ticket, SessionLinkTone::Open)
        );
        assert_eq!(ticket.state_label().as_deref(), Some("Todo"));
        ticket.finished = true;
        assert_eq!(
            ticket.glyph(),
            (SessionLinkGlyph::Ticket, SessionLinkTone::Done)
        );
        ticket.finished = false;
        ticket.state = Some("unavailable".to_string());
        assert_eq!(
            ticket.glyph(),
            (SessionLinkGlyph::Ticket, SessionLinkTone::Draft)
        );
        assert_eq!(kind_of("linear"), SessionLinkKind::Linear);
    }

    #[test]
    fn the_badge_is_grey_once_every_link_is_merged_or_closed() {
        let mut closed_issue = link("#4", "closed", true);
        closed_issue.kind = SessionLinkKind::Issue;
        let finished = SessionLinks {
            items: vec![link("#3", "merged", true), closed_issue],
            suggested: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(finished.most_pressing_tone(), SessionLinkTone::Draft);
        assert_eq!(
            finished.row(SessionLinksDisplay::Badge, false).badge,
            Some(SessionLinksBadge {
                count: 2,
                tone: SessionLinkTone::Draft,
                suggested: false,
                label: "#3".to_string(),
                more: 1,
            })
        );
        // One open link beside the merged ones keeps the open colour.
        let one_open = SessionLinks {
            items: vec![link("#3", "merged", true), link("#2", "open", false)],
            suggested: Vec::new(),
            removed: Vec::new(),
        };
        assert_eq!(
            one_open
                .row(SessionLinksDisplay::Badge, false)
                .badge
                .map(|badge| badge.tone),
            Some(SessionLinkTone::Open)
        );
        // A pinned link is the badge's ID, with no line of its own; it still colours it.
        let mut pinned_open = link("#2", "open", false);
        pinned_open.pinned = true;
        let pinned = SessionLinks {
            items: vec![link("#3", "merged", true), pinned_open],
            suggested: Vec::new(),
            removed: Vec::new(),
        };
        let row = pinned.row(SessionLinksDisplay::Badge, false);
        assert!(row.line.is_empty());
        assert_eq!(
            row.badge.map(|badge| (badge.label, badge.more, badge.tone)),
            Some(("#2".to_string(), 1, SessionLinkTone::Open))
        );
    }

    #[test]
    fn open_only_hides_finished_links_and_the_line_once_all_are_done() {
        assert_eq!(
            links().row(SessionLinksDisplay::OpenOnly, false).line,
            vec![0, 1]
        );
        let done = SessionLinks {
            items: vec![link("#3", "merged", true)],
            suggested: Vec::new(),
            removed: Vec::new(),
        };
        let row = done.row(SessionLinksDisplay::OpenOnly, false);
        assert!(row.line.is_empty() && row.hover_card);
    }

    #[test]
    fn off_shows_nothing_and_opens_no_card() {
        assert_eq!(
            links().row(SessionLinksDisplay::Off, true),
            SessionLinksRow::default()
        );
    }

    #[test]
    fn chips_fit_with_room_for_their_pill() {
        // "+N" grows by a digit's width per digit.
        let pill = |hidden: usize| 20.0 + hidden.to_string().len() as f32 * 6.0;
        let chips = [40.0; 5];
        // Everything fits: no pill.
        assert_eq!(link_chips_that_fit(&chips, 0, 240.0, 10.0, pill), 5);
        // Five chips need 240; at 239 the fifth gives way to "+1" (four chips 190 + 10 + 26).
        assert_eq!(link_chips_that_fit(&chips, 0, 239.0, 10.0, pill), 4);
        // The pill's room is reserved: four chips alone fit 190, but not with "+1".
        assert_eq!(link_chips_that_fit(&chips, 0, 225.0, 10.0, pill), 3);
        // Links the line never shows go into "+N" even when every chip fits.
        assert_eq!(link_chips_that_fit(&chips, 2, 240.0, 10.0, pill), 4);
        // A pill sized for "+11" needs more room than one sized for "+1".
        assert_eq!(link_chips_that_fit(&chips, 10, 230.0, 10.0, pill), 3);
        // Nothing fits but the pill.
        assert_eq!(link_chips_that_fit(&chips, 0, 30.0, 10.0, pill), 0);
    }
}
