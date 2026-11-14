//! The game's start: `main.lua` run, its handlers and first states, the
//! kinds from `conf.lua` and what they are checked for, the pieces and the
//! player stood in a level built.

use super::*;

impl<K: Kit> LuaGame<K> {
    /// Runs `main.lua` (anew), keeping the game's state: its functions,
    /// globals frozen after `on_world_start` (which runs only the first
    /// time).
    pub(super) fn run_main(&mut self, files: &Files, first: bool) -> Result<()> {
        let path = files.path("main.lua");
        let src = files.read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let code = lex::tokens("main.lua", &src)?;
        *self.sandbox.reading.borrow_mut() = sandbox::Reading::opened(files);
        let started = self.start_main(&src, first);
        let mut reading = self.sandbox.reading.borrow_mut();
        let given = reading.given();
        reading.close();
        let env = started?;
        self.data = std::mem::take(&mut reading.read);
        self.modules = std::mem::take(&mut reading.modules);
        let modules_name_music = reading.names_music;
        let modules_name_daylight = reading.names_daylight;
        let modules_call_camera = reading.calls_camera;
        drop(reading);
        let kept = self.sandbox.freeze(&env)?;
        (self.env, self.required) = (Some(env), given);
        self.tick = Sandbox::function(&kept, "on_tick")?;
        self.draw = Sandbox::function(&kept, "on_draw")?;
        self.globals = Some(kept);
        self.code = code;
        self.uses_music = modules_name_music || lex::names(&src, &["music", "on_beat"]);
        self.moves_sun = modules_name_daylight || lex::names(&src, &["daylight"]);
        self.moves_camera = modules_call_camera || lex::calls(&src, "camera");
        Ok(())
    }

    /// Runs `main.lua`'s top, and the first time `on_world_start` (no
    /// level yet) and then `on_level_start("start")`. A handler by the old
    /// API's name is refused with its new one.
    pub(super) fn start_main(&mut self, src: &str, first: bool) -> Result<Table> {
        let env = self.sandbox.run("main.lua", src)?;
        let line = |f: &Function| f.info().line_defined.map(|l| format!(":{l}")).unwrap_or_default();
        for (old, new) in OLD_NAMES {
            if let Some(f) = Sandbox::function(&env, old)? {
                anyhow::bail!("main.lua{}: function {old}(...) is the old API's name; it is {new} now (world, level, player, screen and controls are there everywhere, input in on_tick; docs/game-model.md)", line(&f));
            }
        }
        for (old, new) in RENAMED {
            if let Some(f) = Sandbox::function(&env, old)? {
                anyhow::bail!("main.lua{}: function {old} is called {new} now (the same parameters)", line(&f));
            }
        }
        for name in WITHOUT_PARAMETERS {
            if let Some(f) = Sandbox::function(&env, name)?
                && f.info().num_params > 0
            {
                anyhow::bail!("main.lua{}: {name}() takes no parameters: world, level, player, input and screen are there everywhere", line(&f));
            }
        }
        misspelt_handler(&env)?;
        if first {
            // The handlers called from here on (`on_level_end` of an ask in
            // the first `on_level_start`) are found in what main.lua made.
            self.globals = Some(env.clone());
            // A handler's mistake stops the game rather than its loading:
            // mended, it starts afresh (`start_failed`).
            if let Err(e) = self.start_handlers(&env) {
                self.started = false;
                self.halt(e);
            }
        }
        Ok(env)
    }

    /// `on_world_start` (no level yet), then `on_level_start("start")`,
    /// what they removed gone, their states checked.
    pub(super) fn start_handlers(&mut self, env: &Table) -> Result<()> {
        self.start_states()?;
        if let Some(start) = Sandbox::function(env, "on_world_start")? {
            let lua = &self.sandbox.lua;
            world::set_phase(lua, Some(world::Phase::WorldStart));
            let started = self.with_world(|sb, _| sb.call_start::<()>(&start, ()));
            world::set_phase(&self.sandbox.lua, None);
            started?;
        }
        if let Some(start) = Sandbox::function(env, "on_level_start")? {
            self.with_world(|sb, _| sb.call_start::<()>(&start, "start"))?;
        }
        self.stage.settle();
        self.arrive();
        self.check_states().map_err(|e| anyhow::anyhow!("{e:#} (at the start)"))?;
        self.write_saves();
        // An ask of the first on_level_start counts at once.
        self.change_levels();
        Ok(())
    }

