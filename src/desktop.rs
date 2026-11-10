//! Voxcade's icon: on its windows, and on Linux in the desktop's menus
//! and task bar.
//!
//! A Wayland desktop (KDE, GNOME) takes a window's icon not from the
//! window but from the desktop entry whose name is the window's app id.
//! So the program names itself `voxcade` (`name_app`), and the launcher
//! keeps an entry and the icon in the user's own data folder
//! (`~/.local/share/applications/voxcade.desktop`,
//! `~/.local/share/icons/hicolor/256x256/apps/voxcade.png`): Voxcade then
//! shows in the applications menu too, started from where it lies. Both
//! are written only when missing or when the program has moved.

use std::path::PathBuf;

/// The icon, 256 by 256 (`tools/make_icon.py`).
const ICON: &[u8] = include_bytes!("icon.png");

/// The app id windows carry (the desktop entry's name): before SDL starts.
pub fn name_app() {
    sdl3::hint::set("SDL_APP_ID", "voxcade");
}

/// Sets the icon on a window (X11 and Windows show it; Wayland takes the
/// desktop entry's).
pub fn set_icon(window: &mut sdl3::video::Window) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(ICON));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let Ok(mut reader) = decoder.read_info() else { return };
    let Some(size) = reader.output_buffer_size() else { return };
    let mut pixels = vec![0; size];
    let Ok(info) = reader.next_frame(&mut pixels) else { return };
    if info.color_type != png::ColorType::Rgba {
        return;
    }
    if let Ok(surface) = sdl3::surface::Surface::from_data(&mut pixels, info.width, info.height, info.width * 4, sdl3::pixels::PixelFormat::RGBA32) {
        window.set_icon(surface);
    }
}

/// The user's data folder (`$XDG_DATA_HOME`, else `~/.local/share`).
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

/// The desktop entry for the program at `exe`.
fn entry(exe: &std::path::Path) -> String {
    // A path in Exec is quoted, with \ " ` $ escaped.
    let quoted: String = exe.display().to_string().chars().flat_map(|c| if matches!(c, '\\' | '"' | '`' | '$') { vec!['\\', c] } else { vec![c] }).collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=Voxcade\nGenericName=Voxel arcade games\nComment=Play voxel arcade games, or make your own\nComment[de]=Voxel-Arcade-Spiele spielen oder eigene machen\nExec=\"{quoted}\"\nIcon=voxcade\nTerminal=false\nCategories=Game;Development;\nStartupWMClass=voxcade\n"
    )
}

/// On Linux: keeps the desktop entry and the icon in the user's data
/// folder, so the desktop shows Voxcade's icon on its windows and lists
/// it among the applications. Quietly: no desktop, no loss.
pub fn keep_entry() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let (Some(data), Ok(exe)) = (data_dir(), std::env::current_exe()) else { return };
    let write = |path: PathBuf, bytes: &[u8]| {
        if std::fs::read(&path).is_ok_and(|there| there == bytes) {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, bytes);
    };
    write(data.join("icons/hicolor/256x256/apps/voxcade.png"), ICON);
    write(data.join("applications/voxcade.desktop"), entry(&exe).as_bytes());
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_desktop_entry_starts_the_program_where_it_lies() {
        let entry = super::entry(std::path::Path::new("/home/someone/My \"Games\"/voxcade"));
        assert!(entry.contains("Exec=\"/home/someone/My \\\"Games\\\"/voxcade\"\n"), "{entry}");
        assert!(entry.contains("Icon=voxcade\n") && entry.contains("StartupWMClass=voxcade\n"));
    }

    #[test]
    fn the_icon_is_a_picture_with_alpha() {
        let reader = png::Decoder::new(std::io::Cursor::new(super::ICON)).read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height, reader.info().color_type), (256, 256, png::ColorType::Rgba));
    }
}
