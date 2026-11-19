//! Theater Mode — the now-playing layout: the slot list, the toolbar and the
//! nav leave the window, which shows only the playing track's cover with its
//! over-cover visualizer and lyrics.
//!
//! `Nokkvi::enter_theater` is the single entry point: every way in (F11, the
//! Queue cover's corner icon and menu row, the `theater` IPC verb) calls it. Entering and leaving are both unmount edges; see
//! the two methods for what each one clears.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use iced::{Task, widget::image::Handle};
use nokkvi_data::types::{hotkey_config::HotkeyAction, player_settings::TheaterControls};

use crate::{
    Nokkvi, Screen,
    app_message::{ArtworkMessage, Message, TheaterMessage},
    state::{ChromeMotion, OldLyrics, Reveal, RevealKind, RollDirection, TheaterState, TrackRoll},
};

/// Id the Now Playing sidebar's cover is tagged with (`BoundsTag`): entering
/// Theater Mode reads where it sits and moves it into the theater layout.
pub(crate) const SIDEBAR_COVER_TAG: &str = "now-playing-sidebar-cover";

/// How long the panel takes to grow to the window or shrink back.
pub(crate) const REVEAL_DURATION: Duration = Duration::from_millis(420);

/// How long a track change takes to roll the cover and the lyrics over.
pub(crate) const ROLL_DURATION: Duration = Duration::from_millis(520);

/// Smooth start and stop: a cubic ease-in-out over `0..=1`.
pub(crate) fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// How far the panel is from its resting place (the cover, or below the
/// window) toward filling the window: `0.0` at rest, `1.0` full. `None` while
/// an entry still waits for the cover's bounds.
pub(crate) fn reveal_fill(reveal: &Reveal, now: Instant) -> Option<f32> {
    let since = reveal.since?;
    let t = (now.saturating_duration_since(since).as_secs_f32() / REVEAL_DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
    let eased = ease_in_out_cubic(t);
    Some(match reveal.kind {
        RevealKind::Enter => eased,
        RevealKind::Leave => 1.0 - eased,
    })
}

/// `from` moved `t` of the way to `to` (`0.0` = `from`, `1.0` = `to`).
pub(crate) fn lerp_rect(from: iced::Rectangle, to: iced::Rectangle, t: f32) -> iced::Rectangle {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    iced::Rectangle {
        x: lerp(from.x, to.x),
        y: lerp(from.y, to.y),
        width: lerp(from.width, to.width),
        height: lerp(from.height, to.height),
    }
}

/// The whole theater layout's rectangle at `fill` while it rises from just
/// below the window at full size (no sidebar cover to leave from): `fill`
/// `0.0` is below the window, `1.0` is the window.
pub(crate) fn rising_rect(window: iced::Size, fill: f32) -> iced::Rectangle {
    lerp_rect(
        iced::Rectangle::new(iced::Point::new(0.0, window.height), window),
        iced::Rectangle::new(iced::Point::ORIGIN, window),
        fill,
    )
}

/// Where Theater Mode puts its two parts in a `window`: the square cover on
/// the left and, when there are lyrics to show, their column to its right.
/// Without lyrics the cover is centered alone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TheaterGeometry {
    pub cover: iced::Rectangle,
    pub lyrics: Option<iced::Rectangle>,
}

/// Narrowest lyrics column worth drawing; a window too narrow for it shows the
/// cover alone.
const MIN_LYRICS_WIDTH: f32 = 220.0;

