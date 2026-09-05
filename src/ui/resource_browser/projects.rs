use crate::{
    app::{Launcher, Message, ResourceBrowserState},
    theme,
    ui::components::{SCROLLBAR_GAP, media},
};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, Length, alignment};

use crate::ui::components::catalog::{compact_number, short_date};

pub(super) fn view<'a>(
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
