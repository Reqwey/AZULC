use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use super::components::section;
use crate::ui::components::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let acknowledgements = section(
        "ACKNOWLEDGEMENTS",
        column![
            acknowledgement_group("UI FRAMEWORK", UI_FRAMEWORKS),
            acknowledgement_group("SOURCE REFERENCES", SOURCE_REFERENCES),
            acknowledgement_group("DOWNLOAD SOURCES", DOWNLOAD_SOURCES),
            acknowledgement_group("FONTS", FONTS),
        ]
        .spacing(18),
    );
    let mut actions = row![
        button(
            text("OPEN SOURCE ↗")
                .font(theme::BODY_BOLD)
                .size(12)
                .width(Fill)
                .height(Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
        )
        .on_press(Message::OpenExternalUrl("https://github.com/Reqwey/AZULC"))
        .width(160)
        .height(28)
        .padding([0, 12])
        .style(theme::ghost_button)
    ]
    .spacing(16)
    .align_y(Alignment::Center);
    if app.release_update.available().is_some() {
        actions = actions.push(
            button(
                text("UPDATE AVAILABLE")
                    .size(12)
                    .width(Fill)
                    .align_x(Alignment::Center)
                    .height(Fill)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::ShowUpdateDetails)
            .width(160)
            .height(28)
            .padding([0, 12])
            .style(theme::primary_button),
        );
    }
    scrollable(
        column![
            container(
                column![
                    text("AZULC").size(68).color(theme::LAVENDER_SOFT),
                    text(format!("AZUSA MINECRAFT LAUNCHER  //  VERSION {}", env!("CARGO_PKG_VERSION")))
                        .font(theme::BODY_BOLD)
                        .size(13)
                        .color(theme::LAVENDER),
                    text("A next-generation lightweight, high-performance Minecraft launcher and technology validation platform.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::TEXT),
                    container(actions).padding(padding::bottom(4))
                ]
                .spacing(14),
            )
            .width(Fill)
            .padding(24)
            .style(theme::hero),
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

struct Acknowledgement {
    name: &'static str,
    contribution: &'static str,
    url: &'static str,
}

const UI_FRAMEWORKS: &[Acknowledgement] = &[Acknowledgement {
    name: "Iced 0.14",
    contribution: "Native Rust UI framework",
    url: "https://github.com/iced-rs/iced",
}];

const SOURCE_REFERENCES: &[Acknowledgement] = &[Acknowledgement {
    name: "SJMCL",
    contribution: "Source-code and implementation reference",
    url: "https://mc.sjtu.cn/sjmcl/",
}];

const DOWNLOAD_SOURCES: &[Acknowledgement] = &[Acknowledgement {
    name: "BMCLAPI",
    contribution: "Minecraft download mirror provider",
    url: "https://bmclapidoc.bangbang93.com/",
}];

const FONTS: &[Acknowledgement] = &[
    Acknowledgement {
        name: "Pixelify Sans",
        contribution: "Display typeface · SIL Open Font License",
        url: "https://github.com/eifetx/Pixelify-Sans",
    },
    Acknowledgement {
        name: "Space Mono",
        contribution: "Body and utility typeface · SIL Open Font License",
        url: "https://github.com/googlefonts/spacemono",
    },
];

fn acknowledgement_group(
    title: &'static str,
    acknowledgements: &'static [Acknowledgement],
) -> Element<'static, Message> {
    let mut list = column![].spacing(0);
    for (index, acknowledgement) in acknowledgements.iter().enumerate() {
        if index > 0 {
            list = list.push(rule::horizontal(1));
        }
        list = list.push(acknowledgement_row(acknowledgement));
    }

    column![
        text(title)
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED),
        container(list).width(Fill).padding(1).style(theme::inset)
    ]
    .spacing(7)
    .into()
}

fn acknowledgement_row(acknowledgement: &'static Acknowledgement) -> Element<'static, Message> {
    button(
        row![
            text(acknowledgement.name)
                .size(17)
                .color(theme::LAVENDER_SOFT)
                .width(180),
            text(acknowledgement.contribution)
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::TEXT),
            Space::new().width(Fill),
            text("OPEN ↗").font(theme::BODY_BOLD).size(12)
        ]
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([12, 14])
    .on_press(Message::OpenExternalUrl(acknowledgement.url))
    .style(theme::acknowledgement_list_button)
    .into()
}
