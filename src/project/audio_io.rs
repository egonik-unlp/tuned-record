//! Decoding imports (symphonia), resampling to the project rate (rubato), WAV read/write (hound).

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use rubato::audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::audio::mixer::Audio;

/// Decodes any supported file (WAV, FLAC, MP3, OGG, …) to interleaved f32, keeping at most 2
/// channels. Returns the audio and its sample rate.
pub fn decode(path: &Path) -> Result<(Audio, u32)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("unsupported or unreadable audio file: {e}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("no audio track"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| anyhow!("no audio parameters"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| anyhow!("unsupported codec: {e}"))?;

    let mut rate = params.sample_rate.unwrap_or(0);
    let mut channels = 0usize;
    let mut out: Vec<f32> = Vec::new();
    let mut buf: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => bail!("decode error: {e}"),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => bail!("decode error: {e}"),
        };
        let spec = decoded.spec();
        rate = spec.rate();
        let ch = spec.channels().count().max(1);
        channels = ch;
        buf.resize(decoded.samples_interleaved(), 0.0);
        decoded.copy_to_slice_interleaved(&mut buf);
        if ch <= 2 {
            out.extend_from_slice(&buf);
        } else {
            for f in buf.chunks_exact(ch) {
                out.extend_from_slice(&f[..2]);
            }
        }
    }
    if out.is_empty() || rate == 0 {
        bail!("no audio could be decoded");
    }
    Ok((
        Audio {
            channels: channels.min(2),
            samples: out,
        },
        rate,
    ))
}

/// Resamples to `to` Hz (no-op when the rates match).
pub fn resample(audio: Audio, from: u32, to: u32) -> Result<Audio> {
    if from == to {
        return Ok(audio);
    }
    let ch = audio.channels;
    let frames = audio.frames();
    let input =
        InterleavedOwned::new_from(audio.samples, ch, frames).map_err(|e| anyhow!("{e:?}"))?;
    let mut r = Fft::<f32>::new(from as usize, to as usize, 1024, ch, FixedSync::Both)
        .map_err(|e| anyhow!("resampler: {e}"))?;
    let out = r
        .process_all(&input, frames, None)
        .map_err(|e| anyhow!("resampling failed: {e}"))?;
    let expected = (frames as f64 * to as f64 / from as f64).round() as usize;
    let mut samples = out.take_data();
    samples.truncate(expected * ch);
    Ok(Audio {
        channels: ch,
        samples,
    })
}

/// Decodes and resamples to the project rate.
pub fn load_for_project(path: &Path, rate: u32) -> Result<Audio> {
    let (audio, from) = decode(path)?;
    resample(audio, from, rate)
}

pub fn write_wav(path: &Path, audio: &Audio, rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: audio.channels as u16,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let tmp = path.with_extension("wav.tmp");
    let mut w = hound::WavWriter::create(&tmp, spec)?;
    for &s in &audio.samples {
        w.write_sample(s)?;
    }
    w.finalize()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Reads a WAV written by this app (f32) or any integer PCM WAV, plus its rate.
pub fn read_wav(path: &Path) -> Result<(Audio, u32)> {
    let mut r =
        hound::WavReader::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let spec = r.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map_while(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map_while(Result::ok)
                .map(|s| s as f32 * scale)
                .collect()
        }
    };
    let ch = spec.channels as usize;
    let whole = samples.len() / ch * ch;
    let mut samples = samples;
    samples.truncate(whole);
    Ok((
        Audio {
            channels: ch,
            samples,
        },
        spec.sample_rate,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f32, ch: usize) -> Audio {
        let n = (rate as f32 * secs) as usize;
        let mut s = Vec::with_capacity(n * ch);
        for i in 0..n {
            let v = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin();
            for _ in 0..ch {
                s.push(v);
            }
        }
        Audio {
            channels: ch,
            samples: s,
        }
    }

    #[test]
    fn wav_roundtrip_and_decode() {
        let d = std::env::temp_dir().join(format!("tr-audio-io-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("a.wav");
        let a = tone(44_100, 0.5, 2);
        write_wav(&p, &a, 44_100).unwrap();
        let (b, rate) = read_wav(&p).unwrap();
        assert_eq!(rate, 44_100);
        assert_eq!(b.samples, a.samples);
        let (c, rate) = decode(&p).unwrap();
        assert_eq!((rate, c.channels, c.frames()), (44_100, 2, a.frames()));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn resample_keeps_duration_and_pitch() {
        let a = tone(44_100, 1.0, 2);
        let b = resample(a, 44_100, 48_000).unwrap();
        assert_eq!(b.frames(), 48_000);
        // Count zero crossings in the middle second on channel 0: ~880 for 440 Hz.
        let left: Vec<f32> = b.samples.chunks_exact(2).map(|f| f[0]).collect();
        let crossings = left[1000..47_000]
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        let expected = 440.0 * 46_000.0 / 48_000.0;
        assert!(
            (crossings as f32 - expected).abs() < 3.0,
            "{crossings} vs {expected}"
        );
    }

    #[test]
    fn undecodable_file_errors() {
        let d = std::env::temp_dir().join(format!("tr-bad-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("bad.mp3");
        std::fs::write(&p, b"definitely not audio").unwrap();
        assert!(decode(&p).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
}
