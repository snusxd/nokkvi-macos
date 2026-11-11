//! Visualization mode (Off / Bars / Lines / Scope / MilkDrop / Cover Beat) and
//! the Cover Beat strength.

use serde::{Deserialize, Serialize};

use crate::define_labeled_enum;

/// Visualization mode for the audio visualizer.
///
/// Cycles: Off → Bars → Lines → Scope → MilkDrop → Cover Beat → Off via
/// `next()`.
/// Serializes to lowercase strings for redb storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualizationMode {
    /// Default: the visualizer is opt-in (cycle it on with its hotkey).
    #[default]
    Off,
    Bars,
    Lines,
    /// Circular oscilloscope (time-domain waveform) drawn over the now-playing
    /// cover art. Cycled after Lines.
    Scope,
    /// Plays MilkDrop presets in the over-cover slot (it replaces the cover).
    /// Cycled after Scope. A pre-MilkDrop binary cannot parse `"milkdrop"`.
    Milkdrop,
    /// No shader: the now-playing cover itself swells on each kick drum
    /// (strength from [`CoverBeatStrength`]). Cycled after MilkDrop.
    #[serde(rename = "cover_beat")]
    CoverBeat,
}

impl VisualizationMode {
    /// Cycle to the next visualization mode.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Bars,
            Self::Bars => Self::Lines,
            Self::Lines => Self::Scope,
            Self::Scope => Self::Milkdrop,
            Self::Milkdrop => Self::CoverBeat,
            Self::CoverBeat => Self::Off,
        }
    }
}

impl std::fmt::Display for VisualizationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Bars => write!(f, "bars"),
            Self::Lines => write!(f, "lines"),
            Self::Scope => write!(f, "scope"),
            Self::Milkdrop => write!(f, "milkdrop"),
            Self::CoverBeat => write!(f, "cover_beat"),
        }
    }
}

define_labeled_enum! {
    /// How far the Cover Beat visualizer swells the cover on a kick.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum CoverBeatStrength {
        /// A gentle swell on each kick.
        #[default]
        Subtle { label: "Subtle", wire: "subtle" },
        /// A pronounced punch on each kick.
        Punchy { label: "Punchy", wire: "punchy" },
    }
}

impl CoverBeatStrength {
    /// How far the hardest kick grows the cover, as a fraction of its side.
    pub fn peak_growth(self) -> f32 {
        match self {
            Self::Subtle => 0.035,
            Self::Punchy => 0.075,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punchy_grows_further_than_subtle() {
        assert!(CoverBeatStrength::Subtle.peak_growth() > 0.0);
        assert!(CoverBeatStrength::Punchy.peak_growth() > CoverBeatStrength::Subtle.peak_growth());
    }

    #[test]
    fn cover_beat_mode_round_trips_through_toml() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Row {
            visualization_mode: VisualizationMode,
        }
        let row = Row {
            visualization_mode: VisualizationMode::CoverBeat,
        };
        let text = toml::to_string(&row).unwrap();
        assert!(text.contains("\"cover_beat\""), "{text}");
        assert_eq!(toml::from_str::<Row>(&text).unwrap(), row);
    }

    #[test]
    fn visualization_mode_cycles() {
        assert_eq!(VisualizationMode::Off.next(), VisualizationMode::Bars);
        assert_eq!(VisualizationMode::Bars.next(), VisualizationMode::Lines);
        assert_eq!(VisualizationMode::Lines.next(), VisualizationMode::Scope);
        assert_eq!(VisualizationMode::Scope.next(), VisualizationMode::Milkdrop);
        assert_eq!(
            VisualizationMode::Milkdrop.next(),
            VisualizationMode::CoverBeat
        );
        assert_eq!(VisualizationMode::CoverBeat.next(), VisualizationMode::Off);
    }

    #[test]
    fn milkdrop_round_trips_through_toml() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Row {
            visualization_mode: VisualizationMode,
        }
        let row = Row {
            visualization_mode: VisualizationMode::Milkdrop,
        };
        let text = toml::to_string(&row).unwrap();
        assert_eq!(text.trim(), "visualization_mode = \"milkdrop\"");
        assert_eq!(toml::from_str::<Row>(&text).unwrap(), row);
    }

    #[test]
    fn cycle_includes_milkdrop_after_scope() {
        let mut mode = VisualizationMode::Off;
        let mut seen = Vec::new();
        for _ in 0..6 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(
            seen,
            [
                VisualizationMode::Bars,
                VisualizationMode::Lines,
                VisualizationMode::Scope,
                VisualizationMode::Milkdrop,
                VisualizationMode::CoverBeat,
                VisualizationMode::Off,
            ]
        );
    }

    #[test]
    fn visualization_mode_default_is_bars() {
        assert_eq!(VisualizationMode::default(), VisualizationMode::Off);
    }

    #[test]
    fn visualization_mode_serde_roundtrip() {
        let modes = [
            VisualizationMode::Off,
            VisualizationMode::Bars,
            VisualizationMode::Lines,
            VisualizationMode::Scope,
            VisualizationMode::Milkdrop,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: VisualizationMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }

    #[test]
    fn visualization_mode_deserializes_from_lowercase_strings() {
        assert_eq!(
            serde_json::from_str::<VisualizationMode>("\"off\"").unwrap(),
            VisualizationMode::Off
        );
        assert_eq!(
            serde_json::from_str::<VisualizationMode>("\"bars\"").unwrap(),
            VisualizationMode::Bars
        );
        assert_eq!(
            serde_json::from_str::<VisualizationMode>("\"lines\"").unwrap(),
            VisualizationMode::Lines
        );
        assert_eq!(
            serde_json::from_str::<VisualizationMode>("\"scope\"").unwrap(),
            VisualizationMode::Scope
        );
        assert_eq!(
            serde_json::from_str::<VisualizationMode>("\"milkdrop\"").unwrap(),
            VisualizationMode::Milkdrop
        );
        assert_eq!(VisualizationMode::Milkdrop.to_string(), "milkdrop");
    }
}
