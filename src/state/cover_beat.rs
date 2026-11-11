//! The Cover Beat visualizer's state (`Nokkvi.cover_beat`): the now-playing
//! cover swells on each kick the visualizer worker's kick detector reports
//! (`widgets/visualizer/kick.rs`). Stepped per frame by
//! `update::cover_beat::step`, read by the cover panels through
//! `Nokkvi::cover_beat_scale`.

use std::time::Instant;

/// Cover Beat's state: the last kick seen from the visualizer's detector and
/// the swell it started. The swell is a pure function of time
/// ([`CoverPulse::level`]), so it stays smooth at any display rate.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoverPulse {
    /// The detector's kick sequence last read (`None`: nothing read yet, so
    /// the first read only syncs and never fires a stale kick).
    pub seen: Option<u32>,
    /// The swell in flight: when it began, the level it rose from, its peak.
    pub swell: Option<Swell>,
}

/// One kick's swell. See [`CoverPulse::level`].
#[derive(Debug, Clone, Copy)]
pub struct Swell {
    pub since: Instant,
    pub from: f32,
    pub peak: f32,
}

/// The swell's rise: quick, so the cover lands on the kick.
const SWELL_ATTACK_SECS: f32 = 0.035;
/// The fall's time constant after the peak.
const SWELL_RELEASE_SECS: f32 = 0.14;
/// Below this the swell is over.
const SWELL_DONE: f32 = 0.002;

impl CoverPulse {
    /// The swell's level (`0.0..=1.0`) at `now`: an ease-out rise from where
    /// the last one stood to the kick's strength over [`SWELL_ATTACK_SECS`],
    /// then an exponential fall. A kick landing mid-fall starts from the
    /// current level, so the cover never jumps.
    pub fn level(&self, now: Instant) -> f32 {
        let Some(s) = self.swell else {
            return 0.0;
        };
        let t = now.saturating_duration_since(s.since).as_secs_f32();
        let v = if t < SWELL_ATTACK_SECS {
            let x = t / SWELL_ATTACK_SECS;
            let ease = 1.0 - (1.0 - x) * (1.0 - x);
            s.from + (s.peak - s.from) * ease
        } else {
            s.peak * (-(t - SWELL_ATTACK_SECS) / SWELL_RELEASE_SECS).exp()
        };
        if v < SWELL_DONE {
            0.0
        } else {
            v.clamp(0.0, 1.0)
        }
    }

    /// Whether a swell is still moving at `now`.
    pub fn in_motion(&self, now: Instant) -> bool {
        self.swell.is_some_and(|s| {
            now.saturating_duration_since(s.since).as_secs_f32() < SWELL_ATTACK_SECS
        }) || self.level(now) > 0.0
    }

    /// The detector reported `(seq, strength)` at `now`. Starts a swell when
    /// `seq` is new and `live` (the Cover Beat visualizer on and the music
    /// playing); a new sequence is always marked seen.
    pub fn observe(&mut self, seq: u32, strength: f32, live: bool, now: Instant) {
        let fresh = self.seen.is_some_and(|seen| seen != seq);
        self.seen = Some(seq);
        if fresh && live {
            let from = self.level(now);
            self.swell = Some(Swell {
                since: now,
                from,
                peak: strength.clamp(0.0, 1.0).max(from),
            });
        }
        if !self.in_motion(now) {
            self.swell = None;
        }
    }
}
