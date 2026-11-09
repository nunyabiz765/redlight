mod app;
mod audio;

use backroom::demos;
use backroom::engine::render_song;
use backroom::pattern::{Slot, Song, VoiceId, VoiceSettings};
use backroom::storage;
use std::path::{Path, PathBuf};
use std::time::Instant;

slint::include_modules!();

const HELP: &str = "\
Backroom: a synthesized drum machine inspired by classic early-80s analog drum machines

USAGE
  backroom [--song NAME|FILE] [--play] [--muted]   Open the machine
  backroom --render SONG OUT.wav [options]         Render offline to a 24-bit WAV (no audio device)
  backroom --list                                  List demos and saved songs
  backroom --kit OUT.wav                           Render every voice once, in kit order

SONG is a demo name (or a unique part of one: boom, electro, slow, warehouse),
a saved song's name in the song folder (~/.local/share/backroom on Linux,
~/Library/Application Support/Backroom on macOS), or a path to a .json file.

RENDER OPTIONS
  --bars N      How many bars (default: the song's arrangement, A A A B)
  --rate N      Sample rate (default 48000)
  --tail S      Seconds of ring-out after the last bar (default 2)

KEYS (in the window)
  Space play/stop · Esc stop · A-D or 1-4 pattern slot · Up/Down tempo · Left/Right swing · Ctrl+S save
";

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("backroom: {msg}");
    std::process::exit(2);
}

/// A song by demo name, saved-file stem, or path. Returns (song, arrangement, file stem if saved).
fn resolve(spec: &str) -> Result<(Song, Vec<Slot>, Option<String>), String> {
    let p = Path::new(spec);
    let dir = storage::data_dir();
    let saved = dir.join(format!("{spec}.json"));
    let path = if p.extension().is_some_and(|x| x == "json") && p.exists() {
        Some(p.to_path_buf())
    } else if saved.exists() {
        Some(saved)
    } else {
        None
    };
    if let Some(path) = path {
        let song = storage::load(&path)?;
        let arr = default_arrangement(&song);
        let stem = (path.parent() == Some(dir.as_path()))
            .then(|| path.file_stem().unwrap().to_string_lossy().into_owned());
        return Ok((song, arr, stem));
    }
    if let Some(d) = demos::find(spec) {
        return Ok((d.song, d.arrangement, None));
    }
    Err(format!("no demo or song called '{spec}' (try --list)"))
}

/// A A A B when B has something in it, otherwise A A A A.
fn default_arrangement(song: &Song) -> Vec<Slot> {
    if song.state.patterns[1].is_empty() {
        vec![Slot::A; 4]
    } else {
        vec![Slot::A, Slot::A, Slot::A, Slot::B]
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--render" | "render") => render(&args[1..]),
        Some("--kit") => kit(args
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| die("--kit needs an output file"))),
        Some("--list" | "list") => {
            println!("Demos:");
            for d in demos::all() {
                println!(
                    "  {:<28} {} ({:.0} BPM, swing {:.0}%)",
                    d.file,
                    d.song.name,
                    d.song.state.tempo,
                    d.song.state.swing * 100.0
                );
            }
            let dir = storage::data_dir();
            println!("Saved in {}:", dir.display());
            for (stem, _) in storage::list(&dir) {
                println!("  {stem}");
            }
        }
        Some("--version" | "-V") => println!("backroom {}", env!("CARGO_PKG_VERSION")),
        Some("--help" | "-h" | "help") => print!("{HELP}"),
        _ => {
            let (mut song, mut file, mut muted, mut play) = (None, None, false, false);
            let mut it = args.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--song" => {
                        let n = it
                            .next()
                            .unwrap_or_else(|| die("--song needs a name or file"));
                        let (s, _, f) = resolve(n).unwrap_or_else(|e| die(e));
                        song = Some(s);
                        file = f;
                    }
                    "--muted" => muted = true,
                    "--play" => play = true,
                    other => die(format!("unknown argument '{other}'. Try --help.")),
                }
            }
            let song = song.unwrap_or_else(|| {
                let d = demos::all().swap_remove(0);
                file = Some(d.file.to_string());
                d.song
            });
            if let Err(e) = app::run(song, file, muted, play) {
                die(format!("couldn't open a window: {e}"));
            }
        }
    }
}

fn render(args: &[String]) {
    let (Some(spec), Some(out)) = (args.first(), args.get(1)) else {
        die("usage: backroom --render SONG OUT.wav [--bars N] [--rate N] [--tail S]");
    };
    let (song, mut arr, _) = resolve(spec).unwrap_or_else(|e| die(e));
    let (mut rate, mut tail, mut bars) = (48_000u32, 2.0f32, None);
    let mut it = args[2..].iter();
    let num = |v: Option<&String>, what: &str| -> f32 {
        v.and_then(|s| s.parse().ok())
            .unwrap_or_else(|| die(format!("{what} needs a number")))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bars" => bars = Some(num(it.next(), "--bars") as usize),
            "--rate" => rate = num(it.next(), "--rate") as u32,
            "--tail" => tail = num(it.next(), "--tail"),
            other => die(format!("unknown render option '{other}'")),
        }
    }
    if let Some(n) = bars {
        let base = arr.clone();
        arr = base.iter().cycle().take(n.max(1)).copied().collect();
    }
    let t0 = Instant::now();
    let samples = render_song(&song.state, &arr, rate as f32, tail);
    let took = t0.elapsed().as_secs_f32();
    let out = PathBuf::from(out);
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| die(e));
    }
    backroom::wav::write_file(&out, rate, 2, &samples).unwrap_or_else(|e| die(e));
    report(
        &out,
        &samples,
        rate,
        took,
        &format!("{} · {} bars", song.name, arr.len()),
    );
}

fn report(out: &Path, samples: &[f32], rate: u32, took: f32, what: &str) {
    let secs = samples.len() as f32 / 2.0 / rate as f32;
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    println!(
        "{}: {what} · {:.1} s · peak {:.1} dBFS · RMS {:.1} dBFS · rendered in {:.2} s ({:.0}x real time)",
        out.display(),
        secs,
        20.0 * peak.max(1e-9).log10(),
        20.0 * rms.max(1e-9).log10(),
        took,
        secs / took.max(1e-6)
    );
}

/// Every voice once at its default settings, accented, half a second apart
/// (the kick gets two seconds of room): for judging the sounds on their own.
fn kit(out: PathBuf) {
    use backroom::engine::{Engine, Event};
    use backroom::pattern::State;
    let sr = 48_000u32;
    let mut st = State::default();
    st.voices = [VoiceSettings {
        level: 0.9,
        ..VoiceSettings::default()
    }; 11];
    st.master.reverb = 0.0;
    st.master.wobble = 0.0;
    st.master.hiss = 0.0;
    let mut e = Engine::new(sr as f32);
    e.handle(Event::Load(st));
    let mut samples = Vec::new();
    let t0 = Instant::now();
    for v in VoiceId::ALL {
        e.handle(Event::Audition(v));
        let secs = if v == VoiceId::Kick || v == VoiceId::OpenHat {
            1.2
        } else {
            0.6
        };
        let mut buf = vec![0.0; (secs * sr as f32) as usize * 2];
        e.render(&mut buf);
        samples.extend_from_slice(&buf);
    }
    let took = t0.elapsed().as_secs_f32();
    backroom::wav::write_file(&out, sr, 2, &samples).unwrap_or_else(|e| die(e));
    report(&out, &samples, sr, took, "kit roll");
}
