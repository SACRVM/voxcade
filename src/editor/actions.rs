//! What the panels, the keys and the menus ask (`Action`), the keys they
//! carry, and doing it (`act`).

use super::*;

/// What a click on the panels (`panels.rs`) or a key does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Tool(Tool),
    /// A file changed on disk while a change of the editor's could not be
    /// written: the editor's written over it, or the file read anew.
    KeepMine,
    TakeFile,
    /// An item hidden in the editor or shown again (its eye); everything
    /// else hidden; everything shown.
    HideItem(Item),
    OnlyItem(Item),
    ShowAllItems,
    /// SLICE across an axis (the same again: off; None: off), its layer.
    Slice(Option<u8>),
    SliceAt(i32),
    /// CONTEXT: the level round a model shaped in place, shown or not.
    Context,
    /// The figure whole chosen (its row in the parts' list).
    WholeFigure,
    /// A figure's part: MOVE (its arrows) or ROTATE (its rings); its rest
    /// rotation about one axis (0 rotation, 1 pitch, 2 roll) set; a part
    /// added, removed, merged into another (the menu of them first),
    /// duplicated, its name to type, the others hidden.
    PartRotating(bool),
    PartRotation(u8, f32),
    /// POSE on or off; the chosen part's pose about one axis; RESET.
    Pose,
    PoseValue(u8, f32),
    PoseReset,
    /// MIRROR: a part's partner of its pair made or made anew.
    MirrorPart(usize),
    /// MOVE at a turned part: along the figure's axes (GLOBAL) or its own.
    MoveGlobal(bool),
    /// At the selection (SELECT): REMOVE, PAINT, FLIP and ROTATE about an
    /// axis, DUPLICATE, NEW PART, MOVE TO PART (the menu of the parts, at
    /// [0, 0] where the last menu was), COPY, PASTE, none selected.
    SelRemove,
    SelPaint,
    SelFlip(u8),
    SelRotate(u8),
    SelDuplicate,
    SelNewPart,
    SelToPart(usize),
    SelToPartMenu([f32; 2]),
    SelCopy,
    SelPaste,
    SelClear,
    /// REPLACE WITH HELD: every voxel of a color takes the held one.
    ReplaceColour([u8; 3]),
    /// BACKGROUND: the view's for this editor (None: the game's sky).
    Background(Option<[u8; 3]>),
    /// SELECT's mode (NEW, ADD, REMOVE); THROUGH on or off.
    SelectMode(super::super::model::selection::SelectMode),
    SelectThrough,
    /// A part's row dropped on another's: hung there (`usize::MAX`: on the
    /// figure whole's row, refused).
    Reparent(usize, usize),
    AddPart(Option<usize>),
    RemovePart(usize),
    MergeInto(usize, usize),
    MergeMenu(usize, [f32; 2]),
    DuplicatePart(usize),
    RenamePart(usize),
    HideOthers(usize),
    Undo,
    Redo,
    /// From a block back to the level.
    Back,
    /// What the block shown or held is to the game.
    KindTo(Kind),
    /// In the level the held block turned, in a block the block itself.
    Turn(Turn),
    /// The block shown or held copied, or taken out of the level.
    Clone,
    Delete,
    /// The level's block (its place) held; twice: opened.
    Hold(usize),
    /// The tileset shown to draw a new block out of, or hidden again.
    Choose,
    Tile(i32),
    Empty,
    Colour([u8; 3]),
    /// The level's tools to its blocks or its things.
    Layer(Layer),
    /// A kind of thing (its place in the game's list) held to put down.
    HoldKind(usize),
    /// A model's next frame (1) or the one before (-1).
    Frame(i32),
    /// A copy of the model's frame after it; the frame taken out.
    CopyFrame,
    DeleteFrame,
    /// The model's box cut down to its voxels.
    Trim,
    /// The model's frames playing on the panel, or held.
    Play,
    /// BOX's edges cut back, or not; every tool mirrored left to right, or
    /// not.
    Bevel,
    Symmetry,
    /// FIGURE: a part chosen in the list to shape and rig (its place among
    /// the parts; chosen again, none: `Back` too), left out of the picture
    /// or shown again, every part shown again. The chosen part's parent:
    /// the next part clicked (CHOOSE), or as the rules find it; its joint
    /// moved half a voxel along an axis (0 x, 1 y, 2 z; the sign the way),
    /// or where the rules find it.
    Part(usize),
    Hide(usize),
    ShowAll,
    PickParent,
    ParentWorkedOut,
    Joint(u8, i8),
    JointWorkedOut,
    /// The held colour's material: one of its words (`frames::WORDS`) set.
    Material(u8, f32),
    /// A colour no voxel has taken out of the model's palette.
    Forget([u8; 3]),
    /// The question a level turned off the grid asks: ALIGN ALL (each
    /// thing to its nearest way, the level opens) or DON'T OPEN (the
    /// window closes, the file untouched).
    AlignAll,
    DontOpen,
    /// Among the pieces: one chosen (in the list; None: nothing); ADD
    /// MODEL (the models listed, or no more), a model of the list set on
    /// the mouse (its place in `Editor::model_choices`), ADD LIGHT, ADD
    /// PLACE, ADD AREA; the chosen one turned a quarter (1 clockwise, -1
    /// back), copied onto the mouse, removed, renamed (its name being typed
    /// in the list; or no more), its model opened in the model editor; a
    /// number set (`Field`); a light more on a piece (the first at its
    /// model's top), one of them taken away, its colour; the piece solid or
    /// not; the level's daylight: its own (from the game's), the sun's or
    /// the sky's colour (`true`: the sun's), as the game's again; GAME
    /// VIEW; the lights in the view on or off.
    ChooseItem(Option<Item>),
    AddModel,
    Piece(usize),
    AddLight,
    AddPlace,
    AddArea,
    TurnItem(i8),
    CopyItem,
    RemoveItem,
    Rename(Option<Item>),
    EditModel,
    Set(room::Field, f64),
    AddPieceLight(i32),
    RemoveLight(u8),
    LightColour(u8, [u8; 3]),
    Solid(bool),
    DayOwn,
    DayColour(bool, [u8; 3]),
    DayAsGame,
    GameView,
    /// F1: the page of the keys shown, or hidden again.
    Keys,
    Lights,
    /// Among the pieces: a thing's menu opened (on the one in the list
    /// right-clicked, at egui's point), closed; ADD's menu on the top bar
    /// opened or closed; the chosen piece's lights on or off; a light of
    /// the chosen piece chosen (None: the piece); a light, place or area
    /// (0, 1, 2) set down at a spot of the floor, ADD MODEL's picker opened
    /// for a spot; the level's own settings in the inspector; a place
    /// facing a way; the chosen piece turned by quarters about an axis (0
    /// pitch, 1 roll, 2 round).
    OpenMenu(Item, [f32; 2]),
    CloseMenu,
    AddMenu,
    SwitchLight,
    ChooseLight(Option<u8>),
    AddAt(u8, IVec3),
    AddModelAt(IVec3),
    LevelSettings,
    FacePlace(crate::grid::Way),
    Tip(u8, i8),
    /// The view's own buttons: turned an eighth round (1 clockwise seen
    /// from above, -1 back), nearer (1) or further (-1).
    TurnView(i8),
    /// FRONT, SIDE or TOP (its place in `view::STRAIGHT`); pressed again,
    /// the free view as it was.
    StraightView(u8),
    ZoomView(i8),
    /// GRID in the voxel editor: the voxels' edges drawn on the faces, or
    /// not.
    Grid,
    /// The chosen one's handles: MOVE (false, its arrows) or ROTATE (true,
    /// its rings).
    HandleMode(bool),
    /// The voxel editor's and the level of blocks': a menu opened on a
    /// block, a frame or a part of the outliner (at egui's point); a block
    /// opened to shape; a frame shaped; a copy of a frame or a block, one
    /// taken out (by its place); the fresh box's depth; the chosen part's
    /// parent (None: as the rules find it) and its joint along an axis
    /// (as its fields show it); a voxel's colour taken up, a voxel erased
    /// (the menu on a voxel).
    BlockMenu(usize, [f32; 2]),
    FrameMenu(usize, [f32; 2]),
    PartMenu(usize, [f32; 2]),
    OpenBlock(usize),
    GoFrame(usize),
    CopyFrameOf(usize),
    DeleteFrameOf(usize),
    CloneOf(usize),
    DeleteOf(usize),
    Depth(i32),
    ParentTo(Option<u16>),
    JointTo(u8, f32),
    PickAt(IVec3),
    EraseAt(IVec3),
    /// A figure's chosen part moved to a place along an axis (its POSITION
    /// field).
    PartAt(u8, f32),
    /// The level's terrain (`land.rs`): added (ADD's menu) or taken out; a
    /// tool among its points; a shape for the chosen points (and PAINT's);
    /// a number; a material's condition (0 ABOVE, 1 BELOW, 2 STEEPER, 3
    /// FLATTER) on or off; a material's color (the materials' count:
    /// UNDER's); a material added, taken out, its row moved; SELECT's mode.
    AddTerrain,
    RemoveTerrain,
    LandTool(super::super::model::land::LandTool),
    LandShape(crate::terrain::Shape),
    LandSet(super::super::model::land::LandField, f64),
    LandCondition(u8, u8, bool),
    LandColour(u8, [u8; 3]),
    AddMaterial,
    RemoveMaterial(u8),
    MoveMaterial(u8, u8),
    LandMode(super::super::model::selection::SelectMode),
    /// The terrain's seed anew; its layers (a band more, one out, a row
    /// moved, a color); a material's tones (one set: material, tone,
    /// color; one more; the last out).
    Reroll,
    LandRow(super::super::model::land::LandRow),
    /// The terrain's scatter rules: one more, one out, a row moved; a
    /// rule's pieces set down or taken away; its model (its place in the
    /// game's models), how it turns, what it makes, a condition on or off.
    AddScatter,
    RemoveScatter(u8),
    MoveScatter(u8, u8),
    Scatter(u8),
    ClearScatter(u8),
    ScatterModel(u8, u16),
    ScatterRotate(u8, crate::terrain::Rotate),
    ScatterAs(u8, crate::terrain::ScatterAs),
    ScatterCondition(u8, u8, bool),
    AddLayer,
    RemoveLayer(u8),
    MoveLayer(u8, u8),
    LayerColour(u8, [u8; 3]),
    Tone(u8, u8, [u8; 3]),
    AddTone(u8),
    RemoveTone(u8),
}

