//! The pattern model: four 16-step pattern slots, the kit settings and the
//! master section, plus the command functions every edit goes through.
//!
//! Design rules (they are what makes the v0.2 "Sal" producer easy):
//! - `State` is plain `Copy` data, so the UI thread and the audio thread each
//!   own a copy and stay in sync by applying the same `Command`s.
//! - Every edit is a `Command` and goes through [`State::apply`], which
//!   validates it. The UI never pokes at fields directly. A future AI tool
//!   layer deserializes its tool calls straight into `Command`s (they are
//!   serde types, tagged by `"cmd"`), and can [`State::preview`] a batch to
//!   show the result as ghost notes before the user accepts it.
//! - Songs save as readable JSON: every track is a 16-character step string
//!   like `"x... x... x... x..."`, which humans and language models can both
//!   read and write.

use serde::de::{self, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

pub const STEPS: usize = 16;
pub const VOICES: usize = 11;
pub const SLOTS: usize = 4;
pub const TEMPO_MIN: f32 = 60.0;
pub const TEMPO_MAX: f32 = 180.0;
pub const SWING_MIN: f32 = 0.50;
pub const SWING_MAX: f32 = 0.75;
pub const FORMAT: &str = "redlight-song";
/// What 0.1.0 wrote, when the app was still called Backroom. Read, never written.
pub const LEGACY_FORMAT: &str = "backroom-song";
pub const VERSION: u32 = 1;

// ------------------------------------------------------------------ ids ---

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceId {
    Kick,
    Snare,
    Clap,
    ClosedHat,
    OpenHat,
    LowTom,
    MidTom,
    HighTom,
    Rim,
    Cowbell,
    Conga,
}

impl VoiceId {
    pub const ALL: [VoiceId; VOICES] = [
        VoiceId::Kick,
        VoiceId::Snare,
        VoiceId::Clap,
        VoiceId::ClosedHat,
        VoiceId::OpenHat,
        VoiceId::LowTom,
        VoiceId::MidTom,
        VoiceId::HighTom,
        VoiceId::Rim,
        VoiceId::Cowbell,
        VoiceId::Conga,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Option<VoiceId> {
        Self::ALL.get(i).copied()
    }

    /// The JSON key (same as the serde name).
    pub fn key(self) -> &'static str {
        match self {
            VoiceId::Kick => "kick",
            VoiceId::Snare => "snare",
            VoiceId::Clap => "clap",
            VoiceId::ClosedHat => "closed_hat",
            VoiceId::OpenHat => "open_hat",
            VoiceId::LowTom => "low_tom",
            VoiceId::MidTom => "mid_tom",
            VoiceId::HighTom => "high_tom",
            VoiceId::Rim => "rim",
            VoiceId::Cowbell => "cowbell",
            VoiceId::Conga => "conga",
        }
    }

    pub fn from_key(k: &str) -> Option<VoiceId> {
        Self::ALL.iter().copied().find(|v| v.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            VoiceId::Kick => "KICK",
            VoiceId::Snare => "SNARE",
            VoiceId::Clap => "CLAP",
            VoiceId::ClosedHat => "CL HAT",
            VoiceId::OpenHat => "OP HAT",
            VoiceId::LowTom => "LO TOM",
            VoiceId::MidTom => "MID TOM",
            VoiceId::HighTom => "HI TOM",
            VoiceId::Rim => "RIM",
            VoiceId::Cowbell => "COWBELL",
            VoiceId::Conga => "CONGA",
        }
    }

    /// What the fourth knob does on this voice.
    pub fn tone_label(self) -> &'static str {
        match self {
            VoiceId::Kick => "DRIVE",
            VoiceId::Snare => "SNAPPY",
            VoiceId::Clap => "SPREAD",
            VoiceId::ClosedHat | VoiceId::OpenHat | VoiceId::Cowbell => "BRIGHT",
            VoiceId::LowTom | VoiceId::MidTom | VoiceId::HighTom | VoiceId::Conga => "SKIN",
            VoiceId::Rim => "CLICK",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Slot {
    A,
    B,
    C,
    D,
}

impl Slot {
    pub const ALL: [Slot; SLOTS] = [Slot::A, Slot::B, Slot::C, Slot::D];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(i: usize) -> Option<Slot> {
        Self::ALL.get(i).copied()
    }
    pub fn letter(self) -> char {
        (b'A' + self as u8) as char
    }
    pub fn from_letter(c: char) -> Option<Slot> {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.letter() == c.to_ascii_uppercase())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceParam {
    /// -1..1 (about an octave each way on most voices; 0 is the voice's home pitch).
    Tune,
    /// 0..1.
    Decay,
    /// 0..1.
    Level,
    /// 0..1: drive on the kick, snappy on the snare, brightness on metals...
    Tone,
}

impl VoiceParam {
    pub const ALL: [VoiceParam; 4] = [
        VoiceParam::Tune,
        VoiceParam::Decay,
        VoiceParam::Level,
        VoiceParam::Tone,
    ];
    pub fn range(self) -> (f32, f32) {
        match self {
            VoiceParam::Tune => (-1.0, 1.0),
            _ => (0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterParam {
    Volume,
    /// Tape saturation.
    Drive,
    /// Tape wow and flutter.
    Wobble,
    /// Tape head wear: top-end roll-off.
    Age,
    Hiss,
    /// Sampler-style bit and rate reduction.
    Crush,
    /// Reverb return level.
    Reverb,
    /// Reverb size (decay time).
    Room,
}

impl MasterParam {
    pub const ALL: [MasterParam; 8] = [
        MasterParam::Volume,
        MasterParam::Drive,
        MasterParam::Wobble,
        MasterParam::Age,
        MasterParam::Hiss,
        MasterParam::Crush,
        MasterParam::Reverb,
        MasterParam::Room,
    ];
    pub fn label(self) -> &'static str {
        match self {
            MasterParam::Volume => "VOLUME",
            MasterParam::Drive => "DRIVE",
            MasterParam::Wobble => "WOBBLE",
            MasterParam::Age => "AGE",
            MasterParam::Hiss => "HISS",
            MasterParam::Crush => "CRUSH",
            MasterParam::Reverb => "REVERB",
            MasterParam::Room => "ROOM",
        }
    }
}

// --------------------------------------------------------------- values ---

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSettings {
    pub tune: f32,
    pub decay: f32,
    pub level: f32,
    pub tone: f32,
    pub mute: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        VoiceSettings {
            tune: 0.0,
            decay: 0.5,
            level: 0.8,
            tone: 0.5,
            mute: false,
        }
    }
}

impl VoiceSettings {
    pub fn get(&self, p: VoiceParam) -> f32 {
        match p {
            VoiceParam::Tune => self.tune,
            VoiceParam::Decay => self.decay,
            VoiceParam::Level => self.level,
            VoiceParam::Tone => self.tone,
        }
    }
    fn slot(&mut self, p: VoiceParam) -> &mut f32 {
        match p {
            VoiceParam::Tune => &mut self.tune,
            VoiceParam::Decay => &mut self.decay,
            VoiceParam::Level => &mut self.level,
            VoiceParam::Tone => &mut self.tone,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Master {
    pub volume: f32,
    pub drive: f32,
    pub wobble: f32,
    pub age: f32,
    pub hiss: f32,
    pub crush: f32,
    pub reverb: f32,
    pub room: f32,
}

impl Default for Master {
    fn default() -> Self {
        Master {
            volume: 0.7,
            drive: 0.35,
            wobble: 0.1,
            age: 0.2,
            hiss: 0.1,
            crush: 0.0,
            reverb: 0.15,
            room: 0.3,
        }
    }
}

impl Master {
    pub fn get(&self, p: MasterParam) -> f32 {
        *match p {
            MasterParam::Volume => &self.volume,
            MasterParam::Drive => &self.drive,
            MasterParam::Wobble => &self.wobble,
            MasterParam::Age => &self.age,
            MasterParam::Hiss => &self.hiss,
            MasterParam::Crush => &self.crush,
            MasterParam::Reverb => &self.reverb,
            MasterParam::Room => &self.room,
        }
    }
    fn slot(&mut self, p: MasterParam) -> &mut f32 {
        match p {
            MasterParam::Volume => &mut self.volume,
            MasterParam::Drive => &mut self.drive,
            MasterParam::Wobble => &mut self.wobble,
            MasterParam::Age => &mut self.age,
            MasterParam::Hiss => &mut self.hiss,
            MasterParam::Crush => &mut self.crush,
            MasterParam::Reverb => &mut self.reverb,
            MasterParam::Room => &mut self.room,
        }
    }
}

// ----------------------------------------------------------------- grid ---

/// One pattern: a 16-bit step mask per voice plus the accent row. Bit `n` is step `n` (0-based).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Grid {
    pub tracks: [u16; VOICES],
    pub accent: u16,
}

impl Grid {
    pub fn get(&self, v: VoiceId, step: usize) -> bool {
        self.tracks[v.index()] >> step & 1 == 1
    }
    pub fn accent_at(&self, step: usize) -> bool {
        self.accent >> step & 1 == 1
    }
    pub fn is_empty(&self) -> bool {
        self.accent == 0 && self.tracks.iter().all(|&t| t == 0)
    }
    /// Builds a grid from `(voice, step string)` rows; `None` is the accent row.
    /// Panics on a malformed string: for the built-in demos.
    pub fn from_rows(rows: &[(Option<VoiceId>, &str)]) -> Grid {
        let mut g = Grid::default();
        for (v, s) in rows {
            let bits = parse_steps(s).unwrap_or_else(|e| panic!("{v:?}: {e}"));
            match v {
                Some(v) => g.tracks[v.index()] = bits,
                None => g.accent = bits,
            }
        }
        g
    }
}

/// `"x... x... x... x..."`: `x`, `X` or `o` is a hit, `.` or `-` a rest; spaces
/// and `|` are ignored. Exactly 16 steps.
pub fn parse_steps(s: &str) -> Result<u16, String> {
    let mut bits = 0u16;
    let mut n = 0;
    for c in s.chars() {
        match c {
            ' ' | '|' | '_' => continue,
            'x' | 'X' | 'o' | 'O' => {
                if n < STEPS {
                    bits |= 1 << n;
                }
            }
            '.' | '-' => {}
            other => return Err(format!("unexpected '{other}' in step string \"{s}\"")),
        }
        n += 1;
    }
    if n != STEPS {
        return Err(format!("step string \"{s}\" has {n} steps, want {STEPS}"));
    }
    Ok(bits)
}

pub fn format_steps(bits: u16) -> String {
    let mut s = String::with_capacity(19);
    for i in 0..STEPS {
        if i > 0 && i % 4 == 0 {
            s.push(' ');
        }
        s.push(if bits >> i & 1 == 1 { 'x' } else { '.' });
    }
    s
}

impl Serialize for Grid {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut m = ser.serialize_map(Some(VOICES + 1))?;
        m.serialize_entry("accent", &format_steps(self.accent))?;
        for v in VoiceId::ALL {
            m.serialize_entry(v.key(), &format_steps(self.tracks[v.index()]))?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for Grid {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let rows = BTreeMap::<String, String>::deserialize(de)?;
        let mut g = Grid::default();
        for (k, s) in rows {
            let bits = parse_steps(&s).map_err(de::Error::custom)?;
            if k == "accent" {
                g.accent = bits;
            } else {
                let v = VoiceId::from_key(&k)
                    .ok_or_else(|| de::Error::custom(format!("unknown voice '{k}'")))?;
                g.tracks[v.index()] = bits;
            }
        }
        Ok(g)
    }
}

// ---------------------------------------------------------------- state ---

/// Everything the engine plays. `Copy`, so it crosses to the audio thread by value.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct State {
    pub tempo: f32,
    /// 0.50 = straight, 0.75 = hard shuffle (fraction of each 8th the first 16th takes).
    pub swing: f32,
    pub patterns: [Grid; SLOTS],
    pub voices: [VoiceSettings; VOICES],
    pub master: Master,
}

impl Default for State {
    fn default() -> Self {
        State {
            tempo: 96.0,
            swing: 0.54,
            patterns: [Grid::default(); SLOTS],
            voices: [VoiceSettings::default(); VOICES],
            master: Master::default(),
        }
    }
}

/// A named state: what's saved to and loaded from disk.
#[derive(Clone, PartialEq, Debug)]
pub struct Song {
    pub name: String,
    pub state: State,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SongFile {
    format: String,
    version: u32,
    name: String,
    tempo: f32,
    swing: f32,
    patterns: BTreeMap<Slot, Grid>,
    #[serde(default)]
    voices: BTreeMap<VoiceId, VoiceSettings>,
    #[serde(default)]
    master: Master,
}

impl Song {
    pub fn to_json(&self) -> String {
        let s = &self.state;
        let file = SongFile {
            format: FORMAT.into(),
            version: VERSION,
            name: self.name.clone(),
            tempo: s.tempo,
            swing: s.swing,
            patterns: Slot::ALL
                .iter()
                .map(|&sl| (sl, s.patterns[sl.index()]))
                .collect(),
            voices: VoiceId::ALL
                .iter()
                .map(|&v| (v, s.voices[v.index()]))
                .collect(),
            master: s.master,
        };
        serde_json::to_string_pretty(&file).expect("song serializes")
    }

    /// Parses and validates a song. Everything a file sets goes through the
    /// same range checks as the edit commands.
    pub fn from_json(text: &str) -> Result<Song, String> {
        let f: SongFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if f.format != FORMAT && f.format != LEGACY_FORMAT {
            return Err(format!("not a Red Light song (format \"{}\")", f.format));
        }
        if f.version > VERSION {
            return Err(format!(
                "song is format version {}, this Red Light reads up to {VERSION}",
                f.version
            ));
        }
        let mut st = State::default();
        st.set_tempo(f.tempo).map_err(|e| e.to_string())?;
        st.set_swing(f.swing).map_err(|e| e.to_string())?;
        for (sl, g) in f.patterns {
            st.patterns[sl.index()] = g;
        }
        for (v, vs) in f.voices {
            for p in VoiceParam::ALL {
                st.set_voice_param(v, p, vs.get(p))
                    .map_err(|e| e.to_string())?;
            }
            st.set_mute(v, vs.mute).map_err(|e| e.to_string())?;
        }
        for p in MasterParam::ALL {
            st.set_master(p, f.master.get(p))
                .map_err(|e| e.to_string())?;
        }
        Ok(Song {
            name: f.name,
            state: st,
        })
    }
}

// ------------------------------------------------------------- commands ---

/// Every edit. Steps are 0-based (0..=15). Serialized with a `"cmd"` tag, e.g.
/// `{"cmd":"set_step","slot":"A","voice":"kick","step":0,"on":true}`.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SetStep {
        slot: Slot,
        voice: VoiceId,
        step: u8,
        on: bool,
    },
    SetAccent {
        slot: Slot,
        step: u8,
        on: bool,
    },
    SetTempo {
        bpm: f32,
    },
    SetSwing {
        swing: f32,
    },
    SetVoiceParam {
        voice: VoiceId,
        param: VoiceParam,
        value: f32,
    },
    SetMute {
        voice: VoiceId,
        muted: bool,
    },
    SetMaster {
        param: MasterParam,
        value: f32,
    },
    ClearPattern {
        slot: Slot,
    },
    CopyPattern {
        from: Slot,
        to: Slot,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditError {
    StepOutOfRange(u8),
    OutOfRange {
        what: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },
    NotFinite(&'static str),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EditError::StepOutOfRange(s) => {
                write!(f, "step {s} is out of range (steps are 0..=15)")
            }
            EditError::OutOfRange {
                what,
                value,
                min,
                max,
            } => {
                write!(f, "{what} {value} is out of range ({min}..={max})")
            }
            EditError::NotFinite(what) => write!(f, "{what} must be a finite number"),
        }
    }
}

impl std::error::Error for EditError {}

fn check(what: &'static str, v: f32, min: f32, max: f32) -> Result<f32, EditError> {
    if !v.is_finite() {
        return Err(EditError::NotFinite(what));
    }
    // A hair of slack for float round-trips through the UI.
    if v < min - 1e-4 || v > max + 1e-4 {
        return Err(EditError::OutOfRange {
            what,
            value: v,
            min,
            max,
        });
    }
    Ok(v.clamp(min, max))
}

fn check_step(step: u8) -> Result<usize, EditError> {
    if (step as usize) < STEPS {
        Ok(step as usize)
    } else {
        Err(EditError::StepOutOfRange(step))
    }
}

fn set_bit(mask: &mut u16, step: usize, on: bool) {
    if on {
        *mask |= 1 << step;
    } else {
        *mask &= !(1 << step);
    }
}

/// A step that a previewed edit would add or remove: what the UI draws as a ghost note.
/// `voice: None` is the accent row.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Ghost {
    pub slot: Slot,
    pub voice: Option<VoiceId>,
    pub step: u8,
    pub added: bool,
}

impl State {
    pub fn set_step(
        &mut self,
        slot: Slot,
        voice: VoiceId,
        step: u8,
        on: bool,
    ) -> Result<(), EditError> {
        let s = check_step(step)?;
        set_bit(
            &mut self.patterns[slot.index()].tracks[voice.index()],
            s,
            on,
        );
        Ok(())
    }

    pub fn set_accent(&mut self, slot: Slot, step: u8, on: bool) -> Result<(), EditError> {
        let s = check_step(step)?;
        set_bit(&mut self.patterns[slot.index()].accent, s, on);
        Ok(())
    }

    pub fn set_tempo(&mut self, bpm: f32) -> Result<(), EditError> {
        self.tempo = check("tempo", bpm, TEMPO_MIN, TEMPO_MAX)?;
        Ok(())
    }

    pub fn set_swing(&mut self, swing: f32) -> Result<(), EditError> {
        self.swing = check("swing", swing, SWING_MIN, SWING_MAX)?;
        Ok(())
    }

    pub fn set_voice_param(
        &mut self,
        voice: VoiceId,
        param: VoiceParam,
        value: f32,
    ) -> Result<(), EditError> {
        let (lo, hi) = param.range();
        let what = match param {
            VoiceParam::Tune => "tune",
            VoiceParam::Decay => "decay",
            VoiceParam::Level => "level",
            VoiceParam::Tone => "tone",
        };
        *self.voices[voice.index()].slot(param) = check(what, value, lo, hi)?;
        Ok(())
    }

    pub fn set_mute(&mut self, voice: VoiceId, muted: bool) -> Result<(), EditError> {
        self.voices[voice.index()].mute = muted;
        Ok(())
    }

    pub fn set_master(&mut self, param: MasterParam, value: f32) -> Result<(), EditError> {
        *self.master.slot(param) = check("master value", value, 0.0, 1.0)?;
        Ok(())
    }

    pub fn clear_pattern(&mut self, slot: Slot) -> Result<(), EditError> {
        self.patterns[slot.index()] = Grid::default();
        Ok(())
    }

    pub fn copy_pattern(&mut self, from: Slot, to: Slot) -> Result<(), EditError> {
        self.patterns[to.index()] = self.patterns[from.index()];
        Ok(())
    }

    /// The one entry point for edits. On error the state is unchanged.
    pub fn apply(&mut self, cmd: Command) -> Result<(), EditError> {
        match cmd {
            Command::SetStep {
                slot,
                voice,
                step,
                on,
            } => self.set_step(slot, voice, step, on),
            Command::SetAccent { slot, step, on } => self.set_accent(slot, step, on),
            Command::SetTempo { bpm } => self.set_tempo(bpm),
            Command::SetSwing { swing } => self.set_swing(swing),
            Command::SetVoiceParam {
                voice,
                param,
                value,
            } => self.set_voice_param(voice, param, value),
            Command::SetMute { voice, muted } => self.set_mute(voice, muted),
            Command::SetMaster { param, value } => self.set_master(param, value),
            Command::ClearPattern { slot } => self.clear_pattern(slot),
            Command::CopyPattern { from, to } => self.copy_pattern(from, to),
        }
    }

    /// Applies a batch all-or-nothing. On error returns the failing command's index.
    pub fn apply_all(&mut self, cmds: &[Command]) -> Result<(), (usize, EditError)> {
        let mut next = *self;
        for (i, &c) in cmds.iter().enumerate() {
            next.apply(c).map_err(|e| (i, e))?;
        }
        *self = next;
        Ok(())
    }

    /// What a batch would do, without doing it: the resulting state and the
    /// steps it adds or removes (ghost notes to draw until the user accepts).
    pub fn preview(&self, cmds: &[Command]) -> Result<(State, Vec<Ghost>), (usize, EditError)> {
        let mut next = *self;
        next.apply_all(cmds)?;
        let ghosts = ghost_diff(self, &next);
        Ok((next, ghosts))
    }
}

pub fn ghost_diff(before: &State, after: &State) -> Vec<Ghost> {
    let mut out = Vec::new();
    for sl in Slot::ALL {
        let (a, b) = (&before.patterns[sl.index()], &after.patterns[sl.index()]);
        let rows = std::iter::once((None, a.accent, b.accent)).chain(
            VoiceId::ALL
                .iter()
                .map(|&v| (Some(v), a.tracks[v.index()], b.tracks[v.index()])),
        );
        for (voice, x, y) in rows {
            let changed = x ^ y;
            for step in 0..STEPS {
                if changed >> step & 1 == 1 {
                    out.push(Ghost {
                        slot: sl,
                        voice,
                        step: step as u8,
                        added: y >> step & 1 == 1,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_song() -> Song {
        let mut st = State::default();
        st.patterns[0] = Grid::from_rows(&[
            (None, "x... x... x... x.x."),
            (Some(VoiceId::Kick), "x... ...x ..x. ...."),
            (Some(VoiceId::Snare), ".... x... .... x..."),
            (Some(VoiceId::ClosedHat), "x.x. x.x. x.x. x.x."),
        ]);
        st.patterns[2] = Grid::from_rows(&[(Some(VoiceId::Conga), "..x. ..x. ...x ..x.")]);
        st.set_tempo(93.5).unwrap();
        st.set_swing(0.62).unwrap();
        st.set_voice_param(VoiceId::Kick, VoiceParam::Tune, -0.35)
            .unwrap();
        st.set_voice_param(VoiceId::Snare, VoiceParam::Tone, 0.9)
            .unwrap();
        st.set_mute(VoiceId::Cowbell, true).unwrap();
        st.set_master(MasterParam::Room, 0.8).unwrap();
        Song {
            name: "Round Trip".into(),
            state: st,
        }
    }

    #[test]
    fn step_strings_round_trip() {
        for bits in [0u16, 1, 0x8000, 0xffff, 0b1010_0101_0011_1100] {
            assert_eq!(parse_steps(&format_steps(bits)).unwrap(), bits);
        }
        assert_eq!(parse_steps("x...|x...|x...|x...").unwrap(), 0x1111);
        assert!(parse_steps("x... x...").is_err());
        assert!(parse_steps("x... x... x... x...x").is_err());
        assert!(parse_steps("x... x... x... x..?").is_err());
    }

    #[test]
    fn song_json_round_trip_is_exact() {
        let song = sample_song();
        let json = song.to_json();
        let back = Song::from_json(&json).unwrap();
        assert_eq!(back, song);
        // And stable: serializing again gives identical text.
        assert_eq!(back.to_json(), json);
        // Readable: tracks are step strings, voices in kit order.
        assert!(json.contains("\"kick\": \"x... ...x ..x. ....\""), "{json}");
        assert!(json.find("\"kick\"").unwrap() < json.find("\"conga\"").unwrap());
    }

    #[test]
    fn json_rejects_bad_files() {
        let good = sample_song().to_json();
        assert!(Song::from_json(&good.replace("redlight-song", "something-else")).is_err());
        assert!(Song::from_json(&good.replace("\"tempo\": 93.5", "\"tempo\": 250.0")).is_err());
        assert!(Song::from_json(&good.replace("\"kick\": \"x...", "\"kik\": \"x...")).is_err());
        assert!(Song::from_json(&good.replace("x... ...x ..x. ....", "x... ...x ..x.")).is_err());
        assert!(Song::from_json("{").is_err());
    }

    #[test]
    fn json_fills_in_missing_voices_and_master() {
        let text = r#"{"format":"redlight-song","version":1,"name":"min","tempo":120,"swing":0.5,
            "patterns":{"B":{"kick":"x... x... x... x..."}}}"#;
        let s = Song::from_json(text).unwrap();
        assert_eq!(s.state.patterns[1].tracks[0], 0x1111);
        assert!(s.state.patterns[0].is_empty());
        assert_eq!(s.state.voices[3], VoiceSettings::default());
        assert_eq!(s.state.master, Master::default());
    }

    #[test]
    fn json_reads_songs_saved_by_0_1_0() {
        let old = sample_song().to_json().replace("\"redlight-song\"", "\"backroom-song\"");
        assert!(old.contains("backroom-song"));
        assert_eq!(Song::from_json(&old).unwrap(), sample_song());
        // Saving it again writes the current format.
        assert!(Song::from_json(&old).unwrap().to_json().contains("\"redlight-song\""));
    }

    #[test]
    fn edit_commands_do_what_they_say() {
        let mut st = State::default();
        st.apply(Command::SetStep {
            slot: Slot::B,
            voice: VoiceId::Rim,
            step: 15,
            on: true,
        })
        .unwrap();
        assert!(st.patterns[1].get(VoiceId::Rim, 15));
        st.apply(Command::SetStep {
            slot: Slot::B,
            voice: VoiceId::Rim,
            step: 15,
            on: false,
        })
        .unwrap();
        assert!(st.patterns[1].is_empty());
        st.apply(Command::SetAccent {
            slot: Slot::A,
            step: 4,
            on: true,
        })
        .unwrap();
        assert!(st.patterns[0].accent_at(4));
        st.apply(Command::SetTempo { bpm: 60.0 }).unwrap();
        st.apply(Command::SetSwing { swing: 0.75 }).unwrap();
        assert_eq!((st.tempo, st.swing), (60.0, 0.75));
        st.apply(Command::SetVoiceParam {
            voice: VoiceId::Conga,
            param: VoiceParam::Tune,
            value: -1.0,
        })
        .unwrap();
        assert_eq!(st.voices[10].tune, -1.0);
        st.apply(Command::SetMute {
            voice: VoiceId::Kick,
            muted: true,
        })
        .unwrap();
        assert!(st.voices[0].mute);
        st.apply(Command::SetMaster {
            param: MasterParam::Crush,
            value: 0.4,
        })
        .unwrap();
        assert_eq!(st.master.crush, 0.4);

        st.set_step(Slot::A, VoiceId::Kick, 0, true).unwrap();
        st.apply(Command::CopyPattern {
            from: Slot::A,
            to: Slot::D,
        })
        .unwrap();
        assert_eq!(st.patterns[3], st.patterns[0]);
        st.apply(Command::ClearPattern { slot: Slot::A }).unwrap();
        assert!(st.patterns[0].is_empty());
        assert!(!st.patterns[3].is_empty());
    }

    #[test]
    fn invalid_edits_are_rejected_and_change_nothing() {
        let mut st = State::default();
        let before = st;
        let bad = [
            Command::SetStep {
                slot: Slot::A,
                voice: VoiceId::Kick,
                step: 16,
                on: true,
            },
            Command::SetAccent {
                slot: Slot::A,
                step: 200,
                on: true,
            },
            Command::SetTempo { bpm: 59.0 },
            Command::SetTempo { bpm: 181.0 },
            Command::SetTempo { bpm: f32::NAN },
            Command::SetSwing { swing: 0.49 },
            Command::SetSwing { swing: 0.8 },
            Command::SetVoiceParam {
                voice: VoiceId::Kick,
                param: VoiceParam::Decay,
                value: 1.5,
            },
            Command::SetVoiceParam {
                voice: VoiceId::Kick,
                param: VoiceParam::Tune,
                value: -1.2,
            },
            Command::SetMaster {
                param: MasterParam::Volume,
                value: f32::INFINITY,
            },
        ];
        for c in bad {
            assert!(st.apply(c).is_err(), "{c:?} should fail");
            assert_eq!(st, before, "{c:?} changed the state");
        }
    }

    #[test]
    fn batches_are_all_or_nothing() {
        let mut st = State::default();
        let cmds = [
            Command::SetStep {
                slot: Slot::A,
                voice: VoiceId::Kick,
                step: 0,
                on: true,
            },
            Command::SetTempo { bpm: 500.0 },
        ];
        assert_eq!(st.apply_all(&cmds).unwrap_err().0, 1);
        assert!(st.patterns[0].is_empty());
    }

    #[test]
    fn commands_deserialize_from_tool_call_json() {
        let c: Command = serde_json::from_str(
            r#"{"cmd":"set_step","slot":"C","voice":"open_hat","step":2,"on":true}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SetStep {
                slot: Slot::C,
                voice: VoiceId::OpenHat,
                step: 2,
                on: true
            }
        );
        let c: Command = serde_json::from_str(
            r#"{"cmd":"set_voice_param","voice":"snare","param":"tone","value":0.8}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SetVoiceParam {
                voice: VoiceId::Snare,
                param: VoiceParam::Tone,
                value: 0.8
            }
        );
        for c in [
            Command::SetSwing { swing: 0.6 },
            Command::CopyPattern {
                from: Slot::A,
                to: Slot::B,
            },
        ] {
            let back: Command = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
            assert_eq!(back, c);
        }
        assert!(serde_json::from_str::<Command>(r#"{"cmd":"delete_everything"}"#).is_err());
    }

    #[test]
    fn preview_reports_ghost_notes_without_applying() {
        let st = sample_song().state;
        let cmds = [
            Command::SetStep {
                slot: Slot::A,
                voice: VoiceId::Kick,
                step: 0,
                on: false,
            },
            Command::SetStep {
                slot: Slot::A,
                voice: VoiceId::Rim,
                step: 7,
                on: true,
            },
            Command::SetStep {
                slot: Slot::A,
                voice: VoiceId::Kick,
                step: 4,
                on: true,
            },
            Command::SetAccent {
                slot: Slot::A,
                step: 3,
                on: true,
            },
            Command::SetTempo { bpm: 100.0 },
        ];
        let (next, ghosts) = st.preview(&cmds).unwrap();
        assert_eq!(next.tempo, 100.0);
        assert_eq!(st.tempo, 93.5, "preview must not touch the original");
        assert_eq!(
            ghosts,
            vec![
                Ghost {
                    slot: Slot::A,
                    voice: None,
                    step: 3,
                    added: true
                },
                Ghost {
                    slot: Slot::A,
                    voice: Some(VoiceId::Kick),
                    step: 0,
                    added: false
                },
                Ghost {
                    slot: Slot::A,
                    voice: Some(VoiceId::Kick),
                    step: 4,
                    added: true
                },
                Ghost {
                    slot: Slot::A,
                    voice: Some(VoiceId::Rim),
                    step: 7,
                    added: true
                },
            ]
        );
    }
}
