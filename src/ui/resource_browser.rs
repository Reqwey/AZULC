use crate::{
    app::{Launcher, Message, ResourceBrowserState},
    services::{
        catalog::{self, CatalogProject, CatalogProvider},
        content::ContentKind,
    },
    theme,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    Space, button, column, container, opaque, row, rule, scrollable, text, text_input,
};
use iced::{Alignment, Element, Fill, Length, alignment};

use super::{SCROLLBAR_GAP, media};

pub(crate) fn view(app: &Launcher) -> Element<'_, Message> {
    let Some(browser) = app.resource_browser.as_ref() else {
        return Space::new().into();
    };

    let header = row![
        column![
            text(format!("DOWNLOAD {}", resource_label(browser.kind))).size(27),
            text(format!(
                "{} catalog // Minecraft {}",
                browser.provider.label(),
                instance_version(app, browser)
            ))
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::MUTED)
        ]
        .spacing(2),
        Space::new().width(Fill),
        button(text("CLOSE  ×").size(13))
            .on_press(Message::CloseResourceBrowser)
            .padding([9, 13])
            .style(theme::ghost_button)
    ]
    .align_y(Alignment::Center);

    let provider_tabs = catalog_provider_tabs(browser.provider);
    let credential: Element<'_, Message> = if let Some(credential) =
        catalog::missing_credential(browser.provider)
    {
        container(
            column![
                text("CURSEFORGE KEY REQUIRED")
                    .font(theme::BODY_BOLD)
                    .size(13)
                    .color(theme::WARNING),
                text(format!(
                    "Set {} before starting AZULC. The key is never written to launcher data or logs.",
                    credential
                ))
                .font(theme::BODY_FONT)
                .size(11)
                .color(theme::TEXT)
            ]
            .spacing(4),
        )
        .width(Fill)
        .padding(11)
        .style(theme::inset)
        .into()
    } else {
        Space::new().height(0).into()
    };

    let body = if let Some(project) = &browser.selected_project {
        project_files(app, browser, project)
    } else {
        project_search(app, browser)
    };

    let feedback: Element<'_, Message> = if let Some(error) = &browser.error {
        container(
            text(error)
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::DANGER),
        )
        .width(Fill)
        .padding(10)
        .style(theme::danger_panel)
        .into()
    } else if let Some(status) = &browser.status {
        container(
            text(status)
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::SUCCESS),
        )
        .width(Fill)
        .padding(10)
        .style(theme::inset)
        .into()
    } else {
        Space::new().height(0).into()
    };

    let panel = opaque(
        container(
            column![
                header,
                rule::horizontal(1),
                provider_tabs,
                credential,
                body,
                feedback
            ]
            .spacing(12),
        )
        .width(Fill)
        .height(Fill)
        .max_width(1010)
        .max_height(720)
        .padding(22)
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

fn catalog_provider_tabs(selected: CatalogProvider) -> Element<'static, Message> {
    let mut tabs = row![
        text("DOWNLOAD SOURCE")
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED)
    ]
    .spacing(7)
    .align_y(Alignment::Center);
    for provider in CatalogProvider::ALL {
        tabs = tabs.push(
            button(text(provider.label().to_uppercase()).size(13))
                .on_press(Message::ResourceProviderPicked(provider))
                .padding([8, 12])
                .style(if selected == provider {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
        );
    }
    tabs.into()
}

fn project_search<'a>(
    app: &'a Launcher,
    browser: &'a ResourceBrowserState,
) -> Element<'a, Message> {
    let controls = row![
        text_input("Search by project name…", &browser.query)
            .on_input(Message::ResourceQueryChanged)
            .on_submit(Message::SearchResources)
            .padding(11)
            .size(13)
            .style(theme::square_text_input),
        button(text("SEARCH").size(14))
            .on_press(Message::SearchResources)
            .padding([11, 18])
            .style(theme::primary_button)
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut projects = column![].spacing(7);
    if browser.loading {
        projects = projects.push(
            container(
                text(format!(
                    "READING {} CATALOG…",
                    browser.provider.label().to_uppercase()
                ))
                .size(18)
                .color(theme::LAVENDER),
            )
            .width(Fill)
            .padding(22)
            .style(theme::inset),
        );
    } else {
        for project in &browser.projects {
            let restricted = !project.available;
            projects = projects.push(
                button(
                    row![
                        media::thumbnail(app.catalog_thumbnail(project), "◇"),
                        column![
                            row![
                                text(&project.name)
                                    .size(17)
                                    .width(Fill)
                                    .wrapping(Wrapping::WordOrGlyph),
                                text(if restricted { "RESTRICTED" } else { "" })
                                    .font(theme::BODY_BOLD)
                                    .size(10)
                                    .color(theme::DANGER)
                            ]
                            .spacing(8)
                            .width(Fill),
                            text(&project.summary)
                                .font(theme::BODY_FONT)
                                .size(11)
                                .width(Fill)
                                .wrapping(Wrapping::WordOrGlyph)
                                .color(theme::MUTED),
                            text(format!(
                                "{} downloads // {} // {}",
                                compact_number(project.download_count),
                                project.author,
                                short_date(&project.date_modified)
                            ))
                            .font(theme::BODY_FONT)
                            .size(11)
                            .width(Fill)
                            .wrapping(Wrapping::WordOrGlyph)
                            .color(theme::LAVENDER_SOFT)
                        ]
                        .spacing(3)
                        .width(Fill),
                        text("FILES  >")
                            .font(theme::BODY_BOLD)
                            .size(12)
                            .width(Length::Fixed(82.0))
                            .align_x(alignment::Horizontal::Right)
                            .color(theme::LAVENDER)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(12)
                .on_press(Message::ResourceProjectPicked(project.clone()))
                .style(theme::ghost_button),
            );
        }
        if browser.projects.is_empty() && browser.error.is_none() {
            projects = projects.push(
                container(
                    column![
                        text("NO MATCHING PROJECTS").size(19),
                        text("Try a shorter name or a different Minecraft instance.")
                            .font(theme::BODY_FONT)
                            .size(12)
                            .color(theme::MUTED)
                    ]
                    .spacing(5),
                )
                .width(Fill)
                .padding(22)
                .style(theme::inset),
            );
        }
    }

    column![
        controls,
        text(format!("{} PROJECTS", browser.projects.len()))
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED),
        scrollable(projects)
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
    ]
    .spacing(9)
    .height(Fill)
    .into()
}

fn project_files<'a>(
    app: &'a Launcher,
    browser: &'a ResourceBrowserState,
    project: &'a CatalogProject,
) -> Element<'a, Message> {
    let top = row![
        button(text("<  PROJECTS").size(12))
            .on_press(Message::ResourceBackToProjects)
            .padding([8, 11])
            .style(theme::ghost_button),
        media::thumbnail(app.catalog_thumbnail(project), "◇"),
        column![
            text(&project.name)
                .size(21)
                .width(Fill)
                .wrapping(Wrapping::WordOrGlyph),
            text(&project.summary)
                .font(theme::BODY_FONT)
                .size(11)
                .width(Fill)
                .wrapping(Wrapping::WordOrGlyph)
                .color(theme::MUTED)
        ]
        .spacing(2)
        .width(Fill),
        text(format!("{} FILES", browser.files.len()))
            .font(theme::BODY_BOLD)
            .size(12)
            .width(Length::Fixed(92.0))
            .align_x(alignment::Horizontal::Right)
            .color(theme::LAVENDER)
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let restricted = !project.available;
    let mut files = column![].spacing(7);
    if browser.loading {
        files = files.push(
            container(
                text("LOADING COMPATIBLE FILES…")
                    .size(18)
                    .color(theme::LAVENDER),
            )
            .width(Fill)
            .padding(22)
            .style(theme::inset),
        );
    } else {
        for file in &browser.files {
            let available = !browser.downloading && !restricted && file.available;
            files = files.push(
                container(
                    row![
                        column![
                            row![
                                text(&file.display_name)
                                    .size(16)
                                    .width(Fill)
                                    .wrapping(Wrapping::WordOrGlyph),
                                text(release_label(file.release_type))
                                    .font(theme::BODY_BOLD)
                                    .size(10)
                                    .color(release_color(file.release_type))
                            ]
                            .spacing(8)
                            .width(Fill),
                            text(format!(
                                "{} // {} // {}",
                                short_date(&file.file_date),
                                format_bytes(file.file_length),
                                file.game_versions
                                    .iter()
                                    .take(4)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(" · ")
                            ))
                            .font(theme::BODY_FONT)
                            .size(11)
                            .width(Fill)
                            .wrapping(Wrapping::WordOrGlyph)
                            .color(theme::MUTED)
                        ]
                        .spacing(3)
                        .width(Fill),
                        button(text(if restricted { "BLOCKED" } else { "DOWNLOAD" }).size(12))
                            .on_press_maybe(
                                available.then_some(Message::ResourceFilePicked(file.clone()))
                            )
                            .padding([9, 13])
                            .width(Length::Fixed(116.0))
                            .style(theme::primary_button)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(11)
                .style(theme::inset),
            );
        }
        if browser.files.is_empty() && browser.error.is_none() {
            files = files.push(
                container(text("NO COMPATIBLE FILES FOR THIS INSTANCE").size(18))
                    .width(Fill)
                    .padding(22)
                    .style(theme::inset),
            );
        }
    }

    column![
        top,
        scrollable(files)
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
    ]
    .spacing(10)
    .height(Fill)
    .into()
}

fn instance_version<'a>(app: &'a Launcher, browser: &ResourceBrowserState) -> &'a str {
    app.persisted
        .instances
        .iter()
        .find(|instance| instance.id == browser.instance_id)
        .map_or("unknown", |instance| instance.minecraft_version.as_str())
}

fn resource_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Mods => "MODS",
        ContentKind::ResourcePacks => "RESOURCE PACKS",
        ContentKind::ShaderPacks => "SHADERS",
        ContentKind::DataPacks => "DATA PACKS",
        ContentKind::Worlds => "WORLDS",
        ContentKind::Screenshots => "SCREENSHOTS",
    }
}

fn release_label(release_type: u8) -> &'static str {
    match release_type {
        1 => "RELEASE",
        2 => "BETA",
        3 => "ALPHA",
        _ => "BUILD",
    }
}

fn release_color(release_type: u8) -> iced::Color {
    match release_type {
        1 => theme::SUCCESS,
        2 => theme::WARNING,
        _ => theme::MUTED,
    }
}

fn short_date(value: &str) -> &str {
    value.split('T').next().unwrap_or(value)
}

fn compact_number(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
