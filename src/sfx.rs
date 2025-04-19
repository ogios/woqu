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
