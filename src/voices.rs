//! The eleven drum voices, all synthesized: oscillators, noise, filters and
//! envelopes in the manner of early-80s analog drum machines. No samples.
//!
//! Each voice reads its tune/decay/tone settings when it's triggered (like the
//! hardware, a knob change shows on the next hit) and produces a raw signal
//! that peaks at or under 1.0. The kit applies level and accent.

use crate::dsp::*;
use crate::pattern::{VoiceId, VoiceSettings, VOICES};

/// Below this (-80 dB) a voice stops computing.
const SILENT: f32 = 1e-4;
/// Unaccented hits play at this gain (about -4.4 dB); accented ones at 1.0.
pub const ACCENT_OFF: f32 = 0.6;

/// Per-sample multiplier that falls 60 dB in `secs`.
fn t60(secs: f32, sr: f32) -> f32 {
    10f32.powf(-3.0 / (secs.max(0.001) * sr))
}

/// Per-sample multiplier for an exponential with time constant `secs`.
fn tau(secs: f32, sr: f32) -> f32 {
    (-1.0 / (secs.max(0.0001) * sr)).exp()
}

fn semis(tune: f32, range: f32) -> f32 {
    2f32.powf(tune * range)
}

#[inline]
fn adv(phase: &mut f32, hz: f32, sr: f32) -> f32 {
    *phase = wrap01(*phase + hz / sr);
    *phase
}

// ------------------------------------------------------------------ kick ---

/// Tuned sine with a fast pitch drop, a click of filtered noise on the attack,
/// and a soft saturator (the DRIVE knob).
pub struct Kick {
    sr: f32,
    phase: f32,
    f0: f32,
    penv: f32,
    pmul: f32,
    amp: f32,
    amul: f32,
    click: f32,
    cmul: f32,
    drive: f32,
    norm: f32,
    lp: OnePole,
    rng: Rng,
}

