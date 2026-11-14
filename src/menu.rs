//! The menus: the title, pause, settings (sound, picture, controls) pages, the high score
//! table and the entering of a new high score's initials, what their items
//! are and what choosing them does. Pure logic, steered by `Nav`
//! from any device; `ui` draws it and `app` carries out the commands.

use crate::controls::{Device, Nav, PadStyle};
use crate::seating::Keys;
use crate::settings::{Settings, VOLUME_STEPS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Title,
    Pause,
    /// The settings: a short page leading to the ones below (the user's
    /// wish: the one page of every setting was too full), and LANGUAGE.
    Settings,
    /// The volumes and the song.
    Audio,
    /// The picture: fullscreen, shadows, blur, shake, flashes, the frame rate.
    Video,
    /// The keys and buttons of a game of one, and rumble.
    Controls,
    /// The three letters of a new high score, picked like at an arcade
    /// machine (`Menu::entry`).
    Initials,
    /// The high score table (a new line blinking: `Menu::highlight`).
    HighScores,
    /// The save slots, to save into or load from (`Menu::slots`).
    Save,
    Load,
    /// The CONTROLS of a game of several, who plays with what
    /// (`Menu::seats`): each seat's part of the keyboard, its pad, its
    /// keys; swapping the first two; rumble.
    Players,
    /// One seat's player's keys and buttons: its part of the keyboard's
    /// (a half's directions too) and its pad's.
    PlayerControls(usize),
    /// A network game (`docs/multiplayer.md`, stage 4): HOST or JOIN, the
    /// host's address typed (`Menu::address`), and waiting for the host
    /// and the run so far.
    Network,
    /// The games found on the LAN (`Menu::found`) and typing an address,
    /// or a game's code (`Menu::code`, across the internet).
    Find,
    Join,
    Code,
    Joining,
}

/// The letters initials are made of, in the order up steps through them.
pub const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ.";

/// A new high score's initials being entered.
#[derive(Clone, Debug)]
pub struct Entry {
    pub score: u32,
    /// Where it stands in the table (0: the best).
    pub rank: usize,
    pub letters: [char; 3],
    /// The letter being picked.
    pub at: usize,
    /// Whose score it is, in a game of several players (each enters theirs
    /// in turn).
    pub seat: Option<usize>,
}

impl Default for Entry {
    fn default() -> Self {
        Self { score: 0, rank: 0, letters: ['A'; 3], at: 0, seat: None }
    }
}

impl Entry {
    pub fn initials(&self) -> String {
        self.letters.iter().collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// The title's: going on from the newest save (a game with slots).
    Continue,
    Start,
    Settings,
    Quit,
    Resume,
    Restart,
    ToTitle,
    /// The settings' pages.
    Audio,
    Video,
    Music,
    Sound,
    Song,
    Fullscreen,
    Shake,
    Flashes,
    Blur,
    Shadows,
    /// How often the picture is drawn (`FrameRate`).
    FrameRate,
    ShowFps,
    Rumble,
    /// The game's language (a game that speaks more than one).
    Language,
    /// The keys and buttons: in a game of several, who plays with what
    /// (`Page::Players`).
    Controls,
    /// Binding one of `Settings::actions` (the game's and pause).
    Bind(&'static str),
    Reset,
    Back,
    /// The pause page's LEAVE for a player after the first (its seat free).
    Leave,
    /// The pause page's SAVE and LOAD (a game with save slots).
    SaveGame,
    LoadGame,
    /// A save slot, from 0 (`Menu::slots`).
    Slot(usize),
    /// A seat's part of the keyboard, its pad, its KEYS page (the
    /// players page, from seat 1).
    SeatKeys(usize),
    SeatPad(usize),
    SeatControls(usize),
    /// The first two seats change devices.
    Swap,
    /// The title's NETWORK (a game of several), its HOST (on the LAN), HOST
    /// ON THE INTERNET and JOIN.
    Network,
    Host,
    HostNet,
    Join,
    /// The host's pause page: drop a machine gone quiet (its number); let
    /// in a machine asking in across the internet (its connection); copy
    /// the code or the ticket again.
    Drop(u8),
    LetIn(u32),
    CopyCode,
    CopyTicket,
    /// A game found on the LAN or on the internet (`Menu::found`), any
    /// game on the internet with a free seat, typing a code or an
    /// address, and joining by the ticket or code on the clipboard.
    Found(usize),
    AnyGame,
    TypeCode,
    TypeAddress,
    PasteTicket,
}

/// A seat as the PLAYERS page shows it: its part of the keyboard, its
/// pad's name ("" for none), whether someone sits there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeatLine {
    pub keyboard: Keys,
    pub pad: String,
    pub taken: bool,
}

impl SeatLine {
    /// Every seat of a seating (`named`: the pads plugged in by name, a
    /// second of the same name numbered).
    pub fn all(seating: &crate::seating::Seating, named: &[(u32, String)]) -> Vec<SeatLine> {
        (1..=seating.seats())
            .map(|n| {
                let pad = seating.pad(n).and_then(|id| named.iter().find(|(p, _)| *p == id)).map_or(String::new(), |(_, name)| name.replace('#', " "));
                SeatLine { keyboard: seating.keys(n), pad, taken: seating.taken(n) }
            })
            .collect()
    }
}

impl Page {
    /// The page's items; the controls page has one for each of `actions`
    /// (the game's and pause, `Settings::actions`).
    pub fn items(self, actions: &[&'static str]) -> Vec<Item> {
        match self {
            Page::Title => vec![Item::Start, Item::Settings, Item::Quit],
            Page::Pause => vec![Item::Resume, Item::Settings, Item::Restart, Item::ToTitle],
            Page::Settings => vec![Item::Audio, Item::Video, Item::Controls, Item::Back],
            Page::Audio => vec![Item::Music, Item::Sound, Item::Song, Item::Back],
            Page::Video => vec![Item::Fullscreen, Item::Shadows, Item::FrameRate, Item::Blur, Item::Shake, Item::Flashes, Item::ShowFps, Item::Back],
            Page::Controls => actions.iter().map(|&a| Item::Bind(a)).chain([Item::Rumble, Item::Reset, Item::Back]).collect(),
            Page::Network => vec![Item::Host, Item::HostNet, Item::Join, Item::Back],
            Page::Initials | Page::HighScores | Page::Save | Page::Load | Page::Players | Page::PlayerControls(_) | Page::Find | Page::Join | Page::Code | Page::Joining => Vec::new(),
        }
    }

    pub fn heading(self) -> &'static str {
        match self {
            Page::Title => "",
            Page::Pause => "PAUSED",
            Page::Settings => "SETTINGS",
            Page::Audio => "AUDIO",
            Page::Video => "VIDEO",
            Page::Controls => "CONTROLS",
            Page::Initials => "NEW HIGH SCORE!",
            Page::HighScores => "HIGH SCORES",
            Page::Save => "SAVE",
            Page::Load => "LOAD",
            Page::Players => "CONTROLS",
            Page::PlayerControls(_) => "CONTROLS",
            Page::Network => "NETWORK",
            Page::Find | Page::Join | Page::Code => "JOIN",
            Page::Joining => "JOINING",
        }
    }
}

impl Item {
    pub fn label(self) -> String {
        self.label_in("en")
    }

