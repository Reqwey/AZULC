use crate::theme;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{Space, Stack, container, image as iced_image, row, text};
use iced::{Alignment, Element, Fill, Padding};
use std::sync::OnceLock;

const HERO_HEIGHT: f32 = 258.0;
const WORDMARK_WIDTH: f32 = 540.0;
const WORDMARK_HEIGHT: f32 = 132.0;
const ICON_SIZE: usize = 96;
const ICON_OUTER_RADIUS: f32 = 46.0;
const ICON_INNER_RADIUS: f32 = 43.0;
const ICON_BORDER: ::image::Rgba<u8> = ::image::Rgba([0xB8, 0x9C, 0xFF, 0xFF]);
const GIRL_SOURCE: &[u8] = include_bytes!("../../assets/brand/girl-source.png");
static SILHOUETTE: OnceLock<iced_image::Handle> = OnceLock::new();

/// Builds the taskbar/window icon from the supplied silhouette source.
pub(crate) fn window_icon() -> Option<iced::window::Icon> {
    let icon = window_icon_image()?;
    iced::window::icon::from_rgba(icon.into_raw(), ICON_SIZE as u32, ICON_SIZE as u32).ok()
}

fn window_icon_image() -> Option<::image::RgbaImage> {
    let source = tinted_source()?;
    let avatar =
        ::image::imageops::resize(&source, 66, 69, ::image::imageops::FilterType::Lanczos3);
    let mut icon = ::image::RgbaImage::from_pixel(
        ICON_SIZE as u32,
        ICON_SIZE as u32,
        ::image::Rgba([0x13, 0x10, 0x1E, 0xFF]),
    );
    ::image::imageops::overlay(&mut icon, &avatar, 15, 13);

    let center = ICON_SIZE as f32 / 2.0;
    for (x, y, pixel) in icon.enumerate_pixels_mut() {
        let x = x as f32 + 0.5;
        let y = y as f32 + 0.5;
        if !inside_regular_hexagon(x, y, center, ICON_OUTER_RADIUS) {
            *pixel = ::image::Rgba([0, 0, 0, 0]);
        } else if !inside_regular_hexagon(x, y, center, ICON_INNER_RADIUS) {
            *pixel = ICON_BORDER;
        }
    }
    Some(icon)
}

fn inside_regular_hexagon(x: f32, y: f32, center: f32, radius: f32) -> bool {
    let dx = (x - center).abs();
    let dy = (y - center).abs();
    let sqrt_three = 3.0_f32.sqrt();
    dx <= sqrt_three * radius / 2.0 && dy <= radius && dx / sqrt_three + dy <= radius
}

/// Builds the large AZULC wordmark and exact supplied-avatar silhouette lockup.
pub(super) fn view<'a, Message>() -> Element<'a, Message>
where
    Message: 'a,
{
    let content = row![wordmark(), Space::new().width(Fill), avatar()]
        .align_y(Alignment::Center)
        .width(Fill);

    container(content)
        .width(Fill)
        .height(HERO_HEIGHT)
        .padding(Padding::from([18.0, 32.0]))
        .style(theme::hero)
        .into()
}

fn wordmark<'a, Message>() -> Element<'a, Message>
where
    Message: 'a,
{
    let layers: Stack<'a, Message> = Stack::new()
        .width(WORDMARK_WIDTH)
        .height(WORDMARK_HEIGHT)
        .push(Space::new().width(WORDMARK_WIDTH).height(WORDMARK_HEIGHT))
        .push(wordmark_layer(theme::CANVAS, 12.0, 22.0))
        .push(wordmark_layer(theme::MUTED, 7.0, 17.0))
        .push(wordmark_layer(theme::LAVENDER, 3.0, 13.0))
        .push(wordmark_layer(theme::LAVENDER_SOFT, 0.0, 9.0));

    layers.into()
}

