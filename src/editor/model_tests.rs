//! The editor's own tests (`model::tests`): a level of blocks and a
//! model shaped without a window.

use super::*;

pub(crate) const N: i32 = 4;

/// A game folder with a 3 by 2 by 2 level of one block (a solid cube,
/// grey) in cell (0, 0, 0), and a tileset of two tiles (1 red, 2
/// nothing).
pub(crate) fn folder(name: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("editor-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut level = GridLevel::new(N, IVec3::new(3, 2, 2));
    level.blocks.push(BlockEntry { name: "1".into(), kind: Kind::Solid });
    level.cells[0] = Cell { block: 1, turn: Turn::NONE };
    level.save(&dir.join("level.toml")).unwrap();
    let mut cube = Block::empty(N);
    for i in 0..N * N * N {
        cube.set(IVec3::new(i % N, i / N % N, i / (N * N)), Some([128, 128, 128])).unwrap();
    }
    cube.save(&blocks::path(&dir, "1")).unwrap();
    // Two tiles side by side: rows of 2N pixels.
    let mut rgba = Vec::new();
    for _ in 0..N {
        for x in 0..2 * N {
            rgba.extend(if x < N { [200, 0, 0, 255] } else { [0; 4] });
        }
    }
    crate::headless::write_png(&dir.join("blocks").join(crate::tiled::SHEET), (2 * N) as u32, N as u32, &rgba).unwrap();
    dir
}

/// A ray straight down onto cell (x, y) of the grid, from high above.
fn down(x: i32, y: i32) -> (Vec3, Vec3) {
    (Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 50.0), Vec3::NEG_Z)
}

#[test]
fn a_ray_finds_the_first_cell_and_the_face_it_enters() {
    let size = IVec3::new(4, 4, 4);
    let filled = |c: IVec3| c == IVec3::new(2, 1, 1) || c == IVec3::new(3, 1, 1);
    // From the front (-y), level with the cell: its front face.
    let hit = pick(Vec3::new(2.5, -5.0, 1.5), Vec3::Y, size, filled).unwrap();
    assert_eq!(hit, Hit { cell: IVec3::new(2, 1, 1), normal: IVec3::NEG_Y });
    // From the left, slanting down: the first of the two, its left face.
    let hit = pick(Vec3::new(-3.0, 1.5, 2.2), Vec3::new(1.0, 0.0, -0.2).normalize(), size, filled).unwrap();
    assert_eq!(hit, Hit { cell: IVec3::new(2, 1, 1), normal: IVec3::NEG_X });
    // From above: its top.
    let (o, d) = down(3, 1);
    assert_eq!(pick(o, d, size, filled).unwrap(), Hit { cell: IVec3::new(3, 1, 1), normal: IVec3::Z });
    // Past the grid: nothing.
    assert_eq!(pick(Vec3::new(-3.0, 9.0, 2.0), Vec3::X, size, filled), None);
}

#[test]
fn a_ray_over_nothing_lands_on_the_floor() {
    let (o, d) = down(1, 2);
    let hit = pick(o, d, IVec3::new(4, 4, 4), |_| false).unwrap();
    assert_eq!(hit, Hit { cell: IVec3::new(1, 2, -1), normal: IVec3::Z });
}

