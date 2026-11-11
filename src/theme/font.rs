//! UI font configuration — independent of theme, stored in config.toml.

use std::{
    collections::HashMap,
    sync::{
        LazyLock,
        atomic::{AtomicI16, AtomicU16, Ordering},
    },
};

use iced::{Font, Pixels, font::Weight};
use nokkvi_data::types::player_settings::{
    FONT_SIZE_PX_DEFAULT, FONT_SIZE_PX_MAX, FONT_SIZE_PX_MIN, FontWeight,
};
use parking_lot::RwLock;

// ============================================================================
// Font Configuration (independent of theme — stored in config.toml)
// ============================================================================

/// Font family name, stored separately from themes so theme switches
/// don't change the user's font preference.
static FONT_FAMILY: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new(String::new()));

/// UI font size in px (14 is the designed size). Read on every text run.
static FONT_SIZE_PX: AtomicU16 = AtomicU16::new(FONT_SIZE_PX_DEFAULT as u16);

/// CSS-weight shift of the base font weight ([`FontWeight::css_shift`]).
static FONT_WEIGHT_SHIFT: AtomicI16 = AtomicI16::new(0);

/// Scale a designed text size by the user's Font Size. Every text run takes its
/// size through this, so one setting moves the whole UI; row heights and
/// chrome are fixed, so text grows or shrinks inside them.
#[inline]
pub(crate) fn fs(designed: impl Into<Pixels>) -> Pixels {
    let px = FONT_SIZE_PX.load(Ordering::Relaxed);
    Pixels(designed.into().0 * f32::from(px) / FONT_SIZE_PX_DEFAULT as f32)
}

/// Set the font size in px (clamped to the setting's range).
pub(crate) fn set_font_size_px(px: u32) {
    let px = px.clamp(FONT_SIZE_PX_MIN, FONT_SIZE_PX_MAX);
    FONT_SIZE_PX.store(px as u16, Ordering::Relaxed);
}

/// The current font size in px (for the settings UI).
pub(crate) fn font_size_px() -> u32 {
    u32::from(FONT_SIZE_PX.load(Ordering::Relaxed))
}

/// Set the base font weight.
pub(crate) fn set_font_weight(weight: FontWeight) {
    FONT_WEIGHT_SHIFT.store(weight.css_shift() as i16, Ordering::Relaxed);
}

/// The current base font weight (for the settings UI).
pub(crate) fn font_weight() -> FontWeight {
    match FONT_WEIGHT_SHIFT.load(Ordering::Relaxed) {
        s if s == FontWeight::Light.css_shift() as i16 => FontWeight::Light,
        s if s == FontWeight::Medium.css_shift() as i16 => FontWeight::Medium,
        s if s == FontWeight::Bold.css_shift() as i16 => FontWeight::Bold,
        _ => FontWeight::Regular,
    }
}

/// `requested` moved by the base weight step, clamped to the CSS scale and
/// snapped to a hundred. With the default (Regular) it is `requested` itself.
fn shifted_weight(requested: Weight) -> Weight {
    let shift = i32::from(FONT_WEIGHT_SHIFT.load(Ordering::Relaxed));
    if shift == 0 {
        return requested;
    }
    let css = (i32::from(weight_to_css(requested)) + shift).clamp(100, 900);
    css_to_weight(css as u16)
}

/// Cached font storage to avoid leaking memory on every reload.
/// Stores (font_name, base weight shift, resolved_font).
static FONT_CACHE: LazyLock<RwLock<(String, i16, Font)>> =
    LazyLock::new(|| RwLock::new((String::new(), 0, Font::DEFAULT)));

/// The line height of every text site. iced's own widgets read it from the
/// daemon `Settings` (set in `main`); nokkvi's custom text widgets name it
/// directly, because iced's `LineHeight::default()` no longer follows it.
///
/// iced 0.15 raised its default line height to `1.375`. nokkvi keeps `1.3`,
/// the value its row heights and text metrics are tuned against.
pub(crate) const UI_LINE_HEIGHT: iced::widget::text::LineHeight =
    iced::widget::text::LineHeight::Relative(1.3);

