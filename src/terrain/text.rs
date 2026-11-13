//! A terrain's text: the `[terrain]` table of a level read, every mistake
//! with its line, and its words written as the file has them.

use super::*;

/// A grid of words as a file has it: its rows of words, the line its
/// first row is on, its item.
pub(super) type Grid<'a> = (Vec<Vec<String>>, usize, &'a Item);

/// The `[terrain]` table of a level's text, every mistake with its line.
pub(crate) fn read(r: &Reader, item: &Item) -> Result<Terrain> {
    let t = item.as_table_like().ok_or_else(|| r.fail(item.span(), "terrain must be a table: [terrain]"))?;
    r.only(t, &["spacing", "at", "heights", "shapes", "terrace", "rough", "roughness", "dip", "dip_to", "fold", "fault", "seed", "under", "layer", "material", "scatter"], "[terrain]")?;
    // Its line: its header's.
    let first = t.iter().filter_map(|(_, v)| v.span()).map(|s| s.start).min().unwrap_or(0);
    let line = r.src[..first].rfind("[terrain]").map_or(0, |at| vvx::line_of(r.src, at));
    let spacing = match t.get("spacing") {
        None => bail!("line {line}: [terrain] has no spacing = N (voxels between its points)"),
        Some(i) => {
            let v = r.number(i, "spacing")?;
            if v.fract() != 0.0 || !(1.0..=1024.0).contains(&v) {
                return Err(r.fail(i.span(), "spacing must be a whole number of voxels, 1 to 1024"));
            }
            v as i32
        }
    };
    let at = match t.get("at") {
        None => [0; 3],
        Some(i) => {
            let a = r.ints(i, "at")?;
            if a.cmplt(IVec3::ZERO).any() {
                return Err(r.fail(i.span(), "at must be 0 or more each way (the terrain's low corner in the level)"));
            }
            a.to_array()
        }
    };
    let grid = |key: &str| -> Result<Option<Grid>> {
        let Some(i) = t.get(key) else { return Ok(None) };
        let text = i.as_str().ok_or_else(|| r.fail(i.span(), format!("{key} must be a text between \"\"\" marks, a row a line")))?;
        let first = r.first_line(i);
        let rows: Vec<Vec<String>> = text.lines().map(|l| l.split_whitespace().map(str::to_string).collect()).collect();
        Ok(Some((rows, first, i)))
    };
    let (rows, first, hitem) = grid("heights")?.ok_or_else(|| anyhow!("line {line}: [terrain] has no heights = \"\"\" ... \"\"\" (a row of heights a line, the far row first)"))?;
    let at_line = |k: usize| if first > 0 { format!("line {}: ", first + k) } else { String::new() };
    // The rows with their lines; blank lines are no rows.
    let filled: Vec<(usize, &Vec<String>)> = rows.iter().enumerate().filter(|(_, r)| !r.is_empty()).collect();
    if filled.is_empty() {
        return Err(r.fail(hitem.span(), "heights has no rows"));
    }
    let cols = filled[0].1.len();
    let nrows = filled.len();
    let mut heights = vec![0; cols * nrows];
    for (n, (k, row)) in filled.iter().enumerate() {
        if row.len() != cols {
            bail!("{}heights: this row has {} points, the first has {cols}", at_line(*k), row.len());
        }
        // The far row first: the file's first row is the last in y.
        let y = nrows - 1 - n;
        for (x, word) in row.iter().enumerate() {
            let v: i32 = word.parse().map_err(|_| anyhow!("{}heights: '{word}' is no height (whole voxels, 0 or more)", at_line(*k)))?;
            if !(0..=HIGHEST).contains(&v) {
                bail!("{}heights: {v} is out of range (0 to {HIGHEST})", at_line(*k));
            }
            heights[x + y * cols] = v;
        }
    }
    // A grid of one sign a point (shapes, roughness), laid out as the
    // heights; `sign` reads one, `what` names the signs in a mistake.
    let signs = |key: &str, what: &str, sign: &dyn Fn(char) -> Option<u8>| -> Result<Option<Vec<u8>>> {
        let Some((srows, sfirst, sitem)) = grid(key)? else { return Ok(None) };
        let sline = |k: usize| if sfirst > 0 { format!("line {}: ", sfirst + k) } else { String::new() };
        let sfilled: Vec<(usize, &Vec<String>)> = srows.iter().enumerate().filter(|(_, r)| !r.is_empty()).collect();
        if sfilled.len() != nrows {
            return Err(r.fail(sitem.span(), format!("{key} has {} rows, heights has {nrows}", sfilled.len())));
        }
        let mut out = vec![0; cols * nrows];
        for (n, (k, row)) in sfilled.iter().enumerate() {
            if row.len() != cols {
                bail!("{}{key}: this row has {} signs, a row of heights has {cols}", sline(*k), row.len());
            }
            let y = nrows - 1 - n;
            for (x, word) in row.iter().enumerate() {
                let mut chars = word.chars();
                let v = match (chars.next(), chars.next()) {
                    (Some(c), None) => sign(c),
                    _ => None,
                };
                out[x + y * cols] = v.ok_or_else(|| anyhow!("{}{key}: '{word}' is no {what}", sline(*k)))?;
            }
        }
        Ok(Some(out))
    };
    let shapes: Vec<Shape> = match signs("shapes", "shape (. SMOOTH, ^ LINEAR, # STEP)", &|c| Shape::of_sign(c).map(|s| s as u8))? {
        None => vec![Shape::Smooth; cols * nrows],
        Some(v) => v.into_iter().map(|k| Shape::ALL.into_iter().find(|s| *s as u8 == k).expect("read")).collect(),
    };
    let roughness = signs("roughness", "roughness (a digit, 0 calm to 9)", &|c| c.to_digit(10).map(|d| d as u8))?;
    let terrace = match t.get("terrace") {
        None => None,
        Some(i) => match i.as_integer() {
            Some(v) if (1..=HIGHEST as i64).contains(&v) => Some(v as i32),
            _ => return Err(r.fail(i.span(), "terrace must be a whole number of voxels, 1 or more")),
        },
    };
    let rough = match t.get("rough") {
        None => None,
        Some(i) => match i.as_float().or_else(|| i.as_integer().map(|v| v as f64)) {
            Some(v) if (0.0..=1.0).contains(&v) => Some(v),
            _ => return Err(r.fail(i.span(), "rough must be between 0 (even) and 1")),
        },
    };
    let seed = match t.get("seed") {
        None => None,
        Some(i) => match i.as_integer() {
            Some(v) if v >= 0 => Some(v as u64),
            _ => return Err(r.fail(i.span(), "seed must be a whole number, 0 or more")),
        },
    };
    let colour = |i: &Item, what: &str| -> Result<String> {
        let text = i.as_str().filter(|s| crate::grid::hex(s).is_some());
        text.map(str::to_string).ok_or_else(|| r.fail(i.span(), format!("{what} must be a color written #rrggbb")))
    };
    let under = t.get("under").map(|i| colour(i, "under")).transpose()?;
    // (Numbers of fractions as the file writes them, not as f32: a
    // grain of 0.35 written back 0.35.)
    let exact = |i: &Item, what: &str| -> Result<f64> { i.as_float().or_else(|| i.as_integer().map(|v| v as f64)).filter(|v| v.is_finite()).ok_or_else(|| r.fail(i.span(), format!("{what} must be a number"))) };
    let mut layers = Vec::new();
    if let Some(item) = t.get("layer") {
        for l in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "layer must be tables: [[terrain.layer]]"))? {
            r.only(l, &["thick", "hard", "color"], "a terrain's layer")?;
            let lline = l.iter().filter_map(|(_, v)| v.span()).map(|s| vvx::line_of(r.src, s.start)).min().unwrap_or(0);
            let thick = match l.get("thick") {
                Some(i) => match i.as_integer() {
                    Some(v) if (1..=HIGHEST as i64).contains(&v) => v as i32,
                    _ => return Err(r.fail(i.span(), "thick must be a whole number of voxels, 1 or more")),
                },
                None => bail!("line {lline}: a terrain's layer has no thick = N (voxels)"),
            };
            let hard = match l.get("hard") {
                Some(i) => {
                    let v = exact(i, "hard")?;
                    if !(0.0..=1.0).contains(&v) {
                        return Err(r.fail(i.span(), "hard must be between 0 (soft: a slope of scree) and 1 (a wall)"));
                    }
                    v
                }
                None => bail!("line {lline}: a terrain's layer has no hard = 0 to 1"),
            };
            let color = colour(l.get("color").ok_or_else(|| anyhow!("line {lline}: a terrain's layer has no color = \"#rrggbb\""))?, "color")?;
            layers.push(Layer { thick, hard, color });
        }
    }
    let mut materials = Vec::new();
    let mitem = t.get("material").ok_or_else(|| anyhow!("line {line}: [terrain] has no [[terrain.material]] (its colors: the first that fits paints, the last for everything else)"))?;
    let tables = vvx::tables(mitem).ok_or_else(|| r.fail(mitem.span(), "material must be tables: [[terrain.material]]"))?;
    for m in tables {
        r.only(m, &["color", "colors", "above", "below", "steeper", "flatter", "depth", "grain", "patch", "blend"], "a terrain's material")?;
        let mline = m.iter().filter_map(|(_, v)| v.span()).map(|s| vvx::line_of(r.src, s.start)).min().unwrap_or(0);
        let (color, tones) = match (m.get("color"), m.get("colors")) {
            (Some(i), None) => (colour(i, "color")?, Vec::new()),
            (None, Some(i)) => {
                let wrong = || r.fail(i.span(), "colors must be two or three colors, [\"#rrggbb\", \"#rrggbb\"]");
                let list = i.as_array().filter(|a| (2..=3).contains(&a.len())).ok_or_else(wrong)?;
                let all: Vec<String> = list.iter().map(|v| v.as_str().filter(|s| crate::grid::hex(s).is_some()).map(str::to_string)).collect::<Option<_>>().ok_or_else(wrong)?;
                (all[0].clone(), all[1..].to_vec())
            }
            (Some(_), Some(i)) => return Err(r.fail(i.span(), "a material has color or colors, not both")),
            (None, None) => bail!("line {mline}: a terrain's material has no color = \"#rrggbb\""),
        };
        let whole = |key: &str| -> Result<Option<i32>> {
            m.get(key)
                .map(|i| {
                    let v = r.number(i, key)?;
                    if v.fract() != 0.0 || v.abs() > HIGHEST as f32 {
                        return Err(r.fail(i.span(), format!("{key} must be a whole number of voxels")));
                    }
                    Ok(v as i32)
                })
                .transpose()
        };
        let degrees = |key: &str| -> Result<Option<f64>> {
            m.get(key)
                .map(|i| {
                    let v = exact(i, key)?;
                    if !(0.0..90.0).contains(&v) {
                        return Err(r.fail(i.span(), format!("{key} must be degrees of slope, 0 to less than 90")));
                    }
                    Ok(v)
                })
                .transpose()
        };
        let depth = whole("depth")?;
        if let (Some(d), Some(i)) = (depth, m.get("depth"))
            && d < 1
        {
            return Err(r.fail(i.span(), "depth must be 1 voxel or more"));
        }
        let grain = match m.get("grain") {
            None => None,
            Some(i) => {
                let v = exact(i, "grain")?;
                if !(0.0..=1.0).contains(&v) {
                    return Err(r.fail(i.span(), format!("grain must be between 0 and 1, not {v}")));
                }
                Some(v)
            }
        };
        let patch = whole("patch")?;
        if let (Some(v), Some(i)) = (patch, m.get("patch"))
            && v < 1
        {
            return Err(r.fail(i.span(), "patch must be 1 voxel or more"));
        }
        let blend = match m.get("blend") {
            None => None,
            Some(i) => {
                let v = exact(i, "blend")?;
                if !(0.0..=1000.0).contains(&v) {
                    return Err(r.fail(i.span(), "blend must be 0 or more (voxels of height, degrees of slope)"));
                }
                Some(v)
            }
        };
        materials.push(Material { color, above: whole("above")?, below: whole("below")?, steeper: degrees("steeper")?, flatter: degrees("flatter")?, depth, grain, tones, patch, blend });
    }
    if materials.len() > MATERIALS {
        return Err(r.fail(mitem.span(), format!("a terrain has {MATERIALS} materials at most")));
    }
    if let Some(last) = materials.last()
        && !last.unconditional()
    {
        return Err(r.fail(mitem.span(), format!("the terrain's last material ({}) has conditions: the last one paints whatever no other does, so it has none", last.color)));
    }
    let mut scatter = Vec::new();
    if let Some(item) = t.get("scatter") {
        for sc in vvx::tables(item).ok_or_else(|| r.fail(item.span(), "scatter must be tables: [[terrain.scatter]]"))? {
            r.only(sc, &["model", "models", "density", "above", "below", "steeper", "flatter", "blend", "apart", "rotate", "sink", "as"], "a terrain's scatter")?;
            let sline = sc.iter().filter_map(|(_, v)| v.span()).map(|s| vvx::line_of(r.src, s.start)).min().unwrap_or(0);
            let models = match (sc.get("model"), sc.get("models")) {
                (Some(i), None) => vec![i.as_str().filter(|s| !s.is_empty()).ok_or_else(|| r.fail(i.span(), "model must be a model's file in quotes"))?.to_string()],
                (None, Some(i)) => {
                    let wrong = || r.fail(i.span(), "models must be models' files in quotes, [\"fir.vvx\", \"fir2.vvx\"]");
                    let list = i.as_array().filter(|a| !a.is_empty()).ok_or_else(wrong)?;
                    list.iter().map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_string)).collect::<Option<Vec<_>>>().ok_or_else(wrong)?
                }
                (Some(_), Some(i)) => return Err(r.fail(i.span(), "a scatter has model or models, not both")),
                (None, None) => bail!("line {sline}: a terrain's scatter has no model = \"NAME.vvx\""),
            };
            let density = match sc.get("density") {
                Some(i) => {
                    let v = exact(i, "density")?;
                    if !(0.0..=10_000.0).contains(&v) {
                        return Err(r.fail(i.span(), "density must be how many on 100 by 100 voxels, 0 or more"));
                    }
                    v
                }
                None => bail!("line {sline}: a terrain's scatter has no density = N (how many on 100 by 100 voxels of ground that fits)"),
            };
            let whole = |key: &str, lo: i32| -> Result<Option<i32>> {
                sc.get(key)
                    .map(|i| match i.as_integer() {
                        Some(v) if v >= lo as i64 && v <= HIGHEST as i64 => Ok(v as i32),
                        _ => Err(r.fail(i.span(), format!("{key} must be a whole number of voxels"))),
                    })
                    .transpose()
            };
            let degrees = |key: &str| -> Result<Option<f64>> {
                sc.get(key)
                    .map(|i| {
                        let v = exact(i, key)?;
                        if !(0.0..90.0).contains(&v) {
                            return Err(r.fail(i.span(), format!("{key} must be degrees of slope, 0 to less than 90")));
                        }
                        Ok(v)
                    })
                    .transpose()
            };
            let rotate = match sc.get("rotate") {
                None => None,
                Some(i) => Some(Rotate::ALL.into_iter().find(|w| Some(w.word()) == i.as_str()).ok_or_else(|| r.fail(i.span(), "rotate must be \"any\", \"quarters\" or \"none\""))?),
            };
            let as_ = match sc.get("as") {
                None => None,
                Some(i) => Some([ScatterAs::Pieces, ScatterAs::Voxels].into_iter().find(|w| Some(w.word()) == i.as_str()).ok_or_else(|| r.fail(i.span(), "as must be \"pieces\" or \"voxels\""))?),
            };
            let blend = sc.get("blend").map(|i| exact(i, "blend")).transpose()?;
            scatter.push(Scatter { models, density, above: whole("above", -HIGHEST)?, below: whole("below", -HIGHEST)?, steeper: degrees("steeper")?, flatter: degrees("flatter")?, blend, apart: whole("apart", 0)?, rotate, sink: whole("sink", 0)?, as_ });
        }
    }
    let ranged = |key: &str, lo: f64, hi: f64, why: &str| -> Result<Option<f64>> {
        t.get(key)
            .map(|i| {
                let v = exact(i, key)?;
                if !(lo..=hi).contains(&v) {
                    return Err(r.fail(i.span(), format!("{key} must be {why}")));
                }
                Ok(v)
            })
            .transpose()
    };
    let dip = ranged("dip", 0.0, 45.0, "degrees the bands tilt, 0 (level) to 45")?;
    let dip_to = ranged("dip_to", -360.0, 720.0, "degrees clockwise from north, the way the bands tilt down")?;
    let fold = ranged("fold", 0.0, 1.0, "between 0 (flat bands) and 1")?;
    let fault = ranged("fault", 0.0, 1.0, "between 0 (none) and 1")?;
    Ok(Terrain { spacing, at, cols: cols as i32, rows: nrows as i32, heights, shapes, terrace, rough, roughness, layers, scatter, dip, dip_to, fold, fault, seed, under, materials, line })
}

