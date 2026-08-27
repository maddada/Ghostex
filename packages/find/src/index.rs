//! The shared search engine: scan once, then answer ranked queries.
//!
//! CDXC:AgentHistorySearch 2026-08-20:
//! The TUI and the Find GUI must rank identically, so both go through this one
//! module rather than each re-implementing filtering and sorting. `gx f` holds a
//! `SearchIndex` for the life of the picker; gxserver keeps one warm and answers
//! GUI keystrokes from it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agent::Agent;
use crate::favorites::{self, FavoriteSet};
use crate::fuzzy::{Match, Matcher};
use crate::scan::{Record, Scanner};

pub const SECONDS_PER_DAY: i64 = 86_400;
pub const UNKNOWN_DAY_KEY: i64 = i64::MIN;

pub fn day_key(ts: i64) -> i64 {
    if ts <= 0 {
        return UNKNOWN_DAY_KEY;
    }
    ts.div_euclid(SECONDS_PER_DAY)
}

/// Resolve the derived-cache directory, matching the layout the Zig build used:
/// `$GHOSTEX_HOME/cache/zehn`, else `$XDG_CACHE_HOME/ghostex/zehn`, else
/// `~/.cache/ghostex/zehn`.
pub fn cache_root(home: &str, ghostex_home: Option<&str>, xdg_cache: Option<&str>) -> PathBuf {
    if let Some(root) = ghostex_home {
        if !root.is_empty() && Path::new(root).is_absolute() {
            return PathBuf::from(root).join("cache").join("zehn");
        }
    }
    if let Some(root) = xdg_cache {
        if !root.is_empty() && Path::new(root).is_absolute() {
            return PathBuf::from(root).join("ghostex").join("zehn");
        }
    }
    PathBuf::from(home)
        .join(".cache")
        .join("ghostex")
        .join("zehn")
}

/// Resolve the cache root from the process environment.
pub fn cache_root_from_env(home: &str) -> PathBuf {
    let ghostex_home = std::env::var("GHOSTEX_HOME").ok();
    let xdg_cache = std::env::var("XDG_CACHE_HOME").ok();
    cache_root(home, ghostex_home.as_deref(), xdg_cache.as_deref())
}

#[derive(Clone, Debug, Default)]
pub struct QueryOptions {
    pub query: String,
    /// Empty means "all agents".
    pub agents: Vec<Agent>,
    /// Exact project path to restrict to.
    pub project: Option<String>,
    /// Day grouping changes the primary sort key to the day bucket.
    pub group_by_day: bool,
    pub offset: usize,
    /// 0 means "no limit".
    pub limit: usize,
}

#[derive(Clone, Debug)]
pub struct Hit {
    /// Index into `SearchIndex::records`.
    pub index: usize,
    pub score: i32,
    pub favorite: bool,
    /// Byte offsets in the prompt text that matched the query.
    pub positions: Vec<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct QueryResult {
    /// Every record in the index, before filtering.
    pub total: usize,
    /// Records that passed filters and matched the query.
    pub matched: usize,
    /// The requested window of `matched`, already ranked.
    pub hits: Vec<Hit>,
}

pub struct SearchIndex {
    pub records: Vec<Record>,
    pub favorites: FavoriteSet,
    pub favorites_path: PathBuf,
    pub opencode_error: Option<String>,
    pub built_at: SystemTime,
    /*
    CDXC:AgentHistorySearch 2026-08-20:
    Prompts are addressed by a stable key rather than a position, so a client can
    act on a result it fetched minutes ago. The key is the favorites hash of
    agent+text — already unique per record, because dedup collapses identical
    (agent, text) pairs — which means a rebuild that reorders everything cannot
    make a pending action point at the wrong prompt.
    */
    by_key: HashMap<u64, usize>,
    matcher: Matcher,
}

impl SearchIndex {
    /// Scan every agent history store under `home` and load the favorites file.
    pub fn build(home: &str, cache_root: &Path, favorites_path: PathBuf) -> Self {
        let mut scanner = Scanner::new(home, cache_root);
        scanner.scan_all();
        let mut favorites = FavoriteSet::new();
        favorites.load(&favorites_path);
        let records = std::mem::take(&mut scanner.records);
        let by_key = build_key_map(&records);
        Self {
            records,
            favorites,
            favorites_path,
            opencode_error: scanner.opencode_error.take(),
            built_at: SystemTime::now(),
            by_key,
            matcher: Matcher::new(),
        }
    }

    /// Stable identity of a record, matching `record_key`.
    pub fn key_of(&self, index: usize) -> Option<u64> {
        self.records.get(index).map(record_key)
    }

