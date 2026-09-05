use crate::{
    app::{Launcher, Message},
    domain::Instance,
    theme,
};
use iced::widget::{Space, button, checkbox, column, row, scrollable, slider, text};
use iced::{Alignment, Element, Fill, padding};

use super::super::components::{CONTENT_END_GAP, SCROLLBAR_GAP, media};
use super::components::{labeled_input, section, setting_slider};

pub(super) fn view<'a>(app: &'a Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let instance_id = instance.id;
    let memory_limit = app.system_resources.memory_limit_mb();
    let memory_value = instance.settings.max_memory_mb.clamp(512, memory_limit);
    let identity = section(
        "IDENTITY",
        column![
            labeled_input(
                "INSTANCE NAME",
                &instance.name,
                "My modded world",
                move |value| Message::EditInstanceName(instance_id, value),
            ),
            labeled_input(
                "DESCRIPTION",
                &instance.description,
                "What makes this workspace special?",
                move |value| Message::EditInstanceDescription(instance_id, value),
            ),
            column![
                text("MARKER COLOR")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
                media::instance_color_picker(instance.color, move |value| {
                    Message::EditInstanceColor(instance_id, value)
                })
            ]
            .spacing(6),
            checkbox(instance.favorite)
                .label("Pin as a favorite")
                .on_toggle(move |value| Message::ToggleInstanceFavorite(instance_id, value))
                .size(20)
                .style(theme::square_checkbox)
        ]
        .spacing(12),
    );

    let manual_java: Element<'a, Message> = if instance.settings.auto_java {
        Space::new().height(0).into()
    } else if app.java_runtimes.is_empty() {
        text("No detected Java runtime is available for manual selection.")
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::WARNING)
            .into()
    } else {
        let mut choices = column![].spacing(7);
        for runtime in &app.java_runtimes {
            let selected = instance.settings.java_path.as_ref() == Some(&runtime.path);
            choices = choices.push(
                button(
                    row![
                        text(format!("JAVA {}", runtime.major)).size(13),
                        text(&runtime.vendor)
                            .font(theme::BODY_FONT)
                            .size(11)
                            .color(if selected {
                                theme::CANVAS
                            } else {
                                theme::MUTED
                            }),
                        Space::new().width(Fill),
                        text(if selected { "SELECTED" } else { "USE" })
                            .font(theme::BODY_BOLD)
                            .size(11)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(9)
                .on_press(Message::SetInstanceJava(instance_id, runtime.path.clone()))
                .style(if selected {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
            );
        }
        choices.into()
    };

    let game = section(
        "GAME / JAVA",
        column![
            checkbox(instance.settings.isolated)
                .label("Keep this instance isolated")
                .on_toggle(move |value| Message::SetInstanceIsolation(instance_id, value))
                .size(20)
                .style(theme::square_checkbox),
            checkbox(instance.settings.auto_java)
                .label("Automatically choose a compatible Java")
                .on_toggle(move |value| Message::SetInstanceAutoJava(instance_id, value))
                .size(20)
                .style(theme::square_checkbox),
            manual_java,
            setting_slider(
                "WINDOW WIDTH",
                format!("{} PX", instance.settings.width),
                slider(640..=3840, instance.settings.width, move |value| {
                    Message::SetInstanceWidth(instance_id, value)
                })
                .step(16_u32)
                .style(theme::square_slider),
            ),
            setting_slider(
                "WINDOW HEIGHT",
                format!("{} PX", instance.settings.height),
                slider(360..=2160, instance.settings.height, move |value| {
                    Message::SetInstanceHeight(instance_id, value)
                })
                .step(9_u32)
                .style(theme::square_slider),
            ),
            checkbox(instance.settings.fullscreen)
                .label("Launch in fullscreen")
                .on_toggle(move |value| Message::SetInstanceFullscreen(instance_id, value))
                .size(20)
                .style(theme::square_checkbox),
        ]
        .spacing(13),
    );

    let performance = section(
        "PERFORMANCE / WINDOW",
        column![
            checkbox(instance.settings.auto_memory)
                .label("Allocate memory automatically")
                .on_toggle(move |value| Message::SetInstanceAutoMemory(instance_id, value))
                .size(20)
                .style(theme::square_checkbox),
            setting_slider(
                "MAXIMUM MEMORY",
                format!("{memory_value} MiB // {memory_limit} MiB AVAILABLE"),
                slider(512..=memory_limit, memory_value, move |value| {
                    Message::SetInstanceMemory(instance_id, value)
                })
                .step(128_u32)
                .style(theme::square_slider),
            ),
            text("AVAILABLE MEMORY REFRESHES EVERY 2 SECONDS")
                .font(theme::BODY_FONT)
                .size(11)
                .color(theme::MUTED),
            labeled_input(
                "CUSTOM WINDOW TITLE",
                &instance.settings.custom_window_title,
                "Leave blank for Minecraft",
                move |value| Message::SetInstanceWindowTitle(instance_id, value),
            ),
            labeled_input(
                "CUSTOM INFORMATION",
                &instance.settings.custom_info,
                "Shown in compatible game screens",
                move |value| Message::SetInstanceCustomInfo(instance_id, value),
            ),
            button(
                text(if app.is_instance_deleting(instance.id) {
                    "DELETING INSTANCE…"
                } else {
                    "DELETE INSTANCE DATA"
                })
                .size(13),
            )
            .on_press_maybe(
                (!app.is_instance_launching(instance.id) && !app.is_instance_deleting(instance.id))
                    .then_some(Message::DeleteInstance(instance.id))
            )
            .padding([10, 14])
            .style(theme::danger_window_button)
        ]
        .spacing(13),
    );

    scrollable(
        column![identity, game, performance]
            .spacing(14)
            .padding(padding::bottom(CONTENT_END_GAP)),
    )
    .width(Fill)
    .height(Fill)
    .spacing(SCROLLBAR_GAP)
    .style(theme::square_scrollable)
    .into()
}