/// A grid of the points as the file writes it: the far row first, each
/// word right-aligned to the widest height, so heights and shapes line up.
pub fn grid_text(t: &Terrain, word: impl Fn(usize) -> String) -> String {
    let width = t.heights.iter().map(|h| h.to_string().len()).max().unwrap_or(1);
    let mut out = String::from("\"\"\"\n");
    for y in (0..t.rows).rev() {
        let row: Vec<String> = (0..t.cols).map(|x| format!("{:>width$}", word((x + y * t.cols) as usize))).collect();
        out += &row.join(" ");
        out.push('\n');
    }
    out += "\"\"\"";
    out
}

pub fn heights_text(t: &Terrain) -> String {
    grid_text(t, |i| t.heights[i].to_string())
}

/// The shapes, None where every point is SMOOTH.
pub fn shapes_text(t: &Terrain) -> Option<String> {
    t.shapes.iter().any(|s| *s != Shape::Smooth).then(|| grid_text(t, |i| t.shapes[i].sign().to_string()))
}

pub(super) fn number(v: f64) -> String {
    format!("{v}")
}

/// The roughness, None where the file has none.
pub fn roughness_text(t: &Terrain) -> Option<String> {
    t.roughness.as_ref().map(|r| grid_text(t, |i| r[i].to_string()))
}