/// The key an action answers to (the panels show it beside its word): a
/// tool the same on both levels. N is NEW in the level, DEEP PAINT in a
/// block; M is MIRROR among the level's blocks, MOVE among its things,
/// TRIM in a model (none is where another is).
pub fn key_of(action: Action) -> Option<char> {
    Some(match action {
        Action::Tool(Tool::Add) => 'A',
        Action::Tool(Tool::Remove) => 'E',
        Action::Tool(Tool::Paint) => 'P',
        Action::Tool(Tool::Deep) => 'N',
        Action::Tool(Tool::Move) => 'M',
        Action::Tool(Tool::Pick) => 'I',
        Action::Tool(Tool::Box) => 'Q',
        Action::Tool(Tool::Sphere) => 'S',
        Action::Tool(Tool::Select) => 'L',
        Action::Tool(Tool::Fill) => 'W',
        Action::Bevel => 'F',
        Action::Symmetry => 'Y',
        Action::Layer(Layer::Blocks) => 'K',
        Action::Layer(Layer::Things) => 'H',
        Action::Undo => 'U',
        Action::Redo => 'R',
        Action::Back => 'B',
        Action::Turn(t) if t == Turn::TURN => 'T',
        Action::Turn(t) if t == Turn::ROLL => 'O',
        Action::Turn(t) if t == Turn::MIRROR => 'M',
        Action::Clone => 'C',
        Action::Choose => 'N',
        Action::Frame(-1) => 'V',
        Action::Frame(_) => 'X',
        Action::AddModel => 'A',
        Action::AddLight => 'L',
        Action::AddPlace => 'P',
        Action::TurnItem(1) => 'T',
        Action::EditModel => 'E',
        Action::CopyItem | Action::CopyFrame => 'C',
        Action::GameView => 'G',
        Action::Trim => 'M',
        _ => return None,
    })
}

