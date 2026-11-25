//! `voxcade mcp --music`: Voxcade's MML synth for an agent without a game
//! folder: read the language (`music_words`), check a song reads
//! (`music_check`), render it to a WAV (`music_render`, the code of
//! `voxcade music FILE.mml`) and look at it (`music_picture`, a
//! spectrogram, for an agent that cannot hear). Every path is the
//! caller's own, used as given (absolute, or relative to the folder the
//! server runs in); nothing is written anywhere else and nothing is left
//! behind: songs are rendered in memory.

use std::path::Path;

use serde_json::{Value, json};

use super::{arg, base64, prop, text};
use crate::audio::music::{LANGUAGE, Samples, Song, SUM_WORDS};
use crate::audio::{SAMPLE_RATE, picture, words};

/// What the handshake says.
pub(super) const INSTRUCTIONS: &str = "Voxcade's MML synth without a game: SID-like voices (pulse, saw, triangle, sine, noise, singing voices), instruments of layers with unison and LFOs, filters, drive, samples, written as text. \
    Work in a loop: music_words first (syntax, instruments, voices, example), write MML, music_check until it reads, music_render to a WAV, then music_picture to look at it, since you cannot hear: a spectrogram says what is playing when. \
    Tell the user the WAV's path so they can listen. Every path is the caller's own choice (absolute, or relative to where this server runs); nothing is written anywhere else.";

/// The most MML text taken, bytes, and the longest song rendered, seconds.
const MOST_TEXT: u64 = 4 << 20;
const LONGEST_SONG: f32 = 1800.0;

/// A short complete song: a layered kick, a wobble bass, a pad of five
/// saws and a lead. The test plays it.
pub const EXAMPLE: &str = "; A small song: every line starting with ; is a comment.
#title Example
#tempo 120
#meter 4

; a kick of two layers: a falling sine and a click of noise
@kick sine pitch_start=160 pitch_decay=0.04 curve=exp decay=0.3 sustain=0 release=0.02 drive=6
+ noise decay=0.03 sustain=0 volume=0.4 filter=highpass cutoff=3000
; a wobble bass: a low-pass saw whose cutoff an LFO moves each eighth
@bass saw filter=lowpass cutoff=400 resonance=0.5 lfo_beats=0.5 lfo_cutoff=24 lfo_wave=triangle release=0.1
; a pad: five saws together
@pad saw unison=5 detune=0.2 attack=0.2 release=0.6 volume=0.5 filter=lowpass cutoff=1800
@lead square duty=0.25 vibrato=0.3 vibrato_speed=5 vibrato_delay=0.2 release=0.1 volume=0.7

K: @kick o2 [c4 c4 c4 c4]4
B: @bass o2 l8 [c c c c g g a a]4
P: @pad o3 c1 g1 a1 f1
L: @lead o5 l8 [e g a g e d c d]4
";

pub(super) fn tools() -> Value {
    let mml = prop("string", "The song as MML text (exactly one of mml and file)");
    let file = prop("string", "A song's file, .mml (exactly one of mml and file)");
    let samples = prop("string", "A folder of WAV files for the wave `sample` (a folder holding a samples/ folder is read there, as a game's); for a file, the samples/ folder beside it or beside its music/ folder is the default");
    json!([
        {
            "name": "music_check",
            "description": "Reads MML and answers either the mistake as the parser says it, with its line, or the song's facts: tempo, meter, bars, length, #loop, each channel's notes and length, the instruments with their waves, and warnings (channels of unequal length).",
            "inputSchema": { "type": "object", "properties": { "mml": mml, "file": file, "samples": samples } },
        },
        {
            "name": "music_render",
            "description": "Renders MML to a WAV (44.1 kHz stereo 16 bit) at out, as voxcade music FILE.mml does: through the limiter at the engine's defaults, a #loop song written to the loop's end and round again into its first bars. Answers the path, length, peak, loudness and the warnings, or the mistake with its line. Tell the user the path so they can listen.",
            "inputSchema": { "type": "object", "properties": {
                "mml": mml, "file": file, "samples": samples,
                "out": prop("string", "Where the WAV goes: a path ending in .wav, its folders made"),
            }, "required": ["out"] },
        },
        {
            "name": "music_picture",
            "description": "A spectrogram PNG, to look at what you cannot hear: time left to right, pitch up on a log axis (30 Hz to 16 kHz), loudness from dark to light, of a WAV file or of MML rendered in memory (exactly one of wav, mml, file), optionally only seconds from to to. Written to out and returned as an image; the text answer gives the axes.",
            "inputSchema": { "type": "object", "properties": {
                "wav": prop("string", "A WAV file to draw (PCM 16 or 24 bit or 32-bit float, mono or stereo, at most 30 minutes and 200 MB)"),
                "mml": mml, "file": file, "samples": samples,
                "out": prop("string", "Where the PNG goes: a path ending in .png, its folders made"),
                "from": prop("number", "The stretch's start, seconds (default 0)"),
                "to": prop("number", "The stretch's end, seconds (default the end)"),
            }, "required": ["out"] },
        },
        {
            "name": "music_words",
            "description": "The reference for writing MML without the Voxcade repo. Without a topic an index; topics: syntax (the language), instruments (every instrument word with unit, range, default and meaning, the + layer and = sum lines), voices (the singing voices' phonemes), example (a short complete song).",
            "inputSchema": { "type": "object", "properties": { "topic": prop("string", "syntax, instruments, voices or example") } },
        },
    ])
}

