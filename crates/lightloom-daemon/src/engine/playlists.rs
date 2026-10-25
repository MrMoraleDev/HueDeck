// SPDX-License-Identifier: GPL-3.0-or-later
//! Effect playlists, the auto-next timer and the other ways the effect changes
//! by itself: shuffle and the developer's `loadEffect`.
//!
//! * A **playlist** shows its entries one after the other, in order or (with
//!   `shuffle`) in a new random order every round, and loops. An entry lasts
//!   its `duration`, else the timer's interval; it loads its preset and then
//!   its values into the effect (stored, like applying a preset). Entries whose
//!   effect is not installed are skipped. Choosing an effect, a preset or
//!   shuffling ends the playlist; next/previous move within it.
//! * The **auto-next timer** (SignalRGB's "Effect Timer Interval", 300 s by
//!   default, off by default) shows the next effect of the library while no
//!   playlist plays.
//! * **Shuffle** and the timer skip game integrations (effects with screen
//!   meters) and developer effects.
//! * Nothing switches while the canvas is paused.
//!
//! The playing playlist and its entry, and the timer, are kept in the database
//! (`lighting.playlist`, `lighting.effectTimer`) and resume at the next start.

use super::{internal, keys, Engine};
use lightloom_api::model::{EffectTimer, Playlist, PlaylistEntry, PlaylistPlayback};
use lightloom_api::{ApiError, Request};
use lightloom_runtime::effects::{EffectFile, EffectSource};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Largest page `loadEffect` takes.
const MAX_DEVELOPER_PAGE: usize = 4 << 20;
/// Longest playlist name and id.
const MAX_NAME: usize = 100;

/// What the database keeps of the playback.
#[derive(Serialize, Deserialize)]
struct Saved {
    id: String,
    entry: usize,
}

/// The playlist playing now.
#[derive(Debug)]
struct Playing {
    playlist: Playlist,
    /// Entry indexes in play order (shuffled when the playlist shuffles).
    order: Vec<usize>,
    /// Position in `order`.
    pos: usize,
}

/// Playback state, free of the engine (unit-tested below).
#[derive(Debug)]
pub(super) struct Player {
    timer: EffectTimer,
    playing: Option<Playing>,
    /// When the next effect is due, and the same moment in Unix milliseconds.
    due: Option<(Instant, u64)>,
    rng: u64,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            timer: EffectTimer::default(),
            playing: None,
            due: None,
            rng: (uuid::Uuid::new_v4().as_u128() as u64) | 1,
        }
    }
}

impl Player {
    pub(super) fn timer(&self) -> EffectTimer {
        self.timer
    }

    pub(super) fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// For the snapshot.
    pub(super) fn playback(&self) -> Option<PlaylistPlayback> {
        self.playing.as_ref().map(|p| PlaylistPlayback {
            id: p.playlist.id.clone(),
            name: p.playlist.name.clone(),
            entry: p.order.get(p.pos).copied().unwrap_or(0),
        })
    }

    /// When the next switch is due (Unix milliseconds).
    pub(super) fn next_switch_ms(&self) -> Option<u64> {
        self.due.map(|(_, ms)| ms)
    }

    fn entry(&self) -> Option<&PlaylistEntry> {
        let p = self.playing.as_ref()?;
        p.playlist.entries.get(*p.order.get(p.pos)?)
    }

    /// How long the effect shown now lasts; `None`: until someone changes it.
    fn period(&self) -> Option<Duration> {
        let secs = if self.playing.is_some() {
            self.entry()?.duration.unwrap_or(self.timer.interval)
        } else if self.timer.enabled {
            self.timer.interval
        } else {
            return None;
        };
        Some(Duration::from_secs(u64::from(
            secs.max(EffectTimer::MIN_INTERVAL),
        )))
    }

    /// Start the period of the effect shown now.
    fn rearm(&mut self, now: Instant) {
        self.due = self.period().map(|d| {
            let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default() + d;
            (now + d, wall.as_millis() as u64)
        });
    }

    fn is_due(&self, now: Instant) -> bool {
        self.due.is_some_and(|(at, _)| now >= at)
    }

