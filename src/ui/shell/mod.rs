mod resize_frame;
mod sidebar;
mod titlebar;

use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Element, Fill};

pub(super) fn view<'a>(app: &'a Launcher, page: Element<'a, Message>) -> Element<'a, Message> {
    let workspace = row![
        sidebar::view(app),
        container(page)
            .width(Fill)
            .height(Fill)
            .padding(iced::Padding {
                top: 24.0,
                right: 24.0,
                bottom: 0.0,
                left: 24.0,
            })
            .style(theme::canvas)
    ]
    .width(Fill)
    .height(Fill);

    let mut shell = column![titlebar::view(app), workspace]
        .width(Fill)
        .height(Fill);
    if let Some(notice) = &app.notice {
        shell = shell.push(
            container(
                row![
                    text("!").size(22).color(theme::LAVENDER),
                    text(notice)
                        .font(theme::BODY_BOLD)
                        .size(13)
                        .color(theme::TEXT),
                    Space::new().width(Fill),
                    button(text("DISMISS").size(13))
                        .on_press(Message::DismissNotice)
                        .padding([7, 12])
                        .style(theme::ghost_button)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([10, 20])
            .style(theme::inset),
        );
    }
    shell.into()
}

pub(super) fn frame<'a>(app: &Launcher, content: Element<'a, Message>) -> Element<'a, Message> {
    if app.window_maximized {
        content
    } else {
        resize_frame::view(content)
    }
}