/// The tools' names, in the order they are listed.
#[cfg(test)]
pub(super) const NAMES: [&str; 4] = ["music_check", "music_render", "music_picture", "music_words"];

pub(super) fn call(name: &str, args: &Value) -> Result<Vec<Value>, String> {
    if !args.is_object() {
        return Err("the arguments are an object".into());
    }
    match name {
        "music_check" => check(args),
        "music_render" => render(args),
        "music_picture" => look(args),
        "music_words" => words_of(args),
        other => Err(format!("no tool {other} (music_check, music_render, music_picture, music_words)")),
    }
}

/// A string argument: None if not given, a mistake naming it if it is
/// something else.
fn string<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => arg(args, key).map(Some).ok_or_else(|| format!("{key} is a string")),
    }
}

/// A number argument, likewise.
fn number(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| format!("{key} is a number of seconds")),
    }
}

/// The one of `keys` the call gives, a mistake if none or several.
fn one_of<'a>(args: &'a Value, keys: &[&'static str]) -> Result<(&'static str, &'a str), String> {
    let mut given = Vec::new();
    for &key in keys {
        if let Some(v) = string(args, key)? {
            given.push((key, v));
        }
    }
    match given.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("give one of {}", keys.join(", "))),
        _ => Err(format!("give only one of {}, not {}", keys.join(", "), given.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(" and "))),
    }
}

/// The song's text and what a mistake calls it, from `mml` or `file`.
fn song_text(kind: &str, value: &str) -> Result<(String, String), String> {
    if kind == "mml" {
        if value.len() as u64 > MOST_TEXT {
            return Err(format!("the MML is {} bytes, {} at most", value.len(), MOST_TEXT));
        }
        return Ok((value.to_string(), "the MML".into()));
    }
    let size = std::fs::metadata(value).map_err(|e| format!("{value}: {e}"))?.len();
    if size > MOST_TEXT {
        return Err(format!("{value}: {size} bytes, a song is {MOST_TEXT} at most"));
    }
    let bytes = std::fs::read(value).map_err(|e| format!("{value}: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{value}: not text (MML is UTF-8 text)"))?;
    Ok((text, value.to_string()))
}

/// The recordings the call names, else those beside a song's file.
fn samples_of(args: &Value, file: Option<&str>) -> Result<Samples, String> {
    let said = |e: anyhow::Error| format!("{e:#}");
    match (string(args, "samples")?, file) {
        (Some(dir), _) => Samples::read_dir(Path::new(dir)).map_err(said),
        (None, Some(file)) => match crate::headless::samples_folder_beside(Path::new(file)) {
            Some(dir) => crate::headless::samples_in(&dir).map_err(said),
            None => Ok(Samples::default()),
        },
        (None, None) => Ok(Samples::default()),
    }
}

