//! The `voxcade` program: runs, shows, checks and films game folders
//! with the Voxcade library (`cli`): Lua games, with the shooter kit when
//! their `conf.lua` asks for it (`Voxcade`).

fn main() -> std::process::ExitCode {
    match voxcade::cli::main::<voxcade::Voxcade>(None) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:?}");
            // Started for a window (a double click, the launcher): nobody
            // may be reading a terminal, so a window says it.
            voxcade::cli::tell_in_a_window(&e);
            std::process::ExitCode::FAILURE
        }
    }
}
