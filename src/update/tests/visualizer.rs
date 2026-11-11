//! Visualizer settings-pipeline handler tests (M3).
//!
//! The visualizer behavior config rides the unified settings path:
//! `LivePlayerSettings.visualizer` (sourced from the manager's in-memory
//! config.toml-backed field) is pushed onto the render-path
//! `SharedVisualizerConfig` by `handle_player_settings_loaded`.

use crate::{test_helpers::test_app, visualizer_config::SharedVisualizerConfigExt};

/// Invariant §5 #15 (dual config.toml write): the secondary (auto-disabled)
/// key writes are DERIVED from what the data-crate setter actually changed
/// (before/after diff) — the setter is the single owner of the
/// monstercat↔waves exclusivity rule, and config.toml must receive BOTH keys
/// because config.toml wins on reload.
#[test]
fn visualizer_secondary_writes_derive_from_setter_state() {
    use nokkvi_data::types::visualizer_config::{VisualizerConfig, keys};

    use crate::{
        update::settings::visualizer_secondary_writes, views::settings::items::SettingValue,
    };

    // Enabling monstercat flipped waves off in the setter → waves=false must
    // be persisted alongside.
    let before = VisualizerConfig {
        waves: true,
        monstercat: 0.0,
        ..Default::default()
    };
    let after = VisualizerConfig {
        waves: false,
        monstercat: 1.0,
        ..Default::default()
    };
    let writes = visualizer_secondary_writes(keys::MONSTERCAT, &before, &after);
    assert_eq!(
        writes.len(),
        1,
        "exactly the auto-disabled sibling: {writes:?}"
    );
    assert!(
        matches!(&writes[0], (k, SettingValue::Bool(false)) if *k == keys::WAVES),
        "got {writes:?}"
    );

    // Enabling waves zeroed monstercat in the setter → monstercat=0.0 must be
    // persisted alongside, carrying the setter's actual post-value.
    let before = VisualizerConfig {
        waves: false,
        monstercat: 1.0,
        ..Default::default()
    };
    let after = VisualizerConfig {
        waves: true,
        monstercat: 0.0,
        ..Default::default()
    };
    let writes = visualizer_secondary_writes(keys::WAVES, &before, &after);
    assert_eq!(writes.len(), 1, "got {writes:?}");
    assert!(
        matches!(&writes[0], (k, SettingValue::Float { val, .. }) if *k == keys::MONSTERCAT && *val == 0.0),
        "got {writes:?}"
    );

    // Sub-threshold monstercat snapped to 0.0 by validate, waves untouched —
    // monstercat is the PRIMARY key (its raw value was already written), so
    // no secondary write fires.
    let before = VisualizerConfig {
        waves: true,
        monstercat: 0.0,
        ..Default::default()
    };
    let after = VisualizerConfig {
        waves: true,
        monstercat: 0.0,
        ..Default::default()
    };
    assert!(visualizer_secondary_writes(keys::MONSTERCAT, &before, &after).is_empty());

    // Disabling waves leaves monstercat alone → nothing secondary.
    let before = VisualizerConfig {
        waves: true,
        monstercat: 0.0,
        ..Default::default()
    };
    let after = VisualizerConfig {
        waves: false,
        monstercat: 0.0,
        ..Default::default()
    };
    assert!(visualizer_secondary_writes(keys::WAVES, &before, &after).is_empty());

    // An unrelated key that changed neither field → nothing secondary.
    let same = VisualizerConfig::default();
    assert!(visualizer_secondary_writes(keys::BLOOM, &same, &same.clone()).is_empty());
}

/// `handle_player_settings_loaded` pushes `settings.visualizer` onto the
/// shared render-path config (replacing the legacy standalone
/// `VisualizerConfigChanged` reload).
#[test]
fn player_settings_loaded_applies_visualizer_to_shared_config() {
    // The load handler mirrors every theme atomic: serialize with the other
    // theme-mutating tests (THEME_MODE_LOCK).
    let _guard = crate::theme::THEME_MODE_LOCK.lock();
    let mut app = test_app();
    let mut settings = nokkvi_data::types::player_settings::LivePlayerSettings::default();
    settings.visualizer.noise_reduction = 0.42;
    settings.visualizer.bars.led_bars = true;
    settings.visualizer.scope.point_count = 128;

    let _ = app.handle_player_settings_loaded(settings);

    let snap = app.visualizer_config.snapshot();
    assert_eq!(
        snap.noise_reduction, 0.42,
        "shared config must receive the loaded visualizer settings"
    );
    assert!(snap.bars.led_bars);
    assert_eq!(snap.scope.point_count, 128);
}

