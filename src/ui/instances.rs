use crate::{
    app::{
        InstallJob, LaunchSession, Launcher, Message,
        navigation::{InstanceTab, Route},
    },
    domain::{InstallStage, Instance},
    services::content::{self, ContentKind},
    theme,
};
use chrono::{DateTime, Local};
use iced::widget::text::Wrapping;
use iced::widget::{
    Space, button, checkbox, column, container, progress_bar, row, rule, scrollable, slider, text,
    text_input,
};
use iced::{Alignment, Element, Fill, Length, alignment, padding};
use std::time::{Duration, UNIX_EPOCH};

use super::{CONTENT_END_GAP, SCROLLBAR_GAP, media};

pub(crate) fn view(app: &Launcher) -> Element<'_, Message> {
    match app.selected {
        Some(id)
            if app.jobs.get(&id).is_some_and(|job| {
                job.active || !job.progress.stage.eq(&InstallStage::Complete)
            }) =>
        {
            install_job(app.jobs.get(&id).expect("selected job exists"))
        }
        _ => app
            .selected_instance()
            .map(|instance| instance_detail(app, instance))
            .unwrap_or_else(|| empty_state().into()),
    }
}

fn instance_detail<'a>(app: &'a Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let launching = app.is_instance_launching(instance.id);
    let deleting = app.is_instance_deleting(instance.id);
    let launch_label = if deleting {
        "DELETING…"
    } else if launching {
        "RUNNING…"
    } else {
        "PLAY  >"
    };
    let header = row![
        column![
            row![
                media::instance_marker(instance.color, 24),
                text(&instance.name).size(30),
                text(if instance.favorite { "📌" } else { "" })
                    .size(22)
                    .color(theme::WARNING)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(if instance.description.is_empty() {
                "No description yet."
            } else {
                &instance.description
            })
            .font(theme::BODY_FONT)
            .size(13)
            .color(theme::MUTED)
        ]
        .spacing(4),
        Space::new().width(Fill),
        button(text(launch_label).size(15))
            .on_press_maybe((!launching && !deleting).then_some(Message::LaunchSelected))
            .padding([10, 20])
            .style(theme::primary_button)
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut tabs = row![].spacing(7);
    for tab in InstanceTab::ALL {
        tabs = tabs.push(
            button(text(tab.label()).size(13))
                .on_press(Message::SelectInstanceTab(tab))
                .padding([8, 12])
                .style(if app.instance_tab == tab {
                    theme::primary_button
                } else {
                    theme::nav_button
                }),
        );
    }

    let tabs = scrollable(tabs)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(4)
                .scroller_width(4)
                .spacing(4),
        ))
        .height(Length::Shrink)
        .style(theme::square_scrollable);

    let page = match app.instance_tab {
        InstanceTab::Overview => overview(app, instance),
        InstanceTab::Settings => instance_settings(app, instance),
        InstanceTab::Worlds
        | InstanceTab::Mods
        | InstanceTab::ResourcePacks
        | InstanceTab::ShaderPacks
        | InstanceTab::DataPacks
        | InstanceTab::Screenshots => content_list(app, instance),
    };

    column![header, tabs, rule::horizontal(1), page]
        .spacing(14)
        .width(Fill)
        .height(Fill)
        .into()
}

