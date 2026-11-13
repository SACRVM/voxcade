//! The shooter's seats: a game of several, tried through the shooter template.

use std::path::Path;

use crate::game::Game;
use crate::input::Input;
use crate::session::Session;

/// A tool's `--at` puts every player there in a game of several, though the
/// figures do not stand until the first tick: seat 1's where it was told,
/// seat 2's beside it.
#[test]
fn at_puts_both_players_there_in_a_game_of_two() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/shooter");
    let mut g = Session::<crate::Voxcade>::load(&dir).unwrap();
    g.game.place_player(60.0, 100.0);
    g.set_seat(2, Some(Input::default()));
    for _ in 0..5 {
        g.step(&Input::default());
    }
    assert!(g.trouble().is_none(), "{:?}", g.trouble());
    let state = g.game.inspect();
    let (one, two) = (&state["player"], &state["players"][0]);
    for (seat, at) in [(1, one), (2, two)] {
        let (x, y) = (at["x"].as_f64().unwrap(), at["y"].as_f64().unwrap());
        assert!((x - 60.0).abs() < 12.0 && (y - 100.0).abs() < 12.0, "seat {seat} at {x}, {y}, not at 60, 100");
    }
}
