use crate::{
    app::{Launcher, Message},
    domain::Instance,
    theme,
};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use super::super::components::{CONTENT_END_GAP, SCROLLBAR_GAP};
use super::{activity, components::info_card};

pub(super) fn view<'a>(app: &'a Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let counts = app
        .insights
        .instances
        .iter()
        .find(|item| item.instance_id == instance.id);
    let information = row![
        info_card(
            "GAME BUILD",
            instance.minecraft_version.clone(),
            format_loader(instance),
            theme::LAVENDER,
        ),
        info_card(
            "PLAY TIME",
            format_duration(instance.play_time_seconds),
            "tracked runtime".into(),
            theme::SUCCESS,
        ),
        info_card(
            "CONTENT",
            format!("{} MODS", counts.map_or(0, |value| value.mods)),
            format!(
                "{} worlds / {} packs",
                counts.map_or(0, |value| value.saves),
                counts.map_or(0, |value| value.resource_packs)
            ),
            theme::WARNING,
        )
    ]
    .spacing(12);

    let folder = container(
        row![
            column![
                text("WORKSPACE PATH")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
                text(instance.game_dir.display().to_string())
                    .font(theme::BODY_FONT)
                    .size(13)
                    .color(theme::LAVENDER_SOFT)
            ]
            .spacing(5)
            .width(Fill),
            button(text("OPEN").size(13))
                .on_press(Message::OpenPath(instance.game_dir.clone()))
                .padding([10, 14])
                .style(theme::ghost_button)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding(15)
    .style(theme::inset);

    if let Some(session) = app.launch_session(instance.id) {
        column![information, folder, activity::launch_session(session)]
            .spacing(14)
            .width(Fill)
            .height(Fill)
            .padding(padding::bottom(CONTENT_END_GAP))
            .into()
    } else {
        scrollable(
            column![information, folder]
                .spacing(14)
                .width(Fill)
                .padding(padding::bottom(CONTENT_END_GAP)),
        )
        .width(Fill)
        .height(Fill)
        .spacing(SCROLLBAR_GAP)
        .style(theme::square_scrollable)
        .into()
    }
}

fn format_loader(instance: &Instance) -> String {
    instance.loader.version.as_ref().map_or_else(
        || instance.loader.kind.to_string(),
        |version| format!("{} {version}", instance.loader.kind),
    )
}

fn format_duration(seconds: u64) -> String {
    format!("{:02}H {:02}M", seconds / 3600, (seconds % 3600) / 60)
}
