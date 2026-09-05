use crate::{
    app::{Launcher, Message, ResourceBrowserState},
    services::{
        catalog::{self, CatalogProvider},
        content::ContentKind,
    },
    theme,
};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Element, Fill};

pub(super) fn header<'a>(
    app: &'a Launcher,
    browser: &ResourceBrowserState,
) -> Element<'a, Message> {
    row![
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
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn credential_notice(provider: CatalogProvider) -> Element<'static, Message> {
    let Some(credential) = catalog::missing_credential(provider) else {
        return Space::new().height(0).into();
    };

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
}

pub(super) fn feedback(browser: &ResourceBrowserState) -> Element<'_, Message> {
    if let Some(error) = &browser.error {
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
    }
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

pub(super) fn release_color(release_type: u8) -> iced::Color {
    match release_type {
        1 => theme::SUCCESS,
        2 => theme::WARNING,
        _ => theme::MUTED,
    }
}