/// Does what a click on the panels or a key says.
pub fn act(ed: &mut Editor, view: &mut View, action: Action) {
    // Among the terrain's points, what is done to pieces brings them back.
    if ed.among_land() && matches!(action, Action::AddModel | Action::AddLight | Action::AddPlace | Action::AddArea | Action::Piece(_) | Action::ChooseItem(Some(_)) | Action::OpenMenu(..)) {
        ed.switch(Layer::Pieces);
    }
    let scope = ed.scope;
    match action {
        Action::Tool(tool) => ed.choose(tool),
        Action::Undo => ed.undo(),
        Action::Redo => ed.redo(),
        Action::Back => ed.back_to_level(),
        Action::AlignAll => ed.align_all(),
        // (The window closes: `EditorWindow::draw`.)
        Action::DontOpen => {}
        Action::KindTo(kind) => ed.set_kind(kind),
        Action::Turn(turn) => match ed.scope {
            Scope::Level => ed.turn(turn),
            Scope::Block(_) | Scope::Frame(_) => ed.turn_block(turn),
            Scope::Figure => {}
        },
        Action::Clone => {
            if let Err(e) = ed.clone_block() {
                ed.message = Some(format!("{e:#}"));
            }
        }
        Action::Delete => ed.delete_block(),
        Action::Hold(i) => {
            if ed.held.block as usize == i + 1 {
                ed.open_block(i);
            } else {
                ed.hold(i);
            }
        }
        Action::Choose => view.choosing = !view.choosing,
        Action::Tile(id) => {
            if let Err(e) = ed.new_block(Some(id)) {
                ed.message = Some(format!("{e:#}"));
            }
            view.choosing = false;
        }
        Action::Empty => {
            if let Err(e) = ed.new_block(None) {
                ed.message = Some(format!("{e:#}"));
            }
            view.choosing = false;
        }
        Action::Colour(c) => ed.colour = c,
        Action::Layer(layer) => {
            ed.switch(layer);
            view.choosing = false;
            (view.hover, view.spot, view.under) = (None, None, None);
        }
        Action::HoldKind(i) => ed.hold_kind(i),
        Action::Frame(by) => ed.step_frame(by),
        Action::CopyFrame => ed.copy_frame(),
        Action::DeleteFrame => ed.delete_frame(),
        Action::Trim => ed.trim(),
        Action::Play => view.playing = !view.playing,
        Action::Bevel => ed.toggle_bevel(),
        Action::Symmetry => ed.symmetry = !ed.symmetry,
        Action::Part(p) => ed.choose_part(p),
        Action::Hide(p) => ed.hide_part(p),
        Action::KeepMine => ed.keep_mine(),
        Action::HideItem(item) => ed.toggle_hidden(item),
        Action::OnlyItem(item) => ed.only(item),
        Action::ShowAllItems => ed.show_all(),
        Action::Slice(axis) => ed.set_slice(axis.map(usize::from)),
        Action::SliceAt(at) => ed.set_slice_at(at),
        Action::Context => ed.toggle_context(),
        Action::WholeFigure => ed.back_to_level(),
        Action::PartRotating(on) => ed.rigging.part_rotating = on,
        Action::Pose => ed.set_posing(!ed.rigging.posing),
        Action::PoseValue(axis, v) => ed.set_pose(axis as usize, v),
        Action::PoseReset => ed.reset_pose(),
        Action::SelRemove => ed.selection_step(super::super::model::selection::SelOp::Remove),
        Action::SelPaint => ed.selection_step(super::super::model::selection::SelOp::Paint(ed.colour)),
        Action::SelFlip(a) => ed.selection_step(super::super::model::selection::SelOp::Flip(a.min(2) as usize)),
        Action::SelRotate(a) => ed.selection_step(super::super::model::selection::SelOp::Rotate(a.min(2) as usize)),
        Action::SelDuplicate => ed.duplicate_selection(),
        Action::SelNewPart => {
            if let Some(name) = ed.selection_to_new_part() {
                // Its name to type in its row.
                view.renaming_part = ed.figure.as_ref().and_then(|f| f.parts().iter().position(|p| p.name == name));
                view.follow_origin(ed);
            }
        }
        Action::SelToPart(p) => ed.selection_to_part(p),
        Action::SelToPartMenu(at) => view.menu = Some(Menu { on: MenuOn::ToPart, at: if at == [0.0, 0.0] { view.menu.map_or(at, |m| m.at) } else { at } }),
        Action::SelCopy => ed.copy_selection(),
        Action::SelPaste => ed.paste(),
        Action::SelClear => ed.clear_selection(),
        Action::ReplaceColour(c) => ed.replace_colour(c),
        Action::SelectMode(m) => ed.selecting.select_mode = m,
        Action::SelectThrough => ed.selecting.select_through = !ed.selecting.select_through,
        Action::Reparent(p, q) => {
            if let Err(why) = ed.reparent(p, q) {
                ed.hint = Some(why);
            }
            view.follow_origin(ed);
        }
        Action::Background(c) => {
            view.backgrounds[(ed.scope != Scope::Level) as usize] = c;
            view.backgrounds_changed = true;
        }
        Action::MoveGlobal(on) => ed.rigging.move_global = on,
        Action::MirrorPart(p) => {
            match ed.cannot_mirror(p) {
                Some(why) => ed.hint = Some(why),
                None => ed.mirror_part(p),
            }
            view.follow_origin(ed);
        }
        Action::PartRotation(axis, v) => ed.set_part_rotation(axis as usize, v),
        Action::AddPart(on) => {
            if let Some(p) = on.filter(|&p| ed.figure.as_ref().is_some_and(|f| f.selected != Some(p))) {
                ed.choose_part(p);
            }
            ed.add_part();
            view.renaming_part = ed.figure.as_ref().and_then(|f| f.selected);
        }
        Action::RemovePart(p) => ed.remove_part(p),
        Action::MergeInto(src, into) => ed.merge_part(src, into),
        // (From a menu: where that menu was.)
        Action::MergeMenu(p, at) => view.menu = Some(Menu { on: MenuOn::MergeWith(p), at: if at == [0.0, 0.0] { view.menu.map_or(at, |m| m.at) } else { at } }),
        Action::DuplicatePart(p) => ed.duplicate_part(p),
        // (usize::MAX: the typing left.)
        Action::RenamePart(p) => (view.renaming_part, view.rename_problem) = ((p != usize::MAX).then_some(p), None),
        Action::HideOthers(p) => ed.hide_other_parts(p),
        Action::TakeFile => ed.take_file(),
        Action::ShowAll => ed.show_all_parts(),
        Action::PickParent => ed.rigging.picking_parent = !ed.rigging.picking_parent,
        Action::ParentWorkedOut => ed.set_parent(None),
        Action::Joint(axis, way) => {
            let mut by = Vec3::ZERO;
            by[axis.min(2) as usize] = 0.5 * way.signum() as f32;
            ed.move_joint(by);
        }
        Action::JointWorkedOut => ed.set_joint(None),
        Action::Material(which, v) => {
            if let Some(m) = ed.material() {
                ed.set_material(super::super::frames::with_word(m, which as usize, v));
            }
        }
        Action::Forget(c) => ed.forget_colour(c),
        Action::ChooseItem(item) => {
            ed.choose_item(item);
            (view.renaming, view.menu, view.level_settings) = (None, None, false);
            view.bring_into_view(ed);
        }
        Action::AddModel => {
            view.picker = !view.picker;
            (view.menu, view.picker_spot) = (None, None);
            ed.drop_hanging();
        }
        Action::Piece(i) => {
            if let Some(file) = ed.model_choices().get(i) {
                ed.add_piece(file);
                // Picked for a spot of the floor (a right click's): set
                // down there at once.
                if let Some(spot) = view.picker_spot.take() {
                    ed.hang_at(Some(spot));
                    ed.set_down();
                }
            }
            view.picker = false;
        }
        Action::OpenMenu(item, at) => {
            if ed.pieces.chosen != Some(item) {
                ed.choose_item(Some(item));
            }
            view.menu = Some(Menu { on: MenuOn::Item(item), at });
        }
        Action::CloseMenu => view.menu = None,
        Action::AddMenu => view.menu = if view.menu.is_some_and(|m| m.on == MenuOn::Add) { None } else { Some(Menu { on: MenuOn::Add, at: [0.0; 2] }) },
        Action::SwitchLight => ed.switch_light(),
        Action::ChooseLight(k) => ed.choose_light(k.map(usize::from)),
        Action::AddAt(what, spot) => {
            match what {
                0 => ed.add_light(),
                1 => ed.add_place(),
                _ => ed.add_area(),
            }
            if let Some(h) = ed.pieces.hanging.clone() {
                ed.add_at(h, spot);
            }
        }
        Action::AddModelAt(spot) => {
            ed.drop_hanging();
            (view.picker, view.picker_spot) = (true, Some(spot));
        }
        Action::LevelSettings => view.level_settings = !view.level_settings,
        Action::FacePlace(way) => ed.face_place(way),
        Action::HandleMode(turn) => view.turning = if turn { ed.pieces.chosen } else { None },
        Action::TurnView(by) => {
            let orbit = view.orbit(ed.scope);
            orbit.yaw = (orbit.yaw + 45.0 * by as f32).rem_euclid(360.0);
            view.leave_game_view(ed.scope);
        }
        Action::StraightView(k) => {
            view.straight_view(ed.scope, k as usize);
            view.leave_game_view(ed.scope);
        }
        Action::ZoomView(by) => {
            view.orbit(ed.scope).zoom_by(by as f32 * 2.0);
            view.leave_game_view(ed.scope);
        }
        Action::Grid => view.grid = !view.grid,
        Action::BlockMenu(i, at) => {
            if ed.scope == Scope::Level {
                ed.hold(i);
            }
            view.menu = Some(Menu { on: MenuOn::Block(i), at });
        }
        Action::FrameMenu(f, at) => view.menu = Some(Menu { on: MenuOn::Frame(f), at }),
        Action::PartMenu(p, at) => view.menu = Some(Menu { on: MenuOn::Part(p), at }),
        Action::OpenBlock(i) => {
            if ed.scope != Scope::Level {
                ed.back_to_level();
            }
            ed.hold(i);
            ed.open_block(i);
            view.choosing = false;
        }
        Action::GoFrame(f) => {
            if let Scope::Frame(now) = ed.scope {
                ed.step_frame(f as i32 - now as i32);
            }
        }
        Action::CopyFrameOf(f) => {
            if let Scope::Frame(now) = ed.scope {
                ed.step_frame(f as i32 - now as i32);
            }
            ed.copy_frame();
        }
        Action::DeleteFrameOf(f) => {
            if let Scope::Frame(now) = ed.scope {
                ed.step_frame(f as i32 - now as i32);
            }
            ed.delete_frame();
        }
        Action::CloneOf(i) => {
            ed.hold(i);
            if let Err(e) = ed.clone_block() {
                ed.message = Some(format!("{e:#}"));
            }
        }
        Action::DeleteOf(i) => {
            ed.hold(i);
            ed.delete_block();
        }
        Action::Depth(d) => {
            let now = ed.forms.fresh.filter(|_| ed.forms.drawing.is_none()).map_or(ed.forms.depth, |f| {
                let a = (0..3).max_by_key(|&a| f.way[a].abs()).unwrap_or(2);
                f.size[a]
            });
            ed.deepen(d.clamp(1, 256) - now);
        }
        Action::ParentTo(p) => ed.set_parent(p.map(usize::from)),
        Action::JointTo(axis, v) => {
            if let Some(figure) = &ed.figure
                && let Some(p) = figure.selected
            {
                // (From the pivot, as the file and the field have it.)
                let mut at = figure.file_joint(p);
                at[axis.min(2) as usize] = (v * 10.0).round() / 10.0;
                ed.set_joint(Some(at));
            }
        }
        // POSITION typed: its `at` and `offset` (4.3: at 4, offset 0.3);
        // in POSE the pose's offset.
        Action::PartAt(axis, v) => {
            let a = axis.min(2) as usize;
            let v = (v * 10.0).round() / 10.0;
            if ed.rigging.posing {
                if let (Some(now), Some(posed)) = (ed.part_place(), ed.chosen_posed()) {
                    ed.set_pose_offset(a, v - (now[a] - posed.offset[a]));
                }
            } else if let Some(now) = ed.part_place() {
                let mut by = Vec3::ZERO;
                by[a] = v - now[a];
                if by != Vec3::ZERO {
                    ed.move_part_by(by);
                }
            }
        }
        Action::PickAt(cell) => ed.pick_at(cell),
        Action::EraseAt(cell) => ed.erase_at(cell),
        Action::Tip(axis, quarters) => {
            ed.tip(axis as usize, quarters as f64 * 90.0);
        }
        Action::AddLight => ed.add_light(),
        Action::AddPlace => ed.add_place(),
        Action::AddArea => ed.add_area(),
        Action::AddTerrain => {
            ed.add_terrain();
            view.menu = None;
        }
        Action::RemoveTerrain => ed.remove_terrain(),
        Action::LandTool(t) => {
            ed.release();
            ed.land.tool = t;
        }
        Action::LandShape(s) => ed.land_shape(s),
        Action::LandSet(f, v) => ed.land_set(f, v),
        Action::LandCondition(k, which, on) => ed.land_condition(k, which, on),
        Action::LandColour(k, c) => ed.land_colour(k, c),
        Action::AddMaterial => ed.add_material(),
        Action::RemoveMaterial(k) => ed.remove_material(k),
        Action::MoveMaterial(from, to) => ed.move_material(from, to),
        Action::LandMode(m) => ed.land.mode = m,
        Action::Reroll => ed.reroll(),
        Action::LandRow(r) => ed.land.row = Some(r),
        Action::AddScatter => ed.add_scatter(),
        Action::RemoveScatter(k) => ed.remove_scatter(k as usize),
        Action::MoveScatter(a, b) => ed.move_scatter(a as usize, b as usize),
        Action::Scatter(k) => ed.scatter_pieces(k as usize),
        Action::ClearScatter(k) => ed.clear_scatter(k as usize),
        Action::ScatterModel(k, m) => {
            if let Some(model) = ed.model_choices().get(m as usize).cloned() {
                ed.scatter_model(k as usize, model);
            }
        }
        Action::ScatterRotate(k, r) => ed.scatter_rotate(k as usize, r),
        Action::ScatterAs(k, a) => ed.scatter_as(k as usize, a),
        Action::ScatterCondition(k, w, on) => ed.scatter_condition(k as usize, w, on),
        Action::AddLayer => ed.add_layer(),
        Action::RemoveLayer(k) => ed.remove_layer(k),
        Action::MoveLayer(a, b) => ed.move_layer(a, b),
        Action::LayerColour(k, c) => ed.layer_colour(k, c),
        Action::Tone(k, n, c) => ed.tone(k, n, c),
        Action::AddTone(k) => ed.add_tone(k),
        Action::RemoveTone(k) => ed.remove_tone(k),
        Action::TurnItem(by) => ed.turn_item(by as i32),
        Action::CopyItem => ed.copy_item(),
        Action::RemoveItem => ed.remove_item(),
        Action::Rename(item) => (view.renaming, view.rename_problem) = (item, None),
        Action::EditModel => ed.edit_model(),
        Action::Set(field, v) => ed.set_field(field, v),
        Action::AddPieceLight(top) => ed.add_piece_light(top),
        Action::RemoveLight(k) => ed.remove_piece_light(k as usize),
        Action::LightColour(k, [r, g, b]) => ed.set_colour(k as usize, &format!("#{r:02x}{g:02x}{b:02x}")),
        Action::Solid(on) => ed.set_solid(on),
        Action::DayOwn => ed.day_own(),
        Action::DayColour(sun, [r, g, b]) => ed.set_day_colour(sun, &format!("#{r:02x}{g:02x}{b:02x}")),
        Action::DayAsGame => ed.day_as_game(),
        // GAME VIEW on and off: off, the camera as it was before.
        // A figure's: the figure whole as the game draws it, from its
        // camera's angle, framed to fill the view; on again, the game's
        // angle afresh.
        Action::GameView if ed.scope != Scope::Level => match view.before_game.take() {
            Some(orbit) => {
                view.block = orbit;
                view.follow_origin(ed);
            }
            None if ed.figure.is_some() => {
                ed.back_to_level();
                ed.follow_figure();
                view.follow_origin(ed);
                view.before_game = Some(view.block);
                view.figure_game_view(ed);
                view.follow_origin(ed);
            }
            None => {}
        },
        Action::Keys => view.keys_shown = !view.keys_shown,
        Action::GameView => match view.before_game.take() {
            Some(orbit) => view.level = orbit,
            None => {
                view.before_game = Some(view.level);
                view.game_view(ed);
            }
        },
        Action::Lights => view.lights = !view.lights,
    }
    // What hangs on the mouse goes when the picker opens or the level is
    // left; a name being typed when another is chosen; a menu once one of
    // its entries is taken.
    if view.picker || !ed.among_pieces() {
        ed.drop_hanging();
    }
    if !matches!(action, Action::OpenMenu(..) | Action::AddMenu | Action::BlockMenu(..) | Action::FrameMenu(..) | Action::PartMenu(..) | Action::MergeMenu(..) | Action::SelToPartMenu(..)) {
        view.menu = None;
    }
    // The figure made again at once (a part chosen, its rig set), and the
    // view kept on it.
    ed.follow_figure();
    if scope != ed.scope {
        view.hover = None;
    }
    view.follow_origin(ed);
}
