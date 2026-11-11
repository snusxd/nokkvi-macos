//! `BeatScale`: draw a child scaled about its center without laying it out
//! again. Theater Mode's Cover Beat grows the cover a few percent on each
//! kick through it; a resize instead would relayout the panel every frame and
//! hand the visualizer layered over the cover a new size (its bar count and
//! MilkDrop's render target debounce on that).
//!
//! The scale is a render transformation: layout, hit testing and events all
//! see the unscaled bounds. With `overflow` the grown edge paints past those
//! bounds (the ancestors' clip still applies); without it the picture zooms
//! inside its own frame. Scale `1.0` draws the child untouched, so
//! the wrapper stays mounted at rest and the tree never changes shape.

use iced::{
    Event, Length, Rectangle, Size, Transformation, Vector,
    advanced::{
        Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree},
    },
};

/// How a cover panel draws the Cover Beat swell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CoverScale {
    /// The picture's scale about its center (`1.0` at rest).
    pub factor: f32,
    /// `true`: the grown picture spills past its frame (Theater Mode, where
    /// the cover stands alone on the field). `false`: it zooms within the
    /// frame (the Queue and Radios panels, beside the list).
    pub overflow: bool,
}

impl CoverScale {
    /// No swell.
    pub(crate) const REST: Self = Self {
        factor: 1.0,
        overflow: false,
    };
}

pub(crate) struct BeatScale<W> {
    content: W,
    scale: CoverScale,
}

impl<W> BeatScale<W> {
    pub(crate) fn new(content: W, scale: CoverScale) -> Self {
        Self { content, scale }
    }
}

/// The transformation that scales by `scale` about `bounds`' center.
fn about_center(bounds: Rectangle, scale: f32) -> Transformation {
    let center = bounds.center();
    Transformation::translate(center.x, center.y)
        * Transformation::scale(scale)
        * Transformation::translate(-center.x, -center.y)
}

impl<W> iced::advanced::widget::Meta for BeatScale<W> {}

impl<W, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for BeatScale<W>
where
    W: Widget<Message, Theme, Renderer>,
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        self.content.tag()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        self.content.state()
    }

    fn diff(&mut self, tree: &mut Tree) {
        self.content.diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(tree, renderer, limits);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .operate(tree, layout, viewport, renderer, operation);
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
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
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
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        let content = &self.content;
        if (self.scale.factor - 1.0).abs() < 1e-4 {
            content.draw(tree, renderer, theme, style, layout, cursor, viewport);
            return;
        }
        let bounds = layout.bounds();
        let clip = if self.scale.overflow {
            *viewport
        } else {
            bounds.intersection(viewport).unwrap_or_default()
        };
        let transformation = about_center(bounds, self.scale.factor);
        let inverse = transformation.inverse();
        renderer.with_layer(clip, |renderer| {
            renderer.with_transformation(transformation, |renderer| {
                content.draw(
                    tree,
                    renderer,
                    theme,
                    style,
                    layout,
                    cursor * inverse,
                    &(*viewport * inverse),
                );
            });
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
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_about_the_center_keeps_the_center_and_grows_the_edges() {
        let bounds = Rectangle::new(iced::Point::new(100.0, 50.0), Size::new(200.0, 200.0));
        let grown = bounds * about_center(bounds, 1.05);
        assert!((grown.center_x() - bounds.center_x()).abs() < 1e-3);
        assert!((grown.center_y() - bounds.center_y()).abs() < 1e-3);
        assert!((grown.width - 210.0).abs() < 1e-3);
        assert!((grown.x - 95.0).abs() < 1e-3);
    }
}
