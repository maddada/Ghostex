//! Token buckets in memory: per caller address, per device token, per start token, and one for the whole relay.
//! A bucket holds `burst` requests and refills `per_minute` a minute; an idle bucket is forgotten after 10
//! minutes. Each table is bounded: when it is full of busy keys a new key is refused rather than growing it.

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
    buckets: Mutex<HashMap<K, Bucket>>,
}

impl<K: Eq + Hash + Clone> Limiter<K> {
    pub fn new(rate: Rate) -> Self {
        Limiter {
            rate,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one request for `key` at `now`; `Err` carries how long until one is free.
    pub fn take(&self, key: &K, now: Instant) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().expect("limiter lock");
        if !buckets.contains_key(key) && buckets.len() >= MAX_KEYS {
            prune(&mut buckets, now);
            if buckets.len() >= MAX_KEYS {
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
        prune(&mut self.buckets.lock().expect("limiter lock"), now);
    }
}

fn prune<K>(buckets: &mut HashMap<K, Bucket>, now: Instant) {
    buckets.retain(|_, bucket| now.saturating_duration_since(bucket.at) < IDLE_FORGET);
}
