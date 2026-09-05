use crate::{domain::Account, theme};
use iced::widget::{container, image as iced_image, text};
use iced::{Element, Length, alignment};

pub(in crate::ui) fn account_avatar<'a, Message>(
    account: &'a Account,
    selected: bool,
    size: f32,
) -> Element<'a, Message>
where
    Message: 'a,
{
    if let Some(bytes) = account
        .avatar_rgba
        .as_ref()
        .filter(|bytes| bytes.len() == 64 * 64 * 4)
    {
        return container(
            iced_image(iced_image::Handle::from_rgba(64, 64, bytes.clone()))
                .width(Length::Fixed(size))
                .height(Length::Fixed(size)),
        )
        .width(size)
        .height(size)
        .into();
    }

    container(text("@").size(size * 0.52).color(if selected {
        theme::LAVENDER_SOFT
    } else {
        theme::LAVENDER
    }))
    .width(size)
    .height(size)
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .style(if selected { theme::pill } else { theme::inset })
    .into()
}
