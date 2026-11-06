//! Temporary access to `/dev/uinput`, which only root can open by default.
//!
//! Instead of running the whole program as root or changing the system
//! permanently, we briefly grant the current user access to `/dev/uinput`
//! via an ACL, create the virtual device, and revoke the ACL immediately.
//!
//! This works because Linux checks permissions only when a file is opened:
//! the already opened `/dev/uinput` handle (and the virtual mouse behind it)
//! keeps working after the access is revoked.
//!
//! The privileged part is a tiny shell script run with `sudo` (or `pkexec`).
//! It grants the ACL, reports "ready", waits until we close its stdin, and
//! revokes the ACL on exit. If our program crashes, its end of the pipe is
//! closed by the kernel, so the ACL is revoked even then.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, IsTerminal};
use std::os::unix::fs::MetadataExt;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};

const UINPUT_PATH: &str = "/dev/uinput";

/// Runs as root: grant access, signal readiness, wait for EOF on stdin, revoke access.
/// `$1` is the numeric user id, `$2` the device path.
const GRANT_SCRIPT: &str = r#"
    set -e
    trap 'setfacl -x "u:$1" "$2"' EXIT
    trap 'exit 1' HUP INT TERM
    setfacl -m "u:$1:rw" "$2"
    echo ready
    read -r _ || true
"#;

/// Calls `create`, which is expected to open `/dev/uinput`, making sure the
/// current user has access to the device for the duration of the call.
///
/// If the user already has access, no privileges are requested.
pub fn with_temporary_access<T>(create: impl FnOnce() -> Result<T>) -> Result<T> {
    if has_uinput_access() {
        return create();
    }

    println!("Creating the virtual mouse requires temporary root access to {UINPUT_PATH}.");
    println!("The access is revoked right after the mouse is created.");

    let mut grant = spawn_grant_process()?;
    wait_until_ready(&mut grant)?;

    let result = create();

    revoke_access(grant)?;
    result
}

/// Returns `true` if `/dev/uinput` can be opened for writing by this process.
fn has_uinput_access() -> bool {
    OpenOptions::new().write(true).open(UINPUT_PATH).is_ok()
}

/// Starts the privileged script that grants access to `/dev/uinput`.
fn spawn_grant_process() -> Result<Child> {
    let uid = current_uid()?.to_string();

    elevated_command()
        .args(["sh", "-c", GRANT_SCRIPT, "super-mouse-grant", &uid, UINPUT_PATH])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to start the privileged helper")
}

/// Blocks until the script reports that access was granted (after the user
/// typed the password). Fails if authentication was cancelled or failed.
fn wait_until_ready(grant: &mut Child) -> Result<()> {
    let stdout = grant.stdout.take().context("Helper stdout is not available")?;
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line)?;

    if line.trim() != "ready" {
        let status = grant.wait()?;
        bail!("Could not obtain access to {UINPUT_PATH} (authentication cancelled or failed, {status})");
    }
    Ok(())
}

/// Closes the script's stdin, which makes it revoke the access and exit.
fn revoke_access(mut grant: Child) -> Result<()> {
    drop(grant.stdin.take());

    let status = grant.wait()?;
    if !status.success() {
        eprintln!("Warning: revoking access to {UINPUT_PATH} may have failed ({status})");
    }
    Ok(())
}

/// Picks how to ask for the root password: `sudo` in the terminal when run
/// from one, otherwise `pkexec`, which shows a graphical polkit dialog.
fn elevated_command() -> Command {
    if std::io::stdin().is_terminal() {
        Command::new("sudo")
    } else {
        Command::new("pkexec")
    }
}

/// Numeric id of the user running this program.
fn current_uid() -> Result<u32> {
    let metadata = fs::metadata("/proc/self").context("Cannot determine the current user id")?;
    Ok(metadata.uid())
}
