use alsa::pcm::{Access, Format, HwParams, PCM};
use std::collections::VecDeque;
use std::error::Error;
use std::path::Path;
use std::sync::atomic::AtomicPtr;
use std::sync::{Arc, Mutex};

static DATA: AtomicPtr<(hound::WavSpec, std::vec::Vec<i16>)> = AtomicPtr::new(std::ptr::null_mut());

pub fn load_data(path: &str, vol_gain: Option<f32>) {
    let mut data = load_wav_file(path).unwrap();
    if let Some(gain) = vol_gain {
        volumn_up_samples(&mut data.1, gain);
    }
    DATA.store(
        Box::into_raw(Box::new(data)),
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn load_wav_file<P: AsRef<Path>>(path: P) -> Result<(hound::WavSpec, Vec<i16>), Box<dyn Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    // assume PCM 16bits
    let samples_result: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    let samples = samples_result?;
    Ok((spec, samples))
}

fn get_data_ref() -> Option<&'static (hound::WavSpec, std::vec::Vec<i16>)> {
    unsafe { DATA.load(std::sync::atomic::Ordering::Relaxed).as_ref() }
}

// static POOL: AtomicPtr<affinitypool::Threadpool> = AtomicPtr::new(std::ptr::null_mut());
//
// pub fn init_pool(threads_count: Option<usize>) {
//     let mut builder = affinitypool::Builder::new();
//     if let Some(threads_count) = threads_count {
//         builder = builder.worker_threads(threads_count);
//     } else {
//         builder = builder.thread_per_core(true);
//     }
//     POOL.store(
//         Box::into_raw(Box::new(builder.build())),
//         std::sync::atomic::Ordering::Relaxed,
//     );
// }
// fn get_pool() -> &'static affinitypool::Threadpool {
//     unsafe { POOL.load(std::sync::atomic::Ordering::Relaxed).as_ref() }.unwrap()
// }

/// 播放音轨结构体
pub struct ActiveTrack {
    samples: Vec<i16>,
    pos: usize,
    channels: usize,
}

pub fn push(tracks: &Arc<Mutex<VecDeque<ActiveTrack>>>) {
    if let Some((_, samples)) = get_data_ref() {
        let mut q = tracks.lock().unwrap();
        q.push_back(ActiveTrack {
            samples: samples.clone(),
            pos: 0,
            channels: 2,
        });
    }
}

/// 简单混音：将多个音轨当前帧累加后裁剪
fn mix(tracks: &mut VecDeque<ActiveTrack>, frames: usize, channels: usize) -> Vec<i16> {
    let mut mix_buffer = vec![0i32; frames * channels];
    tracks.retain_mut(|track| {
        let remain = track.samples.len() - track.pos;
        let can_read = (frames * channels).min(remain);
        (0..can_read).for_each(|i| {
            mix_buffer[i] += track.samples[track.pos + i] as i32;
        });
        track.pos += can_read;
        track.pos < track.samples.len()
    });

    // 裁剪为 i16 输出
    mix_buffer
        .into_iter()
        .map(|s| s.clamp(i16::MIN as i32, i16::MAX as i32) as i16)
        .collect()
}

// pub fn spawn_play() {
//     tokio::spawn(get_pool().spawn(|| {
//         let Some((spec, samples)) = get_data_ref() else {
//             return;
//         };
//
//         thread_local!(
//             #[allow(non_upper_case_globals)]
//             static pcm: PCM = PCM::new("pipewire", alsa::Direction::Playback, false).unwrap()
//         );
//         pcm.with(|p| {
//             let hwp = HwParams::any(p).unwrap();
//             hwp.set_channels(spec.channels as u32).unwrap();
//             hwp.set_rate(spec.sample_rate, alsa::ValueOr::Nearest)
//                 .unwrap();
//             hwp.set_format(Format::s16()).unwrap();
//             hwp.set_access(Access::RWInterleaved).unwrap();
//             p.hw_params(&hwp).unwrap();
//             let io = p.io_i16().unwrap();
//
//             p.prepare().unwrap();
//             play_samples(&io, samples, spec.channels).unwrap();
//             p.drain().unwrap();
//         })
//     }));
// }

pub fn loop_play(tracks: Arc<Mutex<VecDeque<ActiveTrack>>>) {
    let rate = 44100;
    let channels = 2;
    let frames = 256;

    let pcm = PCM::new("pipewire", alsa::Direction::Playback, false).unwrap();
    let hwp = HwParams::any(&pcm).unwrap();
    hwp.set_channels(channels as u32).unwrap();
    hwp.set_rate(rate, alsa::ValueOr::Nearest).unwrap();
    hwp.set_format(Format::s16()).unwrap();
    hwp.set_access(Access::RWInterleaved).unwrap();
    pcm.hw_params(&hwp).unwrap();
    let io = pcm.io_i16().unwrap();

    loop {
        let mixed = {
            let mut q = tracks.lock().unwrap();
            mix(&mut q, frames, channels as usize)
        };
        let mut written = 0;
        if mixed.iter().all(|i| *i == 0) {
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }
        while written < mixed.len() {
            match io.writei(&mixed[written..]) {
                Ok(frames_written) => {
                    written += frames_written * channels as usize;
                }
                Err(err) => {
                    if pcm.state() == alsa::pcm::State::XRun {
                        pcm.prepare().unwrap();
                    } else {
                        eprintln!("写入错误：{}", err);
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn play_samples(
    io: &alsa::pcm::IO<i16>,
    samples: &[i16],
    channels: u16,
) -> Result<(), Box<dyn Error>> {
    let total_frames = samples.len() / channels as usize;
    let frames_per_chunk = 1024;
    let mut frame_index = 0;

    while frame_index < total_frames {
        let start = frame_index * channels as usize;
        let end = std::cmp::min(start + frames_per_chunk * channels as usize, samples.len());
        match io.writei(&samples[start..end]) {
            Ok(frames_written) => {
                frame_index += frames_written;
            }
            Err(err) => {
                eprintln!("error writing PCM: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}

fn volumn_up_samples(samples: &mut [i16], gain: f32) {
    for sample in samples.iter_mut() {
        let v = (*sample as f32) * gain;
        *sample = v.max(i16::MIN as f32).min(i16::MAX as f32).round() as i16;
    }
}