    /// The states a run starts with (`c.state.world`, `c.state.player`),
    /// the engine's keys among the player's written to where the engine
    /// keeps them (`hp`, `score`, the shooter's `weapon`).
    pub(super) fn start_states(&mut self) -> Result<()> {
        let start = self.stage.host.sim().cfg.state.clone();
        let lua = &self.sandbox.lua;
        let world = self.registry("voxcade.state")?;
        for (key, v) in &start.world {
            world.raw_set(key.as_str(), world::from_toml(lua, v)?)?;
        }
        let player = self.registry("voxcade.player")?;
        let host = &mut self.stage.host;
        for (key, v) in &start.player {
            let engine = host.player_keys().contains(&key.as_str());
            match v {
                toml::Value::Integer(n) if engine => {
                    host.set_player_number(key, *n);
                }
                toml::Value::String(s) if engine => host.set_player_kept(key, host::Kept::Text(s.clone())).map_err(|e| anyhow::anyhow!("c.state.player.{key}: {e}"))?,
                _ if engine => anyhow::bail!("c.state.player.{key}: the engine keeps it, as a whole number"),
                _ => player.raw_set(key.as_str(), world::from_toml(lua, v)?)?,
            }
        }
        Ok(())
    }

    /// The kinds of things the config defines, their heights from their
    /// models (`old`: kept from there while a kind's model stays).
    pub(super) fn kinds(cfg: &GameConfig, height: impl Fn(&str, Option<&str>) -> f32) -> Result<Vec<Kind>> {
        let defs = things::defs(cfg, &Self::behaviors(cfg))?;
        // A family is asked for by its name where a kind's would do
        // (`level:things("enemies")`): never a kind's name.
        for d in &defs {
            if let Some(f) = d.families.iter().find(|f| defs.iter().any(|k| &k.name == *f)) {
                anyhow::bail!("c.things.{}.family: '{f}' is a kind's name; a family needs one of its own", d.name);
            }
        }
        // The players' figures of ready-made rules that play several (the
        // shooter's, of kinds without behavior) are theirs, not the
        // script's: their rules move them and keep their hit points.
        let figures: Vec<String> = match &cfg.player.kind {
            Some(k) if with::<K>(cfg) && cfg.players > 1 && K::plays_several(cfg) => k.names().into_iter().map(str::to_string).collect(),
            _ => Vec::new(),
        };
        Ok(defs
            .into_iter()
            .map(|d| {
                let height = height(&d.name, d.model.as_deref());
                let sound = d.sound.map(|s| crate::names::intern(&s));
                let figure = figures.contains(&d.name);
                Kind { name: d.name, model: d.model, height, hp: d.hp.unwrap_or(0), has_hp: d.hp.is_some() || d.behavior.is_some() || figure, scripted: d.behavior.is_none() && !figure, props: d.props, families: d.families, effect: d.effect, sound, reach: d.reach, team: d.team, free_turn: d.free_turn, spin: d.spin, fixed: d.fixed, frames: d.frames, swing: d.swing }
            })
            .collect())
    }

