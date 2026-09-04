use crate::{
    app::{
        Launcher, Message,
        navigation::{Route, VersionFilter},
    },
    theme,
};
use chrono::{Local, Timelike};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use super::{CONTENT_END_GAP, SCROLLBAR_GAP, brand};

pub(crate) fn view(app: &Launcher) -> Element<'_, Message> {
    let account = app.persisted.active_account();
    let greeting = greeting();
    let identity = account.map_or("PLAYER", |value| value.username.as_str());
    let heading = row![
        text(format!("{greeting}, {identity}."))
            .size(32)
            .color(theme::TEXT),
        Space::new().width(Fill),
        text(Local::now().format("%H:%M // %Y.%m.%d").to_string())
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::LAVENDER)
    ]
    .align_y(Alignment::Center);

    let stats = row![
        stat_card(
            "INSTANCES",
            app.persisted.instances.len().to_string(),
            "installed workspaces",
            theme::LAVENDER,
        ),
        stat_card(
            "WORLDS",
            app.insights.total_worlds.to_string(),
            "local saves detected",
            theme::SUCCESS,
        ),
        stat_card(
            "PLAY TIME",
            format_duration(app.insights.total_play_time_seconds),
            "tracked after game ready",
            theme::WARNING,
        ),
    ]
    .spacing(14);

    let versions = row![
        version_card(
            "LATEST RELEASE",
            &app.highlights.release,
            "stable channel",
            VersionFilter::Release,
        ),
        version_card(
            "LATEST SNAPSHOT",
            &app.highlights.snapshot,
            "preview channel",
            VersionFilter::Snapshot,
        ),
        version_card(
            "APRIL FOOLS",
            app.highlights.april_fools.as_deref().unwrap_or_default(),
            "latest experiment",
            VersionFilter::AprilFools,
        ),
    ]
    .spacing(14);

    let onboarding: Element<'_, Message> = if account.is_none() {
        container(
            row![
                column![
                    text("NO PLAYER PROFILE YET").size(21).color(theme::WARNING),
                    text("Sign in with a Microsoft account to unlock licensed Minecraft launching. Offline profiles are temporary test tools.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(6),
                Space::new().width(Fill),
                button(text("CREATE PROFILE  >").size(15))
                    .on_press(Message::Navigate(Route::Accounts))
                    .padding([11, 16])
                    .style(theme::primary_button)
            ]
            .align_y(Alignment::Center),
        )
        .padding(18)
        .style(theme::danger_panel)
        .into()
    } else {
        let selected = app.selected_instance();
        container(
            row![
                column![
                    text("QUICK LAUNCH")
                        .font(theme::BODY_BOLD)
                        .size(10)
                        .color(theme::MUTED),
                    text(selected.map_or("Choose an instance", |instance| instance.name.as_str()))
                        .size(22),
                    text(
                        selected.map_or("Open the library to select a workspace.", |instance| {
                            instance.description.as_str()
                        })
                    )
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::MUTED)
                ]
                .spacing(4),
                Space::new().width(Fill),
                button(
                    text(if app.launching {
                        "RUNNING…"
                    } else {
                        "PLAY  >"
                    })
                    .size(16)
                )
                .on_press_maybe(
                    (selected.is_some() && !app.launching).then_some(Message::LaunchSelected)
                )
                .padding([12, 24])
                .style(theme::primary_button)
            ]
            .align_y(Alignment::Center),
        )
        .padding(18)
        .style(theme::panel)
        .into()
    };

    scrollable(
        column![
            heading,
            Space::new().height(14),
            brand::view(),
            Space::new().height(18),
            stats,
            Space::new().height(14),
            versions,
            Space::new().height(14),
            onboarding
        ]
        .padding(padding::bottom(CONTENT_END_GAP)),
    )
    .width(Fill)
    .height(Fill)
    .spacing(SCROLLBAR_GAP)
    .style(theme::square_scrollable)
    .into()
}

fn greeting() -> &'static str {
    match Local::now().hour() {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        18..=22 => "Good evening",
        _ => "Still awake",
    }
}

fn present(value: &str) -> &str {
    if value.is_empty() {
        "Syncing…"
    } else {
        value
    }
}

fn stat_card<'a>(
    label: &'a str,
    value: String,
    detail: &'a str,
    accent: iced::Color,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .font(theme::BODY_BOLD)
                .size(10)
                .color(theme::MUTED),
            text(value).size(31).color(accent),
            text(detail)
                .font(theme::BODY_FONT)
                .size(10)
                .color(theme::MUTED)
        ]
        .spacing(5),
    )
    .width(Fill)
    .padding(17)
    .style(theme::stat_card)
    .into()
}

fn version_card<'a>(
    label: &'a str,
    version: &'a str,
    detail: &'a str,
    filter: VersionFilter,
) -> Element<'a, Message> {
    button(
        row![
            column![
                text(label)
                    .font(theme::BODY_BOLD)
                    .size(9)
                    .color(theme::MUTED),
                text(present(version)).size(21).color(theme::LAVENDER_SOFT),
                text(detail)
                    .font(theme::BODY_FONT)
                    .size(10)
                    .color(theme::MUTED)
            ]
            .spacing(4),
            Space::new().width(Fill),
            text(">").size(22).color(theme::LAVENDER)
        ]
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding(16)
    .on_press_maybe(
        (!version.is_empty()).then(|| Message::OpenHighlightedVersion(filter, version.to_owned())),
    )
    .style(theme::version_card_button)
    .into()
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format!("{hours:02}H {minutes:02}M")
}