fn wordmark_layer<'a, Message>(color: iced::Color, left: f32, top: f32) -> Element<'a, Message>
where
    Message: 'a,
{
    container(
        text("AZULC")
            .font(theme::DISPLAY_FONT)
            .size(104)
            .line_height(1.0)
            .color(color),
    )
    .width(Fill)
    .height(Fill)
    .padding(Padding {
        top,
        right: 0.0,
        bottom: 0.0,
        left,
    })
    .align_x(Horizontal::Left)
    .align_y(Vertical::Top)
    .into()
}

fn avatar<'a, Message>() -> Element<'a, Message>
where
    Message: 'a,
{
    container(
        iced_image(silhouette_handle())
            .width(214)
            .height(218)
            .content_fit(iced::ContentFit::Contain),
    )
    .width(226)
    .height(226)
    .align_x(Horizontal::Center)
    .align_y(Vertical::Center)
    .into()
}

fn silhouette_handle() -> iced_image::Handle {
    SILHOUETTE
        .get_or_init(|| {
            let source =
                tinted_source().expect("embedded girl silhouette source must be valid PNG");
            let (width, height) = source.dimensions();
            iced_image::Handle::from_rgba(width, height, source.into_raw())
        })
        .clone()
}

fn tinted_source() -> Option<::image::RgbaImage> {
    let mut source = ::image::load_from_memory(GIRL_SOURCE).ok()?.into_rgba8();
    for pixel in source.pixels_mut() {
        let alpha = pixel[3];
        *pixel = ::image::Rgba([0xD8, 0xC8, 0xFF, alpha]);
    }
    Some(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_window_icon_has_valid_rgba_dimensions() {
        let (rgba, size) = window_icon()
            .expect("silhouette icon should be valid")
            .into_raw();
        assert_eq!(size.width, ICON_SIZE as u32);
        assert_eq!(size.height, ICON_SIZE as u32);
        assert_eq!(rgba.len(), ICON_SIZE * ICON_SIZE * 4);
    }

    #[test]
    fn window_icon_has_a_transparent_exterior_and_purple_hex_border() {
        let icon = window_icon_image().expect("window icon image");
        assert_eq!(icon.get_pixel(0, 0), &::image::Rgba([0, 0, 0, 0]));
        assert_eq!(icon.get_pixel(48, 2), &ICON_BORDER);
        assert_ne!(icon.get_pixel(48, 48), &ICON_BORDER);
    }

    #[test]
    fn silhouette_preserves_exterior_and_generated_cutouts() {
        let source = ::image::load_from_memory(GIRL_SOURCE)
            .expect("source PNG")
            .into_rgba8();
        let tinted = tinted_source().expect("tinted PNG");
        assert_eq!(source.dimensions(), tinted.dimensions());
        assert_eq!(source.get_pixel(300, 30)[3], 0);
        assert_eq!(tinted.get_pixel(55, 248)[3], 0);
        assert!(tinted.get_pixel(20, 248)[3] > 0xF0);
        assert_eq!(tinted.get_pixel(220, 405)[3], 0);
        assert_eq!(tinted.get_pixel(390, 400)[3], 0);
    }

    #[test]
    fn silhouette_face_contains_only_the_intended_mouth_gap() {
        let tinted = tinted_source().expect("tinted PNG");
        assert_eq!(tinted.get_pixel(204, 315)[3], 0xFF);
        assert_eq!(tinted.get_pixel(140, 403)[3], 0xFF);
        assert!(tinted.get_pixel(290, 477)[3] < 0x80);
        assert_eq!(tinted.get_pixel(300, 300)[3], 0xFF);
    }

    #[test]
    fn silhouette_uses_one_color() {
        let tinted = tinted_source().expect("tinted PNG");
        assert!(
            tinted
                .pixels()
                .filter(|pixel| pixel[3] > 0)
                .all(|pixel| pixel.0[..3] == [0xD8, 0xC8, 0xFF])
        );
    }
}
