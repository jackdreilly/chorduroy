mod chord_guesser;
mod utils;
use chord_guesser::NOTE_NAMES;
use coremidi::{Client, Destination};

use itertools::Itertools;

use std::{io::BufRead, thread, time::Duration};

const BUFFER_SIZE: usize = 1 << 14;
const CHART_SIZE: usize = 200;

fn freq_indices(sample_rate: u32) -> Vec<Vec<usize>> {
    (0..NOTE_NAMES.len())
        .into_iter()
        .map(|i| {
            let base =
                440f32 * BUFFER_SIZE as f32 * 2f32.powf(i as f32 / 12f32) / sample_rate as f32;
            let mut output = vec![base.round() as usize];
            let mut last = base * 2f32;
            while last < BUFFER_SIZE as f32 {
                output.push(last.round() as usize);
                last *= 2f32;
            }
            let mut last = base / 2f32;
            while last > 0f32 {
                output.push(last.round() as usize);
                last /= 2f32;
            }
            output
        })
        .collect_vec()
}
fn main() {
    let source = Sources.into_iter().next().unwrap();
    let source_id = source.unique_id().unwrap_or_default();
    let client = Client::new("Example Client").unwrap();

    let output_client = Client::new("example-client").unwrap();
    let output_port = output_client.output_port("example-port").unwrap();
    let destination = Destination::from_index(0).unwrap();
    let chord_on = EventBuffer::new(Protocol::Midi10)
        .with_packet(0, &[0x2090407f])
        .with_packet(0, &[0x2090447f]);
    let chord_off = EventBuffer::new(Protocol::Midi10)
        .with_packet(0, &[0x2080407f])
        .with_packet(0, &[0x2080447f]);
    output_port.send(&destination, &chord_on).unwrap();
    thread::sleep(Duration::from_millis(1000));
    output_port.send(&destination, &chord_off).unwrap();
    let callback = move |event_list: &EventList, context: &mut u32| {
        for event in event_list.iter() {
            if event.data()[0] == 0x10f80000 {
                return;
            }
        }
        output_port.send(&destination, &chord_on).unwrap();
        thread::sleep(Duration::from_millis(1000));
        output_port.send(&destination, &chord_off).unwrap();
    };
    let mut input_port = client
        .input_port_with_protocol("Example Port", Protocol::Midi10, callback)
        .unwrap();

    input_port.connect_source(&source, source_id).unwrap();

    let mut input_line = String::new();
    println!("Press Enter to Finish");
    std::io::stdin()
        .read_line(&mut input_line)
        .expect("Failed to read line");
    input_port.disconnect_source(&source).unwrap();
}
