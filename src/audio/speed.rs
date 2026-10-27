//! Practice speed: exact playback ratios, and the song clock that runs at them.
//!
//! A speed is a ratio `p/q` of song frames per output frame. Keeping it rational lets the song
//! position stay an integer frame count that never drifts, and makes `1/1` exactly the
//! unslowed clock (see design.md → Practice speed).

/// A playback speed as an exact ratio of song frames to output (wall) frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio {
    pub p: u32,
    pub q: u32,
}

impl Ratio {
    pub const ONE: Ratio = Ratio { p: 1, q: 1 };

    pub const fn new(p: u32, q: u32) -> Self {
        Self { p, q }
    }

    pub fn is_one(self) -> bool {
        self.p == self.q
    }

    pub fn percent(self) -> u32 {
        self.p * 100 / self.q
    }

    /// Wall (output) frames → song frames, rounded down.
    pub fn to_song(self, wall: i64) -> i64 {
        (wall * self.p as i64).div_euclid(self.q as i64)
    }

    /// Song frames → frames of audio rendered at this speed, rounded down.
    pub fn to_wall(self, song: i64) -> i64 {
        (song * self.q as i64).div_euclid(self.p as i64)
    }

    /// Packs into one word for an atomic.
    pub fn to_bits(self) -> u32 {
        (self.p << 16) | self.q
    }

    /// Inverse of [`Ratio::to_bits`]; an unset (zero) word reads as `1/1`.
    pub fn from_bits(bits: u32) -> Self {
        let r = Self::new(bits >> 16, bits & 0xffff);
        if r.p == 0 || r.q == 0 { Self::ONE } else { r }
    }
}

/// The practice speeds, slowest first: 50, 60, 75 and 100 %.
pub const SPEEDS: [Ratio; 4] = [
    Ratio::new(1, 2),
    Ratio::new(3, 5),
    Ratio::new(3, 4),
    Ratio::ONE,
];

/// The next slower (`step` < 0) or faster speed, staying at the ends.
pub fn step_speed(from: Ratio, step: i32) -> Ratio {
    let i = SPEEDS
        .iter()
        .position(|&s| s == from)
        .unwrap_or(SPEEDS.len() - 1) as i32;
    SPEEDS[(i + step).clamp(0, SPEEDS.len() as i32 - 1) as usize]
}

/// Song position advancing `n·p/q` frames per `n` output frames, carrying the remainder so that
/// no fraction of a frame is ever lost.
#[derive(Debug, Clone, Copy)]
pub struct SongClock {
    pos: i64,
    /// Remainder of `frames·p` not yet worth a whole song frame (always `< q`).
    acc: u64,
    ratio: Ratio,
}

impl Default for SongClock {
    fn default() -> Self {
        Self {
            pos: 0,
            acc: 0,
            ratio: Ratio::ONE,
        }
    }
}

impl SongClock {
    pub fn pos(&self) -> i64 {
        self.pos
    }

    pub fn ratio(&self) -> Ratio {
        self.ratio
    }

    pub fn advance(&mut self, frames: u64) {
        self.acc += frames * self.ratio.p as u64;
        self.pos += (self.acc / self.ratio.q as u64) as i64;
        self.acc %= self.ratio.q as u64;
    }

    pub fn seek(&mut self, pos: i64) {
        self.pos = pos;
        self.acc = 0;
    }

    pub fn set_ratio(&mut self, ratio: Ratio) {
        self.ratio = ratio;
        self.acc = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_speed_advances_half() {
        let mut c = SongClock::default();
        c.set_ratio(Ratio::new(1, 2));
        let mut left = 48_000u64;
        while left > 0 {
            let n = left.min(256);
            c.advance(n);
            left -= n;
        }
        assert_eq!(c.pos(), 24_000);
    }

    #[test]
    fn full_speed_is_frame_for_frame() {
        let mut c = SongClock::default();
        c.seek(1_000);
        for n in [256u64, 128, 1, 1024, 37] {
            let before = c.pos();
            c.advance(n);
            assert_eq!(c.pos(), before + n as i64);
        }
    }

    #[test]
    fn no_drift_over_four_minutes_at_60_percent() {
        let mut c = SongClock::default();
        c.set_ratio(Ratio::new(3, 5));
        let blocks = 4 * 60 * 48_000 / 256 * 5 / 3; // four minutes of song
        for _ in 0..blocks {
            c.advance(256);
        }
        let wall = blocks as i64 * 256;
        assert_eq!(c.pos(), wall * 3 / 5);
    }

    #[test]
    fn conversions_and_steps() {
        let r = Ratio::new(3, 5);
        assert_eq!(r.to_song(500), 300);
        assert_eq!(r.to_wall(300), 500);
        assert_eq!(Ratio::from_bits(r.to_bits()), r);
        assert_eq!(Ratio::from_bits(0), Ratio::ONE);
        assert_eq!(r.percent(), 60);
        assert_eq!(step_speed(Ratio::ONE, 1), Ratio::ONE);
        assert_eq!(step_speed(Ratio::ONE, -1), Ratio::new(3, 4));
        assert_eq!(step_speed(Ratio::new(1, 2), -1), Ratio::new(1, 2));
    }
}
