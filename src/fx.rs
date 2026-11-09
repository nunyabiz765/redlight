//! Master effects lifted from Plaza (~/projects/plaza/src/fx.rs, same author,
//! MIT OR Apache-2.0): the FDN reverb and the cassette tape stage, unchanged
//! apart from dropping Plaza's chorus. Red Light drives them gentler (see engine.rs).

use crate::dsp::*;

// ---------------------------------------------------------------- reverb ---

const FDN: usize = 8;
/// Delay lengths at 48 kHz, mutually prime-ish, 43..93 ms.
const FDN_LENGTHS: [f32; FDN] = [
    2053.0, 2399.0, 2617.0, 2939.0, 3271.0, 3613.0, 4001.0, 4463.0,
];
const DIFFUSER_LENGTHS: [f32; 4] = [142.0, 107.0, 379.0, 277.0];

struct Allpass {
    line: DelayLine,
    len: usize,
    g: f32,
}

impl Allpass {
    fn new(len: usize, g: f32) -> Self {
        Allpass {
            line: DelayLine::new(len + 1),
            len,
            g,
        }
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let d = self.line.tap(self.len - 1);
        let v = x + self.g * d;
        self.line.push(v);
        d - self.g * v
    }
}

/// 8-line feedback delay network with a Hadamard mixing matrix, input
/// diffusion, per-line damping and slowly wandering delay taps for a lush,
/// chorused tail without metallic ringing.
pub struct Reverb {
    pre: DelayLine,
    diff: Vec<Allpass>,
    lines: Vec<DelayLine>,
    lens: [f32; FDN],
    damp: [OnePole; FDN],
    in_hp: OnePole,
    in_lp: OnePole,
    mod_phase: [f32; FDN],
    mod_rate: [f32; FDN],
    sr: f32,
    gains: [f32; FDN],
    last_decay: f32,
    last_damp: f32,
}

impl Reverb {
    pub fn new(sr: f32) -> Self {
        let scale = sr / 48_000.0;
        let lens = FDN_LENGTHS.map(|l| l * scale);
        let lines = lens
            .iter()
            .map(|&l| DelayLine::new(l as usize + 64))
            .collect();
        let diff = DIFFUSER_LENGTHS
            .iter()
            .map(|&l| Allpass::new((l * scale) as usize, 0.7))
            .collect();
        let mut mod_rate = [0.0; FDN];
        for (i, r) in mod_rate.iter_mut().enumerate() {
            *r = 0.11 + 0.07 * i as f32;
        }
        let mut r = Reverb {
            pre: DelayLine::new((sr * 0.21) as usize),
            diff,
            lines,
            lens,
            damp: std::array::from_fn(|_| OnePole::new(8000.0, sr)),
            in_hp: OnePole::new(110.0, sr),
            in_lp: OnePole::new(9500.0, sr),
            mod_phase: std::array::from_fn(|i| i as f32 / FDN as f32),
            mod_rate,
            sr,
            gains: [0.0; FDN],
            last_decay: -1.0,
            last_damp: -1.0,
        };
        r.configure(3.0, 0.4);
        r
    }

    fn configure(&mut self, decay: f32, damp: f32) {
        if (decay - self.last_decay).abs() < 1e-3 && (damp - self.last_damp).abs() < 1e-3 {
            return;
        }
        self.last_decay = decay;
        self.last_damp = damp;
        for i in 0..FDN {
            self.gains[i] = 10f32.powf(-3.0 * self.lens[i] / (decay * self.sr));
            let hz = 16_000.0 * (0.09f32).powf(damp);
            self.damp[i].set(hz, self.sr);
        }
    }