/// The fields of a layer as its table writes them, in order.
pub fn layer_fields(l: &Layer) -> Vec<(&'static str, String)> {
    vec![("thick", l.thick.to_string()), ("hard", number(l.hard)), ("color", vvx::quoted(&l.color))]
}

pub fn layer_text(l: &Layer) -> String {
    let mut out = "[[terrain.layer]]     # from the bottom up, again and again upward\n".to_string();
    for (k, v) in layer_fields(l) {
        out += &format!("{k} = {v}\n");
    }
    out
}

/// The fields of a material as its table writes them, in order.
pub fn material_fields(m: &Material) -> Vec<(&'static str, String)> {
    let mut out = if m.tones.is_empty() {
        vec![("color", vvx::quoted(&m.color))]
    } else {
        let all: Vec<String> = std::iter::once(&m.color).chain(&m.tones).map(|c| vvx::quoted(c)).collect();
        vec![("colors", format!("[{}]", all.join(", ")))]
    };
    out.extend(m.above.map(|v| ("above", v.to_string())));
    out.extend(m.below.map(|v| ("below", v.to_string())));
    out.extend(m.steeper.map(|v| ("steeper", number(v))));
    out.extend(m.flatter.map(|v| ("flatter", number(v))));
    out.extend(m.depth.map(|v| ("depth", v.to_string())));
    out.extend(m.grain.map(|v| ("grain", number(v))));
    out.extend(m.patch.map(|v| ("patch", v.to_string())));
    out.extend(m.blend.map(|v| ("blend", number(v))));
    out
}

