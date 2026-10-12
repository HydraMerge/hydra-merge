use eframe::egui::ViewportBuilder;
use hydra_merge::app::WorktreeBarApp;
use hydra_merge::model::Config;
use hydra_merge::session::load_session;
use hydra_merge::theme::config_dir;

const WINDOW_TITLE: &str = "HydraMerge";

fn main() -> eframe::Result<()> {
    let config = Config::from_env();
    // The window opens where and how large it was last time.
    let saved = hydra_merge::os::home_dir()
        .map(|home| load_session(&config_dir(&home)))
        .and_then(|session| session.window);
    let viewport = ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size([1100.0, 680.0])
        .with_min_inner_size([760.0, 440.0])
        .with_icon(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo.png"))
                .expect("assets/logo.png"),
        );
    let viewport = match saved {
        Some([x, y, w, h]) => viewport.with_position([x, y]).with_inner_size([w, h]),
        None => viewport,
    };
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(WorktreeBarApp::new(cc, config)))),
    )
}
