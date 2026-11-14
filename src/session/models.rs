//! The models a game draws (`Models`): those it keeps for the whole run
//! (its kinds', items' and effects', the title scene's), and those loaded
//! when needed (`Needed`: a level's pieces as it is entered, any other
//! model file of the game the first time it is drawn, `frame:model`), let
//! go when the level is left.

use std::sync::Mutex;

use super::*;

/// The models a game draws. The simulation sees only what is the same
/// whether a model is in memory or not: the kept models (`get`, `shape`)
/// and what it reads of the levels' pieces (`frame_counts`, `heights`,
/// `part_names`); the picture finds any of them (`drawn`).
pub struct Models {
    /// Kept for the whole run, by the names the game gave them
    /// (`Game::models`): its kinds' (their shapes count for the
    /// simulation), its items' and its kinds' effects'.
    pub(super) actors: Vec<(ModelSpec, VoxModel)>,
    /// The title scene's, by file, kept too.
    pub props: HashMap<String, VoxModel>,
    /// What the simulation reads of the models every level's pieces show,
    /// by file: worked out as the game loads, so it never depends on which
    /// of them lie in memory.
    pieces: BTreeMap<String, Facts>,
    /// The rest, loaded when needed and let go again.
    pub(super) needed: Arc<Needed>,
}

/// What the simulation reads of a model: its frames, its height (a piece's
/// body), its parts' names (a pose's).
#[derive(Clone, Debug)]
struct Facts {
    frames: usize,
    height: f32,
    parts: Vec<String>,
}

impl Facts {
    fn of(m: &VoxModel) -> Self {
        Self { frames: m.frames.len(), height: m.size.z as f32, parts: m.parts.iter().map(|p| p.name.clone()).collect() }
    }
}

/// A model as the picture finds it: one kept for the run, or one loaded
/// when needed (`Needed`).
pub enum ModelRef<'a> {
    Kept(&'a VoxModel),
    Needed(Arc<VoxModel>),
}

impl std::ops::Deref for ModelRef<'_> {
    type Target = VoxModel;

    fn deref(&self) -> &VoxModel {
        match self {
            ModelRef::Kept(m) => m,
            ModelRef::Needed(m) => m,
        }
    }
}

/// The models loaded when needed, by their name in the game (relative,
/// `/` between folders): a level's pieces' as it is entered, any other
/// model file of the game the first time it is drawn (`frame:model`), each
/// with its mistake if it would not load, kept until the level is left
/// (`let_go`) or its file changes (`forget`). Only the picture reads
/// them: what is in memory never changes the simulation. A model new here
/// is drawn as cubes until it is put in the bank (`fresh`, `bank_fresh`).
pub struct Needed {
    files: Mutex<Files>,
    light: crate::config::LightConfig,
    held: Mutex<BTreeMap<String, std::result::Result<Arc<VoxModel>, String>>>,
}

/// A model that would not load, said by the setting that names it
/// (`c.things.foe.model`, `c.items.NAME.model`, the title's) and, for a
/// file that is not there, in plain words.
fn model_error(cfg: &GameConfig, file: &str, e: anyhow::Error) -> anyhow::Error {
    let names = |v: Option<&toml::Value>| v.and_then(toml::Value::as_str) == Some(file);
    let by = cfg
        .things
        .iter()
        .find(|(_, t)| names(t.get("model")))
        .map(|(kind, _)| format!("c.things.{kind}.model"))
        .or_else(|| cfg.items.iter().find(|(_, i)| i.model.as_deref() == Some(file)).map(|(name, _)| format!("c.items.{name}.model")))
        .or_else(|| title_models(cfg).any(|m| m == file).then(|| "c.title_screen".to_string()));
    match (by, missing(&e)) {
        (Some(by), true) => anyhow::anyhow!("{by}: no file \"{file}\" in the game folder"),
        (Some(by), false) => e.context(format!("{by}: \"{file}\"")),
        (None, true) => anyhow::anyhow!("the model \"{file}\": no such file in the game folder"),
        (None, false) => e,
    }
}

/// Whether a mistake is a file not there.
fn missing(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound))
}

/// A model file read and shaded as the game's light says.
fn read_model(files: &Files, light: &crate::config::LightConfig, file: &str) -> Result<VoxModel> {
    let mut model = vox::load_model_in(files, &files.path(file))?;
    model.shade(light.model_shading_for.get(file).copied().unwrap_or(light.model_shading), light.edge_tolerance);
    Ok(model)
}

