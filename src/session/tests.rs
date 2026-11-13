//! The session's tests.

use super::*;
use crate::shooter::Controls as Input;
use crate::shooter::Shooter;
use std::collections::HashSet;

/// A game on open flat ground, without model files.
fn flat_game(size: i32) -> Session<Shooter> {
    game_on(crate::sim::tests::flat_world(IVec3::new(size, size, 16)), GameConfig::default())
}

/// A game in `world` with `cfg`, without model files.
fn game_on(world: World, cfg: GameConfig) -> Session<Shooter> {
    let player = VoxModel::new(IVec3::new(5, 4, 10), vec![Vec::new()], vec![Vec::new()]);
    let models = Models { actors: vec![(ModelSpec { name: "player".into(), file: String::new(), shaped: true }, player)], props: HashMap::new() };
    let game = Shooter::new(world, &cfg, crate::shooter::tests::shapes(), &[]);
    Session {
        dir: PathBuf::new(),
        files: Files::disk(),
        cfg,
        game,
        models,
        level_files: Vec::new(),
        piece_models: Vec::new(),
        fades: false,
        fade: None,
        flyby_from: 0.0,
        ticked: false,
        sounds: SoundBank::default(),
        song_titles: Vec::new(),
        songs: Vec::new(),
        cue: None,
        music_changed: false,
        fade_clock: false,
        crossfade: None,
        fade_cut: false,
        song: 0,
        options: Options::default(),
        font: None,
        bank: Default::default(),
        logo: Vec::new(),
        logo_model: None,
        props_ground: Vec::new(),
        actions: crate::shooter::ACTIONS.to_vec(),
        controls: Controls::nobody(&crate::shooter::ACTIONS),
        seats: Vec::new(),
        pending: Vec::new(),
        played: Tick::default(),
        inputs: Default::default(),
        clock: 0,
        local: Default::default(),
        problem: None,
        drawn_sun: Default::default(),
        seen_faces: Default::default(),
        hud_kept: Default::default(),
        limits: Default::default(),
        touched: false,
            problem_shown: true,
        applied: false,
        slots: Slots::default(),
        slots_at_start: BTreeMap::new(),
        started_from: None,
        scenario: None,
        evals: Vec::new(),
    }
}

/// While running steadily the player must keep its place on screen: the
/// drawn player and the camera move in the same whole-voxel steps. If they
/// are rounded separately, the player flickers between two spots.
#[test]
fn running_player_keeps_still_on_screen() {
    for run in [Input { left: true, ..Input::default() }, Input { up: true, left: true, ..Input::default() }] {
        let mut game = flat_game(512);
        let mut offsets = Vec::new();
        for tick in 0..150 {
            game.game.step(&run);
            if tick < 60 {
                continue; // let speed and camera lag settle
            }
            for alpha in [0.0, 0.3, 0.6, 0.9] {
                let offset = game.player_origin(alpha) - game.view_target(alpha);
                offsets.push((offset * 1000.0).round().as_ivec3());
            }
        }
        let changes = offsets.windows(2).filter(|pair| pair[0] != pair[1]).count();
        println!("{run:?}: player jumps against the camera {changes} times in {} frames", offsets.len());
        assert!(changes <= 2, "player shivers against the camera {changes} times while running");
    }
}

/// Every game's HUD is drawn without the faces a neighbour covers:
/// they showed as thin dark lines through the platformer's hearts at
/// some window sizes (1440 by 810 among them).
#[test]
fn a_huds_inner_faces_are_hidden() {
    let game = Session::<crate::Voxcade>::load(Path::new("games/voxkeep")).unwrap();
    let frame = game.compose(0.0, 1440.0 / 810.0, Scene::Play, None);
    let cell = |i: &Instance| Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
    let taken: HashSet<IVec3> = frame.ui.iter().map(cell).collect();
    let next_to = frame.ui.iter().filter(|i| taken.contains(&(cell(i) + IVec3::X))).collect::<Vec<_>>();
    assert!(!next_to.is_empty(), "the hearts have voxels side by side");
    assert!(next_to.iter().all(|i| i.fx[2] & 1 == 1), "their +x faces hidden");
}

/// The HUD the UI pass draws is worked out once while it stays the same,
/// and comes out as worked out afresh: the same cubes, faces hidden, the
/// unseen left out; a HUD changed, or seen from elsewhere, anew.
#[test]
fn an_unchanged_hud_is_kept_and_comes_out_the_same() {
    let game = flat_game(32);
    let cube = |x: i32, z: i32| Instance { pos_size: [(x % 8) as f32 + 0.5, (x / 8) as f32 + 0.5, z as f32 + 0.5, 1.0], color: [9, 9, 9, 255], fx: [0, 255, 0, 0], shade: [0; 4], turn: [0; 4] };
    let fresh = |raw: &[Instance], eye: Vec3| {
        let mut v = raw.to_vec();
        crate::render::hide_inner_faces(&mut v);
        crate::render::leave_out_unseen(&mut v, eye);
        bytemuck::cast_slice::<Instance, u8>(&v).to_vec()
    };
    let bytes = |v: &[Instance]| bytemuck::cast_slice::<Instance, u8>(v).to_vec();
    // A plate 8 by 4, two deep (cell k: x = k % 8, y = k / 8).
    let raw: Vec<Instance> = (0..2).flat_map(|z| (0..32).map(move |k| cube(k, z))).collect();
    let eye = Vec3::new(100.0, 50.0, 300.0);
    let first = game.hud_cubes(raw.clone(), eye);
    assert!(first.len() < raw.len(), "the back layer left out");
    assert_eq!(bytes(&first), fresh(&raw, eye));
    assert_eq!(bytes(&game.hud_cubes(raw.clone(), eye)), fresh(&raw, eye), "kept");
    let mut more = raw.clone();
    more.push(cube(32, 1));
    assert_eq!(bytes(&game.hud_cubes(more.clone(), eye)), fresh(&more, eye), "changed: anew");
    let elsewhere = Vec3::new(-100.0, 50.0, 300.0);
    assert_eq!(bytes(&game.hud_cubes(more.clone(), elsewhere)), fresh(&more, elsewhere), "seen from elsewhere: anew");
}

/// The line round the models is the game's: none unless it sets
/// `c.effects.outline` (voxels thick) and, if it likes, its colour.
#[test]
fn the_outline_round_models_is_the_games() {
    let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
    assert_eq!(game.frame(1.0, 16.0 / 9.0).outline[3], 0.0, "none by default");
    (game.cfg.effects.outline, game.cfg.effects.outline_color) = (1.0, [0.1, 0.2, 0.3]);
    assert_eq!(game.frame(1.0, 16.0 / 9.0).outline, [0.1, 0.2, 0.3, 1.0]);
}

/// A still (the model sheet) draws the one model alone, by its name or
/// its file, from the side asked, on its floor; no other model.
#[test]
fn a_still_draws_one_model_alone() {
    let game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
    let front = game.still("player", 0, IVec2::NEG_Y, Quat::IDENTITY, &|_| PartPose::REST, 40.0, 0.75).unwrap();
    assert_eq!(front.models.len(), 1, "the player alone");
    assert!(front.instances.len() >= 100, "on a floor: {}", front.instances.len());
    let by_file = game.still(&game.cfg.player.model, 0, IVec2::NEG_Y, Quat::IDENTITY, &|_| PartPose::REST, 40.0, 0.75).unwrap();
    assert_eq!(by_file.models[0].voxels, front.models[0].voxels);
    let turned = game.still("player", 0, IVec2::NEG_Y, turning(-45.0, 0.0, 0.0), &|_| PartPose::REST, 40.0, 0.75).unwrap();
    assert!(turned.models[0].rot != Quat::IDENTITY);
    assert!(game.still("nothing.vox", 0, IVec2::Y, Quat::IDENTITY, &|_| PartPose::REST, 40.0, 0.75).is_none());
}

/// Standing still on raised ground must draw the same frame every time,
/// whatever the frame rate. Rounding in the interpolation used to drop the
/// player a voxel in some frames (only above height 4, so only off the floor).
#[test]
fn standing_on_the_plaza_draws_the_same_frame() {
    let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
    for _ in 0..90 {
        game.game.step(&Input { up: true, ..Input::default() });
    }
    // Long enough for the dust of the last steps to settle.
    for _ in 0..60 {
        game.game.step(&Input::default());
    }
    assert!(game.game.player.body.pos.z > 4.0, "the player should stand on the plaza");
    assert!(game.game.particles.is_empty(), "dust still in the air");

    // The window's loop at 200 frames per second, 60 ticks per second.
    let mut acc = 0.0f32;
    let mut last: Option<Vec<[f32; 4]>> = None;
    let mut changes = 0;
    for _ in 0..400 {
        acc += 1.0 / 200.0;
        while acc >= crate::sim::DT {
            game.game.step(&Input::default());
            acc -= crate::sim::DT;
        }
        let drawn: Vec<_> = game.frame(acc / crate::sim::DT, 16.0 / 9.0).cubes().iter().map(|i| i.pos_size).collect();
        if last.as_ref().is_some_and(|l| *l != drawn) {
            changes += 1;
        }
        last = Some(drawn);
    }
    assert_eq!(changes, 0, "the standing player flickers");
}