    /// Its label in the player's language (`lang`); an action is the
    /// game's word.
    pub fn label_in(self, lang: &str) -> String {
        match self {
            // A half's directions are the engine's words.
            Item::Bind(way) if crate::input::DIRECTIONS.contains(&way) => crate::lang::say(lang, direction_word(way)).to_string(),
            // The game's own name for it, as written: a game cannot yet
            // name its actions in each language (BunnyFarm's German
            // CONTROLS reads ACT, BACK, PREV, NEXT, BAG).
            Item::Bind(action) => action.to_uppercase(),
            other => crate::lang::say(lang, other.fixed_label()).to_string(),
        }
    }

    fn fixed_label(self) -> &'static str {
        match self {
            Item::Continue => "CONTINUE",
            Item::Start => "START",
            Item::Settings => "SETTINGS",
            Item::Quit => "QUIT",
            Item::Resume => "RESUME",
            Item::Leave => "LEAVE",
            Item::Restart => "RESTART",
            Item::ToTitle => "QUIT TO TITLE",
            Item::Audio => "AUDIO",
            Item::Video => "VIDEO",
            Item::Music => "MUSIC",
            Item::Sound => "SOUND",
            Item::Song => "SOUNDTRACK",
            Item::Fullscreen => "FULLSCREEN",
            Item::Shake => "SCREEN SHAKE",
            Item::Flashes => "FLASHES",
            Item::Blur => "BLUR",
            Item::Shadows => "SHADOWS",
            Item::FrameRate => "FRAME RATE",
            Item::ShowFps => "SHOW FPS",
            Item::Rumble => "RUMBLE",
            Item::Language => "LANGUAGE",
            Item::Controls => "CONTROLS",
            Item::Bind(_) | Item::Slot(_) | Item::Drop(_) | Item::LetIn(_) | Item::Found(_) => "",
            Item::AnyGame => "ANY GAME ON THE INTERNET",
            Item::TypeCode => "TYPE A CODE",
            Item::TypeAddress => "TYPE AN ADDRESS",
            Item::PasteTicket => "PASTE A TICKET OR CODE",
            Item::CopyCode => "COPY THE CODE",
            Item::CopyTicket => "COPY THE TICKET",
            Item::Reset => "RESET",
            Item::Back => "BACK",
            Item::SaveGame => "SAVE",
            Item::LoadGame => "LOAD",
            Item::SeatKeys(_) => "KEYBOARD",
            Item::SeatPad(_) => "GAMEPAD",
            Item::SeatControls(_) => "KEYS",
            Item::Swap => "SWAP PLAYERS 1 AND 2",
            Item::Network => "NETWORK",
            Item::Host => "HOST",
            Item::HostNet => "HOST ON THE INTERNET",
            Item::Join => "JOIN",
        }
    }
}

/// A direction's word on a CONTROLS page.
fn direction_word(way: &str) -> &'static str {
    match way {
        "left" => "LEFT",
        "right" => "RIGHT",
        "up" => "UP",
        _ => "DOWN",
    }
}

/// What the game has to do after a menu was steered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Start,
    Resume,
    Restart,
    ToTitle,
    Quit,
    /// Settings changed: apply them and save.
    Changed,
    /// The window goes to or from fullscreen (and save).
    Fullscreen,
    /// Another song was chosen (and save).
    Song,
    /// The initials are in (`Menu::entry`): into the table with them.
    Entered,
    /// The player on this seat (2 and on) leaves it.
    Leave(usize),
    /// Save the game into this slot (from 1), or go on from the save in it.
    Save(u32),
    Load(u32),
    /// Back to the title's own page, nothing loaded anew.
    Title,
    /// The players page: a seat's part of the keyboard or its pad a step
    /// on (the window has the devices), and the first two seats swapping.
    SeatKeys(usize, i32),
    SeatPad(usize, i32),
    Swap,
    /// A network game: hosting one at once (on the LAN, across the
    /// internet), joining the address typed or the ticket (`Menu::address`),
    /// joining by a code (`Menu::code`), by the ticket or code on the
    /// clipboard, giving up joining, dropping a machine, letting one in,
    /// copying the code or the ticket.
    Host,
    HostNet,
    Join,
    JoinCode,
    JoinAny,
    PasteTicket,
    Disconnect,
    Drop(u8),
    LetIn(u32),
    CopyCode,
    CopyTicket,
}

/// A game found on the LAN, as JOIN lists it: a line, where it is, and
/// whether it is this game (another is greyed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoundLine {
    pub line: String,
    pub address: String,
    pub same: bool,
}

/// A network game as the menus show it (the window fills it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetView {
    /// Hosting; else a guest.
    pub hosting: bool,
    /// Lines to show: where the game is found; joining, how far.
    pub lines: Vec<String>,
    /// Who plays (a line each, beside the pause page's heading), and how
    /// to let others in while nobody came (under the items).
    pub who: Vec<String>,
    pub help: Vec<String>,
    /// Machines gone quiet long enough to drop: their number and name.
    pub drops: Vec<(u8, String)>,
    /// A guest's players sit (LEAVE gives the seats up, they watch).
    pub seated: bool,
    /// Hosted across the internet: its ticket to copy, its code (once our
    /// server gave one), and the machines asking in (their connection,
    /// and a line: name and the signs they say).
    pub ticket: bool,
    pub code: bool,
    pub knocks: Vec<(u32, String)>,
}

/// The open pages, the newest last, each with its cursor; empty while
/// playing.
#[derive(Clone, Debug, Default)]
pub struct Menu {
    pub stack: Vec<(Page, usize)>,
    /// Waiting for a key or button to bind to this action.
    pub rebinding: Option<&'static str>,
    /// Ticks the menu has been open, and when the page on top opened and
    /// its cursor last moved (for the animations).
    pub tick: u64,
    pub opened: u64,
    pub moved: u64,
    /// What the player steered with last (which hints to show), and the
    /// letters on the gamepad in hand.
    pub device: Device,
    pub pad_style: PadStyle,
    /// The initials being entered (`Page::Initials`).
    pub entry: Entry,
    /// The table's line that blinks (`Page::HighScores`): the one just
    /// entered.
    pub highlight: Option<usize>,
    /// What lies in each save slot, a line each (`slot_line`); none: the
    /// game has no slots, the pause page no SAVE and LOAD.
    pub slots: Vec<Option<String>>,
    /// The slot the title's CONTINUE goes on from (the newest save), if
    /// the game has one.
    pub continues: Option<u32>,
    /// The seat (2 and on) whose START opened the pause page: it offers
    /// LEAVE, its player leaving the seat.
    pub leaving: Option<usize>,
    /// The seats of a game of several as the players page shows them (the
    /// window fills them); none: a game of one, CONTROLS its keys.
    pub seats: Vec<SeatLine>,
    /// The title offers NETWORK (a game of several); the address being
    /// typed on JOIN, and the code; the network game being played, if one
    /// is.
    pub network: bool,
    pub address: String,
    pub code: String,
    /// Our matchmaker is known here: JOIN offers ANY GAME ON THE INTERNET
    /// and TYPE A CODE, and lists the games on the internet.
    pub internet: bool,
    pub net: Option<NetView>,
    /// The games found on the LAN (the window searches while JOIN shows).
    pub found: Vec<FoundLine>,
}

impl Menu {
    pub fn title() -> Self {
        Self { stack: vec![(Page::Title, 0)], ..Self::default() }
    }

    pub fn pause() -> Self {
        Self { stack: vec![(Page::Pause, 0)], ..Self::default() }
    }

    /// The title offering CONTINUE from this slot (the newest save), if
    /// there is one.
    pub fn continuing(mut self, slot: Option<u32>) -> Self {
        self.continues = slot;
        self
    }

