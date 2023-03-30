use std::{fmt::Display, ops::Add};

use itertools::Itertools;
use nalgebra::{Const, SMatrix, SVector};
use nalgebra_mvn::MultivariateNormal;
use num_derive::FromPrimitive;
use ordered_float::OrderedFloat;
use serde::Serialize;
use strum::IntoEnumIterator;
use strum_macros::{EnumIter, FromRepr};

const NUM_NOTES: usize = 12;
pub(crate) const NUM_CHORDS: usize = NUM_NOTES * 2;
#[derive(Clone, Copy, EnumIter, PartialEq, Eq, Debug, Serialize)]
#[repr(u8)]
enum Flavor {
    Major,
    Minor,
}

type Intervals = [u8; 3];

impl From<Flavor> for Intervals {
    fn from(value: Flavor) -> Self {
        match value {
            Flavor::Major => [0, 4, 7],
            Flavor::Minor => [0, 3, 7],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Chord {
    flavor: Flavor,
    note: Note,
}
impl From<Chord> for usize {
    fn from(chord: Chord) -> Self {
        2 * chord.note as usize + chord.flavor as usize
    }
}

impl From<Chord> for Intervals {
    fn from(value: Chord) -> Self {
        Intervals::from(value.flavor)
            .into_iter()
            .map(|f| (f + value.note as u8) % NUM_NOTES as u8)
            .collect_vec()
            .try_into()
            .unwrap()
    }
}

#[derive(Clone, Copy, EnumIter, FromPrimitive, Debug, PartialEq, Eq, FromRepr, Serialize)]
#[repr(u8)]
pub(crate) enum Note {
    A,
    Bb,
    B,
    C,
    Db,
    D,
    Eb,
    E,
    F,
    Gb,
    G,
    Ab,
}
#[derive(Debug)]
pub(crate) struct Model {
    gaussians: [MVGaussian; NUM_CHORDS],
    hmm_params: HMMParams,
}
impl Model {
    pub(crate) fn infer_all(&self, observation: &Observation) -> Vec<Chord> {
        Chord::vec()
            .into_iter()
            .sorted_by_cached_key(|&chord| {
                -OrderedFloat::from(self.gaussians[usize::from(chord)].log_pdf(observation))
            })
            .collect()
    }
    pub(crate) fn infer_viterbi(&self, observations: &[Observation]) -> Vec<Chord> {
        let mut viterbi = SMatrix::<f32, NUM_CHORDS, NUM_CHORDS>::zeros();
        let mut backpointer = SMatrix::<usize, NUM_CHORDS, NUM_CHORDS>::zeros();
        // Initialize viterbi and backpointer
        for i in 0..NUM_CHORDS {
            viterbi[(0, i)] = self.hmm_params.log_initial[i]
                + self
                    .gaussians
                    .get(i)
                    .unwrap()
                    .log_pdf(observations.get(0).unwrap());
            backpointer[(0, i)] = 0;
        }
        for t in 1..observations.len() {
            for i in 0..NUM_CHORDS {
                let mut max = f32::NEG_INFINITY;
                let mut argmax = 0;
                for j in 0..NUM_CHORDS {
                    let value = viterbi[(t - 1, j)]
                        + self.hmm_params.log_transition.get((j, i)).unwrap()
                        + self
                            .gaussians
                            .get(i)
                            .unwrap()
                            .log_pdf(observations.get(t).unwrap());
                    if value > max {
                        max = value;
                        argmax = j;
                    }
                }
                viterbi[(t, i)] = max;
                backpointer[(t, i)] = argmax;
            }
        }
        let mut max = f32::NEG_INFINITY;
        let mut argmax = 0;
        for i in 0..NUM_CHORDS {
            if viterbi[(observations.len() - 1, i)] > max {
                max = viterbi[(observations.len() - 1, i)];
                argmax = i;
            }
        }
        let mut result = Vec::with_capacity(observations.len());
        result.push(argmax.into());
        for t in (1..observations.len()).rev() {
            result.push(backpointer[(t, argmax)].into());
            argmax = backpointer[(t, argmax)];
        }
        result.reverse();
        result
    }
}
impl Default for Model {
    fn default() -> Self {
        Self {
            gaussians: Chord::vec()
                .into_iter()
                .map_into()
                .collect_vec()
                .try_into()
                .unwrap(),
            hmm_params: HMMParams::default(),
        }
    }
}

impl Chord {
    fn vec() -> Vec<Chord> {
        Note::iter()
            .flat_map(|note| Flavor::iter().map(move |flavor| Chord { flavor, note }))
            .collect()
    }

    fn relative_flavor(&self) -> Chord {
        match self.flavor {
            Flavor::Major => Chord {
                flavor: Flavor::Minor,
                note: self.note + 9,
            },
            Flavor::Minor => Chord {
                flavor: Flavor::Major,
                note: self.note + 3,
            },
        }
    }

    pub(crate) fn notes(&self) -> [Note; 3] {
        Intervals::from(self.flavor)
            .into_iter()
            .map(|f| self.note + f)
            .collect_vec()
            .try_into()
            .unwrap()
    }
}
type VNotes = SVector<f32, NUM_NOTES>;
type MNotes = SMatrix<f32, NUM_NOTES, NUM_NOTES>;
type VChords = SVector<f32, NUM_CHORDS>;
type MChords = SMatrix<f32, NUM_CHORDS, NUM_CHORDS>;
pub(crate) type Observation = VNotes;
#[derive(Debug)]
struct MVGaussian {
    mvn: MultivariateNormal<f32, Const<NUM_NOTES>>,
}
impl MVGaussian {
    fn log_pdf(&self, observation: &Observation) -> f32 {
        self.mvn
            .logpdf(&observation.fixed_resize::<NUM_NOTES, 1>(0.0).transpose())[(0, 0)]
    }
}

impl From<usize> for Chord {
    fn from(value: usize) -> Self {
        Chord::vec()[value]
    }
}
impl From<&usize> for Chord {
    fn from(value: &usize) -> Self {
        Chord::vec()[*value]
    }
}

impl From<Chord> for MVGaussian {
    fn from(chord: Chord) -> Self {
        let mut mean = VNotes::zeros();
        let mut covariance = MNotes::zeros();
        let notes = Intervals::from(chord);
        for note in Note::iter() {
            let note = note as usize;
            if notes.contains(&(note as u8)) {
                mean[note] = 1.0;
                covariance[(note, note)] = 1.0;
            } else {
                covariance[(note, note)] = 0.2;
            }
        }
        for (i, j, value) in [(0, 2, 0.8), (1, 2, 0.8), (0, 1, 0.6)] {
            let i = notes[i] as usize;
            let j = notes[j] as usize;
            covariance[(i, j)] = value;
            covariance[(j, i)] = value;
        }
        let mvn =
            nalgebra_mvn::MultivariateNormal::from_mean_and_covariance(&mean, &covariance).unwrap();
        Self { mvn }
    }
}

#[test]
fn transition_matrix() {
    let matrix = HMMParams::default().log_transition;
    for i in 0..NUM_CHORDS {
        let row = matrix.row(i);
        assert!((row.sum() - 1.0).abs() < 1e-6);
        for &v in &row {
            assert!(v >= 0.0);
            assert!(v < 1.0);
        }
    }
}

#[derive(Debug)]
struct HMMParams {
    log_initial: VChords,
    log_transition: MChords,
}
impl Default for HMMParams {
    fn default() -> Self {
        let log_initial = VChords::repeat((NUM_CHORDS as f32).recip().ln());
        let mut log_transition = MChords::zeros();
        for chord in Chord::vec() {
            let i = usize::from(chord);
            for (step_base, Chord { note, flavor }) in
                [(0f32, chord), (0.5f32, chord.relative_flavor())]
            {
                for cycle in [5, 7] {
                    for step in 0..7 {
                        log_transition[(
                            i,
                            usize::from(Chord {
                                note: note + cycle * step,
                                flavor,
                            }),
                        )] = (-(step as f32 + step_base + 3.0) * 0.7).exp();
                    }
                }
            }
        }
        for mut row in log_transition.row_iter_mut() {
            row /= row.sum();
            row.apply(|f| *f = f.ln());
        }
        Self {
            log_initial,
            log_transition,
        }
    }
}

impl Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Display for Flavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = format!("{self:?}");
        let string = &string.to_lowercase()[..3];
        write!(f, "{string}")
    }
}
impl Display for Chord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { note, flavor } = self;
        write!(f, "{note} {flavor}")
    }
}

