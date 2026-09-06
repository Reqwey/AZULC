use crate::{
    app::{Launcher, Message, StorageMigrationState},
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Fill, padding};

use super::components::section;
use crate::ui::components::{CONTENT_END_GAP, SCROLLBAR_GAP};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let is_migrating = matches!(app.storage_migration, StorageMigrationState::Migrating(_));
    let mut choose = button(text(if is_migrating {
        "MIGRATING…"
    } else {
        "CHOOSE NEW ROOT"
    }))
    .padding([9, 13])
    .style(theme::primary_button);
    if !is_migrating {
        choose = choose.on_press(Message::ChooseStorageRoot);
    }
    let mut open = button(text("OPEN").font(theme::BODY_BOLD).size(12))
        .padding([7, 11])
        .style(theme::ghost_button);
    if !is_migrating {
        open = open.on_press(Message::OpenFolder(app.paths.data.clone()));
    }

    let current = section(
        "CURRENT STORAGE",
        column![
            row![
                column![
                    text("SHARED ROOT").font(theme::BODY_BOLD).size(12).color(theme::MUTED),
                    text(app.paths.data.display().to_string())
                        .font(theme::BODY_FONT)
                        .size(13)
                        .color(theme::LAVENDER_SOFT)
                ]
                .spacing(4),
                Space::new().width(Fill),
                open,
                choose
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            rule::horizontal(1),
            path_line("MINECRAFT", &app.paths.minecraft, is_migrating),
            path_line("INSTANCES", &app.paths.instances, is_migrating),
            text("Selecting a new root moves launcher state, shared Minecraft files, and every managed instance together.")
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED)
        ]
        .spacing(14),
    );

    let mut content = column![current].spacing(14);
    match &app.storage_migration {
        StorageMigrationState::Pending(path) => {
            content = content.push(section(
                "CONFIRM MIGRATION",
                column![
                    text("NEW SHARED ROOT")
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(theme::MUTED),
                    container(
                        text(path.display().to_string())
                            .font(theme::BODY_FONT)
                            .size(13)
                            .color(theme::LAVENDER_SOFT)
                    )
                    .width(Fill)
                    .padding(12)
                    .style(theme::inset),
                    text("The folder must be empty. AZULC completes the copy and writes the new state before switching locations; managed instance paths are updated automatically.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED),
                    row![
                        Space::new().width(Fill),
                        button(text("CANCEL").font(theme::BODY_BOLD).size(12))
                            .on_press(Message::CancelStorageMigration)
                            .padding([8, 13])
                            .style(theme::ghost_button),
                        button(text("MOVE DATA").font(theme::BODY_BOLD).size(12))
                            .on_press(Message::ConfirmStorageMigration)
                            .padding([8, 13])
                            .style(theme::primary_button)
                    ]
                    .spacing(9)
                    .align_y(Alignment::Center)
                ]
                .spacing(12),
            ));
        }
        StorageMigrationState::Migrating(path) => {
            content = content.push(section(
                "MIGRATION IN PROGRESS",
                column![
                    text(path.display().to_string())
                        .font(theme::BODY_FONT)
                        .size(13)
                        .color(theme::LAVENDER_SOFT),
                    text("Copying storage and preparing the new location. The current location remains usable unless the final switch succeeds.")
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::MUTED)
                ]
                .spacing(7),
            ));
        }
        StorageMigrationState::Idle => {}
    }
    if let Some(error) = &app.storage_migration_error {
        content = content.push(
            container(
                column![
                    text("MIGRATION DID NOT COMPLETE")
                        .font(theme::BODY_BOLD)
                        .size(12)
                        .color(theme::DANGER),
                    text(error)
                        .font(theme::BODY_FONT)
                        .size(12)
                        .color(theme::TEXT)
                ]
                .spacing(6),
            )
            .width(Fill)
            .padding(16)
            .style(theme::panel),
        );
    }

    scrollable(content.padding(padding::bottom(CONTENT_END_GAP)))
        .width(Fill)
        .height(Fill)
        .spacing(SCROLLBAR_GAP)
        .style(theme::square_scrollable)
        .into()
}

fn path_line<'a>(
    label: &'a str,
    path: &'a std::path::Path,
    is_migrating: bool,
) -> Element<'a, Message> {
    let mut open = button(text("OPEN").font(theme::BODY_BOLD).size(12))
        .padding([5, 9])
        .style(theme::ghost_button);
    if !is_migrating {
        open = open.on_press(Message::OpenFolder(path.to_owned()));
    }
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(path.display().to_string())
            .font(theme::BODY_FONT)
            .size(12)
            .color(theme::LAVENDER_SOFT),
        open
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}
