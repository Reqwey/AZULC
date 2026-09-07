use crate::{app::Message, theme};
use iced::widget::text_input;
use iced::{Element, Length};

pub(in crate::ui) fn compact_search<'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .width(Length::Fixed(220.0))
        .size(12)
        .padding([9, 12])
        .style(theme::square_text_input)
        .into()
}
