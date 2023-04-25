#![feature(iter_collect_into, default_free_fn)]

mod model;

use aubio::Onset;
use chords::{Chord, Note, Scale, ScaleBuilder};
use coremidi::{Client, Destination, Destinations, OutputPort, Protocol};
use coremidi::{PacketBuffer, Sources};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{default_host, StreamConfig, SupportedBufferSize};
use model::{Model, Observation};
use num::ToPrimitive;
use serde::Serialize;
use strum::EnumCount;
use websocket::Message;

use std::collections::{HashSet, VecDeque};
use std::f32::consts::PI;
use std::io::{stdin, BufRead};
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::{default::default, thread};

use clap::Parser;
use itertools::Itertools;

use crate::model::NUM_CHORDS;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum NoteMode {
    Chord,
    Continuous,
    Chromatic,
}

#[derive(Serialize)]
struct FullQ {
    x: Vec<Note>,
    y: Vec<Vec<f32>>,
}
#[derive(Serialize)]
struct BucketedQ {
    x: Vec<Note>,
    y: Vec<f32>,
}
#[derive(Serialize)]
struct WebPayload {
    full_q: FullQ,
    bucketed_q: BucketedQ,
    chord: Chord,
    fft: Vec<f32>,
    beat: bool,
    observations: Observations,
}
#[derive(Serialize)]
struct Observations {
    x: Vec<Note>,
    y: Vec<Vec<f32>>,
    chords: Vec<Chord>,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 200)]
    milliseconds: u32,
    #[arg(short, long, default_value_t = 5)]
    octaves: u32,
    #[arg(short, long, default_value_t = 0)]
    low_octave: u32,
    #[arg(short, long)]
    source: Option<String>,
    #[arg(short, long)]
    destination: Option<String>,
    #[arg(short, long)]
    audio: Option<String>,
    #[arg(short, long, default_value_t = false)]
    plot: bool,
    #[arg(long, default_value_t = true)]
    max_buffer: bool,
    #[arg(short, long, default_value_t = false)]
    chrome: bool,
    #[arg(long, default_value_t = false)]
    disable_output: bool,
}

#[derive(Debug)]
enum Event {
    Note(bool, u8),
    Chords(Chords),
    NoteMode(NoteMode),
}
type Chords = Vec<Chord>;

