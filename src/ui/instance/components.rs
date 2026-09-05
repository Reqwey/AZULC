use crate::{
    app::{Message, navigation::Route},
    theme,
};
use iced::widget::{Space, button, column, container, row, text, text_input};
use iced::{Element, Fill, alignment};

pub(super) fn section<'a>(
    title: &'a str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .font(theme::BODY_BOLD)
                .size(13)
                .color(theme::LAVENDER),
            content.into()
        ]
        .spacing(13),
    )
    .width(Fill)
    .padding(18)
    .style(theme::panel)
    .into()
}

pub(super) fn labeled_input<'a>(
    label: &'a str,
    value: &'a str,
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        text(label)
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED),
        text_input(placeholder, value)
            .on_input(on_input)
            .padding(11)
            .size(14)
            .style(theme::square_text_input)
    ]
    .spacing(5)
    .into()
}

pub(super) fn setting_slider<'a>(
    label: &'a str,
    value: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        row![
            text(label)
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::MUTED),
            Space::new().width(Fill),
            text(value)
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER)
        ],
        control.into()
    ]
    .spacing(6)
    .into()
}

pub(super) fn info_card<'a>(
    label: &'a str,
    value: String,
    detail: String,
    color: iced::Color,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::MUTED),
            text(value).size(23).color(color),
            text(detail)
                .font(theme::BODY_FONT)
                .size(11)
                .color(theme::MUTED)
        ]
        .spacing(4),
    )
    .width(Fill)
    .padding(16)
    .style(theme::stat_card)
    .into()
}

pub(super) fn missing_state<'a>(heading: &'a str) -> iced::widget::Container<'a, Message> {
    container(
        column![
            text(heading).size(28).color(theme::MUTED),
            text("Choose an available workspace on the left.")
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED),
            button(text("BACK HOME  >").size(14))
                .on_press(Message::Navigate(Route::Home))
                .style(theme::primary_button)
        ]
        .spacing(12),
    )
    .width(Fill)
    .height(Fill)
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(theme::panel)
}

pub(super) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