/// [`TheaterGeometry`] for `window`. The margin is a twelfth of the shorter
/// side (at least 24 px); the cover takes the largest square that leaves the
/// lyrics column about as wide as the cover.
pub(crate) fn theater_geometry(window: iced::Size, with_lyrics: bool) -> TheaterGeometry {
    let margin = (window.width.min(window.height) / 12.0).max(24.0);
    let avail_h = (window.height - 2.0 * margin).max(1.0);
    if with_lyrics {
        let side = avail_h.min((window.width - 3.0 * margin) / 2.0);
        let lyrics_x = 2.0 * margin + side;
        let lyrics_w = window.width - lyrics_x - margin;
        if side > 0.0 && lyrics_w >= MIN_LYRICS_WIDTH {
            return TheaterGeometry {
                cover: iced::Rectangle::new(
                    iced::Point::new(margin, (window.height - side) / 2.0),
                    iced::Size::new(side, side),
                ),
                // The column runs the window's full height; the lyrics scroll
                // under it edge to edge.
                lyrics: Some(iced::Rectangle::new(
                    iced::Point::new(lyrics_x, 0.0),
                    iced::Size::new(lyrics_w, window.height),
                )),
            };
        }
    }
    let side = avail_h.min((window.width - 2.0 * margin).max(1.0));
    TheaterGeometry {
        cover: iced::Rectangle::new(
            iced::Point::new((window.width - side) / 2.0, (window.height - side) / 2.0),
            iced::Size::new(side, side),
        ),
        lyrics: None,
    }
}

/// Idle time before the auto-hiding bar (and the cursor) leave; the same
/// hold as the auto-hide toolbar's hotkey reveal.
pub(crate) const HIDE_DELAY: Duration = Duration::from_millis(2500);
/// How long the bar takes to slide fully in or out.
pub(crate) const SLIDE_DURATION: Duration = Duration::from_millis(220);

/// How long after an exit's restore request a contradicting mode report is
/// treated as the compositor not having caught up yet.
pub(crate) const RESTORE_SETTLE: Duration = Duration::from_secs(1);

/// The prior mode to record on entry. `reported` is the window's answer;
/// when an exit asked for a different mode within [`RESTORE_SETTLE`], the
/// answer is stale (Wayland reports the compositor's last reply) and the
/// requested mode is the truth.
pub(crate) fn effective_prior_mode(
    reported: iced::window::Mode,
    restore_sent: Option<(iced::window::Mode, Instant)>,
    now: Instant,
) -> iced::window::Mode {
    match restore_sent {
        Some((requested, at))
            if requested != reported && now.saturating_duration_since(at) < RESTORE_SETTLE =>
        {
            requested
        }
        _ => reported,
    }
}

/// Where the chrome should be right now. `AutoHide` shows it while the
/// window has focus and the cursor is on the bar, an overlay menu is open, or
/// there was activity within [`HIDE_DELAY`].
pub(crate) fn chrome_target(
    state: &TheaterState,
    now: Instant,
    menu_open: bool,
    setting: TheaterControls,
) -> bool {
    match setting {
        TheaterControls::AlwaysShown => true,
        TheaterControls::AlwaysHidden => false,
        TheaterControls::AutoHide => {
            state.window_focused
                && (state.bar_hovered
                    || state.corner_hovered
                    || menu_open
                    || activity_recent(state, now))
        }
    }
}

/// Activity within [`HIDE_DELAY`] of `now`.
fn activity_recent(state: &TheaterState, now: Instant) -> bool {
    state
        .last_activity
        .is_some_and(|at| now.saturating_duration_since(at) < HIDE_DELAY)
}

/// The chrome's slide offset at `now`: 0 = fully shown, 1 = fully hidden,
/// easing from the flip's recorded `from`.
pub(crate) fn slide_offset(chrome: ChromeMotion, now: Instant) -> f32 {
    let (since, from, target) = match chrome {
        ChromeMotion::Shown { since, from } => (since, from, 0.0),
        ChromeMotion::Hidden { since, from } => (since, from, 1.0),
    };
    let t = now.saturating_duration_since(since).as_secs_f32() / SLIDE_DURATION.as_secs_f32();
    let eased = crate::widgets::lyrics_viewport::ease_out_expo(t.clamp(0.0, 1.0));
    (from + (target - from) * eased).clamp(0.0, 1.0)
}

/// Whether the cursor hides over the theater panel. Its own rule,
/// independent of focus and of the controls setting: with the bar always
/// hidden the user must still be able to aim a right-click, and a pointer
/// moved over an unfocused window must show.
pub(crate) fn cursor_hidden(state: &TheaterState, now: Instant, menu_open: bool) -> bool {
    state.active && !menu_open && !activity_recent(state, now)
}

