mod about;
mod components;
mod downloads;
mod java;

use crate::{
    app::{Launcher, Message, navigation::SettingsTab},
    theme,
};
use iced::widget::{button, column, row, rule, text};
use iced::{Alignment, Element, Fill};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let header = row![column![
        text("APP SETTINGS").size(31),
        text("Network routes, runtime inventory, and launcher information.")
            .font(theme::BODY_FONT)
            .size(13)
            .color(theme::MUTED)
    ]]
    .align_y(Alignment::Center);

    let mut tabs = row![].spacing(8);
    for tab in SettingsTab::ALL {
        tabs = tabs.push(
            button(text(tab.label()).size(14))
                .on_press(Message::SettingsTabSelected(tab))
                .padding([9, 14])
                .style(if app.settings_tab == tab {
                    theme::primary_button
                } else {
                    theme::nav_button
                }),
        );
    }
    let content = match app.settings_tab {
        SettingsTab::Downloads => downloads::view(app),
        SettingsTab::Java => java::view(app),
        SettingsTab::About => about::view(app),
    };

    column![header, tabs, rule::horizontal(1), content]
        .spacing(15)
        .width(Fill)
        .height(Fill)
        .into()
}