fn main() {
    let args = Args::parse();
    let Args {
        milliseconds,
        source,
        destination,
        audio,
        chrome,
        ..
    } = args.clone();
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    let (t_chords, r_chords) = mpsc::channel::<WebPayload>();
    let (t_audio, r_audio) = mpsc::channel::<Vec<f32>>();
    let beat_mutex = Arc::new(Mutex::new(false));
    let beat_mutex_beat = beat_mutex.clone();
    thread::spawn(move || {
        let mut beat = Onset::new(aubio::OnsetMode::SpecFlux, 1024, 512, 44100).unwrap();
        for data in r_audio {
            if beat.do_result(&data).unwrap() > 0.0 {
                *beat_mutex_beat.lock().unwrap() = true;
            }
        }
    });
    let tx_input = tx.clone();
    thread::spawn(move || loop {
        match stdin().lock().lines().next().unwrap().unwrap().as_str() {
            "c" => tx_input.send(Event::NoteMode(NoteMode::Chord)).unwrap(),
            "a" => tx_input.send(Event::NoteMode(NoteMode::Chromatic)).unwrap(),
            "n" => tx_input
                .send(Event::NoteMode(NoteMode::Continuous))
                .unwrap(),
            _ => {}
        };
    });
    thread::spawn(move || {
        if chrome {
            Command::new("open")
                .args(["/Applications/Google Chrome.app"])
                .output()
                .unwrap();
        }
        // Run the command, forwarding output to stdout.
        Command::new("deno")
            .args(["task", "--cwd", "web", "start"])
            .status()
            .unwrap();
    });
    thread::spawn(move || {
        use websocket::sync::Server;
        let server = Server::bind("127.0.0.1:1234").unwrap();
        for client in server.filter_map(Result::ok) {
            let mut client = client.accept().unwrap();
            loop {
                let chord = r_chords.recv().unwrap();
                if client
                    .send_message(&Message::text(&serde_json::to_string(&chord).unwrap()))
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    thread::spawn(move || {
        {
            let audio = audio.unwrap_or_else(|| "Black".into());
            let Args {
                max_buffer,
                octaves,
                low_octave,
                ..
            } = args;
            let device = get_audio_device(audio);
            let input_config = device.default_input_config().unwrap();
            let mut config: StreamConfig = input_config.clone().into();
            if max_buffer {
                config.buffer_size = match &input_config.buffer_size() {
                    SupportedBufferSize::Range { max, .. } => cpal::BufferSize::Fixed(*max),
                    SupportedBufferSize::Unknown => cpal::BufferSize::Default,
                };
            }
            let mut buffer: VecDeque<f32> = VecDeque::new();
            let model = Model::default();
            let mut observations: VecDeque<Observation> = [default()].into();
            let max_size = config.sample_rate.0 as usize * milliseconds as usize / 1000;
            let mut current_agg_count = 0.0;
            let stream = device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let data = data
                            .chunks_exact(config.channels as usize)
                            .map(|c| c.iter().sum::<f32>())
                            .collect_vec();
                        if data.iter().map(|f| f.abs()).sum::<f32>() < 1e-1 {
                            observations.clear();
                            observations.push_back(default());
                            current_agg_count = 0.0;
                            buffer.clear();
                        }
                        t_audio.send(data.clone()).unwrap();
                        buffer.extend(data);
                        buffer.drain(0..buffer.len().saturating_sub(max_size));
                        let beat = {
                            let mut lock = beat_mutex.lock().unwrap();
                            let beat = *lock;
                            *lock = false;
                            beat
                        };
                        let mut new_feature: Features = default();
                        for note in 0..12 {
                            for octave in low_octave..(low_octave + octaves) {
                                let bin = octave * 12 + note;
                                let f_k: F = 65.40639 * 2.0f32.powf(bin as F / 12.0);
                                let n_k = (config.sample_rate.0 as F
                                    / (((2.0 as F).powf(1.0 / 12.0) - 1.0) * f_k))
                                    .min(buffer.len() as F);
                                let factor = f_k * -2.0 * (PI as F) / config.sample_rate.0 as F;
                                let mut sum_real: F = 0.0;
                                let mut sum_imag: F = 0.0;
                                for j in 0..n_k.floor() as usize {
                                    let j = j + (buffer.len() - n_k.floor() as usize) / 2;
                                    let d = buffer[j];
                                    let real_common = d / n_k;
                                    let (sin, cos) = (factor
                                        * (j as F + (n_k.floor() / 2.0) - buffer.len() as F / 2.0))
                                        .sin_cos();
                                    sum_real += real_common * cos;
                                    sum_imag += real_common * sin;
                                }
                                new_feature[note as usize] += sum_real.hypot(sum_imag);
                            }
                        }
                        new_feature.normalize_mut();
                        if beat {
                            observations.push_back(new_feature);
                            current_agg_count = 1.0;
                        } else {
                            let features = observations.back_mut().unwrap();
                            *features *= current_agg_count;
                            *features += new_feature;
                            (*features).normalize_mut();
                            current_agg_count += 1.0;
                        }
                        if observations.len() > NUM_CHORDS {
                            observations.pop_front();
                        }
                        if current_agg_count < 3.0 {
                            return;
                        }
                        let chords = model.infer_viterbi(observations.make_contiguous());
                        let chord = &chords[chords.len() - 1];

                        tx.send(Event::Chords(chords.clone())).unwrap();
                        t_chords
                            .send(WebPayload {
                                full_q: FullQ {
                                    x: Note::vec(),
                                    y: default(),
                                },
                                bucketed_q: BucketedQ {
                                    y: observations
                                        .iter()
                                        .last()
                                        .unwrap()
                                        .into_iter()
                                        .copied()
                                        .collect_vec(),
                                    x: Note::vec(),
                                },
                                chord: chord.clone(),
                                fft: default(),
                                beat,
                                observations: Observations {
                                    x: Note::vec(),
                                    y: observations
                                        .iter()
                                        .map(|x| x.iter().copied().collect())
                                        .collect(),
                                    chords,
                                },
                            })
                            .unwrap();
                    },
                    |err| eprintln!("an error occurred on the output audio stream: {err}"),
                    None,
                )
                .unwrap();
            stream.play().unwrap();
        };
        block();
    });

    thread::spawn(move || {
        let _hack = publish_midi_in_events(source.unwrap_or_else(|| "OP-1".into()), tx2);
        block();
    });
    output_remapped_midi_notes(destination, rx, args.disable_output);
}

fn output_remapped_midi_notes(
    destination: Option<String>,
    rx: mpsc::Receiver<Event>,
    disable_output: bool,
) {
    let player = Player::new(
        &destination.unwrap_or_else(|| "Garage".into()),
        disable_output,
    );
    let mut active_chord = "C".parse().unwrap();
    let mut scale: Scale = "C".parse().unwrap();
    let mut midi_note_remapping_history = vec![0; 256];
    let mut note_mode = NoteMode::Chord;
    for event in rx {
        match event {
            Event::NoteMode(mode) => {
                note_mode = mode;
            }
            Event::Chords(chords) => {
                active_chord = chords[chords.len() - 1].clone();
                scale = scale_from_chords(chords);
            }
            Event::Note(on, note) => {
                if !on {
                    player.play_off(midi_note_remapping_history[note as usize]);
                    continue;
                }
                let new_note = match note_mode {
                    NoteMode::Chord => (note - 3..=note)
                        .rev()
                        .find(|note| {
                            active_chord
                                .notes()
                                .iter()
                                .map(|x| x.to_u8().unwrap().rem_euclid(Note::COUNT as u8))
                                .contains(&note.rem_euclid(Note::COUNT as u8))
                        })
                        .unwrap_or(note),
                    _ => {
                        let octave = (note / 12) * 12;
                        let index = if note_mode == NoteMode::Chromatic {
                            note % 12
                        } else {
                            [0, 2, 2, 4, 4, 5, 7, 7, 9, 9, 11, 11][(note % 12) as usize]
                        };
                        let scale_offset = scale.root.to_u8().unwrap();
                        octave + index + scale_offset
                    }
                };
                midi_note_remapping_history[note as usize] = new_note;
                player.play_on(new_note);
                dbg!(note, new_note, &active_chord, scale, note_mode);
            }
        }
    }
}

fn publish_midi_in_events(
    source: String,
    tx: mpsc::Sender<Event>,
) -> (Client, coremidi::InputPortWithContext<u32>) {
    let source = get_source(source);
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
    (client, input_port)
}

fn get_source(source: String) -> coremidi::Source {
    Sources
        .into_iter()
        .find(|x| x.name().unwrap().contains(&source))
        .unwrap()
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
struct Player {
    _client: Client,
    output_port: OutputPort,
    destination: Destination,
    disable_output: bool,
}
impl Player {
    fn new(name: &str, disable_output: bool) -> Self {
        let client = Client::new("Example Client").unwrap();
        let output_port = client.output_port("Example Port").unwrap();
        let destination = get_destination(name);
        Self {
            _client: client,
            output_port,
            destination,
            disable_output,
        }
    }
    fn play_on(&self, note: u8) {
        if self.disable_output {
            return;
        }
        self.output_port
            .send(
                &self.destination,
                &PacketBuffer::new(0, &[0x90, note & 0x7f, 127]),
            )
            .unwrap();
    }
    fn play_off(&self, note: u8) {
        if self.disable_output {
            return;
        }
        self.output_port
            .send(
                &self.destination,
                &PacketBuffer::new(0, &[0x80, note & 0x7f, 127]),
            )
            .unwrap();
    }
}

type Features = Observation;
type F = f32;

fn scale_from_chords(chords: Chords) -> Scale {
    let mut candidates = Note::vec()
        .into_iter()
        .flat_map(|root| ScaleBuilder::default().root(root).build())
        .collect_vec();
    let mut all_notes = HashSet::<Note>::new();
    for chord in chords.into_iter().rev() {
        all_notes.extend(chord.notes());
        let remaining_candidates = candidates
            .iter()
            .copied()
            .filter(|scale| HashSet::from_iter(scale.notes().into_iter()).is_superset(&all_notes))
            .collect_vec();
        match remaining_candidates.len() {
            0 => return candidates[0],
            1 => return remaining_candidates[0],
            _ => {
                candidates = remaining_candidates;
            }
        }
    }
    candidates[0]
}
