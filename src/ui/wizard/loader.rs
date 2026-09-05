use crate::{
    app::{Launcher, Message},
    domain::LoaderKind,
    theme,
};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill, alignment};

use super::presentation::loader_copy;
use crate::ui::components::{SCROLLBAR_GAP, media};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let mut choices = column![].spacing(10);
    for loader in LoaderKind::ALL {
        let selected = app.wizard.loader == loader;
        let detail = loader_copy(loader);
        choices = choices.push(
            button(
                row![
                    container(media::loader_icon(loader, 32.0))
                        .width(42)
                        .height(42)
                        .align_x(alignment::Horizontal::Center)
                        .align_y(alignment::Vertical::Center),
                    column![
                        text(loader.label()).size(19),
                        text(detail)
                            .font(theme::BODY_FONT)
                            .size(12)
                            .color(if selected {
                                theme::CANVAS
                            } else {
                                theme::MUTED
                            })
                    ]
                    .spacing(3),
                    Space::new().width(Fill),
                    text(if selected { "SELECTED" } else { ">" })
                        .font(theme::BODY_BOLD)
                        .size(12)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding(16)
            .on_press(Message::LoaderPicked(loader))
            .style(if selected {
                theme::primary_button
            } else {
                theme::ghost_button
            }),
        );
    }

    let provider = app.loader_catalog.provider.unwrap_or("WAITING FOR CATALOG");
    let catalog_header = row![
        column![
            text("LOADER BUILD")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER),
            text(app.wizard.loader.label()).size(28),
            text(format!(
                "Minecraft {}",
                app.wizard
                    .selected_version
                    .as_deref()
                    .unwrap_or("not selected")
            ))
            .font(theme::BODY_FONT)
            .size(13)
            .color(theme::MUTED)
        ]
        .spacing(3),
        Space::new().width(Fill),
        column![
            text(format!("{} BUILDS", app.loader_catalog.entries.len()))
                .font(theme::BODY_BOLD)
                .size(13)
                .color(theme::TEXT),
            text(provider)
                .font(theme::BODY_FONT)
                .size(11)
                .color(theme::MUTED)
        ]
        .spacing(2)
        .align_x(alignment::Horizontal::Right)
    ]
    .align_y(Alignment::Center);

    let catalog_body: Element<'_, Message> = if app.wizard.loader == LoaderKind::Vanilla {
        container(
            column![
                text("NO LOADER PACKAGE REQUIRED").size(19),
                text("The base Minecraft profile will be installed without a mod-loader layer.")
                    .font(theme::BODY_FONT)
                    .size(13)
                    .color(theme::MUTED)
            ]
            .spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .padding(20)
        .style(theme::inset)
        .into()
    } else if app.loader_catalog.loading {
        container(
            column![
                text("FETCHING COMPATIBLE BUILDS…")
                    .size(19)
                    .color(theme::LAVENDER_SOFT),
                text("Reading the loader catalog for the selected Minecraft version.")
                    .font(theme::BODY_FONT)
                    .size(13)
                    .color(theme::MUTED)
            ]
            .spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .padding(20)
        .style(theme::inset)
        .into()
    } else if let Some(error) = &app.loader_catalog.error {
        container(
            column![
                text("CATALOG REQUEST FAILED").size(19).color(theme::DANGER),
                text(error)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::TEXT),
                button(text("RETRY CATALOG").size(13))
                    .on_press(Message::RetryLoaderCatalog)
                    .padding([9, 13])
                    .style(theme::ghost_button)
            ]
            .spacing(10),
        )
        .width(Fill)
        .height(Fill)
        .padding(20)
        .style(theme::danger_panel)
        .into()
    } else {
        let mut builds = column![].spacing(6);
        for entry in &app.loader_catalog.entries {
            let selected = app.wizard.loader_version == entry.install_version;
            let metadata = if !entry.description.is_empty() {
                entry
                    .description
                    .split('T')
                    .next()
                    .unwrap_or(&entry.description)
                    .to_string()
            } else if let Some(branch) = &entry.branch {
                format!("branch {branch}")
            } else {
                "compatible build".to_string()
            };
            builds = builds.push(
                button(
                    row![
                        text(if selected { "●" } else { "○" })
                            .size(17)
                            .color(if selected {
                                theme::CANVAS
                            } else {
                                theme::MUTED
                            }),
                        column![
                            text(&entry.version).size(16),
                            text(metadata)
                                .font(theme::BODY_FONT)
                                .size(11)
                                .color(if selected {
                                    theme::CANVAS
                                } else {
                                    theme::MUTED
                                })
                        ]
                        .spacing(1),
                        Space::new().width(Fill),
                        text(if entry.stable { "STABLE" } else { "TEST" })
                            .font(theme::BODY_BOLD)
                            .size(11)
                            .color(if selected {
                                theme::CANVAS
                            } else if entry.stable {
                                theme::SUCCESS
                            } else {
                                theme::WARNING
                            })
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([10, 12])
                .on_press(Message::LoaderVersionPicked(entry.install_version.clone()))
                .style(if selected {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
            );
        }
        if app.loader_catalog.entries.is_empty() {
            builds = builds.push(
                container(
                    column![
                        text("NO COMPATIBLE BUILDS").size(19),
                        text("This loader does not publish a build for the selected Minecraft version.")
                            .font(theme::BODY_FONT)
                            .size(13)
                            .color(theme::MUTED)
                    ]
                    .spacing(5),
                )
                .width(Fill)
                .padding(20)
                .style(theme::inset),
            );
        }
        scrollable(builds)
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
            .into()
    };

    let version = container(column![catalog_header, catalog_body].spacing(12))
        .width(Fill)
        .height(Fill)
        .padding(22)
        .style(theme::panel);

    row![container(choices).width(340), version]
        .spacing(18)
        .height(Fill)
        .into()
}
