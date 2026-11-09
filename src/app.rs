//! Window glue. Every edit becomes a `Command`: applied to the UI's copy of
//! the state, then sent to the audio thread, which applies the same command
//! to its own copy. Nothing here touches pattern fields directly.

use crate::audio::{self, send, Audio, Events, Shared};
use crate::{AppWindow, KnobData, VoiceRow};
use backroom::engine::Event;
use backroom::pattern::{
    Command, MasterParam, Slot, Song, State, VoiceId, VoiceParam, STEPS, SWING_MAX, SWING_MIN,
    TEMPO_MAX, TEMPO_MIN,
};
use backroom::storage;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct App {
    song: Song,
    /// File stem in the data folder, if the song came from or went to one.
    file: Option<String>,
    /// The slot being edited (and the one queued to play).
    slot: Slot,
    copied: Option<Slot>,
    files: Vec<(String, PathBuf)>,
    events: Option<Events>,
    rows: Rc<VecModel<VoiceRow>>,
    accent: Rc<VecModel<bool>>,
    master: Rc<VecModel<KnobData>>,
    /// Knob values as they were when the song was loaded: double-click resets to these.
    loaded: State,
}

fn pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

fn voice_knob(v: VoiceId, p: VoiceParam, value: f32) -> KnobData {
    let (lo, hi) = p.range();
    let display = match p {
        VoiceParam::Tune if value.abs() < 0.005 => "0".into(),
        VoiceParam::Tune => format!("{:+.0}", value * 100.0),
        _ => pct(value),
    };
    let label = match p {
        VoiceParam::Tune => "TUNE",
        VoiceParam::Decay => "DECAY",
        VoiceParam::Level => "LEVEL",
        VoiceParam::Tone => v.tone_label(),
    };
    KnobData {
        pid: (v.index() * 4 + p as usize) as i32,
        label: format!("{} {label}", v.label()).into(),
        value: (value - lo) / (hi - lo),
        display: display.into(),
    }
}

fn master_knob(p: MasterParam, value: f32) -> KnobData {
    KnobData {
        pid: p as i32,
        label: p.label().into(),
        value,
        display: pct(value).into(),
    }
}

impl App {
    fn send(&self, e: Event) {
        if let Some(ev) = &self.events {
            send(ev, e);
        }
    }

    /// The single edit path. Returns false (and changes nothing) if the command is rejected.
    fn apply(&mut self, ui: &AppWindow, cmd: Command) -> bool {
        match self.song.state.apply(cmd) {
            Ok(()) => {
                self.send(Event::Cmd(cmd));
                self.refresh(ui);
                true
            }
            Err(e) => {
                ui.set_status_note(format!("Edit rejected: {e}").into());
                false
            }
        }
    }

    fn row(&self, v: VoiceId) -> VoiceRow {
        let st = &self.song.state;
        let g = st.patterns[self.slot.index()];
        let vs = st.voices[v.index()];
        VoiceRow {
            label: v.label().into(),
            tone_label: v.tone_label().into(),
            muted: vs.mute,
            steps: ModelRc::new(VecModel::from(
                (0..STEPS).map(|s| g.get(v, s)).collect::<Vec<_>>(),
            )),
            knobs: ModelRc::new(VecModel::from(
                VoiceParam::ALL
                    .iter()
                    .map(|&p| voice_knob(v, p, vs.get(p)))
                    .collect::<Vec<_>>(),
            )),
        }
    }

    fn refresh(&self, ui: &AppWindow) {
        use slint::Model;
        let st = &self.song.state;
        for v in VoiceId::ALL {
            let new = self.row(v);
            // Only touch rows that changed, so a knob drag doesn't rebuild the grid.
            let same = self.rows.row_data(v.index()).is_some_and(|old| {
                old.muted == new.muted
                    && old.steps.iter().eq(new.steps.iter())
                    && old
                        .knobs
                        .iter()
                        .map(|k| k.value)
                        .eq(new.knobs.iter().map(|k| k.value))
            });
            if !same {
                self.rows.set_row_data(v.index(), new);
            }
        }
        let g = st.patterns[self.slot.index()];
        for s in 0..STEPS {
            if self.accent.row_data(s) != Some(g.accent_at(s)) {
                self.accent.set_row_data(s, g.accent_at(s));
            }
        }
        for p in MasterParam::ALL {
            self.master
                .set_row_data(p as usize, master_knob(p, st.master.get(p)));
        }
        ui.set_tempo(st.tempo);
        ui.set_swing(st.swing * 100.0);
        ui.set_slot(self.slot.index() as i32);
        ui.set_copied(self.copied.map_or(-1, |s| s.index() as i32));
        let used: Vec<bool> = st.patterns.iter().map(|g| !g.is_empty()).collect();
        ui.set_slot_used(ModelRc::new(VecModel::from(used)));
        ui.set_song_name(self.song.name.clone().into());
        ui.set_song_file(self.file.clone().unwrap_or_default().into());
    }

