use crate::{
    app::Message,
    domain::{InstanceColor, LoaderKind},
    theme,
};
use iced::widget::{button, container, image as iced_image, row, text};
use iced::{Alignment, Element, Length, alignment};
use std::sync::OnceLock;

const VANILLA_BYTES: &[u8] = include_bytes!("../../../assets/loaders/JEIcon_Release.png");
const FABRIC_BYTES: &[u8] = include_bytes!("../../../assets/loaders/Fabric.png");
const FORGE_BYTES: &[u8] = include_bytes!("../../../assets/loaders/Forge.png");
const NEOFORGE_BYTES: &[u8] = include_bytes!("../../../assets/loaders/NeoForge.png");

static VANILLA: OnceLock<iced_image::Handle> = OnceLock::new();
static FABRIC: OnceLock<iced_image::Handle> = OnceLock::new();
static FORGE: OnceLock<iced_image::Handle> = OnceLock::new();
static NEOFORGE: OnceLock<iced_image::Handle> = OnceLock::new();

pub(in crate::ui) fn instance_marker<'a>(color: InstanceColor, size: u32) -> Element<'a, Message> {
    text("◆")
        .size(size)
        .color(theme::instance_color(color))
        .into()
}

pub(in crate::ui) fn instance_color_picker(
    selected: InstanceColor,
    on_pick: impl Fn(InstanceColor) -> Message,
) -> Element<'static, Message> {
    let mut colors = row![].spacing(7).align_y(Alignment::Center);
    for color in InstanceColor::ALL {
        let accent = theme::instance_color(color);
        colors = colors.push(
            button(
                row![
                    text(if color == selected { "◆" } else { "◇" })
                        .size(17)
                        .color(accent),
                    text(color.label()).size(12)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .width(Length::FillPortion(1))
            .padding([8, 9])
            .on_press(on_pick(color))
            .style(move |theme, status| {
                theme::color_swatch_button(theme, status, accent, color == selected)
            }),
        );
    }
    colors.into()
}

pub(in crate::ui) fn loader_icon<'a>(kind: LoaderKind, size: f32) -> Element<'a, Message> {
    iced_image(loader_handle(kind))
        .width(size)
        .height(size)
        .content_fit(iced::ContentFit::Contain)
        .into()
}

pub(in crate::ui) fn thumbnail<'a>(
    handle: Option<&iced_image::Handle>,
    fallback: &'static str,
) -> Element<'a, Message> {
    let content: Element<'a, Message> = match handle {
        Some(handle) => iced_image(handle.clone())
            .width(32)
            .height(32)
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => text(fallback).size(20).color(theme::LAVENDER).into(),
    };
    container(content)
        .width(40)
        .height(40)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center)
        .style(theme::thumbnail_frame)
        .into()
}

fn loader_handle(kind: LoaderKind) -> iced_image::Handle {
    let (slot, bytes) = match kind {
        LoaderKind::Vanilla => (&VANILLA, VANILLA_BYTES),
        LoaderKind::Fabric => (&FABRIC, FABRIC_BYTES),
        LoaderKind::Forge => (&FORGE, FORGE_BYTES),
        LoaderKind::NeoForge => (&NEOFORGE, NEOFORGE_BYTES),
    };
    slot.get_or_init(|| iced_image::Handle::from_bytes(bytes))
        .clone()
}
