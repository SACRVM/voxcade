//! Groups in the level editor's outliner (`group = "forest"` on a thing,
//! nothing to the game): the things of a group listed under its row, which
//! folds, hides and locks them all; GROUP sets a thing's, RENAME and
//! UNGROUP change every thing of one in one step. Folds and locks are the
//! editor's while it is open, nothing written; a locked thing is passed
//! through by a click in the view (`view::hit_along`), its row still
//! chooses it.

use super::Editor;
use super::room::Item;

/// A group's name as typed, trimmed (None: empty, no group); refused,
/// with why, where the file could not hold it.
pub fn group_name(typed: &str) -> Result<Option<String>, String> {
    let name = typed.trim();
    if name.contains(['"', '\\']) || name.chars().any(char::is_control) {
        return Err("A group's name takes no quotes or backslashes.".into());
    }
    Ok((!name.is_empty()).then(|| name.to_string()))
}

impl Editor {
    /// The group thing `i` is listed under, if any.
    pub fn group_of(&self, i: usize) -> Option<&str> {
        self.level.things.get(i)?.group.as_deref()
    }

    /// The level's groups, in the order their first things lie in the
    /// file (the outliner's, which lists the things in that order).
    pub fn groups(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for g in self.level.things.iter().filter_map(|t| t.group.as_ref()) {
            if !out.contains(g) {
                out.push(g.clone());
            }
        }
        out
    }

    /// The group by its place among `groups` (an action's).
    pub fn group_at(&self, g: usize) -> Option<String> {
        self.groups().into_iter().nth(g)
    }

    /// The things of a group, in the file's order.
    pub fn group_items(&self, group: &str) -> Vec<Item> {
        (0..self.level.things.len()).filter(|&i| self.group_of(i) == Some(group)).map(Item::Thing).collect()
    }

    /// GROUP: the chosen thing listed under the group typed (one there is,
    /// or a new one; empty: none), one step writing its one line.
    pub fn set_group(&mut self, typed: &str) -> Result<(), String> {
        let Some(Item::Thing(i)) = self.pieces.chosen else { return Ok(()) };
        let group = group_name(typed)?;
        self.change(None, |l| l.things[i].group = group);
        Ok(())
    }

    /// RENAME of a group: every thing of it under the name typed, one step
    /// (a name another group has joins them); refused when empty.
    pub fn rename_group(&mut self, old: &str, typed: &str) -> Result<(), String> {
        let Some(new) = group_name(typed)? else { return Err("A group's name needs a letter at least.".into()) };
        self.regroup(old, Some(new));
        Ok(())
    }

    /// UNGROUP: no thing of the group in one any more, one step.
    pub fn ungroup(&mut self, group: &str) {
        self.regroup(group, None);
    }

    fn regroup(&mut self, old: &str, new: Option<String>) {
        if self.pieces.folded.remove(old)
            && let Some(n) = &new
        {
            self.pieces.folded.insert(n.clone());
        }
        self.change(None, |l| l.things.iter_mut().filter(|t| t.group.as_deref() == Some(old)).for_each(|t| t.group = new.clone()));
    }

    /// Whether every thing of a group is hidden (its row's mark).
    pub fn group_hidden(&self, group: &str) -> bool {
        let items = self.group_items(group);
        !items.is_empty() && items.iter().all(|&i| self.is_hidden(i))
    }

    /// A group's hide mark: all its things hidden, or (all hidden) shown.
    pub fn toggle_group_hidden(&mut self, group: &str) {
        let hide = !self.group_hidden(group);
        for item in self.group_items(group) {
            self.set_hidden(item, hide);
        }
    }

    /// Whether `item` is locked: a click in the view passes through it.
    pub fn is_locked(&self, item: Item) -> bool {
        self.pieces.locked.hides_in(&self.level, item)
    }

    /// LOCK / UNLOCK an item (its mark in the outliner).
    pub fn toggle_locked(&mut self, item: Item) {
        let on = !self.is_locked(item);
        self.pieces.locked.set_in(&self.level, item, on);
    }

    /// Whether every thing of a group is locked (its row's mark).
    pub fn group_locked(&self, group: &str) -> bool {
        let items = self.group_items(group);
        !items.is_empty() && items.iter().all(|&i| self.is_locked(i))
    }

    /// A group's lock mark: all its things locked, or (all locked) free.
    pub fn toggle_group_locked(&mut self, group: &str) {
        let on = !self.group_locked(group);
        for item in self.group_items(group) {
            self.pieces.locked.set_in(&self.level, item, on);
        }
    }

    /// A group's row folded or unfolded.
    pub fn toggle_fold(&mut self, group: &str) {
        if !self.pieces.folded.remove(group) {
            self.pieces.folded.insert(group.to_string());
        }
    }

    /// The things locked by their places, kept so through a step that
    /// changes them (`Editor::change`: an unnamed one is known as it is).
    pub(super) fn locked_things(&self) -> Vec<usize> {
        if self.pieces.locked.is_empty() {
            return Vec::new();
        }
        (0..self.level.things.len()).filter(|&i| self.is_locked(Item::Thing(i))).collect()
    }