/// At the edge of the world the camera behaves as everywhere else: it
/// stays on the player and never changes height (no tilt, no zoom).
#[test]
fn the_edge_changes_nothing() {
    let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
    let height = game.view_target(0.0).z;
    for _ in 0..240 {
        game.game.step(&Input { down: true, ..Input::default() });
        for alpha in [0.0, 0.3, 0.6, 0.9] {
            let view = game.view_target(alpha);
            let feet = game.player_origin(alpha);
            assert_eq!(view.z, height, "the camera changed height");
            assert_eq!((view.x, view.y), (feet.x, feet.y), "the camera left the player");
        }
    }
    assert!(game.game.player.body.pos.y < 10.0, "the player should have reached the south wall");
}

/// When the player stops, the view must stop with it: no catching-up
/// steps afterwards, which read as hitches.
#[test]
fn stopping_stops_the_view() {
    let mut game = flat_game(512);
    for _ in 0..60 {
        game.game.step(&Input { left: true, ..Input::default() });
    }

    let mut late_moves = 0;
    let mut last = (game.player_origin(1.0), game.view_target(1.0));
    let mut player_still_for = 0;
    for _ in 0..60 {
        game.game.step(&Input::default());
        for alpha in [0.0, 0.3, 0.6, 0.9] {
            let now = (game.player_origin(alpha), game.view_target(alpha));
            if now.0 == last.0 {
                player_still_for += 1;
                if now.1 != last.1 {
                    late_moves += 1;
                }
            } else {
                player_still_for = 0;
            }
            last = now;
        }
    }
    println!("view moves {late_moves} times after the player stood still");
    assert!(player_still_for > 100, "the player should have come to rest");
    assert_eq!(late_moves, 0, "the view keeps moving after the player stopped");
}

/// Jumping in place must leave the view where it is: the camera height
/// ignores jumps, so nothing may round it up and down while airborne.
#[test]
fn jumping_keeps_the_view_still() {
    let mut game = flat_game(256);
    for _ in 0..60 {
        game.game.step(&Input::default());
    }

    let mut views = Vec::new();
    for tick in 0..60 {
        game.game.step(&Input { jump: tick < 20, ..Input::default() });
        for alpha in [0.0, 0.3, 0.6, 0.9] {
            views.push((game.view_target(alpha) * 1000.0).round().as_ivec3());
        }
    }
    let changes = views.windows(2).filter(|pair| pair[0] != pair[1]).count();
    println!("view moves {changes} times during a jump");
    assert_eq!(changes, 0, "the view shakes while jumping");
}

/// Stairs rising eastwards from x 130, a voxel every 4, seen from the
/// side by a camera that follows the height within `band`.
fn stairs_game(band: crate::config::FollowHeight) -> Session<Shooter> {
    let mut world = crate::sim::tests::flat_world(IVec3::new(512, 64, 128));
    for x in 130..450 {
        for z in 1..=(x - 130) / 4 {
            for y in 0..64 {
                world.set_raw(IVec3::new(x, y, z), 1);
            }
        }
    }
    let mut cfg = GameConfig::default();
    cfg.player.spawn = [100.0, 32.0];
    cfg.camera.pitch = 15.0;
    cfg.camera.follow_height = Some(band);
    game_on(world, cfg)
}

/// Climbing, a side view's camera follows the feet up in whole voxels,
/// keeping them within its band and never moving back and forth.
#[test]
fn the_side_view_follows_a_climb_without_shaking() {
    let band = crate::config::FollowHeight { below: 6.0, above: 2.0 };
    let mut game = stairs_game(band);
    let run = Input { right: true, ..Input::default() };
    let mut heights = Vec::new();
    for _ in 0..400 {
        game.game.step(&run);
        for alpha in [0.0, 0.3, 0.6, 0.9] {
            let view = game.view_target(alpha);
            let feet = game.player_origin(alpha);
            assert!(view.z == view.z.round(), "the view's height {} is not a whole voxel", view.z);
            let offset = view.z - feet.z;
            assert!((-band.above..=band.below).contains(&offset), "the feet left the band: view {} feet {}", view.z, feet.z);
            heights.push(view.z);
        }
    }
    assert!(heights.last().unwrap() - heights[0] > 40.0, "the view did not climb: {} to {}", heights[0], heights.last().unwrap());
    assert!(heights.windows(2).all(|w| w[1] >= w[0]), "the view went down while climbing");
}

/// A jump that stays within the band keeps a side view still.
#[test]
fn a_jump_inside_the_zone_keeps_the_view_still() {
    let mut game = stairs_game(crate::config::FollowHeight { below: 30.0, above: 60.0 });
    for _ in 0..60 {
        game.game.step(&Input::default());
    }
    let mut views = Vec::new();
    for tick in 0..60 {
        game.game.step(&Input { jump: tick < 20, ..Input::default() });
        for alpha in [0.0, 0.3, 0.6, 0.9] {
            views.push((game.view_target(alpha) * 1000.0).round().as_ivec3());
        }
    }
    assert!(views.windows(2).all(|pair| pair[0] == pair[1]), "the view moves during a jump inside its band");
}

/// With `stop_at_edges` the view stops at the stage's end: the player
/// walks on, the view stays; away from the end it follows again.
#[test]
fn the_view_stops_at_the_level_edges() {
    let mut cfg = GameConfig::default();
    cfg.camera.pitch = 15.0;
    cfg.camera.view_width = 160.0;
    cfg.camera.stop_at_edges = true;
    let mut game = game_on(crate::sim::tests::flat_world(IVec3::new(512, 64, 200)), cfg);
    let cam = game.cfg.camera.clone();
    let (half, down, _) = camera::view_extent(&cam, 16.0 / 9.0);
    let at = |game: &mut Session<Shooter>, x: f32| {
        game.game.player.body.pos.x = x;
        game.game.player.body.prev.x = x;
        game.within_stage(game.view_target(1.0), &cam, 16.0 / 9.0)
    };
    let (near, nearer) = (at(&mut game, 30.0), at(&mut game, 10.0));
    assert_eq!(near, nearer, "the view moved past the left edge");
    assert!((near.x - half).abs() <= 1.0, "the view stops at {} instead of {half}", near.x);
    assert!(near.z >= down - 1.0, "the view shows below the floor");
    let middle = at(&mut game, 256.0);
    assert_eq!(middle.x, 256.0, "away from the edges the view follows");
    let far = at(&mut game, 505.0);
    assert!((far.x - (512.0 - half)).abs() <= 1.0, "the view passed the right edge: {}", far.x);
}

/// Seen from above, `stop_at_edges` keeps the ground the screen shows on
/// the stage across and in depth: at a corner the view stops, the
/// corner rays meet the ground inside; in the middle it follows.
#[test]
fn a_view_from_above_stops_at_the_level_edges() {
    let mut cfg = GameConfig::default();
    cfg.camera.pitch = 50.0;
    cfg.camera.view_width = 320.0;
    cfg.camera.stop_at_edges = true;
    let mut game = game_on(crate::sim::tests::flat_world(IVec3::new(1088, 672, 32)), cfg);
    let cam = game.cfg.camera.clone();
    let (near, far) = camera::ground_extent(&cam, 16.0 / 9.0).expect("a view from above");
    let mut at = |x: f32, y: f32| {
        game.game.player.body.pos = Vec3::new(x, y, 16.0);
        game.game.player.body.prev = game.game.player.body.pos;
        game.game.sim.cam_target = Vec3::new(x, y, 16.0);
        game.game.sim.cam_prev = game.game.sim.cam_target;
        game.within_stage(game.view_target(1.0), &cam, 16.0 / 9.0)
    };
    for (x, y) in [(5.0, 5.0), (1080.0, 668.0), (5.0, 668.0), (1080.0, 5.0)] {
        let t = at(x, y);
        assert!(t.x + near.x >= -1.0 && t.x + far.x <= 1089.0, "the view shows beyond the sides at {t}");
        assert!(t.y + near.y >= -1.0 && t.y + far.y <= 673.0, "the view shows beyond the ends at {t}");
    }
    let middle = at(544.0, 336.0);
    assert_eq!((middle.x, middle.y), (544.0, 336.0), "away from the edges the view follows");
}

