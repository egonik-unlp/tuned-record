use tuned_record::audio::engine::request_low_latency;
use tuned_record::ui::app::App;

fn main() -> eframe::Result {
    // Before any thread exists: PipeWire's JACK layer reads this when the client opens.
    request_low_latency();
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