/// Review #1: a visualizer-only write must ride the SLIM apply path — it
/// mirrors onto `self.settings.visualizer`, pushes the shared render config,
/// and refreshes entries, WITHOUT re-applying playback/SFX volume (the
/// startup-grade `handle_player_settings_loaded` would clobber an in-flight
/// async volume persist with the manager's stale snapshot).
#[test]
fn apply_visualizer_settings_leaves_playback_state_alone() {
    let mut app = test_app();
    app.settings.volume = 0.77;
    app.settings.sfx_volume = 0.55;
    app.settings_page.config_dirty = false;

    let viz = nokkvi_data::types::visualizer_config::VisualizerConfig {
        noise_reduction: 0.42,
        ..Default::default()
    };
    let _ = app.apply_visualizer_settings(viz);

    assert_eq!(
        app.settings.volume, 0.77,
        "visualizer apply must not touch playback volume"
    );
    assert_eq!(
        app.settings.sfx_volume, 0.55,
        "visualizer apply must not touch SFX volume"
    );
    assert_eq!(
        app.visualizer_config.snapshot().noise_reduction,
        0.42,
        "shared render config must receive the change"
    );
    assert_eq!(
        app.settings.visualizer.noise_reduction, 0.42,
        "the live settings mirror must receive the change"
    );
    assert!(
        app.settings_page.config_dirty,
        "settings entries must be marked for refresh"
    );
}

// ============================================================================
// FFT worker mode: written from update, never from view()
// ============================================================================

/// A test app with a real visualizer (its FFT worker idles: nothing feeds it).
fn app_with_visualizer() -> crate::Nokkvi {
    let mut app = test_app();
    app.visualizer = Some(crate::widgets::visualizer::Visualizer::new(
        192,
        app.visualizer_config.clone(),
        app.milkdrop.shared.clone(),
    ));
    app
}

/// Each cycle step points the FFT worker at the new mode before any view()
/// pass runs, and Off closes the feed gate.
#[test]
fn cycling_the_visualizer_points_the_fft_worker_at_each_mode() {
    use nokkvi_data::types::player_settings::VisualizationMode as Setting;

    use crate::widgets::visualizer::VisualizationMode as Worker;

    let mut app = app_with_visualizer();
    app.settings.visualization_mode = Setting::Bars;
    for expected in [Worker::Lines, Worker::Scope, Worker::Milkdrop] {
        let _ = app.handle_cycle_visualization();
        let viz = app.visualizer.as_ref().expect("visualizer");
        assert_eq!(viz.worker_mode(), expected);
        assert!(viz.feed_active(), "{expected:?} keeps the feed open");
    }

    // Cover Beat draws no shader but reads the feed; the worker leaves
    // MilkDrop's analyzer for the spectrum path.
    let _ = app.handle_cycle_visualization();
    assert_eq!(app.settings.visualization_mode, Setting::CoverBeat);
    let viz = app.visualizer.as_ref().expect("visualizer");
    assert_eq!(viz.worker_mode(), Worker::Bars);
    assert!(viz.feed_active(), "Cover Beat keeps the feed open");

    let _ = app.handle_cycle_visualization();
    assert_eq!(app.settings.visualization_mode, Setting::Off);
    let viz = app.visualizer.as_ref().expect("visualizer");
    assert!(!viz.feed_active(), "Off idles the worker");

    let _ = app.handle_cycle_visualization();
    let viz = app.visualizer.as_ref().expect("visualizer");
    assert_eq!(viz.worker_mode(), Worker::Bars);
    assert!(viz.feed_active());
}

