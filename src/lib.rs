//! Voxcade: a voxel arcade engine. Fixed camera, full-screen destructible
//! worlds, levels made in ordinary voxel editors. A library: the `voxcade`
//! program runs game folders with it, and a game written in Rust uses it
//! directly (`docs/engine.md`, "One API, two doors").

pub mod app;
pub mod audio;
pub mod blocks;
pub mod camera;
pub mod cli;
pub mod config;
pub mod editor;
pub mod files;
pub mod controls;
pub mod desktop;
pub mod font;
pub mod game;
pub mod grid;
pub mod session;
pub mod hash;
pub mod headless;
pub mod gui;
pub mod ide;
pub mod input;
pub mod lang;
pub mod levels;
pub mod lua;
pub mod map;
pub mod mcp;
pub mod menu;
pub mod mesh;
pub mod names;
pub mod network;
pub mod net;
pub mod pads;
pub mod par;
pub mod prompt;
pub mod platformer;
pub mod render;
pub mod replay;
pub mod rng;
pub mod saves;
pub mod seating;
pub mod settings;
pub mod shooter;
pub mod sim;
pub mod update;
pub mod tiled;
pub mod ui;
pub mod vox;
pub mod voxtext;
pub mod watch;
pub mod window;
pub mod world;

/// The `voxcade` program's game: a Lua game, with the shooter or the
/// platformer kit when its `conf.lua` asks for one (`c.kit = "shooter"`,
/// `"platformer"`).
pub type Voxcade = lua::LuaGame<lua::host::Either<shooter::Shooter, platformer::Platformer>>;
