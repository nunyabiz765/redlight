//! Small DSP building blocks, lifted from Plaza (~/projects/plaza/src/dsp.rs,
//! same author, MIT OR Apache-2.0). Trimmed to what the drums and the master
//! chain need: no ladder filter or ADSR here.

use std::f32::consts::PI;

pub const TAU: f32 = 2.0 * PI;

/// PolyBLEP residual: subtract from a naive saw (or add/subtract at each edge of
/// a pulse) to round off the discontinuity and push aliasing well down.
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Band-limited sawtooth, phase in [0, 1).
#[inline]
pub fn saw(t: f32, dt: f32) -> f32 {
    2.0 * t - 1.0 - poly_blep(t, dt)
}

/// Band-limited pulse with duty `pw` in (0, 1).
#[inline]
pub fn pulse(t: f32, dt: f32, pw: f32) -> f32 {
    let mut y = if t < pw { 1.0 } else { -1.0 };
    y += poly_blep(t, dt);
    let mut t2 = t - pw;
    if t2 < 0.0 {
        t2 += 1.0;
    }
    y -= poly_blep(t2, dt);
    // Keep the average at zero whatever the width, so PWM doesn't thump.
    y - (2.0 * pw - 1.0)
}

/// Smooth saturator: matches tanh closely up to |x| = 3 and is exactly ±1 beyond.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x <= -3.0 {
        -1.0
    } else if x >= 3.0 {
        1.0
    } else {
        let x2 = x * x;
        (x * (27.0 + x2) / (27.0 + 9.0 * x2)).clamp(-1.0, 1.0)
    }
}

/// Fast sine for phase in [0, 1). Parabolic approximation with one refinement
/// step; error well under 0.1%, which is inaudible on an LFO or FM operator.
#[inline]
pub fn sin01(t: f32) -> f32 {
    let x = t * TAU;
    let x = if x > PI { x - TAU } else { x };
    let y = (4.0 / PI) * x - (4.0 / (PI * PI)) * x * x.abs();
    0.225 * (y * y.abs() - y) + y
}

#[inline]
pub fn wrap01(t: f32) -> f32 {
    t - t.floor()
}

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn midi_to_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// xorshift32 noise. Deterministic per seed, so renders are reproducible.
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.max(1))
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [-1, 1).
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }
}

/// One-pole low-pass, coefficient from a cutoff in Hz.
#[derive(Clone, Default)]
pub struct OnePole {
    a: f32,
    pub z: f32,
}

impl OnePole {
    pub fn new(hz: f32, sr: f32) -> Self {
        let mut f = OnePole::default();
        f.set(hz, sr);
        f
    }
    pub fn set(&mut self, hz: f32, sr: f32) {
        self.a = 1.0 - (-TAU * hz / sr).exp();
    }
    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.z += self.a * (x - self.z);
        self.z
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        x - self.lp(x)
    }
}

/// DC blocker (leaky differentiator, ~10 Hz corner at 48 kHz).
#[derive(Clone, Default)]
pub struct DcBlock {
    x1: f32,
    y1: f32,
    r: f32,
}

impl DcBlock {
    pub fn new(sr: f32) -> Self {
        DcBlock {
            x1: 0.0,
            y1: 0.0,
            r: 1.0 - TAU * 10.0 / sr,
        }
    }
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Topology-preserving (trapezoidal) state-variable filter, used for fixed tone
/// shaping in the effects. Stable under modulation.
#[derive(Clone, Default)]
pub struct Svf {
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    pub fn new(hz: f32, q: f32, sr: f32) -> Self {
        let mut f = Svf::default();
        f.set(hz, q, sr);
        f
    }
    pub fn set(&mut self, hz: f32, q: f32, sr: f32) {
        let hz = hz.clamp(10.0, sr * 0.49);
        self.g = (PI * hz / sr).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }
    #[inline]
    pub fn run(&mut self, x: f32) -> SvfOut {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut {
            lp: v2,
            bp: v1,
            hp: x - self.k * v1 - v2,
        }
    }
    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.run(x).lp
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        self.run(x).hp
    }
}

/// Circular delay line with 4-point Hermite interpolation for modulated reads.
#[derive(Clone)]
pub struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
    mask: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let n = (max_samples + 4).next_power_of_two();
        DelayLine {
            buf: vec![0.0; n],
            pos: 0,
            mask: n - 1,
        }
    }
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }
    /// Integer tap, `d` samples ago (d >= 0).
    #[inline]
    pub fn tap(&self, d: usize) -> f32 {
        self.buf[(self.pos.wrapping_sub(d)) & self.mask]
    }
    /// Fractional tap, `d` samples ago (d >= 1).
    #[inline]
    pub fn read(&self, d: f32) -> f32 {
        let d = d.max(1.0);
        let i = d as usize;
        let f = d - i as f32;
        let xm1 = self.tap(i - 1);
        let x0 = self.tap(i);
        let x1 = self.tap(i + 1);
        let x2 = self.tap(i + 2);
        let c1 = 0.5 * (x1 - xm1);
        let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
        ((c3 * f + c2) * f + c1) * f + x0
    }
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_clip_is_bounded_and_odd() {
        for i in -1000..=1000 {
            let x = i as f32 * 0.01;
            let y = soft_clip(x);
            assert!(y.abs() <= 1.0);
            assert!((y + soft_clip(-x)).abs() < 1e-6);
        }
        assert!((soft_clip(0.5) - 0.5f32.tanh()).abs() < 0.01);
    }

    #[test]
    fn fast_sine_is_close() {
        for i in 0..1000 {
            let t = i as f32 / 1000.0;
            assert!((sin01(t) - (t * TAU).sin()).abs() < 0.002, "{t}");
        }
    }

    #[test]
    fn hermite_reads_between_samples() {
        let mut d = DelayLine::new(64);
        for i in 0..32 {
            d.push(i as f32);
        }
        // Last pushed is 31, so 2.5 samples ago is 28.5 on a ramp.
        assert!((d.read(2.5) - 28.5).abs() < 1e-4);
    }
}
