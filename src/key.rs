use colpetto::{Event, Libinput, Result, event::KeyboardEvent};
use rustix::{
    fd::{FromRawFd, IntoRawFd, OwnedFd},
    fs::{Mode, OFlags, open},
};
use std::{
    ffi::{CStr, c_int},
    os::fd::RawFd,
};
use tokio_stream::StreamExt;

fn open_restricted(path: &CStr, flags: c_int) -> Result<RawFd, c_int> {
    open(path, OFlags::from_bits_retain(flags as u32), Mode::empty())
        .map(IntoRawFd::into_raw_fd)
        .map_err(|err| err.raw_os_error().wrapping_neg())
}

fn close_restricted(fd: RawFd) {
    drop(unsafe { OwnedFd::from_raw_fd(fd) });
}

pub async fn watch_for_keys(mut cb: impl FnMut()) -> Result<()> {
    let mut libinput = Libinput::new(open_restricted, close_restricted)?;
    libinput.udev_assign_seat(c"seat0")?;

    let mut stream = libinput.event_stream()?;

    while let Some(event) = stream.try_next().await? {
        if let Event::Keyboard(KeyboardEvent::Key(_)) = event {
            cb()
        };
    }

    Ok(())
}
