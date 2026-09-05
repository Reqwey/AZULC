use crate::domain::InstanceColor;
use iced::font::{Family, Stretch, Style as FontStyle, Weight};
use iced::widget::{
    button, checkbox, container, pick_list, progress_bar, scrollable, slider, text_input,
};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, border, theme::Palette};

// Violet Cartridge palette
pub const CANVAS: Color = Color::from_rgb8(0x0B, 0x09, 0x12);
pub const SIDEBAR: Color = Color::from_rgb8(0x13, 0x10, 0x1E);
pub const PANEL: Color = Color::from_rgb8(0x13, 0x10, 0x1E);
pub const PANEL_ALT: Color = Color::from_rgb8(0x21, 0x1A, 0x30);
pub const LAVENDER: Color = Color::from_rgb8(0xB8, 0x9C, 0xFF);
pub const SKY: Color = Color::from_rgb8(0x73, 0xC9, 0xFF);
pub const SUCCESS: Color = Color::from_rgb8(0x75, 0xE6, 0xB4);
pub const DANGER: Color = Color::from_rgb8(0xFF, 0x6F, 0x91);

pub const TEXT: Color = Color::from_rgb8(0xF5, 0xF0, 0xFF);
pub const MUTED: Color = Color::from_rgb8(0xC2, 0xB8, 0xD0);
pub const BORDER: Color = Color::from_rgb8(0x78, 0x67, 0x8E);
pub const LAVENDER_SOFT: Color = Color::from_rgb8(0xD8, 0xC8, 0xFF);
pub const WARNING: Color = Color::from_rgb8(0xFF, 0xD1, 0x75);

pub const fn instance_color(color: InstanceColor) -> Color {
    match color {
        InstanceColor::Lavender => LAVENDER,
        InstanceColor::Sky => SKY,
        InstanceColor::Mint => SUCCESS,
        InstanceColor::Amber => WARNING,
        InstanceColor::Rose => DANGER,
    }
}

const LAVENDER_WASH: Color = Color::from_rgba8(0xB8, 0x9C, 0xFF, 0.12);
const DANGER_WASH: Color = Color::from_rgba8(0xFF, 0x6F, 0x91, 0.10);
const DISABLED: Color = Color::from_rgb8(0x91, 0x86, 0x9E);
const WINDOW_CLOSE_HOVER: Color = Color::from_rgb8(0xC4, 0x2B, 0x1C);
const WINDOW_CLOSE_PRESSED: Color = Color::from_rgb8(0xA9, 0x23, 0x15);

pub const DISPLAY_FONT: Font = Font::with_name("Pixelify Sans");
pub const BODY_FONT: Font = Font::with_name("Space Mono");
pub const BODY_BOLD: Font = Font {
    family: Family::Name("Space Mono"),
    weight: Weight::Bold,
    stretch: Stretch::Normal,
    style: FontStyle::Normal,
};

pub fn azulc() -> Theme {
    Theme::custom(
        "Violet Cartridge",
        Palette {
            background: CANVAS,
            text: TEXT,
            primary: LAVENDER,
            success: SUCCESS,
            warning: WARNING,
            danger: DANGER,
        },
    )
}

fn outline(color: Color, width: f32, _radius: f32) -> Border {
    Border {
        color,
        width,
        radius: border::Radius::new(0.0),
    }
}

fn hard_shadow(color: Color, x: f32, y: f32) -> Shadow {
    Shadow {
        color,
        offset: Vector::new(x, y),
        blur_radius: 0.0,
    }
}

pub fn canvas(_: &Theme) -> container::Style {
    container::Style::default().background(CANVAS).color(TEXT)
}

pub fn modal_backdrop(_: &Theme) -> container::Style {
    container::Style::default().background(Color::from_rgba8(0x03, 0x02, 0x07, 0.84))
}

