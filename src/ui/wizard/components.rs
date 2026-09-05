use crate::{app::Message, theme};
use iced::widget::{Space, column, row, text};
use iced::{Element, Fill};

pub(super) fn field<'a>(
    label: &'a str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(label)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        control.into()
    ]
    .spacing(6)
    .into()
}

pub(super) fn summary_line<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(value)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::LAVENDER_SOFT)
    ]
    .width(Fill)
    .into()
}
