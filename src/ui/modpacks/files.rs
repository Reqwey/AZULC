use crate::{
    app::{Launcher, Message, ModpackBrowserState},
    services::catalog::CatalogProject,
    theme,
};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Fill, Length, alignment};

use super::{
    components::empty_catalog,
    presentation::{display_game_versions, release_color, release_glyph},
};
use crate::ui::components::{
    SCROLLBAR_GAP,
    catalog::{compact_number, format_bytes, release_label, short_date},
    media,
};

pub(super) fn view<'a>(
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
