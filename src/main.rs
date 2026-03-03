mod app;
mod engines;
mod benchmarks;
mod model;
mod util;

use iced::{Application, Settings};

fn main() -> iced::Result {
    let settings = Settings {
        window: iced::window::Settings {
            size: iced::Size::new(960.0, 720.0),
            ..Default::default()
        },
        ..Default::default()
    };

    app::ui::OBenchmarkApp::run(settings)
}