impl Kick {
    fn new(sr: f32) -> Self {
        Kick {
            sr,
            phase: 0.0,
            f0: 50.0,
            penv: 0.0,
            pmul: 0.0,
            amp: 0.0,
            amul: 0.0,
            click: 0.0,
            cmul: 0.0,
            drive: 1.0,
            norm: 1.0,
            lp: OnePole::new(3500.0, sr),
            rng: Rng::new(0xb00),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, accent: bool) {
        let sr = self.sr;
        self.f0 = 49.0 * semis(s.tune, 0.85);
        // The long-decay knob: 0.16 s tight to about 2.5 s of boom.
        self.amul = t60(0.16 + 2.3 * s.decay.powf(1.7), sr);
        self.penv = 1.0;
        self.pmul = tau(0.014, sr);
        self.amp = 1.0;
        self.click = 1.0;
        self.cmul = tau(0.0012, sr);
        self.phase = 0.0;
        self.drive = 1.2 + 2.8 * s.tone + if accent { 0.6 } else { 0.0 };
        self.norm = 0.82 / soft_clip(self.drive * 1.15);
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.amp < SILENT {
            return 0.0;
        }
        let f = self.f0 * (1.0 + 1.7 * self.penv);
        let body = sin01(adv(&mut self.phase, f, self.sr)) * self.amp;
        let click = self.lp.lp(self.rng.bipolar()) * self.click;
        let x = body + 0.3 * click;
        self.penv *= self.pmul;
        self.amp *= self.amul;
        self.click *= self.cmul;
        soft_clip(self.drive * x) * self.norm
    }
}

// ----------------------------------------------------------------- snare ---

/// Two detuned sine "heads" with a little pitch drop, plus high-passed noise
/// for the wires. SNAPPY sets the noise level.
pub struct Snare {
    sr: f32,
    ph1: f32,
    ph2: f32,
    f1: f32,
    f2: f32,
    penv: f32,
    pmul: f32,
    tenv: f32,
    tmul: f32,
    nenv: f32,
    nmul: f32,
    snappy: f32,
    hp: Svf,
    lp: OnePole,
    rng: Rng,
}

impl Snare {
    fn new(sr: f32) -> Self {
        Snare {
            sr,
            ph1: 0.0,
            ph2: 0.0,
            f1: 185.0,
            f2: 330.0,
            penv: 0.0,
            pmul: 0.0,
            tenv: 0.0,
            tmul: 0.0,
            nenv: 0.0,
            nmul: 0.0,
            snappy: 0.5,
            hp: Svf::new(1400.0, 0.7, sr),
            lp: OnePole::new(7000.0, sr),
            rng: Rng::new(0x5a4e),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        let k = semis(s.tune, 0.6);
        self.f1 = 185.0 * k;
        self.f2 = 330.0 * k;
        self.ph1 = 0.0;
        self.ph2 = 0.0;
        self.penv = 1.0;
        self.pmul = tau(0.006, sr);
        self.tenv = 1.0;
        self.tmul = t60(0.08 + 0.14 * s.decay, sr);
        self.nenv = 1.0;
        self.nmul = t60(0.10 + 0.45 * s.decay, sr);
        self.snappy = 0.15 + 0.85 * s.tone;
        self.hp.set(1300.0 * k.sqrt(), 0.7, sr);
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.tenv < SILENT && self.nenv < SILENT {
            return 0.0;
        }
        let p = 1.0 + 0.35 * self.penv;
        let a = sin01(adv(&mut self.ph1, self.f1 * p, self.sr));
        let b = sin01(adv(&mut self.ph2, self.f2 * p, self.sr));
        let body = (0.62 * a + 0.38 * b) * self.tenv;
        let wires = self.lp.lp(self.hp.hp(self.rng.bipolar())) * self.nenv * self.snappy;
        self.penv *= self.pmul;
        self.tenv *= self.tmul;
        self.nenv *= self.nmul;
        0.95 * soft_clip(1.4 * body + 1.2 * wires)
    }
}

// ------------------------------------------------------------------ clap ---

/// Band-passed noise through a sawtooth envelope that fires three quick
/// bursts and then a longer fourth one: several hands, slightly apart.
/// SPREAD sets the gap between the bursts.
pub struct Clap {
    sr: f32,
    t: u32,
    gap: u32,
    env: f32,
    bmul: f32,
    tmul: f32,
    active: bool,
    bp: Svf,
    q: f32,
    rng: Rng,
}

impl Clap {
    fn new(sr: f32) -> Self {
        Clap {
            sr,
            t: 0,
            gap: 1,
            env: 0.0,
            bmul: 0.0,
            tmul: 0.0,
            active: false,
            bp: Svf::new(1150.0, 1.4, sr),
            q: 1.4,
            rng: Rng::new(0xc1a9),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        self.t = 0;
        self.gap = ((0.006 + 0.009 * s.tone) * sr) as u32;
        self.env = 1.0;
        self.bmul = tau(0.0028, sr);
        self.tmul = t60(0.12 + 0.6 * s.decay, sr);
        self.active = true;
        self.bp.set(1150.0 * semis(s.tune, 0.6), self.q, sr);
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let in_bursts = self.t < 3 * self.gap;
        if self.t > 0 && self.t % self.gap == 0 && self.t <= 3 * self.gap {
            self.env = 1.0;
        }
        let n = self.bp.run(self.rng.bipolar()).bp / self.q;
        let y = n * self.env;
        self.env *= if in_bursts { self.bmul } else { self.tmul };
        self.t += 1;
        if !in_bursts && self.env < SILENT {
            self.active = false;
        }
        soft_clip(2.5 * y)
    }
}

// ------------------------------------------------------------------ hats ---

/// Six square waves at inharmonic ratios (the classic "metal" bank), band-
/// passed and high-passed, with a touch of noise. The closed hat chokes the
/// open one. BRIGHT moves the band-pass up.
pub struct Hat {
    sr: f32,
    open: bool,
    ph: [f32; 6],
    f: [f32; 6],
    env: f32,
    mul: f32,
    bp: Svf,
    hp: Svf,
    rng: Rng,
}

const METAL: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

impl Hat {
    fn new(sr: f32, open: bool) -> Self {
        Hat {
            sr,
            open,
            // Free-running oscillators: start them out of phase.
            ph: [0.0, 0.17, 0.41, 0.63, 0.29, 0.88],
            f: METAL,
            env: 0.0,
            mul: 0.0,
            bp: Svf::new(8000.0, 1.0, sr),
            hp: Svf::new(6000.0, 0.7, sr),
            rng: Rng::new(if open { 0x0be7 } else { 0xc7a7 }),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        let k = semis(s.tune, 0.5);
        self.f = METAL.map(|f| f * k);
        self.env = 1.0;
        self.mul = if self.open {
            t60(0.2 + 1.0 * s.decay, sr)
        } else {
            t60(0.03 + 0.13 * s.decay, sr)
        };
        let centre = 8000.0 * 2f32.powf(1.1 * (s.tone - 0.5));
        self.bp.set(centre, 1.1, sr);
        self.hp.set(centre * 0.7, 0.7, sr);
    }
    fn choke(&mut self) {
        if self.env > SILENT {
            self.mul = self.mul.min(tau(0.004, self.sr));
        }
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.env < SILENT {
            return 0.0;
        }
        let mut metal = 0.0;
        for i in 0..6 {
            let dt = self.f[i] / self.sr;
            let t = adv(&mut self.ph[i], self.f[i], self.sr);
            metal += pulse(t, dt, 0.5);
        }
        let x = metal * (1.0 / 6.0) + 0.2 * self.rng.bipolar();
        let y = self.hp.hp(self.bp.run(x).bp);
        let out = y * self.env;
        self.env *= self.mul;
        // The closed hat is all attack, so it gets more gain to sit level with the open one.
        soft_clip(if self.open { 3.6 } else { 5.0 } * out)
    }
}

// ------------------------------------------------------------------ toms ---

/// Sine with a gentle downward pitch bend and a puff of noise for the skin.
pub struct Tom {
    sr: f32,
    base: f32,
    ph: f32,
    f0: f32,
    penv: f32,
    pmul: f32,
    amp: f32,
    amul: f32,
    nenv: f32,
    nmul: f32,
    skin: f32,
    lp: OnePole,
    rng: Rng,
}

impl Tom {
    fn new(sr: f32, base: f32, seed: u32) -> Self {
        Tom {
            sr,
            base,
            ph: 0.0,
            f0: base,
            penv: 0.0,
            pmul: 0.0,
            amp: 0.0,
            amul: 0.0,
            nenv: 0.0,
            nmul: 0.0,
            skin: 0.5,
            lp: OnePole::new(2500.0, sr),
            rng: Rng::new(seed),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        self.f0 = self.base * semis(s.tune, 0.7);
        self.ph = 0.0;
        self.penv = 1.0;
        self.pmul = tau(0.05, sr);
        self.amp = 1.0;
        self.amul = t60(0.15 + 0.85 * s.decay, sr);
        self.nenv = 1.0;
        self.nmul = tau(0.012, sr);
        self.skin = s.tone;
        self.lp.set(self.f0 * 18.0, sr);
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.amp < SILENT {
            return 0.0;
        }
        let f = self.f0 * (1.0 + 0.3 * self.penv);
        let body = sin01(adv(&mut self.ph, f, self.sr)) * self.amp;
        let noise = self.lp.lp(self.rng.bipolar()) * self.nenv * self.skin;
        self.penv *= self.pmul;
        self.amp *= self.amul;
        self.nenv *= self.nmul;
        0.85 * soft_clip(0.95 * body + 0.6 * noise)
    }
}

// ------------------------------------------------------------------- rim ---

/// Two high sines, very short, overdriven and high-passed into a knock, with
/// a noise click on top (CLICK).
pub struct Rim {
    sr: f32,
    ph1: f32,
    ph2: f32,
    f1: f32,
    f2: f32,
    env: f32,
    mul: f32,
    click: f32,
    cmul: f32,
    click_amt: f32,
    hp: Svf,
    rng: Rng,
}

impl Rim {
    fn new(sr: f32) -> Self {
        Rim {
            sr,
            ph1: 0.0,
            ph2: 0.0,
            f1: 1700.0,
            f2: 480.0,
            env: 0.0,
            mul: 0.0,
            click: 0.0,
            cmul: 0.0,
            click_amt: 0.5,
            hp: Svf::new(380.0, 0.7, sr),
            rng: Rng::new(0x4141),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        let k = semis(s.tune, 0.5);
        self.f1 = 1700.0 * k;
        self.f2 = 480.0 * k;
        self.ph1 = 0.0;
        self.ph2 = 0.0;
        self.env = 1.0;
        self.mul = t60(0.02 + 0.08 * s.decay, sr);
        self.click = 1.0;
        self.cmul = tau(0.0008, sr);
        self.click_amt = s.tone;
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.env < SILENT {
            return 0.0;
        }
        let a = sin01(adv(&mut self.ph1, self.f1, self.sr));
        let b = sin01(adv(&mut self.ph2, self.f2, self.sr));
        let knock = soft_clip(2.5 * (0.55 * a + 0.45 * b) * self.env);
        let click = self.rng.bipolar() * self.click * self.click_amt;
        self.env *= self.mul;
        self.click *= self.cmul;
        0.8 * soft_clip(self.hp.hp(knock) + 0.5 * click)
    }
}

// --------------------------------------------------------------- cowbell ---

/// Two detuned square waves (about a fifth apart), band-passed, with a fast
/// initial drop into a longer ring. BRIGHT moves the band-pass.
pub struct Cowbell {
    sr: f32,
    ph1: f32,
    ph2: f32,
    f1: f32,
    f2: f32,
    fast: f32,
    fmul: f32,
    slow: f32,
    smul: f32,
    bp: Svf,
    q: f32,
    hp: OnePole,
}

impl Cowbell {
    fn new(sr: f32) -> Self {
        Cowbell {
            sr,
            ph1: 0.0,
            ph2: 0.0,
            f1: 540.0,
            f2: 800.0,
            fast: 0.0,
            fmul: 0.0,
            slow: 0.0,
            smul: 0.0,
            bp: Svf::new(900.0, 1.3, sr),
            q: 1.3,
            hp: OnePole::new(300.0, sr),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        let k = semis(s.tune, 0.5);
        self.f1 = 540.0 * k;
        self.f2 = 800.0 * k;
        self.fast = 1.0;
        self.fmul = tau(0.006, sr);
        self.slow = 1.0;
        self.smul = t60(0.12 + 0.6 * s.decay, sr);
        self.bp
            .set(900.0 * k * 2f32.powf(1.4 * (s.tone - 0.5)), self.q, sr);
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.slow < SILENT {
            return 0.0;
        }
        let a = pulse(adv(&mut self.ph1, self.f1, self.sr), self.f1 / self.sr, 0.5);
        let b = pulse(adv(&mut self.ph2, self.f2, self.sr), self.f2 / self.sr, 0.5);
        let sq = 0.5 * (a + b);
        let tone = self.bp.run(sq).bp / self.q + 0.18 * self.hp.hp(sq);
        let env = 0.62 * self.fast + 0.38 * self.slow;
        self.fast *= self.fmul;
        self.slow *= self.smul;
        0.85 * soft_clip(1.3 * tone * env)
    }
}

// ----------------------------------------------------------------- conga ---

/// A short, high, slightly bent sine with a skin click.
pub struct Conga {
    sr: f32,
    ph: f32,
    f0: f32,
    penv: f32,
    pmul: f32,
    amp: f32,
    amul: f32,
    click: f32,
    cmul: f32,
    skin: f32,
    lp: OnePole,
    rng: Rng,
}

impl Conga {
    fn new(sr: f32) -> Self {
        Conga {
            sr,
            ph: 0.0,
            f0: 310.0,
            penv: 0.0,
            pmul: 0.0,
            amp: 0.0,
            amul: 0.0,
            click: 0.0,
            cmul: 0.0,
            skin: 0.5,
            lp: OnePole::new(5000.0, sr),
            rng: Rng::new(0xc09a),
        }
    }
    fn trigger(&mut self, s: &VoiceSettings, _accent: bool) {
        let sr = self.sr;
        self.f0 = 310.0 * semis(s.tune, 0.7);
        self.ph = 0.0;
        self.penv = 1.0;
        self.pmul = tau(0.008, sr);
        self.amp = 1.0;
        self.amul = t60(0.1 + 0.45 * s.decay, sr);
        self.click = 1.0;
        self.cmul = tau(0.0015, sr);
        self.skin = s.tone;
    }
    #[inline]
    fn next(&mut self) -> f32 {
        if self.amp < SILENT {
            return 0.0;
        }
        let f = self.f0 * (1.0 + 0.2 * self.penv);
        let body = sin01(adv(&mut self.ph, f, self.sr)) * self.amp;
        let click = self.lp.lp(self.rng.bipolar()) * self.click * self.skin;
        self.penv *= self.pmul;
        self.amp *= self.amul;
        self.click *= self.cmul;
        0.85 * soft_clip(0.95 * body + 0.7 * click)
    }
}

// ------------------------------------------------------------------- kit ---

pub struct Kit {
    kick: Kick,
    snare: Snare,
    clap: Clap,
    chat: Hat,
    ohat: Hat,
    toms: [Tom; 3],
    rim: Rim,
    cowbell: Cowbell,
    conga: Conga,
    /// Level x accent, latched at the last trigger of each voice.
    gain: [f32; VOICES],
    /// Last output of each voice, and a short fade of it carried over a retrigger,
    /// so cutting off a still-ringing hit doesn't click.
    last: [f32; VOICES],
    carry: [f32; VOICES],
    carry_mul: f32,
}

impl Kit {
    pub fn new(sr: f32) -> Self {
        Kit {
            kick: Kick::new(sr),
            snare: Snare::new(sr),
            clap: Clap::new(sr),
            chat: Hat::new(sr, false),
            ohat: Hat::new(sr, true),
            toms: [
                Tom::new(sr, 88.0, 0x701),
                Tom::new(sr, 128.0, 0x702),
                Tom::new(sr, 178.0, 0x703),
            ],
            rim: Rim::new(sr),
            cowbell: Cowbell::new(sr),
            conga: Conga::new(sr),
            gain: [0.0; VOICES],
            last: [0.0; VOICES],
            carry: [0.0; VOICES],
            carry_mul: tau(0.0015, sr),
        }
    }

