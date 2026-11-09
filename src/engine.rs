//! The whole machine: clock, kit, mixer and master chain.
//!
//! `Engine` never allocates or locks after `new`, so it runs inside the audio
//! callback. The UI keeps its own copy of the `State` and sends the same
//! `Command`s it applied, so both copies stay identical.

use crate::clock::Clock;
use crate::dsp::*;
use crate::fx::{Reverb, Tape, TapeSettings};
use crate::pattern::{Command, Slot, State, VoiceId, VOICES};
use crate::voices::Kit;

/// Hard ceiling of the soft clipper: nothing leaves the engine louder than this.
pub const LIMIT: f32 = 0.95;
/// Mix bus gain before the master chain.
const BUS: f32 = 0.85;
/// Stereo position per voice, -1 left .. 1 right (kit order).
const PAN: [f32; VOICES] = [
    0.0, 0.04, -0.12, 0.3, 0.34, -0.38, -0.05, 0.32, -0.2, 0.24, -0.3,
];
/// Reverb send per voice: the kick stays mostly dry so the low end doesn't wash.
const SEND: [f32; VOICES] = [
    0.04, 0.55, 0.75, 0.2, 0.35, 0.45, 0.45, 0.45, 0.55, 0.4, 0.5,
];

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Cmd(Command),
    Load(State),
    Play,
    Stop,
    /// Picks the slot to play. While playing, the switch waits for the bar line.
    Select(Slot),
    /// Plays one voice now (clicking a voice name), accented.
    Audition(VoiceId),
}

pub struct Engine {
    sr: f32,
    state: State,
    clock: Clock,
    kit: Kit,
    reverb: Reverb,
    tape: Tape,
    dc_l: DcBlock,
    dc_r: DcBlock,
    slot: Slot,
    queued: Option<Slot>,
    step: Option<usize>,
    bars: u64,
    gain: f32,
    frame: [f32; VOICES],
}

impl Engine {
    pub fn new(sr: f32) -> Self {
        Engine {
            sr,
            state: State::default(),
            clock: Clock::default(),
            kit: Kit::new(sr),
            reverb: Reverb::new(sr),
            tape: Tape::new(sr),
            dc_l: DcBlock::new(sr),
            dc_r: DcBlock::new(sr),
            slot: Slot::A,
            queued: None,
            step: None,
            bars: 0,
            gain: 0.0,
            frame: [0.0; VOICES],
        }
    }

    pub fn handle(&mut self, e: Event) {
        match e {
            // Already validated by the UI; a rejected command is simply dropped.
            Event::Cmd(c) => {
                let _ = self.state.apply(c);
            }
            Event::Load(s) => self.state = s,
            Event::Play => {
                if !self.clock.running() {
                    if let Some(q) = self.queued.take() {
                        self.slot = q;
                    }
                    self.clock.start();
                    self.bars = 0;
                }
            }
            Event::Stop => {
                self.clock.stop();
                self.step = None;
            }
            Event::Select(s) => {
                if self.clock.running() {
                    self.queued = Some(s);
                } else {
                    self.slot = s;
                    self.queued = None;
                }
            }
            Event::Audition(v) => {
                let vs = self.state.voices[v.index()];
                self.kit.trigger(v, &vs, true);
            }
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn playing(&self) -> bool {
        self.clock.running()
    }
    /// The step currently sounding (None when stopped).
    pub fn step(&self) -> Option<usize> {
        self.step
    }
    pub fn slot(&self) -> Slot {
        self.slot
    }
    /// Bars started since Play (the first bar counts as 1).
    pub fn bars(&self) -> u64 {
        self.bars
    }

    fn fire(&mut self, step: usize) {
        if step == 0 {
            if let Some(q) = self.queued.take() {
                self.slot = q;
            }
            self.bars += 1;
        }
        self.step = Some(step);
        let g = self.state.patterns[self.slot.index()];
        let accent = g.accent_at(step);
        for v in VoiceId::ALL {
            let vs = self.state.voices[v.index()];
            if !vs.mute && g.get(v, step) {
                self.kit.trigger(v, &vs, accent);
            }
        }
    }

    /// Fills an interleaved stereo buffer.
    pub fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        let m = self.state.master;
        let tape = TapeSettings {
            wow: 0.25 * m.wobble,
            flutter: 0.7 * m.wobble,
            saturation: m.drive,
            age: 0.85 * m.age,
            hiss: m.hiss,
            crush: m.crush,
            rate: 1.0,
        };
        let rev_decay = 0.4 + 3.0 * m.room;
        let rev_mix = 1.4 * m.reverb;
        // Volume: a squared taper up to +9.5 dB, smoothed to avoid zipper noise.
        let gain_target = 3.0 * m.volume * m.volume;
        let gk = 1.0 - (-1.0 / (0.02 * sr)).exp();
        for f in out.chunks_exact_mut(2) {
            if let Some(s) = self.clock.tick(self.state.tempo, self.state.swing, sr) {
                self.fire(s);
            }
            self.kit.next(&mut self.frame);
            let (mut l, mut r, mut send) = (0.0, 0.0, 0.0);
            for i in 0..VOICES {
                let x = self.frame[i];
                // Equal-power-ish linear pan, centre at unity.
                l += x * (1.0 - PAN[i].max(0.0));
                r += x * (1.0 + PAN[i].min(0.0));
                send += x * SEND[i];
            }
            let (wl, wr) = self
                .reverb
                .run(send * BUS, send * BUS, rev_decay, 0.45, 12.0);
            let (l, r) = self
                .tape
                .run(l * BUS + wl * rev_mix, r * BUS + wr * rev_mix, &tape);
            self.gain += (gain_target - self.gain) * gk;
            let l = self.dc_l.run(l) * self.gain;
            let r = self.dc_r.run(r) * self.gain;
            f[0] = LIMIT * soft_clip(l / LIMIT);
            f[1] = LIMIT * soft_clip(r / LIMIT);
        }
    }
}

/// Offline render: plays `slots` one bar each, in order, then `tail` seconds
/// of decay. Interleaved stereo.
pub fn render_song(state: &State, slots: &[Slot], sr: f32, tail: f32) -> Vec<f32> {
    let mut e = Engine::new(sr);
    e.handle(Event::Load(*state));
    let first = slots.first().copied().unwrap_or(Slot::A);
    e.handle(Event::Select(first));
    // Snap the volume smoother so the first hit isn't faded in.
    e.gain = 3.0 * state.master.volume * state.master.volume;
    e.handle(Event::Play);
    let bar = 4.0 * 60.0 / state.tempo as f64 * sr as f64;
    let total = (bar * slots.len() as f64).round() as usize;
    let mut out = Vec::with_capacity((total + (tail * sr) as usize) * 2);
    let mut buf = [0.0f32; 64 * 2];
    let mut done = 0;
    while done < total {
        // During bar b (1-based), queue the slot for bar b + 1.
        let b = e.bars() as usize;
        if b >= 1 && b < slots.len() && e.queued.is_none() && e.slot() != slots[b] {
            e.handle(Event::Select(slots[b]));
        }
        let n = (total - done).min(64);
        e.render(&mut buf[..n * 2]);
        out.extend_from_slice(&buf[..n * 2]);
        done += n;
    }
    e.handle(Event::Stop);
    let mut tail_buf = vec![0.0f32; (tail * sr) as usize * 2];
    e.render(&mut tail_buf);
    out.extend_from_slice(&tail_buf);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{Grid, MasterParam};

    fn busy_state() -> State {
        let mut st = State::default();
        let all = "xxxx xxxx xxxx xxxx";
        let rows: Vec<(Option<VoiceId>, &str)> = std::iter::once((None, all))
            .chain(VoiceId::ALL.iter().map(|&v| (Some(v), all)))
            .collect();
        st.patterns[0] = Grid::from_rows(&rows);
        for v in &mut st.voices {
            v.level = 1.0;
            v.decay = 1.0;
        }
        st.tempo = 180.0;
        st
    }

    #[test]
    fn output_never_exceeds_the_limit_even_with_everything_maxed() {
        let mut st = busy_state();
        for p in MasterParam::ALL {
            st.set_master(p, 1.0).unwrap();
        }
        let out = render_song(&st, &[Slot::A, Slot::A], 48_000.0, 1.0);
        assert!(out.iter().all(|s| s.is_finite()));
        let peak = out.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak <= LIMIT, "peak {peak}");
        assert!(peak > 0.5, "should be loud: {peak}");
    }