/// Model files read on the cores there are, each as `read_model` reads
/// it, in their order.
fn read_models(files: &Files, light: &crate::config::LightConfig, names: &[String]) -> Vec<Result<VoxModel>> {
    crate::par::in_order(names, 1, |part| part.iter().map(|f| read_model(files, light, f)).collect())
}

/// The display font, if the game names one (a broken one is reported and
/// left out: the pixel font stands in).
pub(super) fn display_font(files: &Files, ui: &crate::config::UiConfig) -> Option<Arc<DisplayFont>> {
    let file = ui.display_font.as_ref()?;
    DisplayFont::load_in(files, &files.path(file)).map_err(|e| eprintln!("display font: {e:#}")).ok().map(Arc::new)
}

/// The files of the title scene's models.
pub(super) fn title_models(cfg: &GameConfig) -> impl Iterator<Item = &str> {
    let t = &cfg.title_screen;
    t.props.iter().map(|p| p.model.as_str()).chain(t.flyby.as_deref()).chain(t.logo_model.as_deref())
}

/// Whether a model is not in the bank yet (one of no frames never is).
fn unbanked(m: &VoxModel) -> bool {
    m.bank.is_empty() && !m.frames.is_empty()
}

impl Needed {
    fn new(files: &Files, cfg: &GameConfig) -> Self {
        Self { files: Mutex::new(files.clone()), light: cfg.light.clone(), held: Mutex::new(BTreeMap::new()) }
    }

    /// The game's files from now on (an APPLY changed them).
    pub(super) fn set_files(&self, files: &Files) {
        *self.files.lock().expect("the files") = files.clone();
    }

    /// The model file `file` of the game (its path in the game folder,
    /// "props/sheet.vvx" too), loaded now if it is not yet; a file that is
    /// not there or no model is a mistake naming it (and stays one until
    /// it changes or the level is left).
    pub fn load(&self, file: &str) -> Result<Arc<VoxModel>> {
        if let Some(found) = self.held.lock().expect("the models").get(file) {
            return found.clone().map_err(anyhow::Error::msg);
        }
        let files = self.files.lock().expect("the files").clone();
        let Some(name) = files.name(Path::new(file)).filter(|_| !Path::new(file).is_absolute()) else {
            anyhow::bail!("\"{file}\" is no file of the game: a model is named by its path in the game folder, as \"sheet.vvx\" or \"props/sheet.vvx\"");
        };
        if let Some(found) = self.held.lock().expect("the models").get(&name) {
            return found.clone().map_err(anyhow::Error::msg);
        }
        let read = read_model(&files, &self.light, &name).map(Arc::new).map_err(|e| {
            let e = if missing(&e) { anyhow::anyhow!("no model file \"{name}\" in the game folder") } else { e };
            format!("{e:#}")
        });
        self.held.lock().expect("the models").insert(name, read.clone());
        read.map_err(anyhow::Error::msg)
    }

    /// Loads every one of `files` not held yet, on the cores there are (a
    /// level's pieces as it is entered); a mistake is said and kept, as
    /// `load` keeps it.
    pub(super) fn load_all(&self, files: &[String]) {
        let wanted: Vec<String> = {
            let held = self.held.lock().expect("the models");
            files.iter().filter(|f| !held.contains_key(*f)).cloned().collect()
        };
        let game = self.files.lock().expect("the files").clone();
        for (file, read) in wanted.iter().zip(read_models(&game, &self.light, &wanted)) {
            if let Err(e) = &read {
                eprintln!("{e:#}");
            }
            self.held.lock().expect("the models").insert(file.clone(), read.map(Arc::new).map_err(|e| format!("{e:#}")));
        }
    }

    /// The model `file` if it loads (else nothing is drawn of it).
    fn get(&self, file: &str) -> Option<Arc<VoxModel>> {
        self.load(file).ok()
    }

    /// Lets go of every model but those in `keep` (a level was left): what
    /// the new level names stays. Whether one that lay in the bank went.
    pub(crate) fn let_go(&self, keep: &[String]) -> bool {
        let mut held = self.held.lock().expect("the models");
        let before = held.values().filter(|m| m.as_ref().is_ok_and(|m| !m.bank.is_empty())).count();
        held.retain(|name, _| keep.contains(name));
        before != held.values().filter(|m| m.as_ref().is_ok_and(|m| !m.bank.is_empty())).count()
    }

