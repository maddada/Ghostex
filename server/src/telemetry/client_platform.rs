/*
CDXC:Telemetry 2026-09-03:
Coarse OS family for a WEB client, read off the browser's User-Agent at
`/api/webBootstrap`. Only the family is kept: the UA string itself carries
browser build numbers and device models that are close to a fingerprint, so it
never leaves this function, and an unrecognised UA becomes `other` rather than
being sent as text. iPad Safari in "desktop website" mode reports itself as a
Mac and is counted as one; that imprecision is accepted rather than sniffing
touch hints. Order matters below: Android UAs also contain "Linux", and iOS
UAs contain "like Mac OS X".
*/

use super::taxonomy;

pub fn platform_from_user_agent(user_agent: &str) -> &'static str {
    let ua = user_agent.to_ascii_lowercase();
    let family = if ua.contains("android") {
        "android"
    } else if ua.contains("iphone") || ua.contains("ipad") || ua.contains("ipod") {
        "ios"
    } else if ua.contains("cros ") {
        "chromeos"
    } else if ua.contains("windows") {
        "windows"
    } else if ua.contains("mac os x") || ua.contains("macintosh") {
        "macos"
    } else if ua.contains("linux") || ua.contains("x11") {
        "linux"
    } else {
        "other"
    };
    /*
    Resolved through the table so the value handed to the validator is the
    table's own `&'static str`, and a typo above is a dropped event in debug
    logs rather than an unknown member reaching PostHog.
    */
    taxonomy::match_enum(taxonomy::CLIENT_PLATFORMS, family).unwrap_or("other")
}
