//! A limit of the renderer's a frame can go past without showing it: the
//! faces' memory of their shadow full (`SHADOW_FACES` faces at a shadow's
//! edge), where a face finds no room and is decided picture by picture
//! again (its edge may flicker). The shader counts such faces in a word
//! after the memory; the renderer reads it now and then without waiting
//! (`watch_memory`), or at once for `shot` (`memory_now`), and says it once
//! (`Renderer::warnings`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use super::Renderer;
use super::pipelines::Memory;
use super::shader::SHADOW_FACES;

/// Where the count of faces that found no room lies in the memory's buffer:
/// the word after its entries (three words each).
pub(super) const FULL_AT: u64 = SHADOW_FACES * 3 * 4;

/// The memory's buffer: its entries and the count after them.
pub(super) const MEMORY_BYTES: u64 = FULL_AT + 4;

/// How often a window reads the count: every this many pictures.
const EVERY: u32 = 60;

/// The count read back without waiting: copied after a picture, mapped
/// once that picture was sent, read once mapped.
pub(super) struct FullWatch {
    buffer: wgpu::Buffer,
    state: State,
}

enum State {
    Idle,
    Copied,
    /// 0 waiting, 1 mapped, 2 failed.
    Mapping(Arc<AtomicU8>),
}

impl FullWatch {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        Self { buffer: readback(device), state: State::Idle }
    }
}

fn readback(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("faces' memory full"), size: 4, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false })
}

/// What is said of the memory full, `count` faces having found no room.
pub fn memory_full(count: u32) -> String {
    format!("face memory: full ({SHADOW_FACES} faces at a shadow's edge, {count} more found no room): their shadows' edges may flicker as the sun moves")
}

impl Renderer {
    /// A step of reading the memory's count without waiting, once a frame
    /// (`render`): copied every `EVERY` pictures, mapped a frame later, read
    /// when mapped. Said once.
    pub(super) fn watch_memory(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.told_full {
            return;
        }
        let Memory::Ready { faces, full, .. } = &mut self.memory else { return };
        match &full.state {
            State::Idle if self.pictures.is_multiple_of(EVERY) => {
                encoder.copy_buffer_to_buffer(faces, FULL_AT, &full.buffer, 0, 4);
                full.state = State::Copied;
            }
            State::Idle => {}
            State::Copied => {
                let done = Arc::new(AtomicU8::new(0));
                let told = done.clone();
                full.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| told.store(if r.is_ok() { 1 } else { 2 }, Ordering::Release));
                full.state = State::Mapping(done);
            }
            State::Mapping(done) => {
                let _ = device.poll(wgpu::PollType::Poll);
                match done.load(Ordering::Acquire) {
                    0 => {}
                    1 => {
                        let count = u32::from_le_bytes(full.buffer.slice(..).get_mapped_range().expect("mapped")[..4].try_into().expect("four bytes"));
                        full.buffer.unmap();
                        full.state = State::Idle;
                        if count > 0 {
                            self.told_full = true;
                            self.said.push(memory_full(count));
                        }
                    }
                    _ => full.state = State::Idle,
                }
            }
        }
    }

    /// The memory's count read at once, waiting for the GPU (`shot`, after
    /// its pictures): said, if it is full and was not said yet.
    pub fn memory_now(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> {
        let Memory::Ready { faces, .. } = &self.memory else { return Ok(()) };
        if self.told_full {
            return Ok(());
        }
        let buffer = readback(device);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("faces' memory full") });
        encoder.copy_buffer_to_buffer(faces, FULL_AT, &buffer, 0, 4);
        queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("mapping the faces' memory's count"));
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let count = u32::from_le_bytes(slice.get_mapped_range()?[..4].try_into()?);
        if count > 0 {
            self.told_full = true;
            self.said.push(memory_full(count));
        }
        Ok(())
    }

    /// What frames went past since last asked (the faces' memory full),
    /// each said once: `shot` prints it, the IDE's PLAY puts it in OUTPUT.
    pub fn warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face finding no room in the memory is counted in the word right
    /// after its entries, which the memory's buffer has room for; only the
    /// memory's pipelines count; what is said names the limit and the count.
    #[test]
    fn a_full_memory_is_counted_where_it_is_read() {
        assert_eq!(FULL_AT, SHADOW_FACES * 12);
        assert_eq!(MEMORY_BYTES, FULL_AT + 4);
        let shader = super::super::shader::SHADER.as_str();
        let memory = &shader[shader.find("fn remembered(").unwrap()..];
        assert!(memory.contains("atomicAdd(&faces[FACES * 3u], 1u)"), "the shader counts at FULL_AT");
        assert!(!super::super::shader::plain_shader(shader).contains("atomicAdd"), "only in the memory's pipelines");
        let said = memory_full(12);
        assert!(said.contains("65536") && said.contains("12 more"), "{said}");
    }
}
