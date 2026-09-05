use crate::{
    app::{Launcher, Message, ResourceBrowserState},
    services::catalog::CatalogProject,
    theme,
    ui::components::{SCROLLBAR_GAP, media},
};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill, Length, alignment};

use super::presentation::release_color;
use crate::ui::components::catalog::{format_bytes, release_label, short_date};

pub(super) fn view<'a>(
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
