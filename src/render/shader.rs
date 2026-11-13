//! The shaders' text and the data the scene's shader reads, laid out as
//! its WGSL structs are.

use bytemuck::{Pod, Zeroable};

use super::MAX_LIGHTS;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LightData {
    pub(super) pos_radius: [f32; 4],
    pub(super) color: [f32; 4],
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
    pub(super) lights: [LightData; MAX_LIGHTS],
}

/// The scene's shader, the faces' memory of their shadow in it
/// (`plain_shader` takes it out).
pub(super) const SHADER: &str = include_str!("../shader.wgsl");

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
    let mut out = String::with_capacity(src.len());
    let mut inside = false;
    for line in src.split_inclusive('\n') {
        match line.trim() {
            "// MEMORY {" => inside = true,
            "// } MEMORY" => inside = false,
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
        let plain = plain_shader(SHADER);
        for (name, src) in [("shader.wgsl", SHADER), ("shader.wgsl without the memory", plain.as_str()), ("post.wgsl", include_str!("../post.wgsl"))] {
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
        let module = naga::front::wgsl::parse_str(SHADER).unwrap();
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
        assert!(body.contains("fx.z") && body.contains("face_index(own)"), "{body}");
    }
}