/// Per-frame step (from the frame tick): flip the chrome when its target
/// changed, recording the offset it leaves from. The only place the chrome
/// flips.
pub(crate) fn tick(app: &mut Nokkvi, now: Instant) {
    if app
        .theater
        .roll
        .as_ref()
        .is_some_and(|roll| now.saturating_duration_since(roll.since) >= ROLL_DURATION)
    {
        app.theater.roll = None;
    }
    // The enter / leave animation ends on its own clock, also after a leave
    // (theater is no longer active then).
    if let Some(reveal) = app.theater.reveal
        && reveal
            .since
            .is_some_and(|since| now.saturating_duration_since(since) >= REVEAL_DURATION)
    {
        app.theater.reveal = None;
    }
    if !app.theater.active {
        return;
    }
    let target = chrome_target(
        &app.theater,
        now,
        app.open_menu.is_some(),
        app.theater_controls(),
    );
    let current = app.theater.chrome;
    if target != current.is_shown() {
        let from = slide_offset(current, now);
        app.theater.chrome = if target {
            ChromeMotion::Shown { since: now, from }
        } else {
            ChromeMotion::Hidden { since: now, from }
        };
    }
}

/// What a hotkey does while Theater Mode is active. See
/// [`theater_key_policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TheaterKeyPolicy {
    /// Acts as normal and stays in theater.
    Passthrough,
    /// Leaves theater, then runs the key against the restored layout.
    ExitThenPerform,
    /// Leaves theater and swallows the key.
    ExitOnly,
}

/// Classify a hotkey action for Theater Mode. Exhaustive on purpose: a new
/// action fails to compile until someone decides what it does here.
///
/// The rule: a key that drives playback or the now-playing surfaces stays;
/// a key that navigates, opens something the user will see, or acts on the
/// PLAYING track leaves and then runs; a key that would act on a row of the
/// hidden list, or edit a hidden Settings row, only leaves. Never mutate
/// blind.
pub(crate) fn theater_key_policy(action: HotkeyAction) -> TheaterKeyPolicy {
    use HotkeyAction as A;
    use TheaterKeyPolicy as P;
    match action {
        A::TogglePlay
        | A::ToggleRandom
        | A::ToggleRepeat
        | A::ToggleConsume
        | A::ToggleAutoplay
        | A::ToggleSoundEffects
        | A::CycleVisualization
        | A::NextVisualizerPreset
        | A::PreviousVisualizerPreset
        | A::ToggleVisualizerPresetLock
        | A::ChooseVisualizerPreset
        | A::ToggleEqModal
        | A::ToggleCrossfade
        | A::ToggleLyrics
        | A::ToggleBitPerfect
        | A::SeekBackward
        | A::SeekForward
        | A::ToggleTheater
        | A::Escape
        | A::MediaPlayPause
        | A::MediaPause
        | A::MediaStop
        | A::MediaNextTrack
        | A::MediaPreviousTrack => P::Passthrough,
        A::SwitchToQueue
        | A::SwitchToAlbums
        | A::SwitchToArtists
        | A::SwitchToSongs
        | A::SwitchToGenres
        | A::SwitchToPlaylists
        | A::SwitchToRadios
        | A::SwitchToHarbour
        | A::GoHome
        | A::SwitchToSettings
        | A::FocusSearch
        | A::ToggleBrowsingPanel
        | A::ToggleNowPlayingSidebar
        | A::CenterOnPlaying
        | A::OpenTrawl
        | A::Roulette
        | A::NewSmartPlaylist
        | A::RefreshView
        | A::SaveQueueAsPlaylist
        | A::FindSimilar
        | A::FindTopSongs
        | A::FocusNavigation => P::ExitThenPerform,
        A::SlotListUp
        | A::SlotListDown
        | A::Activate
        | A::ExpandCenter
        | A::ShufflePlay
        | A::AddToQueue
        | A::RemoveFromQueue
        | A::ClearQueue
        | A::ToggleStar
        | A::IncreaseRating
        | A::DecreaseRating
        | A::GetInfo
        | A::SimilarArtists
        | A::MoveTrackUp
        | A::MoveTrackDown
        | A::TrawlSaveAsPlaylist
        | A::EditCenteredPlaylist
        | A::PrevSortMode
        | A::NextSortMode
        | A::ToggleSortOrder
        | A::EditUp
        | A::EditDown
        | A::ResetToDefault
        | A::SettingsCategoryNext
        | A::SettingsCategoryPrev => P::ExitOnly,
    }
}

