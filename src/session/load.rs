//! Loading a game folder, its fingerprint for the replays, and taking in
//! what changed (hot reload, `apply` of sealed files).

use super::*;

impl<G: Game> Session<G> {
    /// Files changed while the game runs (APPLY): laid over its files and
    /// taken in one after another (`conf.lua`, the levels, the models, the
    /// code, the rest), the game's state kept as a reload keeps it. Part of
    /// the run, as a console line is (`Event::Apply`: a replay applies them
    /// again after the same step). Only a game of sealed files takes them,
    /// files a replay can carry (`Files::seal`). What was reloaded, the
    /// most far-reaching; a file that would not load is the game's trouble
    /// (`trouble`) as on a reload.
    pub fn apply(&mut self, changes: Arc<Changes>) -> Result<Reloaded> {
        anyhow::ensure!(self.files.is_sealed(), "files are applied to a game of sealed files only (crate::files::Files::seal)");
        self.files = self.files.changed(&changes);
        self.pending.push(Event::Apply(changes.clone()));
        self.applied = true;
        // A run applied to still starts from its files: it is kept.
        let touched = self.touched;
        let mut names: Vec<&String> = changes.keys().collect();
        names.sort_by_key(|name| self.apply_order(name));
        let mut most = Reloaded::Nothing;
        let mut failed = Ok(());
        for name in names {
            let path = self.files.path(name);
            match self.take_in(&path) {
                Ok(r) => most = most.max(r),
                Err(e) => failed = Err(e),
            }
        }
        self.touched = touched;
        failed.map(|()| most)
    }

    /// Where a changed file comes in an APPLY: the config first (it reloads
    /// the models and sounds too), the levels, the models, the code last.
    pub(super) fn apply_order(&self, name: &str) -> u8 {
        let path = self.files.path(name);
        if G::config_file() == Some(name) {
            0
        } else if name.starts_with("levels/") || self.level_files.contains(&path) || is_block_file(&self.dir, &path) {
            1
        } else if name.ends_with(".vvx") {
            2
        } else if name.ends_with(".lua") {
            3
        } else {
            4
        }
    }

    /// The scenarios a run of this game may begin from: each one's name
    /// and what it is about.
    pub fn scenarios(&mut self) -> Result<Vec<(String, String)>> {
        self.game.scenarios()
    }

