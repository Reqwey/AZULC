mod delete_instance;
mod launch_auth;

use crate::app::{Launcher, Message};
use crate::theme;
use iced::widget::{Space, container, mouse_area, stack};
use iced::{Element, Fill};

use super::{resource_browser, shell};

pub(super) fn view<'a>(app: &'a Launcher, page: Element<'a, Message>) -> Element<'a, Message> {
    let root = shell::view(app, page);
    let dialogs: Element<'a, Message> = if let Some(instance_id) = app.pending_delete {
        stack![
            root,
            dismissible_backdrop(Message::CancelDeleteInstance(instance_id)),
            delete_instance::view(app, instance_id)
        ]
        .into()
    } else if app.resource_browser.is_some() {
        stack![
            root,
            dismissible_backdrop(Message::CloseResourceBrowser),
            resource_browser::view(app)
        ]
        .into()
    } else {
        root
    };

    launch_auth::layer(app, shell::frame(app, dialogs))
}

fn dismissible_backdrop(message: Message) -> Element<'static, Message> {
    mouse_area(
        container(Space::new())
            .width(Fill)
            .height(Fill)
            .style(theme::modal_backdrop),
    )
    .on_press(message)
    .into()
}
