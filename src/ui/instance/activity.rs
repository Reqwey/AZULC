use crate::{
    app::{InstallJob, LaunchSession, Message},
    domain::InstallStage,
    theme,
};
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, progress_bar, row, scrollable, text};
use iced::{Element, Fill, padding};

use super::super::components::{CONTENT_END_GAP, SCROLLBAR_GAP};
use super::components::format_bytes;

pub(super) fn installation_view(job: &InstallJob) -> Element<'_, Message> {
    let progress = &job.progress;
    let steps = ["METADATA", "GAME", "LOADER", "CONTENT", "FINALIZE", "DONE"];
    let mut rail = row![].spacing(7).align_y(iced::Alignment::Center);
    for (index, label) in steps.iter().enumerate() {
        let color = if progress.stage == InstallStage::Failed {
            theme::DANGER
        } else if progress.stage == InstallStage::Cancelled {
            theme::WARNING
        } else if index < progress.stage.ordinal() {
            theme::SUCCESS
        } else if index == progress.stage.ordinal() {
            theme::LAVENDER_SOFT
        } else {
            theme::MUTED
        };
        rail = rail.push(
            text(format!(
                "{} {label}",
                if index < progress.stage.ordinal() {
                    "●"
                } else {
                    "○"
                }
            ))
            .font(theme::BODY_BOLD)
            .size(12)
            .color(color),
        );
        if index + 1 < steps.len() {
            rail = rail.push(
                text("──")
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::BORDER),
            );
        }
    }
    let stats = if progress.files_total > 0 {
        format!(
            "{} / {} FILES  //  {}  //  {}/S",
            progress.files_done,
            progress.files_total,
            format_bytes(progress.current),
            format_bytes(progress.bytes_per_second as u64)
        )
    } else {
        progress.detail.clone()
    };
    let logs = job.logs.iter().rev().take(80).rev().fold(
        column![].spacing(3).width(Fill),
        |column, line| {
            column.push(
                text(line)
                    .font(theme::BODY_FONT)
                    .size(12)
                    .width(Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(theme::MUTED),
            )
        },
    );
    let terminal_unsuccessful = matches!(
        progress.stage,
        InstallStage::Failed | InstallStage::Cancelled
    );
    let action: Element<'_, Message> = if terminal_unsuccessful {
        button(text("RETRY PIPELINE").size(14))
            .on_press(Message::RetryInstall(job.attempt()))
            .padding([11, 16])
            .style(theme::primary_button)
            .into()
    } else if job.active {
        button(text("CANCEL").size(14))
            .on_press(Message::CancelInstall(job.attempt()))
            .padding([11, 16])
            .style(theme::ghost_button)
            .into()
    } else {
        Space::new().height(0).into()
    };
    let open_log: Element<'_, Message> = if terminal_unsuccessful {
        job.log_path
            .as_ref()
            .map(|path| {
                button(text("OPEN INSTALL LOG").size(14))
                    .on_press(Message::OpenPath(path.clone()))
                    .padding([11, 16])
                    .style(theme::ghost_button)
                    .into()
            })
            .unwrap_or_else(|| Space::new().height(0).into())
    } else {
        Space::new().height(0).into()
    };

    column![
        row![
            column![
                text("INSTALL PIPELINE")
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED),
                text(&job.request.name).size(30),
                text(format!(
                    "Minecraft {} // {}",
                    job.request.minecraft_version, job.request.loader.kind
                ))
                .font(theme::BODY_FONT)
                .size(13)
                .color(theme::LAVENDER)
            ]
            .spacing(4),
            Space::new().width(Fill),
            text(progress.stage.label()).size(18).color(
                if progress.stage == InstallStage::Failed {
                    theme::DANGER
                } else {
                    theme::SUCCESS
                }
            )
        ],
        container(
            column![
                rail,
                progress_bar(0.0..=1.0, progress.fraction())
                    .girth(9)
                    .style(theme::square_progress),
                text(stats)
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(theme::MUTED)
            ]
            .spacing(13)
        )
        .padding(18)
        .style(theme::panel),
        container(
            scrollable(logs)
                .width(Fill)
                .height(Fill)
                .spacing(SCROLLBAR_GAP)
                .anchor_bottom()
                .style(theme::square_scrollable),
        )
        .width(Fill)
        .height(Fill)
        .padding(16)
        .style(theme::inset),
        row![action, open_log].spacing(10)
    ]
    .spacing(15)
    .width(Fill)
    .height(Fill)
    .padding(padding::bottom(CONTENT_END_GAP))
    .into()
}

pub(super) fn launch_session(session: &LaunchSession) -> Element<'_, Message> {
    let color = if session.failed {
        theme::DANGER
    } else if session.ready {
        theme::SUCCESS
    } else {
        theme::LAVENDER
    };
    let logs = session.logs.iter().rev().take(100).rev().fold(
        column![].spacing(2).width(Fill),
        |column, line| {
            column.push(
                text(line)
                    .font(theme::BODY_FONT)
                    .size(11)
                    .width(Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(theme::MUTED),
            )
        },
    );
    let open: Element<'_, Message> = session
        .log_path
        .as_ref()
        .map(|path| {
            button(text("OPEN LOG FILE").size(12))
                .on_press(Message::OpenPath(path.clone()))
                .style(theme::ghost_button)
                .into()
        })
        .unwrap_or_else(|| Space::new().height(0).into());
    container(
        column![
            row![
                column![
                    text(if session.failed {
                        "LAUNCH ERROR"
                    } else {
                        "LAUNCH MONITOR"
                    })
                    .size(19),
                    text(&session.status)
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(color)
                ]
                .spacing(3),
                Space::new().width(Fill),
                text(if session.active { "LIVE" } else { "ENDED" })
                    .font(theme::BODY_BOLD)
                    .size(12)
                    .color(color)
            ],
            container(
                scrollable(logs)
                    .width(Fill)
                    .height(Fill)
                    .spacing(SCROLLBAR_GAP)
                    .anchor_bottom()
                    .style(theme::square_scrollable),
            )
            .width(Fill)
            .height(Fill)
            .padding(12)
            .style(theme::inset),
            open
        ]
        .spacing(11)
        .width(Fill)
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .padding(17)
    .style(if session.failed {
        theme::danger_panel
    } else {
        theme::panel
    })
    .into()
}
