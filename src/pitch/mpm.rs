//! McLeod Pitch Method: normalized square difference function (NSDF) + key-maxima peak picking.
//!
//! Autocorrelation is computed with a zero-padded real FFT; all buffers are preallocated.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Fraction of the highest NSDF peak a key maximum must reach to be chosen.
const PEAK_THRESHOLD: f32 = 0.9;

pub struct Mpm {
    size: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    time: Vec<f32>,
    freq: Vec<Complex<f32>>,
    nsdf: Vec<f32>,
}

impl Mpm {
    pub fn new(size: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fwd = planner.plan_fft_forward(size * 2);
        let inv = planner.plan_fft_inverse(size * 2);
        Self {
            size,
            time: fwd.make_input_vec(),
            freq: fwd.make_output_vec(),
            nsdf: vec![0.0; size],
            fwd,
            inv,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns `(frequency, clarity)` for a window of exactly `size` samples, searching
    /// `min_hz..=max_hz`. `None` when no peak reaches `clarity_threshold`.
    pub fn detect(
        &mut self,
        x: &[f32],
        rate: f32,
        min_hz: f32,
        max_hz: f32,
        clarity_threshold: f32,
    ) -> Option<(f32, f32)> {
        let n = self.size;
        assert_eq!(x.len(), n);

        // r(τ) via |FFT|² of the zero-padded window.
        self.time[..n].copy_from_slice(x);
        self.time[n..].fill(0.0);
        self.fwd.process(&mut self.time, &mut self.freq).ok()?;
        for c in self.freq.iter_mut() {
            *c = Complex::new(c.norm_sqr(), 0.0);
        }
        self.inv.process(&mut self.freq, &mut self.time).ok()?;
        let scale = 1.0 / (2 * n) as f32;

        // m(τ) = Σ x_j² + x_{j+τ}², updated incrementally.
        let mut m: f32 = 2.0 * x.iter().map(|v| v * v).sum::<f32>();
        if m <= f32::EPSILON {
            return None;
        }
        for tau in 0..n {
            if tau > 0 {
                m -= x[tau - 1] * x[tau - 1] + x[n - tau] * x[n - tau];
            }
            self.nsdf[tau] = if m > f32::EPSILON {
                2.0 * self.time[tau] * scale / m
            } else {
                0.0
            };
        }

        // Search lags: keep at least half the window overlapping for a meaningful NSDF.
        let tau_min = ((rate / max_hz).floor() as usize).max(1);
        let tau_max = ((rate / min_hz).ceil() as usize).min(n / 2);
        if tau_min + 2 >= tau_max {
            return None;
        }

        // Key maxima: the highest point of each positive lobe after the first negative zero crossing.
        let mut key: [(usize, f32); 32] = [(0, 0.0); 32];
        let mut n_key = 0;
        let mut tau = 1;
        while tau < tau_max && self.nsdf[tau] > 0.0 {
            tau += 1;
        }
        let mut best: Option<(usize, f32)> = None;
        while tau < tau_max {
            let v = self.nsdf[tau];
            if v > 0.0 {
                if tau >= tau_min && best.is_none_or(|(_, b)| v > b) {
                    best = Some((tau, v));
                }
            } else if let Some(b) = best.take()
                && n_key < key.len()
            {
                key[n_key] = b;
                n_key += 1;
            }
            tau += 1;
        }
        if let Some(b) = best
            && n_key < key.len()
        {
            key[n_key] = b;
            n_key += 1;
        }
        let highest = key[..n_key].iter().map(|k| k.1).fold(0.0, f32::max);
        let &(t, _) = key[..n_key]
            .iter()
            .find(|k| k.1 >= PEAK_THRESHOLD * highest)?;

        // Parabolic interpolation around the chosen lag.
        let (a, b, c) = (
            self.nsdf[t - 1],
            self.nsdf[t],
            self.nsdf[(t + 1).min(n - 1)],
        );
        let denom = a - 2.0 * b + c;
        let (shift, peak) = if denom.abs() > f32::EPSILON {
            let s = 0.5 * (a - c) / denom;
            (s, b - 0.25 * (a - c) * s)
        } else {
            (0.0, b)
        };
        let clarity = peak.min(1.0);
        if clarity < clarity_threshold {
            return None;
        }
        let hz = rate / (t as f32 + shift);
        (min_hz..=max_hz).contains(&hz).then_some((hz, clarity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::note::cents_between;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn accurate_on_sines() {
        for &(hz, n) in &[
            (220.0, 1024),
            (440.0, 1024),
            (987.0, 1024),
            (130.0, 1024),
            (70.0, 2048),
        ] {
            let mut m = Mpm::new(n);
            let (est, clarity) = m
                .detect(&sine(hz, n), 48_000.0, 65.0, 1100.0, 0.5)
                .expect("pitch");
            assert!(cents_between(est, hz).abs() < 5.0, "{hz} @ {n}: {est}");
            assert!(clarity > 0.9, "{hz}: clarity {clarity}");
        }
    }

    #[test]
    fn harmonic_rich_tone_reports_fundamental() {
        let n = 1024;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                (1..=6)
                    .map(|k| (2.0 * std::f32::consts::PI * 196.0 * k as f32 * t).sin() / k as f32)
                    .sum()
            })
            .collect();
        let (est, _) = Mpm::new(n).detect(&x, 48_000.0, 65.0, 1100.0, 0.5).unwrap();
        assert!(cents_between(est, 196.0).abs() < 10.0, "{est}");
    }

    #[test]
    fn noise_is_rejected() {
        let mut seed = 1u32;
        let x: Vec<f32> = (0..1024)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect();
        assert!(
            Mpm::new(1024)
                .detect(&x, 48_000.0, 65.0, 1100.0, 0.7)
                .is_none()
        );
    }
}
