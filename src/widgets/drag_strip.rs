//! Press-and-drag reorder strip for a short, fully visible run of children
//! (the nav tabs): a `Row` or `Column` whose children move by dragging.
//!
//! Unlike [`super::drag_column::DragColumn`] (a virtualized slot window, so its
//! ghost is rendered app-side from data), every child here is on screen, so the
//! strip animates the drag itself, browser-tab style: the grabbed child follows
//! the cursor along the strip's axis and the others step aside to show where it
//! will land. On release it publishes `on_reorder(from, to)`, where `to` is the
//! child's index in the new order; a drop back where it started publishes
//! nothing.
//!
//! A press below [`DRAG_THRESHOLD`] stays a click: the press already reached
//! the children (a tab's `on_press` switches the view), and so does the
//! release. While a drag is live the children get no events, so nothing under
//! the moving tab is hovered or pressed.

use iced::{
    Element, Length, Padding, Pixels, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Shell,
        layout::{self, Layout},
        overlay, renderer,
        widget::{Operation, Tree, Widget, tree},
    },
    alignment::Alignment,
    event::Event,
    mouse,
};

/// Movement (px) along the strip's axis before a press becomes a drag. The
/// same 5 px as the queue's `DragColumn`.
const DRAG_THRESHOLD: f32 = 5.0;

/// The strip's main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StripAxis {
    Horizontal,
    Vertical,
}

impl StripAxis {
    fn main(self, point: Point) -> f32 {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }

    /// `(start, length)` of a rectangle along the main axis.
    fn span(self, rect: Rectangle) -> (f32, f32) {
        match self {
            Self::Horizontal => (rect.x, rect.width),
            Self::Vertical => (rect.y, rect.height),
        }
    }

