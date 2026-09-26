//! `Date.parse` for the ISO-8601 stamps the chat frames carry, without a date crate.
//!
//! The fleet roster, the terminal activity and the subagent clocks all anchor on an ISO-8601
//! `detectedAt`, and the TypeScript read it with `Date.parse`. The core has no clock and no
//! timezone, so the one form `Date.parse` resolves against local time (a date-time with no offset)
//! is resolved against [`crate::ChatContext::utc_offset_minutes`] instead.

/// `Date.parse`, which is `crate::jstime` for every family since 2026-09-22.
pub use crate::jstime::parse_iso_millis;