pub fn modal_panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        text_color: Some(TEXT),
        border: outline(LAVENDER, 2.0, 0.0),
        shadow: hard_shadow(Color::from_rgb8(0x4B, 0x38, 0x72), 8.0, 8.0),
        ..Default::default()
    }
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SIDEBAR)),
        text_color: Some(TEXT),
        border: outline(BORDER, 1.0, 0.0),
        ..Default::default()
    }
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        text_color: Some(TEXT),
        border: outline(BORDER, 1.0, 12.0),
        ..Default::default()
    }
}

pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_ALT)),
        text_color: Some(TEXT),
        border: outline(BORDER, 1.0, 8.0),
        ..Default::default()
    }
}

pub fn titlebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(CANVAS)),
        text_color: Some(TEXT),
        border: outline(BORDER, 1.0, 0.0),
        ..Default::default()
    }
}

pub fn hero(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        text_color: Some(TEXT),
        border: outline(LAVENDER, 2.0, 16.0),
        shadow: hard_shadow(LAVENDER, 6.0, 6.0),
        ..Default::default()
    }
}

pub fn stat_card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_ALT)),
        text_color: Some(TEXT),
        border: outline(BORDER, 1.0, 10.0),
        shadow: hard_shadow(CANVAS, 4.0, 4.0),
        ..Default::default()
    }
}

pub fn selected_card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(LAVENDER_WASH)),
        text_color: Some(TEXT),
        border: outline(LAVENDER, 2.0, 10.0),
        shadow: hard_shadow(LAVENDER, 3.0, 3.0),
        ..Default::default()
    }
}

pub fn danger_panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(DANGER_WASH)),
        text_color: Some(TEXT),
        border: outline(DANGER, 1.0, 10.0),
        ..Default::default()
    }
}

pub fn pill(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(LAVENDER_WASH)),
        text_color: Some(LAVENDER_SOFT),
        border: outline(LAVENDER, 1.0, 999.0),
        ..Default::default()
    }
}

pub fn nav_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered => (Some(Background::Color(PANEL_ALT)), TEXT),
        button::Status::Pressed => (Some(Background::Color(LAVENDER_WASH)), LAVENDER_SOFT),
        button::Status::Disabled => (None, DISABLED),
        button::Status::Active => (None, MUTED),
    };

    button::Style {
        background,
        text_color,
        border: outline(Color::TRANSPARENT, 0.0, 8.0),
        ..Default::default()
    }
}

pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color, shadow) = match status {
        button::Status::Active => (
            LAVENDER,
            CANVAS,
            hard_shadow(Color::from_rgb8(0x69, 0x51, 0xA8), 4.0, 4.0),
        ),
        button::Status::Hovered => (LAVENDER_SOFT, CANVAS, hard_shadow(LAVENDER, 4.0, 4.0)),
        button::Status::Pressed => (LAVENDER, CANVAS, Shadow::default()),
        button::Status::Disabled => (PANEL_ALT, DISABLED, Shadow::default()),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: outline(CANVAS, 1.0, 8.0),
        shadow,
        ..Default::default()
    }
}

pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color, border_color) = match status {
        button::Status::Hovered => (Some(Background::Color(PANEL_ALT)), TEXT, LAVENDER),
        button::Status::Pressed => (
            Some(Background::Color(LAVENDER_WASH)),
            LAVENDER_SOFT,
            LAVENDER,
        ),
        button::Status::Disabled => (None, DISABLED, BORDER),
        button::Status::Active => (None, TEXT, BORDER),
    };

    button::Style {
        background,
        text_color,
        border: outline(border_color, 1.0, 8.0),
        ..Default::default()
    }
}

pub fn color_swatch_button(
    _: &Theme,
    status: button::Status,
    accent: Color,
    selected: bool,
) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => PANEL_ALT,
        button::Status::Disabled | button::Status::Active => PANEL,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: if matches!(status, button::Status::Disabled) {
            DISABLED
        } else {
            TEXT
        },
        border: outline(
            if selected { accent } else { BORDER },
            if selected { 2.0 } else { 1.0 },
            0.0,
        ),
        ..Default::default()
    }
}

