//! Kick-drum onset detector for Theater Mode's Cover Beat.
//!
//! The spectrum-wide beat pulse (`VisualizerState::current_beat_pulse`) reacts
//! to any transient (a snare, a hi-hat run, a vocal entry) and only runs in
//! the spectrum modes. A kick is narrower: a sharp jump of energy below
//! ~150 Hz. This detector reads the raw PCM the FFT worker drains, in every
//! mode (MilkDrop included), and fires once per kick:
//!
//! 1. **Band.** Mono, high-passed at [`HIGH_PASS_HZ`] (rumble, DC) and
//!    low-passed by a 6th-order Butterworth at [`LOW_PASS_HZ`] (a 220 Hz snare
//!    body sits ~27 dB down), and a kick must carry at least
//!    [`MIN_BAND_SHARE`] of the whole signal's energy at that moment, so the
//!    snare's leftover and the hats never read as kicks.
//! 2. **Energy.** Mean square over a [`WINDOW_HOPS`] x [`HOP_SECS`] window
//!    (20 ms), advanced every hop (5 ms). The window spans at least one full
//!    ripple period of a 50 Hz kick, so the energy does not wobble with the
//!    waveform's phase.
//! 3. **Onset function.** Log-energy rise against the window just before it
//!    (half-wave rectified): loudness-independent, so a quiet intro and a
//!    loud drop both read their kicks. The log carries a floor tied to the
//!    song's running bass level, so a near-silent bump cannot read as a jump.
//! 4. **Adaptive threshold.** Running mean + [`THRESHOLD_STDS`] standard
//!    deviations of the onset function (about [`STATS_SECS`] of memory),
//!    never below [`MIN_RISE`]: a busy bass line raises the bar, a sparse
//!    four-on-the-floor lowers it.
//! 5. **Peak picking.** Fires on the hop after the onset function peaks above
//!    the threshold (one hop of latency, 5 ms), at most once per
//!    [`MIN_GAP_SECS`], and only when the band is louder than a fraction of
//!    its running level.
//!
//! The strength (`0.55..=1.0`) grows with how far the peak cleared the
//! threshold, so a heavy kick swells the cover further than a ghost note.

use std::f64::consts::PI;

const HIGH_PASS_HZ: f64 = 38.0;
const LOW_PASS_HZ: f64 = 130.0;
/// One analysis hop.
const HOP_SECS: f64 = 0.005;
/// Hops per energy window (20 ms).
const WINDOW_HOPS: usize = 4;
/// About how much of the onset function the threshold statistics remember.
const STATS_SECS: f64 = 1.5;
/// About how long the running bass level remembers.
const LEVEL_SECS: f64 = 3.0;
/// Standard deviations above the mean the onset must clear.
const THRESHOLD_STDS: f64 = 1.6;
/// The smallest log-energy rise that may count as a kick (about x1.6 the
/// energy of the window before).
const MIN_RISE: f64 = 0.45;
/// The band must be at least this fraction of its running level to fire.
const MIN_LEVEL_SHARE: f64 = 0.35;
/// The band's least share of the full signal's energy for a kick (a kick
/// dominates the low end; a snare's leftover below the cutoff is ~0.2%).
const MIN_BAND_SHARE: f64 = 0.04;
/// The log's floor, as a share of the running level.
const FLOOR_SHARE: f64 = 0.05;
/// Below this mean square (about -70 dBFS) the band is silence.
const SILENCE: f64 = 1e-7;
/// The shortest gap between two kicks (a 1/16 double kick at 150 BPM is 100 ms).
const MIN_GAP_SECS: f64 = 0.09;
/// Strength range of a fired kick.
const STRENGTH_MIN: f32 = 0.55;

