use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use crate::ui::components::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let mut list = column![
        row![
            column![
                text("DETECTED JAVA RUNTIMES").size(20),
                text(
                    "AZULC chooses the smallest compatible major version for each Minecraft build."
                )
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED)
            ]
            .spacing(3),
            Space::new().width(Fill),
            button(text("SCAN AGAIN").size(12))
                .on_press(Message::RefreshJava)
                .padding([8, 13])
                .style(theme::ghost_button)
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(10);
    if app.java_runtimes.is_empty() {
        list = list.push(
            container(
                column![
                    text("NO JAVA FOUND").size(21).color(theme::WARNING),
                    text("Modern Minecraft generally needs Java 17 or 21; legacy builds may need Java 8.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(18)
            .style(theme::danger_panel),
        );
    } else {
        for runtime in &app.java_runtimes {
            list = list.push(
                container(
                    row![
                        container(
                            text(format!("J{}", runtime.major))
                                .size(20)
                                .color(theme::LAVENDER_SOFT)
                        )
                        .width(54)
                        .height(48)
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center)
                        .style(theme::pill),
                        column![
                            row![
                                text(format!("Java {}", runtime.major)).size(18),
                                text(&runtime.vendor)
                                    .font(theme::BODY_BOLD)
                                    .size(11)
                                    .color(theme::LAVENDER)
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                            text(runtime.path.display().to_string())
                                .font(theme::BODY_FONT)
                                .size(11)
                                .color(theme::MUTED)
                        ]
                        .spacing(3),
                        Space::new().width(Fill),
                        text(&runtime.version)
                            .font(theme::BODY_BOLD)
                            .size(12)
                            .color(theme::SUCCESS)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding(14)
                .style(theme::panel),
            );
        }
    }
    scrollable(list.padding(padding::bottom(CONTENT_END_GAP)))
        .width(Fill)
        .height(Fill)
        .spacing(SCROLLBAR_GAP)
        .style(theme::square_scrollable)
        .into()
}
