//! New versions: the program looks at its feed (the website's
//! `release.toml`: the version on offer, each system's package and its
//! checksum), and when the user says so fetches the package, checks it and
//! puts it in place of the one running. Nothing is fetched but the small
//! feed until the user asks, and looking can be switched off (the
//! launcher's line; `ide/prefs.rs`).
//!
//! What it trusts: the feed over HTTPS from our site, and the package only
//! if its SHA-256 is the feed's. In place: the folder the program lies in,
//! when it is a package's (the games beside it). What was there before
//! goes to `before-update/<version>/` in that folder, the version it was:
//! the old program, and each game the package brings anew, so a change
//! someone made to one (the launcher's EDIT opens a game beside the
//! program in place) is not lost. Every generation is kept, never one
//! thrown away by the next update (`kept` says how many and their size,
//! `voxcade update --install` too); deleting them is the user's. Games of
//! the user's own in `games/` stay where they are.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// This program's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the feed lies; `VOXCADE_UPDATE_FEED` names another (a file's
/// path too, for trying it out).
const FEED: &str = "https://voxca.de/release.toml";

/// Where the old files go in the program's folder.
const BACKUP: &str = "before-update";
/// Where the new ones are unpacked first.
const STAGING: &str = ".update";

/// A version on offer for this system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// The package: its address, its file's name, its SHA-256 in hex.
    pub url: String,
    pub file: String,
    pub sha256: String,
    /// The page that says what is new.
    pub notes: String,
}

#[derive(Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    post: String,
    windows: Option<Package>,
    linux: Option<Package>,
}

#[derive(Deserialize)]
struct Package {
    url: String,
    file: String,
    #[serde(default)]
    sha256: String,
}

/// The feed's release for this system (None: none for it).
pub fn parse(feed: &str) -> Result<Option<Release>> {
    let feed: Feed = toml::from_str(feed).context("the feed")?;
    let package = if cfg!(windows) {
        feed.windows
    } else if cfg!(target_os = "linux") {
        feed.linux
    } else {
        None
    };
    Ok(package.map(|p| Release {
        notes: format!("https://voxca.de/blog/{}.html", if feed.post.is_empty() { &feed.version } else { &feed.post }),
        version: feed.version,
        url: p.url,
        file: p.file,
        sha256: p.sha256.to_lowercase(),
    }))
}

/// Whether version `a` is newer than `b` ("0.4.10" is newer than "0.4.9").
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a) > parts(b)
}

/// The folder the program lies in, if it is a package's: the games beside
/// the program, and not a build in a source tree. Only there a new
/// version can be put in place.
pub fn install_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let package = dir.join("games").is_dir() && !dir.join("Cargo.toml").exists() && !dir.components().any(|c| c.as_os_str() == "target");
    package.then(|| dir.to_path_buf())
}

/// Where the games that come with the program lie: beside it, or (a build
/// in its source tree) in the folder it was started in.
pub fn games_dir() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("games")));
    beside.filter(|d| d.is_dir()).or_else(|| Some(PathBuf::from("games")).filter(|d| d.is_dir()))
}

fn feed_address() -> String {
    std::env::var("VOXCADE_UPDATE_FEED").unwrap_or_else(|_| FEED.to_string())
}

/// What lies at `address` (https, or a file's path), `progress` told the
/// share fetched.
fn fetch(address: &str, limit: usize, progress: &dyn Fn(f32)) -> Result<Vec<u8>> {
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return std::fs::read(address).with_context(|| format!("reading {address}"));
    }
    // iroh's reqwest comes without a TLS provider chosen: ring, once.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().user_agent(concat!("voxcade/", env!("CARGO_PKG_VERSION"))).timeout(std::time::Duration::from_secs(600)).build()?;
        let mut response = client.get(address).send().await?.error_for_status()?;
        let total = response.content_length().unwrap_or(0) as usize;
        ensure!(total <= limit, "{address} is bigger than expected");
        let mut bytes = Vec::with_capacity(total);
        while let Some(chunk) = response.chunk().await? {
            bytes.extend_from_slice(&chunk);
            ensure!(bytes.len() <= limit, "{address} is bigger than expected");
            if total > 0 {
                progress(bytes.len() as f32 / total as f32);
            }
        }
        Ok(bytes)
    })
}

