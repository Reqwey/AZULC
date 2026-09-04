use crate::{
    app::{Launcher, Message, navigation::SettingsTab},
    domain::DownloadSource,
    theme,
};
use iced::widget::{
    Space, button, column, container, pick_list, row, rule, scrollable, slider, text,
};
use iced::{Alignment, Element, Fill, padding};

use super::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(crate) fn view(app: &Launcher) -> Element<'_, Message> {
    let header = row![column![
        text("APP SETTINGS").size(31),
        text("Network routes, runtime inventory, and launcher information.")
            .font(theme::BODY_FONT)
            .size(13)
            .color(theme::MUTED)
    ]]
    .align_y(Alignment::Center);

    let mut tabs = row![].spacing(8);
    for tab in SettingsTab::ALL {
        tabs = tabs.push(
            button(text(tab.label()).size(14))
                .on_press(Message::SettingsTabSelected(tab))
                .padding([9, 14])
                .style(if app.settings_tab == tab {
                    theme::primary_button
                } else {
                    theme::nav_button
                }),
        );
    }
    let content = match app.settings_tab {
        SettingsTab::Downloads => downloads(app),
        SettingsTab::Java => java(app),
        SettingsTab::About => about(app),
    };

    column![header, tabs, rule::horizontal(1), content]
        .spacing(15)
        .width(Fill)
        .height(Fill)
        .into()
}

fn downloads(app: &Launcher) -> Element<'_, Message> {
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
                    text("CONTENT SOURCE").font(theme::BODY_BOLD).size(12).color(theme::MUTED),
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
                    text("PARALLEL WORKERS").font(theme::BODY_BOLD).size(12).color(theme::MUTED),
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
                container(text(format!("{:02}", policy.concurrency)).size(19).color(theme::LAVENDER))
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