    fn load(&mut self, ui: &AppWindow, song: Song, file: Option<String>) {
        self.song = song;
        self.loaded = self.song.state;
        self.file = file;
        self.send(Event::Load(self.song.state));
        self.refresh(ui);
    }

    fn select(&mut self, ui: &AppWindow, slot: Slot) {
        self.slot = slot;
        self.send(Event::Select(slot));
        self.refresh(ui);
    }
}

#[cfg(unix)]
fn timestamp() -> String {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}{:02}{:02}-{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        )
    }
}

/// Opens a folder or URL with the desktop's default handler.
fn open_external(target: &std::ffi::OsStr) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(not(any(target_os = "macos", windows)))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(target).spawn();
}

pub fn run(
    start: Song,
    start_file: Option<String>,
    muted: bool,
    autoplay: bool,
) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let dir = storage::data_dir();
    let dir_label = storage::display_dir(&dir);
    ui.set_data_dir(dir_label.clone().into());
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.on_open_source(|| open_external("https://github.com/nunyabiz765/backroom".as_ref()));
    if let Err(e) = storage::seed_demos(&dir) {
        ui.set_status_note(format!("Can't write demos to {}: {e}", dir.display()).into());
    }

    let audio: Result<Audio, String> = audio::start(muted);
    let (events, audio_desc, shared): (Option<Events>, String, Option<Arc<Shared>>) = match audio {
        Ok(a) => {
            let desc = format!(
                "{} · {} · {:.1} kHz{}",
                a.host,
                a.device,
                a.sample_rate as f32 / 1000.0,
                if muted { " · MUTED" } else { "" }
            );
            let a = Box::leak(Box::new(a));
            (Some(a.events.clone()), desc, Some(a.shared.clone()))
        }
        Err(e) => (None, format!("No audio output: {e}"), None),
    };

    let blank_row = VoiceRow::default();
    let rows = Rc::new(VecModel::from(vec![blank_row; VoiceId::ALL.len()]));
    let accent = Rc::new(VecModel::from(vec![false; STEPS]));
    let master = Rc::new(VecModel::from(
        MasterParam::ALL
            .iter()
            .map(|&p| master_knob(p, 0.0))
            .collect::<Vec<_>>(),
    ));
    ui.set_rows(rows.clone().into());
    ui.set_accent(accent.clone().into());
    ui.set_master(master.clone().into());

    let app = Rc::new(RefCell::new(App {
        song: start.clone(),
        file: None,
        slot: Slot::A,
        copied: None,
        files: storage::list(&dir),
        events,
        rows,
        accent,
        master,
        loaded: start.state,
    }));
    app.borrow_mut().load(&ui, start, start_file);
    ui.set_status_audio(audio_desc.clone().into());
    ui.set_hint("Hover a knob to see its value".into());

    // Each handler borrows the app, does one thing, and lets go.
    macro_rules! on {
        ($setter:ident, |$app:ident, $ui:ident $(, $arg:ident : $t:ty)*| $body:block) => {{
            let app = app.clone();
            let w = ui.as_weak();
            ui.$setter(move |$($arg: $t),*| {
                let $ui = w.unwrap();
                #[allow(unused_mut)]
                let mut $app = app.borrow_mut();
                $body
            });
        }};
    }

    on!(on_toggle_step, |a, ui, voice: i32, step: i32| {
        let Some(v) = VoiceId::from_index(voice as usize) else {
            return;
        };
        let on = !a.song.state.patterns[a.slot.index()].get(v, step as usize);
        let slot = a.slot;
        a.apply(
            &ui,
            Command::SetStep {
                slot,
                voice: v,
                step: step as u8,
                on,
            },
        );
    });
    on!(on_toggle_accent, |a, ui, step: i32| {
        let on = !a.song.state.patterns[a.slot.index()].accent_at(step as usize);
        let slot = a.slot;
        a.apply(
            &ui,
            Command::SetAccent {
                slot,
                step: step as u8,
                on,
            },
        );
    });
    on!(on_toggle_mute, |a, ui, voice: i32| {
        let Some(v) = VoiceId::from_index(voice as usize) else {
            return;
        };
        let muted = !a.song.state.voices[v.index()].mute;
        a.apply(&ui, Command::SetMute { voice: v, muted });
    });
    on!(on_audition, |a, _ui, voice: i32| {
        if let Some(v) = VoiceId::from_index(voice as usize) {
            a.send(Event::Audition(v));
        }
    });
    let voice_pid = |pid: i32| -> Option<(VoiceId, VoiceParam)> {
        Some((
            VoiceId::from_index(pid as usize / 4)?,
            *VoiceParam::ALL.get(pid as usize % 4)?,
        ))
    };
    on!(on_vknob, |a, ui, pid: i32, norm: f32| {
        let Some((v, p)) = voice_pid(pid) else { return };
        let (lo, hi) = p.range();
        let mut value = lo + norm.clamp(0.0, 1.0) * (hi - lo);
        if p == VoiceParam::Tune && value.abs() < 0.03 {
            value = 0.0; // a detent at home pitch
        }
        if a.apply(
            &ui,
            Command::SetVoiceParam {
                voice: v,
                param: p,
                value,
            },
        ) {
            let k = voice_knob(v, p, value);
            ui.set_hint(format!("{}  {}", k.label, k.display).into());
        }
    });
    on!(on_vknob_reset, |a, ui, pid: i32| {
        let Some((v, p)) = voice_pid(pid) else { return };
        let value = a.loaded.voices[v.index()].get(p);
        a.apply(
            &ui,
            Command::SetVoiceParam {
                voice: v,
                param: p,
                value,
            },
        );
    });
    on!(on_mknob, |a, ui, pid: i32, norm: f32| {
        let Some(&p) = MasterParam::ALL.get(pid as usize) else {
            return;
        };
        if a.apply(
            &ui,
            Command::SetMaster {
                param: p,
                value: norm.clamp(0.0, 1.0),
            },
        ) {
            ui.set_hint(format!("{}  {}", p.label(), pct(norm)).into());
        }
    });
    on!(on_mknob_reset, |a, ui, pid: i32| {
        let Some(&p) = MasterParam::ALL.get(pid as usize) else {
            return;
        };
        let value = a.loaded.master.get(p);
        a.apply(&ui, Command::SetMaster { param: p, value });
    });
    {
        let w = ui.as_weak();
        ui.on_hover_knob(move |k| {
            w.unwrap()
                .set_hint(format!("{}  {}", k.label, k.display).into());
        });
    }
    on!(on_set_tempo, |a, ui, bpm: f32| {
        a.apply(
            &ui,
            Command::SetTempo {
                bpm: bpm.round().clamp(TEMPO_MIN, TEMPO_MAX),
            },
        );
    });
    on!(on_set_swing, |a, ui, pct: f32| {
        let swing = (pct.round() / 100.0).clamp(SWING_MIN, SWING_MAX);
        a.apply(&ui, Command::SetSwing { swing });
    });
    {
        let app = app.clone();
        let w = ui.as_weak();
        ui.on_play_toggle(move || {
            let ui = w.unwrap();
            let a = app.borrow();
            let playing = !ui.get_playing();
            a.send(if playing { Event::Play } else { Event::Stop });
            ui.set_playing(playing);
        });
    }
    {
        let app = app.clone();
        let w = ui.as_weak();
        ui.on_stop(move || {
            app.borrow().send(Event::Stop);
            w.unwrap().set_playing(false);
        });
    }
    on!(on_select_slot, |a, ui, i: i32| {
        if let Some(s) = Slot::from_index(i as usize) {
            a.select(&ui, s);
        }
    });
    on!(on_copy_slot, |a, ui| {
        a.copied = if a.copied == Some(a.slot) {
            None
        } else {
            Some(a.slot)
        };
        let note = match a.copied {
            Some(s) => format!(
                "Copied pattern {}. Pick another slot and press PASTE.",
                s.letter()
            ),
            None => "Copy cancelled.".into(),
        };
        ui.set_status_note(note.into());
        a.refresh(&ui);
    });
    on!(on_paste_slot, |a, ui| {
        let Some(from) = a.copied else {
            ui.set_status_note(
                "Nothing copied yet: press COPY on the pattern you want first.".into(),
            );
            return;
        };
        let to = a.slot;
        if a.apply(&ui, Command::CopyPattern { from, to }) {
            ui.set_status_note(format!("Pasted {} into {}.", from.letter(), to.letter()).into());
        }
    });
    on!(on_clear_slot, |a, ui| {
        let slot = a.slot;
        if a.apply(&ui, Command::ClearPattern { slot }) {
            ui.set_status_note(format!("Cleared pattern {}.", slot.letter()).into());
        }
    });
    on!(on_new_song, |a, ui| {
        let song = Song {
            name: format!("Beat {}", timestamp()),
            state: State::default(),
        };
        a.load(&ui, song, None);
        a.select(&ui, Slot::A);
        ui.set_status_note(format!("New song. SAVE writes it to {dir_label}.").into());
    });
    {
        let app = app.clone();
        let w = ui.as_weak();
        let dir = dir.clone();
        ui.on_refresh_files(move || {
            let mut a = app.borrow_mut();
            a.files = storage::list(&dir);
            let names: Vec<SharedString> = a.files.iter().map(|(s, _)| s.as_str().into()).collect();
            w.unwrap().set_files(ModelRc::new(VecModel::from(names)));
        });
    }
    on!(on_open_file, |a, ui, i: i32| {
        let Some((stem, path)) = a.files.get(i as usize).cloned() else {
            return;
        };
        match storage::load(&path) {
            Ok(song) => {
                let name = song.name.clone();
                a.load(&ui, song, Some(stem));
                a.select(&ui, Slot::A);
                ui.set_status_note(format!("Opened {name}.").into());
            }
            Err(e) => ui.set_status_note(format!("Can't open: {e}").into()),
        }
    });
    {
        let dir = dir.clone();
        on!(on_save, |a, ui| {
            let stem = match &a.file {
                Some(s) => s.clone(),
                None => storage::free_stem(&dir, &a.song.name),
            };
            match storage::save(&dir, &stem, &a.song) {
                Ok(p) => {
                    a.file = Some(stem);
                    a.loaded = a.song.state;
                    ui.set_status_note(format!("Saved {}", p.display()).into());
                    a.refresh(&ui);
                }
                Err(e) => ui.set_status_note(format!("Can't save: {e}").into()),
            }
        });
    }
    {
        let dir = dir.clone();
        on!(on_save_copy, |a, ui| {
            let base = a
                .song
                .name
                .trim_end_matches(char::is_numeric)
                .trim_end()
                .to_string();
            a.song.name = format!("{base} {}", timestamp());
            let stem = storage::free_stem(&dir, &a.song.name);
            match storage::save(&dir, &stem, &a.song) {
                Ok(p) => {
                    a.file = Some(stem);
                    a.loaded = a.song.state;
                    ui.set_status_note(format!("Saved a copy: {}", p.display()).into());
                    a.refresh(&ui);
                }
                Err(e) => ui.set_status_note(format!("Can't save: {e}").into()),
            }
        });
    }
    {
        let dir = dir.clone();
        ui.on_open_folder(move || {
            let _ = std::fs::create_dir_all(&dir);
            open_external(dir.as_os_str());
        });
    }

    if autoplay {
        app.borrow().send(Event::Play);
        ui.set_playing(true);
    }

    // 30 fps: playhead, slot light, meter, status.
    let timer = slint::Timer::default();
    {
        let w = ui.as_weak();
        let mut level = 0f32;
        let mut load_shown = 0f32;
        let mut last_status = Instant::now() - Duration::from_secs(1);
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(33),
            move || {
                let ui = w.unwrap();
                let Some(shared) = &shared else { return };
                let step = shared.step.load(Relaxed);
                if ui.get_playhead() != step {
                    ui.set_playhead(step);
                }
                ui.set_playing_slot(shared.slot.load(Relaxed) as i32);
                let peak = f32::from_bits(shared.peak.swap(0, Relaxed));
                level = peak.max(level * 0.85);
                ui.set_level(level);
                let load = f32::from_bits(shared.load.swap(0, Relaxed));
                load_shown = load.max(load_shown * 0.97);
                if last_status.elapsed() > Duration::from_millis(500) {
                    last_status = Instant::now();
                    let xruns = shared.xruns.load(Relaxed);
                    ui.set_status_audio(
                        format!(
                            "{audio_desc} · {} frames · DSP {:.0}%{}",
                            shared.frames.load(Relaxed),
                            load_shown * 100.0,
                            if xruns > 0 {
                                format!(" · {xruns} xruns")
                            } else {
                                String::new()
                            }
                        )
                        .into(),
                    );
                }
            },
        );
    }

    ui.run()
}
