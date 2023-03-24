
fn compute_chords_from_audio() {
    let guesser = ChordGuesser::default();
    let mut buffer = VecDeque::<f32>::new();
    let device = device_with_prefix("Black");
    let config = device.default_input_config().unwrap();
    let freq_indices = freq_indices(config.sample_rate().0);
    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                buffer.extend(data.iter().map(|x| x.to_f32().unwrap()));
                if buffer.len() < BUFFER_SIZE {
                    return;
                }
                while buffer.len() > BUFFER_SIZE {
                    buffer.pop_front();
                }
                let mut buffer = buffer.iter().map_into().collect_vec();

                FftPlanner::<f32>::new()
                    .plan_fft_forward(BUFFER_SIZE)
                    .process(&mut buffer);
                let norms = buffer.iter().map(|x| x.norm()).collect_vec();
                let best_notes = freq_indices
                    .iter()
                    .map(|x| {
                        x.iter()
                            .map(|y| {
                                norms[*y]
                                    + (if *y > 40 {
                                        norms[*y - 1] + norms[*y + 1]
                                    } else {
                                        0f32
                                    })
                            })
                            .sum::<f32>()
                    })
                    .enumerate()
                    .sorted_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap())
                    .map(|(i, _)| i)
                    .take(4)
                    .collect_vec();
                println!("{}", best_notes.iter().map(|x| NOTE_NAMES[*x]).join(" "));
                if let Some(x) = guesser.guess(&best_notes) {
                    println!("{}", x);
                }
                plot(&norms[..CHART_SIZE]);
            },
            |err| eprintln!("an error occurred on the output audio stream: {}", err),
            None,
        )
        .unwrap();
    stream.play().unwrap();
    stdin().lock().lines().next();
    stream.pause().unwrap();
}

fn device_with_prefix(prefix: &str) -> cpal::Device {
    cpal::default_host()
        .devices()
        .unwrap()
        .find(|x| x.name().unwrap().starts_with(prefix))
        .unwrap()
}

fn plot<T>(data: &[T])
where
    T: ToPrimitive,
{
    Chart::new(120, 60, 0f32, data.len() as f32 - 1f32)
        .lineplot(&textplots::Shape::Lines(
            &data
                .iter()
                .enumerate()
                .map(|(x, y)| (x.to_f32().unwrap(), y.to_f32().unwrap()))
                .collect_vec(),
        ))
        .display();
}