    /// xorshift64: enough for shuffling effects.
    fn random(&mut self, below: usize) -> usize {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x % below.max(1) as u64) as usize
    }

    /// A play order for `n` entries; shuffled ones do not start with `avoid`.
    fn order(&mut self, n: usize, shuffle: bool, avoid: Option<usize>) -> Vec<usize> {
        let mut order: Vec<usize> = (0..n).collect();
        if shuffle {
            for i in (1..n).rev() {
                let j = self.random(i + 1);
                order.swap(i, j);
            }
            if n > 1 && avoid.is_some() && order.first().copied() == avoid {
                order.swap(0, n - 1);
            }
        }
        order
    }

    /// Play `playlist` from entry `start` (else from the start of its order).
    fn start(&mut self, playlist: Playlist, start: Option<usize>) {
        let n = playlist.entries.len();
        let mut order = self.order(n, playlist.shuffle, None);
        if let Some(i) = start.filter(|&i| i < n) {
            let at = order.iter().position(|&k| k == i).unwrap_or(0);
            if playlist.shuffle {
                order.swap(0, at);
            } else {
                order.rotate_left(at);
            }
        }
        self.playing = Some(Playing {
            playlist,
            order,
            pos: 0,
        });
    }

    /// The playing playlist was edited: keep the entry shown when it still exists.
    fn replace(&mut self, playlist: Playlist) {
        let Some(current) = self.playback().map(|p| p.entry) else {
            return;
        };
        let keep = (current < playlist.entries.len()).then_some(current);
        self.start(playlist, keep);
    }

    /// Move to the next (or previous) entry; a shuffled playlist gets a new
    /// order at the end of a round. Answers with the entry index.
    fn step(&mut self, back: bool) -> Option<usize> {
        let (n, shuffle, last) = {
            let p = self.playing.as_ref()?;
            (p.order.len(), p.playlist.shuffle, p.order.get(p.pos).copied())
        };
        if n == 0 {
            return None;
        }
        let pos = self.playing.as_ref()?.pos;
        let next = if back {
            (pos + n - 1) % n
        } else if pos + 1 < n {
            pos + 1
        } else {
            if shuffle {
                let order = self.order(n, true, last);
                self.playing.as_mut()?.order = order;
            }
            0
        };
        let p = self.playing.as_mut()?;
        p.pos = next;
        p.order.get(next).copied()
    }
}

/// Shuffle and the timer leave out game integrations (screen meters) and
/// developer effects.
fn eligible(e: &EffectFile) -> bool {
    e.source != EffectSource::Developer && !e.meta.uses.meters
}

/// The id a new playlist named `name` gets: lower-case letters and digits
/// joined by `-`, made unique with a number.
fn new_id(name: &str, taken: &[String]) -> String {
    let mut base = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            base.extend(c.to_lowercase());
        } else if !base.ends_with('-') && !base.is_empty() {
            base.push('-');
        }
    }
    let base = base.trim_end_matches('-');
    let base = if base.is_empty() { "playlist" } else { base };
    let base: String = base.chars().take(48).collect();
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

fn check_seconds(what: &str, secs: u32) -> Result<(), ApiError> {
    if (EffectTimer::MIN_INTERVAL..=EffectTimer::MAX_INTERVAL).contains(&secs) {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "{what} must be {}-{} seconds",
            EffectTimer::MIN_INTERVAL,
            EffectTimer::MAX_INTERVAL
        )))
    }
}

/// A `loadEffect` uid becomes a file name: letters, digits, space, `-`, `_`, `.`.
fn check_uid(uid: &str) -> Result<(), ApiError> {
    let ok = !uid.trim().is_empty()
        && uid.len() <= 64
        && !uid.starts_with('.')
        && uid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "uid: 1-64 letters, digits, spaces, '-', '_' or '.', not starting with '.'",
        ))
    }
}

