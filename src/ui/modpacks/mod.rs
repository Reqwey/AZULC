mod browse;
mod components;
mod files;
mod import;
mod presentation;

use crate::{
    app::{Launcher, Message, navigation::ModpackTab},
    theme,
};
use iced::widget::{Space, button, column, row, rule, text};
use iced::{Alignment, Element, Fill};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let header = row![
        column![
            text("MODPACK DEPOT").size(30),
            text("CURSEFORGE + MODRINTH // ONE CONTINUOUS INSTALL PIPELINE")
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::LAVENDER)
        ]
        .spacing(2),
        Space::new().width(Fill),
        section_tabs(app)
    ]
    .align_y(Alignment::End);

    let body = match app.modpack_tab {
        ModpackTab::Browse => browse::view(app),
        ModpackTab::Import => import::view(app),
    };

    column![header, rule::horizontal(1), body]
        .spacing(14)
        .width(Fill)
        .height(Fill)
        .into()
}

fn section_tabs(app: &Launcher) -> Element<'_, Message> {
    let mut tabs = row![].spacing(7);
    for tab in ModpackTab::ALL {
        tabs = tabs.push(
            button(text(tab.label().to_uppercase()).size(12))
                .on_press(Message::ModpackTabSelected(tab))
                .padding([10, 14])
                .style(if app.modpack_tab == tab {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
        );
    }
    tabs.into()
}
