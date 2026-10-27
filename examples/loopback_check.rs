//! Loopback alignment check (spec: audio-engine → "Loopback alignment").
//!
//! Runs the latency calibration, records a take over a click backing, exports it, and measures
//! the exported take against the backing.
//!
//!   cargo run --release --example loopback_check
//!       Patches the app's output straight into its input inside JACK (speakers and mic are
//!       disconnected, nothing is heard). Exercises the whole record/export path, but the
//!       self-patch has no delay, so it does not exercise the latency compensation.
//!   … -- --long
//!       Records a 5-minute take and also checks for dropped blocks/xruns and that the clicks at
//!       the end of the take are still aligned (any lost audio would shift them).
//!
//! Then checks practice speed: a stepped tone, stretched for 60 %, played back at 60 % through
//! the same loopback; the live pitch thread must place each step within 5 ms of song time.
//!
//!   cargo run --release --example loopback_check -- --physical
//!       Keeps the default hardware connections: plug a cable from the audio output into the
//!       input first (the clicks go out through the output). This is the real end-to-end test.

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, unbounded};
use tuned_record::audio::engine::{Command, Engine, request_low_latency};
use tuned_record::audio::latency::{click_track, residual_by_xcorr};
use tuned_record::audio::mixer::{Audio, Playback, Track};
use tuned_record::audio::speed::Ratio;
use tuned_record::audio::stretch;
use tuned_record::pitch::level::InputLevel;
use tuned_record::pitch::live::{LiveDetector, LiveFrame};
use tuned_record::pitch::live_thread::{self, LivePoint};
use tuned_record::pitch::note::hz_to_midi;
use tuned_record::project::TakeEntry;
use tuned_record::project::audio_io::read_wav;
use tuned_record::project::export::{ExportInput, export};
use tuned_record::project::takes::{DiskControl, DiskEvent, spawn_disk_thread, take_path};