/// The theater panel's cover: the frosted lyrics backdrop when the caller
/// passes one, else the large art, else the mini, all keyed by `key` (the
/// playing album id, or the playing station id for radio). Pure, so the
/// lookup order is testable without rendering. Callers pass `blurred` only
/// while the lyrics layer shows: the frost is not transport-gated, and a
/// stopped panel would otherwise show a scrim with nothing on it.
pub(crate) fn theater_cover<'a>(
    large: &'a HashMap<String, Handle>,
    mini: &'a HashMap<String, Handle>,
    blurred: Option<&'a Handle>,
    key: Option<&str>,
) -> Option<&'a Handle> {
    blurred.or_else(|| key.and_then(|k| large.get(k).or_else(|| mini.get(k))))
}

/// Clear what a slot-list page's `on_exit` / blur would have cleared, for a
/// page whose widgets are about to unmount under Theater Mode.
fn clear_unmounting_list_state(common: &mut crate::widgets::SlotListPageState) {
    common.reset_reveal_locks();
    // The box unmounts whether or not auto-hide is on, so this cannot go
    // through `clear_all_search_input_focus` (which returns early when
    // auto-hide is off).
    common.search_input_focused = false;
    // A row's hover clears only on `on_exit`, which cannot fire once the row
    // unmounts; a stale one would let a click-drag on the cover arm a
    // cross-pane drag of a hidden row, or aim a drop from the browser at a
    // hidden editor row.
    common.slot_list.hovered_slot = None;
}

impl Nokkvi {
    pub(crate) fn handle_theater(&mut self, msg: TheaterMessage) -> Task<Message> {
        match msg {
            TheaterMessage::Toggle => self.toggle_theater(),
            TheaterMessage::Exit => self.exit_theater_animated(),
            TheaterMessage::Activity => {
                self.stamp_theater_activity();
                Task::none()
            }
            TheaterMessage::PriorModeKnown(id, prior) => self.handle_prior_window_mode(id, prior),
            TheaterMessage::CoverBounds(rect) => {
                // Only the entry that asked: a fast exit may already have
                // dropped the pending reveal.
                if let Some(reveal) = self.theater.reveal.as_mut()
                    && reveal.kind == RevealKind::Enter
                    && reveal.since.is_none()
                {
                    reveal.since = Some(Instant::now());
                    reveal.rect = rect;
                    self.theater.cover_rect = rect;
                }
                Task::none()
            }
            TheaterMessage::CornerHover(hovered) => {
                if self.theater.active {
                    self.theater.corner_hovered = hovered;
                }
                Task::none()
            }
            TheaterMessage::BarHover(hovered) => {
                // Only while active: a stray exit from an unmounting bar must
                // not re-arm a flag the exit edge just cleared.
                if self.theater.active {
                    self.theater.bar_hovered = hovered;
                }
                Task::none()
            }
        }
    }

    pub(crate) fn toggle_theater(&mut self) -> Task<Message> {
        if self.theater.active {
            self.exit_theater_animated()
        } else {
            self.enter_theater()
        }
    }

