//! The per-period cycle shared by every driver: commands, song clock, playback mix, and the
//! latency-compensated stamping of mic blocks for the live-pitch and disk threads.

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use super::{BlockHeader, BlockKind, Command, MicStream, Shared, Streams};
use crate::audio::mixer::Playback;
use crate::audio::speed::SongClock;

const COMMAND_CAPACITY: usize = 256;
const HEADER_CAPACITY: usize = 4096;
const LIVE_SECONDS: usize = 2;
const DISK_SECONDS: usize = 10;

struct StreamProducer {
    headers: rtrb::Producer<BlockHeader>,
    samples: rtrb::Producer<f32>,
}

impl StreamProducer {
    /// Writes samples then the header; returns false (writing nothing) if either ring is full.
    fn push(&mut self, header: BlockHeader, samples: &[f32]) -> bool {
        if self.headers.slots() == 0 || self.samples.slots() < samples.len() {
            return false;
        }
        if let Ok(mut chunk) = self.samples.write_chunk_uninit(samples.len()) {
            let (a, b) = chunk.as_mut_slices();
            let split = a.len();
            for (d, s) in a.iter_mut().zip(samples) {
                d.write(*s);
            }
            for (d, s) in b.iter_mut().zip(&samples[split..]) {
                d.write(*s);
            }
            // SAFETY: all `samples.len()` slots were initialised above.
            unsafe { chunk.commit_all() };
        }
        self.headers.push(header).is_ok()
    }
}

fn stream(capacity: usize) -> (StreamProducer, MicStream) {
    let (hp, hc) = rtrb::RingBuffer::new(HEADER_CAPACITY);
    let (sp, sc) = rtrb::RingBuffer::new(capacity);
    (
        StreamProducer {
            headers: hp,
            samples: sp,
        },
        MicStream {
            headers: hc,
            samples: sc,
        },
    )
}

/// The engine's side of a [`Cycle`]: where commands go in and retired snapshots come back.
pub(super) struct Control {
    pub commands: rtrb::Producer<Command>,
    pub garbage: rtrb::Consumer<Arc<Playback>>,
}

/// Everything one audio period does, independent of the audio API that drives it.
pub(super) struct Cycle {
    shared: Arc<Shared>,
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Arc<Playback>>,
    playback: Arc<Playback>,
    live: StreamProducer,
    disk: StreamProducer,
    clock: SongClock,
    wall: u64,
    playing: bool,
    recording: bool,
    loop_range: Option<(i64, i64)>,
}

impl Cycle {
    /// A stopped cycle at song frame 0, with its control rings and mic streams sized for `rate`.
    pub fn new(shared: Arc<Shared>, rate: u32) -> (Self, Control, Streams) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
        let (garbage_tx, garbage_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
        let (live_tx, live_rx) = stream(rate as usize * LIVE_SECONDS);
        let (disk_tx, disk_rx) = stream(rate as usize * DISK_SECONDS);
        let cycle = Self {
            shared,
            commands: cmd_rx,
            garbage: garbage_tx,
            playback: Arc::new(Playback::default()),
            live: live_tx,
            disk: disk_tx,
            clock: SongClock::default(),
            wall: 0,
            playing: false,
            recording: false,
            loop_range: None,
        };
        let control = Control {
            commands: cmd_tx,
            garbage: garbage_rx,
        };
        let streams = Streams {
            live: live_rx,
            disk: disk_rx,
        };
        (cycle, control, streams)
    }

    fn header(&self, kind: BlockKind, song_frame: i64, len: usize) -> BlockHeader {
        BlockHeader {
            kind,
            wall_frame: self.wall,
            song_frame,
            len: len as u32,
            playing: self.playing,
            speed: self.clock.ratio(),
        }
    }

    /// Song frame of the mic sample captured now: the compensation is wall time, so it covers
    /// fewer song frames when slowed.
    fn captured_frame(&self, comp: i64) -> i64 {
        self.clock.pos() - self.clock.ratio().to_song(comp)
    }

    fn retire(&mut self, old: Arc<Playback>) {
        if let Err(rtrb::PushError::Full(old)) = self.garbage.push(old) {
            // Never free on the audio thread; leaking is the lesser evil.
            std::mem::forget(old);
        }
    }

    fn stop_recording(&mut self, song_frame: i64) {
        if self.recording {
            self.recording = false;
            let h = self.header(BlockKind::TakeEnd, song_frame, 0);
            // The disk thread must see the end marker; retry is impossible here, so count a drop.
            if !self.disk.push(h, &[]) {
                self.shared.dropped_disk.fetch_add(1, Relaxed);
            }
        }
    }