impl Add<u8> for Note {
    type Output = Self;
    fn add(self, rhs: u8) -> Self::Output {
        Note::from_repr((self as u8 + rhs) % NUM_NOTES as u8).unwrap()
    }
}

#[test]
fn plotter() {
    use plotters::prelude::*;

    const OUT_FILE_NAME: &str = "transition.png";
    fn main() -> Result<(), Box<dyn std::error::Error>> {
        let root = BitMapBackend::new(OUT_FILE_NAME, (1024, 768)).into_drawing_area();

        root.fill(&WHITE)?;
        let matrix = HMMParams::default().log_transition;
        let n = matrix.nrows();

        let mut chart = ChartBuilder::on(&root)
            .caption("Matshow Example", ("sans-serif", 80))
            .margin(5)
            .top_x_label_area_size(40)
            .y_label_area_size(40)
            .build_cartesian_2d(0..n, n..0)?;
        fn formatter(&x: &usize) -> String {
            if x >= NUM_CHORDS {
                "".to_string()
            } else {
                Chord::from(x).to_string()
            }
        }
        chart
            .configure_mesh()
            .x_labels(n)
            .y_labels(n)
            .max_light_lines(4)
            .x_label_offset(35)
            .y_label_offset(25)
            .disable_x_mesh()
            .disable_y_mesh()
            .label_style(("sans-serif", 12))
            .x_label_formatter(&formatter)
            .y_label_formatter(&formatter)
            .draw()?;
        let max = matrix.max() as f64;
        chart.draw_series(
            matrix
                .row_iter()
                .enumerate()
                .flat_map(|(y, l)| {
                    l.iter()
                        .copied()
                        .enumerate()
                        .map(move |(x, v)| (x, y, v as f64))
                        .collect_vec()
                })
                .map(|(x, y, v)| {
                    Rectangle::new(
                        [(x, y), (x + 1, y + 1)],
                        HSLColor(0.0, 0.0, v / max).filled(),
                    )
                }),
        )?;
        Ok(())
    }
    main().unwrap();
}

#[test]
fn test_mvn() {
    let chord: Chord = 0.into();
    let mut observation = Observation::zeros();
    let mvn: MVGaussian = chord.into();
    let zeros = mvn.log_pdf(&observation);
    for note in chord.notes() {
        observation[note as usize] = 1.0;
    }
    let exact = mvn.log_pdf(&observation);
    assert!(exact > zeros);
}

impl Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.hmm_params.log_initial)?;
        writeln!(f, "{}", self.hmm_params.log_transition)
    }
}