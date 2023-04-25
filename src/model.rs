use std::fmt::{Debug, Display};

use chords::{Chord, ChordBuilder, ChordType};
use itertools::Itertools;
use nalgebra::{Const, SMatrix, SVector};
use nalgebra_mvn::MultivariateNormal;
use num::{Float, ToPrimitive};
use ordered_float::OrderedFloat;
use strum::EnumCount;
const NUM_NOTES: usize = chords::Note::COUNT;
pub(crate) const NUM_CHORDS: usize = NUM_NOTES * 2;
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
        // println!("{viterbi}");
        let mut max = f32::NEG_INFINITY;
        let mut argmax = 0;
        for i in 0..NUM_CHORDS {
            if viterbi[(observations.len() - 1, i)] > max {
                max = viterbi[(observations.len() - 1, i)];
                argmax = i;
            }
        }
        let mut result = Vec::with_capacity(observations.len());
        result.push(num_to_chord(argmax));
        for t in (1..observations.len()).rev() {
            result.push(num_to_chord(backpointer[(t, argmax)]));
            argmax = backpointer[(t, argmax)];
        }
        result.reverse();
        result
    }
}
impl Default for Model {
    fn default() -> Self {
        Self {
            gaussians: chords()
                .into_iter()
                .map_into()
                .collect_vec()
                .try_into()
                .unwrap(),
            hmm_params: HMMParams::default(),
        }
    }
}
fn chords() -> impl Iterator<Item = Chord> {
    chords::Note::vec()
        .into_iter()
        .cartesian_product([ChordType::Major, ChordType::Minor])
        .flat_map(|(root, chord_type)| {
            ChordBuilder::default()
                .root(root)
                .chord_type(chord_type)
                .build()
        })
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
            .rotate_left(self.chord.root.to_usize().unwrap());
        self.mvn
            .logpdf(&observation.fixed_resize::<NUM_NOTES, 1>(0.0).transpose())[(0, 0)]
            .clamp(-1e10, 1e10)
    }
}

impl From<Chord> for MVGaussian {
    fn from(chord: Chord) -> Self {
        match chord.chord_type {
            ChordType::Major => Self {
                mvn: nalgebra_mvn::MultivariateNormal::from_mean_and_covariance(
                    &MAJOR_MEANS,
                    &MAJOR_COV,
                )
                .unwrap(),
                chord,
            },
            ChordType::Minor => Self {
                mvn: nalgebra_mvn::MultivariateNormal::from_mean_and_covariance(
                    &MINOR_MEANS,
                    &MINOR_COV,
                )
                .unwrap(),
                chord,
            },
            _ => unreachable!("Only major and minor chords are supported"),
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
        let mut log_transition = MChords::identity() * 0.2 + MChords::repeat(1e-3);
        for (i, mut row) in log_transition.row_iter_mut().enumerate() {
            let mut setter = |j, v| row[(i + j + (i % 2) * 5) % NUM_CHORDS] = v;
            setter(0, 0.6);
            setter(5, 0.4);
            setter(9, 0.3);
            setter(10, 0.5);
            setter(14, 0.5);
            setter(17, 0.5);

            row /= row.sum();
            row.iter_mut().for_each(|f| *f = f.ln());
        }
        Self {
            log_initial,
            log_transition,
        }
    }
}

fn num_to_chord<N: ToPrimitive>(n: N) -> Chord {
    let n = n.to_u8().unwrap();
    let root = n / 2;
    let chord_type = match n % 2 {
        0 => ChordType::Major,
        1 => ChordType::Minor,
        _ => unreachable!(),
    };
    ChordBuilder::default()
        .root(root.into())
        .chord_type(chord_type)
        .build()
        .unwrap()
}

#[test]
fn test_mvn() {
    let chord: Chord = num_to_chord(0);
    let mut observation = Observation::zeros();
    for note in chord.notes() {
        observation[note.to_usize().unwrap()] = 1.0;
    }
    let mvn: MVGaussian = chord.into();
    let zeros = mvn.log_pdf(&observation);
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
