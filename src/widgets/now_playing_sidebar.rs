//! The Now Playing sidebar: a column left of the content showing what plays,
//! the album's name, its cover, then the track's title and artist. Toggled by the `ToggleNowPlayingSidebar` hotkey or the
//! Interface setting; Theater Mode grows its cover out of this card.

use iced::{
    ContentFit, Element, Length, Widget as _,
    font::Weight,
    widget::{Space, container, image, text},
};

use crate::{embedded_svg, theme};

/// Inner width of the sidebar column.
pub(crate) const SIDEBAR_WIDTH: f32 = 260.0;
/// Hairline between the sidebar and the content.
pub(crate) const SIDEBAR_BORDER: f32 = 1.0;
/// Padding around the sidebar's content.
const PADDING: f32 = 16.0;

/// What the sidebar draws. Borrowed from the app state each frame.
pub(crate) struct NowPlayingSidebarData<'a> {
    /// The song's title, or the station's now-playing title.
    pub title: &'a str,
    /// The song's artist, or the station's now-playing artist.
    pub artist: &'a str,
    /// The album's name; empty for a station.
    pub album: &'a str,
    /// The card's cover: the full-size art, else the thumbnail.
    pub cover: Option<&'a image::Handle>,
    /// Radio shows the tower glyph where a cover is missing.
    pub is_radio: bool,
    /// Something is current (a queue song or a station). Nothing playing
    /// shows a quiet note.
    pub has_track: bool,
}

/// Total horizontal footprint: the column and its hairline.
pub(crate) fn sidebar_total_width() -> f32 {
    SIDEBAR_WIDTH + SIDEBAR_BORDER
}

/// Side of the square cover.
pub(crate) fn card_cover_side() -> f32 {
    SIDEBAR_WIDTH - 2.0 * PADDING
}

/// The sidebar. Its cover is tagged ([`SIDEBAR_COVER_TAG`]) so Theater Mode
/// can read where it sits.
///
/// [`SIDEBAR_COVER_TAG`]: crate::update::theater::SIDEBAR_COVER_TAG
pub(crate) fn now_playing_sidebar<'a, Message: 'a>(
    data: &NowPlayingSidebarData<'a>,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if data.has_track {
        // Album name, cover, then the title and artist beneath it.
        let mut body = iced::widget::Column::<Element<'a, Message>>::new()
            .width(Length::Fill)
            .height(Length::Fill);
        if !data.album.is_empty() {
            body = body
                .push(
                    text(data.album.to_string())
                        .size(crate::theme::fs(14.0))
                        .font(theme::weighted_ui_font(Weight::Bold))
                        .color(theme::fg0())
                        .width(Length::Fill)
                        .boxed(),
                )
                .push(Space::new().height(Length::Fixed(12.0)).boxed());
        }
        body.push(cover(data).boxed())
            .push(Space::new().height(Length::Fixed(14.0)).boxed())
            .push(
                text(data.title.to_string())
                    .size(crate::theme::fs(22.0))
                    .font(theme::weighted_ui_font(Weight::Bold))
                    .color(theme::fg0())
                    .width(Length::Fill)
                    .boxed(),
            )
            .push(Space::new().height(Length::Fixed(4.0)).boxed())
            .push(
                text(data.artist.to_string())
                    .size(crate::theme::fs(14.0))
                    .font(theme::ui_font())
                    .color(theme::fg3())
                    .width(Length::Fill)
                    .boxed(),
            )
            .boxed()
    } else {
        container(
            text("Nothing playing")
                .size(crate::theme::fs(13.0))
                .font(theme::ui_font())
                .color(theme::fg4()),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .boxed()
    };

    let column_el = container(body)
        .width(Length::Fixed(SIDEBAR_WIDTH))
        .height(Length::Fill)
        .padding(PADDING)
        .style(|_| container::Style {
            background: Some(theme::bg0_hard().into()),
            ..Default::default()
        });
    let border = container(Space::new())
        .width(Length::Fixed(SIDEBAR_BORDER))
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(theme::border().into()),
            ..Default::default()
        });
    iced::widget::row![column_el, border]
        .height(Length::Fill)
        .boxed()
}

/// The cover: a rounded square the width of the column, tagged for Theater
/// Mode.
fn cover<'a, Message: 'a>(data: &NowPlayingSidebarData<'a>) -> Element<'a, Message> {
    let side = card_cover_side();
    let radius = theme::ui_radius_md();
    let picture: Element<'a, Message> = match data.cover {
        Some(handle) => image(handle.clone())
            .width(Length::Fixed(side))
            .height(Length::Fixed(side))
            .content_fit(ContentFit::Cover)
            .border_radius(radius)
            .boxed(),
        None => {
            let glyph = if data.is_radio {
                "assets/icons/radio-tower.svg"
            } else {
                "assets/icons/disc-3.svg"
            };
            container(
                embedded_svg::svg_widget(glyph)
                    .width(Length::Fixed(side * 0.3))
                    .height(Length::Fixed(side * 0.3))
                    .style(|_, _| iced::widget::svg::Style {
                        color: Some(theme::fg4()),
                    }),
            )
            .width(Length::Fixed(side))
            .height(Length::Fixed(side))
            .center(Length::Fixed(side))
            .style(move |_| container::Style {
                background: Some(theme::bg0_soft().into()),
                border: iced::Border {
                    radius,
                    ..Default::default()
                },
                ..Default::default()
            })
            .boxed()
        }
    };
    crate::widgets::bounds_tag::BoundsTag::new(picture, crate::update::theater::SIDEBAR_COVER_TAG)
        .boxed()
}