/// The call's song read, with the text, its name and the recordings.
fn read(args: &Value) -> Result<(String, String, Samples, Song), String> {
    let (kind, value) = one_of(args, &["mml", "file"])?;
    let (text, name) = song_text(kind, value)?;
    let samples = samples_of(args, (kind == "file").then_some(value))?;
    let song = crate::headless::read_song(&text, &name, &samples).map_err(|e| format!("{e:#}"))?;
    let length = song.length as f32 / SAMPLE_RATE as f32;
    if length > LONGEST_SONG {
        return Err(format!("{name}: the song is {length:.0} seconds long, {LONGEST_SONG:.0} at most"));
    }
    Ok((text, name, samples, song))
}

/// Seconds as a clock shows them: 2:40.5.
fn clock(seconds: f32) -> String {
    let tenths = (seconds * 10.0).round() as u64;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

fn check(args: &Value) -> Result<Vec<Value>, String> {
    let (_, name, _, song) = read(args)?;
    let length = song.length as f32 / SAMPLE_RATE as f32;
    let mut lines = vec![format!("{name} reads.")];
    if !song.title.trim().is_empty() {
        lines.push(format!("title: {}", song.title.trim()));
    }
    lines.push(format!("tempo {} beats a minute, {} beats a bar, {} bars, {length:.2} s ({})", song.tempo, song.meter, song.bars, clock(length)));
    lines.push(match (song.round, song.round_seconds()) {
        (Some((from, to)), Some((a, b))) => format!("#loop: bar {from} ({}) to {} ({}) goes round", clock(a), to.map_or_else(|| "the song's end".to_string(), |to| format!("bar {to}")), clock(b)),
        _ => "no #loop: the song plays once".into(),
    });
    lines.push("channels:".into());
    let ends = song.channel_lengths();
    for ((letter, notes), (_, end)) in song.note_counts().into_iter().zip(ends) {
        lines.push(format!("  {letter}: {notes} notes, {end:.2} s long"));
    }
    lines.push("instruments:".into());
    for (name, wave, layers, unison) in song.instrument_list() {
        let more = [(layers > 1).then(|| format!("{layers} layers")), (unison > 1).then(|| format!("unison {unison}"))];
        let more: Vec<String> = more.into_iter().flatten().collect();
        lines.push(format!("  @{name}: {wave}{}", if more.is_empty() { String::new() } else { format!(" ({})", more.join(", ")) }));
    }
    match crate::headless::unequal_channels(&song) {
        Some(warning) => lines.push(warning),
        None => lines.push("no warnings".into()),
    }
    Ok(vec![text(&lines.join("\n"))])
}

/// A path ending in `ending` (any case), not an existing folder.
fn out_path<'a>(args: &'a Value, ending: &str) -> Result<&'a Path, String> {
    let out = string(args, "out")?.ok_or("out is needed: where the file goes")?;
    let path = Path::new(out);
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ending)) {
        return Err(format!("out is {out}: a path ending in .{ending} (nothing else is written over)"));
    }
    if path.is_dir() {
        return Err(format!("out is {out}: a folder, not a file"));
    }
    Ok(path)
}

fn render(args: &Value) -> Result<Vec<Value>, String> {
    let out = out_path(args, "wav")?;
    let (text, name, samples, _) = read(args)?;
    let done = crate::headless::render_song(&text, &name, None, &samples, None).map_err(|e| format!("{e:#}"))?;
    crate::headless::write_wav(out, &done.samples, 2).map_err(|e| format!("{e:#}"))?;
    let limited = done.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let mut lines = vec![format!("wrote {}", out.display())];
    lines.push(format!("{:.2} s ({}) of '{}', which is {:.2} s long; peak {:.2} before the limiter, {limited:.2} after; {}", done.seconds, clock(done.seconds), done.title, done.length, done.peak, done.limiting));
    lines.extend(done.notes);
    Ok(vec![text_of(&lines)])
}

fn text_of(lines: &[String]) -> Value {
    text(&lines.join("\n"))
}

