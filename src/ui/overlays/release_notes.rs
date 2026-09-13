use crate::{app::Message, domain::ReleaseNotes, theme};
use iced::{
    Element, Fill, alignment,
    widget::{button, column, container, opaque, row, scrollable, text},
};

pub(super) fn view(notes: &ReleaseNotes, download: Option<bool>) -> Element<'_, Message> {
    let mut actions = row![
        button(
            text(if download.is_some() {
                "CLOSE"
            } else {
                "GOT IT"
            })
            .size(13)
        )
        .on_press(Message::DismissReleaseNotes)
        .padding([10, 15])
        .style(theme::ghost_button)
    ]
    .spacing(16);
    if let Some(can_download) = download {
        let mut download_button = button(text("DOWNLOAD UPDATE").size(13))
            .padding([10, 15])
            .style(theme::primary_button);
        if can_download {
            download_button = download_button.on_press(Message::DownloadUpdate);
        }
        actions = actions.push(download_button);
    }
    let mut body = column![
        text(format!("WHAT'S NEW — {}", notes.version))
            .size(26)
            .color(theme::LAVENDER_SOFT),
        scrollable(text(&notes.body).font(theme::BODY_FONT).size(13))
            .height(300)
            .spacing(12)
            .style(theme::square_scrollable),
    ]
    .spacing(18);
    if download == Some(false) {
        body = body.push(
            text("No download is available for this operating system and architecture.").size(12),
        );
    }
    let panel = container(body.push(actions))
        .width(Fill)
        .max_width(680)
        .padding(24)
        .style(theme::panel);
    container(opaque(panel))
        .width(Fill)
        .height(Fill)
        .padding(24)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .into()
}
