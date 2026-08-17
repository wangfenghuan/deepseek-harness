//! Platform-specific process lifecycle for Unix-like systems (macOS, Linux).
//!
//! - The child is spawned in its own process group so one signal recycles
//!   npx, dsh, and any agents dsh itself spawns.
//! - Shutdown sends SIGTERM first, then SIGKILL after the grace period.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

const STOP_GRACE: Duration = Duration::from_secs(5);

/// Build the base child command: `npx <args>` in its own process group.
pub fn new_command(args: &[&str]) -> Command {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("npx");
    command.args(args).process_group(0);
    command
}

/// Stop the child and its whole process group. Idempotent.
pub fn stop_child(child: &mut Option<Child>) {
    // Take ownership out of the slot so the slot is left empty and no borrow
    // of it outlives the shutdown loop below.
    let Some(mut child) = child.take() else {
        return;
    };
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    let pid = child.id() as i32;
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + STOP_GRACE;
    loop {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
            let _ = child.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