    pub(super) fn lock_again(&mut self, things: &[usize]) {
        self.pieces.locked.rekeyed(&self.level, things);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridLevel;

    const LEVEL: &str = r##"format = "voxcade level 1"
size = [360, 200, 120]

[[thing]]
name = "fir"
model = "trees/fir.vvx"
at = [40, 150, 0]
group = "forest"

[[thing]]
name = "lamp"
model = "room/lamp.vvx"
at = [212, 40, 0]

[[thing]]
model = "trees/fir.vvx"
at = [60, 150, 0]    # the second fir
group = "forest"

[[thing]]
name = "wall"
model = "room/wall.vvx"
at = [100, 20, 0]
group = "house"
"##;

    fn open(name: &str, text: &str) -> (Editor, crate::scratch::Scratch) {
        let dir = crate::scratch::Scratch::new(&format!("groups-{name}"));
        std::fs::write(dir.join("room.toml"), text).unwrap();
        (Editor::open(&dir, "room.toml").unwrap(), dir)
    }

    fn saved(dir: &std::path::Path) -> String {
        std::fs::read_to_string(dir.join("room.toml")).unwrap()
    }

    /// Lines of `b` that `a` has not, in order.
    fn new_lines(a: &str, b: &str) -> Vec<String> {
        b.lines().filter(|l| !a.lines().any(|p| p == *l)).map(String::from).collect()
    }

    /// A level without groups opens and, a thing moved and moved back, is
    /// the same to the byte; the groups in the order of their first things.
    #[test]
    fn a_level_without_groups_is_written_as_it_was() {
        let plain = crate::grid::tests::PIECES;
        let (mut ed, dir) = open("plain", plain);
        assert!(ed.groups().is_empty());
        ed.choose_item(Some(Item::Thing(0)));
        ed.nudge(glam::IVec3::X);
        ed.nudge(-glam::IVec3::X);
        assert_eq!(saved(&dir), plain);
        let (ed, _dir) = open("order", LEVEL);
        assert_eq!(ed.groups(), ["forest", "house"]);
        assert_eq!(ed.group_items("forest"), [Item::Thing(0), Item::Thing(2)]);
    }

    /// GROUP sets the chosen thing's in one step and its one line; RENAME
    /// and UNGROUP change every thing of one in one step, only their lines;
    /// UNDO brings the file back to the byte each time; a name with quotes
    /// is refused, nothing written.
    #[test]
    fn a_group_is_set_renamed_and_ungrouped_in_one_step_each() {
        let (mut ed, dir) = open("steps", LEVEL);
        let steps = ed.steps();
        ed.choose_item(Some(Item::Thing(1)));
        ed.set_group("house").unwrap();
        assert_eq!((ed.steps(), new_lines(LEVEL, &saved(&dir))), (steps + 1, vec![]), "a line like the wall's");
        assert!(saved(&dir).contains("at = [212, 40, 0]\ngroup = \"house\"\n"), "{}", saved(&dir));
        ed.undo();
        assert_eq!(saved(&dir), LEVEL, "UNDO: the file to the byte");
        ed.set_group(" garden ").unwrap();
        assert_eq!(new_lines(LEVEL, &saved(&dir)), ["group = \"garden\""]);
        ed.set_group("").unwrap();
        assert_eq!(saved(&dir), LEVEL, "cleared: no group");
        assert!(ed.set_group("a \"b\"").is_err() && saved(&dir) == LEVEL);
        // RENAME: both firs, one step, the comment kept.
        ed.toggle_fold("forest");
        ed.rename_group("forest", "woods").unwrap();
        assert_eq!((ed.steps(), new_lines(LEVEL, &saved(&dir))), (steps + 3, vec!["group = \"woods\"".to_string(), "group = \"woods\"".into()]));
        assert!(saved(&dir).contains("# the second fir") && ed.pieces.folded.contains("woods"), "folded still under its new name");
        assert!(ed.rename_group("woods", "  ").is_err());
        ed.undo();
        assert_eq!(saved(&dir), LEVEL);
        // UNGROUP: their lines gone.
        ed.ungroup("forest");
        assert_eq!(ed.steps(), steps + 3);
        let file = GridLevel::from_text(&saved(&dir)).unwrap();
        assert!(file.things.iter().take(3).all(|t| t.group.is_none()) && saved(&dir).lines().count() == LEVEL.lines().count() - 2);
        ed.undo();
        assert_eq!(saved(&dir), LEVEL);
    }

    /// A group's hide mark hides its things and shows them again, nothing
    /// written; its lock mark locks them, kept through a step that moves
    /// one; locks and folds are not written.
    #[test]
    fn a_groups_marks_hide_and_lock_its_things() {
        let (mut ed, dir) = open("marks", LEVEL);
        ed.toggle_group_hidden("forest");
        assert!(ed.group_hidden("forest") && ed.is_hidden(Item::Thing(0)) && ed.is_hidden(Item::Thing(2)) && !ed.is_hidden(Item::Thing(3)));
        ed.toggle_group_hidden("forest");
        assert!(!ed.is_hidden(Item::Thing(0)) && !ed.is_hidden(Item::Thing(2)));
        ed.toggle_group_locked("forest");
        assert!(ed.group_locked("forest") && ed.is_locked(Item::Thing(2)) && !ed.is_locked(Item::Thing(1)));
        // The unnamed fir moved by its row's choice: locked still.
        ed.choose_item(Some(Item::Thing(2)));
        ed.nudge(glam::IVec3::X);
        assert!(ed.is_locked(Item::Thing(2)), "locked through the step");
        ed.undo();
        assert!(ed.is_locked(Item::Thing(2)) && ed.group_locked("forest"), "and through its undo");
        ed.redo();
        assert!(ed.is_locked(Item::Thing(2)), "and its redo");
        ed.toggle_locked(Item::Thing(2));
        assert!(!ed.group_locked("forest") && ed.is_locked(Item::Thing(0)));
        ed.toggle_fold("house");
        assert!(new_lines(LEVEL, &saved(&dir)).iter().all(|l| l.starts_with("at = ")), "only the nudge written");
    }
}