fn overview<'a>(app: &'a Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let counts = app
        .insights
        .instances
        .iter()
        .find(|item| item.instance_id == instance.id);
    let information = row![
        info_card(
            "GAME BUILD",
            instance.minecraft_version.clone(),
            format_loader(instance),
            theme::LAVENDER,
        ),
        info_card(
            "PLAY TIME",
            format_duration(instance.play_time_seconds),
            "tracked runtime".into(),
            theme::SUCCESS,
        ),
        info_card(
            "CONTENT",
            format!("{} MODS", counts.map_or(0, |value| value.mods)),
            format!(
                "{} worlds / {} packs",
                counts.map_or(0, |value| value.saves),
                counts.map_or(0, |value| value.resource_packs)
            ),
            theme::WARNING,
        )
    ]
    .spacing(12);

    let folder = container(
        row![
            column![
                text("WORKSPACE PATH")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
                text(instance.game_dir.display().to_string())
                    .font(theme::BODY_FONT)
                    .size(13)
                    .color(theme::LAVENDER_SOFT)
            ]
            .spacing(5)
            .width(Fill),
            button(text("OPEN").size(13))
                .on_press(Message::OpenPath(instance.game_dir.clone()))
                .padding([10, 14])
                .style(theme::ghost_button)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding(15)
    .style(theme::inset);

    if let Some(session) = app.launch_session(instance.id) {
        column![information, folder, launch_session(session)]
            .spacing(14)
            .width(Fill)
            .height(Fill)
            .padding(padding::bottom(CONTENT_END_GAP))
            .into()
    } else {
        scrollable(
            column![information, folder]
                .spacing(14)
                .width(Fill)
                .padding(padding::bottom(CONTENT_END_GAP)),
        )
        .width(Fill)
        .height(Fill)
        .spacing(SCROLLBAR_GAP)
        .style(theme::square_scrollable)
        .into()
    }
}

fn content_list<'a>(app: &'a Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let kind = app
        .instance_tab
        .content_kind()
        .expect("content list is only rendered for content tabs");
    let folder = instance.game_dir.join(
        kind.directory()
            .expect("every instance content kind has a directory"),
    );
    let download: Element<'a, Message> = if kind.downloadable() {
        button(text("DOWNLOAD").size(13))
            .on_press(Message::OpenResourceBrowser(kind))
            .padding([9, 15])
            .style(theme::primary_button)
            .into()
    } else {
        Space::new().width(0).into()
    };
    let query = app.content_query.trim();
    let visible_count = app
        .content_entries
        .iter()
        .filter(|entry| content::name_matches_query(&entry.name, query))
        .count();
    let item_count = if kind == ContentKind::Mods && !query.is_empty() {
        format!(
            "{visible_count} / {} LOCAL ITEMS",
            app.content_entries.len()
        )
    } else {
        format!("{} LOCAL ITEMS", app.content_entries.len())
    };
    let search: Element<'a, Message> = if kind == ContentKind::Mods {
        text_input("SEARCH MODS", &app.content_query)
            .on_input(Message::ContentQueryChanged)
            .width(Length::Fixed(220.0))
            .size(12)
            .padding([9, 12])
            .style(theme::square_text_input)
            .into()
    } else {
        Space::new().width(0).into()
    };
    let toolbar = row![
        column![
            text(app.instance_tab.label().to_uppercase()).size(21),
            text(item_count)
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::MUTED)
        ]
        .spacing(1),
        Space::new().width(Fill),
        search,
        button(text("OPEN FOLDER").size(12))
            .on_press(Message::OpenFolder(folder))
            .padding([9, 13])
            .style(theme::ghost_button),
        download
    ]
    .spacing(9)
    .align_y(Alignment::Center);

    if app.content_loading {
        return column![
            toolbar,
            container(
                column![
                    text("SCANNING WORKSPACE…").size(20).color(theme::LAVENDER),
                    text("Reading direct children without touching game data.")
                        .font(theme::BODY_FONT)
                        .size(13)
                        .color(theme::MUTED)
                ]
                .spacing(7),
            )
            .width(Fill)
            .padding(24)
            .style(theme::panel)
        ]
        .spacing(10)
        .into();
    }
    let mut items = column![].spacing(8);
    for entry in app
        .content_entries
        .iter()
        .filter(|entry| content::name_matches_query(&entry.name, query))
    {
        let modified = format_timestamp(entry.modified_unix);
        let metadata = format!("{modified} // {}", format_bytes(entry.size));
        items = items.push(
            button(
                row![
                    media::thumbnail(
                        app.content_thumbnail(entry),
                        if entry.is_directory { "▣" } else { "◇" },
                    ),
                    column![
                        text(&entry.name).size(16),
                        text(metadata)
                            .font(theme::BODY_FONT)
                            .size(11)
                            .color(theme::MUTED)
                    ]
                    .spacing(2),
                    Space::new().width(Fill),
                    text(if kind == ContentKind::Mods {
                        "REVEAL  >"
                    } else {
                        "OPEN  >"
                    })
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::LAVENDER)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding(13)
            .on_press(if kind == ContentKind::Mods {
                Message::RevealPath(entry.path.clone())
            } else {
                Message::OpenPath(entry.path.clone())
            })
            .style(theme::ghost_button),
        );
    }
    if visible_count == 0 {
        let searching_mods = kind == ContentKind::Mods && !query.is_empty();
        let hint = if searching_mods {
            "Try another file name or clear the search field."
        } else if kind.downloadable() {
            "Use DOWNLOAD to browse compatible CurseForge files, or add files to this folder."
        } else {
            "Minecraft creates this folder when it first needs it."
        };
        items = items.push(
            container(
                column![
                    text(if searching_mods {
                        "NO MATCHING MODS".to_string()
                    } else {
                        format!("NO {} YET", app.instance_tab.label().to_uppercase())
                    })
                    .size(22)
                    .color(theme::MUTED),
                    text(hint)
                        .font(theme::BODY_FONT)
                        .size(13)
                        .color(theme::MUTED),
                    button(text("OPEN INSTANCE FOLDER").size(13))
                        .on_press(Message::OpenPath(instance.game_dir.clone()))
                        .style(theme::ghost_button)
                ]
                .spacing(10),
            )
            .width(Fill)
            .padding(24)
            .style(theme::panel),
        );
    }
    column![
        toolbar,
        scrollable(items.padding(padding::bottom(CONTENT_END_GAP)))
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
    ]
    .spacing(10)
    .height(Fill)
    .into()
}

