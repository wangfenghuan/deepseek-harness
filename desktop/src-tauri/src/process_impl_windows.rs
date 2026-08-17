//! Platform-specific process lifecycle for Windows.
//!
//! - Direct `CreateProcess` cannot resolve `npx.cmd` (it only searches for
//!   `npx.exe`), so the command goes through `cmd /C`, which applies PATHEXT.
//! - The console window a `cmd` child would flash is suppressed with
//!   CREATE_NO_WINDOW; a Finder-launched GUI app has no console to attach to.
//! - Shutdown recycles the whole child tree with `taskkill /T /F`.

use std::process::{Child, Command};

/// Build the base child command: `cmd /C npx <args>`, no console window.
pub fn new_command(args: &[&str]) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("cmd");
    command
        .arg("/C")
        .arg("npx")
        .args(args)
        .creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    command
}

/// Kill the child and all descendants with `taskkill /T /F`. Idempotent.
pub fn stop_child(child: &mut Option<Child>) {
    // Take ownership out of the slot so the slot is left empty.
    let Some(mut child) = child.take() else {
        return;
    };
    let pid = child.id();
    let pid_arg = pid.to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", pid_arg.as_str(), "/T", "/F"])
        .status();
    let _ = child.wait();
}
