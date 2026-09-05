mod activity;
mod components;
mod content;
mod overview;
mod presentation;
mod settings;

use crate::app::{Launcher, Message, navigation::InstanceTab};
use iced::Element;
use uuid::Uuid;

pub(super) fn view(app: &Launcher, id: Uuid, tab: InstanceTab) -> Element<'_, Message> {
    let Some(instance) = app.instance(id) else {
        return components::missing_state("INSTANCE NO LONGER EXISTS").into();
    };

    let page = match tab {
        InstanceTab::Overview => overview::view(app, instance),
        InstanceTab::Settings => settings::view(app, instance),
        InstanceTab::Worlds
        | InstanceTab::Mods
        | InstanceTab::ResourcePacks
        | InstanceTab::ShaderPacks
        | InstanceTab::DataPacks
        | InstanceTab::Screenshots => content::view(app, instance, tab),
    };

    presentation::view(app, instance, tab, page)
}

pub(super) fn installation_view(app: &Launcher, id: Uuid) -> Element<'_, Message> {
    app.jobs.get(&id).map_or_else(
        || components::missing_state("INSTALLATION NO LONGER EXISTS").into(),
        activity::installation_view,
    )
}
