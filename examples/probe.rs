//! Starts the engine, prints what it negotiated, and checks the callback runs.
//! Run with: cargo run --example probe
use std::sync::atomic::Ordering::Relaxed;
use tuned_record::audio::engine::Engine;

fn main() {
    #[cfg(target_os = "linux")]
    tuned_record::audio::engine::request_low_latency();
    let (mut engine, mut streams) = match Engine::start() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    std::thread::sleep(std::time::Duration::from_millis(500));
    engine.refresh_latency(0);
    let mut buf = Vec::new();
    let mut blocks = 0;
    while streams.live.pop(&mut buf).is_some() {
        blocks += 1;
    }
    println!(
        "rate {} Hz, buffer {} frames",
        engine.sample_rate,
        engine.buffer_size()
    );
    println!(
        "latency: playback {} + capture {} frames = {:.1} ms",
        engine.playback_latency,
        engine.capture_latency,
        engine.latency_ms()
    );
    println!("connections: {:?}", engine.routing);
    println!("mic sources: {:?}", engine.input_sources());
    println!("output devices: {:?}", engine.output_devices());
    println!(
        "mic blocks in 0.5 s: {blocks}, xruns {}",
        engine.shared.xruns.load(Relaxed)
    );
}
