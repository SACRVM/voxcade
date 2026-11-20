//! The shooter's part of the config: `c.weapons` (the aim assist and the
//! weapons, `c.weapons.NAME`) and its keys in `c.player`. Its kinds of
//! things are the engine's `[things.NAME]` with the shooter's behaviors;
//! `kinds` reads both into the rules. The engine leaves these to the game
//! (`GameConfig::game`, `PlayerConfig::game`); read here, a section or key
//! the shooter does not know is an error.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::config::GameConfig;

#[derive(Clone, Debug, Default)]
pub struct ShooterConfig {
    pub player: PlayerConfig,
    /// `[weapons]` as the game wrote it: `aim_assist` and a table for each
    /// weapon it adds or changes (`kinds::Rules::from_game`).
    pub weapons: toml::Table,
}

impl ShooterConfig {
    /// Its sections, by name (the fingerprint of a game's data takes them in).
    pub const SECTIONS: [&str; 1] = ["weapons"];

    /// Reads them from what the engine left in `cfg`.
    pub fn from_game(cfg: &GameConfig) -> Result<Self> {
        if let Some(other) = cfg.game.keys().find(|k| !Self::SECTIONS.contains(&k.as_str())) {
            bail!("conf.lua: no section c.{other} (the shooter reads c.weapons; kinds of things are c.things.NAME)");
        }
        if let Some((key, now)) = MOVED.iter().find(|(k, _)| cfg.player.game.contains_key(*k)) {
            bail!("conf.lua: c.player.{key} is no more: {now}");
        }
        let player: PlayerConfig = toml::Value::Table(cfg.player.game.clone()).try_into().context("conf.lua: c.player")?;
        if player.facing == Some([0.0, 0.0]) {
            bail!("conf.lua: c.player.facing: a way such as {{ 0, 1 }} (north)");
        }
        let weapons = match cfg.game.get("weapons") {
            None => toml::Table::new(),
            Some(toml::Value::Table(t)) => t.clone(),
            Some(_) => bail!("conf.lua: c.weapons is a table"),
        };
        Ok(Self { player, weapons })
    }
}

/// The shooter's keys in `[player]`, beside the engine's movement. Each
/// field's words are what `voxcade api` and the editors show for the key,
/// so each stands alone.
#[derive(Deserialize, serde::Serialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerConfig {
    /// Where shots leave the gun, { ahead, up } from the middle of the feet in voxels. Unset: just in front of the body at half its height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muzzle: Option<[f32; 2]>,
    /// Ticks after a hit during which nothing hurts the player (0: none).
    pub invulnerable: u32,
    /// What each of the game's actions attacks with, by the action's name: its weapons in order, the first that can. A weapon is a name of c.weapons, or "held", the gun in hand (player.state.weapon). An action named here is the ready-made rules'; one not named is your code's.
    pub attacks: std::collections::BTreeMap<String, Vec<String>>,
    /// The way the player faces as a run starts, { x, y } (x east, y north). Unset: south, { 0, -1 }.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facing: Option<[f32; 2]>,
    /// The moves the player has, by the action that does each: "jump", "strafe". None unless named.
    pub moves: Vec<String>,
    /// The player's model's frames for what it does (idle, walk, jump), as a kind's (c.things.NAME.frames).
    pub frames: toml::Table,
    /// How many frames a second the player's sets go where a set does not say its own, as a kind's fps. Unset: 10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f32>,
    /// For a model of parts: the parts that swing as it walks, with how many degrees, as a kind's (c.things.NAME.swing).
    pub swing: std::collections::BTreeMap<String, f32>,
    /// While nothing hurts the player it blinks: shown this many ticks, then hidden as many (0: it does not blink).
    pub blink: u32,
    /// A puff of dust rises every this many voxels the player walks. Unset: half its height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stride: Option<f32>,
    /// How many specks of dust each puff of its steps has (0: no dust).
    pub step_dust: u32,
    /// Landing from higher than this, in voxels, raises a puff of dust.
    pub landing: f32,
    /// How many specks of dust a landing puff has.
    pub landing_dust: u32,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        let steps = super::kinds::Steps::default();
        Self {
            muzzle: None,
            invulnerable: 0,
            attacks: Default::default(),
            facing: None,
            moves: Vec::new(),
            frames: toml::Table::new(),
            fps: None,
            swing: Default::default(),
            blink: 4,
            stride: steps.stride,
            step_dust: steps.step_dust,
            landing: steps.landing,
            landing_dust: steps.landing_dust,
        }
    }
}

impl PlayerConfig {
    /// The dust its steps and landings raise.
    pub fn steps(&self) -> super::kinds::Steps {
        super::kinds::Steps { stride: self.stride, step_dust: self.step_dust, landing: self.landing, landing_dust: self.landing_dust }
    }
}

/// Keys of `c.player` the shooter read before and where they went.
const MOVED: [(&str, &str); 3] = [
    // why: c.player's old key, named in its hint
    ("knife", "the knife is a weapon of its own: c.weapons.knife = { reach = 13, cone = 145, damage = 4, noise = 0 }, and c.player.attacks = { fire = { \"knife\", \"held\" } } uses it when an enemy stands close"),
    ("weapons", "the gun in hand is player.state.weapon (c.state.player.weapon = \"rifle\" to start), and c.player.attacks = { fire = { \"held\" } } fires it; its rounds are the inventory's"),
    // why: c.player's old key, named in its hint
    ("grenade", "what an action throws is its attack: c.player.attacks = { grenade = { \"grenade\" } }"),
];


#[cfg(test)]
mod tests {
    use super::*;

    /// The engine leaves the shooter's sections and its keys in `[player]`
    /// to it; a section or key nobody reads is still a mistake that shows,
    /// the old sections too.
    #[test]
    fn a_section_nobody_reads_is_an_error() {
        let read = |text: &str| ShooterConfig::from_game(&toml::from_str(text).unwrap());
        let own = read("[player]\nspeed = 50\ninvulnerable = 9\n[weapons]\naim_assist = false\n").unwrap();
        assert_eq!((own.player.invulnerable, own.weapons.get("aim_assist").and_then(|v| v.as_bool())), (9, Some(false)));
        let cfg: GameConfig = toml::from_str("[player]\nspeed = 50\ninvulnerable = 9\n").unwrap();
        assert_eq!(cfg.player.speed, 50.0, "the engine lost its own key");
        assert!(read("[weapon]\naim_assist = false\n").is_err(), "a misspelled section passed");
        assert!(read("[player]\nknif = 9\n").is_err(), "a misspelled key passed");
        assert!(read("[player]\nknife = 9\n").unwrap_err().to_string().contains("c.weapons.knife"), "an old key without its new place");
        assert!(read("[enemies]\nhp = 2\n").is_err(), "an old section passed");
    }
}
