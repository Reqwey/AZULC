use crate::{
    app::{
        Launcher, Message,
        navigation::{InstanceTab, Route},
    },
    domain::Instance,
    theme,
};
use iced::widget::{Space, button, column, row, rule, scrollable, text};
use iced::{Alignment, Element, Fill, Length};

use super::super::components::media;

pub(super) fn view<'a>(
    app: &'a Launcher,
    instance: &'a Instance,
    active_tab: InstanceTab,
    page: Element<'a, Message>,
) -> Element<'a, Message> {
    let launching = app.is_instance_launching(instance.id);
    let deleting = app.is_instance_deleting(instance.id);
    let launch_label = if deleting {
        "DELETING…"
    } else if launching {
        "RUNNING…"
    } else {
        "PLAY  >"
    };
    let header = row![
        column![
            row![
                media::instance_marker(instance.color, 24),
                text(&instance.name).size(30),
                text(if instance.favorite { "📌" } else { "" })
                    .size(22)
                    .color(theme::WARNING)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(if instance.description.is_empty() {
                "No description yet."
            } else {
                &instance.description
            })
            .font(theme::BODY_FONT)
            .size(13)
            .color(theme::MUTED)
        ]
        .spacing(4),
        Space::new().width(Fill),
        button(text(launch_label).size(15))
            .on_press_maybe(
                (!launching && !deleting).then_some(Message::LaunchInstance(instance.id))
            )
            .padding([10, 20])
            .style(theme::primary_button)
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let mut tabs = row![].spacing(7);
    for tab in InstanceTab::ALL {
        tabs = tabs.push(
            button(text(tab.label()).size(13))
                .on_press(Message::Navigate(Route::Instance {
                    id: instance.id,
                    tab,
                }))
                .padding([8, 12])
                .style(if active_tab == tab {
                    theme::primary_button
                } else {
                    theme::nav_button
                }),
        );
    }

    let tabs = scrollable(tabs)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(4)
                .scroller_width(4)
                .spacing(4),
        ))
        .height(Length::Shrink)
        .style(theme::square_scrollable);

    column![header, tabs, rule::horizontal(1), page]
        .spacing(14)
        .width(Fill)
        .height(Fill)
        .into()
}