    /// Resolve a record position from its stable key.
    pub fn find_by_key(&self, key: u64) -> Option<usize> {
        self.by_key.get(&key).copied()
    }

    /// Build from the process environment (`$HOME`, `$GHOSTEX_HOME`, `$XDG_*`).
    pub fn build_from_env() -> Option<Self> {
        let home = std::env::var("HOME").ok()?;
        let cache = cache_root_from_env(&home);
        let xdg_config = std::env::var("XDG_CONFIG_HOME").ok();
        let fav_path = favorites::path_for(&home, xdg_config.as_deref());
        Some(Self::build(&home, &cache, fav_path))
    }

    /// Restrict the index to a single agent (the `--agent` flag).
    pub fn retain_agent(&mut self, agent: Agent) {
        self.records.retain(|rec| rec.agent == agent);
        self.by_key = build_key_map(&self.records);
    }

    pub fn is_favorite(&self, rec: &Record) -> bool {
        self.favorites
            .contains(favorites::key(rec.agent.label(), &rec.text))
    }

    /// Toggle the favorite flag for a record and persist the file.
    pub fn toggle_favorite(&mut self, agent: Agent, text: &str) -> bool {
        let now = self.favorites.toggle(favorites::key(agent.label(), text));
        self.favorites.save(&self.favorites_path);
        now
    }

    /// Set the favorite flag explicitly and persist the file.
    pub fn set_favorite(&mut self, agent: Agent, text: &str, favorite: bool) -> bool {
        self.favorites
            .set(favorites::key(agent.label(), text), favorite);
        self.favorites.save(&self.favorites_path);
        favorite
    }

    /// Distinct project paths in first-seen order, for the project picker.
    pub fn projects(&self) -> Vec<&str> {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut out = Vec::new();
        for rec in &self.records {
            if rec.project.is_empty() {
                continue;
            }
            if seen.insert(rec.project.as_str()) {
                out.push(rec.project.as_str());
            }
        }
        out
    }

    /// Agents that actually appear in the index, in canonical order.
    pub fn present_agents(&self) -> Vec<Agent> {
        let present: HashSet<Agent> = self.records.iter().map(|rec| rec.agent).collect();
        crate::agent::ALL_AGENTS
            .into_iter()
            .filter(|a| present.contains(a))
            .collect()
    }

    /// Rank the index against `options` and return the requested window.
    ///
    /// CDXC:AgentHistorySearch 2026-08-20:
    /// The Find GUI runs this per keystroke against ~25k records, where a single
    /// thread spends ~350ms in the DP matcher. The scan is embarrassingly
    /// parallel — records are immutable and each worker owns its own scratch —
    /// so it is split across cores and the ranked order is rebuilt afterwards
    /// from the same total ordering, which keeps results identical to the
    /// single-threaded picker.
    pub fn query(&mut self, options: &QueryOptions) -> QueryResult {
        let needle = options.query.as_bytes();
        let agent_mask = options
            .agents
            .iter()
            .fold(0u8, |mask, agent| mask | agent.bit());
        let project = options.project.as_deref();

        let mut hits = self.collect_hits(needle, agent_mask, project);

        let query_empty = needle.is_empty();
        let group_by_day = options.group_by_day;
        hits.sort_by(|x, y| {
            if hit_less(x, y, query_empty, group_by_day) {
                std::cmp::Ordering::Less
            } else if hit_less(y, x, query_empty, group_by_day) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        let matched = hits.len();
        let start = options.offset.min(matched);
        let end = if options.limit == 0 {
            matched
        } else {
            (start + options.limit).min(matched)
        };
        let window = hits[start..end]
            .iter()
            .map(|h| Hit {
                index: h.index,
                score: h.score,
                favorite: h.favorite,
                positions: h.m.highlights().to_vec(),
            })
            .collect();

        QueryResult {
            total: self.records.len(),
            matched,
            hits: window,
        }
    }

    fn collect_hits(
        &mut self,
        needle: &[u8],
        agent_mask: u8,
        project: Option<&str>,
    ) -> Vec<RankedHit> {
        let workers = query_worker_count(self.records.len());
        if workers <= 1 {
            let mut matcher = std::mem::take(&mut self.matcher);
            let hits = match_range(
                &self.records,
                &self.favorites,
                0,
                self.records.len(),
                needle,
                agent_mask,
                project,
                &mut matcher,
            );
            self.matcher = matcher;
            return hits;
        }

        let records = &self.records;
        let favorites = &self.favorites;
        let chunk = records.len().div_ceil(workers);
        let mut collected: Vec<Vec<RankedHit>> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for worker in 0..workers {
                let start = worker * chunk;
                if start >= records.len() {
                    break;
                }
                let end = (start + chunk).min(records.len());
                handles.push(scope.spawn(move || {
                    let mut matcher = Matcher::new();
                    match_range(
                        records,
                        favorites,
                        start,
                        end,
                        needle,
                        agent_mask,
                        project,
                        &mut matcher,
                    )
                }));
            }
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .collect()
        });
        let total: usize = collected.iter().map(Vec::len).sum();
        let mut hits = Vec::with_capacity(total);
        for part in &mut collected {
            hits.append(part);
        }
        hits
    }
}

/// Records below this count are cheap enough that thread setup would dominate.
const PARALLEL_QUERY_MIN_RECORDS: usize = 2_000;
const PARALLEL_QUERY_MAX_WORKERS: usize = 12;

fn query_worker_count(records: usize) -> usize {
    if records < PARALLEL_QUERY_MIN_RECORDS {
        return 1;
    }
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .min(PARALLEL_QUERY_MAX_WORKERS)
        .max(1)
}

#[allow(clippy::too_many_arguments)]
fn match_range(
    records: &[Record],
    favorites: &FavoriteSet,
    start: usize,
    end: usize,
    needle: &[u8],
    agent_mask: u8,
    project: Option<&str>,
    matcher: &mut Matcher,
) -> Vec<RankedHit> {
    let mut hits = Vec::new();
    for (offset, rec) in records[start..end].iter().enumerate() {
        if agent_mask != 0 && (agent_mask & rec.agent.bit()) == 0 {
            continue;
        }
        if let Some(p) = project {
            if rec.project != p {
                continue;
            }
        }
        let Some(m) = matcher.matches(needle, rec.text.as_bytes()) else {
            continue;
        };
        let favorite = favorites.contains(favorites::key(rec.agent.label(), &rec.text));
        hits.push(RankedHit {
            index: start + offset,
            score: m.score,
            favorite,
            ts: rec.ts,
            m,
        });
    }
    hits
}

/// Stable identity of a prompt: the favorites key of agent + text.
pub fn record_key(record: &Record) -> u64 {
    favorites::key(record.agent.label(), &record.text)
}

fn build_key_map(records: &[Record]) -> HashMap<u64, usize> {
    let mut map = HashMap::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        map.entry(record_key(record)).or_insert(index);
    }
    map
}

