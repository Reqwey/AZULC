use crate::{app::Message, theme};
use iced::Fill;
use iced::widget::{column, container, text};

pub(super) fn empty_catalog<'a>(
    title: &'a str,
    detail: &'a str,
) -> iced::widget::Container<'a, Message> {
    container(
        column![
            text(title).size(20).color(theme::MUTED),
            text(detail)
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED)
        ]
        .spacing(5),
    )
    .width(Fill)
    .padding(24)
    .style(theme::inset)
}
