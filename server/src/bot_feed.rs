//! The Bot automations feed: one channel per Hermes cron job across every profile, and each run
//! of that job as a message. Read-only; `/api/listBotFeed` (server/route_http/projects.rs) answers with
//! [`read_bot_feed`].
//!
//! CDXC:Bots 2026-09-27 DECISION:
//! User: the feed shows one `#channel` per cron job across every profile, each run as a Markdown message, and hides `[SILENT]` runs so it shows only what Discord would have delivered.
//! A run Hermes marked failed shows "failed": an answer opening with `[CRON_FAILURE]`, a script run whose status says it failed, and a run that crashed; every other run shows no status badge. Widens the 2026-09-26 `[CRON_FAILURE]`-only badge (Sven, 2026-09-27).
//!
//! CDXC:Bots 2026-09-27 WHY:
//! Hermes writes three shapes of run file (`cron/scheduler.py`), and each is read the way Hermes reads it back.
//! An agent run ends in `## Response`, and its answer is everything after the LAST such heading (Hermes' own `_archive_answer`), because the prompt above it can quote the heading; it failed when `[CRON_FAILURE]` stands alone on the answer's first line.
//! A script or monitor run has a `**Mode:**` header and no Response heading: a `**Status:**` header marks it silent or failed, and otherwise its output follows a `---` line.
//! A run that crashed before answering ends in `## Error` instead; Hermes delivered those, and scripts whose status says they failed, to the failure channel.
//! An answer is silent by Hermes' own delivery rule (`is_autonomous_silence_response` in `gateway/response_filters.py`), not only when it is exactly `[SILENT]`: models add a note or drop the brackets, and those runs never reached Discord either.
//!
//! SEE-ALSO: apps/desktop/src/app/native_bot_feed/ (the view that draws this), server/src/bot_projects.rs (the profiles).

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::agents::helpers::read_text_value;
use crate::bot_projects::{bot_profile_home, discover_bot_profiles, BotProfile, PublishedBotFacts};
use crate::paths::GxserverPaths;
use crate::session_chat_hermes::is_safe_hermes_session_id;

/// The newest visible runs each channel keeps. Hermes prunes a job's output to its newest 50 files
/// unless `cron.output_retention` turns that off, which would otherwise grow the answer forever.
const MAX_RUNS_PER_CHANNEL: usize = 100;

