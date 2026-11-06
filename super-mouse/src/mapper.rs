//! Translation of controller state into mouse actions.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::Result;

use crate::controller::{Button, ControllerEvent};
use crate::mouse::{MouseButton, VirtualMouse, WHEEL_UNITS_PER_NOTCH};

/// Speed range for one D-pad function: the speed starts at `min` when the
/// D-pad is pressed and grows linearly to `max` over `acceleration_time`.
struct SpeedProfile {
    min: f64,
    max: f64,
    acceleration_time: Duration,
}

impl SpeedProfile {
    /// Speed after the D-pad has been held for `hold_time`.
    fn speed_at(&self, hold_time: Duration) -> f64 {
        let progress = (hold_time.as_secs_f64() / self.acceleration_time.as_secs_f64()).min(1.0);
        self.min + (self.max - self.min) * progress
    }
}

/// Cursor speed in pixels per tick.
const CURSOR_SPEED: SpeedProfile = SpeedProfile {
    min: 2.0,
    max: 18.0,
    acceleration_time: Duration::from_millis(800),
};

/// Scroll speed in wheel notches per tick (with a 10 ms tick: 3 to 20 notches per second).
const SCROLL_SPEED: SpeedProfile = SpeedProfile {
    min: 0.03,
    max: 0.2,
    acceleration_time: Duration::from_millis(1500),
};

/// Controller button that switches the D-pad from cursor movement to scrolling while held.
const SCROLL_MODIFIER: Button = Button::Start;

/// Returns the mouse button that a controller button is mapped to, if any.
fn mouse_button_for(button: Button) -> Option<MouseButton> {
    match button {
        Button::A | Button::X => Some(MouseButton::Left),
        Button::B | Button::Y => Some(MouseButton::Right),
        Button::Select => Some(MouseButton::Middle),
        Button::L => Some(MouseButton::Back),
        Button::R => Some(MouseButton::Forward),
        Button::Start => None,
    }
}

/// Keeps track of the controller state and drives the virtual mouse.
pub struct Mapper {
    mouse: VirtualMouse,
    /// Controller buttons that are currently held down.
    held_buttons: HashSet<Button>,
    /// Current D-pad direction, each component in -1..=1 (positive = right / down).
    dpad: (i32, i32),
    /// How long the D-pad has been held in the current mode.
    hold_time: Duration,
}

impl Mapper {
    pub fn new(mouse: VirtualMouse) -> Self {
        Self {
            mouse,
            held_buttons: HashSet::new(),
            dpad: (0, 0),
            hold_time: Duration::ZERO,
        }
    }

    /// Applies a single controller event.
    pub fn handle_event(&mut self, event: ControllerEvent) -> Result<()> {
        match event {
            ControllerEvent::Button(button, pressed) => self.handle_button(button, pressed)?,
            ControllerEvent::DpadX(x) => self.dpad.0 = x,
            ControllerEvent::DpadY(y) => self.dpad.1 = y,
        }
        Ok(())
    }

    /// Moves the cursor or scrolls according to the D-pad.
    /// Should be called every `tick`.
    pub fn update(&mut self, tick: Duration) -> Result<()> {
        let (x, y) = self.dpad;
        if x == 0 && y == 0 {
            self.hold_time = Duration::ZERO;
            return Ok(());
        }

        self.hold_time += tick;

        if self.is_scroll_mode() {
            self.scroll()
        } else {
            self.move_cursor()
        }
    }

    /// Returns `true` while the scroll modifier is held.
    fn is_scroll_mode(&self) -> bool {
        self.held_buttons.contains(&SCROLL_MODIFIER)
    }

    /// Moves the cursor in the D-pad direction.
    fn move_cursor(&mut self) -> Result<()> {
        let (dx, dy) = scale_direction(self.dpad, CURSOR_SPEED.speed_at(self.hold_time));
        self.mouse.move_by(dx, dy)
    }

    /// Scrolls in the D-pad direction.
    fn scroll(&mut self) -> Result<()> {
        let units_per_tick = SCROLL_SPEED.speed_at(self.hold_time) * f64::from(WHEEL_UNITS_PER_NOTCH);
        let (dx, dy) = scale_direction(self.dpad, units_per_tick);

        // The wheel axis points up while the D-pad Y axis points down.
        self.mouse.scroll(dx, -dy)
    }

    /// Updates the held-button set and emits a mouse button change if needed.
    fn handle_button(&mut self, button: Button, pressed: bool) -> Result<()> {
        if button == SCROLL_MODIFIER {
            // Restart the acceleration so switching modes never begins at full speed.
            self.hold_time = Duration::ZERO;
        }

        match mouse_button_for(button) {
            Some(target) => self.update_mouse_button(button, target, pressed),
            None => {
                self.set_held(button, pressed);
                Ok(())
            }
        }
    }

    /// Several controller buttons can share one mouse button (e.g. A and X),
    /// so the mouse button is released only when all of them are released.
    fn update_mouse_button(&mut self, button: Button, target: MouseButton, pressed: bool) -> Result<()> {
        let was_down = self.is_mouse_button_down(target);
        self.set_held(button, pressed);
        let is_down = self.is_mouse_button_down(target);

        if was_down != is_down {
            self.mouse.set_button(target, is_down)?;
        }
        Ok(())
    }

    /// Marks a controller button as held or released.
    fn set_held(&mut self, button: Button, pressed: bool) {
        if pressed {
            self.held_buttons.insert(button);
        } else {
            self.held_buttons.remove(&button);
        }
    }

    /// Returns `true` if any held controller button maps to `target`.
    fn is_mouse_button_down(&self, target: MouseButton) -> bool {
        self.held_buttons
            .iter()
            .any(|&b| mouse_button_for(b) == Some(target))
    }
}

/// Scales a D-pad direction to the given length, normalizing diagonals so
/// they are not faster than straight moves.
fn scale_direction((x, y): (i32, i32), length: f64) -> (i32, i32) {
    let norm = f64::from(x * x + y * y).sqrt();
    let dx = (f64::from(x) / norm * length).round() as i32;
    let dy = (f64::from(y) / norm * length).round() as i32;
    (dx, dy)
}