    /// Ends a take in progress at the last captured frame, as a stop would; for a driver whose
    /// audio was interrupted, called while no period is running.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn end_take(&mut self) {
        let comp = self.shared.compensation.load(Relaxed);
        self.stop_recording(self.captured_frame(comp));
        self.shared.recording.store(false, Relaxed);
    }

    fn handle_commands(&mut self, comp: i64) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                Command::Play => self.playing = true,
                Command::Stop => {
                    self.stop_recording(self.captured_frame(comp));
                    self.playing = false;
                }
                Command::Seek(f) => {
                    if !self.recording {
                        self.clock.seek(f);
                    }
                }
                // Takes are recorded at 100 % only.
                Command::StartRecord => {
                    if !self.recording && self.clock.ratio().is_one() {
                        let h = self.header(BlockKind::TakeStart, self.captured_frame(comp), 0);
                        self.recording = self.disk.push(h, &[]);
                    }
                }
                Command::StopRecord => self.stop_recording(self.captured_frame(comp)),
                Command::SetLoop(range) => self.loop_range = range.filter(|(s, e)| s < e),
                Command::SetPlayback(new) => {
                    let old = std::mem::replace(&mut self.playback, new);
                    self.retire(old);
                }
                Command::SetSpeed { speed, playback } => {
                    if self.recording {
                        self.retire(playback);
                    } else {
                        self.clock.set_ratio(speed);
                        let old = std::mem::replace(&mut self.playback, playback);
                        self.retire(old);
                    }
                }
            }
        }
    }

    /// One period: `mic` was captured while `out_l`/`out_r` (same length) are about to be played.
    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    pub fn process(&mut self, mic: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        let comp = self.render(out_l, out_r);
        self.capture(mic, comp);
    }

    /// First half of a period: applies pending commands and mixes the playback into the outputs.
    /// Returns the compensation to hand to [`Cycle::capture`] for the same period.
    pub fn render(&mut self, out_l: &mut [f32], out_r: &mut [f32]) -> i64 {
        let comp = self.shared.compensation.load(Relaxed);
        self.handle_commands(comp);
        out_l.fill(0.0);
        out_r.fill(0.0);
        if self.playing {
            self.playback.mix_into(self.clock.pos(), out_l, out_r);
        }
        comp
    }

    /// Second half of a period: stamps and hands off the mic block (as long as the outputs just
    /// rendered), then advances the song clock and publishes the state.
    pub fn capture(&mut self, mic: &[f32], comp: i64) {
        let n = mic.len();
        let song_first = self.captured_frame(comp);
        let h = self.header(BlockKind::Data, song_first, n);
        if !self.live.push(h, mic) {
            self.shared.dropped_live.fetch_add(1, Relaxed);
        }
        if self.recording && self.playing && !self.disk.push(h, mic) {
            self.shared.dropped_disk.fetch_add(1, Relaxed);
        }

        if self.playing {
            self.clock.advance(n as u64);
            if let Some((start, end)) = self.loop_range
                && !self.recording
                && self.clock.pos() >= end
            {
                self.clock.seek(start);
            }
        }
        self.wall += n as u64;
        self.shared.song_pos.store(self.clock.pos(), Relaxed);
        self.shared
            .speed
            .store(self.clock.ratio().to_bits(), Relaxed);
        self.shared.playing.store(self.playing, Relaxed);
        self.shared.recording.store(self.recording, Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::speed::Ratio;

    const N: usize = 256;

    struct Rig {
        cycle: Cycle,
        control: Control,
        streams: Streams,
        shared: Arc<Shared>,
    }

    impl Rig {
        fn new() -> Self {
            let shared = Arc::new(Shared::default());
            let (cycle, control, streams) = Cycle::new(shared.clone(), 48_000);
            Self {
                cycle,
                control,
                streams,
                shared,
            }
        }

        fn send(&mut self, cmd: Command) {
            assert!(self.control.commands.push(cmd).is_ok());
        }

        fn period(&mut self) {
            let mic = [0.25f32; N];
            let (mut l, mut r) = ([0.0f32; N], [0.0f32; N]);
            self.cycle.process(&mic, &mut l, &mut r);
        }

        fn pos(&self) -> i64 {
            self.shared.song_pos.load(Relaxed)
        }

        fn disk_kinds(&mut self) -> Vec<BlockKind> {
            let mut buf = Vec::new();
            std::iter::from_fn(|| self.streams.disk.pop(&mut buf).map(|h| h.kind)).collect()
        }
    }

    fn playback() -> Arc<Playback> {
        Arc::new(Playback::default())
    }

    #[test]
    fn play_seek_stop() {
        let mut rig = Rig::new();
        rig.send(Command::Seek(1_000));
        rig.period();
        assert_eq!(rig.pos(), 1_000, "a stopped clock does not advance");
        rig.send(Command::Play);
        rig.period();
        assert_eq!(rig.pos(), 1_000 + N as i64);
        rig.send(Command::Stop);
        rig.period();
        assert_eq!(rig.pos(), 1_000 + N as i64);
        assert!(!rig.shared.playing.load(Relaxed));
    }

    #[test]
    fn loop_wraps_only_while_not_recording() {
        let mut rig = Rig::new();
        rig.send(Command::SetLoop(Some((0, 300))));
        rig.send(Command::Play);
        rig.period();
        rig.period();
        assert_eq!(rig.pos(), 0, "512 ≥ 300 wraps to the loop start");

        rig.send(Command::StartRecord);
        rig.period();
        rig.period();
        assert_eq!(rig.pos(), 2 * N as i64, "no wrap while recording");
    }

    #[test]
    fn take_is_framed_by_start_and_end_markers() {
        let mut rig = Rig::new();
        rig.send(Command::Play);
        rig.send(Command::StartRecord);
        rig.period();
        rig.period();
        rig.send(Command::StopRecord);
        rig.period();
        assert_eq!(
            rig.disk_kinds(),
            vec![
                BlockKind::TakeStart,
                BlockKind::Data,
                BlockKind::Data,
                BlockKind::TakeEnd
            ]
        );
        // The live stream gets every block, recording or not.
        let mut buf = Vec::new();
        let live = std::iter::from_fn(|| rig.streams.live.pop(&mut buf)).count();
        assert_eq!(live, 3);
    }

    #[test]
    fn recording_refused_below_full_speed() {
        let mut rig = Rig::new();
        rig.send(Command::SetSpeed {
            speed: Ratio::new(3, 5),
            playback: playback(),
        });
        rig.send(Command::Play);
        rig.send(Command::StartRecord);
        rig.period();
        assert!(!rig.shared.recording.load(Relaxed));
        assert!(rig.disk_kinds().is_empty());
    }

    #[test]
    fn speed_change_ignored_while_recording() {
        let mut rig = Rig::new();
        rig.send(Command::Play);
        rig.send(Command::StartRecord);
        rig.period();
        let slowed = playback();
        rig.send(Command::SetSpeed {
            speed: Ratio::new(1, 2),
            playback: slowed.clone(),
        });
        rig.period();
        assert!(Ratio::from_bits(rig.shared.speed.load(Relaxed)).is_one());
        let retired = rig
            .control
            .garbage
            .pop()
            .expect("refused playback handed back");
        assert!(Arc::ptr_eq(&retired, &slowed));
    }

    #[test]
    fn replaced_playback_is_handed_back() {
        let mut rig = Rig::new();
        let first = playback();
        rig.send(Command::SetPlayback(first.clone()));
        rig.send(Command::SetPlayback(playback()));
        rig.period();
        assert!(rig.control.garbage.pop().is_ok(), "the initial empty one");
        let retired = rig.control.garbage.pop().expect("first replaced");
        assert!(Arc::ptr_eq(&retired, &first));
    }

    #[test]
    fn end_take_closes_an_interrupted_recording() {
        let mut rig = Rig::new();
        rig.send(Command::Play);
        rig.send(Command::StartRecord);
        rig.period();
        rig.cycle.end_take();
        assert!(!rig.shared.recording.load(Relaxed));
        let mut buf = Vec::new();
        let mut last = None;
        while let Some(h) = rig.streams.disk.pop(&mut buf) {
            last = Some(h);
        }
        let end = last.unwrap();
        assert_eq!(end.kind, BlockKind::TakeEnd);
        // Stamped where the next block would have started: right after the last captured frame.
        assert_eq!(end.song_frame, N as i64);
        rig.cycle.end_take();
        assert!(rig.disk_kinds().is_empty(), "no second end marker");
    }

    #[test]
    fn interrupted_take_is_saved_by_the_disk_thread() {
        use crate::project::takes::{DiskControl, DiskEvent, spawn_disk_thread};
        use std::time::Duration;

        let shared = Arc::new(Shared::default());
        shared.compensation.store(100, Relaxed);
        let (mut cycle, mut control, streams) = Cycle::new(shared, 48_000);
        let dir =
            std::env::temp_dir().join(format!("tuned-record-end-take-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (ctl_tx, ctl_rx) = crossbeam_channel::unbounded();
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded();
        ctl_tx
            .send(DiskControl::SetTarget {
                takes_dir: dir.clone(),
                next_id: 1,
            })
            .unwrap();
        spawn_disk_thread(streams.disk, 48_000, ctl_rx, ev_tx);

        assert!(control.commands.push(Command::Play).is_ok());
        assert!(control.commands.push(Command::StartRecord).is_ok());
        let mic = [0.25f32; N];
        let (mut l, mut r) = ([0.0f32; N], [0.0f32; N]);
        for _ in 0..3 {
            cycle.process(&mic, &mut l, &mut r);
        }
        // The driver stops calling back; the supervisor closes the take.
        cycle.end_take();

        let next = || {
            ev_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("disk event")
        };
        match next() {
            DiskEvent::Started { start_frame, .. } => assert_eq!(start_frame, -100),
            e => panic!("unexpected {e:?}"),
        }
        match next() {
            DiskEvent::Finished {
                start_frame,
                frames,
                ..
            } => {
                assert_eq!(start_frame, -100, "aligned like any other take");
                assert_eq!(frames, 3 * N as u64, "everything captured before the reset");
            }
            e => panic!("unexpected {e:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
