use crate::{
    app::{
        Launcher, Message, ModpackBrowserState,
        catalog::{CatalogProject, CatalogProvider},
        navigation::ModpackTab,
    },
    services::{modpack::ModpackFormat, providers::curseforge},
    theme,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    Space, button, column, container, rich_text, row, rule, scrollable, span, text, text_input,
};
use iced::{Alignment, Color, Element, Fill, Length, alignment};

use super::{SCROLLBAR_GAP, media};

pub(crate) fn view(app: &Launcher) -> Element<'_, Message> {
    let header = row![
        column![
            text("MODPACK DEPOT").size(30),
            text("CURSEFORGE + MODRINTH // ONE CONTINUOUS INSTALL PIPELINE")
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::LAVENDER)
        ]
        .spacing(2),
        Space::new().width(Fill),
        section_tabs(app)
    ]
    .align_y(Alignment::End);

    let body = match app.modpack_tab {
        ModpackTab::Browse => browse(app),
        ModpackTab::Import => import_file(app),
    };

    column![header, rule::horizontal(1), body]
        .spacing(14)
        .width(Fill)
        .height(Fill)
        .into()
}

fn section_tabs(app: &Launcher) -> Element<'_, Message> {
    let mut tabs = row![].spacing(7);
    for tab in ModpackTab::ALL {
        tabs = tabs.push(
            button(text(tab.label().to_uppercase()).size(12))
                .on_press(Message::ModpackTabSelected(tab))
                .padding([10, 14])
                .style(if app.modpack_tab == tab {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
        );
    }
    tabs.into()
}

fn browse(app: &Launcher) -> Element<'_, Message> {
    let provider_tabs = catalog_provider_tabs(app.modpacks.provider);
    let credential: Element<'_, Message> = if app.modpacks.provider == CatalogProvider::CurseForge
        && std::env::var_os(curseforge::API_KEY_ENV).is_none()
    {
        container(
                row![
                    text("!").size(18).color(theme::WARNING),
                    column![
                        text("CURSEFORGE KEY REQUIRED")
                            .font(theme::BODY_BOLD)
                            .size(12)
                            .color(theme::WARNING),
                        text(format!(
                            "Set {} before starting AZULC. Credentials are never stored in launcher data.",
                            curseforge::API_KEY_ENV
                        ))
                        .font(theme::BODY_FONT)
                        .size(11)
                        .color(theme::TEXT)
                    ]
                    .spacing(2)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding(11)
            .style(theme::inset)
            .into()
    } else {
        Space::new().height(0).into()
    };

    let catalog = if let Some(project) = &app.modpacks.selected_project {
        project_files(app, &app.modpacks, project)
    } else {
        project_search(app, &app.modpacks)
    };

    let feedback: Element<'_, Message> = if let Some(error) = &app.modpacks.error {
        container(
            text(error)
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::DANGER),
        )
        .width(Fill)
        .padding(11)
        .style(theme::danger_panel)
        .into()
    } else {
        Space::new().height(0).into()
    };

    container(column![provider_tabs, credential, catalog, feedback].spacing(10))
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(theme::panel)
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
                .on_press(Message::ModpackProviderPicked(provider))
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

fn project_search<'a>(app: &'a Launcher, browser: &'a ModpackBrowserState) -> Element<'a, Message> {
    let controls = row![
        column![
            text("FIND A PACK")
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::MUTED),
            text_input("Search modpacks by name…", &browser.query)
                .on_input(Message::ModpackQueryChanged)
                .on_submit(Message::SearchModpacks)
                .padding(11)
                .size(13)
                .width(Length::Fixed(460.0))
                .style(theme::square_text_input)
        ]
        .spacing(5),
        button(text("SEARCH").size(14))
            .on_press(Message::SearchModpacks)
            .padding([11, 20])
            .style(theme::primary_button),
        Space::new().width(Fill),
        column![
            text(format!("{:02}", browser.projects.len())).size(23),
            text("RESULTS")
                .font(theme::BODY_BOLD)
                .size(10)
                .color(theme::MUTED)
        ]
        .spacing(0)
        .align_x(alignment::Horizontal::Right)
    ]
    .spacing(10)
    .align_y(Alignment::End);

    let mut projects = column![].spacing(7);
    if browser.loading {
        projects = projects.push(
            container(
                column![
                    text(format!(
                        "READING {} CATALOG…",
                        browser.provider.label().to_uppercase()
                    ))
                    .size(19)
                    .color(theme::LAVENDER),
                    text("Popular modpacks are being indexed.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(4),
            )
            .width(Fill)
            .padding(24)
            .style(theme::inset),
        );
    } else {
        for project in &browser.projects {
            let category = project
                .categories
                .iter()
                .take(3)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" · ");
            let available = project.available;
            projects = projects.push(
                button(
                    row![
                        media::thumbnail(app.catalog_thumbnail(project), "▦"),
                        column![
                            rich_text([
                                span(project.name.as_str()).size(18),
                                span(format!(
                                    "  {}",
                                    if available {
                                        browser.provider.label().to_uppercase()
                                    } else {
                                        "RESTRICTED".to_string()
                                    }
                                ))
                                .font(theme::BODY_BOLD)
                                .size(10)
                                .color(if available {
                                    theme::LAVENDER
                                } else {
                                    theme::DANGER
                                }),
                            ])
                            .on_link_click(iced::never)
                            .width(Fill)
                            .wrapping(Wrapping::WordOrGlyph),
                            text(&project.summary)
                                .font(theme::BODY_FONT)
                                .size(11)
                                .width(Fill)
                                .wrapping(Wrapping::WordOrGlyph)
                                .color(theme::MUTED),
                            text(format!(
                                "{} DOWNLOADS  //  {}  //  {}",
                                compact_number(project.download_count),
                                project.author,
                                if category.is_empty() {
                                    "UNCATEGORIZED"
                                } else {
                                    &category
                                }
                            ))
                            .font(theme::BODY_BOLD)
                            .size(10)
                            .width(Fill)
                            .wrapping(Wrapping::WordOrGlyph)
                            .color(theme::LAVENDER_SOFT)
                        ]
                        .spacing(4)
                        .width(Fill),
                        column![
                            text(short_date(&project.date_modified))
                                .font(theme::BODY_FONT)
                                .size(11)
                                .wrapping(Wrapping::None)
                                .color(theme::MUTED),
                            text("RELEASES  >").font(theme::BODY_BOLD).size(12).color(
                                if available {
                                    theme::LAVENDER
                                } else {
                                    theme::MUTED
                                }
                            )
                        ]
                        .spacing(7)
                        .width(Length::Fixed(132.0))
                        .align_x(alignment::Horizontal::Right)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(12)
                .on_press_maybe(available.then(|| Message::ModpackProjectPicked(project.clone())))
                .style(theme::ghost_button),
            );
        }

        if browser.projects.is_empty() && browser.error.is_none() {
            projects = projects.push(empty_catalog(
                "NO MODPACKS IN VIEW",
                "Search by title, or press Search with an empty field to load popular packs.",
            ));
        }
    }

    column![
        controls,
        rule::horizontal(1),
        scrollable(projects)
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
    ]
    .spacing(11)
    .height(Fill)
    .into()
}

fn project_files<'a>(
    app: &'a Launcher,
    browser: &'a ModpackBrowserState,
    project: &'a CatalogProject,
) -> Element<'a, Message> {
    let top = row![
        button(text("<  ALL MODPACKS").size(13))
            .on_press(Message::ModpackBackToProjects)
            .padding([9, 12])
            .style(theme::ghost_button),
        media::thumbnail(app.catalog_thumbnail(project), "▦"),
        column![
            text(&project.name)
                .size(23)
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
        column![
            text(format!("{:02}", browser.files.len())).size(23),
            text("RELEASES")
                .font(theme::BODY_BOLD)
                .size(10)
                .color(theme::MUTED)
        ]
        .width(Length::Fixed(90.0))
        .align_x(alignment::Horizontal::Right)
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let mut files = column![].spacing(7);
    if browser.loading {
        files = files.push(
            container(
                text("LOADING MODPACK RELEASES…")
                    .size(19)
                    .color(theme::LAVENDER),
            )
            .width(Fill)
            .padding(24)
            .style(theme::inset),
        );
    } else {
        for file in &browser.files {
            let installable = file.available;
            let game_versions = display_game_versions(&file.game_versions);
            files = files.push(
                container(
                    row![
                        container(text(release_glyph(file.release_type)).size(20))
                            .width(38)
                            .height(38)
                            .align_x(alignment::Horizontal::Center)
                            .align_y(alignment::Vertical::Center)
                            .style(theme::inset),
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
                            .width(Fill)
                            .align_y(Alignment::Center),
                            text(&file.file_name)
                                .font(theme::BODY_FONT)
                                .size(11)
                                .width(Fill)
                                .wrapping(Wrapping::WordOrGlyph)
                                .color(theme::MUTED),
                            text(format!(
                                "MC {}  //  {}  //  {} DOWNLOADS",
                                game_versions,
                                format_bytes(file.file_length),
                                compact_number(file.download_count)
                            ))
                            .font(theme::BODY_BOLD)
                            .size(10)
                            .width(Fill)
                            .wrapping(Wrapping::WordOrGlyph)
                            .color(theme::LAVENDER_SOFT)
                        ]
                        .spacing(3)
                        .width(Fill),
                        column![
                            text(short_date(&file.file_date))
                                .font(theme::BODY_FONT)
                                .size(11)
                                .wrapping(Wrapping::None)
                                .color(theme::MUTED),
                            button(
                                text(if installable {
                                    "INSTALL  >"
                                } else {
                                    "UNAVAILABLE"
                                })
                                .size(12)
                            )
                            .on_press_maybe(
                                installable.then(|| Message::ModpackFilePicked(file.clone()))
                            )
                            .padding([9, 14])
                            .style(if installable {
                                theme::primary_button
                            } else {
                                theme::ghost_button
                            })
                        ]
                        .spacing(5)
                        .width(Length::Fixed(145.0))
                        .align_x(alignment::Horizontal::Right)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(11)
                .style(theme::panel),
            );
        }
        if browser.files.is_empty() && browser.error.is_none() {
            files = files.push(empty_catalog(
                "NO INSTALLABLE RELEASES",
                "The selected provider did not return a client pack for this project.",
            ));
        }
    }

    column![
        top,
        rule::horizontal(1),
        scrollable(files)
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
    ]
    .spacing(11)
    .height(Fill)
    .into()
}

fn import_file(app: &Launcher) -> Element<'_, Message> {
    let picker = container(
        row![
            column![
                text("IMPORT AN EXISTING PACK").size(22),
                text("CURSEFORGE ZIP  //  MODRINTH MRPACK  //  MULTIMC ZIP")
                    .font(theme::BODY_BOLD)
                    .size(10)
                    .color(theme::LAVENDER),
                text(
                    "AZULC reads the manifest first, then installs Minecraft, the loader, pack files, and overrides as one tracked job.",
                )
                .font(theme::BODY_FONT)
                .size(11)
                .color(theme::MUTED)
            ]
            .spacing(5),
            Space::new().width(Fill),
            button(text("CHOOSE ARCHIVE…").size(13))
                .on_press(Message::ChooseLocalModpack)
                .padding([11, 16])
                .style(theme::primary_button)
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding(20)
    .style(theme::selected_card);

    let selected: Element<'_, Message> = if app.modpacks.local_loading {
        container(
            column![
                text("INSPECTING ARCHIVE…")
                    .size(20)
                    .color(theme::LAVENDER_SOFT),
                text(path_label(app.modpacks.local_path.as_deref()))
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::MUTED),
                text("Validating manifest paths, file limits, Minecraft version, and loader metadata.")
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::MUTED)
            ]
            .spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(theme::inset)
        .into()
    } else if let Some(plan) = &app.modpacks.local_plan {
        let metadata = &plan.metadata;
        let loader_version = metadata
            .loader
            .version
            .as_deref()
            .unwrap_or("manifest default");
        container(
            column![
                row![
                    column![
                        text("MANIFEST READY")
                            .font(theme::BODY_BOLD)
                            .size(11)
                            .color(theme::SUCCESS),
                        text(&metadata.name).size(27),
                        text(path_label(app.modpacks.local_path.as_deref()))
                            .font(theme::BODY_FONT)
                            .size(11)
                            .color(theme::MUTED)
                    ]
                    .spacing(3),
                    Space::new().width(Fill),
                    text(format_label(plan.format))
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(theme::LAVENDER)
                ]
                .align_y(Alignment::Center),
                rule::horizontal(1),
                row![
                    manifest_stat("MINECRAFT", metadata.minecraft_version.clone()),
                    manifest_stat("LOADER", metadata.loader.kind.label().to_string()),
                    manifest_stat("LOADER BUILD", loader_version.to_string()),
                    manifest_stat("PACK FILES", plan.files.len().to_string()),
                    manifest_stat(
                        "VERSION",
                        metadata
                            .version
                            .clone()
                            .unwrap_or_else(|| "unspecified".into())
                    )
                ]
                .spacing(10),
                row![
                    column![
                        text("AUTHOR")
                            .font(theme::BODY_BOLD)
                            .size(10)
                            .color(theme::MUTED),
                        text(metadata.author.as_deref().unwrap_or("Not provided"))
                            .font(theme::BODY_FONT)
                            .size(12)
                    ]
                    .spacing(3),
                    Space::new().width(Fill),
                    button(text("CHOOSE ANOTHER").size(13))
                        .on_press(Message::ChooseLocalModpack)
                        .padding([9, 12])
                        .style(theme::ghost_button),
                    button(text("INSTALL PACK  >").size(13))
                        .on_press(Message::InstallLocalModpack)
                        .padding([11, 18])
                        .style(theme::primary_button)
                ]
                .align_y(Alignment::Center)
                .spacing(9)
            ]
            .spacing(17),
        )
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(theme::panel)
        .into()
    } else {
        container(
            column![
                text("NO ARCHIVE SELECTED").size(21).color(theme::MUTED),
                text("Choose a supported local pack to inspect its contents before installation.")
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::MUTED)
            ]
            .spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .padding(24)
        .style(theme::inset)
        .into()
    };

    let feedback: Element<'_, Message> = if let Some(error) = &app.modpacks.error {
        container(
            column![
                text("ARCHIVE COULD NOT BE READ")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::DANGER),
                text(error)
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::TEXT)
            ]
            .spacing(3),
        )
        .width(Fill)
        .padding(11)
        .style(theme::danger_panel)
        .into()
    } else {
        Space::new().height(0).into()
    };

    column![picker, selected, feedback]
        .spacing(12)
        .height(Fill)
        .into()
}

