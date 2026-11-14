use super::*;

pub fn plan(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let folder = dir.join(crate::audio::sound::FOLDER);
    for ending in crate::audio::sound::RECORDINGS {
        for p in files_ending(&folder, ending).into_iter().filter(|p| p.parent().is_none_or(|f| f.as_os_str().is_empty())) {
            let name = p.file_stem().map_or(String::new(), |n| n.to_string_lossy().into_owned());
            out.push(format!("sounds/{} is a recording, which Voxcade no longer plays: make the sound \"{name}\" a sounds/{name}.toml of numbers (the IDE's sound maker rolls one to start from), then take the recording out; it is left as it is", named(&p)));
        }
    }
    for p in files_ending(&folder, "toml") {
        let text = std::fs::read_to_string(folder.join(&p)).unwrap_or_default();
        if let Some(n) = text.lines().position(|l| l.trim_start().starts_with("file")) {
            out.push(format!("sounds/{}:{} names a recording (file = ...), which Voxcade no longer plays: write the sound in numbers instead (wave, freq, decay and the rest), and take the recording out; it is left as it is", named(&p), n + 1));
        }
    }
    Ok(out)
}
