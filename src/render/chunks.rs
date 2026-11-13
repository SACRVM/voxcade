//! The world's chunks as meshes on the GPU: built on every core, uploaded
//! in the order given.

use glam::{IVec3, Vec3};

use super::Renderer;
use crate::mesh::{self, FaceMask, Vertex};
use crate::world::{CHUNK, World};

/// A chunk's mesh on the GPU; its buffers may be bigger than the mesh.
pub(super) struct ChunkMesh {
    pub(super) vertices: wgpu::Buffer,
    pub(super) indices: wgpu::Buffer,
    pub(super) index_count: u32,
    pub(super) vertex_count: usize,
    pub(super) min: Vec3,
    pub(super) max: Vec3,
}

impl ChunkMesh {
    pub(super) fn vertex_bytes(&self) -> u64 {
        (self.vertex_count * size_of::<Vertex>()) as u64
    }

    pub(super) fn index_bytes(&self) -> u64 {
        u64::from(self.index_count) * 4
    }
}

/// Fewer chunks than this a thread are meshed on the calling thread: a
/// thread costs more to start than they take.
const MIN_CHUNKS_A_THREAD: usize = 2;

impl Renderer {
    /// Rebuilds the meshes of the chunks that changed; how many.
    pub fn update_chunks(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &mut World, mask: &FaceMask) -> usize {
        let dirty = world.take_dirty();
        self.remesh_all(device, queue, world, &dirty, mask);
        dirty.len()
    }

    /// Throws away all meshes and builds every chunk again.
    pub fn rebuild_all(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &mut World, mask: &FaceMask) {
        self.meshes.clear();
        world.take_dirty();
        let chunks = world.chunk_keys();
        self.remesh_all(device, queue, world, &chunks, mask);
    }

    /// Meshes `chunks`. A few on this thread; more spread over the CPU's
    /// cores (each chunk's mesh depends on the world alone, so they can be
    /// built at once: after a blast at 16-bit scale some fifty change in a
    /// tick), each core taking the next one not yet taken. They go to the
    /// GPU in the order given, so every run builds the same.
    fn remesh_all(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, world: &World, chunks: &[IVec3], mask: &FaceMask) {
        let threads = crate::par::cores().min(chunks.len() / MIN_CHUNKS_A_THREAD);
        if threads <= 1 {
            for &cc in chunks {
                let (mut vertices, mut indices) = (std::mem::take(&mut self.scratch_vertices), std::mem::take(&mut self.scratch_indices));
                mesh::mesh_chunk(world, cc, mask, &mut vertices, &mut indices);
                self.upload(device, queue, cc, &vertices, &indices);
                (self.scratch_vertices, self.scratch_indices) = (vertices, indices);
            }
            return;
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let mut built: Vec<(usize, Vec<Vertex>, Vec<u32>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut out = Vec::new();
                        loop {
                            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(&cc) = chunks.get(k) else { break };
                            let (mut vertices, mut indices) = (Vec::new(), Vec::new());
                            mesh::mesh_chunk(world, cc, mask, &mut vertices, &mut indices);
                            out.push((k, vertices, indices));
                        }
                        out
                    })
                })
                .collect();
            workers.into_iter().flat_map(|w| w.join().expect("meshing a chunk")).collect()
        });
        built.sort_unstable_by_key(|b| b.0);
        for (k, vertices, indices) in built {
            self.upload(device, queue, chunks[k], &vertices, &indices);
        }
    }

    /// Puts a chunk's mesh on the GPU (or drops it when it has none). A
    /// chunk that had a mesh keeps its buffers and the new mesh is written
    /// over the old one; when it outgrows them, the new ones get room to grow
    /// (a chunk that changed once, by a blast or debris settling, tends to
    /// change again). A quarter faster after a blast than new buffers each
    /// time, and the GPU memory does not churn.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cc: IVec3, vertices: &[Vertex], indices: &[u32]) {
        if indices.is_empty() {
            self.meshes.remove(&cc);
            return;
        }
        let (vertex_bytes, index_bytes): (&[u8], &[u8]) = (bytemuck::cast_slice(vertices), bytemuck::cast_slice(indices));
        let old = self.meshes.remove(&cc);
        let rebuilt = old.is_some();
        let buffer = |old: Option<wgpu::Buffer>, bytes: &[u8], usage, label| {
            let fits = old.filter(|b| b.size() >= bytes.len() as u64);
            let buffer = fits.unwrap_or_else(|| {
                // Exact for a chunk built the first time, half again for one rebuilt.
                let room = if rebuilt { bytes.len() / 2 } else { 0 };
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: ((bytes.len() + room) as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
                    usage: usage | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            queue.write_buffer(&buffer, 0, bytes);
            buffer
        };
        let (old_vertices, old_indices) = old.map(|m| (m.vertices, m.indices)).unzip();
        let min = (cc * CHUNK).as_vec3();
        let mesh = ChunkMesh {
            vertices: buffer(old_vertices, vertex_bytes, wgpu::BufferUsages::VERTEX, "chunk vertices"),
            indices: buffer(old_indices, index_bytes, wgpu::BufferUsages::INDEX, "chunk indices"),
            index_count: indices.len() as u32,
            vertex_count: vertices.len(),
            min,
            max: min + Vec3::splat(CHUNK as f32),
        };
        self.meshes.insert(cc, mesh);
    }
}
