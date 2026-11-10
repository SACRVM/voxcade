//! A Lua game's save (`docs/game-model.md`, "Save games") as the text a
//! slot holds: TOML a person can read, the states in it as Lua tables
//! (`data`). What it holds: the run's states, every level's `level.kept`
//! and visits, the level being played with its `level.state`, where the
//! player stands, the numbers the engine keeps for the player, the
//! ready-made rules' run, the time and the random generator, and the
//! level's voxels its file would not bring back. The level itself is read
//! from its file when the save is loaded, so a save outlives updates of
//! the game; a name it holds that the game no longer has is dropped with a
//! note, never a failed load.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use glam::IVec3;
use serde::{Deserialize, Serialize};

/// The engine's save format.
pub const FORMAT: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SaveData {
    /// The engine's save format (`FORMAT`).
    pub format: u32,
    /// The game's title, for people reading the file.
    pub game: String,
    /// The game's own version (`c.saves.version`) it was made with.
    pub version: u32,
    /// The level being played, by its name.
    pub level: String,
    /// The play time: ticks since the run began (`world.tick`).
    pub ticks: u64,
    /// The random generator's state (hex).
    pub rng: String,
    pub states: States,
    /// Each level's `level.kept`, by the level's name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub kept: BTreeMap<String, String>,
    /// How often each level was entered, by its name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub visits: BTreeMap<String, u32>,
    /// The player's numbers the engine keeps (`player.state.score`, and
    /// with ready-made rules `lives`, `hp`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub numbers: BTreeMap<String, i64>,
    /// Where the player's figure stood, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<Figure>,
    /// The seats after the first someone sat on, and seat 1's own keys
    /// where set (`docs/game-model.md`, "Several players").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<SavedSeat>,
    /// The ready-made rules' own run (the shooter's weapon and ammo).
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub run: toml::Table,
    /// The level's voxels its file would not bring back (`voxels`), counted
    /// in a world of `size` (the level's then; a level resized since takes
    /// none of them).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub voxels: String,
    #[serde(default)]
    pub size: [i32; 3],
    /// Its colours that differ from its file's, by palette index:
    /// "#rrggbbaa".
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub palette: BTreeMap<String, String>,
}

/// The states, each a Lua table (`data::write`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct States {
    pub world: String,
    pub player: String,
    pub level: String,
}

/// A seat in a save: its player's state (as `States` holds them), the
/// engine's keys, and where its figure stood.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SavedSeat {
    pub seat: usize,
    /// Someone sat there as it was saved (else its state waits for them).
    #[serde(default)]
    pub taken: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default)]
    pub score: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp: Option<i32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<Figure>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Figure {
    /// Its feet, in voxels.
    pub at: [f32; 3],
    /// The way it faced.
    pub facing: [i32; 2],
}

impl SaveData {
    pub fn read(text: &str) -> Result<Self> {
        let save: SaveData = toml::from_str(text).context("the save does not read")?;
        if save.format > FORMAT {
            bail!("the save was made by a newer Voxcade (format {}, this one reads {FORMAT})", save.format);
        }
        Ok(save)
    }

    pub fn write(&self) -> Result<String> {
        Ok(toml::to_string(self)?)
    }
}

/// Changed voxels as text: runs of cells (counted x, then y, then z in a
/// world of `size`), each the cells skipped since the last run, its length
/// and its palette indices, as hex.
pub fn write_voxels(changed: &[(IVec3, u8)], size: IVec3) -> String {
    let index = |p: IVec3| p.x as u64 + p.y as u64 * size.x as u64 + p.z as u64 * size.x as u64 * size.y as u64;
    let mut bytes = Vec::new();
    let mut end = 0u64;
    let mut i = 0;
    while i < changed.len() {
        let from = index(changed[i].0);
        let mut run = vec![changed[i].1];
        while i + run.len() < changed.len() && index(changed[i + run.len()].0) == from + run.len() as u64 {
            run.push(changed[i + run.len()].1);
        }
        varint(&mut bytes, from - end);
        varint(&mut bytes, run.len() as u64);
        bytes.extend_from_slice(&run);
        end = from + run.len() as u64;
        i += run.len();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The cells `write_voxels` wrote, in a world of `size`.
pub fn read_voxels(text: &str, size: IVec3) -> Result<Vec<(IVec3, u8)>> {
    anyhow::ensure!(text.len().is_multiple_of(2) && text.bytes().all(|b| b.is_ascii_hexdigit()), "the level's voxels do not read");
    let bytes: Vec<u8> = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digits")).collect();
    let (sx, sy) = (size.x as u64, size.y as u64);
    let cells = sx * sy * size.z as u64;
    let mut at = 0;
    let mut end = 0u64;
    let mut out = Vec::new();
    while at < bytes.len() {
        let gap = read_varint(&bytes, &mut at)?;
        let len = read_varint(&bytes, &mut at)?;
        let (from, run_end) = (end.checked_add(gap), end.checked_add(gap).and_then(|f| f.checked_add(len)));
        let (Some(from), Some(run_end)) = (from, run_end) else { bail!("the level's voxels reach outside it") };
        let len = len as usize;
        anyhow::ensure!(at.checked_add(len).is_some_and(|e| e <= bytes.len()) && run_end <= cells, "the level's voxels reach outside it");
        for (n, &v) in bytes[at..at + len].iter().enumerate() {
            let i = from + n as u64;
            out.push((IVec3::new((i % sx) as i32, (i / sx % sy) as i32, (i / (sx * sy)) as i32), v));
        }
        at += len;
        end = from + len as u64;
    }
    Ok(out)
}

fn varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            return out.push(b);
        }
        out.push(b | 0x80);
    }
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Result<u64> {
    let mut n = 0u64;
    for shift in (0..64).step_by(7) {
        let Some(&b) = bytes.get(*at) else { bail!("the level's voxels end too soon") };
        *at += 1;
        n |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(n);
        }
    }
    bail!("the level's voxels do not read")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Changed voxels come back as they were written, runs and gaps alike.
    #[test]
    fn changed_voxels_read_back() {
        let size = IVec3::new(40, 30, 20);
        let changed = vec![(IVec3::new(0, 0, 0), 3), (IVec3::new(1, 0, 0), 0), (IVec3::new(39, 0, 0), 7), (IVec3::new(0, 1, 0), 7), (IVec3::new(5, 29, 19), 255)];
        let text = write_voxels(&changed, size);
        assert_eq!(read_voxels(&text, size).unwrap(), changed);
        assert!(read_voxels(&text, IVec3::new(4, 4, 4)).is_err(), "too big for a smaller world");
        assert_eq!(read_voxels("", size).unwrap(), Vec::new());
    }

    /// Runs that would reach past the end of numbers are refused, not
    /// wrapped.
    #[test]
    fn hostile_voxels_are_refused() {
        let size = IVec3::new(4, 4, 4);
        for text in ["ffffffffffffffffff01ffffffffffffffffff0100", "00ffffffffffffffff7f", "0005010203"] {
            assert!(read_voxels(text, size).is_err(), "{text}");
        }
    }
}