    /// Forgets the model of a file that changed (or came: a mistake for
    /// it is no more): the next draw loads it anew. Whether it was held.
    pub(super) fn forget(&self, files: &Files, changed: &Path) -> bool {
        let Some(name) = files.name(changed) else { return false };
        self.held.lock().expect("the models").remove(&name).is_some()
    }

    /// Whether a model loaded since the bank was made waits to go in it.
    pub(super) fn fresh(&self) -> bool {
        self.held.lock().expect("the models").values().any(|m| m.as_ref().is_ok_and(|m| unbanked(m)))
    }

    /// Puts the models in the bank, every one (`all`, a bank made anew) or
    /// those not in it yet, in the order of their names.
    pub(super) fn bank(&self, bank: &mut ModelBank, all: bool) {
        for m in self.held.lock().expect("the models").values_mut().filter_map(|m| m.as_mut().ok()) {
            if all || unbanked(m) {
                bank.add(Arc::make_mut(m));
            }
        }
    }

    /// The models held now, by name (those that loaded).
    pub fn names(&self) -> Vec<String> {
        self.held.lock().expect("the models").iter().filter(|(_, m)| m.is_ok()).map(|(n, _)| n.clone()).collect()
    }
}

impl Models {
    /// The game's models: those it keeps (`Game::models`), with the parts
    /// its kinds swing checked against them (`c.things.NAME.swing`: a
    /// figure of parts, each part there); and the models its levels'
    /// `pieces` show (which `conf.lua` need not name), each read once for
    /// what the simulation reads of it and held until a level that does
    /// not name it is entered (`Session::enter_level`).
    pub(super) fn load_for<G: Game>(files: &Files, cfg: &GameConfig, pieces: &[String]) -> Result<Self> {
        let mut models = Self::load(files, cfg, G::models(cfg)?)?;
        let pieces: Vec<String> = pieces.iter().filter(|f| !models.actors.iter().any(|(s, _)| &s.file == *f)).cloned().collect();
        for (file, read) in pieces.iter().zip(read_models(files, &cfg.light, &pieces)) {
            let model = Arc::new(read.map_err(|e| model_error(cfg, file, e))?);
            models.pieces.insert(file.clone(), Facts::of(&model));
            models.needed.held.lock().expect("the models").insert(file.clone(), Ok(model));
        }
        for d in crate::sim::things::defs(cfg, &G::behaviors(cfg))? {
            if d.swing.is_empty() {
                continue;
            }
            let Some(m) = d.model.as_deref().and_then(|file| models.named(file)) else {
                anyhow::bail!("c.things.{}.swing: a kind with a model of parts swings them (it has none)", d.name);
            };
            if m.parts.is_empty() {
                anyhow::bail!("c.things.{}.swing: {} is no figure of parts (its objects named in the file: head, body ...)", d.name, d.model.as_deref().unwrap_or_default());
            }
            if let Some(missing) = d.swing.keys().find(|k| !m.parts.iter().any(|p| &p.name == *k)) {
                let names: Vec<&str> = m.parts.iter().map(|p| p.name.as_str()).collect();
                anyhow::bail!("c.things.{}.swing.{missing}: no such part (its parts: {})", d.name, names.join(", "));
            }
        }
        Ok(models)
    }

    pub(super) fn load(files: &Files, cfg: &GameConfig, specs: Vec<ModelSpec>) -> Result<Self> {
        let load = |file: &str| read_model(files, &cfg.light, file).map_err(|e| model_error(cfg, file, e));
        Ok(Self {
            actors: specs
                .into_iter()
                .map(|spec| -> Result<(ModelSpec, VoxModel)> {
                    let model = load(&spec.file)?;
                    Ok((spec, model))
                })
                .collect::<Result<_>>()?,
            props: title_models(cfg).map(|f| Ok((f.to_string(), load(f)?))).collect::<Result<_>>()?,
            pieces: BTreeMap::new(),
            needed: Arc::new(Needed::new(files, cfg)),
        })
    }

