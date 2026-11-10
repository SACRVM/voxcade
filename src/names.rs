//! Names that live as long as the program (`&'static str`): the actions,
//! sounds and kinds a game names in its config, read at run time. Each
//! distinct name is kept once, a few dozen in a game.

use std::collections::BTreeSet;
use std::sync::Mutex;

static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

/// `name` for as long as the program runs.
pub fn intern(name: &str) -> &'static str {
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&kept) = names.get(name) {
        return kept;
    }
    let kept: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(kept);
    kept
}

/// A name read from the config (`sound = "pew"`), kept for the program's
/// life: `.0` is the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name(pub &'static str);

impl<'de> serde::Deserialize<'de> for Name {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Ok(Name(intern(&name)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_name_is_kept_once() {
        let a = super::intern("jump");
        let b = super::intern(&String::from("jump"));
        assert!(std::ptr::eq(a, b));
    }
}
