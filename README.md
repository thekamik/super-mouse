# Super Mouse

Use a Super Nintendo (SNES) controller as a mouse on Linux.

Super Mouse reads the controller through the kernel's evdev interface and
creates a virtual mouse with `uinput`. The system sees it as an ordinary mouse,
so it works everywhere: on X11, on Wayland and in the text console.

The program runs as your normal user. Root access is requested only for a
fraction of a second, to create the virtual mouse, and is dropped right after
(see [How permissions work](#how-permissions-work)).

## Button mapping

| Controller          | Mouse action                       |
|---------------------|------------------------------------|
| **A**, **X**        | Left button                        |
| **B**, **Y**        | Right button                       |
| **Select**          | Middle button                      |
| **L**               | Back (previous page in a browser)  |
| **R**               | Forward (next page in a browser)   |
| **D-pad**           | Move the cursor                    |
| **Start + D-pad**   | Scroll (vertical and horizontal)   |

How the buttons behave:

- **Acceleration:** the cursor starts slowly for precise positioning and
  speeds up the longer a D-pad direction is held. Scrolling accelerates the
  same way.
- **Diagonals:** diagonal movement is normalized, so it is not faster than
  straight movement.
- **Shared buttons:** A and X both press the left button, and B and Y both
  press the right one. Holding A and X together and releasing one of them does
  not release the click. The click ends when both are released.
- **Scroll mode:** scrolling is active only while Start is held. Releasing
  Start switches the D-pad back to cursor movement immediately.
- **Smooth scrolling:** high-resolution wheel events give smooth scrolling in
  modern applications. Classic wheel events are sent as well for older
  applications.
- **Grabbed controller:** while Super Mouse is running it has exclusive
  access to the controller, so games and browsers do not also react to its
  buttons. Everything goes back to normal when the program exits.

## Requirements

- **Linux** with the `uinput` kernel module. It is built in or loaded by
  default on all common distributions, and `/dev/uinput` must exist.
- **An SNES-style controller** that the kernel exposes as a gamepad with a
  hat-style D-pad. Tested with the Nintendo Switch Online SNES controller
  connected over Bluetooth (handled by the `hid-nintendo` driver). Other
  gamepads with the same layout should work too.
- **Rust**, with Cargo supporting edition 2024 (Rust 1.85 or newer).
- **`setfacl`**, from the `acl` package, which most distributions install by
  default.
- **`sudo`**, or `pkexec` when the program is not started from a terminal.
- **Read access to the controller's `/dev/input/event*` node.** On desktop
  systems with systemd-logind the locally logged-in user already has it.

## Installation

```bash
git clone git@github.com:thekamik/super-mouse.git
cd super-mouse/super-mouse
cargo build --release
```

The binary is placed in `target/release/super-mouse`. To install it into
`~/.cargo/bin`:

```bash
cargo install --path .
```

## Usage

1. Connect the controller (for Bluetooth: pair it in your system's Bluetooth
   settings first).
2. Run the program:

   ```bash
   super-mouse
   ```

   or, from the source directory:

   ```bash
   cargo run --release
   ```

3. Enter your password when `sudo` asks for it. The output looks like this:

   ```text
   Using controller: SNES Controller (/dev/input/event18)
   Creating the virtual mouse requires temporary root access to /dev/uinput.
   The access is revoked right after the mouse is created.
   [sudo] password for kamil:
   Super Mouse is running. Press Ctrl+C to quit.
   ```

4. Press `Ctrl+C` to quit. The program also exits on its own when the
   controller disconnects.

By default the first connected gamepad is used. If several are connected, pass
part of the device name (case-insensitive) to pick one:

```bash
super-mouse snes
```

The names of connected input devices are listed in `/proc/bus/input/devices`.

## How permissions work

Creating an input device means opening `/dev/uinput`, which by default only
root can do. This is deliberate. A program that can create input devices could
pretend to be a keyboard and type commands into any session, including the
lock screen. Even Bluetooth mice are created by a root process (`bluetoothd`,
through `/dev/uhid`).

Super Mouse does **not** need to run as root and does **not** change the
system permanently. Instead it does the following:

1. **Check access.** If you can already open `/dev/uinput`, no password is
   requested at all.
2. **Grant access.** Otherwise a tiny shell script is started with `sudo`. It
   adds an ACL entry that gives your user read/write access to `/dev/uinput`:
   `setfacl -m u:<uid>:rw /dev/uinput`.
3. **Create the mouse.** Super Mouse, still running as your user, opens
   `/dev/uinput` and creates the virtual mouse.
4. **Revoke access.** The script removes the ACL entry
   (`setfacl -x u:<uid> /dev/uinput`) and exits.

The mouse keeps working after step 4 because Linux checks file permissions
only when a file is opened. The already open handle stays valid.

The script waits for Super Mouse to close a pipe before revoking access. If
Super Mouse crashes or is killed, the kernel closes that pipe, so the ACL is
removed in that case too. You can check that no extra permissions are left
behind:

```bash
getfacl /dev/uinput
```

When started without a terminal (for example from an application launcher),
`pkexec` is used instead of `sudo`. It shows a graphical polkit password
dialog.

### Lifetime of the virtual mouse

The virtual mouse exists only while Super Mouse is running. The device is tied
to the open `/dev/uinput` handle, and the kernel removes it as soon as that
handle is closed. This happens on a normal exit, on `Ctrl+C`, on a crash and
even on `kill -9`. Nothing is left in the kernel, and repeated runs do not
create piles of unused devices. To check:

```bash
grep -c "Super Mouse" /proc/bus/input/devices   # 1 while running, 0 after exit
```

## Configuration

Settings are compile-time constants in
[`super-mouse/src/mapper.rs`](super-mouse/src/mapper.rs). Rebuild after
changing them.

| Constant          | Meaning                                                       | Default                          |
|-------------------|---------------------------------------------------------------|----------------------------------|
| `CURSOR_SPEED`    | Cursor speed in pixels per tick (`min` → `max`)               | 2 → 18 px over 800 ms            |
| `SCROLL_SPEED`    | Scroll speed in wheel notches per tick (`min` → `max`)        | 0.03 → 0.2 notches over 1500 ms  |
| `SCROLL_MODIFIER` | Button that switches the D-pad to scrolling while held        | `Button::Start`                  |

A tick is 10 ms (`TICK` in [`super-mouse/src/main.rs`](super-mouse/src/main.rs)).
At the default settings the cursor tops out at 1800 px/s and scrolling ranges
from 3 to 20 notches per second.

The button mapping itself is defined in `mouse_button_for` in `mapper.rs`.

## Project structure

```text
super-mouse/
├── Cargo.toml
└── src/
    ├── main.rs           # Startup and the main event loop (fixed 10 ms tick)
    ├── controller.rs     # Finds the controller, reads and translates its events
    ├── mapper.rs         # Controller state -> mouse actions, speeds, scroll mode
    ├── mouse.rs          # Virtual mouse (uinput): buttons, movement, scrolling
    └── uinput_access.rs  # Temporary root access to /dev/uinput
```

How the pieces fit together:

1. **Reading.** A background thread reads raw events from the controller and
   translates them into high-level `ControllerEvent`s (button pressed or
   released, D-pad direction). It sends them to the main thread through a
   channel.
2. **Button events.** The main loop applies each event as soon as it arrives,
   so clicks have no added latency.
3. **D-pad.** While a direction is held, the main loop moves the cursor or
   scrolls every 10 ms, applying acceleration.

## Troubleshooting

**`No matching controller found`**

- Make sure the controller is connected and shows up in
  `/proc/bus/input/devices`.
- Check that you can read its event node with `getfacl /dev/input/eventN`.
  Your user should be listed.
- If you passed a name filter, check that it matches part of the device name.

**`Could not obtain access to /dev/uinput`**

The password prompt was cancelled or authentication failed. Also check that
`setfacl` is installed (package `acl`).

**`Failed to grab the controller`**

Another program has exclusive access to the controller. The most common cause
is a second running instance of Super Mouse. Other remapping tools, such as
Steam Input or input-remapper, can also hold it.

**A and B are swapped (A gives a right click)**

Drivers differ in how they name the face buttons. Swap `BTN_EAST` and
`BTN_SOUTH` in `map_button` in
[`super-mouse/src/controller.rs`](super-mouse/src/controller.rs).

**The cursor or scrolling is too slow or too fast**

Adjust `CURSOR_SPEED` or `SCROLL_SPEED`; see [Configuration](#configuration).

## License

Licensed under the GNU General Public License v2.0. See [LICENSE](LICENSE).
