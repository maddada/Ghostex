//! What the host tells the core about the world outside it, once per turn.
//!
//! The core reads no clock and knows no timezone, so both arrive here. The same values therefore
//! always produce the same document, including the "Today" and "Yesterday" boundaries the
//! transcript computes against local midnight.

use serde::{Deserialize, Serialize};

/// Which locale-formatted rendering of a stamp a [`FormattedTime`] carries.
///
/// One variant per place the brain calls a locale formatter. There are exactly two today, both
/// `new Date(stamp).toLocaleString()` with V8's default arguments, and the brain has no `Intl.*`
/// call anywhere else (`docs/2026-09-21/rust-chat/SEAM.md` section 7.5).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormattedTimeStyle {
    /// `new Date(startedAt).toLocaleString()`, the Codex context panel's "Started" row.
    ContextStartedAt,
    /// `new Date(stamp).toLocaleString()`, the account panel's recovery line.
    AccountDateTime,
}

/// One stamp the host has already rendered in the user's own locale.
///
/// The core cannot read a locale any more than it can read a clock, so a host that wants the
/// user's format rather than the crate's `en-US` fallback passes the rendering in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormattedTime {
    /// Which rendering this is.
    pub style: FormattedTimeStyle,
    /// The epoch milliseconds that were formatted.
    pub stamp_ms: i64,
    /// What the host's locale printed.
    pub text: String,
}

/// The host's clock and locale for this turn.
///
/// Not `Copy`: [`ChatContext::formatted_times`] owns its strings. Pass it by reference inside the
/// crate and build it with [`ChatContext::at`] plus the `with_*` setters outside it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContext {
    /// Epoch milliseconds, the same number `Date.now()` returns.
    ///
    /// Every deadline, elapsed label, retry backoff and settle hold is measured against it.
    pub now_ms: f64,
    /// Minutes to add to UTC to get the user's local time, the negation of
    /// `Date.prototype.getTimezoneOffset()`.
    ///
    /// `crate::transcript::message_time` (ported from
    /// `packages/shared/session-chat-presentation/message-time.ts`) groups rows by local midnight,
    /// which is the only timezone-dependent rule in the brain.
    pub utc_offset_minutes: i32,
    /// The host's uniform random draws for this turn, in `[0, 1)`, consumed in order.
    ///
    /// The core generates no random values. The one rule that needs them is the working strip's
    /// stint word (`pickSessionChatWorkingWord`), and it can draw twice in a single turn: the
    /// `useState` initializer, then the `useEffect` that immediately replaces it when the first
    /// computation already sees a working session. Two slots is therefore the whole supply.
    pub random_units: [f64; 2],
    /// The host's fresh random ids for this turn, consumed in order.
    ///
    /// The core mints no identities either. Two rules need one: the model-selection intent
    /// (`crypto.randomUUID()` in `model-selection.ts`) and the model picker's request id. A host
    /// passes two draws of 128 random bits and [`ChatContext::random_id`] lays them out as a
    /// version 4 UUID.
    ///
    /// Numbers rather than strings so this type stays UniFFI friendly.
    pub random_ids: [u128; 2],
    /// Stamps the host has already rendered in the user's locale, for this turn.
    ///
    /// Empty is the normal case and means "use the crate's own `en-US` rendering", which is what
    /// the TypeScript brain printed under V8's default. A desktop
    /// or mobile host that wants the user's real locale fills the entries it knows the stamps for;
    /// anything it leaves out falls back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formatted_times: Vec<FormattedTime>,
    /// Every clock value the brain read during this turn, in order, when the host knows them.
    ///
    /// The TypeScript read `Date.now()` more than once inside one call, and some of those reads
    /// were LATCHED: the stall watchdog's `now`, a `setNow(Date.now())` inside an interval's
    /// callback, the `useState(Date.now)` of the first render. They are not the first read of the
    /// call, so a core that measures everything against [`ChatContext::now_ms`] lands a
    /// millisecond early on a fraction of turns, and a latched millisecond is republished for the
    /// rest of the session. The replay that checked the port passed the recording's `c` queue here
    /// (index 0 is `now_ms` itself) and the core takes the k-th read where the TypeScript took it
    /// ([`crate::state::CoreState::read_clock`]). Empty for a live host, whose reads all answer
    /// `now_ms`: nothing here changes behaviour, only the millisecond.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clock_reads: Vec<f64>,
}