fn manifest_stat(label: &'static str, value: String) -> Element<'static, Message> {
    container(
        column![
            text(label)
                .font(theme::BODY_BOLD)
                .size(10)
                .color(theme::MUTED),
            text(value).size(15).color(theme::LAVENDER_SOFT)
        ]
        .spacing(4),
    )
    .width(Fill)
    .padding(12)
    .style(theme::inset)
    .into()
}

fn empty_catalog<'a>(title: &'a str, detail: &'a str) -> iced::widget::Container<'a, Message> {
    container(
        column![
            text(title).size(20).color(theme::MUTED),
            text(detail)
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED)
        ]
        .spacing(5),
    )
    .width(Fill)
    .padding(24)
    .style(theme::inset)
}

fn path_label(path: Option<&std::path::Path>) -> String {
    path.map_or_else(
        || "No archive selected".into(),
        |path| path.display().to_string(),
    )
}

fn format_label(format: ModpackFormat) -> &'static str {
    match format {
        ModpackFormat::CurseForge => "CURSEFORGE",
        ModpackFormat::Modrinth => "MODRINTH",
        ModpackFormat::MultiMc => "MULTIMC",
    }
}

fn display_game_versions(versions: &[String]) -> String {
    let displayed = versions
        .iter()
        .filter(|version| {
            version
                .chars()
                .next()
                .is_some_and(|char| char.is_ascii_digit())
        })
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>();
    if displayed.is_empty() {
        "unspecified".into()
    } else {
        displayed.join(", ")
    }
}

fn short_date(value: &str) -> &str {
    value.split('T').next().unwrap_or("unknown date")
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

fn format_bytes(value: u64) -> String {
    if value >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", value as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if value >= 1024 * 1024 {
        format!("{:.1} MiB", value as f64 / (1024.0 * 1024.0))
    } else if value >= 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else {
        format!("{value} B")
    }
}

fn release_label(kind: u8) -> &'static str {
    match kind {
        1 => "RELEASE",
        2 => "BETA",
        3 => "ALPHA",
        _ => "BUILD",
    }
}

fn release_glyph(kind: u8) -> &'static str {
    match kind {
        1 => "◆",
        2 => "◇",
        3 => "△",
        _ => "·",
    }
}

fn release_color(kind: u8) -> Color {
    match kind {
        1 => theme::SUCCESS,
        2 => theme::WARNING,
        3 => theme::DANGER,
        _ => theme::MUTED,
    }
}