impl Engine {
    /// The timer and the playback from the database (start, import).
    pub(super) fn load_player(&mut self) {
        let mut timer: EffectTimer = self
            .store
            .get(keys::EFFECT_TIMER)
            .ok()
            .flatten()
            .unwrap_or_default();
        timer.interval = timer
            .interval
            .clamp(EffectTimer::MIN_INTERVAL, EffectTimer::MAX_INTERVAL);
        self.player.timer = timer;
        self.player.playing = None;
        let saved: Option<Saved> = self.store.get(keys::PLAYLIST).ok().flatten();
        if let Some(saved) = saved {
            if let Ok(Some(p)) = self.store.playlist(&saved.id) {
                if !p.entries.is_empty() {
                    self.player.start(p, Some(saved.entry));
                }
            }
        }
        self.player.rearm(Instant::now());
    }

    /// Called every loop turn: switch when the time has come.
    pub(super) fn tick_player(&mut self) {
        let now = Instant::now();
        if !self.player.is_due(now) {
            return;
        }
        if !self.enabled {
            self.player.rearm(now);
            return;
        }
        let result = if self.player.is_playing() {
            self.step_playlist(false)
        } else {
            self.next_eligible()
        };
        if let Err(e) = result {
            tracing::debug!("auto-next: {e}");
            self.player.rearm(now);
        }
        self.dirty = true;
    }

    /// Show an effect and start its period (every effect change goes through here).
    pub(super) fn show_effect(&mut self, id: String, values: Map<String, Value>) -> Result<Value, ApiError> {
        if self.effects.get(&id).is_none() {
            return Err(ApiError::not_found(format!("no effect {id}")));
        }
        if !values.is_empty() {
            self.store.set_effect_params(&id, &values).map_err(internal)?;
        }
        self.store.set(keys::EFFECT, &id).map_err(internal)?;
        self.effect = Some(id.clone());
        self.start_canvas();
        self.player.rearm(Instant::now());
        self.log(format!("effect: {id}"));
        Ok(json!({ "effect": id }))
    }

    /// End the playlist (a user's choice of effect); the effect stays.
    pub(super) fn stop_playlist(&mut self) {
        if self.player.playing.take().is_some() {
            let _ = self.store.set(keys::PLAYLIST, &Value::Null);
            self.log("playlist stopped");
        }
        self.player.rearm(Instant::now());
    }

    /// Show the playlist's entry at the current position; entries whose effect
    /// is missing are skipped (the playlist stops when none is installed).
    fn show_entry(&mut self, back: bool) -> Result<Value, ApiError> {
        let n = self
            .player
            .playing
            .as_ref()
            .map_or(0, |p| p.playlist.entries.len());
        for _ in 0..n.max(1) {
            let Some(entry) = self.player.entry().cloned() else {
                break;
            };
            if self.effects.get(&entry.effect).is_some() {
                let mut values = Map::new();
                if let Some(preset) = &entry.preset {
                    if let Some(v) = self.store.presets(&entry.effect).map_err(internal)?.get(preset) {
                        values.extend(v.clone());
                    }
                }
                values.extend(entry.values.clone());
                if let Some(p) = self.player.playback() {
                    let _ = self.store.set(
                        keys::PLAYLIST,
                        &Saved {
                            id: p.id,
                            entry: p.entry,
                        },
                    );
                }
                return self.show_effect(entry.effect, values);
            }
            self.player.step(back);
        }
        self.stop_playlist();
        Err(ApiError::conflict("None of the playlist's effects is installed"))
    }

    /// Next/previous entry of the playing playlist.
    pub(super) fn step_playlist(&mut self, back: bool) -> Result<Value, ApiError> {
        self.player.step(back);
        self.show_entry(back)
    }

    /// The timer without a playlist: the next eligible effect of the library.
    fn next_eligible(&mut self) -> Result<Value, ApiError> {
        let all: Vec<(String, bool)> = self.effects.iter().map(|e| (e.id.clone(), eligible(e))).collect();
        let at = self
            .effect
            .as_ref()
            .and_then(|cur| all.iter().position(|(id, _)| id == cur));
        let n = all.len();
        let start = at.map_or(0, |i| i + 1);
        let next = (0..n)
            .map(|k| &all[(start + k) % n])
            .find(|(id, ok)| *ok && Some(id) != self.effect.as_ref())
            .map(|(id, _)| id.clone());
        match next {
            Some(id) => self.show_effect(id, Map::new()),
            None => Err(ApiError::conflict("No Next Effect")),
        }
    }

