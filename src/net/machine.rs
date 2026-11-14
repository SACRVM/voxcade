//! What a machine is to the others in a network game: its id kept in the
//! config folder, this program's among several on one computer, its name,
//! its build; and random bits from the system.

use super::MachineId;

/// 64 random bits from the system (never the game's generator: joining
/// must not change the game).
pub fn random() -> u64 {
    // The system's random source (session keys must not be guessable); the
    // old mixing of the clock and the process only if the system has none.
    if let Ok(bits) = getrandom::u64() {
        return bits;
    }
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.write_u32(std::process::id());
    h.finish()
}

/// This program's id among the machines: the machine's (made once, kept
/// in the config folder) mixed with the program's process, so two windows
/// on one computer are two guests (testing with several windows on one
/// machine is common); a program started anew comes as a new guest, the
/// old one's seat freed after 10 s of silence.
pub fn program_id() -> MachineId {
    let mut id = machine_id();
    let mixed = crate::hash::mix(u64::from_le_bytes(id[..8].try_into().expect("8 bytes")) ^ std::process::id() as u64);
    id[..8].copy_from_slice(&mixed.to_le_bytes());
    id
}

/// This machine's id, made once and kept in the config folder.
pub fn machine_id() -> MachineId {
    let path = crate::kept::config_dir().join("machine-id");
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Some(id) = from_hex(text.trim())
    {
        return id;
    }
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&random().to_le_bytes());
    id[8..].copy_from_slice(&random().to_le_bytes());
    let text: String = id.iter().map(|b| format!("{b:02x}")).collect();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, text + "\n");
    id
}

fn from_hex(text: &str) -> Option<MachineId> {
    let mut id = [0u8; 16];
    if text.len() != 32 {
        return None;
    }
    for (n, b) in id.iter_mut().enumerate() {
        *b = u8::from_str_radix(text.get(n * 2..n * 2 + 2)?, 16).ok()?;
    }
    Some(id)
}

/// What people call this machine: its name, cut short.
pub fn machine_name() -> String {
    let name = system_name().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "player".into());
    name.split('.').next().unwrap_or("player").chars().take(16).collect()
}

#[cfg(unix)]
fn system_name() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer is as long as said; the name ends with a zero
    // within it, or is cut at its end.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    ok.then(|| String::from_utf8_lossy(&buf[..end]).into_owned())
}

#[cfg(not(unix))]
fn system_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

/// The commit this program was built from.
pub const COMMIT: &str = env!("VOXCADE_COMMIT");
