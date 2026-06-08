use colpetto::{
    Event, Libinput, Result,
    event::{AsRawEvent, KeyState, KeyboardEvent, PointerEvent},
};
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

pub async fn watch_for_keys(mut cb: impl FnMut(), keyboard: bool, mouse: bool) -> Result<()> {
    let mut libinput = Libinput::new(open_restricted, close_restricted)?;
    libinput.udev_assign_seat(c"seat0")?;

    let mut stream = libinput.event_stream()?;

    while let Some(event) = stream.try_next().await? {
        match &event {
            Event::Keyboard(KeyboardEvent::Key(l))
                if keyboard && l.key_state() == KeyState::Pressed =>
            {
                cb()
            }
            Event::Pointer(PointerEvent::Button(a))
                if mouse
                    && unsafe {
                        colpetto::sys::libinput_event_pointer_get_button_state(
                            colpetto::sys::libinput_event_get_pointer_event(a.as_raw_event()),
                        )
                    } == colpetto::sys::libinput_button_state::LIBINPUT_BUTTON_STATE_PRESSED =>
            {
                cb()
            }
            _ => {}
        }
    }

    Ok(())
}