pub fn material_text(m: &Material) -> String {
    let mut out = "[[terrain.material]]  # the first that fits paints\n".to_string();
    for (k, v) in material_fields(m) {
        out += &format!("{k} = {v}\n");
    }
    out
}

/// The fields of `[terrain]` itself, in order.
pub fn fields(t: &Terrain) -> Vec<(&'static str, String)> {
    let mut out = vec![("spacing", t.spacing.to_string())];
    if t.at != [0; 3] {
        out.push(("at", format!("[{}, {}, {}]", t.at[0], t.at[1], t.at[2])));
    }
    out.push(("heights", heights_text(t)));
    out.extend(shapes_text(t).map(|s| ("shapes", s)));
    out.extend(t.terrace.map(|v| ("terrace", v.to_string())));
    out.extend(t.rough.map(|v| ("rough", number(v))));
    out.extend(roughness_text(t).map(|r| ("roughness", r)));
    out.extend(t.dip.map(|v| ("dip", number(v))));
    out.extend(t.dip_to.map(|v| ("dip_to", number(v))));
    out.extend(t.fold.map(|v| ("fold", number(v))));
    out.extend(t.fault.map(|v| ("fault", number(v))));
    out.extend(t.seed.map(|s| ("seed", s.to_string())));
    out.extend(t.under.as_ref().map(|u| ("under", vvx::quoted(u))));
    out
}

