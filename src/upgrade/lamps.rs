use super::*;

/// What the step says of a game's lamps: each level with lights and each
/// line of its Lua lighting a picture (`:light(`, `:light{`), and how to
/// keep the old look. Nothing when the game has none, or its `conf.lua`
/// already says what its lamps do (`lamp_shadows`).
pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let conf = std::fs::read_to_string(dir.join("conf.lua")).unwrap_or_default();
    if conf.lines().any(|l| l.contains("lamp_shadows") && !l.trim_start().starts_with("--")) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let levels = dir.join("levels");
    for p in files_ending(&levels, "toml") {
        let text = std::fs::read_to_string(levels.join(&p)).unwrap_or_default();
        let Ok(level) = crate::grid::GridLevel::from_text(&text) else { continue };
        let lights: usize = level.things.iter().map(|t| t.lights.iter().filter(|l| l.shadows.is_none()).count()).sum();
        if lights > 0 {
            let s = if lights == 1 { "" } else { "s" };
            out.push(format!("levels/{}: {lights} light{s}: walls stop them now; shadows = \"none\" on a light keeps it shining through", named(&p)));
        }
    }
    for lua in files_ending(dir, "lua") {
        let text = std::fs::read_to_string(dir.join(&lua)).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            let code = line.split("--").next().unwrap_or("");
            if [":light(", ":light{", ":light {"].iter().any(|c| code.contains(c)) && !code.contains("shadows") {
                out.push(format!("{}:{}: frame:light: walls stop it now; frame:light{{ at =, radius =, color =, shadows = \"none\" }} keeps it shining through", named(&lua), i + 1));
            }
        }
    }
    if !out.is_empty() {
        out.push("lamps are stopped by walls since Voxcade 0.7 (the level's voxels and its pieces): c.light.lamp_shadows = \"none\" in conf.lua keeps the old look for every lamp of the game; nothing here is changed".into());
    }
    Ok(out)
}
