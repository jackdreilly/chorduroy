#![feature(iter_collect_into)]

mod model;

use coremidi::{Client, Destination, Destinations, OutputPort, Protocol};
use coremidi::{PacketBuffer, Sources};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{default_host, StreamConfig, SupportedBufferSize};
use model::{Chord, Model, Note, Observation};
use rustfft::FftPlanner;
use serde::Serialize;
use strum::IntoEnumIterator;
use textplots::{Chart, Plot};
use websocket::Message;

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::io::{stdin, BufRead};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

use clap::Parser;
use itertools::Itertools;

use crate::model::NUM_CHORDS;

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
    #[arg(long, default_value_t = false)]
    max_buffer: bool,
    #[arg(short, long, default_value_t = false)]
    chrome: bool,
}

#[derive(Debug)]
enum Event {
    Note(bool, u8),
    Chord(Chord),
}

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
        publish_chords_from_audio(
            audio.unwrap_or_else(|| "Black".into()),
            milliseconds,
            tx,
            t_chords,
            args,
        );
        block();
    });

    thread::spawn(move || {
        let _hack = publish_midi_in_events(source.unwrap_or_else(|| "OP-1".into()), tx2);
        block();
    });
    output_remapped_midi_notes(destination, rx);
}

fn output_remapped_midi_notes(destination: Option<String>, rx: mpsc::Receiver<Event>) {
    let player = Player::new(&destination.unwrap_or_else(|| "Garage".into()));
    let mut active_chord = Chord::from(0);
    let mut midi_note_remapping_history = vec![0; 256];
    for event in rx {
        match event {
            Event::Chord(chord) => {
                active_chord = chord;
            }
            Event::Note(on, note) => {
                dbg!(on, note, active_chord);
                if !on {
                    player.play_off(midi_note_remapping_history[note as usize]);
                    continue;
                }
                let new_note = active_chord
                    .notes()
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

fn publish_chords_from_audio(
    audio: String,
    milliseconds: u32,
    tx: mpsc::Sender<Event>,
    t_chords: mpsc::Sender<WebPayload>,
    Args {
        max_buffer,
        octaves,
        low_octave,
        plot,
        ..
    }: Args,
) {
    let device = get_audio_device(audio);
    let input_config = device.default_input_config().unwrap();
    let mut config: StreamConfig = input_config.clone().into();
    if max_buffer {
        config.buffer_size = match &input_config.buffer_size() {
            SupportedBufferSize::Range { max, .. } => cpal::BufferSize::Fixed(*max),
            SupportedBufferSize::Unknown => cpal::BufferSize::Default,
        };
    }
    let mut buffer: Vec<f32> = vec![];
    let model = Model::default();
    let mut observations: VecDeque<Observation> = VecDeque::new();
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                buffer.reserve(data.len() / config.channels as usize);
                data.chunks_exact(config.channels as usize)
                    .map(|c| c.iter().sum::<f32>())
                    .collect_into(&mut buffer);
                let max_size = config.sample_rate.0 as usize * milliseconds as usize / 1000;
                if buffer.len() > max_size {
                    buffer.drain(0..data.len() / config.channels as usize);
                }
                let data = &buffer;
                let rate = config.sample_rate.0 as f32;
                let mut fft = data.iter().map_into().collect_vec();
                FftPlanner::new()
                    .plan_fft_forward(data.len())
                    .process(&mut fft);
                let fft = fft[0..data.len() / 2]
                    .iter_mut()
                    .take(250)
                    .map(|f| f.norm())
                    .collect_vec();
                let full_q = (0..12)
                    .map(move |note| {
                        (low_octave..octaves + low_octave)
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
                                sum_real.hypot(sum_imag) * (octave + 2) as f32
                            })
                            .collect_vec()
                    })
                    .collect_vec();
                let bars = full_q
                    .iter()
                    .map(|x| x.iter().sum::<f32>() / x.len() as f32)
                    .collect_vec();
                observations.push_back(Observation::from_row_slice(&bars));
                if observations.len() > NUM_CHORDS {
                    observations.pop_front();
                }

                if plot {
                    Chart::new(100, 50, 0f32, 11f32)
                        .lineplot(&textplots::Shape::Bars(
                            &bars
                                .iter()
                                .enumerate()
                                .map(|(i, x)| (i as f32, *x))
                                .collect_vec(),
                        ))
                        .display();
                }
                let chord = model
                    .infer_viterbi(observations.make_contiguous())
                    .last()
                    .copied()
                    .unwrap();
                tx.send(Event::Chord(chord)).unwrap();
                t_chords
                    .send(WebPayload {
                        full_q: FullQ {
                            x: Note::iter().collect(),
                            y: full_q,
                        },
                        bucketed_q: BucketedQ {
                            y: bars,
                            x: Note::iter().collect(),
                        },
                        chord,
                        fft,
                    })
                    .unwrap();
            },
            |err| eprintln!("an error occurred on the output audio stream: {err}"),
            None,
        )
        .unwrap();
    stream.play().unwrap();
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