fn main() {
    request_low_latency();
    let (mut engine, streams) = Engine::start().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let rate = engine.sample_rate;
    let (in_port, outs) = {
        let (i, o) = engine.port_names();
        (i.to_string(), o.clone())
    };
    let physical = std::env::args().any(|a| a == "--physical");
    if physical {
        println!("physical loopback: {:?}", engine.routing);
    } else {
        let c = engine.client();
        for p in c.ports(None, None, jack::PortFlags::empty()) {
            let _ = c.disconnect_ports_by_name(&p, &in_port);
            for o in &outs {
                let _ = c.disconnect_ports_by_name(o, &p);
            }
        }
        c.connect_ports_by_name(&outs[0], &in_port)
            .expect("patch out_l → mic_in");
    }
    std::thread::sleep(Duration::from_millis(300));
    engine.refresh_latency(0);
    println!(
        "rate {rate} Hz, buffer {} · reported playback {} + capture {} frames",
        engine.buffer_size(),
        engine.playback_latency,
        engine.capture_latency
    );

    let dir = std::env::temp_dir().join(format!("tuned-record-loopback-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (ctl_tx, ctl_rx) = unbounded();
    let (ev_tx, ev_rx) = unbounded();
    spawn_disk_thread(streams.disk, rate, ctl_rx, ev_tx);
    // The app's live-pitch thread drains the live stream (its points matter for the speed check).
    let (live_tx, live_rx) = unbounded();
    live_thread::spawn(
        streams.live,
        rate,
        live_tx,
        Arc::new(InputLevel::new(rate)),
        || {},
    );
    ctl_tx
        .send(DiskControl::SetTarget {
            takes_dir: dir.clone(),
            next_id: 1,
        })
        .unwrap();

    // 1. Calibration: residual on top of the reported latencies.
    let click = Arc::new(Audio::mono(click_track(rate as usize / 2, 4)));
    ctl_tx.send(DiskControl::CaptureToMemory).unwrap();
    record_over(&mut engine, &click);
    let (start, samples) = match wait(&ev_rx) {
        DiskEvent::Captured {
            start_frame,
            samples,
        } => (start_frame, samples),
        e => panic!("unexpected {e:?}"),
    };
    let cpeak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    println!(
        "capture: start {start}, {} frames, peak {cpeak:.2}, first click at {:?}",
        samples.len(),
        samples.iter().position(|v| v.abs() > 0.1)
    );
    let residual = residual_by_xcorr(
        &click.samples,
        &place(&samples, start, click.frames()),
        rate as usize / 10,
    )
    .expect("no click found in the loopback capture");
    println!(
        "calibration residual: {residual:+} frames ({:+.2} ms)",
        residual as f32 * 1000.0 / rate as f32
    );
    engine.refresh_latency(residual);

    // 2. A take over a different click backing, then export.
    let long = std::env::args().any(|a| a == "--long");
    let clicks = if long { 3 * 300 } else { 6 };
    let backing = Arc::new(Audio::mono(click_track(rate as usize / 3, clicks)));
    record_over(&mut engine, &backing);
    let (id, start_frame) = match wait(&ev_rx) {
        DiskEvent::Started { id, start_frame } => (id, start_frame),
        e => panic!("unexpected {e:?}"),
    };
    assert!(matches!(wait(&ev_rx), DiskEvent::Finished { .. }));
    let (take, _) = read_wav(&take_path(&dir, id, "wav")).unwrap();
    let peak = take.samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let first = take.samples.iter().position(|v| v.abs() > 0.1);
    println!(
        "take {id}: start {start_frame}, {} frames, peak {peak:.2}, first click at {first:?}",
        take.frames()
    );
    let input = ExportInput {
        name: "loopback".into(),
        rate,
        backing: Some(backing.clone()),
        stem_frames: 0,
        takes: vec![(TakeEntry::new(id, start_frame), Arc::new(take))],
    };
    let out = dir.join("export");
    export(&input, &out, |_| {}).unwrap();
    let (exported, _) = read_wav(&out.join(format!("loopback-take-{id:03}.wav"))).unwrap();
    // Correlate the first 10 s only (a full 5-minute correlation would take minutes).
    let head = backing.frames().min(10 * rate as usize);
    let off = residual_by_xcorr(
        &backing.samples[..head],
        &exported.samples[..head],
        rate as usize / 10,
    )
    .expect("no clicks in take");
    let ms = off as f32 * 1000.0 / rate as f32;
    println!("exported take vs backing: {off:+} frames ({ms:+.3} ms)");
    let mut ok = ms.abs() <= 1.0;
    if long {
        let tail = 10 * rate as usize;
        let (b, e) = (&backing.samples, &exported.samples);
        let tail_off = residual_by_xcorr(
            &b[b.len() - tail..],
            &e[b.len() - tail..b.len()],
            rate as usize / 10,
        );
        let (xruns, dropped) = (
            engine.shared.xruns.load(Relaxed),
            engine.shared.dropped_disk.load(Relaxed),
        );
        println!(
            "long take: {:.1} s recorded, last 10 s offset {tail_off:?} frames, {xruns} xruns, {dropped} blocks lost from the take",
            exported.frames() as f32 / rate as f32
        );
        ok &= tail_off == Some(off) && xruns == 0 && dropped == 0;
    }
    ok &= speed_check(&mut engine, &live_rx);
    let _ = std::fs::remove_dir_all(&dir);
    if ok {
        println!("PASS");
    } else {
        println!("FAIL");
        std::process::exit(2);
    }
}

/// Plays `audio` from song frame 0 while recording, until it has passed.
fn record_over(engine: &mut Engine, audio: &Arc<Audio>) {
    let pb = Playback {
        tracks: vec![Track {
            audio: audio.clone(),
            start: 0,
            gain: 1.0,
            speed: Ratio::ONE,
        }],
    };
    for cmd in [
        Command::Stop,
        Command::Seek(0),
        Command::SetPlayback(Arc::new(pb)),
        Command::Play,
        Command::StartRecord,
    ] {
        assert!(engine.send(cmd));
    }
    let stop_at = audio.frames() as i64 + engine.sample_rate as i64 / 4;
    // Until the engine reports recording, `song_pos` may predate the seek to 0.
    let t = Instant::now();
    while !engine.shared.recording.load(Relaxed) {
        std::thread::sleep(Duration::from_millis(1));
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "engine never started recording (is the graph running?)"
        );
    }
    while engine.shared.song_pos.load(Relaxed) < stop_at {
        std::thread::sleep(Duration::from_millis(10));
        let limit =
            Duration::from_secs_f32(audio.frames() as f32 / engine.sample_rate as f32 + 10.0);
        assert!(
            t.elapsed() < limit,
            "transport stalled at {}",
            engine.shared.song_pos.load(Relaxed)
        );
    }
    engine.send(Command::StopRecord);
    engine.send(Command::Stop);
    engine.collect_garbage();
}

/// Steps of a phase-continuous tone, 300 ms each.
const STEPS_HZ: [f32; 5] = [220.0, 262.0, 233.0, 294.0, 247.0];
const STEP_SECS: f32 = 0.3;

/// Plays a stepped tone stretched for 60 % at 60 % and compares where the live pitch thread puts
/// each step (song frames) with where the detector puts it in the unstretched tone.
fn speed_check(engine: &mut Engine, live: &Receiver<LivePoint>) -> bool {
    let rate = engine.sample_rate;
    let speed = Ratio::new(3, 5);
    let step = (STEP_SECS * rate as f32) as usize;
    let mut phase = 0.0f32;
    let mut tone = Vec::with_capacity(step * STEPS_HZ.len());
    for hz in STEPS_HZ {
        for _ in 0..step {
            tone.push(0.5 * phase.sin());
            phase = (phase + std::f32::consts::TAU * hz / rate as f32) % std::f32::consts::TAU;
        }
    }
    let tone = Audio::mono(tone);
    // Where the detector itself sees the steps, on the song timeline.
    let mut det = LiveDetector::new(rate);
    let mut frames: Vec<LiveFrame> = Vec::new();
    for (i, c) in tone.samples.chunks(256).enumerate() {
        det.push(c, (i * 256) as i64, &mut frames);
    }
    let expected = steps(frames.iter().map(|f| (f.frame, f.hz)));

    let lead = rate as i64 / 2;
    let stretched = stretch::render(&tone, rate, speed, |_| true).expect("render");
    let pb = Playback {
        tracks: vec![Track {
            audio: Arc::new(stretched),
            start: lead,
            gain: 1.0,
            speed,
        }],
    };
    for cmd in [
        Command::Stop,
        Command::Seek(0),
        Command::SetSpeed {
            speed,
            playback: Arc::new(pb),
        },
        Command::Play,
    ] {
        assert!(engine.send(cmd));
    }
    let end = lead + tone.frames() as i64 + rate as i64 / 4;
    let t = Instant::now();
    // Until the engine reports the new speed and playing, `song_pos` may predate the seek to 0.
    while !(engine.speed() == speed && engine.shared.playing.load(Relaxed)) {
        std::thread::sleep(Duration::from_millis(1));
        assert!(t.elapsed() < Duration::from_secs(5), "engine never started");
    }
    while engine.shared.song_pos.load(Relaxed) < end {
        std::thread::sleep(Duration::from_millis(10));
        assert!(t.elapsed() < Duration::from_secs(10), "transport stalled");
    }
    engine.send(Command::Stop);
    engine.send(Command::SetSpeed {
        speed: Ratio::ONE,
        playback: Arc::new(Playback::default()),
    });
    std::thread::sleep(Duration::from_millis(100));
    engine.collect_garbage();
    let points: Vec<LivePoint> = live.try_iter().collect();
    let got = steps(
        points
            .iter()
            .filter_map(|p| Some((p.song_frame? - lead, p.hz)))
            .filter(|(f, _)| *f >= 0),
    );
    if got.len() != expected.len() {
        println!(
            "speed 60 %: found {} pitch steps, expected {}",
            got.len(),
            expected.len()
        );
        return false;
    }
    let errs: Vec<f32> = got
        .iter()
        .zip(&expected)
        .map(|(g, e)| (g - e) as f32 * 1000.0 / rate as f32)
        .collect();
    let worst = errs.iter().fold(0f32, |m, e| m.max(e.abs()));
    println!("speed 60 %: live pitch steps vs song time (ms) {errs:.2?}, worst {worst:.2} ms");
    worst <= 5.0
}

/// Song frames where the pitch crosses the midpoint between consecutive steps, in order.
fn steps(points: impl Iterator<Item = (i64, Option<f32>)>) -> Vec<f64> {
    let voiced: Vec<(i64, f32)> = points
        .filter_map(|(f, hz)| Some((f, hz_to_midi(hz?))))
        .collect();
    let mut out = Vec::new();
    let mut from = 0;
    for w in STEPS_HZ.windows(2) {
        let mid = (hz_to_midi(w[0]) + hz_to_midi(w[1])) / 2.0;
        let up = w[1] > w[0];
        let found = (from + 1..voiced.len()).find(|&i| {
            let (a, b) = (voiced[i - 1], voiced[i]);
            let crosses = if up {
                a.1 < mid && b.1 >= mid
            } else {
                a.1 > mid && b.1 <= mid
            };
            crosses && b.0 - a.0 < 4_800
        });
        let Some(i) = found else { break };
        let (a, b) = (voiced[i - 1], voiced[i]);
        out.push(a.0 as f64 + (mid - a.1) as f64 / (b.1 - a.1) as f64 * (b.0 - a.0) as f64);
        from = i;
    }
    out
}

fn wait(rx: &Receiver<DiskEvent>) -> DiskEvent {
    let t = Instant::now();
    loop {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(100)) {
            return e;
        }
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "timed out waiting for the disk thread"
        );
    }
}

/// Places captured samples on the song timeline `[0, len)`.
fn place(samples: &[f32], start: i64, len: usize) -> Vec<f32> {
    let mut v = vec![0.0; len];
    for (i, s) in samples.iter().enumerate() {
        let f = start + i as i64;
        if f >= 0 && (f as usize) < len {
            v[f as usize] = *s;
        }
    }
    v
}
