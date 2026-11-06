//! Virtual mouse created through the Linux `uinput` subsystem.
//!
//! The kernel exposes it like any physical mouse, so it works everywhere:
//! X11, Wayland and the text console.

use anyhow::{Context, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};

/// Number of high-resolution wheel units that make up one classic wheel notch
/// (defined by the kernel input API).
pub const WHEEL_UNITS_PER_NOTCH: i32 = 120;

/// Mouse buttons supported by the virtual mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// "Back" side button (previous page in browsers).
    Back,
    /// "Forward" side button (next page in browsers).
    Forward,
}

impl MouseButton {
    const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ];

    /// Kernel key code emitted for this button.
    fn key_code(self) -> KeyCode {
        match self {
            MouseButton::Left => KeyCode::BTN_LEFT,
            MouseButton::Right => KeyCode::BTN_RIGHT,
            MouseButton::Middle => KeyCode::BTN_MIDDLE,
            MouseButton::Back => KeyCode::BTN_SIDE,
            MouseButton::Forward => KeyCode::BTN_EXTRA,
        }
    }
}

/// A virtual mouse device that the system sees like a real one.
pub struct VirtualMouse {
    device: VirtualDevice,
    /// High-resolution scroll not yet reported as a whole classic notch
    /// (horizontal, vertical).
    pending_scroll: (i32, i32),
}

impl VirtualMouse {
    /// Creates the virtual mouse. Requires write access to `/dev/uinput`
    /// (see `uinput_access::with_temporary_access`).
    pub fn new() -> Result<Self> {
        let keys: AttributeSet<KeyCode> = MouseButton::ALL.iter().map(|b| b.key_code()).collect();
        let axes: AttributeSet<RelativeAxisCode> = [
            RelativeAxisCode::REL_X,
            RelativeAxisCode::REL_Y,
            RelativeAxisCode::REL_WHEEL,
            RelativeAxisCode::REL_HWHEEL,
            RelativeAxisCode::REL_WHEEL_HI_RES,
            RelativeAxisCode::REL_HWHEEL_HI_RES,
        ]
        .into_iter()
        .collect();

        let device = VirtualDevice::builder()
            .context("Cannot open /dev/uinput")?
            .name("Super Mouse (SNES controller)")
            .with_keys(&keys)?
            .with_relative_axes(&axes)?
            .build()
            .context("Failed to create the virtual mouse")?;

        Ok(Self { device, pending_scroll: (0, 0) })
    }

    /// Presses (`pressed = true`) or releases a mouse button.
    pub fn set_button(&mut self, button: MouseButton, pressed: bool) -> Result<()> {
        let event = InputEvent::new(EventType::KEY.0, button.key_code().0, i32::from(pressed));
        self.device.emit(&[event])?;
        Ok(())
    }

    /// Moves the cursor by the given number of pixels.
    pub fn move_by(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.device.emit(&[
            relative_event(RelativeAxisCode::REL_X, dx),
            relative_event(RelativeAxisCode::REL_Y, dy),
        ])?;
        Ok(())
    }

    /// Scrolls by the given amount in high-resolution wheel units
    /// (`WHEEL_UNITS_PER_NOTCH` = one notch). Positive `dx` scrolls right,
    /// positive `dy` scrolls up.
    ///
    /// Modern applications use the smooth high-resolution events; for older
    /// ones a classic wheel event is sent each time a whole notch accumulates.
    pub fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        let notches_x = take_whole_notches(&mut self.pending_scroll.0, dx);
        let notches_y = take_whole_notches(&mut self.pending_scroll.1, dy);

        let mut events = vec![
            relative_event(RelativeAxisCode::REL_HWHEEL_HI_RES, dx),
            relative_event(RelativeAxisCode::REL_WHEEL_HI_RES, dy),
        ];
        if notches_x != 0 {
            events.push(relative_event(RelativeAxisCode::REL_HWHEEL, notches_x));
        }
        if notches_y != 0 {
            events.push(relative_event(RelativeAxisCode::REL_WHEEL, notches_y));
        }

        self.device.emit(&events)?;
        Ok(())
    }
}

/// Builds a relative-axis input event.
fn relative_event(axis: RelativeAxisCode, value: i32) -> InputEvent {
    InputEvent::new(EventType::RELATIVE.0, axis.0, value)
}

/// Adds `delta` to the `pending` accumulator and removes (and returns)
/// the number of whole notches it now contains.
fn take_whole_notches(pending: &mut i32, delta: i32) -> i32 {
    *pending += delta;
    let notches = *pending / WHEEL_UNITS_PER_NOTCH;
    *pending -= notches * WHEEL_UNITS_PER_NOTCH;
    notches
}