    /// The single entry point. No-op off the Home screen or when already
    /// active. Entering unmounts the slot list, its header and the split view,
    /// so every flag their `on_exit` / `on_close` would have cleared is
    /// cleared here; the search query, the split view, the editor and the
    /// current view are left alone for the way back.
    pub(crate) fn enter_theater(&mut self) -> Task<Message> {
        if self.screen != Screen::Home || self.theater.active {
            return Task::none();
        }
        self.theater.active = true;
        // The panel grows from the Queue's cover once its bounds are read.
        self.theater.reveal = Some(Reveal {
            kind: RevealKind::Enter,
            since: None,
            rect: None,
        });
        self.theater.cover_rect = None;
        let now = Instant::now();
        self.theater.last_activity = Some(now);
        self.theater.bar_hovered = false;
        self.theater.corner_hovered = false;
        self.theater.chrome = ChromeMotion::Shown {
            since: now,
            from: 0.0,
        };

        self.open_menu = None;
        // The nav bar it would highlight is hidden.
        self.nav_focus = None;
        self.clear_stranded_within_list_drag();
        self.cancel_roulette_restoring_offset();
        self.cancel_cross_pane_drag();
        // A find-and-expand chain would land its expansion on a hidden row.
        self.cancel_pending_expand();
        for common in self.all_slot_list_commons_mut() {
            clear_unmounting_list_state(common);
        }
        if let Some(editor) = self.playlist_editor.as_mut() {
            clear_unmounting_list_state(&mut editor.common);
        }
        // The queue toolbar's playlist identity unmounts with the queue.
        self.queue_page.collapse_playlist_strip();

        // Theater Fills the Screen: ask the window for its current mode; the
        // answer (`PriorModeKnown`) records it and goes fullscreen.
        let window_mode = match self.main_window_id {
            Some(id) if self.settings.theater_window_fullscreen => iced::window::mode(id)
                .map(move |prior| Message::Theater(TheaterMessage::PriorModeKnown(id, prior))),
            _ => Task::none(),
        };

        // Theater may be entered from a view that is not a lyrics surface, so
        // the playing track's cover and lyrics may never have been fetched.
        let cover_probe = crate::widgets::bounds_tag::probe_bounds(&[SIDEBAR_COVER_TAG])
            .map(|rect| Message::Theater(TheaterMessage::CoverBounds(rect)));
        Task::batch([
            window_mode,
            cover_probe,
            self.theater_large_art_task().unwrap_or_else(Task::none),
            self.lyrics_kick_if_unresolved(),
        ])
    }

    /// Leave Theater Mode. Also an unmount edge: the theater panel's menu
    /// closes with it. Never touches the current view, the browsing panel,
    /// the editor or any search query.
    pub(crate) fn exit_theater(&mut self) -> Task<Message> {
        if !self.leave_theater_state() {
            return Task::none();
        }
        // Restore the window mode Theater Fills the Screen replaced. Taken even
        // without a window, so a stale prior never outlives this exit.
        match (self.theater.prior_window_mode.take(), self.main_window_id) {
            (Some(prior), Some(id)) => {
                self.theater.restore_sent = Some((prior, Instant::now()));
                iced::window::set_mode(id, prior)
            }
            _ => Task::none(),
        }
    }

    /// Leave theater with the panel shrinking back to the cover it grew from
    /// (or sinking below the window). Only for exits that land on the view
    /// theater was entered from: Escape, the toggle key, the exit icon and the
    /// panel's menu row. An exit that goes on to change the view (a key that
    /// leaves and then acts) stays instant, so no panel animates over a
    /// different layout.
    pub(crate) fn exit_theater_animated(&mut self) -> Task<Message> {
        let was_active = self.theater.active;
        let rect = self.theater.cover_rect;
        let now = Instant::now();
        // Leaving while the panel is still growing turns it around from where
        // it is: the cubic is symmetric, so a leave that has run for
        // `D * (1 - t)` sits where an enter that has run for `D * t` stood.
        let since = match self.theater.reveal {
            Some(Reveal {
                kind: RevealKind::Enter,
                since: Some(started),
                ..
            }) => {
                let t = (now.saturating_duration_since(started).as_secs_f32()
                    / REVEAL_DURATION.as_secs_f32())
                .clamp(0.0, 1.0);
                now.checked_sub(REVEAL_DURATION.mul_f32(1.0 - t))
                    .unwrap_or(now)
            }
            _ => now,
        };
        let task = self.exit_theater();
        if was_active {
            self.theater.reveal = Some(Reveal {
                kind: RevealKind::Leave,
                since: Some(since),
                rect,
            });
        }
        task
    }

    /// Keep the bar and the cursor on screen for another [`HIDE_DELAY`].
    pub(crate) fn stamp_theater_activity(&mut self) {
        if self.theater.active {
            self.theater.last_activity = Some(Instant::now());
        }
    }

    /// The Theater Controls setting.
    pub(crate) fn theater_controls(&self) -> TheaterControls {
        self.settings.theater_controls
    }