struct RankedHit {
    index: usize,
    score: i32,
    favorite: bool,
    ts: i64,
    m: Match,
}

fn score_bucket(score: i32) -> i32 {
    score.div_euclid(32)
}

/// Empty queries are strict reverse chronology. Search results retain the
/// established ordering: day bucket (when grouping) → favorites tier → score
/// bucket → recency → exact score → stable index.
fn hit_less(x: &RankedHit, y: &RankedHit, query_empty: bool, group_by_day: bool) -> bool {
    if query_empty {
        if x.ts != y.ts {
            return x.ts > y.ts;
        }
        return x.index < y.index;
    }
    if group_by_day {
        let dx = day_key(x.ts);
        let dy = day_key(y.ts);
        if dx != dy {
            return dx > dy;
        }
    }
    if x.favorite != y.favorite {
        return x.favorite;
    }
    let bx = score_bucket(x.score);
    let by = score_bucket(y.score);
    if bx != by {
        return bx > by;
    }
    if x.ts != y.ts {
        return x.ts > y.ts;
    }
    if x.score != y.score {
        return x.score > y.score;
    }
    x.index < y.index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::Meta;

    fn record(agent: Agent, text: &str, project: &str, ts: i64) -> Record {
        Record {
            agent,
            title: String::new(),
            text: text.to_string(),
            project: project.to_string(),
            session: String::new(),
            ts,
            meta: Meta::default(),
        }
    }

    fn index(records: Vec<Record>) -> SearchIndex {
        let by_key = build_key_map(&records);
        SearchIndex {
            by_key,
            records,
            favorites: FavoriteSet::new(),
            favorites_path: PathBuf::from("/nonexistent/favorites"),
            opencode_error: None,
            built_at: SystemTime::now(),
            matcher: Matcher::new(),
        }
    }

    #[test]
    fn empty_query_orders_by_recency() {
        let mut idx = index(vec![
            record(Agent::Claude, "older", "/a", 100),
            record(Agent::Codex, "newer", "/b", 300),
            record(Agent::Pi, "middle", "/a", 200),
        ]);
        let out = idx.query(&QueryOptions::default());
        assert_eq!(out.total, 3);
        assert_eq!(out.matched, 3);
        let texts: Vec<&str> = out
            .hits
            .iter()
            .map(|h| idx.records[h.index].text.as_str())
            .collect();
        assert_eq!(texts, vec!["newer", "middle", "older"]);
    }

    #[test]
    fn agent_and_project_filters_narrow_the_result_set() {
        let mut idx = index(vec![
            record(Agent::Claude, "one", "/a", 1),
            record(Agent::Codex, "two", "/a", 2),
            record(Agent::Codex, "three", "/b", 3),
        ]);
        let out = idx.query(&QueryOptions {
            agents: vec![Agent::Codex],
            ..Default::default()
        });
        assert_eq!(out.matched, 2);
        let out = idx.query(&QueryOptions {
            agents: vec![Agent::Codex],
            project: Some("/a".to_string()),
            ..Default::default()
        });
        assert_eq!(out.matched, 1);
        assert_eq!(idx.records[out.hits[0].index].text, "two");
    }

    #[test]
    fn favorites_float_above_better_scoring_matches() {
        let mut idx = index(vec![
            record(Agent::Claude, "refactor the parser", "/a", 500),
            record(Agent::Claude, "r e f a c t o r", "/a", 100),
        ]);
        let before = idx.query(&QueryOptions {
            query: "refactor".into(),
            ..Default::default()
        });
        assert_eq!(
            idx.records[before.hits[0].index].text,
            "refactor the parser"
        );

        idx.favorites
            .set(favorites::key("claude", "r e f a c t o r"), true);
        let after = idx.query(&QueryOptions {
            query: "refactor".into(),
            ..Default::default()
        });
        assert!(after.hits[0].favorite);
        assert_eq!(idx.records[after.hits[0].index].text, "r e f a c t o r");
    }

    #[test]
    fn day_grouping_sorts_newest_day_first() {
        let day = SECONDS_PER_DAY;
        let mut idx = index(vec![
            record(Agent::Claude, "old day high score", "/a", day),
            record(Agent::Claude, "new day", "/a", day * 5),
        ]);
        let out = idx.query(&QueryOptions {
            group_by_day: true,
            ..Default::default()
        });
        assert_eq!(idx.records[out.hits[0].index].text, "new day");
    }

    #[test]
    fn offset_and_limit_window_the_ranked_list() {
        let mut idx = index(vec![
            record(Agent::Claude, "a", "/a", 300),
            record(Agent::Claude, "b", "/a", 200),
            record(Agent::Claude, "c", "/a", 100),
        ]);
        let out = idx.query(&QueryOptions {
            offset: 1,
            limit: 1,
            ..Default::default()
        });
        assert_eq!(out.matched, 3);
        assert_eq!(out.hits.len(), 1);
        assert_eq!(idx.records[out.hits[0].index].text, "b");
    }

    #[test]
    fn projects_are_listed_once_in_first_seen_order() {
        let idx = index(vec![
            record(Agent::Claude, "a", "/b", 1),
            record(Agent::Claude, "b", "/a", 2),
            record(Agent::Claude, "c", "/b", 3),
            record(Agent::Claude, "d", "", 4),
        ]);
        assert_eq!(idx.projects(), vec!["/b", "/a"]);
    }

    #[test]
    fn records_resolve_from_their_stable_key_across_reordering() {
        let a = record(Agent::Claude, "first", "/a", 1);
        let b = record(Agent::Codex, "second", "/b", 2);
        let key_b = record_key(&b);

        let forward = index(vec![a.clone(), b.clone()]);
        assert_eq!(forward.find_by_key(key_b), Some(1));

        // Rebuilt in the opposite order: the same key still finds the same prompt.
        let reversed = index(vec![b, a]);
        assert_eq!(reversed.find_by_key(key_b), Some(0));
        assert_eq!(reversed.records[0].text, "second");
    }

    #[test]
    fn an_unknown_key_resolves_to_nothing() {
        let idx = index(vec![record(Agent::Claude, "only", "/a", 1)]);
        assert_eq!(idx.find_by_key(0xdead_beef), None);
    }

    #[test]
    fn cache_root_prefers_ghostex_home_then_xdg() {
        assert_eq!(
            cache_root("/home/x", Some("/gx"), Some("/xdg")),
            PathBuf::from("/gx/cache/zehn")
        );
        assert_eq!(
            cache_root("/home/x", None, Some("/xdg")),
            PathBuf::from("/xdg/ghostex/zehn")
        );
        assert_eq!(
            cache_root("/home/x", None, None),
            PathBuf::from("/home/x/.cache/ghostex/zehn")
        );
        // Relative overrides are ignored, matching the Zig resolver.
        assert_eq!(
            cache_root("/home/x", Some("relative"), None),
            PathBuf::from("/home/x/.cache/ghostex/zehn")
        );
    }
}