    #[test]
    fn steps_trigger_on_the_right_samples() {
        // One kick on step 4 at 120 BPM, straight: it starts at sample 4 * 6000 (+ tape delay).
        let mut st = State::default();
        st.swing = 0.5;
        st.tempo = 120.0;
        st.master = crate::pattern::Master {
            reverb: 0.0,
            wobble: 0.0,
            hiss: 0.0,
            ..st.master
        };
        st.patterns[0] = Grid::from_rows(&[(Some(VoiceId::Kick), ".... x... .... ....")]);
        let out = render_song(&st, &[Slot::A], 48_000.0, 0.0);
        let first = out.chunks(2).position(|f| f[0].abs() > 1e-3).unwrap();
        let tape_delay = (0.012 * 48_000.0) as usize;
        assert!(
            (first as i64 - (24_000 + tape_delay) as i64).abs() <= 2,
            "first sound at {first}"
        );
    }

    #[test]
    fn mute_silences_a_voice_and_slot_switch_waits_for_the_bar() {
        let mut st = State::default();
        st.master.hiss = 0.0;
        st.patterns[0] = Grid::from_rows(&[(Some(VoiceId::Kick), "x... x... x... x...")]);
        st.set_mute(VoiceId::Kick, true).unwrap();
        let out = render_song(&st, &[Slot::A], 48_000.0, 0.5);
        assert!(
            out.iter().all(|s| s.abs() < 1e-3),
            "muted kick still sounds"
        );

        let mut e = Engine::new(48_000.0);
        e.handle(Event::Load(State::default()));
        e.handle(Event::Play);
        let mut buf = vec![0.0; 2000];
        e.render(&mut buf);
        e.handle(Event::Select(Slot::C));
        assert_eq!(e.slot(), Slot::A, "switch should wait for the bar line");
        let bar = (4.0 * 60.0 / 96.0 * 48_000.0) as usize;
        let mut rest = vec![0.0; bar * 2];
        e.render(&mut rest);
        assert_eq!(e.slot(), Slot::C);
    }

    #[test]
    fn render_song_length_matches_the_bars() {
        let st = State {
            tempo: 120.0,
            ..State::default()
        };
        let out = render_song(&st, &[Slot::A, Slot::B, Slot::A], 48_000.0, 1.0);
        let secs = out.len() as f32 / 2.0 / 48_000.0;
        // 3 bars at 120 BPM = 6 s, plus 1 s tail.
        assert!((secs - 7.0).abs() < 0.01, "{secs}");
    }
}