/// Seen from above the shadow box is what it always was, whatever the
/// stage; seen from the side it is fitted to the stage and stays small.
#[test]
fn shadows_fit_the_view_from_above_and_from_the_side() {
    let world = Vec3::new(512.0, 512.0, 64.0);
    let sun = Vec3::new(0.45, 0.6, -1.0);
    for pitch in [35.0, 58.0, 72.0] {
        let cam = crate::config::CameraConfig { pitch, ..Default::default() };
        let a = camera::light_view_proj(&cam, Vec3::new(200.0, 200.0, 8.0), 16.0 / 9.0, sun, world, (Vec3::ZERO, world), 2048);
        let b = camera::light_view_proj(&cam, Vec3::new(200.0, 200.0, 8.0), 16.0 / 9.0, sun, world, (Vec3::ONE, Vec3::splat(40.0)), 2048);
        assert_eq!(a, b, "the stage changed the shadows of a view from above at pitch {pitch}");
    }
    let side = crate::config::CameraConfig { pitch: 15.0, view_width: 400.0, ..Default::default() };
    let stage = (Vec3::new(0.0, 100.0, 0.0), Vec3::new(2000.0, 164.0, 800.0));
    let big = Vec3::new(2000.0, 1000.0, 800.0);
    let (_, depth) = camera::light_view_proj(&side, Vec3::new(600.0, 132.0, 300.0), 16.0 / 9.0, sun, big, stage, 2048);
    assert!(depth < 1500.0, "the side view's shadow box is {depth} voxels deep");
}

/// A game that moves its sun (BunnyFarm's day) has its faces remember
/// their shadow, and its shadows follow the sun as it is, picture by
/// picture (the shader's `remembered`: a face at a shadow's edge
/// changes once, as the edge passes); a game whose sun stands still
/// draws as it always did.
#[test]
fn a_moving_sun_has_the_faces_remember_their_shadow() {
    let farm = Session::<crate::Voxcade>::load(Path::new("games/bunnyfarm")).unwrap();
    assert!(farm.frame(1.0, 16.0 / 9.0).remember, "BunnyFarm moves its sun");
    let keep = Session::<crate::Voxcade>::load(Path::new("games/voxkeep")).unwrap();
    assert!(!keep.frame(1.0, 16.0 / 9.0).remember, "Voxkeep's sun stands still");
    let cam = crate::config::CameraConfig::default();
    let world = Vec3::new(512.0, 512.0, 64.0);
    let at = |hour: f32| {
        let a = (hour - 6.0) / 14.0 * std::f32::consts::PI;
        let sun = Vec3::new(-a.cos() * 0.9, 0.55, -(0.25 + a.sin() * 0.85));
        camera::light_view_proj(&cam, Vec3::new(200.0, 200.0, 8.0), 16.0 / 9.0, sun, world, (Vec3::ZERO, world), 2048).0
    };
    assert_ne!(at(7.0), at(7.0 + 1.0 / 600.0), "the shadows follow the sun picture by picture");
}

/// On the GPU: posts' shadows creep over the floor as the sun goes
/// round slowly, a little each picture (too little for a shadow to
/// pass a face wholly). Remembering, no face of the floor turns lit
/// and back (or shaded and back): each changes once at most as an
/// edge passes. Without the memory the shadow map's texels, laid anew
/// every picture, flip faces at the edges back and forth (what the
/// user saw in BunnyFarm). A game whose sun stands still draws with
/// the plain pipelines, the memory never made.
#[test]
fn a_moving_sun_turns_each_face_once() {
    let mut world = crate::sim::tests::flat_world(IVec3::new(96, 96, 24));
    for (x, y, h) in [(30, 40, 14), (50, 30, 10), (60, 60, 18), (40, 66, 12), (70, 40, 16)] {
        for z in 1..=h {
            for (dx, dy) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)] {
                world.set_raw(IVec3::new(x + dx, y + dy, z), 1);
            }
        }
    }
    let mut cfg = GameConfig::default();
    cfg.light.shadow_style = crate::config::ShadowStyle::Voxel;
    cfg.camera.view_width = 100.0;
    let mut game = game_on(world, cfg);
    game.game.sim_mut().cam_target = Vec3::new(48.0, 48.0, 1.0);
    game.game.sim_mut().cam_prev = Vec3::new(48.0, 48.0, 1.0);
    let (w, h) = (480u32, 270u32);
    const PICTURES: usize = 48;
    // Each picture's light at each pixel (r + g + b), remembering or not.
    let mut looks = |remember: bool| -> (Vec<Vec<i32>>, bool) {
        let mut canvas = crate::headless::Canvas::new(w, h, false).unwrap();
        canvas.mesh_world(game.world_mut(), &[true; 6]);
        let mut out = Vec::new();
        for k in 0..PICTURES {
            let a = 0.6 + k as f32 * 0.0015;
            game.cfg.light.sun = [a.cos() * 0.7, a.sin() * 0.7, -1.0];
            let mut f = game.frame(1.0, w as f32 / h as f32);
            f.remember = remember;
            let pixels = canvas.draw(&f).unwrap();
            out.push(pixels.chunks(4).map(|p| p[0] as i32 + p[1] as i32 + p[2] as i32).collect());
        }
        (out, canvas.remembers())
    };
    // A pixel that grows clearly lighter and later clearly darker (or
    // the other way) went back.
    let flips = |looks: &[Vec<i32>]| {
        (0..looks[0].len())
            .filter(|&i| {
                let mut way = 0;
                for k in 1..looks.len() {
                    let d = looks[k][i] - looks[k - 1][i];
                    if d.abs() > 40 {
                        if way != 0 && d.signum() != way {
                            return true;
                        }
                        way = d.signum();
                    }
                }
                false
            })
            .count()
    };
    let (plain, made) = looks(false);
    assert!(!made, "a game whose sun stands still never makes the memory");
    let (kept, made) = looks(true);
    assert!(made, "the GPU keeps the faces' memory");
    let (plain, kept) = (flips(&plain), flips(&kept));
    eprintln!("faces flipping back and forth: {plain} pixels without the memory, {kept} with it");
    assert!(plain > 0, "the scene shows the texels' jitter without the memory (else the test sees nothing)");
    assert_eq!(kept, 0, "with the memory no face goes back ({plain} pixels did without it)");
}

/// Saving any file a map level is built from, a prefab for one, rebuilds
/// the level; other files leave it alone.
#[test]
fn saving_a_prefab_rebuilds_the_level() {
    let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let prefab = game.reload(Path::new("games/moreblood/prefabs/palm_0.vvx")).unwrap();
    assert!(matches!(prefab, Reloaded::World), "a changed prefab must rebuild the level");
    let other = game.reload(Path::new("games/moreblood/notes.txt")).unwrap();
    assert!(matches!(other, Reloaded::Nothing));
}

/// More Blood starts calm: standing still at the start for three
/// seconds, nobody notices the player and nothing hits.
#[test]
fn more_blood_starts_out_of_sight() {
    let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    for _ in 0..180 {
        game.step(&Default::default());
    }
    assert_eq!(game.game.alerted_enemies(), 0, "enemies notice the player at the start");
    assert_eq!(game.game.status.hp, 5, "the player is hit at the start");
}

/// The number keys switch between the songs `c.audio.music` lists; each plays.
#[test]
fn songs_can_be_switched() {
    let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let files: Vec<String> = game.cfg.audio.music.files().iter().map(|f| f.to_string()).collect();
    assert!(files.len() >= 2, "More Blood should list several songs");
    for (i, file) in files.iter().enumerate() {
        // The first song already plays; every other one is a change.
        assert_eq!(game.select_song(i), i != 0);
        assert_eq!(game.song_file(), Some(file.as_str()));
        assert!(game.music().is_some(), "{file} does not play");
    }
    assert!(!game.select_song(files.len()), "there is no song past the list");
}

/// Only models that can be seen or cast a shadow into the view are built:
/// at More Blood's start not the enemies far up the level (the view
/// itself reaches some 800 voxels ahead, up to the haze).
#[test]
fn models_out_of_view_are_not_built() {
    let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let frame = game.frame(1.0, 16.0 / 9.0);
    let far = game.view_target(1.0).y + 1200.0;
    assert!(game.game.enemies().any(|(e, _)| e.body.pos.y > far + 200.0), "More Blood should have enemies far ahead");
    let cubes = frame.cubes();
    assert!(cubes.iter().all(|i| i.pos_size[1] < far), "a cube far up the level was built");
    let all = game.game.enemies().count() * game.models.get("enemy.vvx").unwrap().visible[0].len();
    assert!(cubes.len() < all / 2, "{} cubes built, the enemies alone have {all}", cubes.len());
}

