use crate::theme;
use iced::Color;

pub(super) fn display_game_versions(versions: &[String]) -> String {
    let displayed = versions
        .iter()
        .filter(|version| {
            version
                .chars()
                .next()
                .is_some_and(|char| char.is_ascii_digit())
        })
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>();
    if displayed.is_empty() {
        "unspecified".into()
    } else {
        displayed.join(", ")
    }
}

pub(super) fn release_glyph(kind: u8) -> &'static str {
    match kind {
        1 => "◆",
        2 => "◇",
        3 => "△",
        _ => "·",
    }
}

pub(super) fn release_color(kind: u8) -> Color {
    match kind {
        1 => theme::SUCCESS,
        2 => theme::WARNING,
        3 => theme::DANGER,
        _ => theme::MUTED,
    }
}
