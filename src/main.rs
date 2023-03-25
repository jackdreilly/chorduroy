use coremidi::{Client, Destination, Destinations, OutputPort, Protocol};
use coremidi::{PacketBuffer, Sources};
use cpal::default_host;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use textplots::{Chart, Plot};

use std::collections::HashMap;
use std::f32::consts::PI;
use std::io::{stdin, BufRead};
use std::sync::mpsc;
use std::thread;

use clap::Parser;
use itertools::Itertools;
pub const NOTE_NAMES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 14)]
    milliseconds: u32,
    #[arg(short, long)]
    source: Option<String>,
    #[arg(short, long)]
    destination: Option<String>,
    #[arg(short, long)]
    audio: Option<String>,
}

enum Event {
    Note(bool, u8),
    Chord(String, Vec<usize>),
}

fn main() {
    let Args {
        milliseconds,
        source,
        destination,
        audio,
    } = Args::parse();
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    thread::spawn(move || {
        publish_chords_from_audio(audio.unwrap_or_else(|| "Black".into()), milliseconds, tx);
        block();
    });

    thread::spawn(move || {
        publish_midi_in_events(source.unwrap_or_else(|| "OP-1".into()), tx2);
        block();
    });
    output_remapped_midi_notes(destination, rx);
}

fn output_remapped_midi_notes(destination: Option<String>, rx: mpsc::Receiver<Event>) {
    let player = Player::new(&destination.unwrap_or_else(|| "Garage".into()));
    let mut active_chord_notes = vec![];
    let mut midi_note_remapping_history = vec![0; 256];
    let mut active_chord_name = "".to_string();
    for event in rx {
        match event {
            Event::Chord(new_name, chord) => {
                if active_chord_name != new_name {
                    println!("{}", new_name);
                }
                active_chord_name = new_name;
                active_chord_notes = chord;
            }
            Event::Note(on, note) => {
                if active_chord_notes.is_empty() {
                    continue;
                }
                if !on {
                    player.play_off(midi_note_remapping_history[note as usize]);
                    continue;
                }
                let new_note = active_chord_notes
                    .iter()
                    .map(|&n| n as u8 + 9)
                    .min_by_key(|f| f.abs_diff(note) % 12)
                    .unwrap()
                    + (note - (note % 12));
                midi_note_remapping_history[note as usize] = new_note;
                player.play_on(new_note);
            }
        }
    }
}

fn publish_chords_from_audio(audio: String, milliseconds: u32, tx: mpsc::Sender<Event>) {
    let guesser = ChordGuesser::default();
    let device = get_audio_device(audio);
    let config = device.default_input_config().unwrap().into();
    let mut buffer = vec![];
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                buffer.extend_from_slice(data);
                buffer = buffer[0
                    .max(buffer.len() as i32 - (milliseconds / 1000 * config.sample_rate.0) as i32)
                    as usize..]
                    .to_vec();
                let rate = config.sample_rate.0 as f32;
                let bars = (0..12)
                    .map(move |note| {
                        (0..4)
                            .map(|octave| {
                                let bin = octave * 12 + note;
                                let f_k: f32 = 55f32 * 2.0_f32.powf(bin as f32 / 12_f32);
                                let n_k = (rate / ((2.0_f32.powf(1.0 / 12f32) - 1.0) * f_k))
                                    .min(data.len() as f32);
                                let factor = f_k * -2f32 * PI / rate;
                                let mut sum_real: f32 = 0.0;
                                let mut sum_imag: f32 = 0.0;

                                for j in 0..n_k.floor() as usize {
                                    let j = j + (data.len() - n_k.floor() as usize) / 2;
                                    let d = data[j];
                                    let real_common = d / n_k;
                                    let (sin, cos) = (factor
                                        * (j as f32 + (n_k.floor() / 2f32)
                                            - data.len() as f32 / 2f32))
                                        .sin_cos();
                                    sum_real += real_common * cos;
                                    sum_imag += real_common * sin;
                                }
                                sum_real.hypot(sum_imag)
                            })
                            .sum::<f32>()
                    })
                    .collect_vec();
                Chart::new_with_y_range(100, 50, 0f32, 11f32, 0f32, 0.1f32)
                    .lineplot(&textplots::Shape::Bars(
                        &bars
                            .iter()
                            .enumerate()
                            .map(|(i, x)| (i as f32, *x))
                            .collect_vec(),
                    ))
                    .display();

                let best_notes = bars
                    .iter()
                    .enumerate()
                    .sorted_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap())
                    .take(4)
                    .map(|(i, _)| i)
                    .collect_vec();
                if let Some(x) = guesser.guess(&best_notes) {
                    tx.send(Event::Chord(x.clone(), guesser.chord_to_notes(&x)))
                        .unwrap();
                }
            },
            |err| eprintln!("an error occurred on the output audio stream: {}", err),
            None,
        )
        .unwrap();
    stream.play().unwrap();
}