    /// The playlist, timer, shuffle and developer requests.
    pub(super) fn handle_playlist(&mut self, request: Request) -> Result<Value, ApiError> {
        match request {
            Request::ShuffleEffect => {
                let pool: Vec<String> = self
                    .effects
                    .iter()
                    .filter(|e| eligible(e) && Some(&e.id) != self.effect.as_ref())
                    .map(|e| e.id.clone())
                    .collect();
                if pool.is_empty() {
                    return Err(ApiError::conflict("No effect to shuffle to"));
                }
                let id = pool[self.player.random(pool.len())].clone();
                self.stop_playlist();
                self.show_effect(id, Map::new())
            }
            Request::SavePlaylist { id, playlist } => {
                let name = playlist.name.trim().to_string();
                if name.is_empty() || name.chars().count() > MAX_NAME {
                    return Err(ApiError::bad_request(format!(
                        "a playlist needs a name of 1-{MAX_NAME} characters"
                    )));
                }
                for e in &playlist.entries {
                    if e.effect.trim().is_empty() {
                        return Err(ApiError::bad_request("every entry needs an effect"));
                    }
                    if let Some(d) = e.duration {
                        check_seconds("duration", d)?;
                    }
                }
                let id = match id {
                    Some(id) if id.trim().is_empty() || id.chars().count() > MAX_NAME => {
                        return Err(ApiError::bad_request("bad playlist id"))
                    }
                    Some(id) => id,
                    None => {
                        let taken: Vec<String> = self
                            .store
                            .playlists()
                            .map_err(internal)?
                            .into_iter()
                            .map(|p| p.id)
                            .collect();
                        new_id(&name, &taken)
                    }
                };
                let saved = self
                    .store
                    .save_playlist(&Playlist { id, name, ..playlist })
                    .map_err(internal)?;
                if self.player.playback().is_some_and(|p| p.id == saved.id) {
                    if saved.entries.is_empty() {
                        self.stop_playlist();
                    } else {
                        self.player.replace(saved.clone());
                        self.player.rearm(Instant::now());
                    }
                }
                serde_json::to_value(saved).map_err(internal)
            }
            Request::DeletePlaylist { id } => {
                if !self.store.delete_playlist(&id).map_err(internal)? {
                    return Err(ApiError::not_found("The requested playlist was not found"));
                }
                if self.player.playback().is_some_and(|p| p.id == id) {
                    self.stop_playlist();
                }
                Ok(json!({}))
            }
            Request::PlayPlaylist { id, start } => {
                let Some(p) = self.store.playlist(&id).map_err(internal)? else {
                    return Err(ApiError::not_found("The requested playlist was not found"));
                };
                if p.entries.is_empty() {
                    return Err(ApiError::conflict("The playlist is empty"));
                }
                let first = match &start {
                    Some(effect) => match p.entries.iter().position(|e| &e.effect == effect) {
                        Some(i) => Some(i),
                        None => {
                            return Err(ApiError::not_found("The requested effect is not in the playlist"))
                        }
                    },
                    None => None,
                };
                self.log(format!("playlist: {}", p.name));
                self.player.start(p, first);
                self.show_entry(false)?;
                Ok(json!({ "playlist": id, "effect": self.effect }))
            }
            Request::StopPlaylist => {
                let was = self.player.is_playing();
                self.stop_playlist();
                Ok(json!({ "stopped": was }))
            }
            Request::SetEffectTimer { enabled, interval } => {
                if let Some(secs) = interval {
                    check_seconds("interval", secs)?;
                    self.player.timer.interval = secs;
                }
                if let Some(on) = enabled {
                    self.player.timer.enabled = on;
                }
                self.store
                    .set(keys::EFFECT_TIMER, &self.player.timer)
                    .map_err(internal)?;
                self.player.rearm(Instant::now());
                serde_json::to_value(self.player.timer).map_err(internal)
            }
            Request::LoadEffect { uid, html } => {
                check_uid(&uid)?;
                if html.len() > MAX_DEVELOPER_PAGE {
                    return Err(ApiError::bad_request("the page is larger than 4 MiB"));
                }
                let dir = self.opts.paths.developer_effects();
                std::fs::create_dir_all(&dir).map_err(internal)?;
                std::fs::write(dir.join(format!("{uid}.html")), html).map_err(internal)?;
                self.effects.add_dir(&dir, EffectSource::Developer, false);
                self.stop_playlist();
                self.show_effect(format!("{}:{uid}", EffectSource::Developer.prefix()), Map::new())
            }
            Request::EffectImage { id } => {
                let Some(e) = self.effects.get(&id) else {
                    return Err(ApiError::not_found("The requested effect was not found"));
                };
                match e.image() {
                    Some((path, mime)) => Ok(json!({ "path": path, "contentType": mime })),
                    None => Err(ApiError::not_found("The requested effect has no image")),
                }
            }
            other => Err(ApiError::internal(format!(
                "{} is not a playlist request",
                other.name()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlist(n: usize, shuffle: bool) -> Playlist {
        Playlist {
            id: "p".into(),
            name: "P".into(),
            shuffle,
            entries: (0..n)
                .map(|i| PlaylistEntry {
                    effect: format!("e{i}"),
                    duration: Some(10 + i as u32),
                    ..PlaylistEntry::default()
                })
                .collect(),
            ..Playlist::default()
        }
    }

    #[test]
    fn an_ordered_playlist_loops_and_steps_back() {
        let mut p = Player::default();
        p.start(playlist(3, false), Some(1));
        assert_eq!(p.playback().unwrap().entry, 1);
        assert_eq!(p.step(false), Some(2));
        assert_eq!(p.step(false), Some(0));
        assert_eq!(p.step(true), Some(2));
        assert_eq!(p.period(), Some(Duration::from_secs(12)));
    }

    #[test]
    fn a_shuffled_playlist_plays_every_entry_once_per_round() {
        let mut p = Player::default();
        p.start(playlist(6, true), Some(4));
        assert_eq!(p.playback().unwrap().entry, 4);
        for _ in 0..5 {
            let mut seen = vec![p.playback().unwrap().entry];
            for _ in 0..5 {
                seen.push(p.step(false).unwrap());
            }
            let last = *seen.last().unwrap();
            seen.sort();
            assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
            // A new round does not repeat the entry just shown.
            assert_ne!(p.step(false), Some(last));
        }
    }

    #[test]
    fn the_timer_runs_only_when_enabled_or_while_a_playlist_plays() {
        let mut p = Player::default();
        assert_eq!(p.timer().interval, 300);
        let now = Instant::now();
        p.rearm(now);
        assert!(p.next_switch_ms().is_none() && !p.is_due(now + Duration::from_secs(9999)));
        p.timer.enabled = true;
        p.rearm(now);
        assert!(p.is_due(now + Duration::from_secs(300)) && !p.is_due(now + Duration::from_secs(299)));
        p.timer.enabled = false;
        let mut list = playlist(2, false);
        list.entries[1].duration = None;
        p.start(list, Some(1));
        p.rearm(now);
        assert_eq!(
            p.period(),
            Some(Duration::from_secs(300)),
            "no duration: the interval"
        );
        assert!(p.next_switch_ms().is_some());
    }

    #[test]
    fn editing_the_playing_playlist_keeps_the_entry() {
        let mut p = Player::default();
        p.start(playlist(3, false), Some(2));
        p.replace(playlist(4, false));
        assert_eq!(p.playback().unwrap().entry, 2);
        p.replace(playlist(2, false));
        assert_eq!(p.playback().unwrap().entry, 0);
    }

    #[test]
    fn ids_come_from_names() {
        assert_eq!(new_id("Evening Chill!", &[]), "evening-chill");
        assert_eq!(new_id("  ", &[]), "playlist");
        let taken = vec!["mix".to_string(), "mix-2".to_string()];
        assert_eq!(new_id("Mix", &taken), "mix-3");
    }

    #[test]
    fn developer_uids_are_plain_file_names() {
        assert!(check_uid("my effect_1.2").is_ok());
        for bad in ["", "../x", ".hidden", "a/b", "a\\b", "c:d", &"x".repeat(65)] {
            assert!(check_uid(bad).is_err(), "{bad}");
        }
        assert!(check_seconds("interval", 0).is_err());
        assert!(check_seconds("interval", 300).is_ok());
    }
}
