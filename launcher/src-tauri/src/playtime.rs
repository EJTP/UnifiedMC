//! How long each server and instance has actually been played.
//!
//! Written when a game exits, which is the only moment the number is known: a launch blocks
//! until the window closes, so the length of a session is the length of that call.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::paths;

/// A day is a whole day since the epoch, not a date.
///
/// A sparkline needs buckets, not calendars. Integers mean no date library here and no date
/// parsing in the window - JavaScript turns one back into a local date with a multiplication.
pub type Day = i64;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Played {
    pub seconds: u64,
    /// Unix seconds of the last session's end.
    pub last: u64,
    pub sessions: u32,
    /// Seconds per day, for the trend. Older days are dropped rather than kept forever.
    pub days: BTreeMap<Day, u64>,
}

/// How much history the sparkline can draw from. Two weeks shown, a month kept, so that
/// widening the chart later does not need a migration.
const KEPT_DAYS: Day = 30;

/// A session shorter than this is a crash on startup, not play. It still counts towards the
/// total - the time was really spent - but it does not count as having played a round.
const A_REAL_SESSION: u64 = 60;

pub type Book = BTreeMap<String, Played>;

fn file() -> PathBuf {
    paths::data().join("playtime.json")
}

pub fn load() -> Book {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(book: &Book) -> Result<()> {
    let path = file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(book)?)?;
    Ok(())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn today() -> Day {
    (now() / 86_400) as Day
}

/// Add one session. Never fatal: losing a playtime figure is not a reason to fail a launch
/// that already happened.
pub fn record(key: &str, seconds: u64) {
    if key.is_empty() || seconds == 0 {
        return;
    }
    let mut book = load();
    let played = book.entry(key.to_string()).or_default();

    played.seconds += seconds;
    played.last = now();
    if seconds >= A_REAL_SESSION {
        played.sessions += 1;
    }

    let today = today();
    *played.days.entry(today).or_default() += seconds;
    played.days.retain(|day, _| *day > today - KEPT_DAYS);

    if let Err(error) = save(&book) {
        eprintln!("could not write playtime: {error}");
    }
}

/// Run something and write down how long it took.
///
/// Every launch goes through here, so no path can quietly stop counting. `Instant`, not the
/// wall clock: a session's length must survive the machine's clock being corrected mid-game.
pub async fn timed<F, T>(key: String, run: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let started = Instant::now();
    let result = run.await;
    record(&key, started.elapsed().as_secs());
    result
}

/// What a server's playtime is filed under. Its address, because that is what survives the
/// row being removed and added back, which an id does not.
pub fn server_key(address: &str) -> String {
    format!("server:{address}")
}

pub fn instance_key(id: &str) -> String {
    format!("instance:{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `record` writes to the real data directory, so the parts worth testing are the ones
    /// that decide what it writes.
    fn apply(played: &mut Played, seconds: u64, day: Day) {
        played.seconds += seconds;
        if seconds >= A_REAL_SESSION {
            played.sessions += 1;
        }
        *played.days.entry(day).or_default() += seconds;
        played.days.retain(|d, _| *d > day - KEPT_DAYS);
    }

    #[test]
    fn a_crash_on_startup_is_not_a_session_but_its_seconds_are_still_real() {
        let mut played = Played::default();
        apply(&mut played, 3_600, 20_000);
        apply(&mut played, 12, 20_000); // launched, died immediately, launched again
        apply(&mut played, 1_800, 20_000);

        assert_eq!(played.sessions, 2, "the twelve-second one was not a round");
        assert_eq!(
            played.seconds, 5_412,
            "but the time it took is not invented away"
        );
        assert_eq!(played.days[&20_000], 5_412, "one day, one bucket");
    }

    #[test]
    fn a_days_bucket_accumulates_and_old_days_fall_off() {
        let mut played = Played::default();
        apply(&mut played, 600, 100);
        apply(&mut played, 600, 105);
        assert_eq!(played.days.len(), 2);

        // A month later: the first two are past the window, the newest is kept.
        apply(&mut played, 600, 140);
        assert_eq!(
            played.days.keys().copied().collect::<Vec<_>>(),
            vec![140],
            "only what is inside the window survives"
        );
        assert_eq!(played.seconds, 1_800, "the total is not a rolling window");
    }

    #[test]
    fn a_server_and_an_instance_that_share_a_name_do_not_share_a_total() {
        assert_ne!(server_key("abc"), instance_key("abc"));
        assert_eq!(
            server_key("mc.example.com:25565"),
            "server:mc.example.com:25565"
        );
    }
}