pub fn thumbnail_frame(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_ALT)),
        border: outline(BORDER, 1.0, 0.0),
        ..Default::default()
    }
}

pub fn window_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered => (Some(Background::Color(PANEL_ALT)), TEXT),
        button::Status::Pressed => (Some(Background::Color(LAVENDER_WASH)), LAVENDER),
        button::Status::Disabled => (None, DISABLED),
        button::Status::Active => (None, MUTED),
    };

    button::Style {
        background,
        text_color,
        border: outline(Color::TRANSPARENT, 0.0, 4.0),
        ..Default::default()
    }
}

pub fn danger_window_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Hovered => (Some(Background::Color(WINDOW_CLOSE_HOVER)), TEXT),
        button::Status::Pressed => (Some(Background::Color(WINDOW_CLOSE_PRESSED)), TEXT),
        button::Status::Disabled => (None, DISABLED),
        button::Status::Active => (None, MUTED),
    };

    button::Style {
        background,
        text_color,
        border: outline(Color::TRANSPARENT, 0.0, 4.0),
        ..Default::default()
    }
}

pub fn danger_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Active => (DANGER, TEXT),
        button::Status::Hovered => (WINDOW_CLOSE_HOVER, TEXT),
        button::Status::Pressed => (WINDOW_CLOSE_PRESSED, TEXT),
        button::Status::Disabled => (PANEL_ALT, DISABLED),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: outline(DANGER, 1.0, 0.0),
        ..Default::default()
    }
}

pub fn version_card_button(_: &Theme, status: button::Status) -> button::Style {
    let (background, border_color) = match status {
        button::Status::Hovered => (PANEL_ALT, LAVENDER),
        button::Status::Pressed => (LAVENDER_WASH, LAVENDER_SOFT),
        button::Status::Disabled => (PANEL, BORDER),
        button::Status::Active => (PANEL, BORDER),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: if matches!(status, button::Status::Disabled) {
            DISABLED
        } else {
            TEXT
        },
        border: outline(border_color, 1.0, 0.0),
        ..Default::default()
    }
}

pub fn square_text_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(PANEL_ALT);
    style.border = outline(
        if matches!(status, text_input::Status::Focused { .. }) {
            LAVENDER
        } else {
            BORDER
        },
        1.0,
        0.0,
    );
    style.icon = MUTED;
    style.placeholder = MUTED;
    style.value = TEXT;
    style.selection = LAVENDER_WASH;
    style
}

pub fn square_checkbox(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let mut style = checkbox::primary(theme, status);
    style.border.radius = border::Radius::new(0.0);
    style.border.color = BORDER;
    style.text_color = Some(TEXT);
    style
}

pub fn square_pick_list(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut style = pick_list::default(theme, status);
    style.background = Background::Color(PANEL_ALT);
    style.border = outline(BORDER, 1.0, 0.0);
    style.text_color = TEXT;
    style.placeholder_color = MUTED;
    style.handle_color = LAVENDER;
    style
}

pub fn square_slider(theme: &Theme, status: slider::Status) -> slider::Style {
    let mut style = slider::default(theme, status);
    style.rail.border.radius = border::Radius::new(0.0);
    style.handle.shape = slider::HandleShape::Rectangle {
        width: 12,
        border_radius: border::Radius::new(0.0),
    };
    style
}

pub fn square_progress(theme: &Theme) -> progress_bar::Style {
    let mut style = progress_bar::primary(theme);
    style.border.radius = border::Radius::new(0.0);
    style
}

pub fn square_scrollable(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.vertical_rail.border.radius = border::Radius::new(0.0);
    style.vertical_rail.scroller.border.radius = border::Radius::new(0.0);
    style.horizontal_rail.border.radius = border::Radius::new(0.0);
    style.horizontal_rail.scroller.border.radius = border::Radius::new(0.0);
    style.auto_scroll.border.radius = border::Radius::new(0.0);
    style
}