fn instance_settings<'a>(app: &'a Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let memory_limit = app.system_resources.memory_limit_mb();
    let memory_value = instance.settings.max_memory_mb.clamp(512, memory_limit);
    let identity = section(
        "IDENTITY",
        column![
            labeled_input(
                "INSTANCE NAME",
                &instance.name,
                "My modded world",
                Message::EditInstanceName,
            ),
            labeled_input(
                "DESCRIPTION",
                &instance.description,
                "What makes this workspace special?",
                Message::EditInstanceDescription,
            ),
            column![
                text("MARKER COLOR")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
                media::instance_color_picker(instance.color, Message::EditInstanceColor)
            ]
            .spacing(6),
            checkbox(instance.favorite)
                .label("Pin as a favorite")
                .on_toggle(Message::ToggleInstanceFavorite)
                .size(20)
                .style(theme::square_checkbox)
        ]
        .spacing(12),
    );

    let manual_java: Element<'a, Message> = if instance.settings.auto_java {
        Space::new().height(0).into()
    } else if app.java_runtimes.is_empty() {
        text("No detected Java runtime is available for manual selection.")
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::WARNING)
            .into()
    } else {
        let mut choices = column![].spacing(7);
        for runtime in &app.java_runtimes {
            let selected = instance.settings.java_path.as_ref() == Some(&runtime.path);
            choices = choices.push(
                button(
                    row![
                        text(format!("JAVA {}", runtime.major)).size(13),
                        text(&runtime.vendor)
                            .font(theme::BODY_FONT)
                            .size(11)
                            .color(if selected {
                                theme::CANVAS
                            } else {
                                theme::MUTED
                            }),
                        Space::new().width(Fill),
                        text(if selected { "SELECTED" } else { "USE" })
                            .font(theme::BODY_BOLD)
                            .size(11)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(9)
                .on_press(Message::SetInstanceJava(runtime.path.clone()))
                .style(if selected {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
            );
        }
        choices.into()
    };

    let game = section(
        "GAME / JAVA",
        column![
            checkbox(instance.settings.isolated)
                .label("Keep this instance isolated")
                .on_toggle(Message::SetInstanceIsolation)
                .size(20)
                .style(theme::square_checkbox),
            checkbox(instance.settings.auto_java)
                .label("Automatically choose a compatible Java")
                .on_toggle(Message::SetInstanceAutoJava)
                .size(20)
                .style(theme::square_checkbox),
            manual_java,
            setting_slider(
                "WINDOW WIDTH",
                format!("{} PX", instance.settings.width),
                slider(
                    640..=3840,
                    instance.settings.width,
                    Message::SetInstanceWidth
                )
                .step(16_u32)
                .style(theme::square_slider),
            ),
            setting_slider(
                "WINDOW HEIGHT",
                format!("{} PX", instance.settings.height),
                slider(
                    360..=2160,
                    instance.settings.height,
                    Message::SetInstanceHeight
                )
                .step(9_u32)
                .style(theme::square_slider),
            ),
            checkbox(instance.settings.fullscreen)
                .label("Launch in fullscreen")
                .on_toggle(Message::SetInstanceFullscreen)
                .size(20)
                .style(theme::square_checkbox),
        ]
        .spacing(13),
    );

    let performance = section(
        "PERFORMANCE / WINDOW",
        column![
            checkbox(instance.settings.auto_memory)
                .label("Allocate memory automatically")
                .on_toggle(Message::SetInstanceAutoMemory)
                .size(20)
                .style(theme::square_checkbox),
            setting_slider(
                "MAXIMUM MEMORY",
                format!("{memory_value} MiB // {memory_limit} MiB AVAILABLE"),
                slider(512..=memory_limit, memory_value, Message::SetInstanceMemory)
                    .step(128_u32)
                    .style(theme::square_slider),
            ),
            text("AVAILABLE MEMORY REFRESHES EVERY 2 SECONDS")
                .font(theme::BODY_FONT)
                .size(11)
                .color(theme::MUTED),
            labeled_input(
                "CUSTOM WINDOW TITLE",
                &instance.settings.custom_window_title,
                "Leave blank for Minecraft",
                Message::SetInstanceWindowTitle,
            ),
            labeled_input(
                "CUSTOM INFORMATION",
                &instance.settings.custom_info,
                "Shown in compatible game screens",
                Message::SetInstanceCustomInfo,
            ),
            button(
                text(if app.is_instance_deleting(instance.id) {
                    "DELETING INSTANCE…"
                } else {
                    "DELETE INSTANCE DATA"
                })
                .size(13),
            )
            .on_press_maybe(
                (!app.is_instance_launching(instance.id) && !app.is_instance_deleting(instance.id))
                    .then_some(Message::DeleteInstance(instance.id))
            )
            .padding([10, 14])
            .style(theme::danger_window_button)
        ]
        .spacing(13),
    );

    scrollable(
        column![identity, game, performance]
            .spacing(14)
            .padding(padding::bottom(CONTENT_END_GAP)),
    )
    .width(Fill)
    .height(Fill)
    .spacing(SCROLLBAR_GAP)
    .style(theme::square_scrollable)
    .into()
}

fn install_job(job: &InstallJob) -> Element<'_, Message> {
    let progress = &job.progress;
    let steps = ["METADATA", "GAME", "LOADER", "CONTENT", "FINALIZE", "DONE"];
    let mut rail = row![].spacing(7).align_y(Alignment::Center);
    for (index, label) in steps.iter().enumerate() {
        let color = if progress.stage == InstallStage::Failed {
            theme::DANGER
        } else if progress.stage == InstallStage::Cancelled {
            theme::WARNING
        } else if index < progress.stage.ordinal() {
            theme::SUCCESS
        } else if index == progress.stage.ordinal() {
            theme::LAVENDER_SOFT
        } else {
            theme::MUTED
        };
        rail = rail.push(
            text(format!(
                "{} {label}",
                if index < progress.stage.ordinal() {
                    "●"
                } else {
                    "○"
                }
            ))
            .font(theme::BODY_BOLD)
            .size(12)
            .color(color),
        );
        if index + 1 < steps.len() {
            rail = rail.push(
                text("──")
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::BORDER),
            );
        }
    }
    let stats = if progress.files_total > 0 {
        format!(
            "{} / {} FILES  //  {}  //  {}/S",
            progress.files_done,
            progress.files_total,
            format_bytes(progress.current),
            format_bytes(progress.bytes_per_second as u64)
        )
    } else {
        progress.detail.clone()
    };
    let logs = job.logs.iter().rev().take(80).rev().fold(
        column![].spacing(3).width(Fill),
        |column, line| {
            column.push(
                text(line)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .width(Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(theme::MUTED),
            )
        },
    );
    let terminal_unsuccessful = matches!(
        progress.stage,
        InstallStage::Failed | InstallStage::Cancelled
    );
    let action: Element<'_, Message> = if terminal_unsuccessful {
        button(text("RETRY PIPELINE").size(14))
            .on_press(Message::RetryInstall(job.request.instance_id))
            .padding([11, 16])
            .style(theme::primary_button)
            .into()
    } else if job.active {
        button(text("CANCEL").size(14))
            .on_press(Message::CancelInstall(job.request.instance_id))
            .padding([11, 16])
            .style(theme::ghost_button)
            .into()
    } else {
        Space::new().height(0).into()
    };
    let open_log: Element<'_, Message> = if terminal_unsuccessful {
        job.log_path
            .as_ref()
            .map(|path| {
                button(text("OPEN INSTALL LOG").size(14))
                    .on_press(Message::OpenPath(path.clone()))
                    .padding([11, 16])
                    .style(theme::ghost_button)
                    .into()
            })
            .unwrap_or_else(|| Space::new().height(0).into())
    } else {
        Space::new().height(0).into()
    };

    column![
        row![
            column![
                text("INSTALL PIPELINE")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
                text(&job.request.name).size(30),
                text(format!(
                    "Minecraft {} // {}",
                    job.request.minecraft_version, job.request.loader.kind
                ))
                .font(theme::BODY_FONT)
                .size(13)
                .color(theme::LAVENDER)
            ]
            .spacing(4),
            Space::new().width(Fill),
            text(progress.stage.label()).size(18).color(
                if progress.stage == InstallStage::Failed {
                    theme::DANGER
                } else {
                    theme::SUCCESS
                }
            )
        ],
        container(
            column![
                rail,
                progress_bar(0.0..=1.0, progress.fraction())
                    .girth(9)
                    .style(theme::square_progress),
                text(stats)
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED)
            ]
            .spacing(13)
        )
        .padding(18)
        .style(theme::panel),
        container(
            scrollable(logs)
                .width(Fill)
                .height(Fill)
                .spacing(SCROLLBAR_GAP)
                .anchor_bottom()
                .style(theme::square_scrollable),
        )
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(theme::inset),
        row![action, open_log].spacing(10)
    ]
    .spacing(15)
    .width(Fill)
    .height(Fill)
    .padding(padding::bottom(CONTENT_END_GAP))
    .into()
}

