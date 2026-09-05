use crate::{
    app::{Launcher, Message, navigation::VersionFilter},
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, alignment};

use super::presentation::version_badge;
use crate::ui::components::SCROLLBAR_GAP;

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let mut filters = row![].spacing(8);
    for filter in VersionFilter::ALL {
        let count = app
            .versions
            .iter()
            .filter(|entry| filter.matches(entry))
            .count();
        let selected = app.version_filter == filter;
        let accent = theme::instance_color(filter.color());
        filters = filters.push(
            button(
                row![
                    text(if selected { "◆" } else { "◇" })
                        .size(15)
                        .color(accent),
                    text(format!("{}  {count}", filter.label())).size(13)
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .on_press(Message::VersionFilterSelected(filter))
            .padding([9, 12])
            .style(move |theme, status| {
                theme::color_swatch_button(theme, status, accent, selected)
            }),
        );
    }
    let controls = row![
        filters,
        Space::new().width(Fill),
        text_input("Search versions…", &app.wizard.search)
            .on_input(Message::VersionSearchChanged)
            .padding(10)
            .size(13)
            .width(245)
            .style(theme::square_text_input)
    ]
    .align_y(Alignment::Center);

    let search = app.wizard.search.trim().to_ascii_lowercase();
    let mut list = column![].spacing(6);
    let mut visible = 0usize;
    for version in app.versions.iter().filter(|entry| {
        app.version_filter.matches(entry)
            && (search.is_empty() || entry.id.to_ascii_lowercase().contains(&search))
    }) {
        visible += 1;
        if visible > 300 {
            break;
        }
        let selected = app.wizard.selected_version.as_deref() == Some(version.id.as_str());
        let released = version
            .release_time
            .split('T')
            .next()
            .unwrap_or("unknown date");
        let accent = theme::instance_color(VersionFilter::for_version(version).color());
        list = list.push(
            button(
                row![
                    container(
                        text(if selected { "◆" } else { "◇" })
                            .size(20)
                            .color(accent)
                    )
                    .width(38)
                    .height(40)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center),
                    column![
                        row![
                            text(&version.id).size(17),
                            text(version_badge(version))
                                .font(theme::BODY_BOLD)
                                .size(11)
                                .color(accent)
                        ]
                        .spacing(9)
                        .align_y(Alignment::Center),
                        text(released)
                            .font(theme::BODY_FONT)
                            .size(12)
                            .color(theme::MUTED)
                    ]
                    .spacing(3),
                    Space::new().width(Fill),
                    text("WORLD  >").font(theme::BODY_BOLD).size(11)
                ]
                .spacing(9)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding(12)
            .on_press(Message::VersionPicked(version.id.clone()))
            .style(move |theme, status| {
                theme::color_swatch_button(theme, status, accent, selected)
            }),
        );
    }
    if visible == 0 {
        list = list.push(
            container(
                column![
                    text(if app.versions.is_empty() {
                        "SYNCING VERSION CATALOG…"
                    } else {
                        "NO MATCHING BUILDS"
                    })
                    .size(20)
                    .color(theme::MUTED),
                    text("Try another channel or search term.")
                        .font(theme::BODY_FONT)
                        .size(13)
                        .color(theme::MUTED)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(24)
            .style(theme::inset),
        );
    }
    container(
        column![
            controls,
            rule::horizontal(1),
            scrollable(list)
                .width(Fill)
                .height(Fill)
                .spacing(SCROLLBAR_GAP)
                .style(theme::square_scrollable)
        ]
        .spacing(12),
    )
    .width(Fill)
    .height(Fill)
    .padding(16)
    .style(theme::panel)
    .into()
}