fn publish_midi_in_events(source: String, tx: mpsc::Sender<Event>) {
    let source = Sources
        .into_iter()
        .find(|x| x.name().unwrap().contains(&source))
        .unwrap();
    let client = Client::new("Example Client").unwrap();
    let mut input_port = client
        .input_port_with_protocol("Example Port", Protocol::Midi10, move |event_list, _| {
            for event in event_list.iter() {
                if event.data()[0] == 0x10f80000 {
                    continue;
                }
                let note = ((event.data()[0]) << 15 >> 23) as u8;
                let on = (event.data()[0] & 1) == 1;
                tx.send(Event::Note(on, note)).unwrap();
            }
        })
        .unwrap();
    input_port
        .connect_source(&source, source.unique_id().unwrap_or(0))
        .unwrap();
}

fn block() {
    stdin().lock().lines().next();
}

fn get_audio_device(audio: String) -> cpal::Device {
    let device = default_host()
        .input_devices()
        .unwrap()
        .into_iter()
        .find(|x| x.name().unwrap().contains(&audio))
        .unwrap_or_else(|| {
            panic!(
                "No audio device found with name containing '{}' {:#?}",
                audio,
                default_host()
                    .input_devices()
                    .unwrap()
                    .map(|x| x.name().unwrap())
                    .collect_vec()
            )
        });
    device
}

fn get_destination(destination: &str) -> coremidi::Destination {
    Destinations
        .into_iter()
        .find(|f| f.name().unwrap().contains(destination))
        .unwrap_or_else(|| {
            panic!(
                "No match for {destination} {:#?}",
                Destinations
                    .into_iter()
                    .map(|x| x.name().unwrap())
                    .collect_vec()
            )
        })
}

pub struct ChordGuesser {
    chord_hash_to_name: HashMap<String, String>,
    chord_name_to_notes: HashMap<String, Vec<usize>>,
}
impl ChordGuesser {
    fn new() -> Self {
        let chord_name_to_notes: HashMap<String, Vec<usize>> = NOTE_NAMES
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
                        chord.to_string() + n,
                        [i].into_iter()
                            .chain(c.into_iter().map(|x| i + x))
                            .map(|f| f % 12)
                            .collect_vec(),
                    )
                })
            })
            .collect();
        let chord_hash_to_name = chord_name_to_notes
            .iter()
            .map(|(k, v)| (v.iter().sorted().join(","), k.clone()))
            .collect();
        Self {
            chord_hash_to_name,
            chord_name_to_notes,
        }
    }
    pub fn guess(&self, notes: &[usize]) -> Option<String> {
        [vec![0, 1, 2]]
            .into_iter()
            .flat_map(|idxs| {
                self.chord_hash_to_name
                    .get(&idxs.iter().map(|i| notes[*i]).sorted().join(","))
            })
            .next()
            .cloned()
    }

    pub(crate) fn chord_to_notes(&self, x: &str) -> Vec<usize> {
        self.chord_name_to_notes[x].clone()
    }
}

impl Default for ChordGuesser {
    fn default() -> Self {
        Self::new()
    }
}

struct Player {
    _client: Client,
    output_port: OutputPort,
    destination: Destination,
}
impl Player {
    fn new(name: &str) -> Self {
        let client = Client::new("Example Client").unwrap();
        let output_port = client.output_port("Example Port").unwrap();
        let destination = get_destination(name);
        Self {
            _client: client,
            output_port,
            destination,
        }
    }
    fn play_on(&self, note: u8) {
        self.output_port
            .send(
                &self.destination,
                &PacketBuffer::new(0, &[0x90, note & 0x7f, 127]),
            )
            .unwrap();
    }
    fn play_off(&self, note: u8) {
        self.output_port
            .send(
                &self.destination,
                &PacketBuffer::new(0, &[0x80, note & 0x7f, 127]),
            )
            .unwrap();
    }
}
