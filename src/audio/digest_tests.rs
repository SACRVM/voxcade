//! Every built-in sound, rendered, against a digest kept as text
//! (`tests/sounds.txt`, audit round 2): a change to the synth that makes a
//! sound differ is then deliberate. Samples are quantised to 12 bits and
//! hashed as integers (the synth runs on `libm`, so the system's maths
//! does not enter); renew after a change meant to alter sounds with
//! `VOXCADE_SOUNDS=renew cargo test --release built_in_sounds`, look at the
//! diff, and say in the commit what changed.

use std::fmt::Write as _;

fn digest(samples: &[f32]) -> (usize, u64) {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for s in samples {
        let q = (s * 2048.0).round() as i16;
        for b in q.to_le_bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    (samples.len(), hash)
}

fn table() -> String {
    let mut out = String::from("voxcade sounds 1\n# Each built-in sound: its samples and a hash of them quantised to 12 bits.\n# Made by `VOXCADE_SOUNDS=renew cargo test --release built_in_sounds`; see src/audio/digest_tests.rs.\n");
    for (kit, list) in [("engine", super::CORE_FILES), ("shooter", crate::shooter::SOUND_FILES), ("platformer", crate::platformer::SOUND_FILES)] {
        for (name, def) in super::sound::builtin(list) {
            let samples = match &def {
                super::SoundDef::Silent => Vec::new(),
                super::SoundDef::Synth(synth) => synth.render(),
            };
            let (n, hash) = digest(&samples);
            writeln!(out, "{kit}/{name} {n} {hash:016x}").unwrap();
        }
    }
    out
}

#[test]
fn built_in_sounds_render_as_recorded() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sounds.txt");
    let now = table();
    if std::env::var("VOXCADE_SOUNDS").is_ok_and(|v| v == "renew") {
        std::fs::write(&path, &now).unwrap();
        return;
    }
    let kept = std::fs::read_to_string(&path).unwrap_or_default();
    if kept != now {
        let differing: Vec<String> = now.lines().filter(|l| !kept.lines().any(|k| k == *l)).map(str::to_string).collect();
        panic!("built-in sounds changed from tests/sounds.txt; if meant, renew with VOXCADE_SOUNDS=renew. Now: {differing:#?}");
    }
}

#[test]
fn the_digest_sees_a_change() {
    let a = [0.1f32, -0.2, 0.3];
    let b = [0.1f32, -0.2, 0.31];
    assert_ne!(digest(&a), digest(&b));
    assert_eq!(digest(&a), digest(&a));
}
