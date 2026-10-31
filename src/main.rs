// No console window on Windows (errors are shown in the app; panics go to `crash.log`).
#![cfg_attr(windows, windows_subsystem = "windows")]

use tuned_record::ui::app::App;

fn main() -> eframe::Result {
    // Before any thread exists: PipeWire's JACK layer reads this when the client opens.
    #[cfg(target_os = "linux")]
    tuned_record::audio::engine::request_low_latency();
    #[cfg(windows)]
    log_panics_to_file();
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tuned-record")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    // Optional argument: a project folder to open.
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    eframe::run_native(
        "tuned-record",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc, open)))),
    )
}

/// Writes panics to `%APPDATA%\tuned-record\crash.log`: without a console they would vanish.
#[cfg(windows)]
fn log_panics_to_file() {
    let Some(path) = dirs::config_dir().map(|d| d.join("tuned-record").join("crash.log")) else {
        return;
    };
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(&path, format!("{info}\n"));
        default(info);
    }));
}