    pub fn trigger(&mut self, v: VoiceId, s: &VoiceSettings, accent: bool) {
        let i = v.index();
        self.gain[i] = s.level * if accent { 1.0 } else { ACCENT_OFF };
        self.carry[i] = self.last[i];
        match v {
            VoiceId::Kick => self.kick.trigger(s, accent),
            VoiceId::Snare => self.snare.trigger(s, accent),
            VoiceId::Clap => self.clap.trigger(s, accent),
            VoiceId::ClosedHat => {
                self.ohat.choke();
                self.carry[VoiceId::OpenHat.index()] = 0.0;
                self.chat.trigger(s, accent)
            }
            VoiceId::OpenHat => self.ohat.trigger(s, accent),
            VoiceId::LowTom => self.toms[0].trigger(s, accent),
            VoiceId::MidTom => self.toms[1].trigger(s, accent),
            VoiceId::HighTom => self.toms[2].trigger(s, accent),
            VoiceId::Rim => self.rim.trigger(s, accent),
            VoiceId::Cowbell => self.cowbell.trigger(s, accent),
            VoiceId::Conga => self.conga.trigger(s, accent),
        }
    }

    /// One sample of every voice, level and accent applied.
    #[inline]
    pub fn next(&mut self, out: &mut [f32; VOICES]) {
        let raw = [
            self.kick.next(),
            self.snare.next(),
            self.clap.next(),
            self.chat.next(),
            self.ohat.next(),
            self.toms[0].next(),
            self.toms[1].next(),
            self.toms[2].next(),
            self.rim.next(),
            self.cowbell.next(),
            self.conga.next(),
        ];
        for i in 0..VOICES {
            let y = raw[i] * self.gain[i] + self.carry[i];
            self.carry[i] *= self.carry_mul;
            if self.carry[i].abs() < 1e-6 {
                self.carry[i] = 0.0;
            }
            self.last[i] = y;
            out[i] = y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Renders one hit of `v` with these settings; returns the voice's samples.
    fn hit(v: VoiceId, s: VoiceSettings, accent: bool, secs: f32) -> Vec<f32> {
        let mut kit = Kit::new(SR);
        kit.trigger(v, &s, accent);
        let mut frame = [0.0; VOICES];
        (0..(secs * SR) as usize)
            .map(|_| {
                kit.next(&mut frame);
                frame[v.index()]
            })
            .collect()
    }

    fn settings(tune: f32, decay: f32, tone: f32) -> VoiceSettings {
        VoiceSettings {
            tune,
            decay,
            level: 1.0,
            tone,
            mute: false,
        }
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn every_voice_sounds_and_stays_under_full_scale_at_every_knob_extreme() {
        for v in VoiceId::ALL {
            for tune in [-1.0, 0.0, 1.0] {
                for decay in [0.0, 0.5, 1.0] {
                    for tone in [0.0, 1.0] {
                        let x = hit(v, settings(tune, decay, tone), true, 3.0);
                        assert!(x.iter().all(|s| s.is_finite()), "{v:?} produced NaN/inf");
                        let p = peak(&x);
                        assert!(p <= 1.0, "{v:?} t{tune} d{decay} k{tone}: peak {p}");
                        assert!(
                            p > 0.1,
                            "{v:?} t{tune} d{decay} k{tone}: nearly silent, peak {p}"
                        );
                        assert!(rms(&x[..2400]) > 0.01, "{v:?}: first 50 ms too quiet");
                    }
                }
            }
        }
    }

    #[test]
    fn voices_decay_to_silence() {
        for v in VoiceId::ALL {
            let x = hit(v, settings(0.0, 1.0, 0.5), true, 6.0);
            let tail = &x[x.len() - 4800..];
            assert!(
                tail.iter().all(|s| *s == 0.0),
                "{v:?} still sounding after 6 s: {}",
                peak(tail)
            );
        }
    }

    #[test]
    fn accent_is_louder_and_level_zero_is_silent() {
        for v in VoiceId::ALL {
            let a = peak(&hit(v, settings(0.0, 0.5, 0.5), true, 0.5));
            let n = peak(&hit(v, settings(0.0, 0.5, 0.5), false, 0.5));
            assert!(a > n * 1.3, "{v:?}: accent {a} vs {n}");
            let silent = VoiceSettings {
                level: 0.0,
                ..settings(0.0, 0.5, 0.5)
            };
            assert_eq!(peak(&hit(v, silent, true, 0.5)), 0.0);
        }
    }

    #[test]
    fn decay_knob_lengthens_the_kick() {
        let short = hit(VoiceId::Kick, settings(0.0, 0.0, 0.3), true, 3.0);
        let long = hit(VoiceId::Kick, settings(0.0, 1.0, 0.3), true, 3.0);
        let audible = |x: &[f32]| x.iter().rposition(|s| s.abs() > 0.01).unwrap() as f32 / SR;
        assert!(audible(&short) < 0.3, "{}", audible(&short));
        assert!(audible(&long) > 1.2, "{}", audible(&long));
    }

    #[test]
    fn kick_tune_moves_its_pitch() {
        // Count zero crossings in the tail, after the pitch envelope has settled.
        let zc = |tune: f32| {
            let x = hit(VoiceId::Kick, settings(tune, 1.0, 0.0), true, 1.0);
            let t = &x[24_000..48_000];
            t.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32 * 2.0
        };
        let (lo, mid, hi) = (zc(-1.0), zc(0.0), zc(1.0));
        assert!((mid - 49.0).abs() < 4.0, "home pitch {mid} Hz");
        assert!(lo < mid && mid < hi, "{lo} {mid} {hi}");
    }

    #[test]
    fn retriggering_a_ringing_voice_does_not_jump() {
        // Low tom with SKIN at zero is a pure sine, so any jump would be the retrigger.
        let mut kit = Kit::new(SR);
        let s = settings(0.0, 1.0, 0.0);
        let mut f = [0.0; VOICES];
        kit.trigger(VoiceId::LowTom, &s, true);
        // Run to a point where the sine is near a peak, then hit again.
        let mut prev = 0.0;
        for _ in 0..6000 {
            kit.next(&mut f);
            prev = f[5];
        }
        assert!(prev.abs() > 0.05, "{prev}");
        kit.trigger(VoiceId::LowTom, &s, true);
        kit.next(&mut f);
        assert!((f[5] - prev).abs() < 0.02, "jump {prev} -> {}", f[5]);
    }

    #[test]
    fn closed_hat_chokes_open_hat() {
        let mut kit = Kit::new(SR);
        let s = settings(0.0, 1.0, 0.5);
        kit.trigger(VoiceId::OpenHat, &s, true);
        let mut f = [0.0; VOICES];
        for _ in 0..2400 {
            kit.next(&mut f);
        }
        assert!(f[VoiceId::OpenHat.index()].abs() > 0.0);
        kit.trigger(VoiceId::ClosedHat, &s, true);
        let mut late = 0f32;
        for i in 0..4800 {
            kit.next(&mut f);
            if i > 2400 {
                late = late.max(f[VoiceId::OpenHat.index()].abs());
            }
        }
        assert!(late < 1e-3, "open hat rang through the choke: {late}");
    }
}
