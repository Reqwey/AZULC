use crate::{
    app::{Launcher, Message, ModpackBrowserState},
    services::catalog,
    theme,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    Space, button, column, container, rich_text, row, rule, scrollable, span, text, text_input,
};
use iced::{Alignment, Element, Fill, Length, alignment};

use super::{components::empty_catalog, files};
use crate::ui::components::{
    SCROLLBAR_GAP,
    catalog::{compact_number, provider_tabs, short_date},
    media,
};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let source_tabs = provider_tabs(app.modpacks.provider, Message::ModpackProviderPicked);
    let credential: Element<'_, Message> = if let Some(credential) =
        catalog::missing_credential(app.modpacks.provider)
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
                        credential
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
        files::view(app, &app.modpacks, project)
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

    container(column![source_tabs, credential, catalog, feedback].spacing(10))
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(theme::panel)
        .into()
}

fn project_search<'a>(app: &'a Launcher, browser: &'a ModpackBrowserState) -> Element<'a, Message> {
    let controls = row![
        text_input("Search modpacks by name…", &browser.query)
            .on_input(Message::ModpackQueryChanged)
            .on_submit(Message::SearchModpacks)
            .padding(11)
            .size(13)
            .width(Length::Fixed(460.0))
            .style(theme::square_text_input),
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
