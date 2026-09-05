mod files;
mod presentation;
mod projects;

use crate::{
    app::{Launcher, Message},
    theme,
    ui::components::catalog::provider_tabs,
};
use iced::widget::{Space, column, container, opaque, rule};
use iced::{Element, Fill, alignment};

pub(super) fn view(app: &Launcher) -> Element<'_, Message> {
    let Some(browser) = app.resource_browser.as_ref() else {
        return Space::new().into();
    };

    let body = if let Some(project) = &browser.selected_project {
        files::view(app, browser, project)
    } else {
        projects::view(app, browser)
    };

    let panel = opaque(
        container(
            column![
                presentation::header(app, browser),
                rule::horizontal(1),
                provider_tabs(browser.provider, Message::ResourceProviderPicked),
                presentation::credential_notice(browser.provider),
                body,
                presentation::feedback(browser)
            ]
            .spacing(12),
        )
        .width(Fill)
        .height(Fill)
        .max_width(1010)
        .max_height(720)
        .padding(22)
        .style(theme::modal_panel),
    );

    container(panel)
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .padding(24)
        .into()
}
