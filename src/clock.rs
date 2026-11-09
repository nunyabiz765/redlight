//! The sequencer clock. Runs inside the audio callback, one tick per sample,
//! so steps land on exact sample positions whatever the buffer size.
//!
//! Swing works on pairs of 16ths: the first 16th of each 8th note gets
//! `swing` of the pair's length and the second gets the rest, so 0.50 is
//! straight, 0.667 is a triplet shuffle, and a bar is always exactly 16
//! 16ths long. Time is kept in `f64` samples and step onsets are the first
//! sample at or after the exact time, so nothing drifts over long runs.

use crate::pattern::STEPS;

/// Length of one step in samples. `step` is the step that is starting.
pub fn step_len(step: usize, tempo: f32, swing: f32, sr: f32) -> f64 {
    let sixteenth = sr as f64 * 60.0 / tempo as f64 / 4.0;
    let pair = 2.0 * sixteenth;
    let sw = swing as f64;
    if step % 2 == 0 {
        pair * sw
    } else {
        pair * (1.0 - sw)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Clock {
    /// Samples since `start`.
    now: u64,
    /// Exact sample time the next step starts.
    next: f64,
    /// The step that fires at `next`.
    step: usize,
    running: bool,
}

impl Clock {
    pub fn start(&mut self) {
        *self = Clock {
            running: true,
            ..Clock::default()
        };
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// Advance one sample. Returns the step that starts on this sample, if any.
    /// Tempo and swing are read at each step boundary, so changes take effect
    /// from the next step without a jump.
    #[inline]
    pub fn tick(&mut self, tempo: f32, swing: f32, sr: f32) -> Option<usize> {
        if !self.running {
            return None;
        }
        let fired = if self.now as f64 >= self.next {
            let s = self.step;
            self.next += step_len(s, tempo, swing, sr);
            self.step = (s + 1) % STEPS;
            Some(s)
        } else {
            None
        };
        self.now += 1;
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onsets(tempo: f32, swing: f32, sr: f32, samples: usize) -> Vec<(u64, usize)> {
        let mut c = Clock::default();
        c.start();
        (0..samples as u64)
            .filter_map(|t| c.tick(tempo, swing, sr).map(|s| (t, s)))
            .collect()
    }

    #[test]
    fn straight_sixteenths_at_120() {
        // 120 BPM at 48 kHz: a 16th is 6000 samples.
        let o = onsets(120.0, 0.5, 48_000.0, 96_000 * 2);
        assert_eq!(o.len(), 32);
        for (i, &(t, s)) in o.iter().enumerate() {
            assert_eq!(t, i as u64 * 6000);
            assert_eq!(s, i % 16);
        }
    }

    #[test]
    fn swing_delays_the_offbeat_sixteenth_only() {
        // 0.625 is exact in binary, so the expected positions are exact integers.
        let o = onsets(120.0, 0.625, 48_000.0, 96_000);
        // Pairs are 12000 samples; offbeats land at 0.625 of the pair.
        for (i, &(t, _)) in o.iter().enumerate() {
            let pair = (i / 2) as u64 * 12_000;
            let want = if i % 2 == 0 { pair } else { pair + 7500 };
            assert_eq!(t, want, "step {i}");
        }
    }

    #[test]
    fn bar_length_does_not_depend_on_swing() {
        for swing in [0.5, 0.54, 0.58, 0.62, 0.667, 0.75] {
            let o = onsets(97.0, swing, 44_100.0, 44_100 * 6);
            // Bar = 4 beats at 97 BPM.
            let bar = 4.0 * 60.0 / 97.0 * 44_100.0;
            let bar_starts: Vec<u64> = o.iter().filter(|(_, s)| *s == 0).map(|(t, _)| *t).collect();
            for (k, &t) in bar_starts.iter().enumerate() {
                assert_eq!(t, (k as f64 * bar).ceil() as u64, "swing {swing}, bar {k}");
            }
        }
    }

    #[test]
    fn no_drift_over_a_long_run_at_an_awkward_tempo() {
        let (tempo, sr) = (93.7f32, 48_000.0f32);
        let o = onsets(tempo, 0.58, sr, 48_000 * 600);
        let len = step_len(0, tempo, 0.58, sr) + step_len(1, tempo, 0.58, sr);
        // Ten minutes in, every even step still lands within one sample of the ideal time.
        for (i, &(t, _)) in o.iter().enumerate().filter(|(i, _)| i % 2 == 0) {
            let ideal = (i / 2) as f64 * len;
            assert!((t as f64 - ideal).abs() <= 1.0, "step {i}: {t} vs {ideal}");
        }
        assert!(o.len() > 3500);
    }

    #[test]
    fn tempo_change_takes_effect_at_the_next_step() {
        let mut c = Clock::default();
        c.start();
        let mut fired = Vec::new();
        for t in 0..30_000u64 {
            let tempo = if t < 6000 { 120.0 } else { 60.0 };
            if let Some(s) = c.tick(tempo, 0.5, 48_000.0) {
                fired.push((t, s));
            }
        }
        // Step 0 at 0 (scheduled at 120), step 1 at 6000 (120), then 60 BPM: 12000 per step.
        assert_eq!(fired, vec![(0, 0), (6000, 1), (18_000, 2)]);
    }

    #[test]
    fn stopped_clock_is_silent_and_restart_begins_at_step_zero() {
        let mut c = Clock::default();
        assert_eq!(c.tick(120.0, 0.5, 48_000.0), None);
        c.start();
        assert_eq!(c.tick(120.0, 0.5, 48_000.0), Some(0));
        for _ in 0..7000 {
            c.tick(120.0, 0.5, 48_000.0);
        }
        c.stop();
        assert_eq!(c.tick(120.0, 0.5, 48_000.0), None);
        c.start();
        assert_eq!(c.tick(120.0, 0.5, 48_000.0), Some(0));
    }
}
