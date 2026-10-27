//! A pitch contour on the song timeline: one f0 value per hop, starting at a song frame.

use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 4] = b"TRF0";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contour {
    /// Song frame of the first value.
    pub start_frame: i64,
    /// Frames between values.
    pub hop: u32,
    /// f0 in Hz; `0.0` means unvoiced.
    pub hz: Vec<f32>,
    /// Voicing probability / clarity in 0..=1.
    pub prob: Vec<f32>,
    /// Identifies what produced this contour (analyzer version + source hash), for cache checks.
    pub analyzer_version: u32,
    pub source_hash: [u8; 32],
}

impl Contour {
    pub fn new(start_frame: i64, hop: u32) -> Self {
        Self {
            start_frame,
            hop,
            ..Default::default()
        }
    }

    pub fn len(&self) -> usize {
        self.hz.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hz.is_empty()
    }

    pub fn push(&mut self, hz: Option<f32>, prob: f32) {
        self.hz.push(hz.unwrap_or(0.0));
        self.prob.push(prob);
    }

    /// Appends a value at `song_frame` (rounded to the hop grid), filling any gap with unvoiced
    /// values. Values at or before the current end are ignored.
    pub fn push_at(&mut self, song_frame: i64, hz: Option<f32>, prob: f32) {
        if self.hz.is_empty() {
            self.start_frame = song_frame;
        }
        let idx = ((song_frame - self.start_frame) as f64 / self.hop as f64).round();
        if idx < self.len() as f64 {
            return;
        }
        while (self.len() as f64) < idx {
            self.push(None, 0.0);
        }
        self.push(hz, prob);
    }

    pub fn frame_of(&self, i: usize) -> i64 {
        self.start_frame + i as i64 * self.hop as i64
    }

    pub fn end_frame(&self) -> i64 {
        self.frame_of(self.len())
    }

    /// Index of the value nearest to `song_frame`, if inside the contour.
    pub fn index_at(&self, song_frame: i64) -> Option<usize> {
        if self.hz.is_empty() || self.hop == 0 {
            return None;
        }
        let rel = song_frame - self.start_frame;
        let i = (rel as f64 / self.hop as f64).round();
        if i < 0.0 || i as usize >= self.hz.len() {
            None
        } else {
            Some(i as usize)
        }
    }

    /// Voiced f0 at `song_frame`, if any.
    pub fn hz_at(&self, song_frame: i64) -> Option<f32> {
        self.index_at(song_frame)
            .map(|i| self.hz[i])
            .filter(|&h| h > 0.0)
    }

    pub fn voiced(&self, i: usize) -> Option<f32> {
        self.hz.get(i).copied().filter(|&h| h > 0.0)
    }

    /// Index range of values whose frames fall within `[from, to)`.
    pub fn index_range(&self, from: i64, to: i64) -> std::ops::Range<usize> {
        if self.hz.is_empty() || self.hop == 0 {
            return 0..0;
        }
        let hop = self.hop as i64;
        let a = ((from - self.start_frame).max(0) + hop - 1) / hop;
        let b = ((to - self.start_frame).max(0) + hop - 1) / hop;
        (a as usize).min(self.len())..(b as usize).min(self.len())
    }

    pub fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(MAGIC)?;
        w.write_all(&FORMAT_VERSION.to_le_bytes())?;
        w.write_all(&self.analyzer_version.to_le_bytes())?;
        w.write_all(&self.source_hash)?;
        w.write_all(&self.start_frame.to_le_bytes())?;
        w.write_all(&self.hop.to_le_bytes())?;
        w.write_all(&(self.hz.len() as u64).to_le_bytes())?;
        for v in self.hz.iter().chain(self.prob.iter()) {
            w.write_all(&v.to_le_bytes())?;
        }
        Ok(())
    }

    pub fn read_from(r: &mut impl Read) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a contour file",
            ));
        }
        if read_u32(r)? != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported contour version",
            ));
        }
        let analyzer_version = read_u32(r)?;
        let mut source_hash = [0u8; 32];
        r.read_exact(&mut source_hash)?;
        let mut b8 = [0u8; 8];
        r.read_exact(&mut b8)?;
        let start_frame = i64::from_le_bytes(b8);
        let hop = read_u32(r)?;
        r.read_exact(&mut b8)?;
        let n = u64::from_le_bytes(b8) as usize;
        let mut read_vec = |n: usize| -> io::Result<Vec<f32>> {
            let mut bytes = vec![0u8; n * 4];
            r.read_exact(&mut bytes)?;
            Ok(bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect())
        };
        let hz = read_vec(n)?;
        let prob = read_vec(n)?;
        Ok(Self {
            start_frame,
            hop,
            hz,
            prob,
            analyzer_version,
            source_hash,
        })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("tmp");
        {
            let mut f = io::BufWriter::new(std::fs::File::create(&tmp)?);
            self.write_to(&mut f)?;
            f.flush()?;
        }
        std::fs::rename(tmp, path)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        Self::read_from(&mut io::BufReader::new(std::fs::File::open(path)?))
    }
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Contour {
        let mut c = Contour::new(1000, 256);
        c.push(Some(220.0), 0.9);
        c.push(None, 0.1);
        c.push(Some(230.0), 0.8);
        c.analyzer_version = 3;
        c.source_hash = [7; 32];
        c
    }

    #[test]
    fn lookup_by_song_frame() {
        let c = sample();
        assert_eq!(c.hz_at(1000), Some(220.0));
        assert_eq!(c.hz_at(1256), None);
        assert_eq!(c.hz_at(1512 + 100), Some(230.0));
        assert_eq!(c.hz_at(999 - 200), None);
        assert_eq!(c.index_range(1000, 1513), 0..3);
        assert_eq!(c.index_range(1256, 1300), 1..2);
        assert_eq!(c.index_range(1257, 1300), 2..2);
    }

    #[test]
    fn push_at_fills_gaps() {
        let mut c = Contour::new(0, 128);
        c.push_at(1000, Some(200.0), 1.0);
        c.push_at(1128, Some(201.0), 1.0);
        c.push_at(1128 + 3 * 128, Some(202.0), 1.0);
        c.push_at(1000, Some(999.0), 1.0); // stale, ignored
        assert_eq!(c.start_frame, 1000);
        assert_eq!(c.hz, vec![200.0, 201.0, 0.0, 0.0, 202.0]);
    }

    #[test]
    fn roundtrip() {
        let c = sample();
        let mut buf = Vec::new();
        c.write_to(&mut buf).unwrap();
        assert_eq!(Contour::read_from(&mut buf.as_slice()).unwrap(), c);
    }
}
