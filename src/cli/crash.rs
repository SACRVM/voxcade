//! A crash said, not silent: the panic hook of every command that opens a
//! window (`Call::opens_a_window`: a double click, the launcher, the IDE,
//! an editor, a game; `cli::main` installs it for those alone). What went
//! wrong (the message, where, a backtrace) is written to a report in the
//! config folder (`crashes/`, the newest `KEPT`), on the terminal as
//! before, and said in a window with where the report lies, in plain
//! words. A command without a window (`shot`, `check`, `inspect` …) has
//! Rust's own hook: the panic and its backtrace on the terminal, no
//! report, never a window (a script or an agent would hang on it).

use std::path::{Path, PathBuf};

/// How many reports the folder keeps, the newest.
pub const KEPT: usize = 10;

/// The folder the reports go to.
pub fn folder() -> PathBuf {
    crate::settings::config_dir().join("crashes")
}

/// Puts the hook in place: `window`, the program was started for a window.
pub fn install(window: bool) {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // The terminal's lines as ever.
        before(info);
        let main = std::thread::current().name() == Some("main");
        let report = report(info, &std::backtrace::Backtrace::force_capture().to_string());
        let written = write(&folder(), &report);
        match &written {
            Ok(path) => eprintln!("the crash report lies in {}", path.display()),
            Err(e) => eprintln!("the crash report could not be written: {e:#}"),
        }
        if says_in_a_window(window, main) {
            let message = plainly(written.as_deref().ok(), &crate::settings::system_languages());
            let _ = sdl3::messagebox::show_simple_message_box(sdl3::messagebox::MessageBoxFlag::ERROR, "Voxcade", &message, None);
        }
    }));
}

/// Whether a crash is said in a message window: only for a program that
/// opened one, and only for a crash of the main thread, the one whose end
/// ends the program (a worker's crash reaches it when it is waited for).
pub fn says_in_a_window(opened_a_window: bool, main_thread: bool) -> bool {
    opened_a_window && main_thread
}

/// The report's text: the version, the command line, the thread, the
/// message, where and the backtrace.
fn report(info: &std::panic::PanicHookInfo, backtrace: &str) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".into());
    let at = info.location().map_or_else(|| "(unknown)".into(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let args: Vec<String> = std::env::args().collect();
    let thread = std::thread::current().name().unwrap_or("(unnamed)").to_string();
    report_text(&message, &at, &thread, &args.join(" "), backtrace)
}

/// The report from its parts.
pub fn report_text(message: &str, at: &str, thread: &str, command: &str, backtrace: &str) -> String {
    format!(
        "voxcade {} crashed ({} {})\ncommand: {command}\nthread: {thread}\nwhere: {at}\nmessage: {message}\n\nbacktrace:\n{backtrace}\n",
        crate::update::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Writes `report` into `dir` as `crash-<time>.txt` (a second's second
/// crash gets `-2` and so on) and keeps the newest `KEPT` there; the file
/// written.
pub fn write(dir: &Path, report: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = crate::replay::stamp();
    let mut path = dir.join(format!("crash-{stamp}.txt"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("crash-{stamp}-{n}.txt"));
        n += 1;
    }
    std::fs::write(&path, report)?;
    prune(dir);
    Ok(path)
}

/// Removes all but the newest `KEPT` reports (by name: they sort by time).
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut reports: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt")))
        .collect();
    // `crash-T.txt` before `crash-T-2.txt`: the name without its ending.
    reports.sort_by_key(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
    let over = reports.len().saturating_sub(KEPT);
    for old in &reports[..over] {
        let _ = std::fs::remove_file(old);
    }
}

/// The message window's words in the system's language (English, German
/// or French), with where the report lies.
pub fn plainly(report: Option<&Path>, languages: &[String]) -> String {
    let lang = languages.iter().map(String::as_str).find(|l| matches!(*l, "en" | "de" | "fr")).unwrap_or("en");
    let said = match lang {
        "de" => "Voxcade ist an einem Fehler im Programm gescheitert und musste aufhören.",
        "fr" => "Voxcade s'est arrêté à cause d'une erreur dans le programme.",
        _ => "Voxcade stopped because of a mistake in the program.",
    };
    let where_ = match (report, lang) {
        (Some(p), "de") => format!("Ein Bericht darüber liegt in:\n{}\n\nSchick ihn uns gern (voxca.de), damit wir den Fehler beheben können.", p.display()),
        (Some(p), "fr") => format!("Un rapport se trouve dans :\n{}\n\nEnvoyez-le-nous (voxca.de) pour que nous puissions corriger l'erreur.", p.display()),
        (Some(p), _) => format!("A report on it lies in:\n{}\n\nSend it to us (voxca.de) so we can mend the mistake.", p.display()),
        (None, "de") => "Ein Bericht konnte nicht geschrieben werden.".into(),
        (None, "fr") => "Aucun rapport n'a pu être écrit.".into(),
        (None, _) => "No report could be written.".into(),
    };
    format!("{said}\n\n{where_}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A report is written with all it says, a second one beside it; only
    /// the newest `KEPT` stay.
    #[test]
    fn a_crash_report_is_written_and_the_newest_kept() {
        let dir = crate::scratch::Scratch::new("crashes");
        let text = report_text("index out of bounds", "src/x.rs:1:2", "main", "voxcade run games/x", "0: main");
        let first = write(&dir, &text).unwrap();
        let read = std::fs::read_to_string(&first).unwrap();
        for part in ["voxcade ", "command: voxcade run games/x", "thread: main", "where: src/x.rs:1:2", "message: index out of bounds", "backtrace:\n0: main"] {
            assert!(read.contains(part), "{part}: {read}");
        }
        let second = write(&dir, &text).unwrap();
        assert_ne!(first, second, "a second crash keeps the first report");
        // Older ones, as if from earlier days.
        for day in 10..30 {
            std::fs::write(dir.join(format!("crash-202601{day}-120000.txt")), "old").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "mine").unwrap();
        write(&dir, &text).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&*dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left.len(), KEPT + 1, "{left:?}");
        assert!(left.contains(&"notes.txt".to_string()), "nothing but reports is taken");
        assert!(!left.contains(&"crash-20260112-120000.txt".to_string()) && left.contains(&"crash-20260129-120000.txt".to_string()), "the oldest went: {left:?}");
        assert!(left.contains(&first.file_name().unwrap().to_string_lossy().into_owned()) && left.contains(&second.file_name().unwrap().to_string_lossy().into_owned()));
    }

    /// Only a program that opened a window says it in one, and only for a
    /// crash of its main thread; the words name the report's place in the
    /// system's language.
    #[test]
    fn a_crash_is_said_in_a_window_only_for_a_window() {
        assert!(says_in_a_window(true, true));
        assert!(!says_in_a_window(true, false));
        assert!(!says_in_a_window(false, true));
        assert!(!says_in_a_window(false, false));
        let at = Path::new("/home/x/.config/voxcade/crashes/crash-1.txt");
        assert!(plainly(Some(at), &["de".into()]).contains("Bericht") && plainly(Some(at), &["de".into()]).contains("crash-1.txt"));
        assert!(plainly(Some(at), &["ja".into(), "fr".into()]).starts_with("Voxcade s'est arrêté"));
        assert!(plainly(None, &[]).ends_with("No report could be written."));
    }
}