/// Hermes' `LIVE_GATEWAY_SILENT_MARKERS`, the translated forms included.
const SILENCE_MARKERS: [&str; 8] = [
    "[SILENT]", "SILENT", "NO_REPLY", "NO REPLY", "[静默]", "静默", "[沉默]", "沉默",
];
/// Hermes' `_MARKER_LENGTH_CAP`: longer text is never a marker, whatever its punctuation.
const MARKER_LENGTH_CAP: usize = 64;
const CRON_FAILURE_MARKER: &str = "[CRON_FAILURE]";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedChannel {
    job_id: String,
    name: String,
    profile: String,
    bot_name: String,
    schedule: Option<String>,
    /// Oldest first, so the newest run is drawn at the bottom.
    runs: Vec<FeedRun>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedRun {
    /// Hermes' own `Run Time` header, in the Hermes host's local time (`YYYY-MM-DD HH:MM:SS`).
    run_at: String,
    body: String,
    failed: bool,
}

/// Bots and Bot automations are both switched on. `botAutomationsHidden` is an inverted key that
/// defaults to hidden, like `botsHidden`.
pub(crate) fn bot_automations_enabled(paths: &GxserverPaths) -> bool {
    crate::session_lifecycle::read_sidebar_settings(paths).is_some_and(|settings| {
        ["botsHidden", "botAutomationsHidden"]
            .iter()
            .all(|key| settings.get(*key).and_then(Value::as_bool) == Some(false))
    })
}

/// The feed a client may read: nothing unless Bot automations is on, since the runs hold whole
/// agent reports.
pub(crate) fn read_bot_feed(paths: &GxserverPaths, hermes_home: &Path) -> Value {
    if bot_automations_enabled(paths) {
        list_bot_feed(hermes_home)
    } else {
        json!({ "channels": [] })
    }
}

/// Every profile's cron jobs that delivered a run, as channels, newest activity first.
///
/// CDXC:Bots 2026-09-27 DECISION:
/// User (the feed tracer's try-it): hide a channel with no delivered run, such as a paused job, a job that never ran, or a job whose every run was silent.
pub(crate) fn list_bot_feed(hermes_home: &Path) -> Value {
    let mut channels: Vec<FeedChannel> = discover_bot_profiles(hermes_home)
        .iter()
        .flat_map(profile_channels)
        .filter(|channel| !channel.runs.is_empty())
        .collect();
    // Newest activity first: the time of each channel's newest visible run.
    channels.sort_by(|left, right| {
        let last_run = |channel: &FeedChannel| channel.runs.last().map(|run| run.run_at.clone());
        last_run(right)
            .cmp(&last_run(left))
            .then_with(|| left.name.cmp(&right.name))
    });
    json!({ "channels": channels })
}

fn profile_channels(bot: &BotProfile) -> Vec<FeedChannel> {
    let cron = bot.path.join("cron");
    cron_jobs(&cron)
        .into_iter()
        .map(|(job_id, job)| {
            let runs = job_runs(&cron.join("output").join(&job_id));
            FeedChannel {
                name: read_text_value(&job, "name").unwrap_or_else(|| job_id.clone()),
                job_id,
                profile: bot.profile.clone(),
                bot_name: bot.name.clone(),
                schedule: read_text_value(&job, "schedule_display").or_else(|| {
                    job.get("schedule")
                        .and_then(|schedule| read_text_value(schedule, "display"))
                }),
                runs,
            }
        })
        .collect()
}

/// A profile's cron jobs (`cron/jobs.json`) with their ids. The id names the job's output folder,
/// so a job whose id is not one plain path segment is left out.
fn cron_jobs(cron: &Path) -> Vec<(String, Value)> {
    let Some(mut jobs) = fs::read_to_string(cron.join("jobs.json"))
        .ok()
        .and_then(|jobs| serde_json::from_str::<Value>(&jobs).ok())
    else {
        return Vec::new();
    };
    let Some(Value::Array(jobs)) = jobs.get_mut("jobs").map(Value::take) else {
        return Vec::new();
    };
    jobs.into_iter()
        .filter_map(|job| {
            let job_id = read_text_value(&job, "id").filter(|id| is_safe_hermes_session_id(id))?;
            Some((job_id, job))
        })
        .collect()
}

/// The runs a bot delivered since local midnight, which the Automations row adds up. Hermes names
/// each output file after its local run time, so only today's files are read.
pub(crate) fn bot_runs_today(hermes_home: &Path, profile: &str) -> u64 {
    let today = chrono::Local::now().format("%Y-%m-%d_").to_string();
    let cron = bot_profile_home(hermes_home, profile).join("cron");
    let delivered = cron_jobs(&cron)
        .into_iter()
        .flat_map(|(job_id, _)| fs::read_dir(cron.join("output").join(job_id)).into_iter())
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&today) && name.ends_with(".md"))
        })
        .filter(|entry| {
            fs::read_to_string(entry.path())
                .ok()
                .and_then(|text| parse_run(&text))
                .is_some()
        })
        .count();
    delivered as u64
}

static PUBLISHED_RUNS_TODAY: PublishedBotFacts<u64> = PublishedBotFacts::new();

/// A bot's runs today, as last counted; the 60s pass recounts them only while Bot automations is
/// on.
pub(crate) fn published_bot_runs_today(profile: &str) -> u64 {
    PUBLISHED_RUNS_TODAY.get(profile, || {
        bot_runs_today(&crate::session_chat_hermes::hermes_home(), profile)
    })
}

/// Counts each profile's runs again and returns the profiles whose count changed since it was
/// last published, which is also how the count drops back at midnight.
pub(crate) fn refresh_published_bot_runs_today<'a>(
    hermes_home: &Path,
    profiles: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    PUBLISHED_RUNS_TODAY.refresh(
        profiles
            .into_iter()
            .map(|profile| (profile, bot_runs_today(hermes_home, profile))),
    )
}

