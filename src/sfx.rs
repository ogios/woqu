use alsa::pcm::{Access, Format, HwParams, PCM};
use std::error::Error;
use std::path::Path;
use std::sync::atomic::AtomicPtr;

static DATA: AtomicPtr<(hound::WavSpec, std::vec::Vec<i16>)> = AtomicPtr::new(std::ptr::null_mut());

pub fn load_data(path: &str) {
    let data = load_wav_file(path).unwrap();
    DATA.store(
        Box::into_raw(Box::new(data)),
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn load_wav_file<P: AsRef<Path>>(path: P) -> Result<(hound::WavSpec, Vec<i16>), Box<dyn Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    // 这里假设 WAV 文件的数据格式为 PCM 16位
    let samples_result: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    let samples = samples_result?;
    Ok((spec, samples))
}

fn get_data_ref() -> Option<&'static (hound::WavSpec, std::vec::Vec<i16>)> {
    unsafe { DATA.load(std::sync::atomic::Ordering::Relaxed).as_ref() }
}

// NOTE: IF WE ENCOUNTERED ANY BUG RELATED TO THIS, TRY THIS
// fn clone() -> (hound::WavSpec, std::vec::Vec<i16>) {
//     let data = unsafe {
//         DATA.load(std::sync::atomic::Ordering::Relaxed)
//             .as_ref()
//             .unwrap()
//     };
//     data.clone()
// }
pub fn init_thread_pool(threads: Option<usize>) {
    let mut builder = affinitypool::Builder::new();
    if let Some(threads) = threads {
        builder = builder.worker_threads(threads);
    } else {
        builder = builder.thread_per_core(true);
    }
    POOL.store(
        Box::into_raw(Box::new(builder.build())),
        std::sync::atomic::Ordering::Relaxed,
    );
}

static POOL: AtomicPtr<affinitypool::Threadpool> = AtomicPtr::new(std::ptr::null_mut());

fn get_pool_ref() -> &'static affinitypool::Threadpool {
    unsafe { POOL.load(std::sync::atomic::Ordering::Relaxed).as_ref() }.unwrap()
}

pub fn spawn_play() {
    tokio::spawn(get_pool_ref().spawn(|| {
        let Some((spec, samples)) = get_data_ref() else {
            return;
        };

        thread_local! {
            #[allow(non_upper_case_globals)]
            static pcm: PCM = {
                PCM::new("default", alsa::Direction::Playback, false).unwrap()
            };
        };

        pcm.with(|p| {
            let hwp = HwParams::any(p).unwrap();
            hwp.set_channels(spec.channels as u32).unwrap();
            hwp.set_rate(spec.sample_rate, alsa::ValueOr::Nearest)
                .unwrap();
            hwp.set_format(Format::s16()).unwrap();
            hwp.set_access(Access::RWInterleaved).unwrap();
            p.hw_params(&hwp).unwrap();
            let io = p.io_i16().unwrap();

            p.prepare().unwrap();
            play_samples(&io, samples, spec.channels).unwrap();
            p.drain().unwrap();
        });
    }));

    // tokio::spawn(async {
    //     let Some((spec, samples)) = get_data_ref() else {
    //         return;
    //     };
    //
    //     thread_local! {
    //         #[allow(non_upper_case_globals)]
    //         static pcm: PCM = {
    //             println!("Create PCM instance");
    //             PCM::new("default", alsa::Direction::Playback, false).unwrap()
    //         };
    //     };
    //
    //     pcm.with(|p| {
    //         let hwp = HwParams::any(p).unwrap();
    //         hwp.set_channels(spec.channels as u32).unwrap();
    //         hwp.set_rate(spec.sample_rate, alsa::ValueOr::Nearest)
    //             .unwrap();
    //         hwp.set_format(Format::s16()).unwrap();
    //         hwp.set_access(Access::RWInterleaved).unwrap();
    //         p.hw_params(&hwp).unwrap();
    //         let io = p.io_i16().unwrap();
    //
    //         p.prepare().unwrap();
    //         play_samples(&io, samples, spec.channels).unwrap();
    //         p.drain().unwrap();
    //     });
    // });
}

/// 分块写入数据到 PCM 设备进行播放
fn play_samples(
    io: &alsa::pcm::IO<i16>,
    samples: &[i16],
    channels: u16,
) -> Result<(), Box<dyn Error>> {
    let total_frames = samples.len() / channels as usize;
    let frames_per_chunk = 256; // 可根据实际情况调整分块大小
    let mut frame_index = 0;

    while frame_index < total_frames {
        // 计算本次写入的数据范围（注意数据是 interleaved 的）
        let start = frame_index * channels as usize;
        let end = std::cmp::min(start + frames_per_chunk * channels as usize, samples.len());
        // 写入采样数据，返回写入的帧数
        match io.writei(&samples[start..end]) {
            Ok(frames_written) => {
                // 写入成功则推进对应的帧数
                frame_index += frames_written;
            }
            Err(err) => {
                eprintln!("写入 PCM 设备时出现错误: {:?}", err);
                // 如果错误是暂时性的，考虑重试；这里简单直接退出播放
                break;
            }
        }
    }
    Ok(())
}