    /// Returns the wet signal only.
    #[inline]
    pub fn run(&mut self, l: f32, r: f32, decay: f32, damp: f32, pre_ms: f32) -> (f32, f32) {
        self.configure(decay, damp);
        let x = 0.5 * (l + r);
        self.pre.push(x);
        let mut x = self.pre.read((pre_ms * self.sr / 1000.0).max(1.0));
        x = self.in_lp.lp(self.in_hp.hp(x));
        for ap in &mut self.diff {
            x = ap.run(x);
        }

        let mut o = [0.0f32; FDN];
        for i in 0..FDN {
            self.mod_phase[i] = wrap01(self.mod_phase[i] + self.mod_rate[i] / self.sr);
            let wobble = sin01(self.mod_phase[i]) * 9.0 * (self.sr / 48_000.0);
            o[i] = self.lines[i].read(self.lens[i] + wobble);
        }
        // Fast Hadamard transform, normalised: lossless mixing.
        let mut h = o;
        let mut len = 1;
        while len < FDN {
            for i in (0..FDN).step_by(len * 2) {
                for j in i..i + len {
                    let (a, b) = (h[j], h[j + len]);
                    h[j] = a + b;
                    h[j + len] = a - b;
                }
            }
            len *= 2;
        }
        let norm = 1.0 / (FDN as f32).sqrt();
        for i in 0..FDN {
            let fb = self.damp[i].lp(h[i] * norm * self.gains[i]);
            let inject = if i % 2 == 0 { x } else { -x };
            self.lines[i].push(fb + inject * 0.5);
        }
        let wl = o[0] + o[2] - o[4] + o[6] - 0.5 * (o[1] - o[5]);
        let wr = o[1] + o[3] - o[5] + o[7] - 0.5 * (o[2] - o[6]);
        (wl * 0.35, wr * 0.35)
    }
}

// ------------------------------------------------------------------ tape ---

/// Cassette deck: saturation, head-loss roll-off, wow and flutter on a shared
/// transport (with slow random drift), hiss, and an optional sampler-style
/// lo-fi stage (bit and rate reduction).
pub struct Tape {
    line_l: DelayLine,
    line_r: DelayLine,
    tone_l: Svf,
    tone_r: Svf,
    dc_l: DcBlock,
    dc_r: DcBlock,
    hiss_l: Svf,
    hiss_r: Svf,
    wow_ph: f32,
    flut_ph: f32,
    drift: f32,
    drift_target: f32,
    drift_count: u32,
    flut_noise: OnePole,
    rng: Rng,
    hold: (f32, f32),
    hold_count: f32,
    last_age: f32,
    sr: f32,
}

pub struct TapeSettings {
    pub wow: f32,
    pub flutter: f32,
    pub saturation: f32,
    pub age: f32,
    pub hiss: f32,
    pub crush: f32,
    /// Transport speed from slow mode; slows the wow too.
    pub rate: f32,
}

const TAPE_BASE_MS: f32 = 12.0;

impl Tape {
    pub fn new(sr: f32) -> Self {
        Tape {
            line_l: DelayLine::new((sr * 0.03) as usize),
            line_r: DelayLine::new((sr * 0.03) as usize),
            tone_l: Svf::new(18_000.0, 0.7, sr),
            tone_r: Svf::new(18_000.0, 0.7, sr),
            dc_l: DcBlock::new(sr),
            dc_r: DcBlock::new(sr),
            hiss_l: Svf::new(5000.0, 0.5, sr),
            hiss_r: Svf::new(5000.0, 0.5, sr),
            wow_ph: 0.0,
            flut_ph: 0.0,
            drift: 0.0,
            drift_target: 0.0,
            drift_count: 0,
            flut_noise: OnePole::new(12.0, sr),
            rng: Rng::new(0x7a9e),
            hold: (0.0, 0.0),
            hold_count: 0.0,
            last_age: -1.0,
            sr,
        }
    }