    /// The sounds' names: the kit's (or the engine's) and the game's own
    /// files'.
    pub(super) fn sound_names(cfg: &GameConfig) -> Arc<Vec<&'static str>> {
        Arc::new(crate::audio::sound::all(&Self::sounds(cfg), &cfg.sounds).into_iter().map(|(n, ..)| n).collect())
    }

    /// Places what the level's markers stand for, lifted out of anything
    /// solid.
    pub(super) fn place(host: &mut Plain, markers: &[Marker]) {
        let half = host.sim.half();
        for m in markers {
            let height = host.kinds[m.kind].height;
            let mut at = m.cell.as_vec3() + Vec3::new(0.5, 0.5, 0.0);
            while body_hits(&host.sim.world, at, half, height) && at.z < host.sim.world.size.z as f32 + 8.0 {
                at.z += 1.0;
            }
            host.spawn(m.kind, at.x, at.y, Some(at.z)).expect("a plain host places every kind");
        }
    }

    /// Sets down the pieces of the level being played (`grid::PIECE`, its
    /// `Here` fresh):
    /// each a thing of the piece kind standing exactly where the file puts
    /// its pivot, as tall as its model, facing its way, its light on. The
    /// camera keeps to what it followed.
    pub(super) fn spawn_pieces(&mut self) {
        let levels = Arc::clone(&self.stage.levels);
        let level = &levels.list[self.stage.level];
        let Some(kind) = self.stage.kinds.iter().position(|k| k.name == crate::grid::PIECE) else { return };
        let focus = self.plain().map(|h| h.focus);
        for p in &level.pieces {
            let at = p.cell.as_vec3() + Vec3::new(0.5, 0.5, 0.0);
            let Ok(id) = self.stage.host.spawn(kind, at.x, at.y, Some(at.z)) else { continue };
            let height = p.model.as_ref().and_then(|m| self.stage.heights.get(m)).copied().unwrap_or(1.0);
            let bearing = p.facing.bearing() as f32;
            let (sin, cos) = crate::sim::math::sin_cos(bearing.to_radians());
            if let Some((body, _)) = self.stage.host.get_mut(id).and_then(|t| t.moved) {
                body.height = height;
                // Its way exactly where it is one of the four (no rounding
                // off the grid), else its bearing.
                let way = match p.facing {
                    crate::grid::Facing::Way(w) => glam::IVec2::from(w.pair()).as_vec2().extend(0.0),
                    crate::grid::Facing::Turn(_) => Vec3::new(sin, cos, 0.0),
                };
                body.face(way);
                body.prev = body.pos;
            }
            if !p.lights.is_empty() {
                self.stage.here.lights.insert(id.raw(), 1.0);
            }
            self.stage.here.pieces.insert(id.raw(), p.clone());
        }
        if let (Some(h), Some(focus)) = (self.plain_mut(), focus) {
            h.focus = focus;
        }
    }

    /// The kind of the player's figure (`c.player.kind`), for a game
    /// without ready-made rules (theirs have their own player).
    pub(super) fn player_kind(cfg: &GameConfig, kinds: &[Kind]) -> Result<Option<usize>> {
        let Some(seats) = &cfg.player.kind else { return Ok(None) };
        // Ready-made rules that play several take a kind for each seat's
        // figure (seat 1's their own from the level); the others their own.
        let kit = with::<K>(cfg);
        if kit && (cfg.players <= 1 || !K::plays_several(cfg)) {
            anyhow::bail!("c.player.kind = \"{}\": the ready-made rules (c.kit) bring their own player", seats.of(1));
        }
        for name in seats.names() {
            if name == crate::grid::PIECE || !kinds.iter().any(|k| k.name == name) {
                let names: Vec<&str> = kinds.iter().map(|k| k.name.as_str()).filter(|n| *n != crate::grid::PIECE).collect();
                anyhow::bail!("c.player.kind = \"{name}\": no kind of thing called that (c.things: {})", names.join(", "));
            }
        }
        Ok(kinds.iter().position(|k| k.name == seats.of(1)).filter(|_| !kit))
    }

    /// A kind's `sound` is one the game has (its files in sounds/, the
    /// kit's).
    pub(super) fn check_kinds_sounds(cfg: &GameConfig, kinds: &[Kind]) -> Result<()> {
        let sounds = Self::sound_names(cfg);
        for k in kinds {
            if let Some(s) = k.sound.filter(|s| !sounds.contains(s)) {
                anyhow::bail!("c.things.{}.sound = \"{s}\": no sound called that (a sound is a file, sounds/{s}.toml; the game's and the ready-made rules': {})", k.name, sounds.join(", "));
            }
        }
        Ok(())
    }

    /// The seats (`c.players`) and the hold (`c.camera.hold`) are of a size
    /// the engine plays; the ready-made rules play one player yet. The
    /// picture's shape (`c.camera.aspect`) is one a window can show.
    pub(super) fn check_players(cfg: &GameConfig) -> Result<()> {
        if !(1..=crate::config::MOST_PLAYERS).contains(&cfg.players) {
            anyhow::bail!("c.players = {}: 1 to {}", cfg.players, crate::config::MOST_PLAYERS);
        }
        if cfg.players > 1 && with::<K>(cfg) && !K::plays_several(cfg) {
            anyhow::bail!("c.players = {}: the ready-made rules (c.kit = \"{}\") play one player so far", cfg.players, cfg.kit.as_deref().unwrap_or(""));
        }
        if let Some(hold) = &cfg.camera.hold
            && (hold.is_empty() || hold.len() > 2 || hold.iter().any(|h| h.is_nan() || *h <= 0.0))
        {
            anyhow::bail!("c.camera.hold: how far apart the players may be, across and deep ({{ 240, 150 }}, or {{ 240 }}), each above 0");
        }
        if let Some(aspect) = cfg.camera.aspect
            && !(0.25..=4.0).contains(&aspect)
        {
            anyhow::bail!("c.camera.aspect = {aspect}: the picture's width over its height, from 0.25 to 4 (2.39 a film's, 16 / 9 a television's)");
        }
        Ok(())
    }

    /// Every item holds at least one; the save slots are a menu's worth.
    pub(super) fn check_items(cfg: &GameConfig) -> Result<()> {
        for (name, item) in &cfg.items {
            if item.stack == 0 {
                anyhow::bail!("c.items.{name}.stack = 0: an inventory holds at least one");
            }
        }
        for (name, icon) in &cfg.icons {
            icon.check(&format!("c.icons.{name}")).map_err(anyhow::Error::msg)?;
        }
        if cfg.saves.slots > MAX_SLOTS {
            anyhow::bail!("c.saves.slots = {}: at most {MAX_SLOTS}", cfg.saves.slots);
        }
        Ok(())
    }

    /// Stands the player's figure (`c.player.kind`) at `place`, its feet
    /// at the place's height, facing its way if it has one.
    pub(super) fn stand_player(h: &mut Plain, place: &crate::levels::Place) {
        if let Some(id) = h.player {
            Self::stand_figure(h, id, place);
        }
    }

    /// Stands a figure at `place`, its feet at the place's height, facing
    /// its way if it has one.
    pub(super) fn stand_figure(h: &mut Plain, id: Id, place: &crate::levels::Place) {
        let half = h.sim.half();
        let Some(t) = h.things.get_mut(id) else { return };
        let at = crate::sim::stand_at(&h.sim.world, place.at, t.body.height, half);
        (t.body.pos, t.body.prev, t.body.vel) = (at, at, Vec3::ZERO);
        if let Some(way) = place.facing {
            t.body.face(way.as_vec2().extend(0.0));
        }
    }

    /// A world the script builds (`c.world.size`) was empty when the player
    /// was placed: it stands on what `on_level_start` built.
    pub(super) fn built(&mut self) {
        if self.stage.host.sim().cfg.world.size.is_none() {
            return;
        }
        if let Some(kit) = self.kit_mut() {
            return kit.built();
        }
        let h = self.plain_mut().expect("a plain host without a kit");
        let Some(id) = h.player else { return };
        let half = h.sim.half();
        let Some(t) = h.things.get_mut(id) else { return };
        while body_hits(&h.sim.world, t.body.pos, half, t.body.height) && t.body.pos.z < h.sim.world.size.z as f32 + 8.0 {
            t.body.pos.z += 1.0;
        }
        t.body.prev = t.body.pos;
    }
}