/// The whole of `[terrain]` and its materials.
/// The fields of a scatter rule as its table writes them, in order.
pub fn scatter_fields(s: &Scatter) -> Vec<(&'static str, String)> {
    let mut out = if s.models.len() == 1 {
        vec![("model", vvx::quoted(&s.models[0]))]
    } else {
        vec![("models", format!("[{}]", s.models.iter().map(|m| vvx::quoted(m)).collect::<Vec<_>>().join(", ")))]
    };
    out.push(("density", number(s.density)));
    out.extend(s.above.map(|v| ("above", v.to_string())));
    out.extend(s.below.map(|v| ("below", v.to_string())));
    out.extend(s.steeper.map(|v| ("steeper", number(v))));
    out.extend(s.flatter.map(|v| ("flatter", number(v))));
    out.extend(s.blend.map(|v| ("blend", number(v))));
    out.extend(s.apart.map(|v| ("apart", v.to_string())));
    out.extend(s.rotate.map(|v| ("rotate", vvx::quoted(v.word()))));
    out.extend(s.sink.map(|v| ("sink", v.to_string())));
    out.extend(s.as_.map(|v| ("as", vvx::quoted(v.word()))));
    out
}

pub fn scatter_text(s: &Scatter) -> String {
    let mut out = "[[terrain.scatter]]   # models over the land: as pieces (the editor's SCATTER) or voxels\n".to_string();
    for (k, v) in scatter_fields(s) {
        out += &format!("{k} = {v}\n");
    }
    out
}

