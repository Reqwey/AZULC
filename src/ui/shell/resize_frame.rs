use crate::{app::Message, theme};
use iced::mouse;
use iced::widget::{Space, column, container, mouse_area, row};
use iced::{Element, Fill, Length, window};

pub(super) fn view<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    const EDGE: f32 = 12.0;
    const CORNER: f32 = 24.0;

    let top = row![
        resize_handle(
            window::Direction::NorthWest,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyDown,
        ),
        resize_handle(
            window::Direction::North,
            Fill,
            EDGE.into(),
            mouse::Interaction::ResizingVertically,
        ),
        resize_handle(
            window::Direction::NorthEast,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyUp,
        )
    ];
    let middle = row![
        resize_handle(
            window::Direction::West,
            EDGE.into(),
            Fill,
            mouse::Interaction::ResizingHorizontally,
        ),
        content,
        resize_handle(
            window::Direction::East,
            EDGE.into(),
            Fill,
            mouse::Interaction::ResizingHorizontally,
        )
    ]
    .height(Fill);
    let bottom = row![
        resize_handle(
            window::Direction::SouthWest,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyUp,
        ),
        resize_handle(
            window::Direction::South,
            Fill,
            EDGE.into(),
            mouse::Interaction::ResizingVertically,
        ),
        resize_handle(
            window::Direction::SouthEast,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyDown,
        )
    ];

    container(column![top, middle, bottom].width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(theme::canvas)
        .into()
}

fn resize_handle<'a>(
    direction: window::Direction,
    width: Length,
    height: Length,
    interaction: mouse::Interaction,
) -> Element<'a, Message> {
    mouse_area(Space::new().width(width).height(height))
        .on_press(Message::ResizeWindow(direction))
        .interaction(interaction)
        .into()
}