    /// Theater Fills the Screen, second half: the window's mode is known.
    /// Store it for the exit and go fullscreen, but only while theater is
    /// still active on that same window: a fast double toggle lands this
    /// after the exit, and a stale window may already be gone. Applying
    /// fullscreen here rather than chaining it after the query is what keeps
    /// a double toggle from leaving the window stuck fullscreen.
    fn handle_prior_window_mode(
        &mut self,
        id: iced::window::Id,
        prior: iced::window::Mode,
    ) -> Task<Message> {
        if !self.theater.active
            || self.main_window_id != Some(id)
            || self.theater.prior_window_mode.is_some()
        {
            return Task::none();
        }
        self.theater.prior_window_mode = Some(effective_prior_mode(
            prior,
            self.theater.restore_sent,
            Instant::now(),
        ));
        iced::window::set_mode(id, iced::window::Mode::Fullscreen)
    }

    /// Leave theater for a window that is about to close (to the tray or on
    /// quit): the recorded window mode is dropped rather than restored, so
    /// no mode change is sent to a closing window and the next window opens
    /// normally.
    pub(crate) fn exit_theater_for_closing_window(&mut self) {
        self.theater.prior_window_mode = None;
        self.theater.reveal = None;
        self.leave_theater_state();
    }

    /// The state half of leaving theater, shared by both exits. Returns
    /// whether theater was active.
    fn leave_theater_state(&mut self) -> bool {
        if !self.theater.active {
            return false;
        }
        self.theater.active = false;
        self.theater.reveal = None;
        self.theater.roll = None;
        // The bar unmounts, so its `on_exit` may never fire.
        self.theater.bar_hovered = false;
        self.theater.corner_hovered = false;
        self.open_menu = None;
        true
    }

    /// [`theater_key_policy`] refined by the hidden state: a toggle whose
    /// target is already on behind theater (Settings' key over a hidden
    /// Settings view, the split-view key with the split view open) only
    /// leaves, which reveals that target instead of toggling it away unseen.
    pub(crate) fn theater_key_policy_here(&self, action: HotkeyAction) -> TheaterKeyPolicy {
        let already_on = match action {
            HotkeyAction::SwitchToSettings => self.current_view == crate::View::Settings,
            HotkeyAction::ToggleBrowsingPanel => self.browsing_panel.is_some(),
            _ => false,
        };
        match theater_key_policy(action) {
            TheaterKeyPolicy::ExitThenPerform if already_on => TheaterKeyPolicy::ExitOnly,
            policy => policy,
        }
    }

    /// A held theater toggle key: winit repeats it, and each repeat would
    /// swap the whole layout. Other held keys (seek) keep repeating.
    pub(crate) fn is_repeated_theater_toggle(
        &self,
        key: &iced::keyboard::Key,
        unnamed: Option<u32>,
        modifiers: iced::keyboard::Modifiers,
        repeat: bool,
    ) -> bool {
        repeat
            && crate::hotkeys::resolve_action(key, unnamed, modifiers, &self.hotkey_config)
                == Some(HotkeyAction::ToggleTheater)
    }

    /// Close the overlay menus anchored to the bar. The bar slides away while
    /// the window is unfocused and would leave them hanging at the edge; the
    /// panel's own menu stays because the panel does not move.
    pub(crate) fn close_theater_bar_menus(&mut self) {
        if self.theater.active
            && self
                .open_menu
                .as_ref()
                .is_some_and(|menu| menu.anchor() == crate::app_message::MenuAnchor::Chrome)
        {
            self.open_menu = None;
        }
    }

    /// What the hidden layout would show: the view, the split view's tab and
    /// whether an editor session is open. The root `update` leaves theater
    /// when a message changes it.
    pub(crate) fn theater_route_snapshot(
        &self,
    ) -> (crate::View, Option<crate::views::BrowsingView>, bool) {
        (
            self.current_view,
            self.browsing_panel.as_ref().map(|p| p.active_view),
            self.playlist_editor.is_some(),
        )
    }