#[test]
fn placing_turning_and_removing_blocks() {
    let dir = folder("place");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    // (Tools that change voxels: POINTER is the default.)
    ed.choose(crate::editor::model::Tool::Add);
    // Onto the block's top: a block in the cell above.
    let (o, d) = down(0, 0);
    let hit = ed.aim(o, d).unwrap();
    assert_eq!(hit, Hit { cell: IVec3::ZERO, normal: IVec3::Z });
    ed.turn(Turn::TURN);
    ed.press(hit);
    ed.release();
    assert_eq!(ed.level.cell(IVec3::new(0, 0, 1)), Cell { block: 1, turn: Turn::TURN });
    // On the floor beside it.
    let (o, d) = down(2, 1);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    assert_eq!(ed.level.cell(IVec3::new(2, 1, 0)).block, 1);
    // Further right than the grid reaches: it grows by a column.
    let hit = ed.aim(Vec3::new(9.0, 1.5, 0.5), Vec3::NEG_X).unwrap();
    assert_eq!(hit, Hit { cell: IVec3::new(2, 1, 0), normal: IVec3::X });
    ed.press(hit);
    ed.release();
    assert_eq!(ed.level.size, IVec3::new(4, 2, 2));
    assert_eq!(ed.level.cell(IVec3::new(3, 1, 0)).block, 1);
    assert_eq!(ed.level.cell(IVec3::ZERO).block, 1, "the cells kept their places");
    // Removed.
    ed.choose(Tool::Remove);
    let (o, d) = down(0, 0);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    assert_eq!(ed.level.cell(IVec3::new(0, 0, 1)).block, 0);
    assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap(), ed.level);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn painting_recolours_and_adding_grows_from_the_face() {
    let dir = folder("paint");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.open_block(0);
    assert_eq!(ed.colour, [128, 128, 128]);
    ed.colour = [10, 200, 10];
    ed.choose(Tool::Paint);
    let (o, d) = down(1, 2);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    assert_eq!(ed.blocks[0].colour_at(IVec3::new(1, 2, N - 1)), Some([10, 200, 10]));
    ed.choose(Tool::Add);
    let hit = ed.aim(Vec3::new(1.5, -9.0, 0.5), Vec3::Y).unwrap();
    ed.press(hit);
    ed.release();
    assert_eq!(hit.normal, IVec3::NEG_Y);
    assert_eq!(ed.blocks[0].get(IVec3::new(1, -1, 0)), 0, "nothing before the block: it stays N each way");
    ed.choose(Tool::Remove);
    ed.press(hit);
    ed.release();
    assert_eq!(ed.blocks[0].get(IVec3::new(1, 0, 0)), 0);
    ed.choose(Tool::Add);
    ed.press(ed.aim(Vec3::new(1.5, -9.0, 0.5), Vec3::Y).unwrap());
    ed.release();
    assert_eq!(ed.blocks[0].colour_at(IVec3::new(1, 0, 0)), Some([10, 200, 10]), "added back before the face behind");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deep_paint_colours_every_voxel_behind() {
    let dir = folder("deep");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.open_block(0);
    // A hole in the column, which stays a hole.
    ed.blocks[0].set(IVec3::new(1, 2, 2), None).unwrap();
    ed.colour = [250, 20, 20];
    ed.choose(Tool::Deep);
    ed.press(ed.aim(Vec3::new(1.5, -9.0, 2.5), Vec3::Y).unwrap());
    ed.release();
    let red = Some([250, 20, 20]);
    for y in 0..N {
        let want = if y == 2 { None } else { red };
        assert_eq!(ed.blocks[0].colour_at(IVec3::new(1, y, 2)), want, "through the depth, at y {y}");
    }
    assert_eq!(ed.blocks[0].colour_at(IVec3::new(2, 1, 2)), Some([128, 128, 128]), "the next column stays");
    // From above: down through the block.
    let (o, d) = down(3, 3);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    assert!((0..N).all(|z| ed.blocks[0].colour_at(IVec3::new(3, 3, z)) == red));
    assert_eq!(Block::load(&blocks::path(&dir, "1"), N).unwrap(), ed.blocks[0], "saved");
    ed.back_to_level();
    assert_eq!(ed.tool, Tool::Paint, "the level has no deep paint");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn turning_a_block_turns_its_voxels() {
    let dir = folder("turn-block");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.open_block(0);
    ed.blocks[0].set(IVec3::new(0, N - 1, 0), Some([1, 2, 3])).unwrap();
    let made = ed.blocks[0].clone();
    ed.turn_block(Turn::TURN);
    ed.turn_block(Turn::TURN);
    assert_eq!(ed.blocks[0].colour_at(IVec3::new(N - 1, 0, 0)), Some([1, 2, 3]), "the back left corner comes round to the front right");
    assert_eq!(Block::load(&blocks::path(&dir, "1"), N).unwrap(), ed.blocks[0], "saved");
    ed.undo();
    ed.undo();
    assert_eq!(ed.blocks[0], made);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cloning_a_block_makes_a_copy_to_shape() {
    let dir = folder("clone");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.set_kind(Kind::Oneway);
    ed.clone_block().unwrap();
    assert_eq!(ed.level.blocks[1], BlockEntry { name: "3".into(), kind: Kind::Oneway });
    assert_eq!((ed.held.block, ed.scope), (2, Scope::Level), "the copy is held");
    assert_eq!(Block::load(&blocks::path(&dir, "3"), N).unwrap(), ed.blocks[0]);
    // In a block: the copy opens, to shape instead of the first.
    ed.open_block(0);
    ed.clone_block().unwrap();
    assert_eq!(ed.scope, Scope::Block(2));
    assert_eq!(ed.level.blocks[2].name, "4");
    assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deleting_a_block_empties_its_cells_and_undo_brings_it_back() {
    let dir = folder("delete");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    // (Tools that change voxels: POINTER is the default.)
    ed.choose(crate::editor::model::Tool::Add);
    ed.clone_block().unwrap();
    // The copy (block 2) in a cell beside the first.
    let (o, d) = down(1, 0);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    assert_eq!(ed.level.cell(IVec3::new(1, 0, 0)).block, 2);
    let start = ed.level.clone();
    ed.hold(0);
    ed.delete_block();
    assert_eq!(ed.level.blocks.len(), 1);
    assert_eq!(ed.level.cell(IVec3::ZERO).block, 0, "its cell is empty");
    assert_eq!(ed.level.cell(IVec3::new(1, 0, 0)).block, 1, "the copy is block 1 now");
    assert!(!blocks::path(&dir, "1").exists(), "its file is gone");
    assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap(), ed.level);
    ed.undo();
    assert_eq!(ed.level, start);
    assert_eq!(Block::load(&blocks::path(&dir, "1"), N).unwrap(), ed.blocks[0], "its file is back");
    ed.redo();
    assert_eq!(ed.level.blocks[0].name, "3");
    // Deleting the open block goes back to the level.
    ed.open_block(0);
    ed.delete_block();
    assert_eq!((ed.scope, ed.level.blocks.len(), ed.held.block), (Scope::Level, 0, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stroke_works_on_the_surface_it_began_on() {
    let dir = folder("stroke");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.open_block(0);
    ed.choose(Tool::Remove);
    // Dragged twice over the same voxel from above: one layer off.
    let (o, d) = down(2, 2);
    ed.press(ed.aim(o, d).unwrap());
    ed.drag(ed.aim(o, d).unwrap());
    ed.drag(ed.aim(o, d).unwrap());
    ed.release();
    assert_eq!(ed.blocks[0].get(IVec3::new(2, 2, N - 1)), 0);
    assert_ne!(ed.blocks[0].get(IVec3::new(2, 2, N - 2)), 0, "the stroke did not dig in");
    // Placing dragged over the same cell: one block, not a tower.
    ed.back_to_level();
    ed.choose(Tool::Add);
    let (o, d) = down(0, 0);
    ed.press(ed.aim(o, d).unwrap());
    ed.drag(ed.aim(o, d).unwrap());
    ed.drag(ed.aim(o, d).unwrap());
    ed.release();
    assert_eq!(ed.level.cell(IVec3::new(0, 0, 1)).block, 1);
    assert_eq!(ed.level.size.z, 2, "the stroke did not grow upwards");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_and_redo_step_through_both_levels() {
    let dir = folder("undo");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    // (Tools that change voxels: POINTER is the default.)
    ed.choose(crate::editor::model::Tool::Add);
    let start = (ed.level.clone(), ed.blocks[0].clone());
    let (o, d) = down(1, 0);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    ed.open_block(0);
    ed.choose(Tool::Remove);
    let (o, d) = down(0, 0);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    let end = (ed.level.clone(), ed.blocks[0].clone());
    ed.undo();
    assert_eq!(ed.blocks[0], start.1);
    assert_ne!(ed.level, start.0);
    ed.undo();
    assert_eq!((ed.level.clone(), ed.blocks[0].clone()), start);
    ed.undo();
    assert_eq!((ed.level.clone(), ed.blocks[0].clone()), start, "nothing further back");
    ed.redo();
    ed.redo();
    assert_eq!((ed.level.clone(), ed.blocks[0].clone()), end);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_change_is_saved_at_once() {
    let dir = folder("saved");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.open_block(0);
    ed.choose(Tool::Remove);
    let (o, d) = down(3, 3);
    ed.press(ed.aim(o, d).unwrap());
    ed.release();
    let on_disk = || Block::load(&blocks::path(&dir, "1"), N).unwrap();
    assert_eq!(on_disk(), ed.blocks[0]);
    ed.undo();
    assert_eq!(on_disk(), ed.blocks[0]);
    assert_ne!(on_disk().get(IVec3::new(3, 3, N - 1)), 0);
    let names: Vec<_> = std::fs::read_dir(dir.join("blocks")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert!(names.iter().all(|n| !n.starts_with('.')), "no file half written: {names:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_block_is_drawn_out_from_its_tile() {
    let dir = folder("new");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    assert!(ed.sheet.as_ref().unwrap().tile(2).is_none(), "an empty tile");
    ed.new_block(Some(1)).unwrap();
    assert_eq!(ed.held.block, 1, "the level has tile 1's block: it is held");
    let e = ed.new_block(Some(2)).unwrap_err().to_string();
    assert!(e.contains("no tile 2"), "{e}");
    ed.new_block(None).unwrap();
    assert_eq!(ed.level.blocks.len(), 2);
    assert_eq!(ed.level.blocks[1].name, "3", "numbered on after the tiles");
    assert_eq!(ed.held.block, 2);
    assert!(blocks::path(&dir, "3").exists());
    assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks.len(), 2);
    // Without its own block, a tile's is drawn out of the tileset.
    std::fs::remove_file(blocks::path(&dir, "1")).unwrap();
    ed.level.blocks[0].name = "old".into();
    ed.new_block(Some(1)).unwrap();
    assert_eq!(ed.blocks[2].colour_at(IVec3::new(0, N - 1, 0)), Some([200, 0, 0]));
    ed.undo();
    assert_eq!(ed.level.blocks.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changing_a_kind_is_saved_with_the_level() {
    let dir = folder("kind");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    assert_eq!(ed.kind(), Some(Kind::Solid));
    ed.set_kind(Kind::Oneway);
    assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks[0].kind, Kind::Oneway);
    ed.undo();
    assert_eq!(GridLevel::read(&dir.join("level.toml")).unwrap().blocks[0].kind, Kind::Solid);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn picking_takes_up_and_goes_back_to_the_tool() {
    let dir = folder("pick");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.held = Cell::default();
    ed.choose(Tool::Remove);
    ed.choose(Tool::Pick);
    let (o, d) = down(0, 0);
    ed.press(ed.aim(o, d).unwrap());
    assert_eq!((ed.held.block, ed.tool), (1, Tool::Remove));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Things are put down, moved and taken away, each one step back, and
/// the level saved with them at once; PICK takes up a thing's kind.
#[test]
fn placing_moving_and_removing_things() {
    let dir = folder("things");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    // (Tools that change voxels: POINTER is the default.)
    ed.choose(crate::editor::model::Tool::Add);
    ed.kinds = vec!["fox".into(), "gem".into()];
    ed.hold_kind(1);
    ed.switch(Layer::Things);
    let saved = || GridLevel::read(&dir.join("level.toml")).unwrap().things;
    let gem = |at: [i32; 3]| vec![Thing::new("gem", at)];
    ed.press_thing(Some(IVec3::new(2, 2, 8)), None);
    ed.release();
    assert_eq!((&ed.level.things, saved()), (&gem([2, 2, 8]), gem([2, 2, 8])));
    // Pressed over nothing, REMOVE and MOVE do nothing.
    for tool in [Tool::Remove, Tool::Move] {
        ed.choose(tool);
        ed.press_thing(Some(IVec3::new(5, 2, 8)), None);
        ed.release();
    }
    assert_eq!(ed.steps(), 1);
    ed.choose(Tool::Move);
    ed.press_thing(Some(IVec3::new(2, 2, 8)), Some(0));
    ed.drag_thing(Some(IVec3::new(6, 2, 8)));
    ed.drag_thing(Some(IVec3::new(7, 6, 4)));
    ed.release();
    assert_eq!((ed.steps(), saved()), (2, gem([7, 6, 4])), "a drag is one step");
    ed.hold_kind(0);
    ed.choose(Tool::Pick);
    ed.press_thing(None, Some(0));
    assert_eq!((ed.held_kind.as_deref(), ed.tool), (Some("gem"), Tool::Move));
    ed.choose(Tool::Remove);
    ed.press_thing(None, Some(0));
    ed.release();
    assert!(ed.level.things.is_empty() && saved().is_empty());
    ed.undo();
    assert_eq!(saved(), gem([7, 6, 4]));
    ed.undo();
    assert_eq!(saved(), gem([2, 2, 8]));
    ed.redo();
    assert_eq!(ed.level.things, gem([7, 6, 4]));
    // Back to the blocks, a tool they lack becomes PLACE.
    ed.choose(Tool::Move);
    ed.switch(Layer::Blocks);
    assert_eq!(ed.tool, Tool::Add);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A thing stands before the face the mouse points at: on a top, or
/// before a wall, in the middle of its row's depth (the lane); on the
/// floor in front of nothing; never off the level's sides.
#[test]
fn a_thing_stands_before_the_face_it_is_put_on() {
    let dir = folder("spot");
    let ed = Editor::open(&dir, "level.toml").unwrap();
    let spot = |voxel: [i32; 3], normal: [i32; 3]| ed.thing_spot(IVec3::from(voxel), IVec3::from(normal)).map(|s| s.to_array());
    assert_eq!(spot([1, 1, 3], [0, 0, 1]), Some([1, 2, 4]), "on a top, in the front row's middle");
    assert_eq!(spot([5, 4, 2], [0, -1, 0]), Some([5, 2, 2]), "before a wall of the back row: in the front row");
    assert_eq!(spot([3, 0, 2], [0, -1, 0]), Some([3, 2, 2]), "before the front: still in the front row");
    assert_eq!(spot([5, 6, -1], [0, 0, 1]), Some([5, 6, 0]), "on the floor of the back row");
    assert_eq!(spot([11, 1, 3], [1, 0, 0]), None, "off the side");
    assert_eq!(spot([1, 1, 3], [0, 0, 0]), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A model of two frames 3 by 2 by 5, a glass colour among its
/// colours: a column in the near row, its top glass in the first frame.
fn model_folder(name: &str) -> crate::scratch::Scratch {
    let dir = crate::scratch::Scratch::new(&format!("editor-{name}"));
    let frame = |top: char| format!("[[frame]]\nlayers = \"\"\"\n{}...\n.{top}.\n\"\"\"\n", "...\n.a.\n\n".repeat(4));
    let text = format!("format = \"{}\"\nsize = [3, 2, 5]\n\n[palette]\na = \"#c82828\"\nb = {{ color = \"#78c8ff\", glass = 0.5 }}\n\n{}\n{}", crate::vvx::FORMAT, frame('b'), frame('a'));
    std::fs::write(dir.join("thing.vvx"), text).unwrap();
    dir
}

/// BOX's and SPHERE's shapes: a box of 3 with its edges cut back is a
/// cross, a sphere of 3 a cube without its corners; flat ones keep
/// their middle.
#[test]
fn boxes_and_spheres_are_clean_shapes() {
    let (lo, hi) = (IVec3::ZERO, IVec3::splat(2));
    assert_eq!(form_cells(Tool::Box, lo, hi, false).len(), 27);
    assert_eq!(form_cells(Tool::Box, lo, hi, true).len(), 7, "the edges and corners cut");
    assert_eq!(form_cells(Tool::Sphere, lo, hi, false).len(), 19, "the corners off");
    assert_eq!(form_cells(Tool::Sphere, lo, IVec3::new(4, 4, 0), false).len(), 21, "a disc");
    assert_eq!(form_cells(Tool::Box, lo, IVec3::new(4, 0, 0), true).len(), 5, "a bar keeps its ends");
}

/// BOX on a model: a footprint dragged out on the top face and let go
/// is set right there (its depth the last one used, 1 at first), the
/// box grown to hold it; fresh, + and - change its depth in place and
/// BEVEL its edges, still one step, UNDO takes it away whole; the next
/// drag begins a new one and leaves the first; Alt fills only where
/// empty, Ctrl cuts; with SYMMETRY the other side too.
#[test]
fn a_box_is_set_where_it_is_drawn() {
    let dir = model_folder("box");
    let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
    ed.choose(Tool::Box);
    ed.colour = [10, 200, 10];
    let first = ed.grid();
    let green = |ed: &Editor, x: i32, z: i32| ed.shape().unwrap().colour_at(IVec3::new(x, 0, z)) == Some([10, 200, 10]);
    // On top of the voxel at (1, 0, 4), dragged to x 3, let go.
    ed.begin_form(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z }, false);
    ed.draw_form(Vec3::new(3.5, 0.5, 30.0), Vec3::NEG_Z);
    assert_eq!(ed.form_box(), Some((IVec3::new(1, 0, 5), IVec3::new(3, 0, 5))), "one deep at first");
    ed.set_drawn(false);
    assert!(ed.forms.drawing.is_none() && ed.forms.fresh.is_some());
    // Up, and across on both sides of the pivot (the model stands where
    // it stood): its voxels a column to the right.
    assert_eq!((ed.grid(), ed.history.moved), (IVec3::new(5, 2, 6), IVec3::X), "grown right and up to hold it");
    assert!((2..=4).all(|x| green(&ed, x, 5)) && ed.steps() == 1);
    assert_eq!(ed.form_box(), Some((IVec3::new(2, 0, 5), IVec3::new(4, 0, 5))), "fresh where it stands now");
    // Fresh: a voxel deeper, in place, still one step.
    ed.deepen(1);
    assert_eq!((ed.grid(), ed.history.moved, ed.steps(), ed.forms.depth), (IVec3::new(5, 2, 7), IVec3::X, 1, 2));
    assert!((2..=4).all(|x| green(&ed, x, 5) && green(&ed, x, 6)));
    ed.deepen(1);
    ed.deepen(-1);
    assert!(!green(&ed, 3, 7) && green(&ed, 3, 6) && ed.steps() == 1, "back to two deep");
    // UNDO: the whole box away.
    ed.undo();
    assert_eq!((ed.grid(), ed.steps(), ed.forms.fresh), (first, 0, None));
    // A new one, two deep (the depth used last), then another drag: the
    // first stays as it is, + acts on the second.
    ed.begin_form(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z }, false);
    ed.draw_form(Vec3::new(3.5, 0.5, 30.0), Vec3::NEG_Z);
    ed.set_drawn(false);
    assert_eq!(ed.form_box(), Some((IVec3::new(2, 0, 5), IVec3::new(4, 0, 6))));
    ed.begin_form(Hit { cell: IVec3::new(0, 1, 4), normal: IVec3::Z }, false);
    ed.set_drawn(false);
    ed.deepen(1);
    assert!(green(&ed, 3, 6) && !green(&ed, 3, 7), "the first as it was");
    assert_eq!(ed.steps(), 2);
    // Escape: fresh no more; + is the next footprint's depth.
    assert!(ed.drop_form());
    let shape = ed.shape().unwrap().clone();
    ed.deepen(1);
    assert!(*ed.shape().unwrap() == shape && ed.forms.depth == 4);
    ed.forms.depth = 2;
    // Ctrl: cut into the voxels as drawn.
    ed.begin_form(Hit { cell: IVec3::new(3, 0, 6), normal: IVec3::Z }, true);
    ed.draw_form(Vec3::new(4.5, 0.5, 30.0), Vec3::NEG_Z);
    ed.set_drawn(false);
    assert!((3..=4).all(|x| ed.shape().unwrap().get(IVec3::new(x, 0, 6)) == 0 && ed.shape().unwrap().get(IVec3::new(x, 0, 5)) == 0), "cut two deep");
    ed.undo();
    assert!(green(&ed, 3, 6), "the cut taken back");
    // Alt: only where empty; the box's own voxels keep their colour.
    ed.colour = [255, 255, 0];
    ed.forms.depth = 1;
    ed.begin_form(Hit { cell: IVec3::new(3, 0, 4), normal: IVec3::Z }, false);
    ed.set_drawn(true);
    assert!(green(&ed, 3, 5), "filled only where empty");
    // SYMMETRY: a voxel added at the left, its mirror at the right.
    ed.symmetry = true;
    ed.choose(Tool::Add);
    let w = ed.grid().x;
    ed.press(Hit { cell: IVec3::new(0, 1, 0), normal: IVec3::Z });
    ed.release();
    let frame = ed.shape().unwrap();
    assert!(frame.get(IVec3::new(0, 1, 1)) != 0 && frame.get(IVec3::new(w - 1, 1, 1)) != 0, "both sides");
    let _ = std::fs::remove_dir_all(&dir);
}

/// BEVEL on a fresh box cuts its edges in place, one step still; a
/// sphere set the same way, in a block.
#[test]
fn bevel_and_sphere_act_on_the_fresh_form() {
    let dir = model_folder("bevel");
    let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
    ed.choose(Tool::Box);
    ed.forms.depth = 3;
    ed.begin_form(Hit { cell: IVec3::new(0, 0, 4), normal: IVec3::Z }, false);
    ed.draw_form(Vec3::new(2.5, 2.5, 30.0), Vec3::NEG_Z);
    ed.set_drawn(false);
    let (lo, hi) = ed.form_box().unwrap();
    let count = |ed: &Editor| (lo.z..=hi.z).flat_map(|z| (lo.y..=hi.y).flat_map(move |y| (lo.x..=hi.x).map(move |x| IVec3::new(x, y, z)))).filter(|c| ed.shape().unwrap().get(*c) != 0).count();
    assert_eq!(count(&ed), 27);
    ed.toggle_bevel();
    assert_eq!((count(&ed), ed.steps()), (7, 1), "its edges and corners cut, one step");
    ed.choose(Tool::Sphere);
    assert!(ed.forms.fresh.is_none(), "another tool: fresh no more");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Shift held makes the tool PICK, a click picks the colour under the
/// mouse and the tool stays PICK; letting go brings the tool before
/// back, for BOX with its shape still held; a stroke in the middle is
/// not interrupted; a tool chosen meanwhile stays.
#[test]
fn shift_held_picks_and_letting_go_gives_the_tool_back() {
    let dir = model_folder("shift_pick");
    let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
    let top = Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z };
    ed.choose(Tool::Paint);
    ed.colour = [10, 200, 10];
    ed.press(top);
    ed.release();
    ed.choose(Tool::Remove);
    ed.colour = [0, 0, 0];
    ed.shift_pick(true);
    assert_eq!(ed.tool, Tool::Pick, "Shift held: PICK");
    ed.press(top);
    assert_eq!((ed.colour, ed.tool), ([10, 200, 10], Tool::Pick), "picked, still PICK while Shift is held");
    ed.press(top);
    assert_eq!(ed.tool, Tool::Pick, "another pick, the same");
    ed.shift_pick(false);
    assert_eq!((ed.tool, ed.colour), (Tool::Remove, [10, 200, 10]), "let go: the tool before, the color picked");
    // Shift again and again changes nothing in between; PICK chosen by
    // hand is not given back by it.
    ed.shift_pick(false);
    assert_eq!(ed.tool, Tool::Remove);
    ed.choose(Tool::Pick);
    ed.shift_pick(true);
    ed.shift_pick(false);
    assert_eq!(ed.tool, Tool::Pick, "PICK chosen is PICK until a pick");
    ed.choose(Tool::Paint);
    // A stroke under way keeps its tool.
    ed.press(top);
    ed.shift_pick(true);
    assert_eq!(ed.tool, Tool::Paint, "not in the middle of a stroke");
    ed.release();
    ed.shift_pick(false);
    // A tool chosen with Shift held stays when it is let go.
    ed.shift_pick(true);
    ed.choose(Tool::Add);
    ed.shift_pick(false);
    assert_eq!(ed.tool, Tool::Add);
    // BOX: picking and back.
    ed.choose(Tool::Box);
    ed.colour = [0, 0, 0];
    ed.shift_pick(true);
    assert!(!ed.forms(), "no shape drawn while PICK");
    ed.press(top);
    ed.shift_pick(false);
    assert_eq!((ed.tool, ed.colour), (Tool::Box, [10, 200, 10]));
    assert!(ed.forms());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A colour's material set by its words, a slider dragged one step
/// back; a colour no voxel has stays in the palette until taken out.
#[test]
fn a_colours_material_is_set_and_a_colour_taken_out() {
    let dir = model_folder("material");
    let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
    // (Tools that change voxels: POINTER is the default.)
    ed.choose(crate::editor::model::Tool::Add);
    ed.colour = [200, 40, 40];
    let m = ed.material().unwrap();
    ed.set_material(super::super::frames::with_word(m, 1, 0.5));
    ed.set_material(super::super::frames::with_word(ed.material().unwrap(), 1, 0.8));
    assert_eq!(ed.steps(), 1, "one step for the slider");
    let text = std::fs::read_to_string(dir.join("thing.vvx")).unwrap();
    assert!(text.contains("a = { color = \"#c82828\", metal = 0.8, rough = 0.3 }"), "{text}");
    ed.undo();
    assert_eq!(ed.material(), Some(m));
    assert!(std::fs::read_to_string(dir.join("thing.vvx")).unwrap().contains("a = \"#c82828\""));
    // A new colour painted, then erased: it keeps its place, then goes.
    ed.colour = [1, 2, 3];
    ed.press(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z });
    ed.release();
    ed.choose(Tool::Remove);
    ed.press(Hit { cell: IVec3::new(1, 0, 5), normal: IVec3::Z });
    ed.release();
    assert!(ed.colours().1.contains(&[1, 2, 3]) && std::fs::read_to_string(dir.join("thing.vvx")).unwrap().contains("#010203"));
    ed.forget_colour([1, 2, 3]);
    assert!(!ed.colours().1.contains(&[1, 2, 3]) && !std::fs::read_to_string(dir.join("thing.vvx")).unwrap().contains("#010203"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A model opens at its first frame, any size; ADD on its top grows
/// its box (every frame's), frames are copied, taken out and turned,
/// each a step saved at once; the file keeps its glass.
#[test]
fn a_model_of_any_size_is_edited_and_saved() {
    let dir = model_folder("model");
    let mut ed = Editor::open_model(&dir, "thing.vvx").unwrap();
    // (Tools that change voxels: POINTER is the default.)
    ed.choose(crate::editor::model::Tool::Add);
    assert_eq!((ed.scope, ed.grid(), ed.frames().len()), (Scope::Frame(0), IVec3::new(3, 2, 5), 2));
    ed.colour = [200, 40, 40];
    ed.press(Hit { cell: IVec3::new(1, 0, 4), normal: IVec3::Z });
    ed.release();
    assert_eq!(ed.grid(), IVec3::new(3, 2, 6), "grown up by one");
    assert!(ed.frames().iter().all(|f| f.size == IVec3::new(3, 2, 6)), "every frame's box");
    assert_eq!(ed.shape().unwrap().colour_at(IVec3::new(1, 0, 5)), Some([200, 40, 40]));
    ed.step_frame(1);
    assert_eq!(ed.scope, Scope::Frame(1));
    ed.copy_frame();
    assert_eq!((ed.scope, ed.frames().len()), (Scope::Frame(2), 3));
    ed.delete_frame();
    assert_eq!((ed.scope, ed.frames().len()), (Scope::Frame(1), 2));
    ed.turn_block(Turn::TURN);
    assert_eq!(ed.grid(), IVec3::new(2, 3, 6), "a box's sides change places");
    ed.undo();
    assert_eq!(ed.grid(), IVec3::new(3, 2, 6));
    ed.trim();
    assert_eq!(ed.grid(), IVec3::new(1, 2, 6), "cut to the voxels of both frames, about the pivot (the near row of two)");
    // Saved: the model as the game reads it.
    let back = crate::vox::load_model(&dir.join("thing.vvx")).unwrap();
    assert_eq!((back.frames.len(), back.size), (2, IVec3::new(1, 2, 6)));
    let top = back.frames[0].iter().find(|(p, _)| *p == IVec3::new(0, 0, 4)).unwrap().1;
    assert_eq!(top, [120, 200, 255, 128], "glass stays glass");
    assert!(back.frames[0].iter().any(|(p, c)| *p == IVec3::new(0, 0, 5) && c[3] == 255));
    // ADD to the left of the box grows it there: the voxels move right
    // by one, and the view with them.
    ed.history.moved = IVec3::ZERO;
    ed.press(Hit { cell: IVec3::new(0, 0, 2), normal: IVec3::NEG_X });
    ed.release();
    assert_eq!((ed.grid(), ed.history.moved), (IVec3::new(2, 2, 6), IVec3::X), "grown to the left");
    let frame = ed.shape().unwrap();
    assert_eq!((frame.colour_at(IVec3::new(0, 0, 2)), frame.get(IVec3::new(1, 0, 2)) != 0), (Some([200, 40, 40]), true));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Undo keeps at most so many steps, and so many bytes of them: past
/// either the oldest go (the newest stays, even alone over the bytes),
/// the count of steps taken still counts them, and what is kept is taken
/// back step by step as before.
#[test]
fn undo_keeps_a_bounded_number_of_steps_and_bytes() {
    let dir = folder("undo-bound");
    let mut ed = Editor::open(&dir, "level.toml").unwrap();
    ed.open_block(0);
    ed.choose(Tool::Paint);
    let paint = |ed: &mut Editor, k: i32| {
        ed.colour = [10 + k as u8, 20, 30];
        let (o, d) = down(k % N, k / N % N);
        ed.press(ed.aim(o, d).unwrap());
        ed.release();
    };
    ed.history.keep = (5, KEPT_BYTES);
    for k in 0..8 {
        paint(&mut ed, k);
    }
    assert_eq!((ed.history.past.len(), ed.steps()), (5, 8), "five kept, eight taken");
    for _ in 0..7 {
        ed.undo();
    }
    // The three oldest went: their colours stay.
    assert_eq!(ed.blocks[0].colour_at(IVec3::new(2, 0, N - 1)), Some([12, 20, 30]));
    assert_eq!(ed.blocks[0].colour_at(IVec3::new(3, 0, N - 1)), Some([128, 128, 128]), "the fourth was taken back");
    // By bytes: a step of this block holds two copies of it.
    let one = Step::Block(0, ed.blocks[0].clone(), ed.blocks[0].clone()).bytes();
    ed.history.keep = (KEPT_STEPS, 3 * one + one / 2);
    for k in 0..6 {
        paint(&mut ed, k);
    }
    assert_eq!(ed.history.past.len(), 3, "three steps' bytes");
    ed.history.keep = (KEPT_STEPS, 1);
    paint(&mut ed, 7);
    assert_eq!(ed.history.past.len(), 1, "the newest stays");
}