/// The newest visible runs in a job's output folder, oldest first. Hermes names each file after
/// its run time (`YYYY-MM-DD_HH-MM-SS.md`), so the name order is the run order.
fn job_runs(output: &Path) -> Vec<FeedRun> {
    let mut files: Vec<_> = fs::read_dir(output)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort_unstable_by(|left, right| right.cmp(left));
    let mut runs: Vec<FeedRun> = files
        .iter()
        .filter_map(|path| parse_run(&fs::read_to_string(path).ok()?))
        .take(MAX_RUNS_PER_CHANNEL)
        .collect();
    runs.reverse();
    runs
}

/// One run file as a message, or `None` for a silent run (and for a file in no shape Hermes writes).
fn parse_run(text: &str) -> Option<FeedRun> {
    let (fields, rest) = split_header(text);
    let field = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    };
    let run_at = field("Run Time")?.to_string();
    let (body, failed) = if field("Mode").is_some() {
        let status = field("Status").unwrap_or_default();
        if status.starts_with("silent") || status.starts_with("no_change") {
            return None;
        }
        let output = rest.trim();
        let output = output.strip_prefix("---").map_or(output, str::trim_start);
        (output.to_string(), status.contains("failed"))
    } else if let Some((_, answer)) = rest.rsplit_once("## Response") {
        let answer = answer.trim();
        // Hermes counts only the marker alone on the first line, so a report quoting it stays healthy.
        let (first, evidence) = answer.split_once('\n').unwrap_or((answer, ""));
        if first.trim_end() == CRON_FAILURE_MARKER {
            (evidence.trim().to_string(), true)
        } else if is_silence(answer) {
            return None;
        } else {
            (answer.to_string(), false)
        }
    } else {
        let (_, error) = rest.rsplit_once("## Error")?;
        (error.trim().to_string(), true)
    };
    (failed || !body.is_empty()).then_some(FeedRun {
        run_at,
        body,
        failed,
    })
}

/// Hermes' `is_autonomous_silence_response`: a bracketed marker opening the answer, or a marker
/// as the whole answer or as its own first or last line. A marker inside a sentence is content.
fn is_silence(answer: &str) -> bool {
    if SILENCE_MARKERS.iter().any(|marker| {
        marker.starts_with('[')
            && answer
                .get(..marker.len())
                .is_some_and(|head| head.to_uppercase() == *marker)
    }) {
        return true;
    }
    let mut lines = answer.lines().filter(|line| !line.trim().is_empty());
    let (first, last) = (lines.next(), lines.next_back());
    [Some(answer), first, last]
        .into_iter()
        .flatten()
        .any(is_marker)
}

/// Hermes' `is_intentional_silence_response`: exactly a marker, ignoring case, runs of spaces and
/// stray punctuation at its edges.
fn is_marker(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.chars().nth(MARKER_LENGTH_CAP).is_some() {
        return false;
    }
    // No marker has punctuation at its edges, so the stripped form is the only one that can match.
    let bare = text.trim_matches(is_edge_punctuation);
    let canonical = bare
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase();
    SILENCE_MARKERS.contains(&canonical.as_str())
}

/// Punctuation Hermes strips from a marker's edges (Unicode category P), except the square
/// brackets, which stay part of the marker. Exact for ASCII, where `$+<=>^` and the backtick, bar
/// and tilde are symbols Hermes keeps, and for the Latin-1, General Punctuation, CJK and fullwidth
/// blocks a model's punctuation around a marker comes from; the rest of the Unicode table is left
/// out.
fn is_edge_punctuation(character: char) -> bool {
    match character {
        _ if character.is_ascii() => "!\"#%&'()*,-./:;?@\\_{}".contains(character),
        '\u{a1}' | '\u{a7}' | '\u{ab}' | '\u{b6}' | '\u{b7}' | '\u{bb}' | '\u{bf}' => true,
        '\u{2010}'..='\u{2027}'
        | '\u{2030}'..='\u{2043}'
        | '\u{2045}'..='\u{2051}'
        | '\u{2053}'..='\u{205e}' => true,
        '\u{3001}'..='\u{3003}'
        | '\u{3008}'..='\u{3011}'
        | '\u{3014}'..='\u{301f}'
        | '\u{3030}'
        | '\u{303d}' => true,
        '\u{ff01}'..='\u{ff03}'
        | '\u{ff05}'..='\u{ff0a}'
        | '\u{ff0c}'..='\u{ff0f}'
        | '\u{ff1a}'..='\u{ff1b}'
        | '\u{ff1f}'..='\u{ff20}'
        | '\u{ff3b}'..='\u{ff3d}'
        | '\u{ff3f}'
        | '\u{ff5b}'
        | '\u{ff5d}'
        | '\u{ff5f}'..='\u{ff65}' => true,
        _ => false,
    }
}