/// Looks at the feed: a version newer than this program's, if there is
/// one for this system.
pub fn look() -> Result<Option<Release>> {
    let feed = fetch(&feed_address(), 1 << 20, &|_| {}).context("looking for a new version")?;
    let release = parse(std::str::from_utf8(&feed).context("the feed")?)?;
    Ok(release.filter(|r| newer(&r.version, VERSION)))
}

/// Fetches `release`'s package, checks it and puts it in place in `dir`
/// (`install_dir`); `progress` hears the share fetched. The program
/// running goes on as it is until `restart`.
pub fn install(release: &Release, dir: &Path, progress: &dyn Fn(f32)) -> Result<()> {
    ensure!(!release.sha256.is_empty(), "the feed names no checksum for {}", release.file);
    let package = fetch(&release.url, 1 << 30, progress).with_context(|| format!("fetching {}", release.file))?;
    let sum = hex(&Sha256::digest(&package));
    ensure!(sum == release.sha256, "{} is not the file the feed names (its checksum differs): nothing was changed", release.file);
    let staging = dir.join(STAGING);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).with_context(|| format!("{} cannot be written to", dir.display()))?;
    let done = unpack(&package, &release.file, &staging).and_then(|()| swap(&package_root(&staging)?, dir, VERSION));
    let _ = std::fs::remove_dir_all(&staging);
    done.map(|_| ())
}

/// The generations of what updates replaced in `dir` (`before-update/`'s
/// folders, an older updater's files there counted as one) and the bytes
/// they hold together.
pub fn kept(dir: &Path) -> (usize, u64) {
    let backup = dir.join(BACKUP);
    let Ok(entries) = std::fs::read_dir(&backup) else { return (0, 0) };
    let mut generations = 0;
    let mut loose = false;
    for entry in entries.flatten() {
        if entry.path().is_dir() && entry.file_name() != "games" {
            generations += 1;
        } else {
            loose = true;
        }
    }
    (generations + usize::from(loose), size_of(&backup))
}

/// The bytes of the files under `path`.
fn size_of(path: &Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path).map_or(0, |entries| entries.flatten().map(|e| size_of(&e.path())).sum()),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Starts the program in `dir` anew and ends this one.
pub fn restart(dir: &Path) -> ! {
    let program = dir.join(if cfg!(windows) { "voxcade.exe" } else { "voxcade" });
    let _ = std::process::Command::new(program).current_dir(dir).spawn();
    std::process::exit(0)
}

/// Shows a web page in the user's browser.
pub fn show(address: &str) {
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", address]);
        c
    } else {
        let mut c = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        c.arg(address);
        c
    };
    let _ = command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Unpacks a package (a zip, or a tar.gz) into `to`.
fn unpack(package: &[u8], file: &str, to: &Path) -> Result<()> {
    if file.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).context("the package's zip")?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(name) = entry.enclosed_name() else { bail!("the package holds a path that leads out of it") };
            let path = to.join(name);
            if entry.is_dir() {
                std::fs::create_dir_all(&path)?;
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            std::fs::write(&path, bytes)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
            }
        }
        Ok(())
    } else if file.ends_with(".tar.gz") {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(package));
        for entry in tar.entries().context("the package's tar")? {
            // `unpack_in` refuses a path that leads out of `to`.
            ensure!(entry?.unpack_in(to)?, "the package holds a path that leads out of it");
        }
        Ok(())
    } else {
        bail!("{file}: neither a zip nor a tar.gz")
    }
}

