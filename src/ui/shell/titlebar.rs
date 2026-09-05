use crate::{
    app::{Launcher, Message, navigation::Route},
    theme,
    ui::components::{self, icons},
};
use iced::widget::{Space, button, column, container, mouse_area, row, text};
use iced::{Alignment, Element, Fill, alignment};

use icons::WindowControl;

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let title = container(text("Azusa Minecraft Launcher").align_y(Alignment::Center))
        .padding([0, 16])
        .width(Fill)
        .height(44)
        .align_y(alignment::Vertical::Center);

    let controls = row![
        window_button(WindowControl::Minimize, Message::MinimizeWindow, false),
        window_button(
            if app.window_maximized {
                WindowControl::Restore
            } else {
                WindowControl::Maximize
            },
            Message::ToggleMaximize,
            false
        ),
        window_button(WindowControl::Close, Message::CloseWindow, true),
    ]
    .height(44)
    .align_y(Alignment::Center);

    mouse_area(
        container(row![title, account_button(app), controls])
            .height(44)
            .width(Fill)
            .style(theme::titlebar),
    )
    .on_press(Message::DragWindow)
    .on_double_click(Message::ToggleMaximize)
    .into()
}

fn account_button(app: &Launcher) -> Element<'_, Message> {
    let account = app.persisted.active_account();
    let avatar = match account {
        Some(account) => components::account_avatar(account, true, 32.0),
        None => container(text("@").size(17).color(theme::LAVENDER_SOFT))
            .width(32)
            .height(32)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .style(theme::pill)
            .into(),
    };
    button(
        row![
            avatar,
            column![
                text(account.map_or("NO PROFILE", |value| value.username.as_str())).size(14),
                text(account.map_or("CREATE AN ACCOUNT", |_| "MICROSOFT ACCOUNT"))
                    .font(theme::BODY_FONT)
                    .size(10)
                    .color(theme::MUTED)
            ]
            .spacing(1),
            Space::new().width(Fill),
            icons::account_controls()
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(240)
    .height(44)
    .padding([6, 10])
    .on_press(Message::Navigate(Route::Accounts))
    .style(theme::ghost_button)
    .into()
}

fn window_button<'a>(
    control: WindowControl,
    message: Message,
    danger: bool,
) -> Element<'a, Message> {
    button(icons::window_control(control))
        .width(46)
        .height(44)
        .padding(0)
        .on_press(message)
        .style(if danger {
            theme::danger_window_button
        } else {
            theme::window_button
        })
        .into()
}
