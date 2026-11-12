//! A folder for a test to work in, which goes away with it.
//!
//! The tests used to make folders in the system's temp folder and leave
//! them: every run added a few hundred, and where the temp folder lives in
//! RAM (a tmpfs) two days of runs filled it and stopped the machine. A
//! `Scratch` is made fresh, named `voxcade-NAME-PID-N` (the process id and a
//! counter, as tests run in threads of one process, so the leftovers of a
//! crash can be told and cleared) and removes itself, everything in it, when
//! it is dropped, a panicking test included. It stands in for the path:
//! `&scratch`, `scratch.join("x")` and `scratch.path()` all work. Keep it in
//! a binding that lives as long as the test does.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNT: AtomicUsize = AtomicUsize::new(0);

/// A fresh folder under the temp folder, removed when dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    /// Makes the folder `voxcade-NAME-PID-N`, empty.
    pub fn new(name: &str) -> Scratch {
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("voxcade-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch folder");
        Scratch(path)
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_folder_goes_when_dropped() {
        let path;
        {
            let s = Scratch::new("scratch-test");
            path = s.to_path_buf();
            std::fs::write(s.join("a.txt"), "x").unwrap();
            assert!(path.join("a.txt").exists());
            assert_ne!(Scratch::new("scratch-test").path(), s.path());
        }
        assert!(!path.exists());
    }
}
