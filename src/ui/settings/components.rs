use crate::{app::Message, theme};
use iced::widget::{column, container, text};
use iced::{Element, Fill};

pub(super) fn section<'a>(
    title: &'a str,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER),
            body.into()
        ]
        .spacing(13),
    )
    .width(Fill)
    .padding(18)
    .style(theme::panel)
    .into()
}