impl ChatContext {
    /// A context at `now_ms` in UTC, for a host that has not wired its offset yet.
    ///
    /// This is the constructor every caller should start from. Build the rest with the `with_*`
    /// setters rather than an exhaustive struct literal: a field added here then costs the host
    /// nothing, where a literal stops compiling. See "API changes for the host" in
    /// `docs/2026-09-21/rust-chat/PROGRESS.md`.
    pub fn at(now_ms: f64) -> Self {
        Self {
            now_ms,
            utc_offset_minutes: 0,
            random_units: [0.0; 2],
            random_ids: [0; 2],
            formatted_times: Vec::new(),
            clock_reads: Vec::new(),
        }
    }

    /// The same context with every clock read of this turn, in order, `now_ms` first.
    #[must_use]
    pub fn with_clock_reads(mut self, reads: Vec<f64>) -> Self {
        self.clock_reads = reads;
        self
    }

    /// The `index`-th clock read of this turn.
    ///
    /// Past the end of what the host recorded, the LAST read: it is the latest clock the turn
    /// saw, which is what a read the TypeScript made after it would have returned. With no reads
    /// at all (a live host), `now_ms`.
    pub fn clock_read(&self, index: usize) -> f64 {
        self.clock_reads
            .get(index)
            .or_else(|| self.clock_reads.last())
            .copied()
            .unwrap_or(self.now_ms)
    }

    /// The same context with the host's offset east of UTC, in minutes.
    #[must_use]
    pub fn with_utc_offset_minutes(mut self, minutes: i32) -> Self {
        self.utc_offset_minutes = minutes;
        self
    }

    /// The same context with this turn's two uniform draws in `[0, 1)`.
    #[must_use]
    pub fn with_random_units(mut self, units: [f64; 2]) -> Self {
        self.random_units = units;
        self
    }

    /// The same context with this turn's two 128-bit identity draws.
    #[must_use]
    pub fn with_random_ids(mut self, ids: [u128; 2]) -> Self {
        self.random_ids = ids;
        self
    }

    /// The same context with the host's locale-formatted times for this turn.
    #[must_use]
    pub fn with_formatted_times(mut self, times: Vec<FormattedTime>) -> Self {
        self.formatted_times = times;
        self
    }

    /// The host's rendering of `stamp_ms` in `style`, or `None` when the host supplied none.
    ///
    /// A caller that gets `None` falls back to the crate's own `en-US` formatter (V8's default
    /// under Bun), which is what a host that has not wired its locale gets.
    pub fn formatted_time(&self, style: FormattedTimeStyle, stamp_ms: i64) -> Option<&str> {
        self.formatted_times
            .iter()
            .find(|entry| entry.style == style && entry.stamp_ms == stamp_ms)
            .map(|entry| entry.text.as_str())
    }

    /// One of the turn's random ids, as the canonical lowercase UUID text.
    ///
    /// The version and variant bits are forced the way `crypto.randomUUID()` sets them, so a host
    /// may pass raw entropy and still get a valid version 4 UUID, and a UUID parsed back to a
    /// number prints as itself.
    pub fn random_id(&self, slot: usize) -> String {
        let bits = self.random_ids.get(slot).copied().unwrap_or_default();
        let bytes = bits.to_be_bytes();
        let mut out = String::with_capacity(36);
        for (index, byte) in bytes.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                out.push('-');
            }
            let byte = match index {
                6 => (byte & 0x0f) | 0x40,
                8 => (byte & 0x3f) | 0x80,
                _ => *byte,
            };
            out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
            out.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
        }
        out
    }

    /// `now_ms` as the integer milliseconds every stored timestamp is compared against.
    ///
    /// Rounded rather than truncated so a host that passes a fractional clock cannot drift a
    /// comparison by a millisecond against one that passes whole numbers.
    pub fn now_millis(&self) -> i64 {
        self.now_ms.round() as i64
    }
}
