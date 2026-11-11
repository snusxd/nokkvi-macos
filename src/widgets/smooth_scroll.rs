//! `SmoothScroll`: sub-row glide for slot lists under pixel-delta scrolling
//! (trackpads, high-resolution wheels).
//!
//! A slot list scrolls in whole rows: `viewport_offset` is an item index, so
//! every wheel event used to move it by exactly one row. This widget keeps
//! that model and adds a pixel offset on top of it. A pixel delta slides the
//! rendered rows by that many pixels; each time the slide crosses a full row
//! the widget publishes the usual `ScrollUp` / `ScrollDown` message (which
//! moves `viewport_offset` by one) and carries the remainder over, so the
//! picture never jumps. When the gesture (and its momentum) ends, the
//! remainder eases to the row grid.
//!
//! The wrapped content is `[extra row | the slot list | extra row]`: the two
//! extra rows sit just outside the clip, so the row that slides into view is
//! always already drawn. The widget lays the content out at
//! `y = -step + shift`, which keeps hit testing exact, and clips to its own
//! bounds with a real layer (see `overflow_pin.rs` for why a layer, not a
//! narrowed viewport).
//!
//! Line deltas (a notched mouse wheel) are not touched: they fall through to
//! the surrounding `mouse_area`'s one-row-per-notch handler.

use std::time::{Duration, Instant};

use iced::{
    Event, Length, Rectangle, Size, Vector,
    advanced::{
        Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree, tree},
    },
    window,
};

/// How long after the last scroll event the glide waits before it settles
/// onto the row grid. Trackpad momentum delivers events every frame or two, so
/// this bridges the gap between events without settling mid-fling.
pub(crate) const SETTLE_DELAY: Duration = Duration::from_millis(90);

/// Time constant of the settle ease, in seconds. The remainder decays as
/// `exp(-t / SETTLE_TAU)`: about 95 % gone after three of these.
pub(crate) const SETTLE_TAU: f32 = 0.07;

/// A remainder under this many pixels snaps to zero.
const SNAP_PIXELS: f32 = 0.5;

/// Most rows one event may move. A runaway delta (a driver glitch, a huge
/// fling) must not publish hundreds of messages.
const MAX_ROWS_PER_EVENT: i32 = 24;

/// The pure core of a pixel scroll event.
///
/// `shift` is the content's displacement in pixels, positive = down (the list
/// moved toward the start). Adds `dy`, drops a displacement toward an edge the
/// list cannot scroll past, and splits off whole rows. Returns the new shift
/// and the row count to publish: positive = scroll up (toward the start),
/// negative = scroll down.
pub(crate) fn apply_delta(
    shift: f32,
    dy: f32,
    step: f32,
    can_up: bool,
    can_down: bool,
) -> (f32, i32) {
    if step <= 0.0 || !dy.is_finite() {
        return (0.0, 0);
    }
    let shift = clamp_to_edges(shift + dy, can_up, can_down);
    let rows = ((shift / step).trunc() as i32).clamp(-MAX_ROWS_PER_EVENT, MAX_ROWS_PER_EVENT);
    (shift - rows as f32 * step, rows)
}

/// Drop a displacement toward an edge the list cannot scroll past: nothing
/// would follow it there, so showing it would reveal blank space.
pub(crate) fn clamp_to_edges(shift: f32, can_up: bool, can_down: bool) -> f32 {
    if (shift > 0.0 && !can_up) || (shift < 0.0 && !can_down) {
        0.0
    } else {
        shift
    }
}

/// One frame of the settle. Past half a row the list first commits to the next
/// row (one published message, the shift flips sign and shrinks below half a
/// row), then the remainder decays toward zero. Returns the new shift and the
/// row to publish (`+1` up, `-1` down, `0` none).
pub(crate) fn settle_step(
    shift: f32,
    dt: f32,
    step: f32,
    can_up: bool,
    can_down: bool,
) -> (f32, i32) {
    let shift = clamp_to_edges(shift, can_up, can_down);
    let (shift, row) = if step > 0.0 && shift.abs() > step / 2.0 {
        let row = if shift > 0.0 { 1 } else { -1 };
        (shift - row as f32 * step, row)
    } else {
        (shift, 0)
    };
    let eased = shift * (-dt.max(0.0) / SETTLE_TAU).exp();
    let eased = if eased.abs() < SNAP_PIXELS {
        0.0
    } else {
        eased
    };
    (eased, row)
}

/// Per-widget glide state, kept in the widget tree across rebuilds.
#[derive(Debug, Default)]
struct Glide {
    /// Content displacement in pixels, positive = down.
    shift: f32,
    /// When the last scroll event arrived.
    last_input: Option<Instant>,
    /// When the settle last advanced a frame.
    last_frame: Option<Instant>,
}

