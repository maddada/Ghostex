//! Token buckets in memory: per caller address, per device token, per start token, and one for the whole relay;
//! for the `/push/` routes also per alert, background and widget token, per mailbox and snapshot id, and per
//! caller address for the ids it may create. A bucket holds `burst` requests and refills `per_minute` a minute.
//! An idle bucket of the Live Activity route is forgotten after 10 minutes; one of the `/push/` routes only once it
//! would have refilled completely (10 minutes at least). Each table is bounded: when it is full of busy keys a new
//! key is refused rather than growing it.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Rate {
    pub burst: f64,
    pub per_minute: f64,
}

/// The relay's limits; `README.md` gives the reasons for each number.
#[derive(Clone, Copy, Debug)]
pub struct LimitConfig {
    pub per_address: Rate,
    pub per_token: Rate,
    pub per_start_token: Rate,
    pub global: Rate,
    /// Requests to Apple in flight at once.
    pub in_flight: usize,
    /// Tokens Apple dropped that the relay remembers (refusing them without asking Apple); past this the oldest is
    /// forgotten, so random tokens cannot grow the relay's memory without end.
    pub max_gone_tokens: usize,
    /// The `/push/` routes: every request from one caller address (its own bucket, so this traffic never takes
    /// from the Live Activity route's).
    pub push_per_address: Rate,
    /// Per device token: visible notifications.
    pub alert_per_token: Rate,
    /// Per device token: background pushes.
    pub background_per_token: Rate,
    /// Per widget push token.
    pub widgets_per_token: Rate,
    /// Every push of the `/push/` routes that is about to go to Apple, relay-wide.
    pub push_global: Rate,
    /// Requests of the `/push/` routes to Apple in flight at once.
    pub push_in_flight: usize,
    /// Mailbox and snapshot requests from one caller address.
    pub store_per_address: Rate,
    /// Ids one caller address may create in the mailbox and the snapshot store together (replacing what an
    /// existing id holds does not count).
    pub new_ids_per_address: Rate,
    /// Requests per mailbox id and per snapshot id.
    pub per_mailbox_id: Rate,
    pub per_snapshot_id: Rate,
    /// Mailbox entries and snapshots held at once, together.
    pub store_max_entries: usize,
    /// Bytes of mailbox tokens and snapshots held at once, together.
    pub store_max_bytes: usize,
    /// How long a mailbox entry waits to be taken.
    pub mailbox_lifetime: Duration,
    /// How long a snapshot is kept when nothing replaces it.
    pub snapshot_lifetime: Duration,
    /// Keys each table of the `/push/` routes' limits holds at most (the Live Activity route's tables keep
    /// theirs). Chosen so that every table and the store full fit under the service's memory limit in a local
    /// macOS measurement; bodies being read come on top, and Linux was not measured (README, "Memory").
    pub push_max_keys: usize,
    /// Tokens Apple dropped that the `/push/` routes remember, apart from the Live Activity route's.
    pub push_max_gone_tokens: usize,
}

impl Default for LimitConfig {
    fn default() -> Self {
        LimitConfig {
            per_address: Rate {
                burst: 60.0,
                per_minute: 60.0,
            },
            per_token: Rate {
                burst: 10.0,
                per_minute: 10.0,
            },
            per_start_token: Rate {
                burst: 2.0,
                per_minute: 6.0 / 60.0,
            },
            global: Rate {
                burst: 300.0,
                per_minute: 3000.0,
            },
            in_flight: 64,
            max_gone_tokens: 100_000,
            push_per_address: Rate {
                burst: 60.0,
                per_minute: 60.0,
            },
            alert_per_token: Rate {
                burst: 10.0,
                per_minute: 10.0,
            },
            background_per_token: Rate {
                burst: 3.0,
                per_minute: 3.0 / 60.0,
            },
            widgets_per_token: Rate {
                burst: 4.0,
                per_minute: 6.0 / 60.0,
            },
            push_global: Rate {
                burst: 300.0,
                per_minute: 3000.0,
            },
            push_in_flight: 64,
            store_per_address: Rate {
                burst: 60.0,
                per_minute: 60.0,
            },
            new_ids_per_address: Rate {
                burst: 20.0,
                per_minute: 20.0 / 60.0,
            },
            per_mailbox_id: Rate {
                burst: 20.0,
                per_minute: 20.0,
            },
            per_snapshot_id: Rate {
                burst: 20.0,
                per_minute: 20.0,
            },
            store_max_entries: 5_000,
            store_max_bytes: 5 * 1024 * 1024,
            mailbox_lifetime: Duration::from_secs(10 * 60),
            snapshot_lifetime: Duration::from_secs(24 * 3600),
            push_max_keys: 7_000,
            push_max_gone_tokens: 7_000,
        }
    }
}

const IDLE_FORGET: Duration = Duration::from_secs(600);
const MAX_KEYS: usize = 200_000;

#[derive(Clone, Copy)]
struct Bucket {
    tokens: f64,
    at: Instant,
}

pub struct Limiter<K> {
    rate: Rate,
    max_keys: usize,
    /// An idle bucket is forgotten after this long.
    forget_after: Duration,
    buckets: Mutex<HashMap<K, Bucket>>,
}

impl<K: Eq + Hash + Clone> Limiter<K> {
    /// The Live Activity route's limiter: 200,000 keys, an idle bucket forgotten after 10 minutes.
    pub fn new(rate: Rate) -> Self {
        Limiter {
            rate,
            max_keys: MAX_KEYS,
            forget_after: IDLE_FORGET,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// A limiter whose table holds at most `max_keys` keys and forgets an idle bucket only once it would have
    /// refilled completely (10 minutes at least), so forgetting never hands out more than the rate allows: a quota
    /// of 20 an hour stays 20 an hour across a pause.
    pub fn with_max_keys(rate: Rate, max_keys: usize) -> Self {
        let refill = Duration::try_from_secs_f64((rate.burst / rate.per_minute * 60.0).ceil())
            .unwrap_or(Duration::MAX);
        Limiter {
            rate,
            max_keys: max_keys.max(1),
            forget_after: refill.max(IDLE_FORGET),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one request for `key` at `now`; `Err` carries how long until one is free.
    pub fn take(&self, key: &K, now: Instant) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().expect("limiter lock");
        if !buckets.contains_key(key) && buckets.len() >= self.max_keys {
            prune(&mut buckets, now, self.forget_after);
            if buckets.len() >= self.max_keys {
                return Err(Duration::from_secs(60));
            }
        }
        let rate = self.rate;
        let bucket = buckets.entry(key.clone()).or_insert(Bucket {
            tokens: rate.burst,
            at: now,
        });
        let elapsed = now.saturating_duration_since(bucket.at).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * rate.per_minute / 60.0).min(rate.burst);
        bucket.at = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait = (1.0 - bucket.tokens) * 60.0 / rate.per_minute;
            Err(Duration::from_secs_f64(wait.ceil().max(1.0)))
        }
    }

    pub fn prune(&self, now: Instant) {
        prune(
            &mut self.buckets.lock().expect("limiter lock"),
            now,
            self.forget_after,
        );
    }
}

fn prune<K>(buckets: &mut HashMap<K, Bucket>, now: Instant, forget_after: Duration) {
    buckets.retain(|_, bucket| now.saturating_duration_since(bucket.at) < forget_after);
}
