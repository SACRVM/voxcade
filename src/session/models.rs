//! The models a game draws, by the names it gave them, and the title
//! scene's (`Models`).

use super::*;

/// The models a game draws, by the names it gave them (`Game::models`),
/// and the title scene's, by file.
pub struct Models {
    pub(super) actors: Vec<(ModelSpec, VoxModel)>,
    pub props: HashMap<String, VoxModel>,
}

/// The game's display font, if it names one (a broken one is reported and
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

impl Models {
    /// The game's models, and the parts its kinds swing checked against
    /// them (`c.things.NAME.swing`: a figure of parts, each part there).
    /// The game's models, `pieces` too (the models its levels set down,
    /// which `conf.lua` need not name).
    pub(super) fn load_for<G: Game>(files: &Files, cfg: &GameConfig, pieces: &[String]) -> Result<Self> {
        let mut specs = G::models(cfg)?;
        for file in pieces {
            if !specs.iter().any(|s| &s.file == file) {
                specs.push(ModelSpec { name: file.clone(), file: file.clone(), shaped: false });
            }
        }
        let models = Self::load(files, cfg, specs)?;
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
        let load = |file: &str| -> Result<VoxModel> {
            let mut model = vox::load_model_in(files, &files.path(file))?;
            let light = &cfg.light;
            model.shade(light.model_shading_for.get(file).copied().unwrap_or(light.model_shading), light.edge_tolerance);
            Ok(model)
        };
        Ok(Self {
            actors: specs
                .into_iter()
                .map(|spec| -> Result<(ModelSpec, VoxModel)> {
                    let model = load(&spec.file)?;
                    Ok((spec, model))
                })
                .collect::<Result<_>>()?,
            props: title_models(cfg).map(|f| Ok((f.to_string(), load(f)?))).collect::<Result<_>>()?,
        })
    }

    /// How many frames each of the game's models has, by name.
    pub fn frame_counts(&self) -> std::collections::BTreeMap<String, usize> {
        self.actors.iter().map(|(spec, m)| (spec.name.clone(), m.frames.len())).collect()
    }

    /// How tall each of the game's models is, by name (a piece's body).
    pub fn heights(&self) -> std::collections::BTreeMap<String, f32> {
        self.actors.iter().map(|(spec, m)| (spec.name.clone(), m.size.z as f32)).collect()
    }

    /// The parts of the game's figures of parts, by the model's name and
    /// by its file.
    pub fn part_names(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        self.actors
            .iter()
            .filter(|(_, m)| !m.parts.is_empty())
            .flat_map(|(spec, m)| {
                let names: Vec<String> = m.parts.iter().map(|p| p.name.clone()).collect();
                [(spec.name.clone(), names.clone()), (spec.file.clone(), names)]
            })
            .collect()
    }

    /// The game's model called `name`.
    pub fn get(&self, name: &str) -> Option<&VoxModel> {
        self.actors.iter().find(|(spec, _)| spec.name == name).map(|(_, m)| m)
    }

    /// The game's model called `name`, or kept in that file.
    pub fn named(&self, name: &str) -> Option<&VoxModel> {
        self.get(name).or_else(|| self.actors.iter().find(|(spec, _)| spec.file == name).map(|(_, m)| m))
    }

    /// A model as the simulation sees it: its size, and its voxels to burst
    /// into.
    pub fn shape(&self, name: &str) -> Option<Shape> {
        self.get(name).map(|m| Shape { size: m.size, voxels: m.frames[0].clone() })
    }

    /// Every model, in a fixed order: the game's as it named them, then the
    /// title's by file.
    pub(super) fn all_mut(&mut self) -> Vec<&mut VoxModel> {
        let mut props: Vec<_> = self.props.iter_mut().collect();
        props.sort_unstable_by(|a, b| a.0.cmp(b.0));
        self.actors.iter_mut().map(|(_, m)| m).chain(props.into_iter().map(|(_, m)| m)).collect()
    }

    /// The files of the game's models; `shaped`: only those the simulation
    /// reads.
    pub(super) fn files(&self, shaped: bool) -> impl Iterator<Item = &str> {
        self.actors.iter().filter(move |(spec, _)| spec.shaped || !shaped).map(|(spec, _)| spec.file.as_str())
    }
}
