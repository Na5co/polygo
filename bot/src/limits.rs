//! What the bot refuses to do. An App anyone can install is an App anyone can point at a
//! repository the size of a distribution, or at a branch that is pushed every few seconds;
//! neither should cost more than it is worth, and neither should take the bot down for the
//! installations behaving normally.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Sizes and rates, all overridable from the environment so a bigger box can say yes to
/// more without a new build.
pub struct Limits {
    /// Skip a repository larger than this (GitHub reports the size; the clone is shallow,
    /// so this is a generous upper bound on what lands on disk).
    pub max_repo_mb: u64,
    /// Reviews one installation may ask for within `window`.
    pub per_installation: usize,
    pub window: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        // 500 MB covers every repository polygo has been pointed at (open-webui, the
        // largest, is 66 MB checked out) with room to spare; 20 reviews in 10 minutes is
        // more pull-request activity than a person can produce.
        Limits {
            max_repo_mb: 500,
            per_installation: 20,
            window: Duration::from_secs(600),
        }
    }
}

impl Limits {
    pub fn from_env() -> Limits {
        let d = Limits::default();
        Limits {
            max_repo_mb: env("POLYGO_BOT_MAX_REPO_MB").unwrap_or(d.max_repo_mb),
            per_installation: env("POLYGO_BOT_MAX_REVIEWS").unwrap_or(d.per_installation),
            window: d.window,
        }
    }
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

/// How many reviews each installation has asked for lately. In memory on purpose: the bot
/// keeps no state between restarts, and a limiter that forgets on restart still stops the
/// runaway it exists for.
#[derive(Default)]
pub struct Rate {
    seen: Mutex<HashMap<u64, Vec<Instant>>>,
}

impl Rate {
    /// Record an event for `installation` and say whether it is within the limit.
    pub fn allow(&self, installation: u64, limits: &Limits) -> bool {
        let now = Instant::now();
        let mut seen = match self.seen.lock() {
            Ok(s) => s,
            // A poisoned lock means another thread panicked mid-review; letting the event
            // through is friendlier than refusing every one from then on.
            Err(e) => e.into_inner(),
        };
        // Installations nobody has heard from in a while are forgotten entirely.
        seen.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < limits.window);
            !times.is_empty()
        });
        let times = seen.entry(installation).or_default();
        if times.len() >= limits.per_installation {
            return false;
        }
        times.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installation_gets_its_share_and_no_more() {
        let limits = Limits {
            per_installation: 3,
            window: Duration::from_secs(600),
            ..Limits::default()
        };
        let rate = Rate::default();
        assert!(rate.allow(1, &limits));
        assert!(rate.allow(1, &limits));
        assert!(rate.allow(1, &limits));
        assert!(!rate.allow(1, &limits), "the fourth is over the limit");
        // One noisy installation does not spend another's share.
        assert!(rate.allow(2, &limits));
    }

    #[test]
    fn the_window_passes_and_the_share_comes_back() {
        let limits = Limits {
            per_installation: 1,
            window: Duration::from_millis(50),
            ..Limits::default()
        };
        let rate = Rate::default();
        assert!(rate.allow(7, &limits));
        assert!(!rate.allow(7, &limits));
        std::thread::sleep(Duration::from_millis(60));
        assert!(rate.allow(7, &limits), "the window has passed");
    }

    #[test]
    fn the_environment_can_raise_them() {
        let d = Limits::default();
        assert_eq!(d.max_repo_mb, 500);
        assert_eq!(d.per_installation, 20);
        // Unset variables leave the defaults alone.
        let e = Limits::from_env();
        assert_eq!(e.max_repo_mb, d.max_repo_mb);
    }
}