    /// Transport delay in samples for this sample, before the tick advances.
    fn transport(&mut self, s: &TapeSettings) -> f32 {
        let sr = self.sr;
        self.wow_ph = wrap01(self.wow_ph + 0.55 * s.rate / sr);
        self.flut_ph = wrap01(self.flut_ph + 6.8 / sr);
        if self.drift_count == 0 {
            self.drift_target = self.rng.bipolar();
            self.drift_count = (sr * 1.7) as u32;
        }
        self.drift_count -= 1;
        self.drift += (self.drift_target - self.drift) * (1.0 / (sr * 0.8));
        let jitter = self.flut_noise.lp(self.rng.bipolar()) * 8.0;
        // Wow up to ±2.4 ms at 0.55 Hz is about ±0.8% pitch: proper warble.
        let wow_ms = s.wow * (2.4 * sin01(self.wow_ph) + 1.6 * self.drift);
        let flut_ms = s.flutter * (0.07 * sin01(self.flut_ph) + 0.05 * jitter);
        (TAPE_BASE_MS + wow_ms + flut_ms) * sr / 1000.0
    }

    #[inline]
    pub fn run(&mut self, l: f32, r: f32, s: &TapeSettings) -> (f32, f32) {
        let sr = self.sr;
        if (s.age - self.last_age).abs() > 1e-3 {
            self.last_age = s.age;
            let hz = 19_000.0 * (0.26f32).powf(s.age);
            self.tone_l.set(hz, 0.75, sr);
            self.tone_r.set(hz, 0.75, sr);
        }

        // Record head: saturation with a little asymmetry (even harmonics),
        // normalised to unity gain for small signals.
        let (mut l, mut r) = (l, r);
        if s.saturation > 0.001 {
            let drive = 1.0 + 4.0 * s.saturation;
            let bias = 0.18 * s.saturation;
            let tb = soft_clip(bias);
            let g0 = drive * (1.0 - tb * tb);
            l = self.dc_l.run((soft_clip(drive * l + bias) - tb) / g0);
            r = self.dc_r.run((soft_clip(drive * r + bias) - tb) / g0);
        }

        // Transport: one capstan, so both channels wobble together.
        let d = self.transport(s);
        self.line_l.push(l);
        self.line_r.push(r);
        let mut l = self.line_l.read(d);
        let mut r = self.line_r.read(d);

        // Playback head loss.
        l = self.tone_l.lp(l);
        r = self.tone_r.lp(r);

        if s.hiss > 0.001 {
            let h = s.hiss * 0.012;
            l += self.hiss_l.hp(self.rng.bipolar()) * h;
            r += self.hiss_r.hp(self.rng.bipolar()) * h;
        }

        if s.crush > 0.001 {
            // Sample-and-hold down to ~sr/7 and quantise down to 6 bits at full.
            self.hold_count -= 1.0;
            if self.hold_count <= 0.0 {
                self.hold_count += 1.0 + s.crush * 6.0;
                let bits = 16.0 - 10.0 * s.crush;
                let q = 2f32.powf(bits - 1.0);
                self.hold = ((l * q).round() / q, (r * q).round() / q);
            }
            l = self.hold.0;
            r = self.hold.1;
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_tail_energy(decay: f32) -> Vec<f32> {
        let sr = 48_000.0;
        let mut rv = Reverb::new(sr);
        let mut blocks = vec![0.0f32; 10];
        for i in 0..(sr as usize * 5) {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, r) = rv.run(x, x, decay, 0.3, 0.0);
            assert!(l.is_finite() && r.is_finite());
            blocks[i / 24_000] += l * l + r * r;
        }
        blocks
    }

    #[test]
    fn reverb_tail_decays_at_roughly_the_set_rate() {
        let e = impulse_tail_energy(2.0);
        // Energy should fall monotonically after the build-up...
        for w in e[1..].windows(2) {
            assert!(w[1] < w[0], "{e:?}");
        }
        // ...by about 60 dB over 2 s (damping makes it a bit faster). Compare
        // 0.5..1.0 s with 2.5..3.0 s: two seconds apart.
        let drop_db = 10.0 * (e[5] / e[1]).log10();
        assert!(
            (-80.0..-45.0).contains(&drop_db),
            "drop {drop_db} dB, {e:?}"
        );
    }

    #[test]
    fn reverb_is_stable_at_max_decay() {
        let sr = 48_000.0;
        let mut rv = Reverb::new(sr);
        let mut rng = Rng::new(3);
        let mut peak = 0f32;
        for _ in 0..(sr as usize * 10) {
            let x = rng.bipolar() * 0.3;
            let (l, r) = rv.run(x, x, 20.0, 0.0, 50.0);
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak.is_finite() && peak < 8.0, "peak {peak}");
    }

    /// Wow should move pitch by under a percent: a 1 kHz tone comes out between
    /// 990 and 1010 Hz at every moment, and not stuck at exactly 1 kHz.
    #[test]
    fn wow_bends_pitch_gently() {
        let sr = 48_000.0;
        let mut t = Tape::new(sr);
        let s = TapeSettings {
            wow: 1.0,
            flutter: 0.0,
            saturation: 0.0,
            age: 0.0,
            hiss: 0.0,
            crush: 0.0,
            rate: 1.0,
        };
        let mut out = Vec::new();
        for i in 0..(sr as usize * 4) {
            let x = (TAU * 1000.0 * i as f32 / sr).sin();
            out.push(t.run(x, x, &s).0);
        }
        // Interpolated rising zero crossings, frequency over 20-cycle spans.
        let mut zc = Vec::new();
        for i in 4800..out.len() - 1 {
            if out[i] < 0.0 && out[i + 1] >= 0.0 {
                zc.push(i as f32 + out[i] / (out[i] - out[i + 1]));
            }
        }
        let mut min_hz = f32::MAX;
        let mut max_hz = 0f32;
        for k in 0..zc.len() - 20 {
            let hz = 20.0 * sr / (zc[k + 20] - zc[k]);
            min_hz = min_hz.min(hz);
            max_hz = max_hz.max(hz);
        }
        assert!(min_hz > 975.0 && max_hz < 1025.0, "{min_hz}..{max_hz}");
        assert!(
            max_hz - min_hz >= 10.0,
            "no audible wow: {min_hz}..{max_hz}"
        );
    }

    #[test]
    fn saturation_keeps_small_signals_at_unity() {
        let sr = 48_000.0;
        let mut t = Tape::new(sr);
        let s = TapeSettings {
            wow: 0.0,
            flutter: 0.0,
            saturation: 1.0,
            age: 0.0,
            hiss: 0.0,
            crush: 0.0,
            rate: 1.0,
        };
        let (mut ein, mut eout) = (0.0, 0.0);
        for i in 0..48_000 {
            let x = 0.02 * (TAU * 300.0 * i as f32 / sr).sin();
            let (l, _) = t.run(x, x, &s);
            if i > 4800 {
                ein += x * x;
                eout += l * l;
            }
        }
        let db = 10.0 * (eout / ein).log10();
        assert!(db.abs() < 1.0, "{db} dB");
    }

    #[test]
    fn crush_quantises() {
        let sr = 48_000.0;
        let mut t = Tape::new(sr);
        let s = TapeSettings {
            wow: 0.0,
            flutter: 0.0,
            saturation: 0.0,
            age: 0.0,
            hiss: 0.0,
            crush: 1.0,
            rate: 1.0,
        };
        let mut levels = std::collections::BTreeSet::new();
        for i in 0..48_000 {
            let x = 0.9 * (TAU * 100.0 * i as f32 / sr).sin();
            let (l, _) = t.run(x, x, &s);
            levels.insert((l * 10_000.0).round() as i32);
        }
        // 6 bits => at most 64 levels (allow a few for the tone filter edge).
        assert!(levels.len() <= 70, "{} levels", levels.len());
    }
}