fn java(app: &Launcher) -> Element<'_, Message> {
    let mut list = column![
        row![
            column![
                text("DETECTED JAVA RUNTIMES").size(20),
                text(
                    "AZULC chooses the smallest compatible major version for each Minecraft build."
                )
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED)
            ]
            .spacing(3),
            Space::new().width(Fill),
            button(text("SCAN AGAIN").size(12))
                .on_press(Message::RefreshJava)
                .padding([8, 13])
                .style(theme::ghost_button)
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(10);
    if app.java_runtimes.is_empty() {
        list = list.push(
            container(
                column![
                    text("NO JAVA FOUND").size(21).color(theme::WARNING),
                    text("Modern Minecraft generally needs Java 17 or 21; legacy builds may need Java 8.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(18)
            .style(theme::danger_panel),
        );
    } else {
        for runtime in &app.java_runtimes {
            list = list.push(
                container(
                    row![
                        container(
                            text(format!("J{}", runtime.major))
                                .size(20)
                                .color(theme::LAVENDER_SOFT)
                        )
                        .width(54)
                        .height(48)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(theme::pill),
                        column![
                            row![
                                text(format!("Java {}", runtime.major)).size(18),
                                text(&runtime.vendor)
                                    .font(theme::BODY_BOLD)
                                    .size(11)
                                    .color(theme::LAVENDER)
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                            text(runtime.path.display().to_string())
                                .font(theme::BODY_FONT)
                                .size(11)
                                .color(theme::MUTED)
                        ]
                        .spacing(3),
                        Space::new().width(Fill),
                        text(&runtime.version)
                            .font(theme::BODY_BOLD)
                            .size(12)
                            .color(theme::SUCCESS)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(14)
                .style(theme::panel),
            );
        }
    }
    scrollable(list.padding(padding::bottom(CONTENT_END_GAP)))
        .width(Fill)
        .height(Fill)
        .spacing(SCROLLBAR_GAP)
        .style(theme::square_scrollable)
        .into()
}

fn about(app: &Launcher) -> Element<'_, Message> {
    let architecture = row![
        about_card(
            "UI",
            "ICED 0.14",
            "Native Rust widgets",
            "https://github.com/iced-rs/iced",
        ),
        about_card(
            "BUILD",
            env!("CARGO_PKG_VERSION"),
            "Open Source",
            "https://github.com/Reqwey/AZULC",
        )
    ]
    .spacing(12);
    let data = section(
        "LOCAL DATA",
        column![
            path_info_line("ROOT", app.paths.data.clone()),
            path_info_line("MINECRAFT", app.paths.minecraft.clone()),
            path_info_line("INSTANCES", app.paths.instances.clone()),
            info_line("FONT", "Pixelify Sans + Space Mono (OFL)".into()),
        ]
        .spacing(10),
    );
    let acknowledgements = section(
        "ACKNOWLEDGEMENTS",
        row![
            acknowledgement_card(
                "SJMCL",
                "Source-code & implementation reference",
                "https://mc.sjtu.cn/sjmcl/",
            ),
            acknowledgement_card(
                "BMCLAPI",
                "Minecraft download mirror provider",
                "https://bmclapidoc.bangbang93.com/",
            ),
        ]
        .spacing(12),
    );
    scrollable(
        column![
            container(
                column![
                    text("AZULC").size(68).color(theme::LAVENDER_SOFT),
                    text("AZUSA MINECRAFT LAUNCHER")
                        .font(theme::BODY_BOLD)
                        .size(13)
                        .color(theme::LAVENDER),
                    text("A next-generation lightweight, high-performance Minecraft launcher and technology validation platform.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::TEXT)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(24)
            .style(theme::hero),
            architecture,
            data,
            acknowledgements
        ]
        .spacing(14)
        .padding(padding::bottom(CONTENT_END_GAP)),
    )
    .width(Fill)
    .height(Fill)
    .spacing(SCROLLBAR_GAP)
    .style(theme::square_scrollable)
    .into()
}

fn section<'a>(title: &'a str, body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text(title)
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER),
            body.into()
        ]
        .spacing(13),
    )
    .width(Fill)
    .padding(18)
    .style(theme::panel)
    .into()
}

fn about_card<'a>(
    label: &'a str,
    value: &'a str,
    detail: &'a str,
    url: &'static str,
) -> Element<'a, Message> {
    button(
        row![
            column![
                text(label)
                    .font(theme::BODY_BOLD)
                    .size(13)
                    .color(theme::MUTED),
                text(value).size(24).color(theme::LAVENDER_SOFT),
                text(detail)
                    .font(theme::BODY_FONT)
                    .size(13)
                    .color(theme::TEXT),
                text(url)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::LAVENDER)
            ]
            .spacing(4),
            Space::new().width(Fill),
            text("OPEN ↗")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER)
        ]
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([17, 18])
    .on_press(Message::OpenExternalUrl(url))
    .style(theme::version_card_button)
    .into()
}

fn info_line(label: &'static str, value: String) -> Element<'static, Message> {
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(value)
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::LAVENDER_SOFT)
    ]
    .into()
}

fn path_info_line(label: &'static str, path: std::path::PathBuf) -> Element<'static, Message> {
    let value = path.display().to_string();
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(value)
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::LAVENDER_SOFT),
        button(text("OPEN").font(theme::BODY_BOLD).size(12))
            .on_press(Message::OpenFolder(path))
            .padding([5, 9])
            .style(theme::ghost_button)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn acknowledgement_card(
    name: &'static str,
    contribution: &'static str,
    url: &'static str,
) -> Element<'static, Message> {
    button(
        row![
            column![
                text(name).size(24).color(theme::LAVENDER_SOFT),
                text(contribution)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::TEXT),
                text(url)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::LAVENDER)
            ]
            .spacing(5),
            Space::new().width(Fill),
            text("OPEN ↗")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER)
        ]
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([17, 18])
    .on_press(Message::OpenExternalUrl(url))
    .style(theme::version_card_button)
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