/// Wraps `[extra row | slot list | extra row]` with the glide. See the module docs.
pub(crate) struct SmoothScroll<W, Message> {
    content: W,
    /// One row plus its spacing: the distance the content moves per row.
    step: f32,
    can_up: bool,
    can_down: bool,
    on_up: Message,
    on_down: Message,
}

impl<W, Message> SmoothScroll<W, Message> {
    /// `content` is the three-part column; `step` its row pitch. `can_up` /
    /// `can_down` say whether the list can scroll further in that direction.
    pub(crate) fn new(
        content: W,
        step: f32,
        can_up: bool,
        can_down: bool,
        on_up: Message,
        on_down: Message,
    ) -> Self {
        Self {
            content,
            step,
            can_up,
            can_down,
            on_up,
            on_down,
        }
    }
}

impl<W, Message> iced::advanced::widget::Meta for SmoothScroll<W, Message> {}

/// The content's layout and state: the widget always holds exactly one child.
/// The cursor as the content may see it. The two extra rows sit outside our
/// bounds (over the header and the player bar, where they are clipped away), so
/// a cursor out there must not hover or click them.
fn inside(cursor: mouse::Cursor, bounds: Rectangle) -> mouse::Cursor {
    if cursor.is_over(bounds) {
        cursor
    } else {
        mouse::Cursor::Unavailable
    }
}

fn child(layout: Layout, tree: &Tree) -> (Layout, &Tree) {
    layout
        .iter(&tree.children)
        .next()
        .expect("SmoothScroll always lays out exactly one child")
}

/// [`child`], mutably.
fn child_mut(layout: Layout, tree: &mut Tree) -> (Layout, &mut Tree) {
    layout
        .iter_mut(&mut tree.children)
        .next()
        .expect("SmoothScroll always lays out exactly one child")
}

