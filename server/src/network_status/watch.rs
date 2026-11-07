//! The online check's state machine, apart from how it reads the network: the runtime feeds it
//! the real checker (probe.rs), the tests a scripted one.

use std::collections::{BTreeMap, BTreeSet};

/// How often the API hosts are probed while every probe answers.
pub const ONLINE_RECHECK_MS: i64 = 30_000;
/// How often they are probed while one fails (to confirm it, or to see the network come back).
pub const FAILING_RECHECK_MS: i64 = 5_000;
/// Failed probes in a row, about 5 seconds apart, before a host counts as unreachable while the
/// computer still has a route out: one missed answer is a blip.
pub const FAILURES_FOR_DOWN: u32 = 2;

/// What the operating system says about the way out: whether it has a route to the internet, and
/// a fingerprint of it (the source addresses it picks) that changes when the network does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteReading {
    pub online: bool,
    pub fingerprint: String,
}

/// How the check reads the network. The tests script it; never cut the computer's network to test.
pub trait NetworkChecker {
    /// Cheap and non-blocking: no packet leaves the computer.
    fn route(&self) -> RouteReading;
    /// BLOCKING (a DNS lookup and a connect, a few seconds at most): whether `target`
    /// (`host:port`) answers.
    fn reach(&self, target: &str) -> bool;
}

/// The computer went offline, or came back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Offline { at: i64 },
    Back { offline_since: i64, at: i64 },
}

#[derive(Clone, Copy, Debug, Default)]
struct TargetState {
    /// Failed probes in a row (0 once it answers).
    failures: u32,
    probed: bool,
}

#[derive(Default)]
pub struct NetworkWatch {
    targets: BTreeMap<String, TargetState>,
    route: Option<RouteReading>,
    next_probe_at: i64,
    offline_since: Option<i64>,
}