/// A transposed direct form II biquad.
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ cookbook low- or high-pass at `hz` with quality `q`.
    fn new(rate: f64, hz: f64, q: f64, high: bool) -> Self {
        let w0 = 2.0 * PI * (hz / rate).min(0.49);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = if high {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// See the module docs. Fed interleaved stereo in `-1.0..=1.0`.
pub(crate) struct KickDetector {
    rate: u32,
    filters: [Biquad; 4],
    hop_len: usize,
    hop_fill: usize,
    hop_sum: f64,
    /// The full signal's energy over the same hop.
    hop_full: f64,
    /// The last `WINDOW_HOPS` full-signal hop energies, oldest first.
    full: [f64; WINDOW_HOPS],
    /// The last `2 * WINDOW_HOPS` hop energies, oldest first.
    hops: [f64; 2 * WINDOW_HOPS],
    hops_seen: usize,
    level: f64,
    mean: f64,
    var: f64,
    /// The onset function on the previous hop, and whether it was over the
    /// threshold (a peak candidate).
    prev_rise: f64,
    prev_over: bool,
    prev_threshold: f64,
    hops_since_kick: usize,
    stats_alpha: f64,
    level_alpha: f64,
    min_gap_hops: usize,
}

impl KickDetector {
    pub(crate) fn new(rate: u32) -> Self {
        let r = f64::from(rate.max(8_000));
        let hop_len = (r * HOP_SECS).round().max(1.0) as usize;
        Self {
            rate,
            filters: [
                Biquad::new(r, HIGH_PASS_HZ, std::f64::consts::FRAC_1_SQRT_2, true),
                // A 6th-order Butterworth is three biquads at these Qs.
                Biquad::new(r, LOW_PASS_HZ, 0.517_638_1, false),
                Biquad::new(r, LOW_PASS_HZ, std::f64::consts::FRAC_1_SQRT_2, false),
                Biquad::new(r, LOW_PASS_HZ, 1.931_851_7, false),
            ],
            hop_len,
            hop_fill: 0,
            hop_sum: 0.0,
            hop_full: 0.0,
            full: [0.0; WINDOW_HOPS],
            hops: [0.0; 2 * WINDOW_HOPS],
            hops_seen: 0,
            level: 0.0,
            mean: 0.0,
            var: 0.0,
            prev_rise: 0.0,
            prev_over: false,
            prev_threshold: MIN_RISE,
            hops_since_kick: usize::MAX / 2,
            stats_alpha: HOP_SECS / STATS_SECS,
            level_alpha: HOP_SECS / LEVEL_SECS,
            min_gap_hops: (MIN_GAP_SECS / HOP_SECS).round() as usize,
        }
    }

    /// The stream rate this detector was built for.
    pub(crate) fn rate(&self) -> u32 {
        self.rate
    }

    /// Forget the signal history (a track change), keeping the rate.
    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.rate);
    }

    /// Run interleaved stereo through the detector. Returns the strength of
    /// the strongest kick that fired in it, if any.
    pub(crate) fn push_stereo(&mut self, samples: impl Iterator<Item = f32>) -> Option<f32> {
        let mut fired: Option<f32> = None;
        let mut left: Option<f32> = None;
        for s in samples {
            let Some(l) = left.take() else {
                left = Some(s);
                continue;
            };
            let mut x = f64::from(l + s) * 0.5;
            self.hop_full += x * x;
            for f in &mut self.filters {
                x = f.run(x);
            }
            self.hop_sum += x * x;
            self.hop_fill += 1;
            if self.hop_fill >= self.hop_len {
                let energy = self.hop_sum / self.hop_fill as f64;
                let full = self.hop_full / self.hop_fill as f64;
                self.hop_sum = 0.0;
                self.hop_full = 0.0;
                self.hop_fill = 0;
                if let Some(strength) = self.hop(energy, full) {
                    fired = Some(fired.map_or(strength, |f| f.max(strength)));
                }
            }
        }
        fired
    }

    /// One hop's band energy in; a kick's strength out on the hop it fires.
    fn hop(&mut self, energy: f64, full: f64) -> Option<f32> {
        self.full.rotate_left(1);
        self.full[WINDOW_HOPS - 1] = full;
        let full_now: f64 = self.full.iter().sum::<f64>() / WINDOW_HOPS as f64;
        self.hops.rotate_left(1);
        self.hops[2 * WINDOW_HOPS - 1] = energy;
        self.hops_seen = self.hops_seen.saturating_add(1);
        self.hops_since_kick = self.hops_since_kick.saturating_add(1);

        let now: f64 = self.hops[WINDOW_HOPS..].iter().sum::<f64>() / WINDOW_HOPS as f64;
        let before: f64 = self.hops[..WINDOW_HOPS].iter().sum::<f64>() / WINDOW_HOPS as f64;
        self.level += self.level_alpha * (now - self.level);
        if self.hops_seen < 2 * WINDOW_HOPS {
            return None;
        }

        let floor = self.level * FLOOR_SHARE + SILENCE;
        let rise = ((now + floor) / (before + floor)).ln().max(0.0);

        // Peak picking on the previous hop: it cleared its threshold and this
        // hop is lower, so it was the top of the rise.
        let mut fired = None;
        if self.prev_over
            && rise < self.prev_rise
            && self.hops_since_kick >= self.min_gap_hops
            && now > SILENCE
            && now > self.level * MIN_LEVEL_SHARE
            && now > full_now * MIN_BAND_SHARE
        {
            let over = (self.prev_rise - self.prev_threshold) / self.prev_threshold;
            let strength = STRENGTH_MIN + (1.0 - STRENGTH_MIN) * over.clamp(0.0, 1.0) as f32;
            self.hops_since_kick = 0;
            fired = Some(strength);
        }

        let threshold = (self.mean + THRESHOLD_STDS * self.var.sqrt()).max(MIN_RISE);
        // A peak that already fired must not fire again on its way down.
        self.prev_over = rise > threshold && fired.is_none() && self.hops_since_kick > 0;
        self.prev_rise = rise;
        self.prev_threshold = threshold;

        // Statistics after the decision, so a kick does not raise its own bar.
        let delta = rise - self.mean;
        self.mean += self.stats_alpha * delta;
        self.var = (1.0 - self.stats_alpha) * (self.var + self.stats_alpha * delta * delta);
        fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// `secs` of interleaved stereo from a mono generator over time.
    fn render(secs: f64, f: impl Fn(f64) -> f64) -> Vec<f32> {
        let n = (secs * f64::from(RATE)) as usize;
        let mut out = Vec::with_capacity(n * 2);
        for i in 0..n {
            let s = f(i as f64 / f64::from(RATE)) as f32;
            out.push(s);
            out.push(s);
        }
        out
    }

    /// A synthetic kick: a 55 Hz sine with a pitch drop and a fast decay.
    fn kick(t: f64) -> f64 {
        if t < 0.0 {
            return 0.0;
        }
        let freq = 55.0 + 90.0 * (-t * 40.0).exp();
        (2.0 * PI * freq * t).sin() * (-t * 9.0).exp() * 0.8
    }

    /// Kicks every `period` seconds plus `bed` (other content).
    fn track(secs: f64, period: f64, bed: impl Fn(f64) -> f64) -> Vec<f32> {
        render(secs, |t| kick(t % period) + bed(t))
    }

    /// Feed in ~16 ms chunks like the FFT worker, collecting the fire times.
    fn fire_times(samples: &[f32]) -> Vec<f64> {
        let mut d = KickDetector::new(RATE);
        let chunk = (RATE as usize * 2) / 60;
        let mut times = Vec::new();
        for (i, c) in samples.chunks(chunk).enumerate() {
            if d.push_stereo(c.iter().copied()).is_some() {
                times.push((i * chunk) as f64 / 2.0 / f64::from(RATE));
            }
        }
        times
    }

    #[test]
    fn fires_once_per_kick_on_a_four_on_the_floor() {
        // 120 BPM: a kick every 0.5 s for 6 s.
        let times = fire_times(&track(6.0, 0.5, |_| 0.0));
        assert!(
            (11..=12).contains(&times.len()),
            "about one fire per kick, got {times:?}"
        );
        for pair in times.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (0.45..0.55).contains(&gap),
                "fires sit on the beat: {times:?}"
            );
        }
    }

    #[test]
    fn ignores_hats_and_snares_above_the_bass() {
        // A dense hi-hat (8 kHz bursts every 125 ms) and a snare body at
        // 220 Hz on the off-beats, no kick.
        let times = fire_times(&render(4.0, |t| {
            let hat_t = t % 0.125;
            let snare_t = (t + 0.25) % 0.5;
            (2.0 * PI * 8_000.0 * t).sin() * (-hat_t * 60.0).exp() * 0.5
                + (2.0 * PI * 220.0 * t).sin() * (-snare_t * 20.0).exp() * 0.5
        }));
        assert!(times.len() <= 1, "no kicks in hats and snares: {times:?}");
    }

    #[test]
    fn a_steady_bass_note_is_no_kick() {
        let times = fire_times(&render(4.0, |t| (2.0 * PI * 60.0 * t).sin() * 0.5));
        assert!(
            times.len() <= 1,
            "a held note fires at most its start: {times:?}"
        );
    }

    #[test]
    fn reads_quiet_kicks_as_well_as_loud_ones() {
        let quiet: Vec<f32> = track(4.0, 0.5, |_| 0.0).iter().map(|s| s * 0.03).collect();
        let times = fire_times(&quiet);
        assert!(times.len() >= 6, "a -30 dB kick still fires: {times:?}");
    }

    #[test]
    fn silence_never_fires() {
        assert!(fire_times(&render(2.0, |_| 0.0)).is_empty());
    }

    #[test]
    fn kicks_over_a_busy_mix_still_fire() {
        // The kick under a bright pad and hats.
        let times = fire_times(&track(6.0, 0.5, |t| {
            (2.0 * PI * 440.0 * t).sin() * 0.2
                + (2.0 * PI * 660.0 * t).sin() * 0.15
                + (2.0 * PI * 9_000.0 * t).sin() * (-(t % 0.125) * 60.0).exp() * 0.3
        }));
        assert!(times.len() >= 10, "kicks fire through the mix: {times:?}");
    }

    #[test]
    fn strength_stays_in_range() {
        let mut d = KickDetector::new(RATE);
        let samples = track(3.0, 0.5, |_| 0.0);
        for c in samples.chunks(1600) {
            if let Some(s) = d.push_stereo(c.iter().copied()) {
                assert!((STRENGTH_MIN..=1.0).contains(&s), "strength {s}");
            }
        }
    }
}
