use crate::{
    app::{Launcher, Message, navigation::Route},
    domain::{InstallStage, Instance},
    theme,
    ui::components::media,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text};
use iced::{Alignment, Element, Fill, alignment};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let mut instance_cards = column![].spacing(6).width(Fill);
    let mut has_instance_rows = false;
    let mut instances = app.persisted.instances.iter().collect::<Vec<_>>();
    instances.sort_by_key(|instance| !instance.favorite);
    for instance in instances {
        has_instance_rows = true;
        instance_cards = instance_cards.push(instance_button(app, instance));
    }
    for job in app.jobs.values().filter(|job| {
        job.progress.stage != InstallStage::Complete
            && !app
                .persisted
                .instances
                .iter()
                .any(|instance| instance.id == job.request.instance_id)
    }) {
        has_instance_rows = true;
        let selected = app.route == Route::Installation(job.request.instance_id);
        let signal = if job.active {
            theme::SUCCESS
        } else if job.progress.stage == InstallStage::Failed {
            theme::DANGER
        } else {
            theme::WARNING
        };
        instance_cards = instance_cards.push(
            button(
                row![
                    text(if job.active { "↓" } else { "!" })
                        .size(18)
                        .color(if selected { theme::CANVAS } else { signal }),
                    column![
                        text(&job.request.name).size(15),
                        text(format!(
                            "{} // {:02}%",
                            job.progress.stage.label(),
                            (job.progress.fraction() * 100.0) as u8
                        ))
                        .font(theme::BODY_FONT)
                        .size(11)
                        .color(if selected {
                            theme::CANVAS
                        } else {
                            theme::MUTED
                        })
                    ]
                    .spacing(1)
                ]
                .spacing(9)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([10, 11])
            .on_press(Message::Navigate(Route::Installation(
                job.request.instance_id,
            )))
            .style(if selected {
                theme::primary_button
            } else {
                theme::ghost_button
            }),
        );
    }
    if !has_instance_rows {
        instance_cards = instance_cards.push(
            text("No local instances yet.")
                .font(theme::BODY_FONT)
                .size(12)
                .color(theme::MUTED),
        );
    }
    let library = scrollable(instance_cards)
        .width(Fill)
        .height(Fill)
        .style(theme::square_scrollable)
        .spacing(9);

    container(
        column![
            navigation_button(Route::Home, app.route),
            rule::horizontal(1),
            library,
            navigation_button(Route::NewInstance, app.route),
            rule::horizontal(1),
            navigation_button(Route::Accounts, app.route),
            navigation_button(Route::Settings, app.route)
        ]
        .padding([20, 16])
        .spacing(12),
    )
    .width(278)
    .height(Fill)
    .style(theme::sidebar)
    .into()
}

fn instance_button<'a>(app: &Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let selected = matches!(
        app.route,
        Route::Instance { id, .. } if id == instance.id
    );
    let launching = app.is_instance_launching(instance.id);
    button(
        row![
            container(media::instance_marker(instance.color, 19))
                .width(30)
                .height(28)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center),
            column![
                text(&instance.name).size(15),
                text(format!(
                    "MC {} // {}",
                    instance.minecraft_version, instance.loader.kind
                ))
                .font(theme::BODY_FONT)
                .size(11)
                .color(if selected {
                    theme::CANVAS
                } else {
                    theme::MUTED
                })
            ]
            .spacing(1)
            .width(Fill),
            text(if launching { "LIVE" } else { "" })
                .font(theme::BODY_BOLD)
                .size(10)
                .color(if selected {
                    theme::CANVAS
                } else {
                    theme::SUCCESS
                }),
            container(
                text(if instance.favorite { "📌" } else { ">" })
                    .size(14)
                    .width(Fill)
                    .height(Fill)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center),
            )
            .width(28)
            .height(32)
        ]
        .spacing(7)
        .width(Fill)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([10, 9])
    .on_press(Message::Navigate(Route::instance(instance.id)))
    .style(if selected {
        theme::primary_button
    } else {
        theme::nav_button
    })
    .into()
}

fn navigation_button(route: Route, active: Route) -> Element<'static, Message> {
    let is_active = route == active;
    let icon = match route {
        Route::Home => "⌂",
        Route::Instance { .. } | Route::Installation(_) => "",
        Route::NewInstance => "+",
        Route::Accounts => "@",
        Route::Settings => "#",
    };
    button(
        row![
            text(icon).size(19).color(if is_active {
                theme::CANVAS
            } else {
                theme::LAVENDER
            }),
            text(route.label()).size(17),
            Space::new().width(Fill),
            text(if is_active { "◆" } else { "" }).size(12)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([12, 13])
    .on_press(Message::Navigate(route))
    .style(if is_active {
        theme::primary_button
    } else {
        theme::nav_button
    })
    .into()
}
