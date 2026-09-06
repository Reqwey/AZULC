use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use super::components::section;
use crate::ui::components::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(super) fn view(_app: &Launcher) -> Element<'_, Message> {
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
    let acknowledgements = section(
        "ACKNOWLEDGEMENTS",
        column![
            acknowledgement_group("SOURCE REFERENCES", SOURCE_REFERENCES),
            acknowledgement_group("DOWNLOAD SOURCES", DOWNLOAD_SOURCES),
            acknowledgement_group("FONTS", FONTS),
        ]
        .spacing(18),
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

struct Acknowledgement {
    name: &'static str,
    contribution: &'static str,
    url: &'static str,
}

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