/// Loading settings (login, hot reload) points the worker at the loaded mode,
/// and leaving MilkDrop through a reload moves it off the analyzer.
#[test]
fn loaded_settings_point_the_fft_worker_at_their_mode() {
    // The load handler mirrors every theme atomic: serialize with the other
    // theme-mutating tests (THEME_MODE_LOCK).
    let _guard = crate::theme::THEME_MODE_LOCK.lock();
    use nokkvi_data::types::player_settings::{LivePlayerSettings, VisualizationMode as Setting};

    use crate::widgets::visualizer::VisualizationMode as Worker;

    let mut app = app_with_visualizer();
    for (setting, expected) in [
        (Setting::Scope, Worker::Scope),
        (Setting::Milkdrop, Worker::Milkdrop),
        (Setting::Lines, Worker::Lines),
    ] {
        let settings = LivePlayerSettings {
            visualization_mode: setting,
            ..Default::default()
        };
        let _ = app.handle_player_settings_loaded(settings);
        let viz = app.visualizer.as_ref().expect("visualizer");
        assert_eq!(viz.worker_mode(), expected);
        assert!(viz.feed_active());
    }
}

/// Over the cover, the Reflection's water hangs below the cover only while
/// that mode's Reflection Below Cover is on: the visualizer then carries
/// the spill floor. Off, or without the Reflection, the water stays on the
/// cover. Each mode follows its own toggle.
#[test]
fn reflection_below_cover_gates_the_over_cover_spill() {
    use nokkvi_data::types::player_settings::VisualizationMode as Setting;

    let mut app = app_with_visualizer();
    // The test app reads the developer's config.debug.toml; start from the
    // shipped defaults (Bars / Lines over the cover) instead.
    *app.visualizer_config.write() = crate::visualizer_config::VisualizerConfig::default();
    let spill_floor = |app: &crate::Nokkvi| {
        let (viz, _boat) = app.over_cover_overlays();
        viz.expect("Bars / Lines draw over the cover by default")
            .0
            .spill_floor_value()
    };

    app.settings.visualization_mode = Setting::Bars;
    app.visualizer_config.write().bars.reflection = true;
    assert_eq!(
        spill_floor(&app),
        Some(app.reflection_spill_floor()),
        "on by default"
    );
    app.visualizer_config.write().bars.reflection_below_cover = false;
    assert_eq!(spill_floor(&app), None, "off keeps the water on the cover");

    app.settings.visualization_mode = Setting::Lines;
    assert_eq!(spill_floor(&app), None, "no Lines Reflection yet");
    app.visualizer_config.write().lines.reflection = true;
    assert!(spill_floor(&app).is_some(), "Lines follows its own toggle");
    app.visualizer_config.write().lines.reflection_below_cover = false;
    assert_eq!(spill_floor(&app), None);
}

// ============================================================================
// Cover Beat
// ============================================================================

mod cover_beat {
    use std::time::{Duration, Instant};

    use nokkvi_data::types::player_settings::{CoverBeatStrength, VisualizationMode};

    use crate::{state::CoverPulse, test_helpers::test_app};

    fn cover_beat_app(strength: CoverBeatStrength) -> crate::Nokkvi {
        let mut app = test_app();
        app.settings.visualization_mode = VisualizationMode::CoverBeat;
        app.settings.cover_beat_strength = strength;
        app
    }

    /// One kick at `t0`: the first read only syncs, the second is new.
    fn kick(app: &mut crate::Nokkvi, t0: Instant) {
        app.cover_beat.observe(7, 1.0, true, t0);
        app.cover_beat.observe(8, 1.0, true, t0);
    }

    #[test]
    fn cover_beat_follows_off_in_the_cycle_after_milkdrop() {
        assert_eq!(
            VisualizationMode::Milkdrop.next(),
            VisualizationMode::CoverBeat
        );
        assert_eq!(VisualizationMode::CoverBeat.next(), VisualizationMode::Off);
    }

