// SPDX-License-Identifier: GPL-3.0-or-later
//! The `custom_leds` table: every endpoint's per-LED custom colours, mode
//! and zones ([`CustomLeds`]), keyed by endpoint uid, with the key of the
//! device the endpoint belongs to.

use super::{to_json, Store};
use anyhow::Context;
use lightloom_engine::CustomLeds;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One endpoint's custom LEDs as stored and exported.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomLedsRow {
    /// Key of the device the endpoint belongs to.
    pub device: String,
    /// Mode, colours and zones.
    #[serde(flatten)]
    pub state: CustomLeds,
}

impl Store {
    /// Every endpoint's custom LEDs, by endpoint uid.
    pub fn custom_leds(&self) -> anyhow::Result<BTreeMap<String, CustomLedsRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT uid, device, state FROM custom_leds ORDER BY uid")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(uid, device, state)| {
                let state: CustomLeds =
                    serde_json::from_str(&state).with_context(|| format!("custom LEDs of {uid}"))?;
                Ok((uid, CustomLedsRow { device, state }))
            })
            .collect()
    }

    /// Store an endpoint's custom LEDs; a blank state (off, black, no zones) deletes the row.
    pub fn set_custom_leds(&self, uid: &str, device: &str, state: &CustomLeds) -> anyhow::Result<()> {
        if state.is_blank() {
            self.conn
                .execute("DELETE FROM custom_leds WHERE uid = ?1", [uid])?;
            return Ok(());
        }
        Self::insert_custom_leds(&self.conn, uid, device, state)
    }

    pub(super) fn insert_custom_leds(
        conn: &Connection,
        uid: &str,
        device: &str,
        state: &CustomLeds,
    ) -> anyhow::Result<()> {
        conn.execute(
            "INSERT INTO custom_leds (uid, device, state) VALUES (?1, ?2, ?3)
             ON CONFLICT(uid) DO UPDATE SET device = excluded.device, state = excluded.state",
            params![uid, device, to_json(state)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_engine::Rgb;

    #[test]
    fn custom_leds_round_trip_and_blank_states_go() {
        let dir = std::env::temp_dir().join(format!("lightloom-store-leds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state.sqlite");
        let mut state = CustomLeds {
            enabled: true,
            colors: vec![Rgb::new(255, 0, 0), Rgb::new(0, 0, 255)],
            ..CustomLeds::default()
        };
        state.save_zone("WASD", &[1]).unwrap();
        {
            let s = Store::open(&path).unwrap();
            s.set_custom_leds("1:2:K", "1:2:K", &state).unwrap();
            s.set_custom_leds("1:2:K:Logo", "1:2:K", &CustomLeds::default())
                .unwrap();
        }
        let s = Store::open(&path).unwrap();
        let all = s.custom_leds().unwrap();
        assert_eq!(all.len(), 1, "the blank state was not stored");
        assert_eq!(
            all["1:2:K"],
            CustomLedsRow {
                device: "1:2:K".into(),
                state: state.clone()
            }
        );
        // Turning it off keeps the colours; clearing everything deletes the row.
        state.enabled = false;
        s.set_custom_leds("1:2:K", "1:2:K", &state).unwrap();
        assert!(!s.custom_leds().unwrap()["1:2:K"].state.enabled);
        s.set_custom_leds("1:2:K", "1:2:K", &CustomLeds::default())
            .unwrap();
        assert!(s.custom_leds().unwrap().is_empty());
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
