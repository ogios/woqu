use std::path::Path;

use clap::Parser;
use key::watch_for_keys;
use rustix::{path::Arg, process::geteuid};
use sdl2::mixer::{AUDIO_S16LSB, InitFlag};

mod key;

#[derive(Debug, Parser)]
pub struct Cli {
    #[arg(short = 'f', long)]
    pub file: String,
    #[arg(short = 'v', long)]
    pub vol_gain: Option<f32>,
    /// how many audios to play at the same time, default 8.
    #[arg(short = 'c', long)]
    pub channels: Option<u32>,
}

fn main() {
    fn is_root() -> bool {
        geteuid().is_root()
    }

    fn is_in_input_group() -> bool {
        let out = std::process::Command::new("groups").output().unwrap();
        let res = out.stdout.to_string_lossy();
        res.contains("input")
    }

    if !is_root() && !is_in_input_group() {
        eprintln!(
            "WARN: This program requires root privileges or membership in the 'input' group."
        );
    }

    println!("Press any key to play the sound");

    let cli = Cli::parse();
    let sdl = sdl2::init().unwrap();
    let _audio = sdl.audio().unwrap();

    println!("SDL initialized");

    std::thread::spawn(move || {
        let samples = {
            let (spec, mut samples) = load_wav_file(&cli.file);
            // volume gain
            if let Some(gain) = cli.vol_gain {
                volumn_up_samples(&mut samples, gain);
            }

            let frequency = spec.sample_rate as i32;
            let channels = spec.channels as i32;
            let format = AUDIO_S16LSB; // signed 16 bit samples, in little-endian byte order
            let chunk_size = 256;
            sdl2::mixer::open_audio(frequency, format, channels, chunk_size).unwrap();

            samples
        };
        let _mixer_context = sdl2::mixer::init(InitFlag::MP3 | InitFlag::OGG).unwrap();

        let sound = sdl2::mixer::Chunk::from_raw_buffer(samples.into_boxed_slice()).unwrap();
        let rt = tokio::runtime::LocalRuntime::new().unwrap();
        let channel = sdl2::mixer::Channel::all();
        if let Some(channels) = cli.channels {
            sdl2::mixer::allocate_channels(channels as i32);
        }

        println!("Audio initialized");

        rt.block_on(async {
            let p = || {
                let _ = channel.play(&sound, 0);
            };
            watch_for_keys(p).await.unwrap();
        })
    });

    let mut event_pump = sdl.event_pump().unwrap();
    for event in event_pump.wait_iter() {
        if let sdl2::event::Event::Quit { .. } = event {
            std::process::exit(0);
        }
    }
}

fn volumn_up_samples(samples: &mut [i16], gain: f32) {
    for sample in samples.iter_mut() {
        let v = (*sample as f32) * gain;
        *sample = v.max(i16::MIN as f32).min(i16::MAX as f32).round() as i16;
    }
}

fn load_wav_file<P: AsRef<Path>>(path: P) -> (hound::WavSpec, Vec<i16>) {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    // assume PCM 16bits
    let samples_result: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    let samples = samples_result.unwrap();
    (spec, samples)
}
