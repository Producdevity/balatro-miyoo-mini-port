use anyhow::Result;
use love_api::state::SharedState;

// The Miyoo kernel exposes the legacy 32-bit evdev record layout. Rust's
// arm-musl timeval uses 64-bit time fields, so libc::timeval is not ABI-safe here.
#[cfg(any(target_os = "linux", test))]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputEvent32 {
    time_sec: i32,
    time_usec: i32,
    event_type: u16,
    code: u16,
    value: i32,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum HandheldButton {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    Menu,
}

#[cfg(any(target_os = "linux", test))]
impl HandheldButton {
    fn key(self) -> &'static str {
        match self {
            Self::Up => "miyoo_up",
            Self::Down => "miyoo_down",
            Self::Left => "miyoo_left",
            Self::Right => "miyoo_right",
            Self::A => "miyoo_a",
            Self::B => "miyoo_b",
            Self::X => "miyoo_x",
            Self::Y => "miyoo_y",
            Self::L1 => "miyoo_l1",
            Self::R1 => "miyoo_r1",
            Self::L2 => "miyoo_l2",
            Self::R2 => "miyoo_r2",
            Self::Select => "miyoo_select",
            Self::Start => "miyoo_start",
            Self::Menu => "miyoo_menu",
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn button_for_code(code: u16) -> Option<HandheldButton> {
    match code {
        103 | 0x220 => Some(HandheldButton::Up),
        108 | 0x221 => Some(HandheldButton::Down),
        105 | 0x222 => Some(HandheldButton::Left),
        106 | 0x223 => Some(HandheldButton::Right),
        57 | 0x130 => Some(HandheldButton::A),
        29 | 0x131 => Some(HandheldButton::B),
        42 | 0x133 => Some(HandheldButton::X),
        56 | 0x134 => Some(HandheldButton::Y),
        18 | 0x136 => Some(HandheldButton::L1),
        20 | 0x137 => Some(HandheldButton::R1),
        15 | 0x138 => Some(HandheldButton::L2),
        14 | 0x139 => Some(HandheldButton::R2),
        97 | 0x13a => Some(HandheldButton::Select),
        28 | 0x13b => Some(HandheldButton::Start),
        1 | 102 | 354 | 0x13c => Some(HandheldButton::Menu),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use love_api::state::LoveEvent;
    use std::collections::HashSet;
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::sync::{LazyLock, Mutex};

    const EV_KEY: u16 = 1;
    const EV_ABS: u16 = 3;
    const ABS_HAT0X: u16 = 16;
    const ABS_HAT0Y: u16 = 17;

    struct Input {
        devices: Vec<File>,
        held: HashSet<HandheldButton>,
        seen_codes: HashSet<u16>,
        hat_x: i32,
        hat_y: i32,
        retry_frames: u32,
    }

    impl Input {
        fn new() -> Self {
            let mut input = Self {
                devices: Vec::new(),
                held: HashSet::new(),
                seen_codes: HashSet::new(),
                hat_x: 0,
                hat_y: 0,
                retry_frames: 0,
            };
            input.open_devices();
            input
        }

        fn open_devices(&mut self) {
            let candidates: Vec<String> = match std::env::var("BALATRO_INPUT_DEVICE") {
                Ok(path) => vec![path],
                Err(_) => (0..16)
                    .map(|index| format!("/dev/input/event{index}"))
                    .collect(),
            };
            self.devices = candidates
                .into_iter()
                .filter_map(|path| {
                    OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(path)
                        .ok()
                })
                .collect();
            if !self.devices.is_empty() {
                eprintln!(
                    "[input] opened {} evdev device(s), event size {}",
                    self.devices.len(),
                    std::mem::size_of::<InputEvent32>()
                );
            }
        }

        fn poll(&mut self, state: &SharedState) {
            if self.devices.is_empty() {
                self.retry_frames += 1;
                if self.retry_frames >= 60 {
                    self.retry_frames = 0;
                    self.open_devices();
                }
                return;
            }

            let mut events = Vec::with_capacity(16);
            for device in &self.devices {
                loop {
                    let mut event = InputEvent32::default();
                    let result = unsafe {
                        libc::read(
                            device.as_raw_fd(),
                            (&mut event as *mut InputEvent32).cast(),
                            std::mem::size_of::<InputEvent32>(),
                        )
                    };
                    if result == std::mem::size_of::<InputEvent32>() as isize {
                        events.push(event);
                        continue;
                    }
                    break;
                }
            }

            for event in events {
                match event.event_type {
                    EV_KEY if event.value != 2 => {
                        if let Some(button) = button_for_code(event.code) {
                            if event.value == 1 && self.seen_codes.insert(event.code) {
                                eprintln!(
                                    "[input] code={} button={:?} key={}",
                                    event.code,
                                    button,
                                    button.key()
                                );
                            }
                            self.set_button(state, button, event.value != 0);
                        } else if event.value == 1 && self.seen_codes.insert(event.code) {
                            eprintln!("[input] ignored code={}", event.code);
                        }
                    }
                    EV_ABS if event.code == ABS_HAT0X => {
                        let previous = self.hat_x;
                        self.hat_x = event.value.signum();
                        self.update_hat(
                            state,
                            previous,
                            self.hat_x,
                            HandheldButton::Left,
                            HandheldButton::Right,
                        );
                    }
                    EV_ABS if event.code == ABS_HAT0Y => {
                        let previous = self.hat_y;
                        self.hat_y = event.value.signum();
                        self.update_hat(
                            state,
                            previous,
                            self.hat_y,
                            HandheldButton::Up,
                            HandheldButton::Down,
                        );
                    }
                    _ => {}
                }
            }
        }

        fn update_hat(
            &mut self,
            state: &SharedState,
            previous: i32,
            current: i32,
            negative: HandheldButton,
            positive: HandheldButton,
        ) {
            if previous < 0 {
                self.set_button(state, negative, false);
            } else if previous > 0 {
                self.set_button(state, positive, false);
            }
            if current < 0 {
                self.set_button(state, negative, true);
            } else if current > 0 {
                self.set_button(state, positive, true);
            }
        }

        fn set_button(&mut self, state: &SharedState, button: HandheldButton, down: bool) {
            let key = button.key();
            if down {
                if !self.held.insert(button) {
                    return;
                }
                state.keys_down.write().insert(key.to_owned());
                state.event_queue.lock().push_back(LoveEvent::KeyPressed {
                    key: key.to_owned(),
                    scancode: key.to_owned(),
                    is_repeat: false,
                });
            } else {
                if !self.held.remove(&button) {
                    return;
                }
                state.keys_down.write().remove(key);
                state.event_queue.lock().push_back(LoveEvent::KeyReleased {
                    key: key.to_owned(),
                    scancode: key.to_owned(),
                });
            }
        }
    }

    static INPUT: LazyLock<Mutex<Input>> = LazyLock::new(|| Mutex::new(Input::new()));

    pub fn poll(state: &SharedState) -> Result<()> {
        INPUT.lock().expect("input lock poisoned").poll(state);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub fn poll(state: &SharedState) -> Result<()> {
    linux::poll(state)
}

#[cfg(not(target_os = "linux"))]
pub fn poll(_state: &SharedState) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn uses_miyoo_evdev_record_size() {
        assert_eq!(std::mem::size_of::<InputEvent32>(), 16);
    }

    #[test]
    fn maps_miyoo_keyboard_codes() {
        let expected = [
            (103, HandheldButton::Up),
            (108, HandheldButton::Down),
            (105, HandheldButton::Left),
            (106, HandheldButton::Right),
            (57, HandheldButton::A),
            (29, HandheldButton::B),
            (42, HandheldButton::X),
            (56, HandheldButton::Y),
            (18, HandheldButton::L1),
            (20, HandheldButton::R1),
            (15, HandheldButton::L2),
            (14, HandheldButton::R2),
            (97, HandheldButton::Select),
            (28, HandheldButton::Start),
            (1, HandheldButton::Menu),
            (102, HandheldButton::Menu),
            (354, HandheldButton::Menu),
        ];
        for (code, button) in expected {
            assert_eq!(button_for_code(code), Some(button));
        }
    }

    #[test]
    fn maps_gamepad_codes() {
        let expected = [
            (0x220, HandheldButton::Up),
            (0x221, HandheldButton::Down),
            (0x222, HandheldButton::Left),
            (0x223, HandheldButton::Right),
            (0x130, HandheldButton::A),
            (0x131, HandheldButton::B),
            (0x133, HandheldButton::X),
            (0x134, HandheldButton::Y),
            (0x136, HandheldButton::L1),
            (0x137, HandheldButton::R1),
            (0x138, HandheldButton::L2),
            (0x139, HandheldButton::R2),
            (0x13a, HandheldButton::Select),
            (0x13b, HandheldButton::Start),
            (0x13c, HandheldButton::Menu),
        ];
        for (code, button) in expected {
            assert_eq!(button_for_code(code), Some(button));
        }
    }

    #[test]
    fn uses_distinct_balatro_keys_for_every_button() {
        let buttons = [
            HandheldButton::Up,
            HandheldButton::Down,
            HandheldButton::Left,
            HandheldButton::Right,
            HandheldButton::A,
            HandheldButton::B,
            HandheldButton::X,
            HandheldButton::Y,
            HandheldButton::L1,
            HandheldButton::R1,
            HandheldButton::L2,
            HandheldButton::R2,
            HandheldButton::Select,
            HandheldButton::Start,
            HandheldButton::Menu,
        ];
        let keys: HashSet<_> = buttons.into_iter().map(HandheldButton::key).collect();
        assert_eq!(keys.len(), buttons.len());
    }
}
