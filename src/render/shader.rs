//! The shaders' text and the data the scene's shader reads, laid out as
//! its WGSL structs are.

use bytemuck::{Pod, Zeroable};

use super::FORWARD_LIGHTS;

/// A light as the shaders take it (`light.wgsl`'s `Light`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LightData {
    pub(super) pos_radius: [f32; 4],
    /// rgb: colour times brightness, w: the falloff's power.
    pub(super) color: [f32; 4],
    /// xyz: a spot's way, w: the cosine of half its cone (-2: every way).
    pub(super) spot: [f32; 4],
    /// x: the cosine within which the cone's edge is hard; y: 1 when walls
    /// stop it (in the lamps' pass with walls only); z, w: its place's key.
    pub(super) edge: [f32; 4],
}

impl LightData {
    /// `l` as the shaders take it, walls stopping it where `walls` and it
    /// says so.
    pub(super) fn of(l: &super::Light, walls: bool) -> Self {
        let (spot, inner) = match l.spot {
            Some(s) => (s.way.extend(s.outer).to_array(), s.inner),
            None => ([0.0, 0.0, 0.0, -2.0], -2.0),
        };
        let stopped = if walls && l.stopped() { 1.0 } else { 0.0 };
        // Its exact place as two whole numbers a float holds exactly: what
        // the lamps' memory knows it by (`lamps.wgsl`'s `stopped`).
        let mut h = crate::hash::Stable::default();
        h.vec3(l.pos);
        let place = h.finish();
        let ids = [(place & 0xff_ffff) as f32, ((place >> 24) & 0xff_ffff) as f32];
        Self { pos_radius: l.pos.extend(l.radius).to_array(), color: [l.color[0], l.color[1], l.color[2], l.falloff], spot, edge: [inner, stopped, ids[0], ids[1]] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Globals {
    pub(super) view_proj: [[f32; 4]; 4],
    pub(super) light_vp: [[f32; 4]; 4],
    pub(super) sun_dir: [f32; 4],
    pub(super) params: [f32; 4],
    pub(super) eye: [f32; 4],
    pub(super) haze: [f32; 4],
    pub(super) misc: [f32; 4],
    /// rgb: the sky metal mirrors, overhead and at the horizon.
    pub(super) reflection: [f32; 4],
    pub(super) horizon: [f32; 4],
    /// rgb: the models' outline colour, w: how many voxels thick (0: none).
    pub(super) outline: [f32; 4],
    /// rgb: the sunlight's colour and strength.
    pub(super) sun_color: [f32; 4],
    /// x: the picture's number, y: 1 when the faces remember their shadow.
    pub(super) memory: [u32; 4],
    pub(super) lights: [LightData; FORWARD_LIGHTS],
}

/// The scene's shader as written (`shader.wgsl`), without the numbers it
/// shares with the Rust side (`prelude`).
const WRITTEN: &str = include_str!("../shader.wgsl");

/// What a light gives a point (`light.wgsl`): the scene's shader's and the
/// lamps' pass's alike.
const LIGHT: &str = include_str!("../light.wgsl");

/// The scene's shader whole: the prelude, a light, then its text; the
/// faces' memory of their shadow in it (`plain_shader` takes it out).
pub(super) static SHADER: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| prelude() + LIGHT + WRITTEN);

/// The lamps' pass's shader whole (`lamps.wgsl`, `render::lamps`): the
/// prelude, a light, then its text; walls stopping lamps in it
/// (`without_walls` takes them out).
pub(super) static LAMPS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| prelude() + LIGHT + include_str!("../lamps.wgsl"));

/// How a metal colour's metalness and smoothness share its byte
/// (`vox::metal::pack`): metalness in the high bits, each in this many
/// steps.
pub const METAL_SHIFT: u32 = 4;
pub const METAL_STEPS: u32 = 15;

/// The numbers the shader shares with the Rust side, as WGSL constants
/// written ahead of its text: the one table both read, so a lane, a limit
/// or the faces' order cannot differ between them.
pub(super) fn prelude() -> String {
    use crate::mesh::DIRS;
    use glam::IVec3;
    let face = |d: IVec3| DIRS.iter().position(|e| *e == d).expect("every way is one of DIRS");
    let mut out = String::from("// Written by render::shader::prelude: shared with the Rust side.\n");
    let mut put = |name: &str, value: String| out += &format!("const {name} = {value};\n");
    put("MAX_LIGHTS", format!("{}u", super::FORWARD_LIGHTS));
    put("FACES", format!("{SHADOW_FACES}u"));
    put("FX_GLOW", format!("{}", super::FX_GLOW));
    put("FX_OPACITY", format!("{}", super::FX_OPACITY));
    put("FX_COVERED", format!("{}", super::FX_COVERED));
    put("FX_METAL", format!("{}", super::FX_METAL));
    put("METAL_SHIFT", format!("{METAL_SHIFT}u"));
    put("METAL_STEPS", format!("{METAL_STEPS}u"));
    // `Instance::shade` reads as Snorm8x4: its w back to whole bits.
    put("SNORM8", format!("{:.1}", i8::MAX));
    for (name, d) in [("FACE_POS_X", IVec3::X), ("FACE_NEG_X", IVec3::NEG_X), ("FACE_POS_Y", IVec3::Y), ("FACE_NEG_Y", IVec3::NEG_Y), ("FACE_POS_Z", IVec3::Z), ("FACE_NEG_Z", IVec3::NEG_Z)] {
        put(name, format!("{}u", face(d)));
    }
    // How a model's baked face packs its cell (`baked::Face::cell`).
    put("FACE_CELL_BITS", format!("{}u", super::baked::CELL_BITS));
    put("FACE_DIR_AT", format!("{}u", super::baked::DIR_AT));
    put("FACE_LIT_AT", format!("{}u", super::baked::LIT_AT));
    put("FACE_FIRST_AT", format!("{}u", super::baked::FIRST_AT));
    // The unit cube the cubes are drawn with (`mesh::unit_cube`), for a
    // baked face drawn as one of its faces: its corners (four a face, face
    // after face in `mesh::DIRS` order) and their faces' ways, and those
    // ways by face.
    let (corners, _) = crate::mesh::unit_cube();
    let vec3 = |v: [f32; 3]| format!("vec3<f32>({:?}, {:?}, {:?})", v[0], v[1], v[2]);
    let array = |items: Vec<String>| format!("array<vec3<f32>, {}>({})", items.len(), items.join(", "));
    put("CUBE_CORNERS", array(corners.iter().map(|c| vec3(c.pos)).collect()));
    put("CUBE_NORMALS", array(corners.iter().map(|c| vec3([0, 1, 2].map(|k| f32::from(c.normal[k]) / f32::from(i8::MAX)))).collect()));
    put("FACE_DIRS", array(DIRS.iter().map(|d| vec3(d.as_vec3().to_array())).collect()));
    out
}

/// How many faces' shadows the memory holds (the shader's FACES; three
/// words each, `FrameData::remember`).
pub(super) const SHADOW_FACES: u64 = 65536;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct PostParams {
    pub(super) params: [f32; 4],
    pub(super) step: [f32; 4],
}

/// The shader without what lies between its MEMORY marks (lines `// MEMORY
/// {` and `// } MEMORY`): the faces' memory of their shadow, which only a
/// game moving its sun needs. A fragment shader that may write storage
/// loses early depth testing on many GPUs (the scene pass cost 10 to 30 %
/// more with it, never used, on the Radeon 610M), and some GPUs have no
/// such writes at all.
pub(crate) fn plain_shader(src: &str) -> String {
    without(src, "MEMORY")
}

/// The lamps' pass's shader without walls stopping lamps (what lies
/// between its WALLS marks): for a GPU without storage written from a
/// fragment shader, whose lamps light through walls.
pub(super) fn without_walls(src: &str) -> String {
    without(src, "WALLS")
}

/// The shader without what lies between the lines `// MARK {` and `// }
/// MARK`.
fn without(src: &str, mark: &str) -> String {
    let (open, close) = (format!("// {mark} {{"), format!("// }} {mark}"));
    let mut out = String::with_capacity(src.len());
    let mut inside = false;
    for line in src.split_inclusive('\n') {
        match line.trim() {
            l if l == open => inside = true,
            l if l == close => inside = false,
            _ if !inside => out.push_str(line),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{SHADER, plain_shader};
    use crate::render::composite_entry;

    /// The shaders parse and validate: wgpu only finds out when it runs them.
    #[test]
    fn the_shaders_are_valid() {
        use wgpu::naga;
        let plain = plain_shader(&SHADER);
        let lamps_plain = super::without_walls(&super::LAMPS);
        for (name, src) in [("shader.wgsl", SHADER.as_str()), ("shader.wgsl without the memory", plain.as_str()), ("post.wgsl", include_str!("../post.wgsl")), ("lamps.wgsl", super::LAMPS.as_str()), ("lamps.wgsl without walls", lamps_plain.as_str())] {
            let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
        // The plain shader holds nothing of the memory: no storage, no
        // atomics (the whole one has both); the marks come in pairs.
        assert!(SHADER.contains("atomicCompareExchangeWeak") && SHADER.contains("var<storage, read_write>"));
        assert!(!plain.contains("atomic") && !plain.contains("var<storage") && !plain.contains("remembered("), "{plain}");
        assert_eq!(SHADER.matches("// MEMORY {").count(), SHADER.matches("// } MEMORY").count());
        // The lamps' pass without walls reads no volume and writes nothing.
        assert!(super::LAMPS.contains("var<storage, read_write>") && !lamps_plain.contains("var<storage") && !lamps_plain.contains("texture_3d") && !lamps_plain.contains("stopped("), "{lamps_plain}");
        assert_eq!(super::LAMPS.matches("// WALLS {").count(), super::LAMPS.matches("// } WALLS").count());
        // Both composites are there by the names the renderer asks for.
        for entry in [composite_entry(wgpu::TextureFormat::Bgra8Unorm), composite_entry(wgpu::TextureFormat::Bgra8UnormSrgb)] {
            assert!(include_str!("../post.wgsl").contains(&format!("fn {entry}(")), "{entry}");
        }
    }

    /// The shadow pass leaves out a model's covered faces as the main pass
    /// does (BunnyFarm's shadows took 11.5 ms of 21.9 on the small GPU at
    /// HIGH with every face of every voxel drawn; 5.5 without them, the
    /// pictures the same): its vertex stage reads the faces' direction and
    /// the covered bits (`fx`, location 4) as `vs_model` does.
    #[test]
    fn the_shadow_pass_leaves_out_covered_faces() {
        use wgpu::naga;
        let module = naga::front::wgsl::parse_str(&SHADER).unwrap();
        let locations = |name: &str| -> Vec<u32> {
            let entry = module.entry_points.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name}"));
            entry.function.arguments.iter().filter_map(|a| match a.binding {
                Some(naga::Binding::Location { location, .. }) => Some(location),
                _ => None,
            }).collect()
        };
        for wanted in [1, 4] {
            assert!(locations("vs_model").contains(&wanted));
            assert!(locations("vs_model_shadow").contains(&wanted), "vs_model_shadow reads location {wanted}");
        }
        let body = &SHADER[SHADER.find("fn vs_model_shadow(").unwrap()..];
        let body = &body[..body.find("\n}\n").unwrap()];
        assert!(body.contains("fx[FX_COVERED]") && body.contains("face_index(own)"), "{body}");
    }

    /// A struct of the shader's: its size, alignment and members' offsets,
    /// as naga lays it out.
    fn wgsl_struct(module: &wgpu::naga::Module, name: &str) -> (u32, u32, Vec<(String, u32)>) {
        use wgpu::naga;
        let mut layouter = naga::proc::Layouter::default();
        layouter.update(module.to_ctx()).unwrap();
        let (handle, ty) = module.types.iter().find(|(_, t)| t.name.as_deref() == Some(name)).unwrap_or_else(|| panic!("no struct {name} in the shader"));
        let naga::TypeInner::Struct { members, span } = &ty.inner else { panic!("{name} is no struct") };
        let members = members.iter().map(|m| (m.name.clone().unwrap_or_default(), m.offset)).collect();
        (*span, layouter[handle].alignment.round_up(1), members)
    }

    /// Every struct the Rust side writes into a buffer the shaders read
    /// (`Globals`, its lights, a model draw's place, the post passes'
    /// numbers) is laid out as the WGSL struct of the same members: the
    /// same size, every member at the same offset, and the size a whole
    /// number of the WGSL alignment (so the next one in an array, or a
    /// draw's place `ACTOR_STRIDE` on, lies right). Naga's validation alone
    /// would not see a member added on one side only, or two swapped.
    #[test]
    fn the_rust_structs_are_laid_out_as_the_shaders() {
        use super::{Globals, LightData, PostParams};
        use crate::render::bank::{ACTOR_STRIDE, ActorData};
        use std::mem::{offset_of, size_of};
        let scene = wgpu::naga::front::wgsl::parse_str(&SHADER).unwrap();
        let post = wgpu::naga::front::wgsl::parse_str(include_str!("../post.wgsl")).unwrap();
        let globals = [
            ("view_proj", offset_of!(Globals, view_proj)),
            ("light_vp", offset_of!(Globals, light_vp)),
            ("sun_dir", offset_of!(Globals, sun_dir)),
            ("params", offset_of!(Globals, params)),
            ("eye", offset_of!(Globals, eye)),
            ("haze", offset_of!(Globals, haze)),
            ("misc", offset_of!(Globals, misc)),
            ("reflection", offset_of!(Globals, reflection)),
            ("horizon", offset_of!(Globals, horizon)),
            ("outline", offset_of!(Globals, outline)),
            ("sun_color", offset_of!(Globals, sun_color)),
            ("memory", offset_of!(Globals, memory)),
            ("lights", offset_of!(Globals, lights)),
        ];
        let light = [("pos_radius", offset_of!(LightData, pos_radius)), ("color", offset_of!(LightData, color)), ("spot", offset_of!(LightData, spot)), ("edge", offset_of!(LightData, edge))];
        let actor = [
            ("origin", offset_of!(ActorData, origin)),
            ("tint", offset_of!(ActorData, tint)),
            ("extra", offset_of!(ActorData, extra)),
            ("rot", offset_of!(ActorData, rot)),
            ("pivot", offset_of!(ActorData, pivot)),
        ];
        let params = [("params", offset_of!(PostParams, params)), ("step", offset_of!(PostParams, step))];
        type Fields<'a> = &'a [(&'a str, usize)];
        let pairs: [(&wgpu::naga::Module, &str, usize, Fields); 4] = [
            (&scene, "Globals", size_of::<Globals>(), &globals),
            (&scene, "Light", size_of::<LightData>(), &light),
            (&scene, "Actor", size_of::<ActorData>(), &actor),
            (&post, "Post", size_of::<PostParams>(), &params),
        ];
        for (module, name, size, rust) in pairs {
            let (span, align, members) = wgsl_struct(module, name);
            assert_eq!(span as usize, size, "{name}: the shader's size and the Rust struct's");
            assert!(size.is_multiple_of(align as usize), "{name}: its size a whole number of its WGSL alignment ({align})");
            let rust: Vec<(String, u32)> = rust.iter().map(|(n, o)| (n.to_string(), *o as u32)).collect();
            assert_eq!(members, rust, "{name}: the members and their offsets");
        }
        assert!(size_of::<ActorData>() <= ACTOR_STRIDE && ACTOR_STRIDE.is_multiple_of(256), "a model draw's place fits its stride, which every GPU allows");
    }

    /// The vertex buffers the pipelines give (`chunk_buffers`,
    /// `instance_buffers`) lie as the Rust structs they are made of, field
    /// by field, and every input a vertex entry point reads is one of them
    /// with as many components.
    #[test]
    fn the_vertex_inputs_are_the_rust_structs() {
        use crate::mesh::Vertex;
        use crate::render::Instance;
        use crate::render::baked::Face;
        use crate::render::pipelines::{chunk_buffers, face_buffers, instance_buffers};
        use std::mem::{offset_of, size_of};
        use wgpu::naga;
        let vertex = [(offset_of!(Vertex, pos), 12), (offset_of!(Vertex, normal), 4), (offset_of!(Vertex, color), 4)];
        let instance = [(offset_of!(Instance, pos_size), 16), (offset_of!(Instance, color), 4), (offset_of!(Instance, fx), 4), (offset_of!(Instance, shade), 4), (offset_of!(Instance, turn), 4)];
        let check = |layout: &wgpu::VertexBufferLayout, stride: usize, fields: &[(usize, u64)]| {
            assert_eq!(layout.array_stride as usize, stride);
            assert_eq!(layout.attributes.len(), fields.len());
            for (a, (offset, size)) in layout.attributes.iter().zip(fields) {
                assert_eq!((a.offset as usize, a.format.size()), (*offset, *size), "location {}", a.shader_location);
            }
        };
        let [chunk] = chunk_buffers();
        let chunk = chunk.unwrap();
        check(&chunk, size_of::<Vertex>(), &vertex);
        let [cube, inst] = instance_buffers();
        let (cube, inst) = (cube.unwrap(), inst.unwrap());
        check(&cube, size_of::<Vertex>(), &vertex[..2]);
        check(&inst, size_of::<Instance>(), &instance);
        // A baked face: its cell, colour, shade, and its shade's four bytes
        // again as whole numbers (the last its metal).
        let [face] = face_buffers();
        let face = face.unwrap();
        check(&face, size_of::<Face>(), &[(offset_of!(Face, cell), 4), (offset_of!(Face, color), 4), (offset_of!(Face, shade), 4), (offset_of!(Face, shade), 4)]);
        assert_eq!((offset_of!(Face, metal), size_of::<Face>()), (offset_of!(Face, shade) + 3, 12), "the metal the shade's fourth byte, the last");
        let components = |f: wgpu::VertexFormat| match format!("{f:?}") {
            n if n.ends_with("x4") => 4,
            n if n.ends_with("x3") => 3,
            n if n.ends_with("x2") => 2,
            _ => 1,
        };
        let module = naga::front::wgsl::parse_str(&SHADER).unwrap();
        let cubes = [cube.attributes, inst.attributes].concat();
        let faces = [("vs_face", face.attributes), ("vs_face_shadow", face.attributes), ("vs_face_outline", face.attributes)];
        for (name, attrs) in [("vs_chunk", chunk.attributes), ("vs_chunk_shadow", chunk.attributes), ("vs_inst", &cubes[..]), ("vs_inst_shadow", &cubes[..]), ("vs_model", &cubes[..]), ("vs_model_shadow", &cubes[..]), ("vs_model_outline", &cubes[..])].into_iter().chain(faces) {
            let entry = module.entry_points.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name}"));
            for arg in &entry.function.arguments {
                let Some(naga::Binding::Location { location, .. }) = arg.binding else { continue };
                let given = attrs.iter().find(|a| a.shader_location == location).unwrap_or_else(|| panic!("{name} reads location {location}, which no buffer gives"));
                let wanted = match module.types[arg.ty].inner {
                    naga::TypeInner::Vector { size, .. } => size as usize,
                    naga::TypeInner::Scalar(_) => 1,
                    ref other => panic!("{name}: location {location} is {other:?}"),
                };
                assert_eq!(components(given.format), wanted, "{name}: location {location}");
                // Whole numbers where the shader reads them whole.
                let whole = matches!(module.types[arg.ty].inner, naga::TypeInner::Vector { scalar: naga::Scalar { kind: naga::ScalarKind::Uint, .. }, .. } | naga::TypeInner::Scalar(naga::Scalar { kind: naga::ScalarKind::Uint, .. }));
                assert_eq!(whole, format!("{:?}", given.format).starts_with("Uint"), "{name}: location {location}");
            }
        }
    }

    /// The numbers the shader shares with the Rust side are the prelude's,
    /// written from the Rust table: the lanes of `fx` as the bank fills
    /// them, the faces' order as `mesh::DIRS`, metal's packing as
    /// `vox::metal::pack`, the limits; and the shader's own text defines
    /// none of them a second time.
    #[test]
    fn the_shader_and_the_rust_side_share_one_table() {
        use crate::render::{FORWARD_LIGHTS, FX_COVERED, FX_GLOW, FX_METAL, FX_OPACITY};
        let prelude = super::prelude();
        for (name, value) in [("MAX_LIGHTS", format!("{FORWARD_LIGHTS}u")), ("FACES", format!("{}u", super::SHADOW_FACES)), ("FX_GLOW", FX_GLOW.to_string()), ("FX_OPACITY", FX_OPACITY.to_string()), ("FX_COVERED", FX_COVERED.to_string()), ("FX_METAL", FX_METAL.to_string())] {
            assert!(prelude.contains(&format!("const {name} = {value};")), "{name} in {prelude}");
            assert!(!super::WRITTEN.contains(&format!("const {name}")), "shader.wgsl defines {name} itself");
        }
        let mut lanes = [FX_GLOW, FX_OPACITY, FX_COVERED, FX_METAL];
        lanes.sort_unstable();
        assert_eq!(lanes, [0, 1, 2, 3], "four lanes, each its own");
        for (k, d) in crate::mesh::DIRS.iter().enumerate() {
            let name = format!("FACE_{}_{}", if d.max_element() > 0 { "POS" } else { "NEG" }, ["X", "Y", "Z"][d.abs().max_position()]);
            assert!(prelude.contains(&format!("const {name} = {k}u;")), "{name} in {prelude}");
        }
        // A baked face's packing, and the unit cube a face of which it is
        // drawn as: face `w`'s four corners `4w` to `4w + 3`, of its way,
        // and its triangles the first face's moved on by `4w` (`vs_face`
        // draws every face by the first's, `Renderer::face_indices`).
        use crate::render::baked::{CELL_BITS, DIR_AT, FIRST_AT, LIT_AT};
        for (name, value) in [("FACE_CELL_BITS", CELL_BITS), ("FACE_DIR_AT", DIR_AT), ("FACE_LIT_AT", LIT_AT), ("FACE_FIRST_AT", FIRST_AT)] {
            assert!(prelude.contains(&format!("const {name} = {value}u;")), "{name} in {prelude}");
        }
        assert_eq!(FIRST_AT, 31, "a face's cell, way, lit and first in one word");
        let (corners, triangles) = crate::mesh::unit_cube();
        assert_eq!((corners.len(), triangles.len()), (24, 36));
        for (w, d) in crate::mesh::DIRS.iter().enumerate() {
            let first = triangles[..6].iter().map(|i| i + 4 * w as u32);
            assert!(triangles[6 * w..6 * w + 6].iter().copied().eq(first), "face {w}");
            for c in &corners[4 * w..4 * w + 4] {
                assert_eq!(c.normal[..3], [d.x as i8 * 127, d.y as i8 * 127, d.z as i8 * 127], "face {w}");
            }
        }
        for name in ["CUBE_CORNERS", "CUBE_NORMALS", "FACE_DIRS"] {
            assert!(prelude.contains(&format!("const {name} = array<")) && !super::WRITTEN.contains(&format!("const {name}")), "{name}");
        }
        let m = |metal: f32, smooth: f32| u32::from(crate::vox::metal::pack(metal, smooth));
        assert_eq!(m(1.0, 0.0) >> super::METAL_SHIFT, super::METAL_STEPS, "metalness in the high bits");
        assert_eq!(m(0.0, 1.0) & super::METAL_STEPS, super::METAL_STEPS, "smoothness in the low bits");
        assert_eq!(m(1.0, 1.0), 255);
        // A voxel of the bank carries its glow, opacity, covered faces and
        // metal in the lanes so named.
        let frame: Vec<_> = (0..8).map(|n| (glam::IVec3::new(n & 1, n >> 1 & 1, n >> 2), [200, 100, 50, 255])).collect();
        let metal = m(1.0, 0.5) as u8;
        let mut model = crate::vox::VoxModel::new(glam::IVec3::splat(2), vec![frame], vec![vec![metal; 8]]);
        model.glow = vec![vec![7; 8]];
        let mut bank = crate::render::ModelBank::default();
        bank.add(&mut model);
        let v = bank.voxels[0];
        assert_eq!((v.fx[FX_GLOW], v.fx[FX_OPACITY], v.fx[FX_METAL]), (7, 255, metal));
        assert_eq!(v.fx[FX_COVERED].count_ones(), 3, "a corner of a 2 by 2 by 2 block: three faces against its neighbours");
    }
}