/// A model turned freely and drawn all as cubes (the level editor's
/// pieces, `model_cubes`) has its neighbours meet corner to corner as
/// the shader turns them (by the packed turn, `render::turn_as_drawn`),
/// and stands where the exact turn puts it. Placed by the exact turn,
/// neighbours were a hundredth of a voxel apart or over each other,
/// and single pixels of what lay behind showed on its faces.
#[test]
fn a_model_turned_freely_as_cubes_meets_corner_to_corner() {
    let mut frame: Vec<(IVec3, [u8; 4])> = Vec::new();
    for z in 0..4 {
        for y in 0..8 {
            for x in 0..24 {
                frame.push((IVec3::new(x, y, z), [110, 75, 50, 255]));
            }
        }
    }
    let metal = vec![0; frame.len()];
    let m = VoxModel::new(IVec3::new(24, 8, 4), vec![frame], vec![metal]);
    let origin = Vec3::new(100.0, 80.0, 3.0);
    for (look, rot) in [(IVec2::Y, turning(30.0, 0.0, 0.0)), (IVec2::X, turning(30.0, 20.0, 0.0)), (IVec2::NEG_Y, turning(-75.0, 12.0, 15.0))] {
        let (mut cubes, mut glass) = (Vec::new(), Vec::new());
        model_cubes(&m, 0, origin, look, rot, &mut cubes, &mut glass);
        let mut exact = Vec::new();
        draw(&mut |corner: Vec3, _, _, _, _, _, _| exact.push(corner + 0.5), None, &m, 0, origin, look, rot, &[], None);
        assert_eq!(cubes.len(), exact.len());
        let centre = |i: &Instance| Vec3::from_slice(&i.pos_size[..3]);
        let off = cubes.iter().zip(&exact).map(|(i, e)| centre(i).distance(*e)).fold(0.0, f32::max);
        assert!(off < 0.25, "{rot}: a cube {off} voxels from where the exact turn puts it");
        // As the shader turns each cube (`instance_turn`: the packed turn, normalized).
        let drawn = Quat::from_array(cubes[0].turn.map(|c| c as f32 / 127.0)).normalize();
        assert!(cubes.iter().all(|i| i.turn == cubes[0].turn));
        let (mut pairs, mut worst) = (0, 0.0f32);
        for a in &cubes {
            for b in &cubes {
                // Neighbours in the model's grid: one step along an axis, seen turned.
                let d = drawn.inverse() * (centre(b) - centre(a));
                let step = d.round();
                if step.abs().element_sum() == 1.0 {
                    pairs += 1;
                    worst = worst.max((d - step).abs().max_element());
                }
            }
        }
        assert!(pairs > 1000, "{rot}: {pairs} pairs of neighbours");
        assert!(worst < 1e-4, "{rot}: neighbours' shared corners {worst} voxels apart as drawn");
    }
}

/// A model drawn as cubes (the level editor's pieces, `model_cubes`)
/// leaves out the faces between its own opaque voxels, as the bank
/// does, counted in the world's directions whichever way it faces:
/// drawn, they won single bright pixels along the edges of a wall in
/// shade. Its glass keeps them, for the join's skins.
#[test]
fn a_model_as_cubes_leaves_out_the_faces_between_its_voxels() {
    // A wall 4 wide, 2 deep, 3 high, a pane of glass on its top.
    let mut frame: Vec<(IVec3, [u8; 4])> = Vec::new();
    for z in 0..3 {
        for y in 0..2 {
            for x in 0..4 {
                frame.push((IVec3::new(x, y, z), [200, 190, 150, 255]));
            }
        }
    }
    frame.extend((0..4).map(|x| (IVec3::new(x, 0, 3), [150, 200, 220, 120])));
    let metal = vec![0; frame.len()];
    let m = VoxModel::new(IVec3::new(4, 2, 4), vec![frame], vec![metal]);
    for look in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
        let (mut opaque, mut glass) = (Vec::new(), Vec::new());
        model_cubes(&m, 0, Vec3::new(20.0, 30.0, 4.0), look, Quat::IDENTITY, &mut opaque, &mut glass);
        assert_eq!((opaque.len(), glass.len()), (24, 4), "{look}: the wall's voxels are all on its surface");
        let cell = |i: &Instance| Vec3::from_slice(&i.pos_size[..3]).floor().as_ivec3();
        let taken: crate::world::CellSet = opaque.iter().map(cell).collect();
        for i in &opaque {
            let against = crate::mesh::DIRS.iter().enumerate().filter(|(_, d)| taken.contains(&(cell(i) + **d))).fold(0u8, |b, (k, _)| b | 1 << k);
            assert_eq!(i.fx[2], against, "{look}: the cube at {} leaves out exactly the faces its neighbours cover", cell(i));
        }
        assert!(glass.iter().all(|i| i.fx[2] == 0), "{look}: glass keeps its faces");
    }
}

/// A model drawn from the bank on the GPU puts the same cubes in the
/// world as drawn cube by cube, every way it turns, quarter turns and
/// free ones, flashing or not; its glass stays cubes
/// (`render::ModelBank::cubes` does what the shader's `vs_model` does).
#[test]
fn models_from_the_bank_match_their_cubes() {
    let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    type Cube = ([u32; 3], [u8; 4], u8, [i8; 4], [i8; 4], u8);
    let key = |centre: Vec3, color, metal, shade, turn, covered| -> Cube { (centre.to_array().map(f32::to_bits), color, metal, shade, turn, covered) };
    let huey = game.models.get("huey.vvx").unwrap();
    assert!(huey.glass.iter().any(|g| !g.is_empty()), "the Huey has no glass to test with");
    for m in [game.models.get("player").unwrap(), huey] {
        for look in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
            for rot in [Quat::IDENTITY, turning(30.0, 10.0, -25.0)] {
                for tint in [None, Some([255, 255, 255, 255])] {
                    let origin = Vec3::new(100.0, 200.0, 7.0);
                    let mut old: Vec<Cube> = Vec::new();
                    draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| old.push(key(corner + 0.5, color, metal, shade, turn, covered)), None, m, 1, origin, look, rot, &[], tint);
                    let mut new: Vec<Cube> = Vec::new();
                    let mut on_gpu = Vec::new();
                    draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| new.push(key(corner + 0.5, color, metal, shade, turn, covered)), Some(&mut on_gpu), m, 1, origin, look, rot, &[], tint);
                    assert_eq!(on_gpu.len(), 1);
                    let glass = new.len();
                    let bank = &game.bank;
                    new.extend(on_gpu.iter().flat_map(|d| bank.cubes(d)).map(|i| key(Vec3::from_slice(&i.pos_size[..3]), i.color, i.fx[3], i.shade, i.turn, i.fx[2])));
                    assert!(new.len() > glass * 10, "most of the model should come from the bank");
                    old.sort_unstable();
                    new.sort_unstable();
                    assert!(old == new, "{look} {rot} {tint:?}: the bank draws other cubes");
                }
            }
        }
    }
}

