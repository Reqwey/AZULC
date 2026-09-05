use crate::{
    app::{Launcher, Message},
    domain::{AccountProvider, OfflineAccount},
    services::auth::microsoft,
    theme,
};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill};

use super::components::{
    account_avatar,
    icons::{self, WindowControl},
};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let header = row![
        column![
            text("PLAYER ACCOUNTS").size(31),
            text("Microsoft accounts are supported for normal play. Offline profiles are temporary launch-test tools.")
                .font(theme::BODY_FONT)
                .size(13)
                .color(theme::TEXT)
        ]
    ]
    .align_y(Alignment::Center);

    let microsoft_panel = microsoft_panel(app);
    let offline_panel = container(
        column![
            row![
                text("OFFLINE TEST PROFILE").size(20),
                Space::new().width(Fill),
                text("DEVELOPER ONLY")
                    .font(theme::BODY_BOLD)
                    .size(11)
                    .color(theme::WARNING)
            ]
            .align_y(Alignment::Center),
            text(
                "Temporary launch-testing path. It will be removed before production distribution."
            )
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::WARNING),
            row![
                text_input("Steve", &app.account_input)
                    .on_input(Message::AccountInputChanged)
                    .on_submit(Message::AddOfflineAccount)
                    .padding(12)
                    .size(15)
                    .style(theme::square_text_input),
                button(text("ADD TEST PROFILE  >").size(14))
                    .on_press(Message::AddOfflineAccount)
                    .padding([12, 18])
                    .style(theme::ghost_button)
            ]
            .spacing(10)
        ]
        .spacing(11),
    )
    .padding(18)
    .style(theme::inset);

    let mut list = column![].spacing(9);
    for account in &app.persisted.accounts {
        let selected = app.persisted.selected_account == Some(account.uuid);
        let refreshing = app
            .microsoft_login
            .refreshing_accounts
            .contains(&account.uuid);
        list = list.push(account_row(account, selected, refreshing));
    }
    if app.persisted.accounts.is_empty() {
        list = list.push(
            container(
                column![
                    text("NO PLAYER PROFILE").size(21).color(theme::WARNING),
                    text("Sign in with Microsoft to launch a licensed Minecraft account.")
                        .font(theme::BODY_FONT)
                        .size(13)
                        .color(theme::TEXT)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(20)
            .style(theme::danger_panel),
        );
    }

    scrollable(
        column![
            header,
            microsoft_panel,
            offline_panel,
            text("AVAILABLE PROFILES")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::MUTED),
            list,
            Space::new().height(24)
        ]
        .spacing(14)
        .width(Fill),
    )
    .width(Fill)
    .height(Fill)
    .into()
}

fn microsoft_panel(app: &Launcher) -> Element<'_, Message> {
    let login = &app.microsoft_login;
    let configured = microsoft::is_configured();
    let action = if login.active {
        button(text("CANCEL").size(14))
            .on_press(Message::CancelMicrosoftLogin)
            .padding([12, 18])
            .style(theme::danger_button)
    } else {
        let button = button(text("SIGN IN WITH MICROSOFT  >").size(14))
            .padding([12, 18])
            .style(theme::primary_button);
        if configured {
            button.on_press(Message::BeginMicrosoftLogin)
        } else {
            button
        }
    };

    let mut content = column![
        row![
            column![
                text("MICROSOFT ACCOUNT").size(20),
                text("Official device-code sign-in · Xbox Live · Minecraft Services")
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::TEXT)
            ]
            .spacing(3),
            Space::new().width(Fill),
            action
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(10);

    if !configured {
        content = content.push(
            text(format!(
                "Set {} in .env to your registered public-client application ID.",
                microsoft::CLIENT_ID_ENV
            ))
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::WARNING),
        );
    }
    if !login.user_code.is_empty() {
        content = content.push(
            row![
                column![
                    text("ENTER THIS CODE")
                        .font(theme::BODY_BOLD)
                        .size(11)
                        .color(theme::MUTED),
                    button(text(&login.user_code).size(26).color(theme::LAVENDER_SOFT))
                        .on_press(Message::CopyMicrosoftLoginCode)
                        .padding([5, 9])
                        .style(theme::ghost_button),
                    text("CLICK TO COPY")
                        .font(theme::BODY_BOLD)
                        .size(10)
                        .color(theme::MUTED)
                ]
                .spacing(3),
                Space::new().width(18),
                column![
                    text(&login.status)
                        .font(theme::BODY_BOLD)
                        .size(13)
                        .color(theme::TEXT),
                    text(&login.verification_url)
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::LAVENDER)
                ]
                .spacing(4)
            ]
            .align_y(Alignment::Center),
        );
    } else if login.active {
        content = content.push(
            text(&login.status)
                .font(theme::BODY_BOLD)
                .size(13)
                .color(theme::LAVENDER),
        );
    }
    if let Some(error) = &login.error {
        content = content.push(
            container(
                text(error)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::DANGER),
            )
            .width(Fill)
            .padding(12)
            .style(theme::danger_panel),
        );
    }

    container(content)
        .width(Fill)
        .padding(18)
        .style(theme::panel)
        .into()
}

fn account_row(account: &OfflineAccount, selected: bool, refreshing: bool) -> Element<'_, Message> {
    let mut actions = row![
        text(if selected { "ACTIVE" } else { "STANDBY" })
            .font(theme::BODY_BOLD)
            .size(11)
            .color(if selected {
                theme::SUCCESS
            } else {
                theme::MUTED
            }),
        button(text(if selected { "SELECTED" } else { "USE" }).size(12))
            .on_press(Message::SelectAccount(account.uuid))
            .padding([8, 13])
            .style(if selected {
                theme::primary_button
            } else {
                theme::ghost_button
            })
    ]
    .spacing(11)
    .align_y(Alignment::Center);
    if account.provider == AccountProvider::Microsoft {
        let refresh = account_refresh_button(account.uuid, refreshing);
        actions = actions.push(refresh);
    }
    actions = actions.push(
        button(icons::window_control(WindowControl::Close))
            .on_press(Message::DeleteAccount(account.uuid))
            .width(38)
            .height(38)
            .padding(0)
            .style(theme::danger_window_button),
    );

    container(
        row![
            account_avatar(account, selected, 44.0),
            column![
                row![
                    text(&account.username).size(19),
                    text(account.provider.to_string().to_uppercase())
                        .font(theme::BODY_BOLD)
                        .size(11)
                        .color(if account.provider == AccountProvider::Microsoft {
                            theme::SUCCESS
                        } else {
                            theme::WARNING
                        })
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                text(account.uuid.hyphenated().to_string())
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::MUTED)
            ]
            .spacing(3),
            Space::new().width(Fill),
            actions
        ]
        .spacing(11)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding(14)
    .style(if selected {
        theme::selected_card
    } else {
        theme::panel
    })
    .into()
}

fn account_refresh_button(
    account_id: uuid::Uuid,
    refreshing: bool,
) -> iced::widget::Button<'static, Message> {
    let button = button(
        text(if refreshing {
            "REFRESHING…"
        } else {
            "REFRESH"
        })
        .size(12),
    )
    .padding([8, 13])
    .style(theme::ghost_button);
    if refreshing {
        button
    } else {
        button.on_press(Message::RefreshMicrosoftAccount(account_id))
    }
}
