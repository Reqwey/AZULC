use crate::{
    app::{Launcher, Message},
    domain::DownloadSource,
    theme,
};
use iced::widget::{
    Space, button, column, container, pick_list, row, rule, scrollable, slider, text,
};
use iced::{Alignment, Element, Fill, padding};

use super::components::section;
use crate::ui::components::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let policy = &app.persisted.settings.download;
    let worker_limit = app.system_resources.cpu_threads.max(1);
    let worker_limit_u16 = u16::try_from(worker_limit).unwrap_or(u16::MAX);
    let workers = u16::try_from(policy.concurrency)
        .unwrap_or(worker_limit_u16)
        .clamp(1, worker_limit_u16);
    let route = section(
        "DOWNLOAD ROUTE",
        column![
            row![
                column![
                    text("CONTENT SOURCE")
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(theme::MUTED),
                    text("Applied to Minecraft metadata, libraries, assets, Forge, Fabric, and NeoForge.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(4),
                Space::new().width(Fill),
                pick_list(
                    [DownloadSource::Official, DownloadSource::Bmcl],
                    Some(policy.source),
                    Message::DownloadSourcePicked,
                )
                .width(210)
                .style(theme::square_pick_list)
            ]
            .align_y(Alignment::Center),
            rule::horizontal(1),
            row![
                column![
                    text("PARALLEL WORKERS")
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(theme::MUTED),
                    text(format!(
                        "Detected {worker_limit} CPU hardware threads; this defines the worker limit."
                    ))
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::MUTED)
                ]
                .spacing(4),
                Space::new().width(Fill),
                slider(
                    1..=worker_limit_u16,
                    workers,
                    Message::DownloadConcurrencyChanged
                )
                .step(1_u16)
                .width(300)
                .style(theme::square_slider),
                container(
                    text(format!("{:02}", policy.concurrency))
                        .size(19)
                        .color(theme::LAVENDER)
                )
                .width(56)
                .padding(8)
                .style(theme::inset)
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        ]
        .spacing(16),
    );

    let mut signals = column![
        row![
            column![
                text("NETWORK SIGNALS").size(20),
                text("Round-trip response checks for launcher-critical services.")
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::MUTED)
            ]
            .spacing(3),
            Space::new().width(Fill),
            button(text("PING AGAIN").size(12))
                .on_press(Message::RefreshPings)
                .padding([8, 13])
                .style(theme::ghost_button)
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(9);
    if app.pings.is_empty() {
        signals = signals.push(
            container(
                text("WAITING FOR NETWORK CHECK…")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
            )
            .width(Fill)
            .padding(16)
            .style(theme::inset),
        );
    } else {
        for ping in &app.pings {
            signals = signals.push(
                container(
                    row![
                        text(if ping.reachable { "●" } else { "×" }).size(16).color(
                            if ping.reachable {
                                theme::SUCCESS
                            } else {
                                theme::DANGER
                            }
                        ),
                        column![
                            text(&ping.name).size(15),
                            text(&ping.url)
                                .font(theme::BODY_FONT)
                                .size(11)
                                .color(theme::MUTED)
                        ]
                        .spacing(2),
                        Space::new().width(Fill),
                        text(if ping.reachable {
                            format!("{} MS", ping.latency_ms)
                        } else {
                            "OFFLINE".into()
                        })
                        .font(theme::BODY_BOLD)
                        .size(13)
                        .color(if ping.reachable {
                            latency_color(ping.latency_ms)
                        } else {
                            theme::DANGER
                        })
                    ]
                    .spacing(11)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(12)
                .style(theme::inset),
            );
        }
    }
    let signals = container(signals).padding(18).style(theme::panel);
    scrollable(
        column![route, signals]
            .spacing(14)
            .padding(padding::bottom(CONTENT_END_GAP)),
    )
    .width(Fill)
    .height(Fill)
    .spacing(SCROLLBAR_GAP)
    .style(theme::square_scrollable)
    .into()
}

fn latency_color(milliseconds: u64) -> iced::Color {
    if milliseconds < 250 {
        theme::SUCCESS
    } else if milliseconds < 900 {
        theme::WARNING
    } else {
        theme::DANGER
    }
}