/// Get the UI font - loaded from config.toml (hot-reloadable)
/// Default: System sans-serif font (works on all systems)
#[inline]
pub(crate) fn ui_font() -> Font {
    let current_family = { FONT_FAMILY.read().clone() };
    let shift = FONT_WEIGHT_SHIFT.load(Ordering::Relaxed);

    // Fast path: check if cached font matches current config
    {
        let cache = FONT_CACHE.read();
        if cache.0 == current_family && cache.1 == shift {
            return cache.2;
        }
    }

    // Slow path: font or base weight changed, update cache
    let base = if current_family.is_empty() {
        Font::DEFAULT
    } else {
        // Family::name() interns the string in a global FxHashSet (leaked once, deduped)
        Font::with_family(iced::font::Family::name(&current_family))
    };
    let new_font = Font {
        weight: weighted_weight(&current_family, Weight::Normal),
        ..base
    };

    let mut cache = FONT_CACHE.write();
    *cache = (current_family, shift, new_font);

    new_font
}

/// Set the font family (called on startup and when changed in settings).
pub(crate) fn set_font_family(family: String) {
    // Warm the available-weights cache off the render path so the first frame
    // after a font change doesn't pay the font-introspection cost.
    if !family.is_empty() {
        let _ = available_weights(&family);
    }
    *FONT_FAMILY.write() = family;
}

/// Get the current font family name (for settings UI display).
pub(crate) fn font_family() -> String {
    FONT_FAMILY.read().clone()
}

// ----------------------------------------------------------------------------
// Weight-aware font construction
//
// A custom family that ships only a subset of weights (the common case for
// pixel fonts — e.g. Departure Mono ships only `Regular`) must not be asked for
// a weight it lacks: iced's cosmic-text stack responds to a weight miss by
// abandoning the family and falling back to a generic serif/sans rather than
// reusing the in-family face. We therefore down-grade every weighted request to
// the nearest weight the active family actually provides.
// ----------------------------------------------------------------------------

/// Per-family cache of available weights (CSS hundreds), memoized to avoid
/// re-introspecting font files. An empty vector means "unknown family" — treat
/// it as "all weights allowed" and honor the request unchanged.
static FAMILY_WEIGHTS: LazyLock<RwLock<HashMap<String, Vec<u16>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn weight_to_css(weight: Weight) -> u16 {
    match weight {
        Weight::Thin => 100,
        Weight::ExtraLight => 200,
        Weight::Light => 300,
        Weight::Normal => 400,
        Weight::Medium => 500,
        Weight::Semibold => 600,
        Weight::Bold => 700,
        Weight::ExtraBold => 800,
        Weight::Black => 900,
    }
}

fn css_to_weight(css: u16) -> Weight {
    match css {
        100 => Weight::Thin,
        200 => Weight::ExtraLight,
        300 => Weight::Light,
        500 => Weight::Medium,
        600 => Weight::Semibold,
        700 => Weight::Bold,
        800 => Weight::ExtraBold,
        900 => Weight::Black,
        // 400, and any weight off the CSS scale.
        _ => Weight::Normal,
    }
}

/// Pick the available weight numerically closest to `requested`; ties resolve to
/// the lighter weight. Returns `requested` when `available` is empty.
fn nearest_available_weight(available: &[u16], requested: u16) -> u16 {
    available
        .iter()
        .copied()
        .min_by_key(|w| (w.abs_diff(requested), *w))
        .unwrap_or(requested)
}

/// Available weights for `family` (memoized). Empty = unknown/undetectable.
fn available_weights(family: &str) -> Vec<u16> {
    if let Some(weights) = FAMILY_WEIGHTS.read().get(family) {
        return weights.clone();
    }
    let weights = nokkvi_data::services::font_discovery::family_weights(family);
    FAMILY_WEIGHTS
        .write()
        .insert(family.to_string(), weights.clone());
    weights
}

/// Map `requested` to the nearest weight `family` actually ships, so the
/// in-family face is kept instead of falling back to a generic typeface.
/// Returns `requested` unchanged when the family's weights are unknown.
fn effective_weight(family: &str, requested: Weight) -> Weight {
    let weights = available_weights(family);
    if weights.is_empty() {
        return requested;
    }
    let req = weight_to_css(requested);
    if weights.contains(&req) {
        return requested;
    }
    css_to_weight(nearest_available_weight(&weights, req))
}

/// [`ui_font`] at `weight`, down-graded to the nearest weight the active family
/// actually provides. Use this for every weighted UI text run in place of
/// `Font { weight, ..ui_font() }` so single-weight custom fonts render in their
/// own face instead of a generic fallback.
pub(crate) fn weighted_ui_font(weight: Weight) -> Font {
    let family = { FONT_FAMILY.read().clone() };
    if family.is_empty() {
        // No custom font: the default look relies on iced's bundled Fira Sans
        // plus nokkvi's bundled Medium/Bold faces, which cover these weights.
        return Font {
            weight: shifted_weight(weight),
            ..Font::DEFAULT
        };
    }
    Font {
        weight: weighted_weight(&family, weight),
        ..ui_font_family(&family)
    }
}

