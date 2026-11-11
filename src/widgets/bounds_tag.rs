//! `BoundsTag`: a pass-through wrapper that makes a widget's laid-out bounds
//! readable by an [`Id`], and [`probe_bounds`], the widget operation that
//! reads them.
//!
//! Theater Mode's enter animation grows the playing cover from where it sits on
//! screen, so it needs the REAL rectangle of the Queue's cover panel, which the
//! layout math (artwork modes, split view, bleed) only gives indirectly. The
//! wrapper changes nothing else: size, layout, events and drawing are the
//! child's own.

use iced::{
    Event, Length, Rectangle, Size, Task, Vector,
    advanced::{
        Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Id, Operation, Tree, operate, operation},
    },
};

/// Reports its child's bounds to a widget operation under `id`.
pub(crate) struct BoundsTag<W> {
    content: W,
    id: Id,
}

impl<W> BoundsTag<W> {
    pub(crate) fn new(content: W, id: &'static str) -> Self {
        Self {
            content,
            id: Id::new(id),
        }
    }
}

impl<W> iced::advanced::widget::Meta for BoundsTag<W> {}

impl<W, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for BoundsTag<W>
where
    W: Widget<Message, Theme, Renderer>,
    Renderer: iced::advanced::Renderer,
{
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    // The child fills our bounds exactly, so it shares our `Layout`.
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);

        tree.size = tree.children[0].size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(Some(&self.id), layout.bounds(), viewport);
        self.content
            .operate(&mut tree.children[0], layout, viewport, renderer, operation);
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
        self.content.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
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
        self.content.draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
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
        self.content.overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}

/// Reads the bounds of the widgets tagged with each of `ids`, in the tree.
struct FindBounds {
    ids: Vec<Id>,
    found: Vec<Option<Rectangle>>,
}

impl Operation<Option<Rectangle>> for FindBounds {
    fn traverse(&mut self, recurse: &mut dyn FnMut(&mut dyn Operation<Option<Rectangle>>)) {
        recurse(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, _viewport: &Rectangle) {
        if let Some(id) = id
            && let Some(slot) = self.ids.iter().position(|wanted| wanted == id)
        {
            self.found[slot] = Some(bounds);
        }
    }

    fn finish(&self) -> operation::Outcome<Option<Rectangle>> {
        // The first id in priority order that is in the tree.
        operation::Outcome::Some(self.found.iter().copied().flatten().next())
    }
}

/// A task answering with the on-screen bounds of the first of `ids`
/// (highest priority first) that is tagged in the tree, or `None` when none
/// is.
pub(crate) fn probe_bounds(ids: &[&'static str]) -> Task<Option<Rectangle>> {
    operate(FindBounds {
        ids: ids.iter().map(|&id| Id::new(id)).collect(),
        found: vec![None; ids.len()],
    })
}
