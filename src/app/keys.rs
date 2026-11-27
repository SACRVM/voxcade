//! The maker's keys, the window's alone (`voxcade run --maker`); what the
//! other keys and buttons mean before the game hears them is the front
//! end's (`crate::front::keys`), shared with the IDE's PLAY.

use sdl3::keyboard::Scancode;

/// The maker's keys: F5, F9 and the number keys that pick a song.
pub(super) const MAKER_KEYS: [Scancode; 2] = [Scancode::F5, Scancode::F9];

/// `code` if it is a maker's key the window answers: only behind the maker
/// switch (`voxcade run --maker`), F5 also in a replay being watched (it
/// plays it again). Without it nothing here is taken, so a player's keys
/// are the game's alone.
pub(super) fn maker_key(code: Scancode, maker: bool, watching: bool) -> Option<Scancode> {
    MAKER_KEYS.contains(&code).then_some(code).filter(|&c| maker || (watching && c == Scancode::F5))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player of a packaged game has no maker's keys: F5 (restart on the
    /// files), F9 (keep the run) and the number keys (the song) are the
    /// game's; `--maker` brings them, and a replay watched may be played
    /// again with F5.
    #[test]
    fn the_makers_keys_need_the_maker_switch() {
        for key in MAKER_KEYS {
            assert_eq!(maker_key(key, false, false), None, "{key:?} works for a player");
            assert_eq!(maker_key(key, true, false), Some(key));
        }
        assert_eq!(maker_key(Scancode::F5, false, true), Some(Scancode::F5), "F5 plays a replay again");
        assert_eq!(maker_key(Scancode::F9, false, true), None);
        assert_eq!(maker_key(Scancode::F11, true, false), None, "fullscreen is everyone's, not a maker key");
    }
}
