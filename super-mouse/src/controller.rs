//! Discovery of the SNES controller and translation of its raw evdev events
//! into a small, high-level event type used by the rest of the program.

use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{Context, Result, bail};
use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode};

/// Physical buttons of the SNES controller that the program cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    A,
    B,
    X,
    Y,
    L,
    R,
    Start,
    Select,
}

/// High-level events produced by the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerEvent {
    /// A button was pressed (`true`) or released (`false`).
    Button(Button, bool),
    /// Horizontal D-pad state: -1 = left, 0 = centered, 1 = right.
    DpadX(i32),
    /// Vertical D-pad state: -1 = up, 0 = centered, 1 = down.
    DpadY(i32),
}

/// Finds the first input device that looks like a gamepad with a D-pad.
///
/// If `name_hint` is given, only devices whose name contains it
/// (case-insensitive) are considered.
pub fn find_controller(name_hint: Option<&str>) -> Result<Device> {
    let hint = name_hint.map(str::to_lowercase);

    for (path, device) in evdev::enumerate() {
        let name = device.name().unwrap_or_default().to_lowercase();

        let name_matches = hint.as_deref().is_none_or(|h| name.contains(h));
        if name_matches && is_gamepad(&device) {
            println!("Using controller: {} ({})", device.name().unwrap_or("?"), path.display());
            return Ok(device);
        }
    }

    bail!("No matching controller found. Is it connected and do you have read access to /dev/input/event*?")
}

/// Returns `true` if the device exposes gamepad face buttons and a hat-style D-pad.
fn is_gamepad(device: &Device) -> bool {
    let has_face_buttons = device
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::BTN_SOUTH) && keys.contains(KeyCode::BTN_EAST));

    let has_dpad = device
        .supported_absolute_axes()
        .is_some_and(|axes| axes.contains(AbsoluteAxisCode::ABS_HAT0X));

    has_face_buttons && has_dpad
}

/// Spawns a background thread that reads events from the controller and
/// forwards them to `sender`. The thread ends when the device disconnects
/// or the receiving side is dropped.
pub fn spawn_reader(mut device: Device, sender: Sender<ControllerEvent>) -> Result<()> {
    // Grab the device so other applications (e.g. games, browsers)
    // do not react to the controller while it acts as a mouse.
    device.grab().context("Failed to grab the controller")?;

    thread::spawn(move || {
        loop {
            let events = match device.fetch_events() {
                Ok(events) => events,
                Err(err) => {
                    eprintln!("Controller read error: {err}");
                    return;
                }
            };

            for event in events.filter_map(|e| translate_event(e.destructure())) {
                if sender.send(event).is_err() {
                    return;
                }
            }
        }
    });

    Ok(())
}

/// Converts a raw evdev event into a `ControllerEvent`, ignoring anything unmapped.
fn translate_event(event: EventSummary) -> Option<ControllerEvent> {
    match event {
        // Value 2 means auto-repeat, which is irrelevant for us.
        EventSummary::Key(_, code, value @ (0 | 1)) => {
            map_button(code).map(|button| ControllerEvent::Button(button, value == 1))
        }
        EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0X, value) => {
            Some(ControllerEvent::DpadX(value.signum()))
        }
        EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_HAT0Y, value) => {
            Some(ControllerEvent::DpadY(value.signum()))
        }
        _ => None,
    }
}

/// Maps kernel key codes to SNES buttons.
///
/// The `hid-nintendo` driver uses positional names: the face buttons are
/// named after their compass position (A is on the right side, B at the
/// bottom, X at the top, Y on the left).
fn map_button(code: KeyCode) -> Option<Button> {
    match code {
        KeyCode::BTN_EAST => Some(Button::A),
        KeyCode::BTN_SOUTH => Some(Button::B),
        KeyCode::BTN_NORTH => Some(Button::X),
        KeyCode::BTN_WEST => Some(Button::Y),
        KeyCode::BTN_TL => Some(Button::L),
        KeyCode::BTN_TR => Some(Button::R),
        KeyCode::BTN_START => Some(Button::Start),
        KeyCode::BTN_SELECT => Some(Button::Select),
        _ => None,
    }
}
