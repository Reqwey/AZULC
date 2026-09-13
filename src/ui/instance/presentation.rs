use crate::{
    app::{
        Launcher, Message,
        navigation::{InstanceTab, Route},
    },
    domain::Instance,
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text};
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

    let warning = app
        .instance_java_requirement
        .as_ref()
        .filter(|(id, _)| *id == instance.id)
        .and_then(|(_, requirement)| match requirement {
            Ok(_) if app.java_detecting => None,
            Ok(required) => crate::services::java::compatibility_warning(
                &app.java_runtimes,
                &instance.settings,
                *required,
            ),
            Err(error) => Some(format!("Unable to check Java compatibility: {error}")),
        });
    let mut layout = column![header, tabs];
    if let Some(warning) = warning {
        let mut content = column![
            row![
                text("JAVA WARNING")
                    .font(theme::DISPLAY_FONT)
                    .size(12)
                    .color(theme::WARNING)
                    .width(Fill),
                button(text("RESCAN JAVA").size(12))
                    .on_press(Message::RefreshJava)
                    .style(theme::ghost_button),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        if let Some((_, Ok(required))) = app.instance_java_requirement.as_ref() {
            let closest = app
                .java_runtimes
                .iter()
                .min_by_key(|runtime| (runtime.major.abs_diff(*required), runtime.major));
            content = content.push(
                row![
                    java_version("REQUIRED", format!("JAVA {required}"), theme::WARNING),
                    java_version(
                        "CLOSEST DETECTED",
                        closest.map_or_else(
                            || "NONE".to_owned(),
                            |runtime| format!("JAVA {}", runtime.major),
                        ),
                        theme::LAVENDER_SOFT
                    ),
                ]
                .spacing(20),
            );
        }
        let hint = match app.instance_java_requirement.as_ref() {
            Some((_, Ok(_))) if !instance.settings.auto_java => {
                let selected = app
                    .java_runtimes
                    .iter()
                    .find(|runtime| Some(&runtime.path) == instance.settings.java_path.as_ref());
                selected.map_or_else(
                    || "Selected Java not found. Choose Java in Settings.".to_owned(),
                    |runtime| format!("Java {} selected. Change it in Settings.", runtime.major),
                )
            }
            Some((_, Ok(_))) => "Install the required Java, then rescan.".to_owned(),
            _ => warning,
        };
        content = content.push(
            text(hint)
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED),
        );
        layout =
            layout.push(
                container(content)
                    .width(Fill)
                    .padding(16)
                    .style(|_| container::Style {
                        background: Some(theme::WARNING.scale_alpha(0.05).into()),
                        border: iced::Border {
                            color: theme::WARNING.scale_alpha(0.6),
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..container::Style::default()
                    }),
            );
    }
    layout
        .push(rule::horizontal(1))
        .push(page)
        .spacing(14)
        .width(Fill)
        .height(Fill)
        .into()
}

fn java_version(
    label: &'static str,
    value: String,
    color: iced::Color,
) -> Element<'static, Message> {
    column![
        text(label)
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED),
        text(value).font(theme::DISPLAY_FONT).size(24).color(color),
    ]
    .spacing(5)
    .width(Fill)
    .into()
}
