mod components;
mod details;
mod loader;
mod presentation;
mod version;

use crate::{
    app::{
        Launcher, Message,
        navigation::{NewInstanceTab, Route, WizardStep},
    },
    domain::LoaderKind,
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, text};
use iced::{Alignment, Element, Fill, alignment, padding};

use super::components::CONTENT_END_GAP;

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let mut tabs = row![].spacing(7);
    for tab in NewInstanceTab::ALL {
        tabs = tabs.push(
            button(text(tab.label().to_uppercase()).size(13))
                .on_press(Message::NewInstanceTabSelected(tab))
                .padding([11, 17])
                .style(if app.new_instance_tab == tab {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
        );
    }
    let page_tabs = row![
        text("NEW INSTANCE")
            .font(theme::BODY_BOLD)
            .size(12)
            .color(theme::MUTED),
        Space::new().width(18),
        tabs,
        Space::new().width(Fill)
    ]
    .align_y(Alignment::Center);

    let page = match app.new_instance_tab {
        NewInstanceTab::Minecraft => minecraft_view(app),
        NewInstanceTab::Modpacks => super::modpacks::view(app),
    };

    column![page_tabs, page]
        .spacing(15)
        .width(Fill)
        .height(Fill)
        .into()
}

fn minecraft_view(app: &Launcher) -> Element<'_, Message> {
    let header = text("BUILD A NEW INSTANCE")
        .size(31)
        .align_y(Alignment::Center);

    let stepper = row![
        step(
            "01",
            minecraft_step_title(app),
            None,
            WizardStep::Version,
            app,
        ),
        container(rule::horizontal(2)).width(70),
        step(
            "02",
            app.wizard.loader.label().to_uppercase(),
            loader_step_version(app),
            WizardStep::Loader,
            app,
        ),
        container(rule::horizontal(2)).width(70),
        step("03", "BASIC DETAILS".into(), None, WizardStep::Details, app,),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let content = match app.wizard_step {
        WizardStep::Version => version::view(app),
        WizardStep::Loader => loader::view(app),
        WizardStep::Details => details::view(app),
    };
    let can_advance = match app.wizard_step {
        WizardStep::Version => app.wizard.selected_version.is_some(),
        WizardStep::Loader | WizardStep::Details => {
            app.wizard.selected_version.is_some() && loader_selection_ready(app)
        }
    };
    let footer = row![
        Space::new().width(Fill),
        button(text("CANCEL").size(13))
            .on_press(Message::Navigate(
                app.last_instance()
                    .map_or(Route::Home, |instance| Route::instance(instance.id)),
            ))
            .padding([10, 15])
            .style(theme::nav_button),
        button(text("BACK").size(13))
            .on_press_maybe((app.wizard_step != WizardStep::Version).then_some(Message::WizardBack))
            .padding([10, 15])
            .style(theme::ghost_button),
        button(
            text(if app.wizard_step == WizardStep::Details {
                "CREATE & INSTALL  >"
            } else {
                "NEXT  >"
            })
            .size(14)
        )
        .on_press_maybe(can_advance.then_some(Message::WizardNext))
        .padding([11, 18])
        .style(theme::primary_button)
    ]
    .spacing(9)
    .align_y(Alignment::Center);

    column![header, stepper, content, footer]
        .spacing(17)
        .width(Fill)
        .height(Fill)
        .padding(padding::bottom(CONTENT_END_GAP))
        .into()
}

fn step<'a>(
    number: &'static str,
    title: String,
    subtitle: Option<String>,
    target: WizardStep,
    app: &'a Launcher,
) -> Element<'a, Message> {
    let current = app.wizard_step == target;
    let complete = match target {
        WizardStep::Version => app.wizard.selected_version.is_some() && !current,
        WizardStep::Loader => app.wizard_step == WizardStep::Details,
        WizardStep::Details => false,
    };
    let title_color = if current { theme::TEXT } else { theme::MUTED };
    let mut copy = column![
        text(title)
            .font(theme::BODY_BOLD)
            .size(13)
            .color(title_color)
    ]
    .spacing(1);
    if let Some(subtitle) = subtitle {
        copy = copy.push(
            text(subtitle)
                .font(theme::BODY_FONT)
                .size(11)
                .color(if current {
                    theme::LAVENDER_SOFT
                } else {
                    theme::MUTED
                }),
        );
    }

    button(
        row![
            container(text(if complete { "✓" } else { number }).size(16).color(
                if current || complete {
                    theme::LAVENDER_SOFT
                } else {
                    theme::MUTED
                }
            ))
            .width(38)
            .height(38)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .style(if current || complete {
                theme::pill
            } else {
                theme::inset
            }),
            copy
        ]
        .spacing(9)
        .align_y(Alignment::Center),
    )
    .on_press(Message::WizardStepSelected(target))
    .padding(3)
    .style(theme::nav_button)
    .into()
}

fn minecraft_step_title(app: &Launcher) -> String {
    app.wizard
        .selected_version
        .clone()
        .unwrap_or_else(|| "MINECRAFT VERSION".into())
}

fn loader_step_version(app: &Launcher) -> Option<String> {
    if app.wizard.loader == LoaderKind::Vanilla {
        return Some("NO LOADER".into());
    }

    if app.wizard.loader_version.trim().is_empty() {
        return None;
    }

    Some(
        app.loader_catalog
            .entries
            .iter()
            .find(|entry| entry.install_version == app.wizard.loader_version)
            .map(|entry| entry.version.clone())
            .unwrap_or_else(|| app.wizard.loader_version.clone()),
    )
}

fn loader_selection_ready(app: &Launcher) -> bool {
    app.wizard.loader == LoaderKind::Vanilla || !app.wizard.loader_version.trim().is_empty()
}
