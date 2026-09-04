pub(crate) mod accounts;
pub(crate) mod brand;
pub(crate) mod home;
pub(crate) mod icons;
pub(crate) mod instances;
pub(crate) mod media;
pub(crate) mod modpacks;
pub(crate) mod resource_browser;
pub(crate) mod settings;
pub(crate) mod wizard;

use crate::{
    app::{Launcher, Message, navigation::Route},
    domain::{InstallStage, Instance},
    theme,
};
use iced::mouse;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, rule, scrollable, stack, text,
};
use iced::{Alignment, Element, Fill, Length, alignment, window};

use self::icons::WindowControl;

pub(crate) const CONTENT_END_GAP: f32 = 24.0;
pub(crate) const SCROLLBAR_GAP: f32 = 10.0;

impl Launcher {
    pub fn view(&self) -> Element<'_, Message> {
        let page = match self.route {
            Route::Home => home::view(self),
            Route::Instances => instances::view(self),
            Route::NewInstance => wizard::view(self),
            Route::Accounts => accounts::view(self),
            Route::Settings => settings::view(self),
        };

        let workspace = row![
            sidebar(self),
            container(page)
                .width(Fill)
                .height(Fill)
                .padding(iced::Padding {
                    top: 24.0,
                    right: 24.0,
                    bottom: 0.0,
                    left: 24.0,
                })
                .style(theme::canvas)
        ]
        .width(Fill)
        .height(Fill);

        let mut root = column![titlebar(self), workspace].width(Fill).height(Fill);
        if let Some(notice) = &self.notice {
            root = root.push(
                container(
                    row![
                        text("!").size(22).color(theme::LAVENDER),
                        text(notice)
                            .font(theme::BODY_BOLD)
                            .size(13)
                            .color(theme::TEXT),
                        Space::new().width(Fill),
                        button(text("DISMISS").size(13))
                            .on_press(Message::DismissNotice)
                            .padding([7, 12])
                            .style(theme::ghost_button)
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .padding([10, 20])
                .style(theme::inset),
            );
        }
        let root: Element<'_, Message> = root.into();
        let content: Element<'_, Message> = if self.pending_delete.is_some() {
            let backdrop = mouse_area(
                container(Space::new())
                    .width(Fill)
                    .height(Fill)
                    .style(theme::modal_backdrop),
            )
            .on_press(Message::CancelDeleteInstance);

            stack![root, backdrop, delete_confirmation(self)].into()
        } else if self.resource_browser.is_some() {
            let backdrop = mouse_area(
                container(Space::new())
                    .width(Fill)
                    .height(Fill)
                    .style(theme::modal_backdrop),
            )
            .on_press(Message::CloseResourceBrowser);

            stack![root, backdrop, resource_browser::view(self)].into()
        } else {
            root
        };

        resize_frame(content)
    }
}

fn delete_confirmation(app: &Launcher) -> Element<'_, Message> {
    let name = app
        .pending_delete
        .and_then(|id| {
            app.persisted
                .instances
                .iter()
                .find(|instance| instance.id == id)
        })
        .map_or("this instance", |instance| instance.name.as_str());
    let panel = opaque(
        container(
            column![
                text("DELETE INSTANCE?").size(27).color(theme::DANGER),
                text(name).size(18).color(theme::TEXT),
                text("This instance will be lost forever! (A long time!)")
                    .font(theme::BODY_BOLD)
                    .size(13)
                    .color(theme::WARNING),
                text("Saves, mods, resource packs, screenshots, and instance settings in its folder will be permanently removed.")
                    .font(theme::BODY_FONT)
                    .size(10)
                    .color(theme::TEXT),
                row![
                    Space::new().width(Fill),
                    button(text("CANCEL").size(13))
                        .on_press(Message::CancelDeleteInstance)
                        .padding([10, 15])
                        .style(theme::ghost_button),
                    button(text("DELETE FOREVER").size(13))
                        .on_press(Message::ConfirmDeleteInstance)
                        .padding([10, 15])
                        .style(theme::danger_button)
                ]
                .spacing(9)
                .align_y(Alignment::Center)
            ]
            .spacing(14),
        )
        .width(Fill)
        .max_width(560)
        .padding(24)
        .style(theme::danger_panel),
    );

    container(panel)
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .padding(24)
        .into()
}

fn titlebar(app: &Launcher) -> Element<'_, Message> {
    let drag_region = mouse_area(
        container(text("Azusa Minecraft Launcher").align_y(Alignment::Center))
            .padding([0, 16])
            .width(Fill)
            .height(44)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::DragWindow)
    .on_double_click(Message::ToggleMaximize);

    let controls = row![
        window_button(WindowControl::Minimize, Message::MinimizeWindow, false),
        window_button(WindowControl::Maximize, Message::ToggleMaximize, false),
        window_button(WindowControl::Close, Message::CloseWindow, true),
    ]
    .height(44)
    .align_y(Alignment::Center);

    container(row![drag_region, titlebar_account(app), controls])
        .height(44)
        .width(Fill)
        .style(theme::titlebar)
        .into()
}