    /// Models without files: these kept, nothing else (tests).
    #[cfg(test)]
    pub(super) fn kept(actors: Vec<(ModelSpec, VoxModel)>) -> Self {
        Self { actors, props: HashMap::new(), pieces: BTreeMap::new(), needed: Arc::new(Needed::new(&Files::disk(), &GameConfig::default())) }
    }

    /// How many frames each of the game's models has, by name: the kept
    /// ones and the levels' pieces' (`level:effect`, a piece's pose).
    pub fn frame_counts(&self) -> BTreeMap<String, usize> {
        let pieces = self.pieces.iter().map(|(f, p)| (f.clone(), p.frames));
        pieces.chain(self.actors.iter().map(|(spec, m)| (spec.name.clone(), m.frames.len()))).collect()
    }

    /// How tall each of the game's models is, by name (a piece's body).
    pub fn heights(&self) -> BTreeMap<String, f32> {
        let pieces = self.pieces.iter().map(|(f, p)| (f.clone(), p.height));
        pieces.chain(self.actors.iter().map(|(spec, m)| (spec.name.clone(), m.size.z as f32))).collect()
    }

    /// The parts of the game's figures of parts, by the model's name and
    /// by its file.
    pub fn part_names(&self) -> BTreeMap<String, Vec<String>> {
        let pieces = self.pieces.iter().filter(|(_, p)| !p.parts.is_empty()).map(|(f, p)| (f.clone(), p.parts.clone()));
        pieces
            .chain(self.actors.iter().filter(|(_, m)| !m.parts.is_empty()).flat_map(|(spec, m)| {
                let names: Vec<String> = m.parts.iter().map(|p| p.name.clone()).collect();
                [(spec.name.clone(), names.clone()), (spec.file.clone(), names)]
            }))
            .collect()
    }

    /// The game's model called `name`, of those it keeps (what the
    /// simulation may read).
    pub fn get(&self, name: &str) -> Option<&VoxModel> {
        self.actors.iter().find(|(spec, _)| spec.name == name).map(|(_, m)| m)
    }

    /// The game's kept model called `name`, or kept in that file.
    pub fn named(&self, name: &str) -> Option<&VoxModel> {
        self.get(name).or_else(|| self.actors.iter().find(|(spec, _)| spec.file == name).map(|(_, m)| m))
    }

    /// A model as the picture finds it: a kept one by its name or file,
    /// else any model file of the game, loaded now if it is not yet
    /// (`Needed`); None if it would not load.
    pub fn drawn(&self, name: &str) -> Option<ModelRef<'_>> {
        match self.named(name) {
            Some(m) => Some(ModelRef::Kept(m)),
            None => self.needed.get(name).map(ModelRef::Needed),
        }
    }

    /// `drawn`, or the mistake why there is none (`voxcade sheet`).
    pub fn drawn_or_why(&self, name: &str) -> Result<ModelRef<'_>> {
        match self.named(name) {
            Some(m) => Ok(ModelRef::Kept(m)),
            None => self.needed.load(name).map(ModelRef::Needed),
        }
    }

    /// The models loaded when needed (`frame:model` asks them).
    pub fn needed(&self) -> Arc<Needed> {
        self.needed.clone()
    }

    /// A model as the simulation sees it: its size, and its voxels to burst
    /// into.
    pub fn shape(&self, name: &str) -> Option<Shape> {
        self.get(name).map(|m| Shape { size: m.size, voxels: m.frames[0].clone() })
    }

    /// The kept models, in a fixed order: the game's as it named them, then
    /// the title's by file.
    pub(super) fn all_mut(&mut self) -> Vec<&mut VoxModel> {
        let mut props: Vec<_> = self.props.iter_mut().collect();
        props.sort_unstable_by(|a, b| a.0.cmp(b.0));
        self.actors.iter_mut().map(|(_, m)| m).chain(props.into_iter().map(|(_, m)| m)).collect()
    }

    /// The files of the game's kept models; `shaped`: only those the
    /// simulation reads.
    pub(super) fn files(&self, shaped: bool) -> impl Iterator<Item = &str> {
        self.actors.iter().filter(move |(spec, _)| spec.shaped || !shaped).map(|(spec, _)| spec.file.as_str())
    }

    /// The files of the models the levels' pieces show.
    pub(super) fn piece_files(&self) -> impl Iterator<Item = &str> {
        self.pieces.keys().map(String::as_str)
    }
}
