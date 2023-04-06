use std::{
    fmt::{Debug, Display},
    ops::Add,
};

use itertools::Itertools;
use nalgebra::{Const, SMatrix, SVector};
use nalgebra_mvn::MultivariateNormal;
use num::Float;
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
    chord: Chord,
}
impl MVGaussian {
    fn log_pdf(&self, observation: &Observation) -> f32 {
        let mut observation = *observation;
        observation
            .column_mut(0)
            .data
            .into_slice_mut()
            .rotate_left(self.chord.note as usize);
        self.mvn
            .logpdf(&observation.fixed_resize::<NUM_NOTES, 1>(0.0).transpose())[(0, 0)]
            .clamp(-1e10, 1e10)
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
        let mut covariance = MNotes::identity() * 0.2;
        let color_note = if chord.flavor == Flavor::Major { 4 } else { 3 };
        for note in [0, 7, color_note] {
            covariance[(note, note)] = 1.0;
        }
        covariance[(0, 7)] = 0.8;
        covariance[(7, 0)] = 0.8;
        covariance[(color_note, 7)] = 0.8;
        covariance[(7, color_note)] = 0.8;
        covariance[(0, color_note)] = 0.6;
        covariance[(color_note, 0)] = 0.6;
        covariance *= 0.1;
        let mut mean = VNotes::from_element(0.1);
        mean[0] = 1.0;
        mean[7] = 0.8;
        mean[color_note] = 0.4;
        mean.normalize_mut();

        match chord.flavor {
            Flavor::Major => Self {
                mvn: nalgebra_mvn::MultivariateNormal::from_mean_and_covariance(
                    &mean,
                    &covariance,
                )
                .unwrap(),
                chord,
            },
            Flavor::Minor => Self {
                mvn: nalgebra_mvn::MultivariateNormal::from_mean_and_covariance(
                    &mean,
                    &covariance,
                )
                .unwrap(),
                chord,
            },
        }
    }
}

#[test]
fn transition_matrix() {
    let matrix = HMMParams::default().log_transition;
    for i in 0..NUM_CHORDS {
        let row = matrix.row(i).map(|f| f.exp());
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
            for (_step_base, Chord { note, flavor }) in [(0, chord), (1, chord.relative_flavor())] {
                for cycle in [5, 7] {
                    for step in 0..7 {
                        log_transition[(
                            i,
                            Chord {
                                note: note + cycle * step,
                                flavor,
                            }
                            .into(),
                        )] = if step < 3 {
                            3.0
                        } else if step < 5 {
                            1.0
                        } else {
                            0.1
                        };
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
pub(crate) trait SortFloat<A, Fl: Float, F: Fn(&A) -> Fl> {
    fn sorted_by_cached_float(self, f: F) -> std::vec::IntoIter<A>;
}
impl<A, Fl: Float, F: Fn(&A) -> Fl, T> SortFloat<A, Fl, F> for T
where
    T: Iterator<Item = A>,
{
    fn sorted_by_cached_float(self, f: F) -> std::vec::IntoIter<A> {
        self.sorted_by_cached_key(|x| OrderedFloat(f(x)))
    }
}

#[test]
fn float_sorted() {
    let a = [0.0, 4.2, 2.2]
        .iter()
        .sorted_by_cached_float(|&&f| f)
        .copied()
        .collect_vec();
    assert_eq!(a, vec![0.0, 2.2, 4.2]);
}

use lazy_static::lazy_static;

lazy_static! {
    static ref MAJOR_MEANS: VNotes =
        VNotes::from_vec(serde_json::from_str(include_str!("../data/maj_mean.json")).unwrap());
    static ref MINOR_MEANS: VNotes =
        VNotes::from_vec(serde_json::from_str(include_str!("../data/min_mean.json")).unwrap());
    static ref MAJOR_COV: MNotes = MNotes::from_vec(
        serde_json::from_str::<Vec<Vec<f32>>>(include_str!("../data/maj_cov.json"))
            .unwrap()
            .into_iter()
            .flatten()
            .collect()
    );
    static ref MINOR_COV: MNotes = MNotes::from_vec(
        serde_json::from_str::<Vec<Vec<f32>>>(include_str!("../data/min_cov.json"))
            .unwrap()
            .into_iter()
            .flatten()
            .collect()
    );
}
