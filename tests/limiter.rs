//! The limiter in the audio callback: it must not allocate there (a test
//! binary of its own, to count with an allocator of its own), and what it
//! costs (`cargo test --release --test limiter -- --ignored --nocapture`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use voxcade::audio::limiter::Limiter;
use voxcade::audio::{Mixer, SAMPLE_RATE, Synth, Wave};

/// Counts the allocations this thread makes while it counts.
struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.with(Cell::get) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(0));
    COUNTING.with(|c| c.set(true));
    f();
    COUNTING.with(|c| c.set(false));
    COUNT.with(Cell::get)
}

fn noise(frames: usize, amplitude: f32) -> Vec<f32> {
    let mut seed = 0x1234_5678u32;
    (0..frames * 2)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

/// Limiting, loud or quiet, in blocks of any size, allocates nothing; nor
/// does the mixer's mix around it, once its voices are playing.
#[test]
fn the_callbacks_path_allocates_nothing() {
    let mut lim = Limiter::new(SAMPLE_RATE);
    let mut loud = noise(SAMPLE_RATE as usize, 3.0);
    let mut quiet = noise(SAMPLE_RATE as usize, 0.1);
    let count = allocations(|| {
        for block in loud.chunks_mut(2 * 511).chain(quiet.chunks_mut(2)) {
            lim.process_interleaved_stereo(block);
        }
        lim.read_telemetry();
    });
    assert_eq!(count, 0, "the limiter allocated {count} times");

    let mut mixer = Mixer::default();
    let sound: std::sync::Arc<[f32]> = Synth { wave: Wave::Noise, hold: 2.0, volume: 1.0, ..Synth::default() }.render().into();
    for i in 0..8 {
        mixer.play(["a", "b", "c", "d"][i % 4], &sound, 1.0, 0.0);
    }
    let mut block = vec![0.0f32; 1024];
    let count = allocations(|| {
        for _ in 0..50 {
            mixer.mix(&mut block);
        }
    });
    assert_eq!(count, 0, "the mix allocated {count} times");
}

/// What a second of stereo costs: 60 s of noise at the ceiling and of
/// loud noise (limiting all the time), in the speaker's blocks.
#[test]
#[ignore = "a measurement: run with --ignored --nocapture in a release build"]
fn what_the_limiter_costs() {
    for (name, amplitude) in [("at the ceiling", 0.8913), ("6 dB over", 1.78)] {
        let input = noise(SAMPLE_RATE as usize * 60, amplitude);
        let mut best = f64::MAX;
        for _ in 0..5 {
            let mut buf = input.clone();
            let mut lim = Limiter::new(SAMPLE_RATE);
            let start = std::time::Instant::now();
            for block in buf.chunks_mut(2 * 512) {
                lim.process_interleaved_stereo(block);
            }
            best = best.min(start.elapsed().as_secs_f64());
            std::hint::black_box(&buf);
        }
        let per_second = best / 60.0;
        println!("{name}: {:.3} ms a second of stereo at {SAMPLE_RATE} Hz, {:.2} % of one core", per_second * 1000.0, per_second * 100.0);
    }
}
