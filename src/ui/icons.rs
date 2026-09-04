use iced::widget::{container, svg};
use iced::{Element, Fill, alignment};

#[derive(Debug, Clone, Copy)]
pub enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

const MINIMIZE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 12h14" fill="none" stroke="#F5F0FF" stroke-width="1.8" stroke-linecap="square"/></svg>"##;
const MAXIMIZE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="5.5" y="5.5" width="13" height="13" fill="none" stroke="#F5F0FF" stroke-width="1.5"/></svg>"##;
const CLOSE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" fill="none" stroke="#F5F0FF" stroke-width="1.7" stroke-linecap="square"/></svg>"##;

pub fn window_control<'a, Message>(control: WindowControl) -> Element<'a, Message>
where
    Message: 'a,
{
    let bytes = match control {
        WindowControl::Minimize => MINIMIZE,
        WindowControl::Maximize => MAXIMIZE,
        WindowControl::Close => CLOSE,
    };
    container(svg(svg::Handle::from_memory(bytes)).width(15).height(15))
        .width(Fill)
        .height(Fill)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .into()
}