impl<W, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for SmoothScroll<W, Message>
where
    W: Widget<Message, Theme, Renderer>,
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Glide>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Glide::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let max = limits.max;
        // The content gets two extra rows of height: one above, one below.
        let content_limits = layout::Limits::new(
            Size::ZERO,
            Size::new(max.width, max.height + 2.0 * self.step),
        );
        self.content
            .layout(&mut tree.children[0], renderer, &content_limits);

        let shift = tree.state.downcast_ref::<Glide>().shift;
        let shift = clamp_to_edges(shift, self.can_up, self.can_down);
        tree.children[0].translation = Vector::new(0.0, shift - self.step);

        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let clipped = layout.bounds().intersection(viewport).unwrap_or_default();
        let (layout, tree) = child_mut(layout, tree);
        self.content
            .operate(tree, layout, &clipped, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        {
            let clipped = bounds.intersection(viewport).unwrap_or_default();
            let (child_layout, child_tree) = child_mut(layout, tree);
            self.content.update(
                child_tree,
                event,
                child_layout,
                inside(cursor, bounds),
                renderer,
                shell,
                &clipped,
            );
        }
        if shell.is_event_captured() {
            return;
        }

        let glide = tree.state.downcast_mut::<Glide>();

        match event {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { y, .. },
            }) if cursor.is_over(bounds) => {
                let (shift, rows) =
                    apply_delta(glide.shift, *y, self.step, self.can_up, self.can_down);
                glide.shift = shift;
                glide.last_input = Some(Instant::now());
                glide.last_frame = None;
                self.publish_rows(rows, shell);
                shell.capture_event();
                shell.invalidate_layout();
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(now)) if glide.shift != 0.0 => {
                let idle = glide
                    .last_input
                    .map_or(SETTLE_DELAY, |at| now.saturating_duration_since(at));
                if idle >= SETTLE_DELAY {
                    let dt = glide.last_frame.map_or(1.0 / 60.0, |at| {
                        now.saturating_duration_since(at).as_secs_f32().min(0.05)
                    });
                    let (shift, row) =
                        settle_step(glide.shift, dt, self.step, self.can_up, self.can_down);
                    glide.shift = shift;
                    glide.last_frame = Some(*now);
                    self.publish_rows(row, shell);
                    shell.invalidate_layout();
                }
                if glide.shift != 0.0 {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        let clipped = bounds.intersection(viewport).unwrap_or_default();
        let (layout, tree) = child(layout, tree);
        self.content
            .mouse_interaction(tree, layout, inside(cursor, bounds), &clipped, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let Some(clipped) = layout.bounds().intersection(viewport) else {
            return;
        };
        let cursor = inside(cursor, layout.bounds());
        let (layout, tree) = child(layout, tree);
        // A real clip layer: the extra rows must not paint outside the list.
        renderer.with_layer(clipped, |renderer| {
            self.content
                .draw(tree, renderer, theme, style, layout, cursor, &clipped);
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (layout, tree) = child_mut(layout, tree);
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}

impl<W, Message: Clone> SmoothScroll<W, Message> {
    /// Publish `rows` scroll messages: positive = up, negative = down.
    fn publish_rows(&self, rows: i32, shell: &mut Shell<'_, Message>) {
        let message = if rows > 0 { &self.on_up } else { &self.on_down };
        for _ in 0..rows.unsigned_abs() {
            shell.publish(message.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 60.0;

    #[test]
    fn a_small_delta_slides_without_publishing() {
        let (shift, rows) = apply_delta(0.0, 25.0, STEP, true, true);
        assert_eq!((shift, rows), (25.0, 0));
    }

    #[test]
    fn crossing_a_row_publishes_and_carries_the_remainder() {
        let (shift, rows) = apply_delta(50.0, 25.0, STEP, true, true);
        assert_eq!(rows, 1);
        assert!((shift - 15.0).abs() < 1e-4, "{shift}");
        let (shift, rows) = apply_delta(-50.0, -25.0, STEP, true, true);
        assert_eq!(rows, -1);
        assert!((shift + 15.0).abs() < 1e-4, "{shift}");
    }

    #[test]
    fn a_long_fling_publishes_several_rows_at_once() {
        let (shift, rows) = apply_delta(0.0, 150.0, STEP, true, true);
        assert_eq!(rows, 2);
        assert!((shift - 30.0).abs() < 1e-4, "{shift}");
    }

    #[test]
    fn an_edge_the_list_cannot_pass_drops_the_displacement() {
        assert_eq!(apply_delta(0.0, 40.0, STEP, false, true), (0.0, 0));
        assert_eq!(apply_delta(0.0, -40.0, STEP, true, false), (0.0, 0));
        // The open direction still moves.
        assert_eq!(apply_delta(0.0, -40.0, STEP, false, true), (-40.0, 0));
    }

    #[test]
    fn a_runaway_delta_is_capped() {
        let (_, rows) = apply_delta(0.0, 1.0e9, STEP, true, true);
        assert_eq!(rows, MAX_ROWS_PER_EVENT);
        assert_eq!(apply_delta(0.0, f32::NAN, STEP, true, true), (0.0, 0));
        assert_eq!(apply_delta(0.0, 10.0, 0.0, true, true), (0.0, 0));
    }

    #[test]
    fn settling_under_half_a_row_eases_toward_the_grid_without_a_row() {
        let (shift, row) = settle_step(20.0, 1.0 / 60.0, STEP, true, true);
        assert_eq!(row, 0);
        assert!(shift > 0.0 && shift < 20.0, "{shift}");
    }

    #[test]
    fn settling_past_half_a_row_commits_to_the_next_row() {
        // 45 px down is closer to the next row up than to the current one.
        let (shift, row) = settle_step(45.0, 0.0, STEP, true, true);
        assert_eq!(row, 1);
        assert!((shift + 15.0).abs() < 1e-4, "{shift}");
        let (shift, row) = settle_step(-45.0, 0.0, STEP, true, true);
        assert_eq!(row, -1);
        assert!((shift - 15.0).abs() < 1e-4, "{shift}");
    }

    #[test]
    fn settling_never_commits_toward_an_edge_it_cannot_pass() {
        assert_eq!(settle_step(45.0, 0.0, STEP, false, true), (0.0, 0));
    }

    #[test]
    fn the_settle_reaches_the_grid_and_stops() {
        let mut shift = 55.0;
        let mut published = 0;
        for _ in 0..120 {
            let (next, row) = settle_step(shift, 1.0 / 60.0, STEP, true, true);
            shift = next;
            published += row;
        }
        assert_eq!(shift, 0.0);
        assert_eq!(published, 1, "55 px of a 60 px row rounds up by one row");
    }

    #[test]
    fn the_picture_is_continuous_across_a_published_row() {
        // The visible offset of the content from the grid is `shift` before and
        // `shift - rows * step` after: publishing a row moves the picture by one
        // row, so the carried shift must cancel it exactly.
        let before = 50.0_f32;
        let (after, rows) = apply_delta(before, 25.0, STEP, true, true);
        let total_motion = (before + 25.0) - (after + rows as f32 * STEP);
        assert!(total_motion.abs() < 1e-4);
    }
}
