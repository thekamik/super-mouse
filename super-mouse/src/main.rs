//! Super Mouse: use an SNES controller as a mouse.
//!
//! Mapping:
//! - A, X  -> left mouse button
//! - B, Y  -> right mouse button
//! - D-pad -> cursor movement (scrolling while Start is held)
//! - L     -> back (previous page)
//! - R     -> forward (next page)
//! - Select -> middle mouse button
//!
//! Usage: `super-mouse [DEVICE_NAME_FILTER]`

mod controller;
mod mapper;
mod mouse;
mod uinput_access;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::Result;

use controller::ControllerEvent;
use mapper::Mapper;
use mouse::VirtualMouse;

/// How often the cursor or scroll position is updated while the D-pad is held.
const TICK: Duration = Duration::from_millis(10);

fn main() -> Result<()> {
    let name_filter = std::env::args().nth(1);

    let device = controller::find_controller(name_filter.as_deref())?;
    let mouse = uinput_access::with_temporary_access(VirtualMouse::new)?;

    let (sender, receiver) = mpsc::channel();
    controller::spawn_reader(device, sender)?;

    println!("Super Mouse is running. Press Ctrl+C to quit.");
    run_event_loop(Mapper::new(mouse), receiver)
}

/// Main loop: processes controller events as they arrive and moves the
/// cursor at a fixed rate. Returns when the controller disconnects.
fn run_event_loop(mut mapper: Mapper, receiver: Receiver<ControllerEvent>) -> Result<()> {
    let mut next_tick = Instant::now() + TICK;

    loop {
        let timeout = next_tick.saturating_duration_since(Instant::now());

        match receiver.recv_timeout(timeout) {
            Ok(event) => mapper.handle_event(event)?,
            Err(RecvTimeoutError::Timeout) => {
                mapper.update(TICK)?;
                next_tick += TICK;
            }
            Err(RecvTimeoutError::Disconnected) => {
                println!("Controller disconnected.");
                return Ok(());
            }
        }
    }
}
