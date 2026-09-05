use crate::{services::catalog::CatalogProvider, theme};
use iced::widget::{button, row, text};
use iced::{Alignment, Element};

pub(in crate::ui) fn provider_tabs<Message>(
    selected: CatalogProvider,
    on_select: fn(CatalogProvider) -> Message,
) -> Element<'static, Message>
where
    Message: Clone + 'static,
{
    let mut tabs = row![
        text("DOWNLOAD SOURCE")
            .font(theme::BODY_BOLD)
            .size(11)
            .color(theme::MUTED)
    ]
    .spacing(7)
    .align_y(Alignment::Center);
    for provider in CatalogProvider::ALL {
        tabs = tabs.push(
            button(text(provider.label().to_uppercase()).size(13))
                .on_press(on_select(provider))
                .padding([8, 12])
                .style(if selected == provider {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
        );
    }
    tabs.into()
}

pub(in crate::ui) fn short_date(value: &str) -> &str {
    value.split('T').next().unwrap_or(value)
}

pub(in crate::ui) fn compact_number(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

pub(in crate::ui) fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

pub(in crate::ui) fn release_label(release_type: u8) -> &'static str {
    match release_type {
        1 => "RELEASE",
        2 => "BETA",
        3 => "ALPHA",
        _ => "BUILD",
    }
}

#[cfg(test)]
mod tests {
    use super::{compact_number, format_bytes, release_label, short_date};

    #[test]
    fn short_date_returns_the_date_before_the_time_separator() {
        assert_eq!(short_date("2026-09-05T12:34:56Z"), "2026-09-05");
    }

    #[test]
    fn short_date_preserves_a_value_without_a_time_separator() {
        assert_eq!(short_date("2026-09-05"), "2026-09-05");
    }

    #[test]
    fn compact_number_preserves_the_existing_thousands_format() {
        assert_eq!(compact_number(1_250), "1.2K");
    }

    #[test]
    fn format_bytes_preserves_the_existing_binary_unit_format() {
        assert_eq!(format_bytes(1_536), "1.5 KiB");
    }

    #[test]
    fn release_label_preserves_the_existing_alpha_mapping() {
        assert_eq!(release_label(3), "ALPHA");
    }
}