    fn vector(self, amount: f32) -> Vector {
        match self {
            Self::Horizontal => Vector::new(amount, 0.0),
            Self::Vertical => Vector::new(0.0, amount),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum Drag {
    #[default]
    Idle,
    Picking {
        index: usize,
        origin: Point,
    },
    Dragging {
        index: usize,
        origin: Point,
        cursor: Point,
    },
}

#[derive(Debug, Default)]
struct State {
    drag: Drag,
    flex: layout::flex::Cache,
}

/// The index the grabbed child takes in the new order: how many of the other
/// children have their midpoint before the grabbed child's (moved) midpoint.
/// `other_mids` are the main-axis midpoints of every child but the grabbed
/// one, in strip order.
pub(crate) fn landing_index(grabbed_mid: f32, other_mids: &[f32]) -> usize {
    other_mids.iter().filter(|mid| **mid < grabbed_mid).count()
}

/// Which way child `i` steps aside while child `from` is held over `to`
/// (`landing_index`): `1` toward the end, `-1` toward the start, `0` stays.
pub(crate) fn step_aside(i: usize, from: usize, to: usize) -> i8 {
    if i == from {
        return 0;
    }
    if from < to && i > from && i <= to {
        -1
    } else if to < from && i >= to && i < from {
        1
    } else {
        0
    }
}

pub(crate) struct DragStrip<'a, Message, Renderer = iced::Renderer> {
    axis: StripAxis,
    spacing: f32,
    width: Length,
    height: Length,
    align: Alignment,
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    on_reorder: Option<Box<dyn Fn(usize, usize) -> Message + 'a>>,
}

impl<'a, Message, Renderer> DragStrip<'a, Message, Renderer>
where
    Renderer: renderer::Renderer,
{
    pub(crate) fn new(axis: StripAxis) -> Self {
        Self {
            axis,
            spacing: 0.0,
            width: Length::Shrink,
            height: Length::Shrink,
            align: Alignment::Start,
            children: Vec::new(),
            on_reorder: None,
        }
    }

    pub(crate) fn spacing(mut self, amount: impl Into<Pixels>) -> Self {
        self.spacing = amount.into().0;
        self
    }

    pub(crate) fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub(crate) fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Cross-axis alignment of the children.
    pub(crate) fn align(mut self, align: Alignment) -> Self {
        self.align = align;
        self
    }

    pub(crate) fn push(mut self, child: Element<'a, Message, Theme, Renderer>) -> Self {
        self.children.push(child);
        self
    }

    /// `(from, to)` once a drag drops a child somewhere new. Without it the
    /// strip is a plain row / column.
    pub(crate) fn on_reorder(mut self, f: impl Fn(usize, usize) -> Message + 'a) -> Self {
        self.on_reorder = Some(Box::new(f));
        self
    }

    /// Where the grabbed child would land with the cursor at `cursor`.
    fn landing(
        &self,
        layout: Layout,
        tree: &Tree,
        index: usize,
        origin: Point,
        cursor: Point,
    ) -> usize {
        let axis = self.axis;
        let delta = axis.main(cursor) - axis.main(origin);
        let mut grabbed_mid = 0.0;
        let mut others = Vec::with_capacity(self.children.len());
        for (i, (child, _)) in layout.iter(&tree.children).enumerate() {
            let (start, len) = axis.span(child.bounds());
            let mid = start + len / 2.0;
            if i == index {
                grabbed_mid = mid + delta;
            } else {
                others.push(mid);
            }
        }
        landing_index(grabbed_mid, &others)
    }
}

impl<Message, Renderer> iced::advanced::widget::Meta for DragStrip<'_, Message, Renderer> {}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for DragStrip<'_, Message, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let state = tree.state.downcast_mut::<State>();
        let axis = match self.axis {
            StripAxis::Horizontal => layout::flex::Axis::Horizontal,
            StripAxis::Vertical => layout::flex::Axis::Vertical,
        };
        tree.size = layout::flex::resolve(
            axis,
            renderer,
            limits,
            self.width,
            self.height,
            Padding::ZERO,
            self.spacing,
            self.align,
            &mut tree.children,
            &mut self.children,
            &mut state.flex,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
                .for_each(|(child, (layout, state))| {
                    child.operate(state, layout, viewport, renderer, operation);
                });
        });
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
        if self.on_reorder.is_some() {
            let drag = tree.state.downcast_ref::<State>().drag;
            let next = match (event, drag) {
                (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), _) => cursor
                    .position_over(layout.bounds())
                    .and_then(|origin| {
                        layout
                            .iter(&tree.children)
                            .position(|(child, _)| child.bounds().contains(origin))
                            .map(|index| Drag::Picking { index, origin })
                    })
                    .unwrap_or(Drag::Idle),
                (
                    Event::Mouse(mouse::Event::CursorMoved { position, .. }),
                    Drag::Picking { index, origin },
                ) => {
                    let axis = self.axis;
                    if (axis.main(*position) - axis.main(origin)).abs() >= DRAG_THRESHOLD {
                        shell.request_redraw();
                        Drag::Dragging {
                            index,
                            origin,
                            cursor: *position,
                        }
                    } else {
                        drag
                    }
                }
                (
                    Event::Mouse(mouse::Event::CursorMoved { position, .. }),
                    Drag::Dragging { index, origin, .. },
                ) => {
                    shell.request_redraw();
                    Drag::Dragging {
                        index,
                        origin,
                        cursor: *position,
                    }
                }
                (
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    Drag::Dragging {
                        index,
                        origin,
                        cursor,
                    },
                ) => {
                    let to = self.landing(layout, tree, index, origin, cursor);
                    if to != index
                        && let Some(on_reorder) = &self.on_reorder
                    {
                        shell.publish(on_reorder(index, to));
                    }
                    shell.request_redraw();
                    Drag::Idle
                }
                (Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), _) => Drag::Idle,
                _ => drag,
            };
            tree.state.downcast_mut::<State>().drag = next;

            // A live drag keeps every event from the children, so nothing
            // under the moving tab hovers or clicks.
            if matches!(next, Drag::Dragging { .. }) {
                shell.capture_event();
                return;
            }
        }

        self.children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
            .for_each(|(child, (layout, tree))| {
                child.update(tree, event, layout, cursor, renderer, shell, viewport);
            });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if matches!(
            tree.state.downcast_ref::<State>().drag,
            Drag::Dragging { .. }
        ) {
            return mouse::Interaction::Grabbing;
        }
        self.children
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(child, (layout, state))| {
                child.mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        let Drag::Dragging {
            index,
            origin,
            cursor: drag_cursor,
        } = tree.state.downcast_ref::<State>().drag
        else {
            for (child, (layout, state)) in self.children.iter().zip(layout.iter(&tree.children)) {
                child.draw(state, renderer, theme, style, layout, cursor, viewport);
            }
            return;
        };

        let axis = self.axis;
        let strip = layout.bounds();
        let to = self.landing(layout, tree, index, origin, drag_cursor);
        let (grabbed_start, grabbed_len) = layout
            .iter(&tree.children)
            .nth(index)
            .map_or((0.0, 0.0), |(child, _)| axis.span(child.bounds()));
        let step = grabbed_len + self.spacing;

        // The others step aside to open the gap the grabbed child lands in.
        // No child is hovered mid-drag (they get no events either).
        for (i, (child, (child_layout, state))) in self
            .children
            .iter()
            .zip(layout.iter(&tree.children))
            .enumerate()
        {
            if i == index {
                continue;
            }
            let shift = f32::from(step_aside(i, index, to)) * step;
            renderer.with_translation(axis.vector(shift), |renderer| {
                child.draw(
                    state,
                    renderer,
                    theme,
                    style,
                    child_layout,
                    mouse::Cursor::Unavailable,
                    viewport,
                );
            });
        }

        // The grabbed child follows the cursor, kept inside the strip, in its
        // own layer so it draws over its neighbours.
        let (strip_start, strip_len) = axis.span(strip);
        let delta = (axis.main(drag_cursor) - axis.main(origin)).clamp(
            strip_start - grabbed_start,
            strip_start + strip_len - grabbed_start - grabbed_len,
        );
        if let Some((child_layout, state)) = layout.iter(&tree.children).nth(index)
            && let Some(child) = self.children.get(index)
        {
            renderer.with_layer(*viewport, |renderer| {
                renderer.with_translation(axis.vector(delta), |renderer| {
                    child.draw(
                        state,
                        renderer,
                        theme,
                        style,
                        child_layout,
                        mouse::Cursor::Unavailable,
                        viewport,
                    );
                });
            });
        }
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
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{landing_index, step_aside};

    #[test]
    fn landing_counts_the_midpoints_passed() {
        let others = [50.0, 150.0, 250.0];
        assert_eq!(landing_index(10.0, &others), 0);
        assert_eq!(landing_index(100.0, &others), 1);
        assert_eq!(landing_index(300.0, &others), 3);
    }

    #[test]
    fn moving_right_pulls_the_passed_children_left() {
        // 0 held over slot 2: 1 and 2 step back, 3 stays.
        assert_eq!(step_aside(0, 0, 2), 0);
        assert_eq!(step_aside(1, 0, 2), -1);
        assert_eq!(step_aside(2, 0, 2), -1);
        assert_eq!(step_aside(3, 0, 2), 0);
    }

    #[test]
    fn moving_left_pushes_the_passed_children_right() {
        // 3 held over slot 1: 1 and 2 step forward, 0 stays.
        assert_eq!(step_aside(0, 3, 1), 0);
        assert_eq!(step_aside(1, 3, 1), 1);
        assert_eq!(step_aside(2, 3, 1), 1);
        assert_eq!(step_aside(4, 3, 1), 0);
    }

    #[test]
    fn holding_in_place_moves_nothing() {
        for i in 0..5 {
            assert_eq!(step_aside(i, 2, 2), 0);
        }
    }
}