/// The base weight step applied to `requested`, then down-graded to what
/// `family` ships (an empty family is the bundled default: no down-grade).
fn weighted_weight(family: &str, requested: Weight) -> Weight {
    let shifted = shifted_weight(requested);
    if family.is_empty() {
        shifted
    } else {
        effective_weight(family, shifted)
    }
}

/// The face for a named family at its default weight, no weight applied.
fn ui_font_family(family: &str) -> Font {
    Font::with_family(iced::font::Family::name(family))
}

/// Like [`weighted_ui_font`] but for an explicit `family` — the font picker
/// preview draws each row in its own typeface, so it must down-grade against the
/// previewed family rather than the active one.
pub(crate) fn weighted_font_for_family(family: &str, weight: Weight) -> Font {
    Font {
        weight: effective_weight(family, weight),
        ..Font::with_family(iced::font::Family::name(family))
    }
}

// ----------------------------------------------------------------------------
// Title font (independent of body font — supports italic/serif title
// typefaces like IBM Plex Sans without affecting JetBrains-Mono-style body text)
// ----------------------------------------------------------------------------

// Title-font family was intended to give L1 hero titles + active-breadcrumb
// segments a typographic distinction from the body font, but the
// `set_title_font_family` setter never landed (the L5 settings lane it was
// dead-coded for was descoped). Every `theme::title_font()` call therefore
// returned the body font, shipping the design's distinction disabled. The
// machinery (`TITLE_FONT_FAMILY` static + `TITLE_FONT_CACHE` + the public
// readers) was removed — callers now use `ui_font()` directly. If a future
// design re-introduces the distinction, the simpler shape is a config-side
// `theme.title_font_family` field threaded through `ResolvedTheme` rather
// than a global mutable static.

#[cfg(test)]
mod tests {
    use iced::font::Weight;

    use super::{
        FAMILY_WEIGHTS, css_to_weight, effective_weight, nearest_available_weight, weight_to_css,
    };

    #[test]
    fn weight_css_round_trips() {
        for w in [
            Weight::Thin,
            Weight::ExtraLight,
            Weight::Light,
            Weight::Normal,
            Weight::Medium,
            Weight::Semibold,
            Weight::Bold,
            Weight::ExtraBold,
            Weight::Black,
        ] {
            assert_eq!(css_to_weight(weight_to_css(w)), w);
        }
    }

    #[test]
    fn nearest_picks_closest_with_lighter_tie_break() {
        assert_eq!(nearest_available_weight(&[400], 700), 400);
        assert_eq!(nearest_available_weight(&[400, 700], 600), 700);
        assert_eq!(nearest_available_weight(&[400, 700], 500), 400);
        assert_eq!(nearest_available_weight(&[300, 500], 400), 300); // tie → lighter
        assert_eq!(nearest_available_weight(&[], 700), 700); // unknown → unchanged
    }

    #[test]
    fn single_weight_family_downgrades_all_to_regular() {
        FAMILY_WEIGHTS
            .write()
            .insert("FakeRegularOnly".to_string(), vec![400]);
        assert_eq!(
            effective_weight("FakeRegularOnly", Weight::Bold),
            Weight::Normal
        );
        assert_eq!(
            effective_weight("FakeRegularOnly", Weight::Medium),
            Weight::Normal
        );
        assert_eq!(
            effective_weight("FakeRegularOnly", Weight::Normal),
            Weight::Normal
        );
    }

    #[test]
    fn multi_weight_family_keeps_present_weight() {
        FAMILY_WEIGHTS
            .write()
            .insert("FakeMulti".to_string(), vec![400, 500, 700]);
        assert_eq!(effective_weight("FakeMulti", Weight::Bold), Weight::Bold);
        assert_eq!(
            effective_weight("FakeMulti", Weight::Medium),
            Weight::Medium
        );
        // Semibold(600) absent → nearest of {400,500,700}: |600-500|==|600-700| → lighter 500.
        assert_eq!(
            effective_weight("FakeMulti", Weight::Semibold),
            Weight::Medium
        );
    }

    #[test]
    fn unknown_family_trusts_requested_weight() {
        FAMILY_WEIGHTS
            .write()
            .insert("FakeEmpty".to_string(), Vec::new());
        assert_eq!(effective_weight("FakeEmpty", Weight::Bold), Weight::Bold);
    }
}