fn titlebar_account(app: &Launcher) -> Element<'_, Message> {
    let account = app.persisted.active_account();
    let avatar = match account {
        Some(account) => accounts::account_avatar(account, true, 32.0),
        None => container(text("@").size(17).color(theme::LAVENDER_SOFT))
            .width(32)
            .height(32)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(theme::pill)
            .into(),
    };
    button(
        row![
            avatar,
            column![
                text(account.map_or("NO PROFILE", |value| value.username.as_str())).size(14),
                text(
                    account.map_or("CREATE AN ACCOUNT", |value| match value.provider {
                        crate::domain::AccountProvider::Microsoft => "MICROSOFT / ACTIVE",
                        crate::domain::AccountProvider::Offline => "OFFLINE TEST / ACTIVE",
                        crate::domain::AccountProvider::ThirdParty => "UNSUPPORTED / ACTIVE",
                    })
                )
                .font(theme::BODY_FONT)
                .size(8)
                .color(theme::MUTED)
            ]
            .spacing(1),
            Space::new().width(Fill),
            text(">").size(15).color(theme::LAVENDER)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .width(240)
    .height(44)
    .padding([4, 10])
    .on_press(Message::Navigate(Route::Accounts))
    .style(theme::ghost_button)
    .into()
}

fn window_button<'a>(
    control: WindowControl,
    message: Message,
    danger: bool,
) -> Element<'a, Message> {
    button(icons::window_control(control))
        .width(46)
        .height(44)
        .padding(0)
        .on_press(message)
        .style(if danger {
            theme::danger_window_button
        } else {
            theme::window_button
        })
        .into()
}

fn resize_frame<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    const EDGE: f32 = 12.0;
    const CORNER: f32 = 24.0;

    let top = row![
        resize_handle(
            window::Direction::NorthWest,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyDown,
        ),
        resize_handle(
            window::Direction::North,
            Fill,
            EDGE.into(),
            mouse::Interaction::ResizingVertically,
        ),
        resize_handle(
            window::Direction::NorthEast,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyUp,
        )
    ];
    let middle = row![
        resize_handle(
            window::Direction::West,
            EDGE.into(),
            Fill,
            mouse::Interaction::ResizingHorizontally,
        ),
        content,
        resize_handle(
            window::Direction::East,
            EDGE.into(),
            Fill,
            mouse::Interaction::ResizingHorizontally,
        )
    ]
    .height(Fill);
    let bottom = row![
        resize_handle(
            window::Direction::SouthWest,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyUp,
        ),
        resize_handle(
            window::Direction::South,
            Fill,
            EDGE.into(),
            mouse::Interaction::ResizingVertically,
        ),
        resize_handle(
            window::Direction::SouthEast,
            CORNER.into(),
            EDGE.into(),
            mouse::Interaction::ResizingDiagonallyDown,
        )
    ];

    container(column![top, middle, bottom].width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(theme::canvas)
        .into()
}

fn resize_handle<'a>(
    direction: window::Direction,
    width: Length,
    height: Length,
    interaction: mouse::Interaction,
) -> Element<'a, Message> {
    mouse_area(Space::new().width(width).height(height))
        .on_press(Message::ResizeWindow(direction))
        .interaction(interaction)
        .into()
}

fn sidebar(app: &Launcher) -> Element<'_, Message> {
    let mut instance_cards = column![].spacing(6);
    let mut has_instance_rows = false;
    let mut instances = app.persisted.instances.iter().collect::<Vec<_>>();
    instances.sort_by_key(|instance| !instance.favorite);
    for instance in instances {
        has_instance_rows = true;
        instance_cards = instance_cards.push(instance_sidebar_button(app, instance));
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
        let selected =
            app.route == Route::Instances && app.selected == Some(job.request.instance_id);
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
                        .size(9)
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
            .on_press(Message::SelectInstance(job.request.instance_id))
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
                .size(10)
                .color(theme::MUTED),
        );
    }
    let library = scrollable(instance_cards)
        .height(Fill)
        .style(theme::square_scrollable)
        .spacing(9)
        .height(Fill);

    container(
        column![
            nav_button(Route::Home, app.route),
            rule::horizontal(1),
            library,
            nav_button(Route::NewInstance, app.route),
            rule::horizontal(1),
            nav_button(Route::Accounts, app.route),
            nav_button(Route::Settings, app.route)
        ]
        .padding([20, 16])
        .spacing(12),
    )
    .width(278)
    .height(Fill)
    .style(theme::sidebar)
    .into()
}

fn instance_sidebar_button<'a>(app: &Launcher, instance: &'a Instance) -> Element<'a, Message> {
    let selected = app.route == Route::Instances && app.selected == Some(instance.id);
    button(
        row![
            container(media::instance_marker(instance.color, 19))
                .width(30)
                .height(28)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
            column![
                text(&instance.name).size(15),
                text(format!(
                    "MC {} // {}",
                    instance.minecraft_version, instance.loader.kind
                ))
                .font(theme::BODY_FONT)
                .size(9)
                .color(if selected {
                    theme::CANVAS
                } else {
                    theme::MUTED
                })
            ]
            .spacing(1),
            Space::new().width(Fill),
            text(if instance.favorite { "📌" } else { ">" }).size(14)
        ]
        .spacing(7)
        .align_y(Alignment::Center),
    )
    .width(Fill)
    .padding([10, 9])
    .on_press(Message::SelectInstance(instance.id))
    .style(if selected {
        theme::primary_button
    } else {
        theme::nav_button
    })
    .into()
}

fn nav_button(route: Route, active: Route) -> Element<'static, Message> {
    let is_active = route == active;
    let icon = match route {
        Route::Home => "⌂",
        Route::Instances => "▣",
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
            text(if is_active { "◆" } else { "" }).size(10)
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
