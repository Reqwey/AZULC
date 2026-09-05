use iced::widget::{container, svg};
use iced::{Element, Fill, alignment};

#[derive(Debug, Clone, Copy)]
pub(in crate::ui) enum WindowControl {
    Minimize,
    Maximize,
    Restore,
    Close,
}

const MINIMIZE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 12h14" fill="none" stroke="#F5F0FF" stroke-width="1.8" stroke-linecap="square"/></svg>"##;
const MAXIMIZE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="5.5" y="5.5" width="13" height="13" fill="none" stroke="#F5F0FF" stroke-width="1.5"/></svg>"##;
const RESTORE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M8.5 8.5h10v10h-10zM5.5 15.5v-10h10v3" transform="translate(24 0) scale(-1 1)" fill="none" stroke="#F5F0FF" stroke-width="1.5" stroke-linejoin="miter"/></svg>"##;
const CLOSE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" fill="none" stroke="#F5F0FF" stroke-width="1.7" stroke-linecap="square"/></svg>"##;
const ACCOUNT_CONTROLS: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 8h5M14 8h5M5 16h9M18 16h1" fill="none" stroke="#CDBBFF" stroke-width="1.7" stroke-linecap="square"/><rect x="10" y="5.5" width="4" height="5" fill="none" stroke="#CDBBFF" stroke-width="1.7"/><rect x="14" y="13.5" width="4" height="5" fill="none" stroke="#CDBBFF" stroke-width="1.7"/></svg>"##;

pub(in crate::ui) fn account_controls<'a, Message>() -> Element<'a, Message>
where
    Message: 'a,
{
    container(
        svg(svg::Handle::from_memory(ACCOUNT_CONTROLS))
            .width(16)
            .height(16),
    )
    .width(16)
    .height(16)
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Center)
    .into()
}

pub(in crate::ui) fn window_control<'a, Message>(control: WindowControl) -> Element<'a, Message>
where
    Message: 'a,
{
    let bytes = match control {
        WindowControl::Minimize => MINIMIZE,
        WindowControl::Maximize => MAXIMIZE,
        WindowControl::Restore => RESTORE,
        WindowControl::Close => CLOSE,
    };
    container(svg(svg::Handle::from_memory(bytes)).width(15).height(15))
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .into()
}
