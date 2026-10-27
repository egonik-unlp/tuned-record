//! Frequency ↔ MIDI note conversions and note naming.

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Fractional MIDI note number for a frequency (A4 = 440 Hz = 69).
pub fn hz_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Cents from `b` to `a` (positive when `a` is higher).
pub fn cents_between(a_hz: f32, b_hz: f32) -> f32 {
    1200.0 * (a_hz / b_hz).log2()
}

/// Name and octave of an integer MIDI note, e.g. 57 → ("A", 3).
pub fn note_name(midi: i32) -> (&'static str, i32) {
    (NAMES[midi.rem_euclid(12) as usize], midi.div_euclid(12) - 1)
}

pub fn note_label(midi: i32) -> String {
    let (n, o) = note_name(midi);
    format!("{n}{o}")
}

/// Nearest note and the deviation from it, for the readout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteReading {
    pub midi: i32,
    pub cents: f32,
}

impl NoteReading {
    pub fn from_hz(hz: f32) -> Self {
        let m = hz_to_midi(hz);
        let midi = m.round() as i32;
        Self {
            midi,
            cents: (m - midi as f32) * 100.0,
        }
    }

    pub fn label(&self) -> String {
        note_label(self.midi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_69() {
        assert!((hz_to_midi(440.0) - 69.0).abs() < 1e-5);
        assert!((midi_to_hz(57.0) - 220.0).abs() < 1e-3);
    }

    #[test]
    fn names_and_octaves() {
        assert_eq!(note_name(60), ("C", 4));
        assert_eq!(note_name(57), ("A", 3));
        assert_eq!(note_name(59), ("B", 3));
    }

    #[test]
    fn slightly_sharp_a3() {
        let r = NoteReading::from_hz(225.0);
        assert_eq!(r.label(), "A3");
        assert!((r.cents - 38.9).abs() < 0.5, "cents = {}", r.cents);
    }

    #[test]
    fn cents_sign() {
        assert!((cents_between(440.0, 220.0) - 1200.0).abs() < 1e-3);
        assert!(cents_between(220.0, 440.0) < 0.0);
    }
}