fn look(args: &Value) -> Result<Vec<Value>, String> {
    let out = out_path(args, "png")?;
    let (from, to) = (number(args, "from")?, number(args, "to")?);
    let (kind, value) = one_of(args, &["wav", "mml", "file"])?;
    let (rate, mono, what) = if kind == "wav" {
        let size = std::fs::metadata(value).map_err(|e| format!("{value}: {e}"))?.len();
        if size > picture::MOST_BYTES {
            return Err(format!("{value}: {} MB, a picture is of {} MB at most", size >> 20, picture::MOST_BYTES >> 20));
        }
        let bytes = std::fs::read(value).map_err(|e| format!("{value}: {e}"))?;
        let (rate, mono) = picture::read_wav(value, &bytes).map_err(|e| format!("{e:#}"))?;
        (rate, mono, value.to_string())
    } else {
        let (text, name, samples, _) = read(args)?;
        let done = crate::headless::render_song(&text, &name, None, &samples, None).map_err(|e| format!("{e:#}"))?;
        let mono = done.samples.as_chunks::<2>().0.iter().map(|[l, r]| 0.5 * (l + r)).collect();
        (SAMPLE_RATE, mono, format!("{name} rendered"))
    };
    let seen = picture::spectrogram(&mono, rate, from, to).map_err(|e| format!("{e:#}"))?;
    let png = crate::headless::png_bytes(seen.width, seen.height, &seen.rgba).map_err(|e| format!("{e:#}"))?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(out, &png).map_err(|e| format!("writing {}: {e}", out.display()))?;
    let said = format!("wrote {}: the spectrogram of {what} ({:.2} s at {rate} Hz); {}", out.display(), mono.len() as f64 / f64::from(rate), seen.says());
    Ok(vec![text(&said), json!({ "type": "image", "data": base64(&png), "mimeType": "image/png" })])
}

/// What an instrument word's value is counted in, as a word.
fn unit_name(unit: words::Unit) -> &'static str {
    use words::Unit::*;
    match unit {
        Seconds => "seconds",
        Hz => "Hz",
        Semitones => "semitones",
        Share => "a share, 0 to 1",
        Choice => "one of a few words",
        YesNo => "yes or no",
        SemitoneList => "semitones as a list, 0,4,7",
        Decibels => "dB",
        Bits => "bits",
        Note => "a note, like c4",
        Name => "a name",
        Count => "a whole number",
        Beats => "beats of the song",
    }
}

/// Every instrument word of the register, a line each.
fn instruments() -> String {
    let mut lines = vec![
        "An instrument is a line @name WAVE word=value ..., WAVE one of square (or pulse), saw, triangle, sine, noise, sample, voice, singer; a channel plays it after @name. Written only what differs from the defaults; a value out of its range is a mistake saying the range.".to_string(),
        String::new(),
        "The words (name: unit, range, default; meaning):".into(),
    ];
    for w in words::WORDS {
        let Some(u) = &w.mml else { continue };
        let range = u.range.map_or(String::new(), |(lo, hi)| format!(", {lo} to {hi}"));
        lines.push(format!("{}: {}{range}, default {}; {}", w.name, unit_name(u.unit), u.default, u.meaning));
    }
    lines.push(String::new());
    lines.push("Layers: lines starting with + right under an @ line are more layers of that instrument (four with its own line at most): each a wave with words of its own (all the words above but sidechain, sidechain_release and duck, which are the whole note's and go on the @ line), every note sounding them all, each with its own envelope, oscillator and filters.".into());
    lines.push(format!("Sum: one line starting with = after the layers is what they go through together; it takes only {}.", SUM_WORDS.join(", ")));
    lines.push("Example: @kick sine pitch_start=160 pitch_decay=0.04 curve=exp decay=0.3 sustain=0 drive=6, then + noise decay=0.03 sustain=0 volume=0.4 filter=highpass cutoff=3000, then = drive=3.".into());
    lines.join("\n")
}