    /// Loads the game folder `dir` as a game of kind `G`, its save slots
    /// for this run only.
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_with(dir, Slots::default())
    }

    /// Loads the game folder `dir` with these save slots (the player's on
    /// disk, `Slots::on_disk`, or a replay's).
    pub fn load_with(dir: &Path, slots: Slots) -> Result<Self> {
        Self::load_from(Files::folder(dir), slots)
    }

    /// Loads the game from its files (`crate::files`: the folder, or a set
    /// sealed) with these save slots.
    pub fn load_from(files: Files, mut slots: Slots) -> Result<Self> {
        let dir = files.dir().to_path_buf();
        let cfg = config_in::<G>(&files)?;
        slots.set_count(cfg.saves.slots);
        let levels = std::sync::Arc::new(crate::levels::load::<G>(&files, &cfg)?);
        let level_files = levels.files.clone();
        let piece_models = levels.piece_models();
        let models = Models::load_for::<G>(&files, &cfg, &piece_models)?;
        let first = &levels.list[0];
        let songs = song_timings(&files, &cfg);
        let game = G::load(Setup { files: &files, cfg: &cfg, world: first.world.clone(), markers: first.markers.clone(), models: &models, levels: levels.clone(), slots: slots.listing(), slot_texts: slots.texts(), replayed: slots.replayed().to_vec(), songs: songs.clone() })?;
        let cue = game.music().map(|c| c.song);
        let game_mark = game.level_mark();
        let sounds = SoundBank::load(&files, &cfg.sounds, &G::sounds(&cfg))?;
        let actions = G::actions(&cfg);
        let song_titles = song_titles(&files, &cfg);
        let cfg_ui = cfg.ui.clone();
        let seats = session_seats(&cfg);
        let mut session = Self {
            dir,
            cfg,
            game,
            models,
            level_files,
            piece_models,
            sounds,
            song_titles,
            songs,
            cue,
            music_changed: true,
            fade_clock: false,
            crossfade: None,
            fade_cut: false,
            fades: false,
            fade: None,
            flyby_from: 0.0,
            ticked: false,
            song: 0,
            options: Options::default(),
            font: display_font(&files, &cfg_ui),
            bank: Default::default(),
            entered: game_mark,
            logo: Vec::new(),
            logo_model: None,
            props_ground: Vec::new(),
            touched: false,
            problem_shown: true,
            applied: false,
            controls: Controls::nobody(&actions),
            seats: vec![crate::game::Seat { input: None, controls: Controls::nobody(&actions) }; seats],
            actions,
            pending: Vec::new(),
            played: Tick::default(),
            inputs: Default::default(),
            clock: crate::saves::now(),
            local: Default::default(),
            problem: None,
            drawn_sun: Default::default(),
            seen_faces: Default::default(),
            hud_kept: Default::default(),
            limits: Default::default(),
            slots_at_start: slots.headers(),
            slots,
            started_from: None,
            scenario: None,
            evals: Vec::new(),
            files,
        };
        // Every level's pieces were read for what the simulation reads of
        // them: those the level entered does not name are let go.
        session.models.needed.let_go(&session.game.level_models());
        session.rebuild_bank();
        session.place_props();
        // A save asked for in the first on_level_start.
        session.saves_due();
        Ok(session)
    }

    /// Finds the ground under each of the title's props: its usual height
    /// under them, so a cactus at the edge does not lift them. The title's
    /// world never changes (back to the title the game loads afresh); after
    /// the level or `conf.lua` reloads they are placed again.
    pub(super) fn place_props(&mut self) {
        let world = &self.game.sim().world;
        self.props_ground = self
            .cfg
            .title_screen
            .props
            .iter()
            .map(|prop| {
                let Some(m) = self.models.props.get(&prop.model) else { return 0 };
                let reach = if prop.rotation % 180 != 0 { IVec2::new(m.size.y, m.size.x) } else { m.size.truncate() } / 2;
                let at = IVec2::new(prop.at[0] as i32, prop.at[1] as i32);
                let mut tops = Vec::new();
                for y in (at.y - reach.y..=at.y + reach.y).step_by(3) {
                    for x in (at.x - reach.x..=at.x + reach.x).step_by(3) {
                        tops.push(world.column_top(x, y));
                    }
                }
                tops.sort_unstable();
                tops.get(tops.len() / 2).copied().unwrap_or(0)
            })
            .collect();
    }

    /// What a run of the game depends on, as one number: the simulation's
    /// part of the config, the game's actions and code, the level's files
    /// and the figures' models. A replay carries it and is not played on
    /// other data; changes to the look, the sound or the title leave it
    /// alone.
    pub fn fingerprint(&self) -> Result<u64> {
        let files = self.fingerprinted()?;
        fingerprint_of(&self.cfg, &G::sections(&self.cfg), &self.actions, &files)
    }

    /// The fingerprint's parts by name, each as one number: two machines
    /// whose games differ learn which file does ("levels/3.toml").
    pub fn fingerprint_files(&self) -> Result<Vec<(String, u64)>> {
        let files = self.fingerprinted()?;
        let parts = fingerprint_parts(&self.cfg, &G::sections(&self.cfg), &self.actions, &files)?;
        Ok(parts.into_iter().map(|(name, bytes)| (name, crate::hash::Stable::default().bytes(&bytes).finish())).collect())
    }

    /// The files the fingerprint reads, by their names.
    pub(super) fn fingerprinted(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut paths: Vec<PathBuf> = self.level_files.clone();
        paths.extend(self.models.files(true).map(|f| self.dir.join(f)));
        paths.sort();
        paths.dedup();
        let mut files = Vec::new();
        for path in paths {
            // Named the same on every system: relative, with forward slashes.
            let name = crate::files::slashed(path.strip_prefix(&self.dir).unwrap_or(&path));
            let bytes = self.files.read(&path).with_context(|| format!("reading {}", path.display()))?;
            files.push((name, bytes));
        }
        files.extend(self.game.code());
        // A game that reads or picks its music: the songs' beats.
        if self.game.uses_music() {
            let beats: String = self.songs.iter().map(|t| format!("{} {} {}\n", t.name, t.tempo, t.meter)).collect();
            files.push(("music".into(), beats.into_bytes()));
        }
        Ok(files)
    }

    /// The models read anew from the files (the kinds', the pieces', the
    /// title's) and the game's shapes from them: the run no longer starts
    /// from the files it began on.
    fn reload_models(&mut self) -> Result<()> {
        self.models = Models::load_for::<G>(&self.files, &self.cfg, &self.piece_models)?;
        self.game.reshape(&self.models);
        self.touched = true;
        Ok(())
    }

    /// The models read anew shown: those the level no longer draws let go,
    /// the bank built anew, the title's props on their ground again.
    fn show_models(&mut self) {
        self.models.needed.let_go(&self.game.level_models());
        self.rebuild_bank();
        self.place_props();
    }

    /// Puts the models and the title logo's letters in a new model bank
    /// (after any of them changed: a model, the font, the logo in
    /// `conf.lua`); the renderer uploads it with the next frame.
    pub(super) fn rebuild_bank(&mut self) {
        let t = &self.cfg.title_screen;
        let letters = crate::ui::logo_letters(&self.logo_lines(), &t.logo_heights, &t.logo_cracks, self.font.as_deref(), &self.cfg.ui);
        let own = t.logo_model.as_ref().and_then(|f| self.models.props.get(f)).map(crate::ui::logo_model_parts);
        let mut bank = ModelBank::default();
        for model in self.models.all_mut() {
            bank.add(model);
        }
        self.logo = letters.iter().map(|line| line.iter().map(|cubes| bank.add_cubes(cubes)).collect()).collect();
        self.logo_model = own.map(|(size, parts)| crate::ui::LogoModel { size, parts: parts.iter().map(|(cubes, at)| (bank.add_cubes(cubes), *at)).collect() });
        // The models loaded when needed last: the bank grows by those to
        // come (`bank_fresh`).
        self.models.needed.bank(&mut bank, true);
        *self.bank.borrow_mut() = std::sync::Arc::new(bank);
    }

    /// The model bank as it stands (what a frame draws from).
    pub fn bank(&self) -> std::sync::Arc<ModelBank> {
        self.bank.borrow().clone()
    }

    /// Puts the models loaded since the bank was made (drawn the first
    /// time, `frame:model`) in it: a new bank of the voxels there and
    /// theirs after them, which the renderer uploads with the next frame.
    /// Whether there were any.
    pub(super) fn bank_fresh(&self) -> bool {
        if !self.models.needed.fresh() {
            return false;
        }
        let mut bank = ModelBank { voxels: self.bank.borrow().voxels.clone() };
        self.models.needed.bank(&mut bank, false);
        *self.bank.borrow_mut() = std::sync::Arc::new(bank);
        true
    }

    /// Whether a level was entered since the last look (a change, a
    /// restart, a console line's, a save or a scenario begun, a state
    /// brought back: `Game::level_mark` moved on): the models loaded when
    /// needed but those it names (its pieces', `Game::level_models`) are
    /// let go, out of memory and the bank, and its own are loaded, so that
    /// what one level alone shows does not lie in memory for the rest of
    /// the run. Only the picture reads them.
    pub(super) fn enter_level(&mut self) {
        let mark = self.game.level_mark();
        if mark == self.entered {
            return;
        }
        self.entered = mark;
        let keep = self.game.level_models();
        let dropped = self.models.needed.let_go(&keep);
        self.models.needed.load_all(&keep);
        if dropped {
            self.rebuild_bank();
        } else {
            self.bank_fresh();
        }
    }

    /// The logo's lines and their colours: `[title_screen] logo`, else the
    /// game's title.
    pub(super) fn logo_lines(&self) -> Vec<(String, [u8; 4])> {
        let t = &self.cfg.title_screen;
        let lines: Vec<String> = if t.logo.is_empty() { vec![self.cfg.title.to_uppercase()] } else { t.logo.clone() };
        lines
            .into_iter()
            .enumerate()
            .map(|(i, l)| (l, t.logo_colors.get(i).map_or([240, 230, 204, 255], |c| crate::ui::hex_color(c))))
            .collect()
    }

    /// Applies a file that changed in the game folder, keeping the game running.
    /// A failed reload is kept to show over the game (`problem`) until the
    /// next one works. A game of sealed files is not touched by the folder
    /// (its changes come by `apply`).
    pub fn reload(&mut self, changed: &Path) -> Result<Reloaded> {
        if self.files.is_sealed() {
            return Ok(Reloaded::Nothing);
        }
        // A file gone counts in the sounds' folder only (a sound taken
        // away); elsewhere the game keeps what it read.
        if !changed.exists() && !audio::sound::in_folder(&self.dir, changed) {
            return Ok(Reloaded::Nothing);
        }
        self.take_in(changed)
    }

    /// Takes in a file of the game's files that changed.
    pub(super) fn take_in(&mut self, changed: &Path) -> Result<Reloaded> {
        let reloaded = self.reload_file(changed);
        match &reloaded {
            Ok(Reloaded::Nothing) => {}
            Ok(_) => self.problem = None,
            Err(e) => self.problem = Some(format!("{e:#}")),
        }
        reloaded
    }

    pub(super) fn reload_file(&mut self, changed: &Path) -> Result<Reloaded> {
        // A model loaded when needed from now on comes from the files as
        // they are (an APPLY changed them).
        self.models.needed.set_files(&self.files);
        let name = changed.file_name();
        let is = |file: &str| name == Path::new(file).file_name();
        if let Some(file) = G::config_file().filter(|f| is(f)) {
            let before = self.cfg.audio.music.clone();
            let mut cfg = config_in::<G>(&self.files)?;
            let waiting = self.cfg.restart_only(&cfg);
            if !waiting.is_empty() {
                self.cfg.keep_for_restart(&mut cfg);
                eprintln!("{file}: {} take effect after a restart (F5)", waiting.join(", "));
            }
            self.cfg = cfg;
            self.song_titles = song_titles(&self.files, &self.cfg);
            self.font = display_font(&self.files, &self.cfg.ui);
            self.retime_songs();
            self.music_changed |= self.cfg.audio.music != before;
            self.game.apply_config(&self.cfg)?;
            // As many save slots as the config says now.
            self.slots.set_count(self.cfg.saves.slots);
            self.game.set_slots(self.slots.listing(), self.slots.texts());
            if let Some(world) = G::level(&self.cfg)? {
                self.game.replace_world(world, &[]);
            }
            // Kinds of things may have come, gone or changed their models.
            self.reload_models()?;
            self.sounds = SoundBank::load(&self.files, &self.cfg.sounds, &G::sounds(&self.cfg))?;
            self.show_models();
            // The camera or sun may have moved, which changes the face set.
            return Ok(Reloaded::World);
        }
        let mask = self.face_mask();
        if self.game.reload_code(&self.files, changed)? {
            if self.game.start_failed() && self.game.halted().is_none() {
                // The start was stopped half way: begin afresh from the
                // files, as F5 does (it may stop again, mended or not).
                let slots = self.slots.clone();
                *self = Self::load_from(self.files.clone(), slots)?;
                return Ok(Reloaded::World);
            }
            self.touched = true;
            // A sun the game moves now (or no more) wants other faces.
            if self.face_mask() != mask {
                return Ok(Reloaded::World);
            }
            return Ok(Reloaded::Code);
        }
        if self.cfg.audio.music.files().iter().chain(self.song_file().as_slice()).any(|f| same_file(&self.dir.join(f), changed)) {
            self.retime_songs();
            self.music_changed = true;
            return Ok(Reloaded::Music);
        }
        if self.cfg.ui.display_font.as_deref().is_some_and(is) {
            self.font = display_font(&self.files, &self.cfg.ui);
            self.rebuild_bank();
            return Ok(Reloaded::Models);
        }
        if audio::sound::in_folder(&self.dir, changed) {
            return self.reload_sounds();
        }
        let block = self.cfg.world.blocks.is_some() && is_block_file(&self.dir, changed);
        // A level file new in `levels/` (not for a world the script builds).
        let ending = crate::levels::ending(&self.cfg);
        let new_level = self.cfg.world.size.is_none() && changed.parent().is_some_and(|d| same_file(d, &self.dir.join("levels"))) && (changed.extension().is_some_and(|e| e == ending) || crate::levels::says_level(&self.files, changed));
        if block || new_level || self.level_files.iter().any(|f| same_file(f, changed)) {
            let levels = std::sync::Arc::new(crate::levels::load::<G>(&self.files, &self.cfg)?);
            self.game.check_levels(&levels)?;
            self.level_files = levels.files.clone();
            // The pieces' models may have come, gone or changed.
            self.piece_models = levels.piece_models();
            self.reload_models()?;
            self.game.replace_levels(levels);
            self.show_models();
            return Ok(Reloaded::World);
        }
        if self.models.files(false).chain(self.models.piece_files()).any(is) || title_models(&self.cfg).any(is) {
            self.reload_models()?;
            self.show_models();
            return Ok(Reloaded::Models);
        }
        // Any other model file of the game, drawn only: loaded anew the
        // next time it is drawn (one new in the folder at once), a drawing
        // that stopped for want of it tried again; the run goes on as a
        // replay would (the picture alone).
        if crate::vvx::is_model(changed) && self.files.name(changed).is_some() {
            let held = self.models.needed.forget(&self.files, changed);
            if held {
                self.rebuild_bank();
            }
            if held | self.game.models_came() {
                return Ok(Reloaded::Models);
            }
        }
        Ok(Reloaded::Nothing)
    }

    /// A file in `sounds/` came, changed or went: the sounds are read
    /// again (a mistake keeps them as they were), and only those that
    /// differ are made anew. A name come or gone is the game's to know
    /// (`Game::sounds_changed`: `level:sound` finds it).
    pub(super) fn reload_sounds(&mut self) -> Result<Reloaded> {
        let own = audio::sound::read_folder(&self.files)?;
        let names = |own: &BTreeMap<String, audio::GameSound>| own.keys().cloned().collect::<Vec<_>>();
        if names(&own) != names(&self.cfg.sounds) {
            let mut cfg = self.cfg.clone();
            cfg.sounds = own.clone();
            self.game.sounds_changed(&cfg)?;
        }
        let brought = G::sounds(&self.cfg);
        let before = audio::sound::all(&brought, &self.cfg.sounds);
        let now = audio::sound::all(&brought, &own);
        for (name, def, file) in &now {
            if !before.iter().any(|(n, d, _)| n == name && d == def) {
                self.sounds.put(&self.files, name, def).with_context(|| file.clone().unwrap_or_else(|| format!("{name} (built in)")))?;
            }
        }
        for (name, ..) in before.iter().filter(|(n, ..)| !now.iter().any(|(m, ..)| m == n)) {
            self.sounds.put(&self.files, name, &audio::SoundDef::Silent)?;
        }
        self.cfg.sounds = own;
        Ok(Reloaded::Sounds)
    }
}