/// Where the package's files lie in what was unpacked: there, or in the
/// one folder it holds (a Linux package's).
fn package_root(staging: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(staging)?.collect::<std::io::Result<_>>()?;
    let root = match entries.as_slice() {
        [one] if one.path().is_dir() => one.path(),
        _ => staging.to_path_buf(),
    };
    ensure!(root.join("games").is_dir() && (root.join("voxcade").is_file() || root.join("voxcade.exe").is_file()), "the package is not a Voxcade folder: nothing was changed");
    Ok(root)
}

/// Puts everything in `new` in place in `dir`: what it replaces goes to
/// `before-update/<was>/` first (`was` the version being replaced; a
/// second update from the same version to `<was>-2` and so on), and no
/// generation before it is touched. The games folder is gone through game
/// by game, so games the package does not bring stay. If a step fails,
/// what was moved is moved back. The generation's folder.
fn swap(new: &Path, dir: &Path, was: &str) -> Result<PathBuf> {
    let mut backup = dir.join(BACKUP).join(was);
    let mut n = 2;
    while backup.exists() {
        backup = dir.join(BACKUP).join(format!("{was}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(backup.join("games"))?;
    std::fs::create_dir_all(dir.join("games"))?;
    // (from the new, in place, its backup)
    let mut moves = Vec::new();
    for entry in std::fs::read_dir(new)? {
        let name = entry?.file_name();
        if name == "games" {
            for game in std::fs::read_dir(new.join("games"))? {
                let game = Path::new("games").join(game?.file_name());
                moves.push((new.join(&game), dir.join(&game), backup.join(&game)));
            }
        } else {
            moves.push((new.join(&name), dir.join(&name), backup.join(&name)));
        }
    }
    let mut done: Vec<&(PathBuf, PathBuf, PathBuf)> = Vec::new();
    let step = |m: &(PathBuf, PathBuf, PathBuf)| -> Result<()> {
        let (from, place, old) = m;
        if place.exists() {
            std::fs::rename(place, old).with_context(|| format!("moving {} aside", place.display()))?;
        }
        std::fs::rename(from, place).with_context(|| format!("putting {} in place", place.display()))
    };
    for m in &moves {
        if let Err(e) = step(m) {
            // Back as it was: the one that failed, then the ones before it.
            for (_, place, old) in std::iter::once(m).chain(done.iter().rev().copied()) {
                if old.exists() {
                    let _ = if place.is_dir() { std::fs::remove_dir_all(place) } else { std::fs::remove_file(place) };
                    let _ = std::fs::rename(old, place);
                }
            }
            let _ = std::fs::remove_dir_all(&backup);
            return Err(e.context("the new version could not be put in place: everything is as it was"));
        }
        done.push(m);
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> crate::scratch::Scratch {
        crate::scratch::Scratch::new(&format!("update-{name}"))
    }

    fn put(dir: &Path, file: &str, text: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(dir: &Path, file: &str) -> String {
        std::fs::read_to_string(dir.join(file)).unwrap_or_else(|_| format!("no {file}"))
    }

    /// A package as the Builds workflow makes them: a zip with the files
    /// at its top (Windows), a tar.gz with one folder (Linux).
    fn zipped(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn tarred(folder: &str, files: &[(&str, &str)]) -> Vec<u8> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (name, text) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(if *name == "voxcade" { 0o755 } else { 0o644 });
            header.set_cksum();
            tar.append_data(&mut header, format!("{folder}/{name}"), text.as_bytes()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    const NEW: [(&str, &str); 4] = [("voxcade", "program 2"), ("voxcade.exe", "program 2"), ("README.txt", "readme 2"), ("games/pulsar/conf.lua", "pulsar 2")];

    fn installed(name: &str) -> crate::scratch::Scratch {
        let dir = temp(name);
        put(&dir, "voxcade", "program 1");
        put(&dir, "voxcade.exe", "program 1");
        put(&dir, "README.txt", "readme 1");
        put(&dir, "games/pulsar/conf.lua", "pulsar 1, changed by hand");
        put(&dir, "games/mine/conf.lua", "my own game");
        put(&dir, "notes.txt", "my notes");
        dir
    }

    fn release(dir: &Path, file: &str, package: &[u8]) -> Release {
        let path = dir.join(file);
        std::fs::write(&path, package).unwrap();
        Release { version: "9.9.9".into(), url: path.display().to_string(), file: file.into(), sha256: hex(&Sha256::digest(package)), notes: String::new() }
    }

    #[test]
    fn a_newer_version_is_told_by_its_numbers() {
        assert!(newer("0.4.10", "0.4.9"));
        assert!(newer("0.5.0", "0.4.12"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("0.4.1", "0.4.1"));
        assert!(!newer("0.4.0", "0.4.1"));
    }

    #[test]
    fn the_feed_names_this_systems_package() {
        let feed = "version = \"0.5.0\"\ndate = \"2026-12-01\"\npost = \"0.5.0\"\n[windows]\nurl = \"https://x/w.zip\"\nfile = \"w.zip\"\nmb = 19.8\nsha256 = \"AB\"\n[linux]\nurl = \"https://x/l.tar.gz\"\nfile = \"l.tar.gz\"\nmb = 22.9\nsha256 = \"cd\"\n";
        let release = parse(feed).unwrap();
        if cfg!(windows) {
            assert_eq!(release.unwrap().sha256, "ab");
        } else if cfg!(target_os = "linux") {
            let release = release.unwrap();
            assert_eq!((release.file.as_str(), release.version.as_str()), ("l.tar.gz", "0.5.0"));
            assert_eq!(release.notes, "https://voxca.de/blog/0.5.0.html");
        } else {
            assert_eq!(release, None);
        }
    }

    /// The new program and the games it brings are in place, what they
    /// replaced lies in before-update, and what the package does not
    /// bring (a game of the user's, a file of theirs) is untouched.
    #[test]
    fn a_new_version_is_put_in_place_and_the_old_kept() {
        for (file, package) in [("new.zip", zipped(&NEW)), ("new.tar.gz", tarred("voxcade-9.9.9-linux-x86_64", &NEW))] {
            let dir = installed(if file.ends_with("zip") { "zip" } else { "tar" });
            let source = temp("source");
            install(&release(&source, file, &package), &dir, &|_| {}).unwrap();
            assert_eq!(read(&dir, "voxcade"), "program 2");
            assert_eq!(read(&dir, "README.txt"), "readme 2");
            assert_eq!(read(&dir, "games/pulsar/conf.lua"), "pulsar 2");
            assert_eq!(read(&dir, "games/mine/conf.lua"), "my own game");
            assert_eq!(read(&dir, "notes.txt"), "my notes");
            let was = format!("before-update/{VERSION}");
            assert_eq!(read(&dir, &format!("{was}/voxcade")), "program 1");
            assert_eq!(read(&dir, &format!("{was}/games/pulsar/conf.lua")), "pulsar 1, changed by hand");
            assert!(!dir.join(STAGING).exists(), "the staging folder is gone");
            #[cfg(unix)]
            if file.ends_with(".tar.gz") {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(dir.join("voxcade")).unwrap().permissions().mode() & 0o111, 0o111, "the program can be run");
            }
            std::fs::remove_dir_all(&dir).unwrap();
            std::fs::remove_dir_all(&source).unwrap();
        }
    }

    /// A second update keeps the first one's generation: a game changed
    /// by hand before the first update is still there after the second,
    /// and `kept` counts both generations and their bytes (an older
    /// updater's loose files as one more).
    #[test]
    fn every_update_keeps_its_generation() {
        let dir = installed("generations");
        let source = temp("generations-source");
        let first = release(&source, "new.zip", &zipped(&NEW));
        let third = [("voxcade", "program 3"), ("voxcade.exe", "program 3"), ("README.txt", "readme 3"), ("games/pulsar/conf.lua", "pulsar 3")];
        let second = release(&source, "third.zip", &zipped(&third));
        let a = swap_from(&first, &dir, "0.6.0");
        let b = swap_from(&second, &dir, "0.7.0");
        assert_eq!(read(&dir, "games/pulsar/conf.lua"), "pulsar 3");
        assert_eq!(read(&a, "games/pulsar/conf.lua"), "pulsar 1, changed by hand", "the hand's change is still kept");
        assert_eq!(read(&b, "games/pulsar/conf.lua"), "pulsar 2");
        assert!(a.ends_with("before-update/0.6.0") && b.ends_with("before-update/0.7.0"), "{a:?} {b:?}");
        // The same version again: beside it, not over it.
        let again = swap_from(&first, &dir, "0.7.0");
        assert!(again.ends_with("before-update/0.7.0-2"), "{again:?}");
        assert_eq!(read(&b, "games/pulsar/conf.lua"), "pulsar 2");
        put(&dir, "before-update/voxcade", "an older updater's program");
        let (generations, bytes) = kept(&dir);
        assert_eq!(generations, 4);
        let sum: usize = ["program 1", "program 1", "readme 1", "pulsar 1, changed by hand", "program 2", "program 2", "readme 2", "pulsar 2", "program 3", "program 3", "readme 3", "pulsar 3", "an older updater's program"].iter().map(|t| t.len()).sum();
        assert_eq!(bytes, sum as u64);
        assert_eq!(kept(&source), (0, 0));
    }

    /// `install`'s steps with the version being replaced named.
    fn swap_from(release: &Release, dir: &Path, was: &str) -> PathBuf {
        let package = std::fs::read(&release.url).unwrap();
        let staging = dir.join(STAGING);
        std::fs::create_dir_all(&staging).unwrap();
        unpack(&package, &release.file, &staging).unwrap();
        let done = swap(&package_root(&staging).unwrap(), dir, was).unwrap();
        std::fs::remove_dir_all(&staging).unwrap();
        done
    }

    /// A package that is not the one the feed names, or no Voxcade folder
    /// at all, changes nothing.
    #[test]
    fn a_wrong_package_changes_nothing() {
        let dir = installed("wrong");
        let source = temp("wrong-source");
        let mut other = release(&source, "new.zip", &zipped(&NEW));
        other.sha256 = hex(&Sha256::digest(b"another file"));
        let said = install(&other, &dir, &|_| {}).unwrap_err().to_string();
        assert!(said.contains("checksum"), "{said}");
        let stranger = release(&source, "stranger.zip", &zipped(&[("hello.txt", "hi")]));
        let said = format!("{:#}", install(&stranger, &dir, &|_| {}).unwrap_err());
        assert!(said.contains("not a Voxcade folder"), "{said}");
        let unnamed = Release { sha256: String::new(), ..release(&source, "new.zip", &zipped(&NEW)) };
        assert!(install(&unnamed, &dir, &|_| {}).is_err(), "no checksum, no update");
        assert_eq!(read(&dir, "voxcade"), "program 1");
        assert_eq!(read(&dir, "games/pulsar/conf.lua"), "pulsar 1, changed by hand");
        assert!(!dir.join(STAGING).exists());
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&source).unwrap();
    }

    /// A path in the package that leads out of it is refused.
    #[test]
    fn a_package_cannot_write_outside_its_folder() {
        let to = temp("outside");
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("../evil.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"x").unwrap();
        let package = zip.finish().unwrap().into_inner();
        assert!(unpack(&package, "p.zip", &to.join("in")).is_err());
        assert!(!to.join("evil.txt").exists());
        std::fs::remove_dir_all(&to).unwrap();
    }
}