    /// The pause page of a game with these save slots (`slot_line` each).
    pub fn with_slots(mut self, slots: Vec<Option<String>>) -> Self {
        self.slots = slots;
        self
    }

    /// The seats of a game of several: CONTROLS shows who plays with what.
    pub fn with_seats(mut self, seats: Vec<SeatLine>) -> Self {
        self.seats = seats;
        self
    }

    /// A page's items: the pause page's SAVE and LOAD with save slots, the
    /// slots on theirs, LANGUAGE in the settings of a game that speaks
    /// more than one, the players page's seats (`settings` its actions and
    /// languages).
    pub fn items(&self, page: Page, settings: &Settings) -> Vec<Item> {
        self.items_with(page, &settings.actions, settings.languages.len())
    }

    fn items_with(&self, page: Page, actions: &[&'static str], languages: usize) -> Vec<Item> {
        match page {
            Page::Settings if languages > 1 => {
                let mut items = page.items(actions);
                items.insert(items.len() - 1, Item::Language);
                items
            }
            // A guest's own menu: the game goes on; its players may give up
            // their seats (and watch) or leave the network game.
            Page::Pause if self.net.as_ref().is_some_and(|n| !n.hosting) => {
                let seated = self.net.as_ref().is_some_and(|n| n.seated);
                [Item::Resume].into_iter().chain(seated.then_some(Item::Leave)).chain([Item::Settings, Item::ToTitle]).collect()
            }
            // A player after the first can leave; the first's run goes on to its end.
            Page::Pause if self.leaving.is_some() => vec![Item::Resume, Item::Leave, Item::Settings],
            // The host's: machines asking in across the internet let in, a
            // machine gone quiet dropped, the ticket copied again.
            Page::Pause if self.net.as_ref().is_some_and(|n| n.hosting && (!n.drops.is_empty() || n.ticket)) => {
                let net = self.net.as_ref();
                let knocks = net.into_iter().flat_map(|n| n.knocks.iter().map(|&(p, _)| Item::LetIn(p)));
                let drops = net.into_iter().flat_map(|n| n.drops.iter().map(|&(m, _)| Item::Drop(m)));
                let ticket = net.filter(|n| n.ticket).map(|n| if n.code { Item::CopyCode } else { Item::CopyTicket });
                let saves = (!self.slots.is_empty()).then_some([Item::SaveGame, Item::LoadGame]).into_iter().flatten();
                [Item::Resume].into_iter().chain(knocks).chain(drops).chain(ticket).chain(saves).chain([Item::Settings, Item::Restart, Item::ToTitle]).collect()
            }
            Page::Pause if !self.slots.is_empty() => vec![Item::Resume, Item::SaveGame, Item::LoadGame, Item::Settings, Item::Restart, Item::ToTitle],
            Page::Title => {
                let first = self.continues.map(|_| Item::Continue).into_iter().chain([Item::Start]);
                first.chain(self.network.then_some(Item::Network)).chain([Item::Settings, Item::Quit]).collect()
            }
            Page::Save | Page::Load => (0..self.slots.len()).map(Item::Slot).chain([Item::Back]).collect(),
            Page::Find => {
                let internet = self.internet.then_some([Item::AnyGame, Item::TypeCode]).into_iter().flatten();
                (0..self.found.len()).map(Item::Found).chain(internet).chain([Item::TypeAddress, Item::PasteTicket, Item::Back]).collect()
            }
            Page::Players => {
                let seats = (1..=self.seats.len()).flat_map(|n| [Item::SeatKeys(n), Item::SeatPad(n), Item::SeatControls(n)]);
                let swap = (self.seats.len() > 1).then_some(Item::Swap);
                seats.chain(swap).chain([Item::Rumble, Item::Back]).collect()
            }
            // A half's directions first: they are its own.
            Page::PlayerControls(seat) => {
                let half = self.seats.get(seat.wrapping_sub(1)).is_some_and(|s| matches!(s.keyboard, Keys::Left | Keys::Right));
                let ways = ["up", "down", "left", "right"].into_iter().filter(|_| half).map(Item::Bind);
                ways.chain(actions.iter().map(|&a| Item::Bind(a))).chain([Item::Reset, Item::Back]).collect()
            }
            _ => page.items(actions),
        }
    }

    /// Entering the initials of a score that stands at `rank` in the table.
    pub fn initials(score: u32, rank: usize) -> Self {
        Self { stack: vec![(Page::Initials, 0)], entry: Entry { score, rank, ..Entry::default() }, ..Self::default() }
    }

    /// The high score table, the line at `highlight` blinking.
    pub fn high_scores(highlight: Option<usize>) -> Self {
        Self { stack: vec![(Page::HighScores, 0)], highlight, ..Self::default() }
    }

    pub fn is_open(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn page(&self) -> Option<Page> {
        self.stack.last().map(|&(p, _)| p)
    }

    /// The settings are open over the title: the chosen song plays then,
    /// so the player hears what they choose.
    pub fn previews_song(&self) -> bool {
        self.stack.first().is_some_and(|&(p, _)| p == Page::Title) && self.stack.iter().any(|&(p, _)| p == Page::Settings)
    }

    pub fn cursor(&self) -> usize {
        self.stack.last().map_or(0, |&(_, c)| c)
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<Item> {
        let (page, cursor) = *self.stack.last()?;
        self.items_with(page, &crate::shooter::ACTIONS, 0).get(cursor).copied()
    }

    /// One tick of the animations.
    pub fn update(&mut self) {
        self.tick += 1;
    }

    fn open(&mut self, page: Page) {
        self.stack.push((page, 0));
        self.opened = self.tick;
    }

    fn close(&mut self) {
        self.stack.pop();
        self.opened = self.tick;
    }

    /// Steers the menu; what the game must do then, if anything. `songs` is
    /// how many songs the game has.
    pub fn nav(&mut self, nav: Nav, settings: &mut Settings, songs: usize) -> Option<Command> {
        let (page, cursor) = *self.stack.last()?;
        if self.rebinding.is_some() {
            return None;
        }
        match page {
            Page::Initials => return self.pick(nav),
            Page::HighScores => return matches!(nav, Nav::Confirm | Nav::Back | Nav::Pause).then_some(Command::Title),
            Page::Join => return self.edit_address(nav),
            Page::Code => return self.edit_code(nav),
            Page::Joining => {
                return matches!(nav, Nav::Back | Nav::Confirm).then(|| {
                    self.stack.truncate(1);
                    Command::Disconnect
                });
            }
            _ => {}
        }
        let items = self.items(page, settings);
        let item = items[cursor.min(items.len() - 1)];
        match nav {
            Nav::Up | Nav::Down => {
                let n = items.len();
                let next = if nav == Nav::Up { (cursor + n - 1) % n } else { (cursor + 1) % n };
                self.stack.last_mut().unwrap().1 = next;
                self.moved = self.tick;
                None
            }
            Nav::Left | Nav::Right => {
                let step: i32 = if nav == Nav::Left { -1 } else { 1 };
                match item {
                    Item::Music | Item::Sound => {
                        let v = if item == Item::Music { &mut settings.music } else { &mut settings.sound };
                        let next = (*v as i32 + step).clamp(0, VOLUME_STEPS as i32) as u8;
                        (next != *v).then(|| {
                            *v = next;
                            Command::Changed
                        })
                    }
                    Item::Song if songs > 0 => {
                        settings.song = (settings.song as i32 + step).rem_euclid(songs as i32) as usize;
                        Some(Command::Song)
                    }
                    Item::Language => {
                        settings.step_language(step < 0);
                        Some(Command::Changed)
                    }
                    // Right: finer.
                    Item::Shadows => {
                        settings.shadows = settings.shadows.step(-step);
                        Some(Command::Changed)
                    }
                    Item::FrameRate => {
                        settings.frame_rate = settings.frame_rate.step(step);
                        Some(Command::Changed)
                    }
                    Item::SeatKeys(seat) => Some(Command::SeatKeys(seat, step)),
                    Item::SeatPad(seat) => Some(Command::SeatPad(seat, step)),
                    _ => toggle(item, settings),
                }
            }
            Nav::Confirm => match item {
                Item::Start => Some(Command::Start),
                Item::Quit => Some(Command::Quit),
                Item::Resume => {
                    self.stack.clear();
                    Some(Command::Resume)
                }
                Item::Leave => {
                    self.stack.clear();
                    // A guest's first player too (it then watches).
                    Some(Command::Leave(self.leaving.unwrap_or(1)))
                }
                Item::Restart => {
                    self.stack.clear();
                    Some(Command::Restart)
                }
                Item::ToTitle => Some(Command::ToTitle),
                Item::Settings => {
                    self.open(Page::Settings);
                    None
                }
                Item::Audio => {
                    self.open(Page::Audio);
                    None
                }
                Item::Video => {
                    self.open(Page::Video);
                    None
                }
                // A game of several: who plays with what, each its keys.
                Item::Controls => {
                    self.open(if self.seats.len() > 1 { Page::Players } else { Page::Controls });
                    None
                }
                Item::SeatControls(seat) => {
                    self.open(Page::PlayerControls(seat));
                    None
                }
                Item::SeatKeys(seat) => Some(Command::SeatKeys(seat, 1)),
                Item::SeatPad(seat) => Some(Command::SeatPad(seat, 1)),
                Item::Swap => Some(Command::Swap),
                Item::Network => {
                    self.open(Page::Network);
                    None
                }
                Item::Host => {
                    self.stack.clear();
                    Some(Command::Host)
                }
                Item::HostNet => {
                    self.stack.clear();
                    Some(Command::HostNet)
                }
                Item::PasteTicket => Some(Command::PasteTicket),
                Item::CopyCode => Some(Command::CopyCode),
                Item::CopyTicket => Some(Command::CopyTicket),
                Item::AnyGame => {
                    self.stack.truncate(1);
                    self.open(Page::Joining);
                    Some(Command::JoinAny)
                }
                Item::TypeCode => {
                    self.code.clear();
                    self.open(Page::Code);
                    None
                }
                Item::LetIn(peer) => Some(Command::LetIn(peer)),
                Item::Join => {
                    self.open(Page::Find);
                    None
                }
                Item::TypeAddress => {
                    if self.address.is_empty() {
                        self.address = settings.address.clone();
                    }
                    self.open(Page::Join);
                    None
                }
                // Another game's greyed: nothing.
                Item::Found(n) => {
                    let found = self.found.get(n).filter(|f| f.same)?;
                    self.address = found.address.clone();
                    self.stack.truncate(1);
                    self.open(Page::Joining);
                    Some(Command::Join)
                }
                Item::Drop(m) => Some(Command::Drop(m)),
                Item::Back => {
                    self.close();
                    None
                }
                Item::SaveGame => {
                    self.open(Page::Save);
                    None
                }
                Item::LoadGame => {
                    self.open(Page::Load);
                    None
                }
                Item::Continue => {
                    self.stack.clear();
                    self.continues.map(Command::Load)
                }
                Item::Slot(n) if page == Page::Save => Some(Command::Save(n as u32 + 1)),
                // An empty slot holds nothing to go on from.
                Item::Slot(n) if self.slots.get(n).is_some_and(Option::is_some) => {
                    self.stack.clear();
                    Some(Command::Load(n as u32 + 1))
                }
                Item::Slot(_) => None,
                Item::Song if songs > 0 => {
                    settings.song = (settings.song + 1) % songs;
                    Some(Command::Song)
                }
                Item::Shadows => {
                    settings.shadows = settings.shadows.step(1);
                    Some(Command::Changed)
                }
                Item::FrameRate => {
                    settings.frame_rate = settings.frame_rate.step(1);
                    Some(Command::Changed)
                }
                Item::Language => {
                    settings.step_language(false);
                    Some(Command::Changed)
                }
                Item::Music | Item::Sound | Item::Song => None,
                Item::Bind(action) => {
                    self.rebinding = Some(action);
                    None
                }
                Item::Reset => {
                    match page {
                        Page::PlayerControls(seat) => settings.reset_seat(seat, self.seats.get(seat - 1).map_or(Keys::Nothing, |s| s.keyboard)),
                        _ => settings.reset_controls(),
                    }
                    Some(Command::Changed)
                }
                _ => toggle(item, settings),
            },
            Nav::Back => {
                if self.stack.len() > 1 {
                    self.close();
                    None
                } else if page == Page::Pause {
                    self.stack.clear();
                    Some(Command::Resume)
                } else {
                    None
                }
            }
            Nav::Pause => match page {
                Page::Title => self.nav(Nav::Confirm, settings, songs),
                _ if self.stack[0].0 == Page::Pause => {
                    self.stack.clear();
                    Some(Command::Resume)
                }
                _ => None,
            },
        }
    }

    /// Picking initials like at an arcade machine: up and down step through
    /// the letters, right or confirm takes one and goes on, left or back
    /// goes to the one before; after the third (or at start) they are in.
    fn pick(&mut self, nav: Nav) -> Option<Command> {
        let e = &mut self.entry;
        let letters: Vec<char> = LETTERS.chars().collect();
        let at = letters.iter().position(|&c| c == e.letters[e.at]).unwrap_or(0);
        match nav {
            Nav::Up => e.letters[e.at] = letters[(at + 1) % letters.len()],
            Nav::Down => e.letters[e.at] = letters[(at + letters.len() - 1) % letters.len()],
            Nav::Right | Nav::Confirm if e.at < 2 => e.at += 1,
            Nav::Right | Nav::Confirm | Nav::Pause => return Some(Command::Entered),
            Nav::Left | Nav::Back => e.at = e.at.saturating_sub(1),
        }
        self.moved = self.tick;
        None
    }

    /// The signs an address is typed with on a pad, in the order up steps
    /// through them.
    pub const ADDRESS: &str = "0123456789.:";

    /// The JOIN page on a pad, as initials are picked: up and down the last
    /// sign, right one more, left one less, confirm joins, back goes back.
    fn edit_address(&mut self, nav: Nav) -> Option<Command> {
        let signs: Vec<char> = Self::ADDRESS.chars().collect();
        match nav {
            Nav::Up | Nav::Down => {
                let last = self.address.pop().unwrap_or('0');
                let at = signs.iter().position(|&c| c == last).unwrap_or(0) as i32;
                let step = if nav == Nav::Up { 1 } else { -1 };
                self.address.push(signs[(at + step).rem_euclid(signs.len() as i32) as usize]);
            }
            Nav::Right if self.address.len() < 40 => self.address.push('0'),
            Nav::Left => {
                self.address.pop();
            }
            Nav::Confirm if !self.address.trim().is_empty() => {
                self.stack.truncate(1);
                self.open(Page::Joining);
                return Some(Command::Join);
            }
            Nav::Back | Nav::Pause => self.close(),
            _ => return None,
        }
        self.moved = self.tick;
        None
    }

    /// The code page on a pad, as the JOIN page: up and down the last
    /// sign (a code's own: no 0 and O, no 1 and I), right one more, left
    /// one less; confirm joins once all six are there.
    fn edit_code(&mut self, nav: Nav) -> Option<Command> {
        let signs = matchmaker_protocol::SIGNS;
        match nav {
            Nav::Up | Nav::Down => {
                let last = self.code.pop().unwrap_or('A');
                let at = signs.iter().position(|&c| c as char == last).unwrap_or(0) as i32;
                let step = if nav == Nav::Up { 1 } else { -1 };
                self.code.push(signs[(at + step).rem_euclid(signs.len() as i32) as usize] as char);
            }
            Nav::Right if self.code.len() < matchmaker_protocol::LENGTH => self.code.push('A'),
            Nav::Left => {
                self.code.pop();
            }
            Nav::Confirm if self.code.len() == matchmaker_protocol::LENGTH => {
                self.stack.truncate(1);
                self.open(Page::Joining);
                return Some(Command::JoinCode);
            }
            Nav::Back | Nav::Pause => self.close(),
            _ => return None,
        }
        self.moved = self.tick;
        None
    }

    /// A sign typed on the JOIN page (a host's address: its name or its
    /// numbers, a port after a colon) or the code page (a code's signs, in
    /// capitals); Backspace takes the last.
    pub fn type_address(&mut self, c: char) {
        match self.page() {
            Some(Page::Join) if self.address.len() < 40 && (c.is_ascii_alphanumeric() || ".:-".contains(c)) => self.address.push(c),
            Some(Page::Code) if self.code.len() < matchmaker_protocol::LENGTH && matchmaker_protocol::SIGNS.contains(&(c.to_ascii_uppercase() as u8)) => self.code.push(c.to_ascii_uppercase()),
            _ => return,
        }
        self.moved = self.tick;
    }

    pub fn backspace_address(&mut self) {
        match self.page() {
            Some(Page::Join) => self.address.pop(),
            Some(Page::Code) => self.code.pop(),
            _ => return,
        };
        self.moved = self.tick;
    }

    /// A letter typed while picking initials: it is taken, and the next is
    /// picked (after the third they are in).
    pub fn type_letter(&mut self, c: char) -> Option<Command> {
        let c = c.to_ascii_uppercase();
        if self.page() != Some(Page::Initials) || !LETTERS.contains(c) {
            return None;
        }
        self.entry.letters[self.entry.at] = c;
        self.pick(Nav::Right)
    }

    /// While waiting for a binding: `name` was pressed on `device`. Cancel
    /// (Escape, the back button) keeps what was bound.
    /// A player's CONTROLS page binds its part of the keyboard and its
    /// pad.
    pub fn bind(&mut self, device: Device, name: &str, cancel: bool, settings: &mut Settings) -> Option<Command> {
        let action = self.rebinding.take()?;
        if cancel {
            return None;
        }
        let seat = match self.page() {
            Some(Page::PlayerControls(seat)) => Some(seat),
            _ => None,
        };
        match (device, seat) {
            (Device::Keyboard, None) => settings.keys.bind(action, name),
            (Device::Gamepad, None) => settings.pad.bind(action, name),
            (Device::Keyboard, Some(seat)) => settings.keys_of_mut(self.seats.get(seat - 1).map_or(Keys::Nothing, |s| s.keyboard))?.bind(action, name),
            (Device::Gamepad, Some(seat)) => settings.pad_of_mut(seat).bind(action, name),
        }
        Some(Command::Changed)
    }
}

/// A save slot's line in the menu: its level, the play time and how long
/// ago it was saved (`now` and `when` in seconds since 1970).
pub fn slot_line(n: u32, info: Option<&crate::saves::SlotInfo>, now: u64, lang: &str) -> String {
    use crate::lang::{say, say_with};
    let Some(info) = info else { return format!("{n}  {}", say(lang, "EMPTY")) };
    let secs = info.ticks / 60;
    let played = format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    let ago = now.saturating_sub(info.when);
    let ago = match ago {
        _ if info.when == 0 => String::new(),
        0..60 => say(lang, "JUST NOW").into(),
        60..3600 => say_with(lang, "{} MIN AGO", &[&(ago / 60).to_string()]),
        3600..86_400 => say_with(lang, "{} H AGO", &[&(ago / 3600).to_string()]),
        _ => say_with(lang, "{} DAYS AGO", &[&(ago / 86_400).to_string()]),
    };
    let level: String = info.level.to_uppercase().chars().take(12).collect();
    format!("{n}  {level}  {played}  {ago}").trim_end().to_string()
}

/// Flips an on/off item; the command to apply it.
fn toggle(item: Item, settings: &mut Settings) -> Option<Command> {
    let flag = match item {
        Item::Fullscreen => &mut settings.fullscreen,
        Item::Shake => &mut settings.shake,
        Item::Flashes => &mut settings.flashes,
        Item::Blur => &mut settings.blur,
        Item::Rumble => &mut settings.rumble,
        Item::ShowFps => &mut settings.show_fps,
        _ => return None,
    };
    *flag = !*flag;
    Some(if item == Item::Fullscreen { Command::Fullscreen } else { Command::Changed })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From the title: down to settings, into AUDIO, change the music,
    /// back out, and start.
    #[test]
    fn the_title_menu_leads_everywhere() {
        let mut s = Settings::default();
        let mut m = Menu::title();
        assert_eq!(m.selected(), Some(Item::Start));
        m.nav(Nav::Down, &mut s, 4);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 4), None);
        assert_eq!(m.page(), Some(Page::Settings));
        assert_eq!(m.items(Page::Settings, &s), [Item::Audio, Item::Video, Item::Controls, Item::Back]);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 4), None);
        assert_eq!(m.page(), Some(Page::Audio));
        let before = s.music;
        assert_eq!(m.nav(Nav::Left, &mut s, 4), Some(Command::Changed));
        assert_eq!(s.music, before - 1);
        m.nav(Nav::Down, &mut s, 4);
        m.nav(Nav::Down, &mut s, 4);
        assert_eq!(m.nav(Nav::Right, &mut s, 4), Some(Command::Song));
        assert_eq!(s.song, 1);
        assert!(m.previews_song(), "the chosen song should play in the title's settings");
        assert_eq!(m.nav(Nav::Back, &mut s, 4), None);
        assert_eq!(m.page(), Some(Page::Settings));
        assert_eq!(m.selected(), Some(Item::Audio), "back where it was");
        m.nav(Nav::Down, &mut s, 4);
        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(m.page(), Some(Page::Video));
        m.nav(Nav::Back, &mut s, 4);
        m.nav(Nav::Down, &mut s, 4);
        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(m.page(), Some(Page::Controls), "a game of one: its keys");
        m.nav(Nav::Back, &mut s, 4);
        assert_eq!(m.nav(Nav::Back, &mut s, 4), None);
        assert_eq!(m.page(), Some(Page::Title));
        assert!(!m.previews_song());
        m.nav(Nav::Up, &mut s, 4);
        assert_eq!(m.selected(), Some(Item::Start));
        assert_eq!(m.nav(Nav::Pause, &mut s, 4), Some(Command::Start));
    }

    /// A game that speaks more than one language has LANGUAGE on the
    /// settings' own page, after CONTROLS; left and right step through them.
    #[test]
    fn the_settings_choose_a_language() {
        let mut s = Settings::default();
        let m = Menu::title();
        assert!(!m.items(Page::Settings, &s).contains(&Item::Language));
        s.offer_languages(&["de".into(), "fr".into()]);
        let items = m.items(Page::Settings, &s);
        let at = items.iter().position(|&i| i == Item::Language).expect("LANGUAGE");
        assert_eq!((items[at - 1], items[at + 1]), (Item::Controls, Item::Back));
        let mut m = Menu { stack: vec![(Page::Title, 1), (Page::Settings, at)], ..Menu::title() };
        assert_eq!(m.nav(Nav::Right, &mut s, 0), Some(Command::Changed));
        assert_eq!(s.language(), "fr");
        assert_eq!(Item::Language.label_in("de"), "SPRACHE");
    }

    /// A game with a save offers CONTINUE first on the title: it goes on
    /// from that slot, the menu closed; without one the title is as it was.
    #[test]
    fn the_title_continues_from_the_newest_save() {
        let mut s = Settings::default();
        assert_eq!(Menu::title().items(Page::Title, &Settings::default()), [Item::Start, Item::Settings, Item::Quit]);
        let mut m = Menu::title().continuing(Some(2));
        assert_eq!(m.items(Page::Title, &Settings::default()), [Item::Continue, Item::Start, Item::Settings, Item::Quit]);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::Load(2)));
        assert!(!m.is_open());
        let mut m = Menu::title().continuing(Some(2));
        m.nav(Nav::Down, &mut s, 0);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::Start), "START begins a new run");
    }

    /// The pause page a player after the first opened offers LEAVE: its
    /// seat is left, the game goes on.
    #[test]
    fn a_second_player_leaves_from_the_pause_page() {
        let mut s = Settings::default();
        let mut m = Menu::pause();
        m.leaving = Some(2);
        assert_eq!(m.items(Page::Pause, &s), [Item::Resume, Item::Leave, Item::Settings]);
        m.nav(Nav::Down, &mut s, 0);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::Leave(2)));
        assert!(!m.is_open());
    }

    /// A game of several has who plays with what under the settings'
    /// CONTROLS: each seat's keyboard and pad step on as commands (the
    /// window has the devices), SWAP swaps the first two; a player's KEYS
    /// bind its half of the keyboard (its directions first) and its own
    /// pad, and RESET gives it back the start's.
    #[test]
    fn the_players_page_steers_each_seat() {
        let mut s = Settings::default();
        s.adopt(&crate::shooter::ACTIONS, &crate::shooter::keys(), &crate::shooter::pad());
        let seats = vec![SeatLine { keyboard: Keys::Left, pad: String::new(), taken: true }, SeatLine { keyboard: Keys::Right, pad: "Pad".into(), taken: true }];
        let mut m = Menu::title().with_seats(seats.clone());
        assert_eq!(m.items(Page::Title, &s), [Item::Start, Item::Settings, Item::Quit], "the title as in a game of one");
        assert_eq!(Menu::pause().with_seats(seats).items(Page::Pause, &s), Menu::pause().items(Page::Pause, &s));
        m.nav(Nav::Down, &mut s, 1);
        m.nav(Nav::Confirm, &mut s, 1);
        m.nav(Nav::Down, &mut s, 1);
        m.nav(Nav::Down, &mut s, 1);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 1), None);
        assert_eq!(m.page(), Some(Page::Players));
        assert_eq!(m.items(Page::Players, &s)[6..], [Item::Swap, Item::Rumble, Item::Back]);
        assert_eq!(m.nav(Nav::Right, &mut s, 1), Some(Command::SeatKeys(1, 1)));
        m.nav(Nav::Down, &mut s, 1);
        assert_eq!(m.nav(Nav::Left, &mut s, 1), Some(Command::SeatPad(1, -1)));
        for _ in 0..4 {
            m.nav(Nav::Down, &mut s, 1);
        }
        assert_eq!(m.selected(), Some(Item::SeatControls(2)));
        m.nav(Nav::Down, &mut s, 1);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 1), Some(Command::Swap));
        m.nav(Nav::Up, &mut s, 1);
        m.nav(Nav::Confirm, &mut s, 1);
        assert_eq!((m.page(), m.selected()), (Some(Page::PlayerControls(2)), Some(Item::Bind("up"))), "a half's directions first");
        m.nav(Nav::Confirm, &mut s, 1);
        assert_eq!(m.bind(Device::Keyboard, "U", false, &mut s), Some(Command::Changed));
        assert_eq!((s.right_keys.get("up"), s.left_keys.get("up")), (&["U".to_string(), "Up".to_string()][..], &["W".to_string()][..]), "the right half's own");
        assert_eq!(s.keys.get("fire"), crate::shooter::keys().get("fire"), "the whole keyboard's as they were");
        for _ in 0..4 {
            m.nav(Nav::Down, &mut s, 1);
        }
        assert_eq!(m.selected(), Some(Item::Bind("fire")));
        m.nav(Nav::Confirm, &mut s, 1);
        m.bind(Device::Gamepad, "north", false, &mut s);
        assert_eq!((s.pad_of(2).get("fire").first(), s.pad_of(1).get("fire").first()), (Some(&"north".to_string()), s.pad.get("fire").first()), "its own pad's");
        assert_ne!(s.pad.get("fire").first(), Some(&"north".to_string()), "the first player's as they were");
        s.reset_seat(2, Keys::Right);
        assert_eq!((s.right_keys.get("up"), s.pad_of(2).get("fire")), (&["Up".to_string()][..], s.pad.get("fire")), "back to the start's");
    }

    /// A game with save slots has SAVE and LOAD on the pause page: a slot
    /// chosen saves into it; an empty one loads nothing; a filled one goes
    /// on from it, the menu closed.
    #[test]
    fn the_pause_page_saves_and_loads() {
        let mut s = Settings::default();
        let info = crate::saves::SlotInfo { level: "hall".into(), ticks: 3723 * 60, when: 1_000 };
        let mut m = Menu::pause().with_slots(vec![Some(slot_line(1, Some(&info), 1_000 + 7_200, "en")), None]);
        assert_eq!(m.slots[0].as_deref(), Some("1  HALL  1:02:03  2 H AGO"));
        assert_eq!(m.items(Page::Pause, &Settings::default()), [Item::Resume, Item::SaveGame, Item::LoadGame, Item::Settings, Item::Restart, Item::ToTitle]);
        assert_eq!(Menu::pause().items(Page::Pause, &Settings::default()).len(), 4, "no slots, no SAVE and LOAD");
        m.nav(Nav::Down, &mut s, 1);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 1), None);
        assert_eq!(m.page(), Some(Page::Save));
        m.nav(Nav::Down, &mut s, 1);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 1), Some(Command::Save(2)));
        m.nav(Nav::Back, &mut s, 1);
        m.nav(Nav::Down, &mut s, 1);
        m.nav(Nav::Confirm, &mut s, 1);
        assert_eq!(m.page(), Some(Page::Load));
        m.nav(Nav::Down, &mut s, 1);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 1), None, "slot 2 is empty");
        m.nav(Nav::Up, &mut s, 1);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 1), Some(Command::Load(1)));
        assert!(!m.is_open());
    }

    /// Volumes stop at their ends; songs go round.
    #[test]
    fn sliders_stop_and_songs_go_round() {
        let mut s = Settings::default();
        (s.music, s.song) = (VOLUME_STEPS, 3);
        let mut m = Menu { stack: vec![(Page::Audio, 0)], ..Menu::default() };
        assert_eq!(m.nav(Nav::Right, &mut s, 4), None);
        assert_eq!(s.music, VOLUME_STEPS);
        m.stack[0].1 = 2;
        m.nav(Nav::Right, &mut s, 4);
        assert_eq!(s.song, 0);
        m.nav(Nav::Left, &mut s, 4);
        assert_eq!(s.song, 3);
    }

    /// SHOW FPS in the settings' VIDEO turns the frame counter on and off; it
    /// starts off.
    #[test]
    fn show_fps_is_a_setting() {
        let mut s = Settings::default();
        assert!(!s.show_fps, "the counter shows from the start");
        let mut m = Menu::title();
        let at = Page::Video.items(&[]).iter().position(|&i| i == Item::ShowFps).expect("no SHOW FPS in the settings");
        m.stack.push((Page::Video, at));
        assert_eq!(m.nav(Nav::Right, &mut s, 4), Some(Command::Changed));
        assert!(s.show_fps);
        m.nav(Nav::Left, &mut s, 4);
        assert!(!s.show_fps);
    }

    /// FRAME RATE steps through its choices (right on, left back, round the
    /// list; confirm steps on); it starts unlimited, as the window always drew.
    #[test]
    fn the_frame_rate_is_a_setting() {
        use crate::settings::FrameRate;
        let mut s = Settings::default();
        assert_eq!(s.frame_rate, FrameRate::Unlimited);
        let mut m = Menu::title();
        let at = Page::Video.items(&[]).iter().position(|&i| i == Item::FrameRate).expect("no FRAME RATE in the settings");
        m.stack.push((Page::Video, at));
        assert_eq!(m.nav(Nav::Right, &mut s, 0), Some(Command::Changed));
        assert_eq!(s.frame_rate, FrameRate::Vsync);
        m.nav(Nav::Confirm, &mut s, 0);
        assert_eq!(s.frame_rate, FrameRate::Fps60);
        m.nav(Nav::Left, &mut s, 0);
        m.nav(Nav::Left, &mut s, 0);
        assert_eq!(s.frame_rate, FrameRate::Unlimited);
        m.nav(Nav::Left, &mut s, 0);
        assert_eq!(s.frame_rate, FrameRate::Fps30, "round the list");
        assert_eq!(FrameRate::Fps60.limit(), Some(60));
        assert_eq!(FrameRate::Vsync.limit(), None);
        assert!(!FrameRate::Unlimited.waits_for_the_screen() && FrameRate::Fps30.waits_for_the_screen());
    }

    /// SHADOWS steps through the qualities, right finer, left coarser, round
    /// the list; confirm steps coarser. It starts at the finest.
    #[test]
    fn shadows_step_through_the_qualities() {
        use crate::settings::Shadows;
        let mut s = Settings::default();
        assert_eq!(s.shadows, Shadows::High);
        let mut m = Menu::title();
        let at = Page::Video.items(&[]).iter().position(|&i| i == Item::Shadows).expect("no SHADOWS in the settings");
        m.stack.push((Page::Video, at));
        assert_eq!(m.nav(Nav::Left, &mut s, 4), Some(Command::Changed));
        assert_eq!(s.shadows, Shadows::Medium);
        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(s.shadows, Shadows::Low);
        m.nav(Nav::Right, &mut s, 4);
        assert_eq!(s.shadows, Shadows::Medium);
        m.nav(Nav::Left, &mut s, 4);
        m.nav(Nav::Left, &mut s, 4);
        assert_eq!(s.shadows, Shadows::High, "the list goes round");
    }

    /// The pause button closes the pause menu, back closes a page on top
    /// first, and quitting to the title is the game's to do.
    #[test]
    fn pause_resumes_and_backs_out() {
        let mut s = Settings::default();
        let mut m = Menu::pause();
        m.nav(Nav::Down, &mut s, 4);
        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(m.page(), Some(Page::Settings));
        assert!(!m.previews_song(), "while paused the game's song plays on");
        assert_eq!(m.nav(Nav::Back, &mut s, 4), None);
        assert_eq!(m.page(), Some(Page::Pause));
        assert_eq!(m.nav(Nav::Pause, &mut s, 4), Some(Command::Resume));
        assert!(!m.is_open());

        let mut m = Menu::pause();
        m.stack[0].1 = 3;
        assert_eq!(m.nav(Nav::Confirm, &mut s, 4), Some(Command::ToTitle));
    }

    /// Choosing an action waits for a key or button and binds it; cancel
    /// keeps the old one.
    #[test]
    fn rebinding_waits_for_a_press() {
        let mut s = Settings::default();
        s.adopt(&crate::shooter::ACTIONS, &crate::shooter::keys(), &crate::shooter::pad());
        let mut m = Menu { stack: vec![(Page::Controls, 2)], ..Menu::default() };
        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(m.rebinding, Some("jump"));
        assert_eq!(m.nav(Nav::Down, &mut s, 4), None, "the menu moved while waiting");
        assert_eq!(m.bind(Device::Gamepad, "north", false, &mut s), Some(Command::Changed));
        assert_eq!(s.pad.get("jump")[0], "north");

        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(m.bind(Device::Keyboard, "Escape", true, &mut s), None);
        assert_eq!(s.keys.get("jump")[0], "Space");
        assert_eq!(m.rebinding, None);
        // RUMBLE among the controls, then RESET: the game's again.
        m.stack.last_mut().unwrap().1 = 5;
        assert_eq!(m.items(Page::Controls, &s)[5], Item::Rumble);
        m.nav(Nav::Confirm, &mut s, 4);
        assert!(!s.rumble);
        m.nav(Nav::Down, &mut s, 4);
        m.nav(Nav::Confirm, &mut s, 4);
        assert_eq!(s.pad.get("jump")[0], "south");
    }

    /// Initials are picked like at an arcade machine: up and down step
    /// through the letters (round), right takes one, left goes back; typed
    /// letters are taken at once; after the third they are in. The table
    /// goes back to the title at a key.
    #[test]
    fn initials_are_picked_like_at_an_arcade_machine() {
        let mut s = Settings::default();
        let mut m = Menu::initials(14_600, 2);
        assert_eq!(m.page(), Some(Page::Initials));
        m.nav(Nav::Down, &mut s, 0);
        assert_eq!(m.entry.letters[0], '.', "down from A goes round");
        m.nav(Nav::Up, &mut s, 0);
        m.nav(Nav::Up, &mut s, 0);
        assert_eq!(m.nav(Nav::Right, &mut s, 0), None);
        assert_eq!(m.entry.at, 1);
        assert_eq!(m.type_letter('x'), None);
        assert_eq!(m.entry.at, 2);
        m.nav(Nav::Left, &mut s, 0);
        assert_eq!(m.type_letter('y'), None);
        assert_eq!(m.type_letter('3'), None, "not a letter");
        assert_eq!(m.entry.at, 2);
        assert_eq!(m.type_letter('z'), Some(Command::Entered));
        assert_eq!(m.entry.initials(), "BYZ");

        let mut m = Menu::high_scores(Some(4));
        assert_eq!(m.nav(Nav::Down, &mut s, 0), None);
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::Title));
    }

    /// Where our matchmaker is known, JOIN offers any game on the
    /// internet and typing a game's code, on a page of its own: on a pad
    /// sign by sign through a code's signs (no 0 or O, no 1 or I), at the
    /// keyboard in any case; it joins once all six are there. The host's
    /// pause page copies the code once the matchmaker gave one, else the
    /// ticket.
    #[test]
    fn a_game_is_joined_by_its_code() {
        let mut s = Settings::default();
        let mut m = Menu { stack: vec![(Page::Title, 1), (Page::Network, 2), (Page::Find, 1)], network: true, internet: true, ..Menu::default() };
        m.found = vec![FoundLine { line: "PULSAR  ANNA  1/2  DX67A3".into(), address: "DX67A3".into(), same: true }];
        assert_eq!(m.items(Page::Find, &s), [Item::Found(0), Item::AnyGame, Item::TypeCode, Item::TypeAddress, Item::PasteTicket, Item::Back]);
        let mut any = m.clone();
        assert_eq!((any.nav(Nav::Confirm, &mut s, 0), any.page()), (Some(Command::JoinAny), Some(Page::Joining)), "any game with a free seat");
        let mut listed = m.clone();
        listed.stack.last_mut().unwrap().1 = 0;
        assert_eq!((listed.nav(Nav::Confirm, &mut s, 0), listed.address.as_str()), (Some(Command::Join), "DX67A3"), "a game on the internet joins by its code");
        m.stack.last_mut().unwrap().1 = 2;
        m.code = "OLD".into();
        m.nav(Nav::Confirm, &mut s, 0);
        assert_eq!((m.page(), m.code.as_str()), (Some(Page::Code), ""), "a code is typed afresh");
        m.nav(Nav::Down, &mut s, 0);
        assert_eq!(m.code, "9", "down from the first sign goes round to the last");
        m.nav(Nav::Up, &mut s, 0);
        m.nav(Nav::Right, &mut s, 0);
        assert_eq!(m.code, "AA");
        for c in ['x', '0', 'o', '1', '-', '7', 'q'] {
            m.type_address(c);
        }
        assert_eq!(m.code, "AAX7Q", "a code's signs only, in capitals");
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), None, "five signs are no code");
        m.type_address('3');
        m.type_address('4');
        assert_eq!(m.code, "AAX7Q3", "six at most");
        m.backspace_address();
        m.type_address('k');
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::JoinCode));
        assert_eq!((m.page(), m.code.as_str()), (Some(Page::Joining), "AAX7QK"));
        let mut host = Menu::pause();
        host.net = Some(NetView { hosting: true, ticket: true, ..NetView::default() });
        assert!(host.items(Page::Pause, &s).contains(&Item::CopyTicket));
        host.net = Some(NetView { hosting: true, ticket: true, code: true, ..NetView::default() });
        let items = host.items(Page::Pause, &s);
        assert!(items.contains(&Item::CopyCode) && !items.contains(&Item::CopyTicket));
    }

    /// A game of several offers NETWORK on its title: HOST hosts at once,
    /// JOIN takes the address typed (on a pad sign by sign, as initials;
    /// the last one typed to start with) and waits on JOINING, where back
    /// gives up. A guest's own pause page lets the game go on (LEAVE, and
    /// no SAVE, LOAD or RESTART); the host's offers to drop a machine gone
    /// quiet.
    #[test]
    fn a_network_game_is_hosted_and_joined() {
        let mut s = Settings::default();
        s.address = "10.0.0.7".into();
        let mut m = Menu::title();
        m.network = true;
        assert_eq!(m.items(Page::Title, &s), [Item::Start, Item::Network, Item::Settings, Item::Quit]);
        m.nav(Nav::Down, &mut s, 0);
        m.nav(Nav::Confirm, &mut s, 0);
        assert_eq!(m.page(), Some(Page::Network));
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::Host));
        let mut across = Menu { stack: vec![(Page::Title, 1), (Page::Network, 1)], network: true, ..Menu::default() };
        assert_eq!(across.nav(Nav::Confirm, &mut s, 0), Some(Command::HostNet), "hosting across the internet");
        let mut m = Menu { stack: vec![(Page::Title, 1), (Page::Network, 2)], network: true, ..Menu::default() };
        m.nav(Nav::Confirm, &mut s, 0);
        assert_eq!(m.page(), Some(Page::Find));
        m.found = vec![FoundLine { line: "OTHER".into(), address: "10.0.0.9:26500".into(), same: false }, FoundLine { line: "PULSAR".into(), address: "10.0.0.8:26500".into(), same: true }];
        assert_eq!(m.items(Page::Find, &s), [Item::Found(0), Item::Found(1), Item::TypeAddress, Item::PasteTicket, Item::Back], "no matchmaker known: the LAN, an address, a ticket");
        let mut paste = m.clone();
        paste.stack.last_mut().unwrap().1 = 3;
        assert_eq!(paste.nav(Nav::Confirm, &mut s, 0), Some(Command::PasteTicket), "joining by the ticket or code on the clipboard");
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), None, "another game's is grayed");
        m.nav(Nav::Down, &mut s, 0);
        let mut found = m.clone();
        assert_eq!((found.nav(Nav::Confirm, &mut s, 0), found.address.as_str(), found.page()), (Some(Command::Join), "10.0.0.8:26500", Some(Page::Joining)));
        m.nav(Nav::Down, &mut s, 0);
        m.nav(Nav::Confirm, &mut s, 0);
        assert_eq!((m.page(), m.address.as_str()), (Some(Page::Join), "10.0.0.7"), "the last address to start with");
        m.nav(Nav::Left, &mut s, 0);
        m.nav(Nav::Right, &mut s, 0);
        m.nav(Nav::Up, &mut s, 0);
        m.nav(Nav::Up, &mut s, 0);
        m.type_address('x');
        assert_eq!(m.address, "10.0.0.2x");
        m.backspace_address();
        assert_eq!(m.nav(Nav::Confirm, &mut s, 0), Some(Command::Join));
        assert_eq!(m.page(), Some(Page::Joining));
        assert_eq!(m.nav(Nav::Back, &mut s, 0), Some(Command::Disconnect));
        assert_eq!(m.page(), Some(Page::Title));
        let mut guest = Menu::pause().with_slots(vec![None]);
        guest.net = Some(NetView { seated: true, ..NetView::default() });
        assert_eq!(guest.items(Page::Pause, &s), [Item::Resume, Item::Leave, Item::Settings, Item::ToTitle]);
        assert_eq!(guest.nav(Nav::Down, &mut s, 0).or(guest.nav(Nav::Confirm, &mut s, 0)), Some(Command::Leave(1)), "its first player gives up its seat");
        let mut host = Menu::pause().with_slots(vec![None]);
        host.net = Some(NetView { hosting: true, drops: vec![(3, "ben".into())], ..NetView::default() });
        assert_eq!(host.items(Page::Pause, &s), [Item::Resume, Item::Drop(3), Item::SaveGame, Item::LoadGame, Item::Settings, Item::Restart, Item::ToTitle]);
        // Across the internet: a machine asking in is let in, the ticket
        // copied again.
        let mut net = Menu::pause();
        net.net = Some(NetView { hosting: true, ticket: true, knocks: vec![(4, "BEN KQ7P".into())], ..NetView::default() });
        assert_eq!(net.items(Page::Pause, &s), [Item::Resume, Item::LetIn(4), Item::CopyTicket, Item::Settings, Item::Restart, Item::ToTitle]);
        assert_eq!(net.nav(Nav::Down, &mut s, 0).or(net.nav(Nav::Confirm, &mut s, 0)), Some(Command::LetIn(4)));
    }
}
