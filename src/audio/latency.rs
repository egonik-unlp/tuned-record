//! Latency compensation: where on the song timeline a captured sample belongs.
//!
//! In a callback whose output buffer starts at song frame `p`, what the singer hears *now* was
//! written `lo` frames ago, and mic sample `i` was captured `li` frames ago. So sample `i` matches
//! what they heard at song frame `p + i − lo − li − c`, `c` being the calibrated residual.

/// Total compensation in frames: reported playback + capture latency plus the calibration residual.
pub fn compensation(playback_latency: u32, capture_latency: u32, calibration: i64) -> i64 {
    playback_latency as i64 + capture_latency as i64 + calibration
}

/// Song frame matching mic sample `i` of a buffer whose output starts at song frame `p`.
pub fn capture_song_frame(p: i64, i: usize, compensation: i64) -> i64 {
    p + i as i64 - compensation
}

/// Finds the residual delay (frames) of `captured` relative to `reference` by cross-correlation,
/// searching lags `-max_lag..=max_lag`. Positive means the capture is late.
pub fn residual_by_xcorr(reference: &[f32], captured: &[f32], max_lag: usize) -> Option<i64> {
    let n = reference.len().min(captured.len());
    if n == 0 {
        return None;
    }
    let mut best = (0i64, f32::MIN);
    for lag in -(max_lag as i64)..=max_lag as i64 {
        let mut acc = 0.0;
        for (i, &r) in reference[..n].iter().enumerate() {
            let j = i as i64 + lag;
            if j >= 0 && (j as usize) < captured.len() {
                acc += r * captured[j as usize];
            }
        }
        if acc > best.1 {
            best = (lag, acc);
        }
    }
    (best.1 > 0.0).then_some(best.0)
}

/// Builds a calibration click track: `count` single-sample-wide clicks (short decaying bursts)
/// every `spacing` frames, starting at `spacing`.
pub fn click_track(spacing: usize, count: usize) -> Vec<f32> {
    let mut v = vec![0.0; spacing * (count + 1)];
    for k in 1..=count {
        for (j, s) in v[k * spacing..].iter_mut().take(32).enumerate() {
            *s = 0.8 * (1.0 - j as f32 / 32.0) * if j % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compensation_shifts_capture_back() {
        let c = compensation(256, 256, 10);
        assert_eq!(c, 522);
        assert_eq!(capture_song_frame(10_000, 5, c), 10_000 + 5 - 522);
    }

    #[test]
    fn loopback_alignment_after_compensation() {
        // Simulate a loopback: the sample written at song frame f reaches the mic `rt` frames later.
        let rt = 700usize;
        let click = click_track(4800, 3);
        let mut mic_time = vec![0.0; click.len() + rt];
        mic_time[rt..].copy_from_slice(&click);
        // Engine runs from song frame 0 with buffers of 256; capture is placed with compensation.
        let comp = compensation(300, 400, 0);
        let mut take = vec![0.0f32; click.len()];
        for (wall, &s) in mic_time.iter().enumerate() {
            let f = capture_song_frame(wall as i64, 0, comp);
            if f >= 0 && (f as usize) < take.len() {
                take[f as usize] = s;
            }
        }
        assert_eq!(residual_by_xcorr(&click, &take, 200), Some(0));
    }

    #[test]
    fn xcorr_finds_residual() {
        let click = click_track(4800, 3);
        let mut late = vec![0.0; click.len()];
        late[37..].copy_from_slice(&click[..click.len() - 37]);
        assert_eq!(residual_by_xcorr(&click, &late, 100), Some(37));
    }
}