pub fn text(t: &Terrain) -> String {
    let mut out = "[terrain]             # heights in voxels, the far row first; shapes . SMOOTH ^ LINEAR # STEP\n".to_string();
    for (k, v) in fields(t) {
        out += &format!("{k} = {v}\n");
    }
    for l in &t.layers {
        out += &format!("\n{}", layer_text(l));
    }
    for m in &t.materials {
        out += &format!("\n{}", material_text(m));
    }
    for s in &t.scatter {
        out += &format!("\n{}", scatter_text(s));
    }
    out
}

/// A terrain's mistakes against its level: it lies inside the level's
/// voxels `room`.
pub fn check_room(t: &Terrain, room: IVec3) -> Result<()> {
    // In whole numbers that cannot overflow: a row of a million points
    // times the spacing is past any level, not a wrapped number inside it.
    let size = glam::I64Vec2::new(t.cols as i64, t.rows as i64) * t.spacing as i64;
    let high = t.heights.iter().copied().max().unwrap_or(0) as i64;
    let far = glam::I64Vec3::new(t.at[0] as i64 + size.x, t.at[1] as i64 + size.y, t.at[2] as i64 + high);
    if far.cmpgt(room.as_i64vec3()).any() {
        bail!(
            "line {}: the terrain reaches to {} by {} by {} voxels (at, the points times spacing, the highest point), past the level's size {} by {} by {}",
            t.line,
            far.x,
            far.y,
            far.z,
            room.x,
            room.y,
            room.z
        );
    }
    Ok(())
}