fn words_of(args: &Value) -> Result<Vec<Value>, String> {
    let topic = string(args, "topic")?.map(|t| t.trim().to_lowercase());
    let said = match topic.as_deref() {
        None | Some("") | Some("index") => "MML, Voxcade's music text. Topics (music_words topic=...):\n\
            syntax: the language: settings, instruments, channels, notes, lengths, repeats, macros, loops, layers, samples, singing\n\
            instruments: every instrument word with its unit, range, default and meaning, and the + layer and = sum lines\n\
            voices: the phonemes the singing voices take\n\
            example: one short complete song (layered kick, wobble bass, unison pad, lead)\n\
            The loop: write MML, music_check until it reads, music_render to a WAV, music_picture to look at it."
            .to_string(),
        Some("syntax") => format!("A song is text: lines of settings (#), instruments (@, with + and = lines), macros ($) and channels (a letter and a colon, then notes).\n\n{LANGUAGE}"),
        Some("instruments") => instruments(),
        Some("voices") => format!(
            "Two voices sing, instead of a wave: `singer` (the clear one, made for words to be understood) and `voice` (rougher, for shouts): @vox singer ... Then a quoted text before a note is the syllable it sings, in phonemes split by spaces: \"s ae ng\" e4 (consonants, one vowel, consonants). A note without a text holds the vowel before it. The phonemes: {}. The syntax topic says how each sounds and the rest.",
            crate::audio::music::phonemes()
        ),
        Some("example") => EXAMPLE.to_string(),
        Some(other) => return Err(format!("no topic {other} (syntax, instruments, voices, example)")),
    };
    Ok(vec![text(&said)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    /// The call as a client makes it: the tool's answer or its mistake.
    fn ask(name: &str, args: Value) -> Result<Vec<Value>, String> {
        call(name, &args)
    }

    fn said(r: &Result<Vec<Value>, String>) -> String {
        match r {
            Ok(c) => c.iter().filter_map(|v| v["text"].as_str()).collect::<Vec<_>>().join("\n"),
            Err(e) => e.clone(),
        }
    }

    /// A WAV's seconds, from its header: the data's bytes over the rate
    /// and the frames.
    fn wav_seconds(bytes: &[u8]) -> f64 {
        let size = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as f64;
        size / (f64::from(SAMPLE_RATE) * 4.0)
    }

    const GOOD: &str = "#tempo 120\n@lead square\nA: @lead o4 [c4 e4 g4 e4]2\nB: @lead o3 [c4 g4]4\n";

    #[test]
    fn check_says_the_facts_or_the_line() {
        let ok = ask("music_check", json!({ "mml": GOOD }));
        let text = said(&ok);
        for want in ["reads", "tempo 120", "2 bars", "A: 8 notes", "B: 8 notes", "@lead: square", "no warnings"] {
            assert!(text.contains(want), "{want} in {text}");
        }
        let bad = ask("music_check", json!({ "mml": "#tempo 120\n@lead square\nA: @lead o4 c4 e4\nB: @lead o4 c4 q q\n" }));
        assert!(bad.is_err() && said(&bad).contains("line 4"), "{}", said(&bad));
        let bad = ask("music_check", json!({ "mml": "#tempo 9999\nA: c\n" }));
        assert!(said(&bad).contains("line 1") && said(&bad).contains("#tempo"), "{}", said(&bad));
        // Channels of unequal length are said.
        let uneven = said(&ask("music_check", json!({ "mml": "A: c1 c1\nB: c1\n" })));
        assert!(uneven.contains("warning: channels differ in length"), "{uneven}");
        // Layers and unison show.
        let layered = said(&ask("music_check", json!({ "mml": EXAMPLE })));
        assert!(layered.contains("@pad: saw (unison 5)") && layered.contains("@kick: sine (2 layers)"), "{layered}");
    }

    #[test]
    fn render_writes_a_wav_as_long_as_check_says() {
        let dir = Scratch::new("mcp-music");
        let out = dir.path().join("deep/in/song.wav");
        let checked = said(&ask("music_check", json!({ "mml": EXAMPLE })));
        let length: f64 = checked.split("bars, ").nth(1).and_then(|s| s.split(" s").next()).unwrap().parse().unwrap();
        let answer = ask("music_render", json!({ "mml": EXAMPLE, "out": out.display().to_string() }));
        let text = said(&answer);
        assert!(text.contains("wrote") && text.contains("peak"), "{text}");
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert!((wav_seconds(&bytes) - length).abs() < 0.02, "{} against {length}", wav_seconds(&bytes));
        // The same bytes as voxcade music writes for the same text.
        let file = dir.path().join("e.mml");
        std::fs::write(&file, EXAMPLE).unwrap();
        let again = dir.path().join("again.wav");
        crate::headless::song(&file, None, &Samples::default(), &again, None).unwrap();
        assert_eq!(std::fs::read(&again).unwrap(), bytes, "one renderer");
        // A song of a file, rendered from it.
        let from_file = dir.path().join("f.wav");
        ask("music_render", json!({ "file": file.display().to_string(), "out": from_file.display().to_string() })).unwrap();
        assert_eq!(std::fs::read(&from_file).unwrap(), bytes);
        // A broken song names its line and writes nothing.
        let none = dir.path().join("none.wav");
        let bad = ask("music_render", json!({ "mml": "A: c4 zz\n", "out": none.display().to_string() }));
        assert!(said(&bad).contains("line 1") || said(&bad).contains("unknown command"), "{}", said(&bad));
        assert!(!none.exists());
    }

    #[test]
    fn a_loop_song_is_written_round_as_voxcade_music_does() {
        let dir = Scratch::new("mcp-music-loop");
        let mml = "#tempo 120\n#loop 2 3\n@a square\nA: @a [c4 e4 g4 e4]4\n";
        let out = dir.path().join("l.wav");
        let text = said(&ask("music_render", json!({ "mml": mml, "out": out.display().to_string() })));
        assert!(text.contains("the loop: bar 2"), "{text}");
        let check = said(&ask("music_check", json!({ "mml": mml })));
        assert!(check.contains("#loop: bar 2"), "{check}");
    }

    #[test]
    fn a_sample_is_found_in_the_folder_named() {
        let dir = Scratch::new("mcp-music-samples");
        let mml = "@s sample name=thump\nA: @s c4 c4\n";
        let missing = said(&ask("music_check", json!({ "mml": mml })));
        assert!(missing.contains("thump"), "{missing}");
        // A folder of WAVs, as a game's drums are.
        let wav = dir.path().join("wavs");
        std::fs::create_dir_all(&wav).unwrap();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/drums/samples/thump.wav");
        std::fs::copy(source, wav.join("thump.wav")).unwrap();
        let found = ask("music_check", json!({ "mml": mml, "samples": wav.display().to_string() }));
        assert!(said(&found).contains("@s: sample"), "{}", said(&found));
        assert!(said(&ask("music_check", json!({ "mml": mml, "samples": dir.path().join("none").display().to_string() }))).contains("thump"));
    }

    /// The picture is a PNG of the size it says, bright where a 440 Hz
    /// note lies.
    #[test]
    fn the_picture_of_a_note_is_bright_at_its_pitch() {
        let dir = Scratch::new("mcp-music-picture");
        let out = dir.path().join("p/note.png");
        let mml = "@s sine\nA: @s o4 a1 a1\n";
        let answer = ask("music_picture", json!({ "mml": mml, "out": out.display().to_string() })).unwrap();
        assert_eq!(answer[1]["type"], "image");
        let bytes = std::fs::read(&out).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        let facts = answer[0]["text"].as_str().unwrap();
        assert!(facts.contains(&format!("{}x{} px", info.width, info.height)), "{facts}");
        assert!(info.width <= 1600 && info.height <= 400);
        let rows = 380.0;
        let row = |f: f64| (((16000.0f64 / 30.0).ln() - (f / 30.0).ln()) / (16000.0f64 / 30.0).ln() * rows).round() as usize;
        let at = |f: f64| {
            let (x, y) = (26 + (info.width as usize - 26) / 2, row(f));
            let i = (y * info.width as usize + x) * 4;
            u32::from(buf[i]) + u32::from(buf[i + 1]) + u32::from(buf[i + 2])
        };
        assert!(at(440.0) > at(220.0) + 150 && at(440.0) > at(880.0) + 150, "{} {} {}", at(220.0), at(440.0), at(880.0));
        // Of a WAV file the same, and of a stretch of it.
        let wav = dir.path().join("note.wav");
        ask("music_render", json!({ "mml": mml, "out": wav.display().to_string() })).unwrap();
        let from_wav = dir.path().join("w.png");
        let a = said(&ask("music_picture", json!({ "wav": wav.display().to_string(), "out": from_wav.display().to_string(), "from": 1.0, "to": 3.0 })));
        assert!(a.contains("from 1.00 s to 3.00 s"), "{a}");
    }

    #[test]
    fn hostile_input_is_a_mistake_naming_what_is_wrong() {
        let dir = Scratch::new("mcp-music-hostile");
        let d = |n: &str| dir.path().join(n).display().to_string();
        let both = ask("music_check", json!({ "mml": GOOD, "file": "x.mml" }));
        assert!(said(&both).contains("only one of mml, file"), "{}", said(&both));
        assert!(said(&ask("music_check", json!({}))).contains("give one of mml, file"));
        assert!(said(&ask("music_check", json!({ "mml": 5 }))).contains("mml is a string"));
        assert!(said(&ask("music_check", json!({ "file": d("none.mml") }))).contains("none.mml"));
        let wrong = ask("music_render", json!({ "mml": GOOD, "out": d("x.txt") }));
        assert!(said(&wrong).contains("ending in .wav"), "{}", said(&wrong));
        assert!(said(&ask("music_render", json!({ "mml": GOOD }))).contains("out is needed"));
        assert!(said(&ask("music_render", json!({ "mml": GOOD, "out": 3 }))).contains("out is a string"));
        assert!(said(&ask("music_picture", json!({ "mml": GOOD, "out": d("x.wav") }))).contains("ending in .png"));
        assert!(said(&ask("music_picture", json!({ "mml": GOOD, "wav": "a.wav", "out": d("x.png") }))).contains("only one of wav, mml"));
        assert!(said(&ask("music_picture", json!({ "mml": GOOD, "out": d("x.png"), "from": "soon" }))).contains("from is a number"));
        assert!(said(&ask("music_picture", json!({ "mml": GOOD, "out": d("x.png"), "from": 9.0, "to": 2.0 }))).contains("from 9"));
        // Not a WAV, cut short, an enormous header.
        std::fs::write(d("junk.wav"), b"RIFFjunk").unwrap();
        let junk = ask("music_picture", json!({ "wav": d("junk.wav"), "out": d("j.png") }));
        assert!(junk.is_err(), "{junk:?}");
        let mut cut = b"RIFF\xff\xff\xff\xffWAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x88\x58\x01\0\x02\0\x10\0data\xff\xff\xff\x7f".to_vec();
        cut.extend([0u8; 100]);
        std::fs::write(d("cut.wav"), &cut).unwrap();
        let cut = ask("music_picture", json!({ "wav": d("cut.wav"), "out": d("c.png") }));
        assert!(said(&cut).contains("cut short"), "{}", said(&cut));
        assert!(!dir.path().join("c.png").exists());
        // A studio (instruments, no channel) is no song to render.
        let studio = said(&ask("music_check", json!({ "mml": "@a square\n" })));
        assert!(studio.contains("no song"), "{studio}");
        assert!(said(&ask("music_words", json!({ "topic": "love" }))).contains("no topic love"));
        assert!(call("music_nothing", &json!({})).is_err());
        assert!(call("music_check", &json!([1])).is_err());
    }

    #[test]
    fn the_words_hold_the_register_and_an_example_that_plays() {
        let index = said(&ask("music_words", json!({})));
        for topic in ["syntax", "instruments", "voices", "example"] {
            assert!(index.contains(topic), "{index}");
        }
        let instruments = said(&ask("music_words", json!({ "topic": "instruments" })));
        for w in words::WORDS.iter().filter(|w| w.mml.is_some()) {
            assert!(instruments.contains(&format!("\n{}: ", w.name)), "the word {} is not listed", w.name);
        }
        assert!(instruments.contains("lfo_cutoff") && instruments.contains("Sum:") && instruments.contains("Layers:"));
        let syntax = said(&ask("music_words", json!({ "topic": "syntax" })));
        assert!(syntax.contains("#loop") && syntax.contains("#tempo") && syntax.contains("#layer") && syntax.contains("[ ... ]3"), "{syntax}");
        assert!(said(&ask("music_words", json!({ "topic": "voices" }))).contains(&crate::audio::music::phonemes()));
        let example = said(&ask("music_words", json!({ "topic": "example" })));
        assert_eq!(example, EXAMPLE);
        let checked = said(&ask("music_check", json!({ "mml": example })));
        assert!(checked.contains("reads") && checked.contains("no warnings") && checked.contains("8.00 s"), "{checked}");
        // And plays: the loudest of it is not silence.
        let rendered = crate::headless::render_song(&example, "example", None, &Samples::default(), None).unwrap();
        assert!(rendered.peak > 0.05 && rendered.notes.iter().all(|n| !n.starts_with("warning")), "{} {:?}", rendered.peak, rendered.notes);
    }

    #[test]
    fn the_tools_are_listed_by_name() {
        let listed: Vec<String> = tools().as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(listed, NAMES);
    }
}
