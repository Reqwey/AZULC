use crate::{
    app::{Launcher, Message, navigation::InstanceTab},
    domain::Instance,
    services::content::{self as content_service, ContentKind},
    theme,
};
use chrono::{DateTime, Local};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length, padding};
use std::time::{Duration, UNIX_EPOCH};

use super::super::components::{CONTENT_END_GAP, SCROLLBAR_GAP, media};
use super::components::format_bytes;

pub(super) fn view<'a>(
    app: &'a Launcher,
    instance: &'a Instance,
    active_tab: InstanceTab,
) -> Element<'a, Message> {
    let kind = active_tab
        .content_kind()
        .expect("content list is only rendered for content tabs");
    let folder = instance.game_dir.join(
        kind.directory()
            .expect("every instance content kind has a directory"),
    );
    let download: Element<'a, Message> = if kind.downloadable() {
        button(text("DOWNLOAD").size(13))
            .on_press(Message::OpenResourceBrowser(instance.id, kind))
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
        .filter(|entry| content_service::name_matches_query(&entry.name, query))
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
            text(active_tab.label().to_uppercase()).size(21),
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
        .filter(|entry| content_service::name_matches_query(&entry.name, query))
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
                        format!("NO {} YET", active_tab.label().to_uppercase())
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

fn format_timestamp(timestamp: u64) -> String {
    let time = UNIX_EPOCH + Duration::from_secs(timestamp);
    let local: DateTime<Local> = time.into();
    local.format("%Y.%m.%d %H:%M").to_string()
}
