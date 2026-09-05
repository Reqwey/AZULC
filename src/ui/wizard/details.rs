use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Fill, alignment};

use super::components::{field, summary_line};
use crate::ui::components::media;

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let selected = app.wizard.selected_version.as_deref().unwrap_or("none");
    let form = container(
        column![
            field(
                "INSTANCE NAME",
                text_input("My Minecraft instance", &app.wizard.name)
                    .on_input(Message::WizardNameChanged)
                    .padding(12)
                    .size(14)
                    .style(theme::square_text_input)
            ),
            field(
                "DESCRIPTION",
                text_input("What is this workspace for?", &app.wizard.description)
                    .on_input(Message::WizardDescriptionChanged)
                    .padding(12)
                    .size(14)
                    .style(theme::square_text_input)
            ),
            field(
                "MARKER COLOR",
                media::instance_color_picker(app.wizard.color, Message::WizardColorPicked)
            ),
            container(
                column![
                    text("INSTANCE ISOLATION")
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(theme::SUCCESS),
                    text("A dedicated game directory will be created under AZULC data. Shared libraries and assets remain deduplicated.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(5)
            )
            .width(Fill)
            .padding(14)
            .style(theme::inset)
        ]
        .spacing(15),
    )
    .width(Fill)
    .height(Fill)
    .padding(22)
    .style(theme::panel);

    let summary = container(
        column![
            text("BUILD SUMMARY")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER),
            media::instance_marker(app.wizard.color, 47),
            text(&app.wizard.name).size(25),
            summary_line("MINECRAFT", selected.to_string()),
            summary_line("LOADER", app.wizard.loader.label().to_string()),
            summary_line(
                "LOADER BUILD",
                if app.wizard.loader_version.trim().is_empty() {
                    "AUTO".into()
                } else {
                    app.wizard.loader_version.clone()
                }
            ),
            summary_line("SOURCE", app.persisted.settings.download.source.to_string()),
            summary_line(
                "WORKERS",
                app.persisted.settings.download.concurrency.to_string()
            )
        ]
        .spacing(12)
        .align_x(alignment::Horizontal::Center),
    )
    .width(330)
    .height(Fill)
    .padding(22)
    .style(theme::selected_card);

    row![form, summary].spacing(18).height(Fill).into()
}