impl NetworkWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// One step at `now`: reads the route, probes every target in `wanted` when a probe is due
    /// (its interval passed, a new target appeared, or the route changed), and says whether the
    /// computer went offline or came back. BLOCKING only while it probes.
    pub fn tick(
        &mut self,
        now: i64,
        wanted: &BTreeSet<String>,
        checker: &dyn NetworkChecker,
    ) -> Option<Transition> {
        let route = checker.route();
        let route_changed = self.route.as_ref() != Some(&route);
        self.route = Some(route);
        self.targets.retain(|target, _| wanted.contains(target));
        let new_target = wanted
            .iter()
            .any(|target| !self.targets.contains_key(target));
        if !(route_changed || new_target || now >= self.next_probe_at) {
            return None;
        }
        for target in wanted {
            let reachable = checker.reach(target);
            let state = self.targets.entry(target.clone()).or_default();
            state.probed = true;
            state.failures = if reachable { 0 } else { state.failures + 1 };
        }
        let failing = self.targets.values().any(|state| state.failures > 0);
        self.next_probe_at = now
            + if failing {
                FAILING_RECHECK_MS
            } else {
                ONLINE_RECHECK_MS
            };
        let offline = self.judged_offline();
        match (self.offline_since, offline) {
            (None, true) => {
                self.offline_since = Some(now);
                Some(Transition::Offline { at: now })
            }
            (Some(since), false) => {
                self.offline_since = None;
                Some(Transition::Back {
                    offline_since: since,
                    at: now,
                })
            }
            _ => None,
        }
    }

    /// Offline when no wanted target answers: at once when the system has no route out either,
    /// else after `FAILURES_FOR_DOWN` misses in a row.
    fn judged_offline(&self) -> bool {
        if self.targets.is_empty() || self.targets.values().any(|state| !state.probed) {
            return false;
        }
        let needed = if self.route.as_ref().is_some_and(|route| !route.online) {
            1
        } else {
            FAILURES_FOR_DOWN
        };
        self.targets.values().all(|state| state.failures >= needed)
    }

    /// Since when the computer has been offline, while it is.
    pub fn offline_since(&self) -> Option<i64> {
        self.offline_since
    }

    /// The targets that missed `FAILURES_FOR_DOWN` probes in a row (every one while offline).
    pub fn down_targets(&self) -> BTreeSet<String> {
        self.targets
            .iter()
            .filter(|(_, state)| {
                state.failures >= FAILURES_FOR_DOWN || self.offline_since.is_some()
            })
            .map(|(target, _)| target.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A network the test sets by hand, counting the probes it answers.
    struct Scripted {
        route: RefCell<RouteReading>,
        up: RefCell<HashMap<String, bool>>,
        probes: RefCell<usize>,
    }

    impl Scripted {
        fn new() -> Self {
            Self {
                route: RefCell::new(RouteReading {
                    online: true,
                    fingerprint: "wifi".to_string(),
                }),
                up: RefCell::new(HashMap::new()),
                probes: RefCell::new(0),
            }
        }
        fn set(&self, target: &str, up: bool) {
            self.up.borrow_mut().insert(target.to_string(), up);
        }
        fn route(&self, online: bool, fingerprint: &str) {
            *self.route.borrow_mut() = RouteReading {
                online,
                fingerprint: fingerprint.to_string(),
            };
        }
        fn probes(&self) -> usize {
            *self.probes.borrow()
        }
    }

    impl NetworkChecker for Scripted {
        fn route(&self) -> RouteReading {
            self.route.borrow().clone()
        }
        fn reach(&self, target: &str) -> bool {
            *self.probes.borrow_mut() += 1;
            self.up.borrow().get(target).copied().unwrap_or(true)
        }
    }

    const CLAUDE: &str = "api.anthropic.com:443";
    const CODEX: &str = "chatgpt.com:443";

    fn wanted(targets: &[&str]) -> BTreeSet<String> {
        targets.iter().map(|target| target.to_string()).collect()
    }

    #[test]
    fn an_answer_is_cached_for_thirty_seconds() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE]);
        assert_eq!(watch.tick(0, &targets, &net), None);
        assert_eq!(net.probes(), 1);
        for now in (2_000..ONLINE_RECHECK_MS).step_by(2_000) {
            watch.tick(now, &targets, &net);
        }
        assert_eq!(net.probes(), 1, "no probe inside the cache window");
        watch.tick(ONLINE_RECHECK_MS, &targets, &net);
        assert_eq!(net.probes(), 2);
    }

    #[test]
    fn a_network_change_probes_at_once() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE]);
        watch.tick(0, &targets, &net);
        net.route(true, "ethernet");
        watch.tick(2_000, &targets, &net);
        assert_eq!(net.probes(), 2);
    }

    #[test]
    fn one_missed_probe_is_a_blip_and_two_are_offline() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE]);
        watch.tick(0, &targets, &net);
        net.set(CLAUDE, false);
        assert_eq!(watch.tick(30_000, &targets, &net), None);
        assert_eq!(watch.offline_since(), None);
        // The miss brings the next probe forward to 5 seconds.
        assert_eq!(watch.tick(33_000, &targets, &net), None);
        assert_eq!(
            watch.tick(35_000, &targets, &net),
            Some(Transition::Offline { at: 35_000 })
        );
        assert_eq!(watch.offline_since(), Some(35_000));
        assert_eq!(watch.down_targets(), wanted(&[CLAUDE]));
        // It stays offline without saying so again.
        assert_eq!(watch.tick(40_000, &targets, &net), None);
    }

    #[test]
    fn no_route_out_is_offline_on_the_first_miss() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE]);
        watch.tick(0, &targets, &net);
        net.route(false, "none");
        net.set(CLAUDE, false);
        assert_eq!(
            watch.tick(2_000, &targets, &net),
            Some(Transition::Offline { at: 2_000 })
        );
    }

    #[test]
    fn the_first_answer_after_an_outage_is_back_within_seconds() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE]);
        net.route(false, "none");
        net.set(CLAUDE, false);
        assert_eq!(
            watch.tick(0, &targets, &net),
            Some(Transition::Offline { at: 0 })
        );
        // Offline, it looks every 5 seconds, not every 30.
        net.set(CLAUDE, true);
        assert_eq!(watch.tick(2_000, &targets, &net), None);
        assert_eq!(
            watch.tick(FAILING_RECHECK_MS, &targets, &net),
            Some(Transition::Back {
                offline_since: 0,
                at: FAILING_RECHECK_MS
            })
        );
        assert_eq!(watch.offline_since(), None);
        assert!(watch.down_targets().is_empty());
    }

    #[test]
    fn the_route_coming_back_probes_at_once() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE]);
        net.route(false, "none");
        net.set(CLAUDE, false);
        watch.tick(0, &targets, &net);
        net.route(true, "wifi");
        net.set(CLAUDE, true);
        assert!(matches!(
            watch.tick(1_000, &targets, &net),
            Some(Transition::Back { .. })
        ));
    }

    #[test]
    fn one_api_down_is_not_the_network_down() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        let targets = wanted(&[CLAUDE, CODEX]);
        net.set(CLAUDE, false);
        watch.tick(0, &targets, &net);
        assert_eq!(watch.tick(5_000, &targets, &net), None);
        assert_eq!(watch.offline_since(), None);
        assert_eq!(watch.down_targets(), wanted(&[CLAUDE]));
    }

    #[test]
    fn a_new_target_is_probed_at_once_and_never_judged_unprobed() {
        let net = Scripted::new();
        let mut watch = NetworkWatch::new();
        watch.tick(0, &wanted(&[CLAUDE]), &net);
        watch.tick(2_000, &wanted(&[CLAUDE, CODEX]), &net);
        assert_eq!(net.probes(), 3);
        assert_eq!(watch.offline_since(), None);
    }
}