    /// Load the playing album's large cover when the large LRU is cold. The
    /// Queue view warms it through its centered-row prefetch, but theater can
    /// be entered from any view and a song can change while it is active;
    /// without this the panel would upscale the 80 px thumbnail.
    pub(crate) fn theater_large_art_task(&self) -> Option<Task<Message>> {
        let album_id = self.current_queue_song_cover_key()?;
        if self.artwork.large_artwork.snapshot.contains_key(album_id) {
            return None;
        }
        Some(Task::done(Message::Artwork(ArtworkMessage::LoadLarge(
            album_id.to_string(),
        ))))
    }

    /// Whether Theater Mode's panel draws the black backdrop instead of the
    /// cover (the Cover Art setting).
    pub(crate) fn theater_cover_hidden() -> bool {
        crate::theme::artwork_cover().hides_cover(true)
    }

    /// The now-playing cover theater shows: the playing station's art for
    /// radio, else the playing album's (frosted only while the lyrics layer
    /// shows). `None` means the placeholder.
    pub(crate) fn theater_now_playing_cover(&self) -> Option<&Handle> {
        if Self::theater_cover_hidden() {
            return None;
        }
        if let Some(station) = self.active_playback.radio_station() {
            return theater_cover(
                &self.artwork.radio_large_art.snapshot,
                &self.artwork.radio_art.snapshot,
                None,
                Some(&station.id),
            );
        }
        // The lyrics have their own column beside the cover, so the cover is
        // never the frosted backdrop.
        theater_cover(
            &self.artwork.large_artwork.snapshot,
            &self.artwork.album_art.snapshot,
            None,
            self.current_queue_song_cover_key(),
        )
    }
}

impl Nokkvi {
    /// A queue track change while Theater Mode shows: start rolling the cover
    /// and the lyrics over. Called with `scrobble.current_song_id` still naming
    /// the OLD song, so the cover resolved now is the one rolling out. The
    /// direction follows the queue order: a landing earlier in the queue than
    /// the song left is a Previous, anything else a Next. No roll while the
    /// panel is still entering or leaving (its own animation runs).
    pub(crate) fn begin_theater_roll(&mut self, new_id: Option<&str>) {
        if !self.theater.active || self.theater.reveal.is_some() {
            return;
        }
        let Some(old_id) = self.scrobble.current_song_id.as_deref() else {
            return;
        };
        let position = |id: &str| self.library.queue_songs.iter().position(|s| s.id == id);
        let direction = match (position(old_id), new_id.and_then(position)) {
            (Some(old), Some(new)) if new < old => RollDirection::Previous,
            _ => RollDirection::Next,
        };
        self.theater.roll = Some(TrackRoll {
            since: Instant::now(),
            direction,
            old_cover: self.theater_now_playing_cover().cloned(),
            // The sheet on screen now fades out as the next one fades in; read
            // before the lyrics handler clears or promotes it.
            old_lyrics: self.queue_lyrics_panel_data().map(|lyrics| OldLyrics {
                lines: lyrics.lines.to_vec(),
                synced: lyrics.synced,
                center: crate::widgets::lyrics_viewport::lyrics_center_pos(),
            }),
        });
    }

    /// The roll's `(direction, eased progress 0..=1)` at `now`, if one runs.
    pub(crate) fn theater_roll_progress(&self, now: Instant) -> Option<(RollDirection, f32)> {
        let roll = self.theater.roll.as_ref()?;
        let t = (now.saturating_duration_since(roll.since).as_secs_f32()
            / ROLL_DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
        Some((roll.direction, ease_in_out_cubic(t)))
    }
}

/// How far (in px) the outgoing piece sits from rest, and the incoming piece
/// from where it comes in, when the roll is `progress` along: along the
/// vertical axis with a travel of `distance`. `Next` carries things up (the
/// old piece off the top, the new one rising from below, as a list scrolls
/// forward), `Previous` down.
pub(crate) fn roll_offsets(direction: RollDirection, progress: f32, distance: f32) -> (f32, f32) {
    let outgoing = -progress * distance;
    let incoming = (1.0 - progress) * distance;
    match direction {
        RollDirection::Next => (outgoing, incoming),
        RollDirection::Previous => (-outgoing, -incoming),
    }
}
