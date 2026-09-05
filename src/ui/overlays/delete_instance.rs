use crate::{
    app::{Launcher, Message},
    theme,
};
use iced::widget::{Space, button, column, container, opaque, row, text};
use iced::{Alignment, Element, Fill, alignment};
use uuid::Uuid;

pub(super) fn view(app: &Launcher, instance_id: Uuid) -> Element<'_, Message> {
    let name = app
        .persisted
        .instances
        .iter()
        .find(|instance| instance.id == instance_id)
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
                    .size(12)
                    .color(theme::TEXT),
                row![
                    Space::new().width(Fill),
                    button(text("CANCEL").size(13))
                        .on_press(Message::CancelDeleteInstance(instance_id))
                        .padding([10, 15])
                        .style(theme::ghost_button),
                    button(text("DELETE FOREVER").size(13))
                        .on_press(Message::ConfirmDeleteInstance(instance_id))
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
