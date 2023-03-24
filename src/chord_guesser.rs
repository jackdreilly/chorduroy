use std::collections::HashMap;

use itertools::Itertools;

pub const NOTE_NAMES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

pub struct ChordGuesser {
    chords: HashMap<String, String>,
}
impl ChordGuesser {
    fn new() -> Self {
        Self {
            chords: Self::compute_chords(),
        }
    }
    pub fn guess(&self, notes: &[usize]) -> Option<String> {
        [
            vec![0, 1, 2],
            // vec![0, 1, 2, 3],
            vec![0, 1, 3],
            vec![0, 2, 3],
            vec![1, 2, 3],
            // vec![0, 1],
            // vec![0],
        ]
        .into_iter()
        .flat_map(|idxs| {
            self.chords
                .get(&idxs.iter().map(|i| notes[*i]).sorted().join(","))
        })
        .next()
        .cloned()
    }

    fn compute_chords() -> HashMap<String, String> {
        NOTE_NAMES
            .iter()
            .enumerate()
            .flat_map(|(i, chord)| {
                [
                    ("maj", vec![4, 7]),
                    ("m", vec![3, 7]),
                    ("5", vec![7]),
                    ("7", vec![4, 7, 10]),
                    ("maj7", vec![4, 7, 11]),
                    ("m7", vec![3, 7, 10]),
                    ("O", vec![]),
                ]
                .map(|(n, c)| {
                    (
                        [i].into_iter()
                            .chain(c.into_iter().map(|x| i + x))
                            .map(|f| f % 12)
                            .sorted()
                            .join(","),
                        chord.to_string() + n,
                    )
                })
            })
            .collect()
    }
}

impl Default for ChordGuesser {
    fn default() -> Self {
        Self::new()
    }
}
