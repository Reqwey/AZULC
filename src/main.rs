#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod domain;
#[cfg(test)]
mod dotenv_file;
mod environment;
mod services;
mod storage;
mod theme;
mod ui;

fn main() -> iced::Result {
    let mut window_settings = iced::window::Settings {
        size: iced::Size::new(1360.0, 820.0),
        min_size: Some(iced::Size::new(1100.0, 700.0)),
        decorations: false,
        resizable: true,
        icon: ui::brand::window_icon(),
        ..Default::default()
    };
    #[cfg(target_os = "windows")]
    {
        window_settings.platform_specific.corner_preference =
            iced::window::settings::platform::CornerPreference::DoNotRound;
    }

    iced::application(
        app::Launcher::new,
        app::Launcher::update,
        app::Launcher::view,
    )
    .title(app::Launcher::title)
    .subscription(app::Launcher::subscription)
    .theme(app::Launcher::theme)
    .font(include_bytes!("../assets/fonts/PixelifySans-Variable.ttf").as_slice())
    .font(include_bytes!("../assets/fonts/SpaceMono-Regular.ttf").as_slice())
    .font(include_bytes!("../assets/fonts/SpaceMono-Bold.ttf").as_slice())
    .default_font(theme::DISPLAY_FONT)
    .window(window_settings)
    .centered()
    .run()
}
