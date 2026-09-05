use crate::{
    app::{LaunchAuthPhase, LaunchAuthState, Launcher, Message},
    theme,
    ui::components,
};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, scrollable, stack, text,
};
use iced::{Alignment, Element, Fill, alignment};

pub(super) fn layer<'a>(
    app: &'a Launcher,
    workspace: Element<'a, Message>,
) -> Element<'a, Message> {
    if !app.launch_auth.is_blocking() {
        return workspace;
    }

    let blocked_workspace = workspace.map(|_| Message::LaunchAuthenticationBackdropPressed);
    let backdrop = mouse_area(
        container(Space::new())
            .width(Fill)
            .height(Fill)
            .style(theme::modal_backdrop),
    )
    .on_press(Message::LaunchAuthenticationBackdropPressed)
    .on_release(Message::LaunchAuthenticationBackdropPressed)
    .on_right_press(Message::LaunchAuthenticationBackdropPressed)
    .on_right_release(Message::LaunchAuthenticationBackdropPressed)
    .on_middle_press(Message::LaunchAuthenticationBackdropPressed)
    .on_middle_release(Message::LaunchAuthenticationBackdropPressed)
    .on_scroll(|_| Message::LaunchAuthenticationBackdropPressed);

    stack![blocked_workspace, backdrop, gate(app)]
        .width(Fill)
        .height(Fill)
        .into()
}

fn gate(app: &Launcher) -> Element<'_, Message> {
    let content: Element<'_, Message> = match app.launch_auth.state() {
        LaunchAuthState::Checking(launch) => {
            let (heading, detail) = match launch.phase {
                LaunchAuthPhase::Validating => (
                    "VERIFYING ACCOUNT",
                    "AZULC is checking the Minecraft token before launching...",
                ),
                LaunchAuthPhase::Refreshing => (
                    "REFRESHING ACCOUNT",
                    "The Minecraft token is no longer valid, refreshing it automatically...",
                ),
            };
            column![
                text(heading).size(27).color(theme::LAVENDER),
                account_summary(app, launch.check.account_id, &launch.username),
                text(detail)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::MUTED)
            ]
            .spacing(14)
            .into()
        }
        LaunchAuthState::Failed { launch, message } => {
            let (heading, hint) = match launch.phase {
                LaunchAuthPhase::Validating => (
                    "ACCOUNT VERIFICATION FAILED",
                    "Check your connection and retry. If the saved sign-in is no longer usable, sign in with Microsoft again.",
                ),
                LaunchAuthPhase::Refreshing => (
                    "ACCOUNT REFRESH FAILED",
                    "Automatic refresh could not restore this account. Retry the check or sign in with Microsoft again.",
                ),
            };
            column![
                text(heading).size(27).color(theme::DANGER),
                account_summary(app, launch.check.account_id, &launch.username),
                text(format!("INSTANCE / {}", launch.instance.name))
                    .font(theme::BODY_BOLD)
                    .size(13)
                    .color(theme::TEXT),
                container(
                    scrollable(
                        text(message)
                            .font(theme::BODY_FONT)
                            .size(12)
                            .color(theme::DANGER)
                    )
                    .height(150)
                    .style(theme::square_scrollable)
                )
                .width(Fill)
                .padding(14)
                .style(theme::danger_panel),
                text(hint)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::MUTED),
                row![
                    Space::new().width(Fill),
                    button(text("CANCEL LAUNCH").size(13))
                        .on_press(Message::CancelLaunchAuthentication)
                        .padding([10, 15])
                        .style(theme::ghost_button),
                    button(text("SIGN IN AGAIN").size(13))
                        .on_press(Message::ReauthenticateLaunchAccount)
                        .padding([10, 15])
                        .style(theme::ghost_button),
                    button(text("RETRY").size(13))
                        .on_press(Message::RetryLaunchAuthentication)
                        .padding([10, 15])
                        .style(theme::primary_button)
                ]
                .spacing(9)
                .align_y(Alignment::Center)
            ]
            .spacing(14)
            .into()
        }
        LaunchAuthState::Idle => Space::new().into(),
    };

    let panel = opaque(
        container(content)
            .width(Fill)
            .max_width(620)
            .padding(24)
            .style(theme::modal_panel),
    );
    container(panel)
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .padding(24)
        .into()
}

fn account_summary<'a>(
    app: &'a Launcher,
    account_id: uuid::Uuid,
    username: &'a str,
) -> Element<'a, Message> {
    let avatar = app
        .persisted
        .accounts
        .iter()
        .find(|account| account.uuid == account_id)
        .map_or_else(
            || {
                container(text("@").size(24).color(theme::LAVENDER))
                    .width(44)
                    .height(44)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center)
                    .style(theme::inset)
                    .into()
            },
            |account| components::account_avatar(account, true, 44.0),
        );

    row![
        avatar,
        text(username)
            .font(theme::BODY_BOLD)
            .size(13)
            .color(theme::TEXT)
    ]
    .spacing(11)
    .align_y(Alignment::Center)
    .into()
}