/// The `**Key:** value` lines under the `# Cron Job:` title, and the text after them.
fn split_header(text: &str) -> (Vec<(&str, &str)>, &str) {
    let mut fields = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let field = trimmed
            .strip_prefix("**")
            .and_then(|inner| inner.split_once(":**"))
            .map(|(key, value)| (key.trim(), value.trim()));
        match field {
            Some(field) => fields.push(field),
            None if fields.is_empty()
                && (trimmed.is_empty() || trimmed.starts_with("# Cron Job:")) => {}
            None => break,
        }
        offset += line.len();
    }
    (fields, &text[offset..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn write(path: PathBuf, text: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, text).expect("write");
    }

    fn agent_run(job: &str, run_time: &str, prompt: &str, response: &str) -> String {
        format!(
            "# Cron Job: {job}\n\n**Job ID:** x\n**Run Time:** {run_time}\n**Schedule:** 0 8 * * *\n\n## Prompt\n\n{prompt}\n\n## Response\n\n{response}\n"
        )
    }

    fn script_run(run_time: &str, tail: &str) -> String {
        format!("# Cron Job: Quota watchdog\n\n**Job ID:** ccc333\n**Run Time:** {run_time}\n**Mode:** no_agent (script)\n{tail}")
    }

    /// A Hermes home with a default profile, Harry with three jobs, and Dobby with no cron folder.
    fn hermes_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().expect("home");
        let root = home.path();
        write(
            root.join("cron/jobs.json"),
            &json!({"jobs": [{"id": "aaa111", "name": "Morning briefing", "schedule_display": "0 8 * * *"}]})
                .to_string(),
        );
        let briefing = root.join("cron/output/aaa111");
        write(
            briefing.join("2026-09-25_08-00-00.md"),
            &agent_run(
                "Morning briefing",
                "2026-09-25 08:00:00",
                "Brief Sven.",
                "Good morning **Sven**.",
            ),
        );
        write(
            briefing.join("2026-09-26_08-00-00.md"),
            &agent_run(
                "Morning briefing",
                "2026-09-26 08:00:00",
                "Brief Sven.",
                "[SILENT]",
            ),
        );

        let harry = root.join("profiles/harry/cron");
        write(
            harry.join("jobs.json"),
            &json!({"jobs": [
                {"id": "bbb222", "name": "Review 510 watcher", "schedule": {"kind": "cron", "display": "*/20 * * * *"}},
                {"id": "ccc333", "name": "Quota watchdog", "schedule_display": "*/30 * * * *"},
                {"id": "ddd444", "name": "Nightly backup", "schedule_display": "0 4 * * *"},
                {"id": "eee555", "name": "Inbox watcher", "schedule_display": "0 * * * *"},
                {"id": "fff666", "name": "Always quiet", "schedule_display": "0 * * * *"},
                {"id": "ggg777", "name": "Paused job", "schedule_display": "0 9 * * 1"},
                {"id": "../escape", "name": "Not a job id"}
            ], "updated_at": "2026-09-27"})
            .to_string(),
        );
        write(
            harry.join("output/bbb222/2026-09-26_01-21-00.md"),
            &agent_run(
                "Review 510 watcher",
                "2026-09-26 01:21:00",
                "Check PR 510.",
                "[CRON_FAILURE]\nMac preview build failed at `codesign`.",
            ),
        );
        write(
            harry.join("output/bbb222/2026-09-26_01-52-00.md"),
            &agent_run(
                "Review 510 watcher",
                "2026-09-26 01:52:00",
                "Answer in this shape:\n\n## Response\n\nA one-line status.",
                "CI green, 2 review comments open.\n\n## Details\n\n- one nit on naming",
            ),
        );
        write(
            harry.join("output/ccc333/2026-09-27_10-00-00.md"),
            &script_run("2026-09-27 10:00:00", "**Status:** silent (empty output)\n"),
        );
        write(
            harry.join("output/ccc333/2026-09-27_10-30-00.md"),
            &script_run(
                "2026-09-27 10:30:00",
                "\n---\n\n🛡 **Account Watch**\n**Status:** ✅ green\n",
            ),
        );
        write(
            harry.join("output/ddd444/2026-09-24_04-00-00.md"),
            &script_run(
                "2026-09-24 04:00:00",
                "**Status:** script failed\n\nScript exited with code 128\n",
            ),
        );
        write(
            harry.join("output/ddd444/2026-09-25_04-00-00.md"),
            "# Cron Job: Nightly backup (FAILED)\n\n**Job ID:** ddd444\n**Run Time:** 2026-09-25 04:00:00\n**Schedule:** 0 4 * * *\n\n## Prompt\n\nBack up.\n\n## Error\n\n```\nRuntimeError: boom\n```\n",
        );
        // Every shape Hermes' silence rule holds back, and one report that only quotes the marker.
        for (minute, response) in [
            (1, "Nothing new in the inbox.\n\n[SILENT]"),
            (2, "[SILENT]\nNo changes since the last run."),
            (3, "silent"),
            (4, "[SILENT] No changes detected"),
            (5, "*NO_REPLY*."),
            (
                6,
                "The watcher stayed [SILENT] all night, then CI went green.",
            ),
            (7, "（静默）"),
        ] {
            write(
                harry.join(format!("output/eee555/2026-09-23_12-0{minute}-00.md")),
                &agent_run(
                    "Inbox watcher",
                    &format!("2026-09-23 12:0{minute}:00"),
                    "Watch the inbox.",
                    response,
                ),
            );
        }
        write(
            harry.join("output/fff666/2026-09-26_09-00-00.md"),
            &agent_run(
                "Always quiet",
                "2026-09-26 09:00:00",
                "Report changes.",
                "[SILENT]",
            ),
        );
        fs::create_dir_all(root.join("profiles/dobby")).expect("dobby");
        home
    }

    fn channel_ids(feed: &Value) -> Vec<&str> {
        feed["channels"]
            .as_array()
            .expect("channels")
            .iter()
            .filter_map(|channel| channel["jobId"].as_str())
            .collect()
    }

    fn channel<'a>(feed: &'a Value, job_id: &str) -> &'a Value {
        feed["channels"]
            .as_array()
            .expect("channels")
            .iter()
            .find(|channel| channel["jobId"] == job_id)
            .unwrap_or_else(|| panic!("no channel {job_id}"))
    }

    #[test]
    fn channels_come_from_every_profiles_jobs() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        let ids = channel_ids(&feed);
        assert_eq!(ids.len(), 5, "{ids:?}");
        let briefing = channel(&feed, "aaa111");
        assert_eq!(briefing["profile"], "default");
        assert_eq!(briefing["botName"], "Hermes");
        assert_eq!(briefing["name"], "Morning briefing");
        assert_eq!(briefing["schedule"], "0 8 * * *");
        let watcher = channel(&feed, "bbb222");
        assert_eq!(watcher["profile"], "harry");
        assert_eq!(watcher["botName"], "Harry");
        assert_eq!(watcher["schedule"], "*/20 * * * *");
    }

    #[test]
    fn channels_sort_by_newest_visible_run() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        let ids = channel_ids(&feed);
        // The briefing's silent run on the 26th at 08:00 is not activity, or it would beat 01:52.
        assert_eq!(ids, ["ccc333", "bbb222", "aaa111", "ddd444", "eee555"]);
    }

    #[test]
    fn runs_are_parsed_oldest_first_and_silent_runs_are_dropped() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        assert_eq!(
            channel(&feed, "aaa111")["runs"],
            json!([{"runAt": "2026-09-25 08:00:00", "body": "Good morning **Sven**.", "failed": false}])
        );
        let watcher = &channel(&feed, "bbb222")["runs"];
        assert_eq!(watcher.as_array().map(Vec::len), Some(2));
        assert_eq!(
            watcher[1]["body"],
            "CI green, 2 review comments open.\n\n## Details\n\n- one nit on naming"
        );
        assert_eq!(
            channel(&feed, "ccc333")["runs"],
            json!([{"runAt": "2026-09-27 10:30:00", "body": "🛡 **Account Watch**\n**Status:** ✅ green", "failed": false}])
        );
    }

    #[test]
    fn a_cron_failure_response_is_a_failed_run() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        let watcher = &channel(&feed, "bbb222")["runs"];
        assert_eq!(watcher[0]["failed"], true);
        assert_eq!(
            watcher[0]["body"],
            "Mac preview build failed at `codesign`."
        );
        assert_eq!(watcher[1]["failed"], false);
    }

    #[test]
    fn runs_hermes_held_back_as_silent_are_dropped() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        assert_eq!(
            channel(&feed, "eee555")["runs"],
            json!([{"runAt": "2026-09-23 12:06:00", "body": "The watcher stayed [SILENT] all night, then CI went green.", "failed": false}])
        );
    }

    #[test]
    fn channels_with_no_delivered_run_are_hidden() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        let ids = channel_ids(&feed);
        assert!(!ids.contains(&"fff666"), "every run was silent: {ids:?}");
        assert!(!ids.contains(&"ggg777"), "never ran: {ids:?}");
    }

    #[test]
    fn a_failed_script_and_a_crashed_run_are_failed_runs() {
        let home = hermes_home();
        let feed = list_bot_feed(home.path());
        assert_eq!(
            channel(&feed, "ddd444")["runs"],
            json!([
                {"runAt": "2026-09-24 04:00:00", "body": "Script exited with code 128", "failed": true},
                {"runAt": "2026-09-25 04:00:00", "body": "```\nRuntimeError: boom\n```", "failed": true}
            ])
        );
    }

    #[test]
    fn the_feed_is_empty_until_bots_and_bot_automations_are_both_on() {
        let home = hermes_home();
        let storage = tempfile::tempdir().expect("storage");
        let paths = crate::paths::get_gxserver_paths(Some(storage.path().to_path_buf()));
        fs::create_dir_all(&paths.app_config_dir).expect("config dir");
        let settings = paths.app_config_dir.join("native-sidebar-settings.json");
        let channel_count = || {
            read_bot_feed(&paths, home.path())["channels"]
                .as_array()
                .map(Vec::len)
        };
        assert_eq!(channel_count(), Some(0), "both switches default to hidden");
        fs::write(&settings, r#"{"botAutomationsHidden":false}"#).expect("settings");
        assert_eq!(channel_count(), Some(0), "Bot automations needs Bots");
        fs::write(
            &settings,
            r#"{"botsHidden":false,"botAutomationsHidden":false}"#,
        )
        .expect("settings");
        assert_eq!(channel_count(), Some(5));
    }

    #[test]
    fn runs_today_counts_only_todays_delivered_runs_of_listed_jobs() {
        let home = tempfile::tempdir().expect("home");
        let cron = home.path().join("profiles/harry/cron");
        write(
            cron.join("jobs.json"),
            &json!({"jobs": [{"id": "aaa111", "name": "Watcher"}]}).to_string(),
        );
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let yesterday = (now - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        for (file, response) in [
            (format!("{today}_08-00-00.md"), "Delivered."),
            (format!("{today}_09-00-00.md"), "[SILENT]"),
            (format!("{yesterday}_23-00-00.md"), "Delivered yesterday."),
        ] {
            write(
                cron.join("output/aaa111").join(&file),
                &agent_run("Watcher", &file.replace('_', " "), "Watch.", response),
            );
        }
        write(
            cron.join(format!("output/gone999/{today}_08-00-00.md")),
            &agent_run("Deleted job", "x", "Watch.", "Not listed any more."),
        );
        assert_eq!(bot_runs_today(home.path(), "harry"), 1);
        assert_eq!(bot_runs_today(home.path(), "dobby"), 0);
    }

    #[test]
    fn no_hermes_home_is_an_empty_feed() {
        let home = tempfile::tempdir().expect("home");
        assert_eq!(
            list_bot_feed(&home.path().join("missing")),
            json!({"channels": []})
        );
    }
}
