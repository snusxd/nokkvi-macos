//! The Cover Beat visualizer (`VisualizationMode::CoverBeat`): no shader, the
//! now-playing cover itself swells on each kick drum. The kick comes from the
//! visualizer worker's detector (`widgets/visualizer/kick.rs`), which runs on
//! the same audio feed every mode opens; this step polls it once per frame
//! and the cover panels (Theater, Queue, Radios) draw the swell through
//! [`Nokkvi::cover_beat_scale`].

use std::time::Instant;

use nokkvi_data::types::player_settings::VisualizationMode;

use crate::Nokkvi;

/// One frame: read the detector and start a swell on a new kick (only while
/// the mode is on and the music plays; a kick is still marked seen otherwise,
/// so turning the mode on never replays a stale one).
pub(crate) fn step(app: &mut Nokkvi, now: Instant) {
    let Some((seq, strength)) = app.visualizer.as_ref().map(|viz| viz.cover_kick()) else {
        return;
    };
    let swelling = app.cover_beat_on() && app.playback.playing && !app.playback.paused;
    app.cover_beat.observe(seq, strength, swelling, now);
}

impl Nokkvi {
    /// The Cover Beat visualizer is the active mode.
    pub(crate) fn cover_beat_on(&self) -> bool {
        self.settings.visualization_mode == VisualizationMode::CoverBeat
    }

    /// The now-playing cover's scale at `now` (`1.0` at rest or with another
    /// mode): the swell times the Cover Beat Strength's peak growth.
    pub(crate) fn cover_beat_scale(&self, now: Instant) -> f32 {
        if !self.cover_beat_on() {
            return 1.0;
        }
        1.0 + self.settings.cover_beat_strength.peak_growth() * self.cover_beat.level(now)
    }
}