fn launch_session(session: &LaunchSession) -> Element<'_, Message> {
    let color = if session.failed {
        theme::DANGER
    } else if session.ready {
        theme::SUCCESS
    } else {
        theme::LAVENDER
    };
    let logs = session.logs.iter().rev().take(100).rev().fold(
        column![].spacing(2).width(Fill),
        |column, line| {
            column.push(
                text(line)
                    .font(theme::BODY_FONT)
                    .size(11)
                    .width(Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(theme::MUTED),
            )
        },
    );
    let open: Element<'_, Message> = session
        .log_path
        .as_ref()
        .map(|path| {
            button(text("OPEN LOG FILE").size(12))
                .on_press(Message::OpenPath(path.clone()))
                .style(theme::ghost_button)
                .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    container(
        column![
            row![
                column![
                    text(if session.failed {
                        "LAUNCH ERROR"
                    } else {
                        "LAUNCH MONITOR"
                    })
                    .size(19),
                    text(&session.status)
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(color)
                ]
                .spacing(3),
                Space::new().width(Fill),
                text(if session.active { "LIVE" } else { "ENDED" })
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(color)
            ],
            container(
                scrollable(logs)
                    .width(Fill)
                    .height(Fill)
                    .spacing(SCROLLBAR_GAP)
                    .anchor_bottom()
                    .style(theme::square_scrollable),
            )
            .width(Fill)
            .height(Fill)
            .padding(12)
            .style(theme::inset),
            open
        ]
        .spacing(11)
        .width(Fill)
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .padding(17)
    .style(if session.failed {
        theme::danger_panel
    } else {
        theme::panel
    })
    .into()
}

fn section<'a>(title: &'a str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
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

fn labeled_input<'a>(
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

fn setting_slider<'a>(
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

fn info_card<'a>(
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

fn empty_state<'a>() -> iced::widget::Container<'a, Message> {
    container(
        column![
            text("NO INSTANCE SELECTED").size(28).color(theme::MUTED),
            text("Choose a workspace on the left or build a new one.")
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED),
            button(text("NEW INSTANCE  >").size(14))
                .on_press(Message::Navigate(Route::NewInstance))
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

fn format_loader(instance: &Instance) -> String {
    instance.loader.version.as_ref().map_or_else(
        || instance.loader.kind.to_string(),
        |version| format!("{} {version}", instance.loader.kind),
    )
}

fn format_duration(seconds: u64) -> String {
    format!("{:02}H {:02}M", seconds / 3600, (seconds % 3600) / 60)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn format_timestamp(timestamp: u64) -> String {
    let time = UNIX_EPOCH + Duration::from_secs(timestamp);
    let local: DateTime<Local> = time.into();
    local.format("%Y.%m.%d %H:%M").to_string()
}