/// A figure of parts: posed, a part turns about its joint (an arm held
/// out to the side, turned a quarter to point ahead: its hand ahead at
/// the shoulder's height) and what hangs on it goes along; the rest
/// stays; turned and quartered alike, the bank draws each part where
/// the cubes do.
#[test]
fn a_figures_parts_turn_about_their_joints() {
    let dir = crate::scratch::Scratch::new("parts");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("figure.vox");
    // A body, a head on it, an arm held out to its side, a hand at its end.
    crate::vox::write_figure(&file, &[("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("head", IVec3::new(4, 4, 4), IVec3::new(0, 0, 8)), ("arm", IVec3::new(3, 2, 1), IVec3::new(4, 1, 6)), ("hand", IVec3::new(1, 2, 1), IVec3::new(7, 1, 6))]);
    let mut m = crate::vox::load_model(&file).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let mut bank = ModelBank::default();
    bank.add(&mut m);
    assert_eq!((m.parts[2].parent, m.parts[3].parent), (Some(0), Some(2)), "the arm on the body, the hand on the arm");
    let origin = Vec3::new(100.0, 200.0, 7.0);
    type Cube = ([u32; 3], [u8; 4], u8, [i8; 4], [i8; 4], u8);
    let key = |centre: Vec3, color, metal, shade, turn, covered| -> Cube { (centre.to_array().map(f32::to_bits), color, metal, shade, turn, covered) };
    let cubes = |look: IVec2, rot: Quat, pose: &[Quat]| {
        let pose: Vec<PartPose> = pose.iter().map(|q| PartPose::turned(*q)).collect();
        let pose = &pose[..];
        let mut all: Vec<Cube> = Vec::new();
        draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| all.push(key(corner + 0.5, color, metal, shade, turn, covered)), None, &m, 0, origin, look, rot, pose, None);
        let mut gpu: Vec<Cube> = Vec::new();
        let mut on_gpu = Vec::new();
        draw(&mut |corner: Vec3, color, metal, _, shade, turn, covered| gpu.push(key(corner + 0.5, color, metal, shade, turn, covered)), Some(&mut on_gpu), &m, 0, origin, look, rot, pose, None);
        assert_eq!(on_gpu.len(), 4, "a draw a part");
        gpu.extend(on_gpu.iter().flat_map(|d| bank.cubes(d)).map(|i| key(Vec3::from_slice(&i.pos_size[..3]), i.color, i.fx[3], i.shade, i.turn, i.fx[2])));
        all.sort_unstable();
        gpu.sort_unstable();
        assert!(all == gpu, "{look} {rot}: the bank draws other cubes");
        all
    };
    let colour = |part: usize| m.frames[0][m.parts[part].voxels[0] as usize].1;
    let centres = |cubes: &[Cube], part: usize| -> Vec<Vec3> { cubes.iter().filter(|c| c.1 == colour(part)).map(|c| Vec3::from_array(c.0.map(f32::from_bits))).collect() };
    let still = cubes(IVec2::Y, Quat::IDENTITY, &[]);
    let ahead = cubes(IVec2::Y, Quat::IDENTITY, &[Quat::IDENTITY, Quat::IDENTITY, turning(-90.0, 0.0, 0.0), Quat::IDENTITY]);
    let (before, after) = (centres(&still, 3), centres(&ahead, 3));
    assert!(after.iter().all(|h| h.y > before[0].y + 2.0), "the hand ahead: {after:?} from {before:?}");
    assert!(after.iter().all(|h| (h.z - before[0].z).abs() < 1e-4), "at the shoulder's height: {after:?}");
    assert_eq!(centres(&ahead, 1), centres(&still, 1), "the head stays");
    // Quartered and turned freely, the bank still draws what the cubes do.
    for look in [IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
        cubes(look, turning(20.0, 0.0, 10.0), &[turning(0.0, 0.0, 5.0), turning(30.0, 0.0, 0.0), turning(0.0, -40.0, 0.0), turning(0.0, 60.0, 0.0)]);
    }
    // Posed, a part's depth is taken nearer the further out it hangs, so
    // faces of two parts in one plane do not flicker (a bent elbow); at
    // rest every part lies as it is.
    let nearer = |pose: &[Quat]| {
        let pose: Vec<PartPose> = pose.iter().map(|q| PartPose::turned(*q)).collect();
        let pose = &pose[..];
        let mut on_gpu = Vec::new();
        draw(&mut |_, _, _, _, _, _, _| {}, Some(&mut on_gpu), &m, 0, origin, IVec2::Y, Quat::IDENTITY, pose, None);
        on_gpu.iter().map(|d| d.nearer).collect::<Vec<f32>>()
    };
    assert_eq!(nearer(&[]), [0.0; 4], "at rest");
    assert_eq!(nearer(&[Quat::IDENTITY, Quat::IDENTITY, turning(-90.0, 0.0, 0.0), Quat::IDENTITY]), [0.0, 1.0, 1.0, 2.0], "body, head, arm, hand");
}

/// A part turned at rest in its file (`rotation`, `pitch`, `roll`) is
/// drawn exactly as the unturned figure posed so: from the bank and as
/// cubes alike, what hangs on it carried along; a pose turns on from
/// the rest (`pose · rest`); `part_at_rest` is where `draw` puts it.
#[test]
fn a_part_turned_at_rest_draws_as_the_same_pose() {
    let text = |rest: &str| format!("format = \"voxcade voxel 1\"\nsize = [9, 3, 12]\n\n[palette]\np = \"#6a4a30\"\nl = \"#a07040\"\nh = \"#d0b040\"\n\n[[part]]\nname = \"frame\"\nat = [-4, 0, 0]\nlayers = \"\"\"\np.......p\n*11\n\nppppppppp\n\"\"\"\n\n[[part]]\nname = \"leaf\"\nat = [-3, 0, 0]\nparent = \"frame\"\njoint = [-3, 0.5, 5.5]\n{rest}layers = \"\"\"\nlllllll\n*11\n\"\"\"\n\n[[part]]\nname = \"handle\"\nat = [2, -1, 5]\nparent = \"leaf\"\nlayers = \"\"\"\nh\n\"\"\"\n");
    let model = |rest: &str| {
        let mut m = crate::vvx::read(&text(rest)).unwrap();
        ModelBank::default().add(&mut m);
        m
    };
    let (plain, rested) = (model(""), model("rotation = -70\npitch = 15\n"));
    assert!(!rests_turned(&plain) && rests_turned(&rested));
    let origin = Vec3::new(40.0, 50.0, 3.0);
    // The cubes (every voxel, as a fading model draws) and the bank's draws.
    let drawn = |m: &VoxModel, look: IVec2, rot: Quat, pose: &[Quat]| {
        let pose: Vec<PartPose> = pose.iter().map(|q| PartPose::turned(*q)).collect();
        let pose = &pose[..];
        let mut cubes: Vec<[u32; 3]> = Vec::new();
        draw(&mut |corner: Vec3, _, _, _, _, _, _| cubes.push(corner.to_array().map(f32::to_bits)), None, m, 0, origin, look, rot, pose, None);
        let mut on_gpu = Vec::new();
        draw(&mut |_, _, _, _, _, _, _| {}, Some(&mut on_gpu), m, 0, origin, look, rot, pose, None);
        (cubes, on_gpu)
    };
    let leaf = turning(-70.0, 15.0, 0.0);
    for (look, rot) in [(IVec2::Y, Quat::IDENTITY), (IVec2::X, turning(20.0, 0.0, 10.0)), (IVec2::NEG_Y, Quat::IDENTITY)] {
        assert_eq!(drawn(&rested, look, rot, &[]), drawn(&plain, look, rot, &[Quat::IDENTITY, leaf, Quat::IDENTITY]), "{look}: the rest is that pose");
        // A pose on top: round further, after the rest.
        let more = turning(30.0, 0.0, 0.0);
        assert_eq!(drawn(&rested, look, rot, &[Quat::IDENTITY, more, Quat::IDENTITY]), drawn(&plain, look, rot, &[Quat::IDENTITY, more * leaf, Quat::IDENTITY]), "{look}: pose · rest");
    }
    let (_, at_rest) = drawn(&plain, IVec2::Y, Quat::IDENTITY, &[]);
    assert!(at_rest.iter().all(|d| d.rot == Quat::IDENTITY && d.nearer == 0.0), "none at rest: as before");
    let (_, turned) = drawn(&rested, IVec2::Y, Quat::IDENTITY, &[]);
    assert_eq!(turned.iter().map(|d| d.nearer).collect::<Vec<_>>(), [0.0, 1.0, 2.0], "a part turned at rest is taken nearer as a posed one");
    assert_eq!(part_at_rest(&rested, 2).0, leaf, "the handle carried by the leaf");
    assert_eq!(part_at_rest(&plain, 2), (Quat::IDENTITY, Vec3::ZERO));
    // Only the picture: the simulation's cells are the unturned ones.
    assert_eq!(quarter_cells(&rested, IVec2::Y), quarter_cells(&plain, IVec2::Y));
}

/// MagicaVoxel's emission material on a colour makes a model's voxels
/// of it glow, as `frame:cube`'s glow does: from the bank (`fx[0]`
/// of its cubes) and as cubes (what a fading model draws); a model
/// without one carries no glow at all, as before.
#[test]
fn an_emissive_colour_makes_a_models_voxels_glow() {
    let dir = crate::scratch::Scratch::new("emit");
    std::fs::create_dir_all(&dir).unwrap();
    let model = |emit: bool| {
        let voxels = (0..3).map(|x| dot_vox::Voxel { x, y: 0, z: 0, i: x }).collect();
        let mut palette = vec![dot_vox::Color { r: 0, g: 0, b: 0, a: 255 }; 256];
        palette[0] = dot_vox::Color { r: 200, g: 40, b: 40, a: 255 };
        palette[1] = dot_vox::Color { r: 255, g: 220, b: 80, a: 255 };
        palette[2] = dot_vox::Color { r: 40, g: 40, b: 200, a: 255 };
        let lamp = dot_vox::Material { id: 2, properties: [("_type".to_string(), "_emit".to_string()), ("_emit".to_string(), "0.6".to_string())].into() };
        let data = dot_vox::DotVoxData {
            version: 150,
            index_map: Vec::new(),
            models: vec![dot_vox::Model { size: dot_vox::Size { x: 3, y: 1, z: 1 }, voxels }],
            palette,
            materials: if emit { vec![lamp] } else { Vec::new() },
            scenes: Vec::new(),
            layers: Vec::new(),
        };
        let file = dir.join(if emit { "lamp.vox" } else { "plain.vox" });
        let mut bytes = Vec::new();
        data.write_vox(&mut bytes).unwrap();
        std::fs::write(&file, bytes).unwrap();
        crate::vox::load_model(&file).unwrap()
    };
    let want = (0.6f32 * 255.0).round() as u8;
    // The glow by voxel and the colours' own.
    let mut lamp = model(true);
    assert_eq!(lamp.glow, vec![vec![0, want, 0]]);
    let mut plain = model(false);
    assert!(plain.glow.is_empty(), "no glow where no color has one");
    let origin = Vec3::new(10.0, 10.0, 0.0);
    for m in [&mut lamp, &mut plain] {
        let mut bank = crate::render::ModelBank::default();
        bank.add(m);
        let glowing = |m: &VoxModel| m.glow.first().map_or(0, |g| g.iter().filter(|&&g| g != 0).count());
        // From the bank.
        let mut on_gpu = Vec::new();
        draw(&mut |_, _, _, _, _, _, _| {}, Some(&mut on_gpu), m, 0, origin, IVec2::Y, Quat::IDENTITY, &[], None);
        let from_bank: Vec<u8> = on_gpu.iter().flat_map(|d| bank.cubes(d)).map(|i| i.fx[0]).collect();
        assert_eq!(from_bank.len(), 3);
        assert_eq!(from_bank.iter().filter(|&&g| g == want).count(), glowing(m), "the bank's cubes");
        assert!(from_bank.iter().all(|&g| g == 0 || g == want));
        // As cubes, through a Voxels as the game's frame does: no byte lost.
        let world = crate::world::World::new(IVec3::new(32, 32, 8), [[0; 4]; 256]);
        let mut voxels = Voxels { world: &world, eye: Vec3::new(0.0, 0.0, 100.0), moved: false, boxes: Vec::new(), taken: Default::default(), opaque: Vec::new(), see_through: Vec::new() };
        draw(
            &mut |corner: Vec3, color: [u8; 4], metal: u8, glow: u8, shade: [i8; 4], turn: [i8; 4], covered: u8| {
                let glow = if glow == 0 { 0.0 } else { (glow as f32 + 0.5) / 255.0 };
                voxels.add(corner, color, glow, 1.0, metal, shade, turn, covered)
            },
            None,
            m,
            0,
            origin,
            IVec2::Y,
            Quat::IDENTITY,
            &[],
            None,
        );
        let cubes = voxels.finish();
        assert_eq!(cubes.len(), 3);
        assert_eq!(cubes.iter().filter(|i| i.fx[0] == want).count(), glowing(m), "as cubes");
        assert!(cubes.iter().all(|i| i.fx[0] == 0 || i.fx[0] == want));
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A part's joint set by hand in the file is where it turns: an arm
/// hanging beside the body, its joint put at its top, swung half round
/// rises above it; by the rules (the middle of where it meets the
/// body) it stays about where it hung.
#[test]
fn a_part_turns_about_the_joint_set_in_its_file() {
    let dir = crate::scratch::Scratch::new("rigged");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("figure.vox");
    let parts = [("body", IVec3::new(4, 4, 8), IVec3::ZERO), ("arm", IVec3::new(1, 1, 6), IVec3::new(4, 1, 1))];
    let arm_z = |rig: &[(&str, &str, &str)]| {
        crate::vox::write_figure_with(&file, &parts, rig);
        let m = crate::vox::load_model(&file).unwrap();
        let mut zs = Vec::new();
        let colour = m.frames[0][m.parts[1].voxels[0] as usize].1;
        draw(&mut |corner: Vec3, c, _, _, _, _, _| if c == colour { zs.push(corner.z + 0.5) }, None, &m, 0, Vec3::ZERO, IVec2::Y, Quat::IDENTITY, &[PartPose::REST, PartPose::turned(turning(0.0, 180.0, 0.0))], None);
        (m.parts[1].joint.z, zs.iter().copied().fold(f32::MAX, f32::min), zs.iter().copied().fold(f32::MIN, f32::max))
    };
    // Its top: the arm's voxels end at z 7 (from 1 to 7).
    let (joint, low, high) = arm_z(&[("arm", "_joint", "4.5 1.5 7")]);
    assert_eq!(joint, 7.0);
    assert!(low > 7.0 && high < 13.5, "swung up about its top: {low}..{high}");
    let (joint, low, high) = arm_z(&[]);
    assert_eq!(joint, 4.0, "by the rules: the middle of where it meets the body");
    assert!(low < 2.0 && high > 6.0, "swung about its middle: {low}..{high}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A free turn of a quarter puts every voxel where the quarter turn
/// does (it turns about the same pivot), and none turns nothing:
/// models stay exactly as they were unless a game turns them.
#[test]
fn a_free_quarter_turn_is_the_quarter_turn() {
    let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let m = game.models.get("player").unwrap();
    let origin = Vec3::new(100.0, 200.0, 7.0);
    let cubes = |look: IVec2, rot: Quat| {
        let mut all: Vec<(Vec3, [i8; 4])> = Vec::new();
        draw(&mut |corner: Vec3, _, _, _, _, turn, _| all.push((corner + 0.5, turn)), None, m, 1, origin, look, rot, &[], None);
        all
    };
    assert_eq!(turning(0.0, 0.0, 0.0), Quat::IDENTITY);
    assert!(cubes(IVec2::Y, Quat::IDENTITY).iter().all(|(_, turn)| *turn == [0; 4]), "an unturned model is square to the grid");
    for (degrees, look) in [(90.0, IVec2::X), (180.0, IVec2::NEG_Y), (-90.0, IVec2::NEG_X)] {
        let free = cubes(IVec2::Y, turning(degrees, 0.0, 0.0));
        let quarter = cubes(look, Quat::IDENTITY);
        assert_eq!(free.len(), quarter.len());
        for ((a, _), (b, _)) in free.iter().zip(&quarter) {
            assert!(a.distance(*b) < 1e-3, "{degrees}°: a voxel at {a} instead of {b}");
        }
    }
    // Rolled right by a quarter it lies on its right side, its top
    // to the right (+x) by half its height from the middle: about the
    // voxel at half its height (22 tall: the 12th, 11.5 up), so its
    // top voxel's middle lies 10 to the right, on the grid.
    let rolled = cubes(IVec2::Y, turning(0.0, 0.0, 90.0));
    let right = rolled.iter().map(|(c, _)| c.x).fold(f32::MIN, f32::max);
    let centre = crate::render::turn_centre(m.size).z;
    assert!((right - (origin.x + 0.5 + (m.size.z as f32 - 0.5 - centre))).abs() < 1e-3, "rolled right, its top went to x {right}");
    assert!(rolled.iter().all(|(c, _)| (c - c.floor() - Vec3::splat(0.5)).abs().max_element() < 1e-3), "a quarter roll keeps the voxels on the grid");
    // The turn is in the model's own directions: facing +x, its right
    // is -y, and rolled right its top goes there.
    let rolled_east = cubes(IVec2::X, turning(0.0, 0.0, 90.0));
    let top = rolled_east.iter().map(|(c, _)| c.y).fold(f32::MAX, f32::min);
    assert!(top < origin.y - m.size.z as f32 * 0.25, "facing +x and rolled right, its top should lie towards -y ({top})");
}

/// Where a model's pivot lies and what a turn does to a model of even
/// size (pinned as it is, so nobody changes how games draw by
/// accident): the pivot is the cell `size / 2` (whole voxels, rounded
/// down) of each side, and a quarter turn turns the cells about the
/// middle of that cell. An odd size has the pivot in its middle and
/// fills the same cells at every turn. An even size has one more cell
/// on the low side of the pivot than on the high side; an axis that
/// a turn flips (a half turn flips both, a quarter turn one) then
/// fills the cells one voxel higher than unturned. Free turns (`rotates
/// = "free"`, `frame:model`'s turn) land on the same cells as the
/// quarter turns.
#[test]
fn an_even_sized_model_shifts_a_voxel_when_it_turns() {
    let dir = crate::scratch::Scratch::new("pivot");
    std::fs::create_dir_all(&dir).unwrap();
    // The cells a model fills from the pivot's cell, per look: the
    // lowest and highest of x and y.
    let cells = |size: IVec3, look: IVec2, free: Quat| {
        let file = dir.join("box.vox");
        crate::vox::write_figure(&file, &[("box", size, IVec3::ZERO)]);
        let m = crate::vox::load_model(&file).unwrap();
        assert_eq!(m.size, size);
        let origin = Vec3::new(100.0, 200.0, 0.0);
        let (mut lo, mut hi) = (IVec2::MAX, IVec2::MIN);
        draw(
            &mut |corner: Vec3, _, _, _, _, _, _| {
                let c = IVec2::new(corner.x.round() as i32 - 100, corner.y.round() as i32 - 200);
                (lo, hi) = (lo.min(c), hi.max(c));
            },
            None,
            &m,
            0,
            origin,
            look,
            free,
            &[],
            None,
        );
        (lo.to_array(), hi.to_array())
    };
    // Looking +y, +x, -y, -x: the quarter turns.
    let looks = [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X];
    // Odd across both ways: always -2 to 2.
    for look in looks {
        assert_eq!(cells(IVec3::new(5, 5, 2), look, Quat::IDENTITY), ([-2, -2], [2, 2]), "odd, {look}");
    }
    // Even across both ways (4 by 4): -2 to 1 as it stands; the flipped
    // axis -1 to 2: +x flips y, -y flips both, -x flips x.
    let want = [([-2, -2], [1, 1]), ([-2, -1], [1, 2]), ([-1, -1], [2, 2]), ([-1, -2], [2, 1])];
    for (look, want) in looks.iter().zip(want) {
        assert_eq!(cells(IVec3::new(4, 4, 2), *look, Quat::IDENTITY), want, "even, {look}");
    }
    // Four by five: the five never moves, the four does when flipped.
    let want = [([-2, -2], [1, 2]), ([-2, -1], [2, 2]), ([-1, -2], [2, 2]), ([-2, -2], [2, 1])];
    for (look, want) in looks.iter().zip(want) {
        assert_eq!(cells(IVec3::new(4, 5, 2), *look, Quat::IDENTITY), want, "four by five, {look}");
    }
    // A free half turn fills the cells of the quarter turns' second.
    assert_eq!(cells(IVec3::new(4, 4, 2), IVec2::Y, turning(180.0, 0.0, 0.0)), ([-1, -1], [2, 2]), "a free half turn");
    std::fs::remove_dir_all(&dir).ok();
}

/// The cells a solid piece fills (`quarter_cells`) are exactly where
/// its model's voxels are drawn, every quarter turn, an even size too;
/// the hidden ones inside count as well.
#[test]
fn a_solid_pieces_cells_are_where_it_is_drawn() {
    let dir = crate::scratch::Scratch::new("cells");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("box.vox");
    crate::vox::write_figure(&file, &[("box", IVec3::new(4, 5, 3), IVec3::ZERO)]);
    let m = crate::vox::load_model(&file).unwrap();
    for look in [IVec2::Y, IVec2::X, IVec2::NEG_Y, IVec2::NEG_X] {
        let cells = quarter_cells(&m, look);
        assert_eq!(cells.len(), 4 * 5 * 3, "every voxel, {look}");
        let mut drawn = Vec::new();
        let origin = Vec3::new(100.0, 200.0, 7.0);
        draw(&mut |corner: Vec3, _, _, _, _, _, _| drawn.push((corner - origin).round().as_ivec3()), None, &m, 0, origin, look, Quat::IDENTITY, &[], None);
        assert!(!drawn.is_empty() && drawn.iter().all(|c| cells.contains(c)), "drawn where it is solid, {look}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A piece turned by quarter turns about any axis (`turn`, `pitch`,
/// `roll`: all 64 of them, the 24 ways a box can lie) fills exactly the
/// cells its model is drawn in, as `frame:model` draws it with the same
/// three numbers (the quarter way it faces, then the rest freely): an
/// even height, an odd one, an even width.
#[test]
fn a_solid_piece_turned_any_way_is_where_it_is_drawn() {
    let dir = crate::scratch::Scratch::new("cells-24");
    for size in [IVec3::new(4, 5, 6), IVec3::new(3, 2, 7), IVec3::new(6, 4, 9)] {
        let file = dir.join(format!("box{}.vox", size.z));
        crate::vox::write_figure(&file, &[("box", size, IVec3::ZERO)]);
        let m = crate::vox::load_model(&file).unwrap();
        for k in 0..64 {
            let turns = [k % 4 * 90, k / 4 % 4 * 90, k / 16 * 90];
            let cells = piece_cells(&m, turns).unwrap();
            assert_eq!(cells.len(), (size.x * size.y * size.z) as usize, "every voxel, {turns:?}");
            let look = IVec2::from(crate::grid::Way::nearest(turns[0] as f64).pair());
            let free = turning(0.0, turns[1] as f32, turns[2] as f32);
            let origin = Vec3::new(100.0, 200.0, 7.0);
            let mut drawn = Vec::new();
            draw(
                &mut |corner: Vec3, _, _, _, _, _, _| {
                    let off = (corner - corner.round()).abs().max_element();
                    assert!(off < 1e-3, "a voxel off the grid by {off}, {turns:?}");
                    drawn.push((corner - origin).round().as_ivec3());
                },
                None,
                &m,
                0,
                origin,
                look,
                free,
                &[],
                None,
            );
            assert!(drawn.iter().all(|c| cells.contains(c)), "drawn where it is solid, {turns:?}");
            let bounds = |cs: &[IVec3]| (cs.iter().fold(IVec3::MAX, |a, c| a.min(*c)), cs.iter().fold(IVec3::MIN, |a, c| a.max(*c)));
            assert_eq!(bounds(&drawn), bounds(&cells), "the same box, {turns:?}");
        }
    }
    let ways: std::collections::BTreeSet<_> = (0..64).map(|k| quarter_matrix([k % 4 * 90, k / 4 % 4 * 90, k / 16 * 90]).unwrap()).collect();
    assert_eq!(ways.len(), 24, "the 24 ways a box lies");
    assert_eq!(piece_cells(&crate::vox::load_model(&dir.join("box6.vox")).unwrap(), [0, 30, 0]), None, "off the quarters: none");
}

/// A kind's spin turns its model round its upright axis with the
/// game's time, every thing of the kind alike: at 120 degrees a second
/// a quarter turn after three quarters of a second; none without.
#[test]
fn a_spinning_kind_turns_round() {
    assert_eq!(spinning(0.0, 1000.0), Quat::IDENTITY);
    let same = |a: Quat, b: Quat| a.dot(b).abs() > 0.99999;
    let quarter = spinning(120.0, 45.0);
    assert!(same(quarter, turning(90.0, 0.0, 0.0)), "{quarter}");
    assert!(same(spinning(120.0, 225.0), turning(90.0, 0.0, 0.0)), "round again");
}

/// The title logo's letters drawn from the bank put the same cubes on
/// screen as built every frame: while they fall, as they land and
/// while BLOOD drips (`ui::logo_letters`).
#[test]
fn the_logo_from_the_bank_matches_its_cubes() {
    let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let settings = Settings::default();
    type Cube = ([u32; 3], [u8; 4], [u8; 2], [i8; 4]);
    let cubes = |f: &FrameData| -> Vec<Cube> {
        let mut all: Vec<Cube> = f.ui_cubes().iter().map(|i| ([0, 1, 2].map(|k| i.pos_size[k].to_bits()), i.color, [i.fx[0], i.fx[1]], i.shade)).collect();
        all.sort_unstable();
        all
    };
    for ticks in [20, 45, 80, 240] {
        let mut menu = Menu::title();
        for _ in 0..ticks {
            menu.update();
        }
        let new = game.menu_frame(&menu, &settings, 0.5, 16.0 / 9.0, Scene::Title { time: 1.0 });
        assert!(!new.ui_models.is_empty(), "the logo is not drawn from the bank");
        let logo = std::mem::take(&mut game.logo);
        let old = game.menu_frame(&menu, &settings, 0.5, 16.0 / 9.0, Scene::Title { time: 1.0 });
        game.logo = logo;
        assert!(old.ui_models.is_empty());
        assert!(cubes(&old) == cubes(&new), "{ticks} ticks in: the logo from the bank differs");
    }
}

/// The fingerprint follows what the simulation reads: the light, the
/// music or Windows line ends leave it, the physics, a camera turn, the
/// hit stop or a level file change it (a replay of the old data would
/// run apart). And it is the same every time the game loads.
#[test]
fn the_fingerprint_follows_what_the_simulation_reads() {
    let dir = Path::new("games/moreblood");
    let cfg = config::<Shooter>(dir).unwrap();
    let level = |json: &[u8]| vec![("map.json".to_string(), json.to_vec()), ("ground/height.png".to_string(), vec![137, 80, 0, 13, 10])];
    let print = |c: &GameConfig, json: &[u8]| {
        fingerprint_of(c, &<Shooter as Game>::sections(c), &<Shooter as Game>::actions(c), &level(json)).unwrap()
    };
    let base = print(&cfg, b"{}\n");
    let with = |change: &dyn Fn(&mut GameConfig)| {
        let mut c = cfg.clone();
        change(&mut c);
        print(&c, b"{}\n")
    };
    assert_eq!(with(&|c| (c.light.ambient, c.audio.music_volume, c.camera.view_width) = (0.6, 0.5, 300.0)), base, "the look or the sound changed it");
    assert_eq!(with(&|c| c.audio.drive = 6.0), base, "the drive into the limiter changed it");
    assert_eq!(print(&cfg, b"{}\r\n"), base, "a level file with Windows line ends changes it");
    assert_ne!(with(&|c| c.physics.gravity = 530.0), base);
    assert_ne!(with(&|c| c.effects.hitstop = 3), base);
    assert_ne!(with(&|c| c.camera.yaw = 90.0), base);
    assert_ne!(print(&cfg, b"{ }\n"), base);
    // A default written out is the same game.
    let arena = Path::new("games/arena");
    let mut written = config::<Shooter>(arena).unwrap();
    written.physics.gravity = crate::config::PhysicsConfig::default().gravity;
    let plain = config::<Shooter>(arena).unwrap();
    assert_eq!(print(&written, b"{}\n"), print(&plain, b"{}\n"));
    let game = Session::<Shooter>::load(dir).unwrap();
    assert_eq!(game.fingerprint().unwrap(), Session::<Shooter>::load(dir).unwrap().fingerprint().unwrap());
}

/// Every game's config, written out and read back, is the same config.
#[test]
fn the_config_reads_back_as_written() {
    for game in ["games/moreblood", "games/arena", "games/bench"] {
        let cfg = config::<Shooter>(Path::new(game)).unwrap();
        let text = toml::to_string(&cfg).unwrap();
        let back: GameConfig = toml::from_str(&text).unwrap();
        assert_eq!(toml::to_string(&back).unwrap(), text, "{game} reads back otherwise");
    }
}

/// The same input gives the same state every second, run after run (the
/// check replays carry); other input soon gives another.
#[test]
fn the_same_run_gives_the_same_state() {
    let run = |fire: bool| {
        let mut game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
        let mut states = Vec::new();
        for t in 0..300 {
            let input = Input { up: true, left: t % 90 < 30, fire: fire && t % 20 < 10, grenade: t == 120, ..Input::default() };
            game.step(&input.into());
            if t % 60 == 59 {
                states.push(game.game.state_hash());
            }
        }
        states
    };
    let shooting = run(true);
    assert_eq!(shooting, run(true), "the same run gave another state");
    assert_ne!(shooting.last(), run(false).last(), "shooting changed nothing");
}

/// A game kept at one tick and brought back after playing on stands
/// exactly as it was kept, and plays on exactly as it did the first
/// time: the copy shares nothing the game went on to change.
#[test]
fn a_restored_game_plays_on_the_same() {
    use crate::replay::{Replay, Script, parse_script};
    let dir = Path::new("games/moreblood");
    let replay = Replay::load(&dir.join("replays/tests/run_and_gun.toml")).unwrap();
    let mut game = Session::<Shooter>::load(dir).unwrap();
    let mut script = Script::new(parse_script(&replay.input, game.actions()).unwrap());
    let inputs: Vec<_> = (0..1200).map(|_| script.next_input()).collect();
    for input in &inputs[..600] {
        game.step(input);
    }
    let kept_at = game.game.state_hash();
    let start = std::time::Instant::now();
    let snapshot = game.snapshot().unwrap();
    let took = start.elapsed();
    let mut first = Vec::new();
    for input in &inputs[600..] {
        game.step(input);
        first.push(game.game.state_hash());
    }
    assert_ne!(game.game.state_hash(), kept_at, "the run should have gone on");
    let start = std::time::Instant::now();
    game.restore(&snapshot);
    println!("snapshot {:?}, restore {:?}", took, start.elapsed());
    assert_eq!(game.game.state_hash(), kept_at, "the kept state was changed by the game playing on");
    assert!(!game.world_mut().take_dirty().is_empty(), "the blasts since should be meshed anew");
    for (n, input) in inputs[600..].iter().enumerate() {
        game.step(input);
        assert_eq!(game.game.state_hash(), first[n], "tick {} played out differently", 600 + n + 1);
    }
}

/// More Blood's figures are shaded smooth with hard edges, the dog tags
/// and the Huey face by face (`[light] model_shading_for`).
#[test]
fn more_blood_shades_its_figures_smooth() {
    let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let smooth = |m: &VoxModel| m.shade.iter().flatten().filter(|s| s[3] != 0).count();
    assert!(smooth(game.models.get("player").unwrap()) > 100, "Nick is not shaded smooth");
    assert!(smooth(game.models.get("enemy.vvx").unwrap()) > 100);
    assert!(game.models.props["dogtags.vvx"].shade.is_empty(), "the dog tags lost their crisp letters");
    assert!(game.models.get("huey.vvx").unwrap().shade.is_empty(), "the Huey is shaded smooth");
}

/// The title shows its props (More Blood's dog tags) and no player; the
/// game shows the player and no props.
#[test]
fn the_title_shows_its_props_and_no_player() {
    let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let steel = [168, 172, 176];
    let has = |frame: &FrameData, rgb: [u8; 3]| frame.cubes().iter().filter(|i| i.color[..3] == rgb).count();
    let title = game.compose(1.0, 16.0 / 9.0, Scene::Title { time: 1.0 }, None);
    let play = game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
    assert!(has(&title, steel) > 500, "no dog tags on the title");
    assert_eq!(has(&play, steel), 0, "the dog tags lie in the game");
    // Where the player stands, the game draws the player's colours and
    // the title none (the tags may lie there).
    let feet = game.game.player.body.pos;
    let size = game.models.get("player").unwrap().size.as_vec3();
    let colors: HashSet<[u8; 4]> = game.models.get("player").unwrap().frames[0].iter().map(|(_, c)| *c).collect();
    let near = |f: &FrameData| {
        f.cubes()
            .iter()
            .filter(|i| colors.contains(&i.color))
            .filter(|i| {
                let p = Vec3::new(i.pos_size[0], i.pos_size[1], i.pos_size[2]) - feet;
                p.x.abs() < size.x / 2.0 && p.y.abs() < size.y / 2.0 && p.z > 0.0 && p.z < size.z
            })
            .count()
    };
    assert!(near(&play) > 50, "no player in the game");
    assert_eq!(near(&title), 0, "the player stands on the title");
}

/// The flyby crosses now and then, and is gone in between.
#[test]
fn the_title_flyby_comes_and_goes() {
    let game = Session::<Shooter>::load(Path::new("games/moreblood")).unwrap();
    let every = game.cfg.title_screen.flyby_every;
    assert!(game.flyby_at(1.0).is_none(), "it is there from the start");
    let a = game.flyby_at(4.0).expect("no flyby after 4 s");
    let b = game.flyby_at(5.0).expect("the flyby vanished halfway");
    assert!(b.x > a.x, "it does not cross");
    assert!(game.flyby_at(3.0 + every * 0.5 + 3.0).is_none());
    assert!(game.flyby_at(4.0 + every).is_some(), "it does not come back");
}

/// The player's choices turn off the screen flash and the blur.
#[test]
fn options_turn_off_flashes_and_blur() {
    let mut game = flat_game(64);
    game.cfg.effects.tilt_shift = 0.5;
    game.game.screen_flash = 1.0;
    let f = game.compose(1.0, 1.0, Scene::Play, None);
    assert!(f.screen_flash > 0.0 && f.tilt_shift > 0.0);
    game.options = Options { shake: false, flashes: false, blur: false, ..Options::default() };
    let f = game.compose(1.0, 1.0, Scene::Play, None);
    assert_eq!((f.screen_flash, f.tilt_shift), (0.0, 0.0));
}

/// Stepping over flowers and rubble must not move the camera up or down.
#[test]
fn walking_over_bumps_keeps_the_camera_calm() {
    let mut game = Session::<Shooter>::load(Path::new("games/arena")).unwrap();
    // Single voxels in the path, like flowers and settled rubble.
    let start = game.game.player.body.pos.floor().as_ivec3();
    for i in 1..12 {
        game.game.world.set(start + IVec3::new(-6 * i, 0, 0), 1);
    }
    let walk = Input { left: true, ..Input::default() };
    let (mut steps, mut max_cam) = (0, 0.0f32);
    for _ in 0..240 {
        let cam_before = game.game.cam_target.z;
        let z_before = game.game.player.body.pos.z;
        game.game.step(&walk);
        if game.game.player.body.pos.z > z_before {
            steps += 1;
        }
        max_cam = max_cam.max((game.game.cam_target.z - cam_before).abs());
    }
    println!("step-ups {steps}, max camera rise per tick {max_cam:.3}");
    assert!(steps > 0, "the path should cross something to step over");
    assert_eq!(max_cam, 0.0, "camera height moves {max_cam} voxels in one tick");
}

/// A window wider than the meshes' faces were chosen for (32:9, past
/// `camera::WIDEST_KEPT`) adds the faces its edges can see, as a picture
/// at another pitch does: the window, `shot` and the IDE build the meshes
/// anew once `face_mask` grows. A camera turned 20 degrees sees the +y
/// faces only at the far edge of so wide a window.
#[test]
fn a_window_wider_than_the_faces_were_chosen_for_adds_its_faces() {
    let mut cfg = GameConfig::default();
    (cfg.camera.yaw, cfg.camera.pitch, cfg.camera.fov) = (20.0, 45.0, 40.0);
    let game = game_on(crate::sim::tests::flat_world(IVec3::new(64, 64, 16)), cfg);
    let plus_y = crate::mesh::DIRS.iter().position(|d| *d == IVec3::Y).unwrap();
    assert!(!game.face_mask()[plus_y], "a window up to 21:9 sees no +y face");
    game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
    assert!(!game.face_mask()[plus_y], "16:9 adds nothing");
    let wide = game.face_mask();
    game.compose(1.0, 32.0 / 9.0, Scene::Play, None);
    let wider = game.face_mask();
    assert!(wider[plus_y], "32:9 sees the +y faces at its edge");
    assert!(wide.iter().zip(&wider).all(|(a, b)| !a || *b), "faces are only added");
    game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
    assert_eq!(game.face_mask(), wider, "kept from then on");
}

/// More lights in a picture than the renderer draws (`render::MAX_LIGHTS`)
/// are said, once: in a picture and in `look` (`inspect`'s, without one).
#[test]
fn lights_past_the_limit_are_said_once() {
    let mut game = flat_game(256);
    for i in 0..crate::render::MAX_LIGHTS {
        game.game.explode(Vec3::new(20.0 + i as f32 * 12.0, 128.0, 30.0), 10.0);
    }
    game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
    assert!(game.warnings().is_empty(), "16 lights are all drawn");
    for i in 0..7 {
        game.game.explode(Vec3::new(20.0 + i as f32 * 12.0, 160.0, 30.0), 10.0);
    }
    game.look();
    let said = game.warnings();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with("lights: 23 asked, 16 drawn"), "{said:?}");
    game.compose(1.0, 16.0 / 9.0, Scene::Play, None);
    assert!(game.warnings().is_empty(), "said once");
}
