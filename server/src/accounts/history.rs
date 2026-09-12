use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File},
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant, SystemTime},
};

use chrono::{Local, NaiveDate};
use serde_json::{json, Value};

use super::{
    helpers,
    history_parser::{Entry, Parser},
    model::Provider,
};

#[derive(Default)]
struct CachedFile {
    modified: Option<SystemTime>,
    size: u64,
    offset: u64,
    parser: Parser,
    entries: HashMap<String, Entry>,
    partial: bool,
}

#[derive(Default)]
pub(crate) struct HistoryRuntime {
    snapshot: RwLock<Value>,
    scan: Mutex<Scan>,
}

#[derive(Default)]
struct Scan {
    started: Option<Instant>,
    running: bool,
    files: BTreeMap<(Provider, PathBuf), CachedFile>,
}

impl HistoryRuntime {
    /// CDXC:AgentProviders 2026-09-11 DECISION:
    /// User keeps separate account buttons but shares conversations across accounts of each provider. History is therefore provider-wide on each computer, while live limits remain per login.
    /// SEE-ALSO: apps/desktop/src/app/window/account_usage/history.rs consumes only this server snapshot.
    pub fn snapshot(self: &Arc<Self>) -> Value {
        let mut scan = self.scan.lock().unwrap_or_else(|e| e.into_inner());
        if !scan.running
            && scan
                .started
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            scan.running = true;
            scan.started = Some(Instant::now());
            let mut files = std::mem::take(&mut scan.files);
            let runtime = Arc::clone(self);
            std::thread::spawn(move || {
                for provider in [Provider::Claude, Provider::Codex] {
                    let result = collect(provider, &mut files);
                    let mut snapshot = runtime.snapshot.write().unwrap_or_else(|e| e.into_inner());
                    if !snapshot.is_object() {
                        *snapshot = json!({});
                    }
                    snapshot[provider.id()] = result;
                }
                let mut scan = runtime.scan.lock().unwrap_or_else(|e| e.into_inner());
                scan.files = files;
                scan.running = false;
            });
        }
        self.snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn roots(provider: Provider) -> (Vec<PathBuf>, bool) {
    let home = crate::resume_lookup::home_dir();
    if provider == Provider::Claude {
        let mut roots = crate::agent_transcripts::claude_project_roots();
        if let Some(config) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            roots.push(PathBuf::from(config).join("projects"));
        }
        return (roots, false);
    }
    let mut homes = crate::resume_lookup::codex_homes();
    let mut partial = false;
    if helpers::executable(&home, "xswap").is_some() {
        match helpers::json_command(&home, "xswap", &["list", "--json"]) {
            Ok(value) => {
                for row in value["accounts"].as_array().into_iter().flatten() {
                    if let Some(path) = row["home"]
                        .as_str()
                        .map(PathBuf::from)
                        .filter(|p| p.is_absolute())
                    {
                        homes.push(path);
                    }
                }
            }
            Err(_) => partial = true,
        }
    }
    (
        homes
            .into_iter()
            .flat_map(|p| [p.join("sessions"), p.join("archived_sessions")])
            .collect(),
        partial,
    )
}

fn collect(provider: Provider, cache: &mut BTreeMap<(Provider, PathBuf), CachedFile>) -> Value {
    let today = Local::now().date_naive();
    let first = today - chrono::Duration::days(29);
    let (mut pending, mut partial) = roots(provider);
    let mut directories = HashSet::new();
    let mut files = BTreeMap::new();
    while let Some(path) = pending.pop() {
        let canonical = match fs::canonicalize(&path) {
            Ok(path) => path,
            Err(error) => {
                partial |= error.kind() != io::ErrorKind::NotFound;
                continue;
            }
        };
        let metadata = match fs::metadata(&canonical) {
            Ok(value) => value,
            Err(_) => {
                partial = true;
                continue;
            }
        };
        if metadata.is_dir() {
            if !directories.insert(canonical.clone()) {
                continue;
            }
            match fs::read_dir(canonical) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => pending.push(entry.path()),
                            Err(_) => partial = true,
                        }
                    }
                }
                Err(_) => partial = true,
            }
        } else if canonical.extension().is_some_and(|s| s == "jsonl") {
            // Idle old transcripts cannot add usage to the displayed calendar window.
            if metadata
                .modified()
                .ok()
                .is_some_and(|t| chrono::DateTime::<Local>::from(t).date_naive() < first)
            {
                continue;
            }
            files.insert(canonical, metadata);
        }
    }
    cache.retain(|(p, path), _| *p != provider || files.contains_key(path));
    let mut unique: HashMap<String, Entry> = HashMap::new();
    for (path, metadata) in files {
        let cached = cache.entry((provider, path.clone())).or_default();
        if cached.size != metadata.len() || cached.modified != metadata.modified().ok() {
            // Append-only logs retain their parser's cumulative baseline. Rewrites restart it.
            if metadata.len() <= cached.size {
                *cached = CachedFile::default();
            }
            if update_file(provider, &path, metadata.len(), cached).is_err() {
                partial = true;
            } else {
                cached.size = metadata.len();
                cached.modified = metadata.modified().ok();
            }
        }
        partial |= cached.partial;
        cached.entries.retain(|_, entry| entry.day >= first);
        for entry in cached.entries.values() {
            merge(&mut unique, entry.clone());
        }
    }
    let has_data = !unique.is_empty();
    let mut days = BTreeMap::<NaiveDate, u64>::new();
    for entry in unique.values().filter(|entry| entry.day <= today) {
        let total = days.entry(entry.day).or_default();
        *total = total.saturating_add(entry.tokens);
    }
    let daily: Vec<_> = (0..30)
        .map(|n| {
            let day = first + chrono::Duration::days(n);
            json!({"date":day.to_string(),"tokens":days.get(&day).copied().unwrap_or(0)})
        })
        .collect();
    json!({"status":if partial {"partial"} else {"ready"},"hasData":has_data,"days":daily,"updatedAt":Local::now().to_rfc3339(),"timeZone":Local::now().format("%Z").to_string()})
}

fn merge(entries: &mut HashMap<String, Entry>, entry: Entry) {
    let replace = entries.get(&entry.key).is_none_or(|old| {
        if old.sidechain != entry.sidechain {
            old.sidechain
        } else {
            entry.tokens > old.tokens
        }
    });
    if replace {
        entries.insert(entry.key.clone(), entry);
    }
}

fn update_file(
    provider: Provider,
    path: &Path,
    size: u64,
    cache: &mut CachedFile,
) -> io::Result<()> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(cache.offset))?;
    let mut reader = BufReader::new(std::io::Read::take(file, size.saturating_sub(cache.offset)));
    let mut line = Vec::new();
    let file_key = path.to_string_lossy();
    let mut consumed = 0u64;
    let mut oversized = false;
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            break;
        }
        let newline = bytes.iter().position(|b| *b == b'\n');
        let len = newline.map(|n| n + 1).unwrap_or(bytes.len());
        consumed += len as u64;
        if line.len() + len > 8 * 1024 * 1024 {
            oversized = true;
        }
        if !oversized {
            line.extend_from_slice(&bytes[..len]);
        }
        reader.consume(len);
        if newline.is_some() {
            if oversized {
                cache.partial |= provider == Provider::Claude || Parser::relevant(provider, &line);
            } else if let Some(entry) = cache.parser.observe(provider, &line, &file_key) {
                merge(&mut cache.entries, entry);
            }
            cache.offset += consumed;
            consumed = 0;
            line.clear();
            oversized = false;
        }
    }
    // An unterminated final record may still be getting written. Retry it on the next append.
    Ok(())
}
