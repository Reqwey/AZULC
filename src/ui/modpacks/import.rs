use crate::{
    app::{Launcher, Message},
    services::modpack::ModpackFormat,
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, text};
use iced::{Alignment, Element, Fill};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
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
