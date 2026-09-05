use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use super::components::section;
use crate::ui::components::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let architecture = row![
        about_card(
            "UI",
            "ICED 0.14",
            "Native Rust widgets",
            "https://github.com/iced-rs/iced",
        ),
        about_card(
            "BUILD",
            env!("CARGO_PKG_VERSION"),
            "Open Source",
            "https://github.com/Reqwey/AZULC",
        )
    ]
    .spacing(12);
    let data = section(
        "LOCAL DATA",
        column![
            path_info_line("ROOT", app.paths.data.clone()),
            path_info_line("MINECRAFT", app.paths.minecraft.clone()),
            path_info_line("INSTANCES", app.paths.instances.clone()),
            info_line("FONT", "Pixelify Sans + Space Mono (OFL)".into()),
        ]
        .spacing(10),
    );
    let acknowledgements = section(
        "ACKNOWLEDGEMENTS",
        row![
            acknowledgement_card(
                "SJMCL",
                "Source-code & implementation reference",
                "https://mc.sjtu.cn/sjmcl/",
            ),
            acknowledgement_card(
                "BMCLAPI",
                "Minecraft download mirror provider",
                "https://bmclapidoc.bangbang93.com/",
            ),
        ]
        .spacing(12),
    );
    scrollable(
        column![
            container(
                column![
                    text("AZULC").size(68).color(theme::LAVENDER_SOFT),
                    text("AZUSA MINECRAFT LAUNCHER")
                        .font(theme::BODY_BOLD)
                        .size(13)
                        .color(theme::LAVENDER),
                    text("A next-generation lightweight, high-performance Minecraft launcher and technology validation platform.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::TEXT)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(24)
            .style(theme::hero),
            architecture,
            data,
            acknowledgements
        ]
        .spacing(14)
        .padding(padding::bottom(CONTENT_END_GAP)),
    )
    .width(Fill)
    .height(Fill)
    .spacing(SCROLLBAR_GAP)
    .style(theme::square_scrollable)
    .into()
}

fn about_card<'a>(
    label: &'a str,
    value: &'a str,
    detail: &'a str,
    url: &'static str,
) -> Element<'a, Message> {
    button(
        row![
            column![
                text(label)
                    .font(theme::BODY_BOLD)
                    .size(13)
                    .color(theme::MUTED),
                text(value).size(24).color(theme::LAVENDER_SOFT),
                text(detail)
                    .font(theme::BODY_FONT)
                    .size(13)
                    .color(theme::TEXT),
                text(url)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::LAVENDER)
            ]
            .spacing(4),
            Space::new().width(Fill),
            text("OPEN ↗")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER)
        ]
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([17, 18])
    .on_press(Message::OpenExternalUrl(url))
    .style(theme::version_card_button)
    .into()
}

fn info_line(label: &'static str, value: String) -> Element<'static, Message> {
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(value)
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::LAVENDER_SOFT)
    ]
    .into()
}

fn path_info_line(label: &'static str, path: std::path::PathBuf) -> Element<'static, Message> {
    let value = path.display().to_string();
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(value)
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::LAVENDER_SOFT),
        button(text("OPEN").font(theme::BODY_BOLD).size(12))
            .on_press(Message::OpenFolder(path))
            .padding([5, 9])
            .style(theme::ghost_button)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn acknowledgement_card(
    name: &'static str,
    contribution: &'static str,
    url: &'static str,
) -> Element<'static, Message> {
    button(
        row![
            column![
                text(name).size(24).color(theme::LAVENDER_SOFT),
                text(contribution)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::TEXT),
                text(url)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .color(theme::LAVENDER)
            ]
            .spacing(5),
            Space::new().width(Fill),
            text("OPEN ↗")
                .font(theme::BODY_BOLD)
                .size(12)
                .color(theme::LAVENDER)
        ]
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([17, 18])
    .on_press(Message::OpenExternalUrl(url))
    .style(theme::version_card_button)
    .into()
}