    #[test]
    fn cover_beat_opens_the_feed_and_draws_no_shader() {
        let app = cover_beat_app(CoverBeatStrength::Subtle);
        assert!(
            app.visualizer_feed_wanted(),
            "the kick detector needs the feed"
        );
        let slots = crate::widgets::visualizer::resolve_placement(
            VisualizationMode::CoverBeat,
            crate::visualizer_config::VisualizerPlacement::OverCover,
            crate::visualizer_config::VisualizerPlacement::BottomBand,
        );
        assert_eq!(slots.over_art, None);
        assert_eq!(slots.bottom_band, None);
    }

    #[test]
    fn another_mode_keeps_the_cover_still() {
        let mut app = cover_beat_app(CoverBeatStrength::Punchy);
        app.settings.visualization_mode = VisualizationMode::Scope;
        let t0 = Instant::now();
        kick(&mut app, t0);
        let at_peak = t0 + Duration::from_millis(35);
        assert!((app.cover_beat_scale(at_peak) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_kick_swells_the_cover_and_it_settles() {
        let mut app = cover_beat_app(CoverBeatStrength::Punchy);
        let t0 = Instant::now();
        app.cover_beat.observe(7, 1.0, true, t0);
        assert!(app.cover_beat.swell.is_none(), "the first read only syncs");
        app.cover_beat.observe(8, 1.0, true, t0);
        let peak = app.cover_beat_scale(t0 + Duration::from_millis(35));
        let growth = CoverBeatStrength::Punchy.peak_growth();
        assert!(
            (peak - (1.0 + growth)).abs() < 1e-3,
            "a full-strength kick reaches the strength's growth: {peak}"
        );
        let later = app.cover_beat_scale(t0 + Duration::from_millis(200));
        assert!(later > 1.0 && later < peak, "it falls back: {later}");
        let settled = t0 + Duration::from_secs(2);
        assert!((app.cover_beat_scale(settled) - 1.0).abs() < 1e-4);
        assert!(!app.cover_beat.in_motion(settled));
    }

    #[test]
    fn punchy_grows_further_than_subtle() {
        let t0 = Instant::now();
        let peak = |strength| {
            let mut app = cover_beat_app(strength);
            kick(&mut app, t0);
            app.cover_beat_scale(t0 + Duration::from_millis(35))
        };
        assert!(peak(CoverBeatStrength::Punchy) > peak(CoverBeatStrength::Subtle));
        assert!(peak(CoverBeatStrength::Subtle) > 1.0);
    }

    #[test]
    fn a_kick_while_paused_starts_no_swell() {
        let mut pulse = CoverPulse::default();
        let now = Instant::now();
        pulse.observe(0, 1.0, false, now);
        pulse.observe(1, 1.0, false, now);
        assert!(pulse.swell.is_none());
        assert_eq!(pulse.seen, Some(1), "the kick is still marked seen");
    }

    #[test]
    fn a_kick_mid_fall_rises_from_where_the_cover_stands() {
        let mut pulse = CoverPulse::default();
        let t0 = Instant::now();
        pulse.observe(0, 1.0, true, t0);
        pulse.observe(1, 1.0, true, t0);
        let t1 = t0 + Duration::from_millis(120);
        let before = pulse.level(t1);
        pulse.observe(2, 0.6, true, t1);
        assert!(
            (pulse.level(t1) - before).abs() < 1e-4,
            "no jump on the new kick"
        );
    }

    #[test]
    fn the_frame_step_starts_a_swell_only_while_playing() {
        let mut app = super::app_with_visualizer();
        app.settings.visualization_mode = VisualizationMode::CoverBeat;
        app.playback.playing = true;
        app.playback.paused = false;
        let now = Instant::now();
        crate::update::cover_beat::step(&mut app, now);
        assert_eq!(app.cover_beat.seen, Some(0), "the first frame syncs");
        assert!(app.cover_beat.swell.is_none(), "no kick yet");
    }

    #[test]
    fn a_settling_swell_keeps_the_frames_running_after_the_mode_leaves() {
        let mut app = cover_beat_app(CoverBeatStrength::Subtle);
        app.playback.playing = false;
        app.settings.visualization_mode = VisualizationMode::Off;
        let now = Instant::now();
        kick(&mut app, now);
        assert!(app.frames_wanted(), "a swell in flight needs frames");
    }
}
