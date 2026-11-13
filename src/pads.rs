//! `voxcade pads`: which controllers SDL sees (and whether it knows them as
//! gamepads), then what they send for a few seconds. For when a controller
//! does nothing in the game.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use sdl3::event::Event;
use sdl3::gamepad::Gamepad;
use sdl3::joystick::JoystickId;
use sdl3::GamepadSubsystem;

/// How long a pad plugged in while a window runs is left to settle.
const SETTLE: Duration = Duration::from_secs(2);
/// A pad that would not open is tried again so often, so many times.
const RETRY: Duration = Duration::from_secs(1);
const TRIES: u32 = 10;

/// The gamepads a window plays with: every one plugged in at the start,
/// and one plugged in later once it has settled. A controller that has
/// just connected is still being set up by the system's own drivers (a
/// Nintendo pad over Bluetooth by Linux's hid-nintendo, and by Steam if it
/// runs), and SDL setting it up at the same time left it silent: it
/// worked only when plugged in before the game started.
pub struct Pads {
    sub: Option<GamepadSubsystem>,
    pub open: Vec<Gamepad>,
    /// Pads plugged in, when to try opening each, and how often it failed.
    waiting: Vec<(JoystickId, Instant, u32)>,
}

impl Pads {
    pub fn new(sdl: &sdl3::Sdl) -> Self {
        let sub = sdl.gamepad().map_err(|e| eprintln!("no gamepads: {e}")).ok();
        let mut pads = Self { sub, open: Vec::new(), waiting: Vec::new() };
        let ids = pads.sub.as_ref().and_then(|s| s.gamepads().ok()).unwrap_or_default();
        for id in ids {
            pads.waiting.push((id, Instant::now(), 0));
        }
        pads.settle();
        pads
    }

    /// A pad plugged in (SDL's `GamepadAdded`).
    pub fn added(&mut self, id: JoystickId) {
        if !self.open.iter().any(|p| p.id().ok() == Some(id)) && !self.waiting.iter().any(|(w, ..)| *w == id) {
            self.waiting.push((id, Instant::now() + SETTLE, 0));
        }
    }

    /// A pad unplugged (`GamepadRemoved`).
    pub fn removed(&mut self, id: JoystickId) {
        self.open.retain(|p| p.id().ok() != Some(id));
        self.waiting.retain(|(w, ..)| *w != id);
    }

    /// Opens the pads whose time has come; called every frame.
    pub fn settle(&mut self) {
        let Some(sub) = &self.sub else { return };
        let now = Instant::now();
        let open = &mut self.open;
        self.waiting.retain_mut(|(id, at, failed)| {
            if *at > now {
                return true;
            }
            let name = sub.name_for_id(*id).unwrap_or_else(|_| "?".into());
            match sub.open(*id) {
                Ok(pad) => {
                    println!("gamepad: {name}");
                    open.push(pad);
                    false
                }
                Err(e) => {
                    *failed += 1;
                    *at = now + RETRY;
                    if *failed >= TRIES {
                        eprintln!("gamepad {name} does not open: {e}");
                    }
                    *failed < TRIES
                }
            }
        });
    }

    /// When the next pad waiting is to be opened (a window that sleeps
    /// until something happens wakes then).
    pub fn wakes_at(&self) -> Option<Instant> {
        self.waiting.iter().map(|(_, at, _)| *at).min()
    }

    /// The ids of the pads plugged in and open.
    pub fn ids(&self) -> Vec<u32> {
        self.open.iter().filter_map(|p| p.id().ok()).map(|id| id.raw()).collect()
    }

    /// The pads plugged in by the names the settings keep them by (the
    /// PLAYERS page's plan): their own, a second of the same name
    /// "NAME#2", in the order they came.
    pub fn named(&self) -> Vec<(u32, String)> {
        let mut out: Vec<(u32, String)> = Vec::new();
        for pad in &self.open {
            let Ok(id) = pad.id() else { continue };
            let name = pad.name().unwrap_or_else(|| "Gamepad".into());
            let before = out.iter().filter(|(_, n)| n.split('#').next() == Some(name.as_str())).count();
            out.push((id.raw(), if before == 0 { name } else { format!("{name}#{}", before + 1) }));
        }
        out
    }
}

pub fn pads(seconds: f32) -> Result<()> {
    // No window here, so let controller input through anyway.
    sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    let sdl = sdl3::init().context("initializing SDL")?;
    let joysticks = sdl.joystick().context("SDL joysticks")?;
    let gamepads = sdl.gamepad().context("SDL gamepads")?;
    let mut events = sdl.event_pump().context("SDL event pump")?;
    println!("SDL {}", sdl3::version::version());

    // Devices arrive as events at start; give them a moment.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(300) {
        for _ in events.poll_iter() {}
        std::thread::sleep(Duration::from_millis(20));
    }

    let ids = joysticks.joysticks().unwrap_or_default();
    if ids.is_empty() {
        println!("no controllers: SDL sees no joystick at all");
    }
    let mut open = Vec::new();
    for id in ids {
        let name = gamepads.name_for_id(id).unwrap_or_else(|_| "?".into());
        if gamepads.is_gamepad(id) {
            println!("gamepad  {id:?}: {name} ({:?})", gamepads.type_for_id(id));
            match gamepads.open(id) {
                Ok(pad) => open.push(pad),
                Err(e) => println!("  cannot open it: {e}"),
            }
        } else {
            match joysticks.open(id) {
                Ok(j) => println!(
                    "joystick {id:?}: {} ({} buttons, {} axes) - SDL has no gamepad mapping for it, so games do not see it as a gamepad",
                    j.name(),
                    j.num_buttons(),
                    j.num_axes()
                ),
                Err(e) => println!("joystick {id:?}: cannot open it: {e}"),
            }
        }
    }

    if seconds <= 0.0 {
        return Ok(());
    }
    println!("press buttons, move the sticks ({seconds} s):");
    let until = Instant::now() + Duration::from_secs_f32(seconds);
    while Instant::now() < until {
        for event in events.poll_iter() {
            match event {
                Event::GamepadAdded { which, .. } => println!("  gamepad {which:?} connected"),
                Event::GamepadRemoved { which, .. } => println!("  gamepad {which:?} gone"),
                Event::JoyDeviceAdded { which, .. } => println!("  joystick {which:?} connected"),
                Event::GamepadButtonDown { which, button, .. } => println!("  gamepad {which:?}: {button:?}"),
                Event::GamepadAxisMotion { which, axis, value, .. } if value.unsigned_abs() > 16000 => {
                    println!("  gamepad {which:?}: {axis:?} {value}")
                }
                Event::JoyButtonDown { which, button_idx, .. } => println!("  joystick {which:?}: button {button_idx}"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(open);
    Ok(())
}
