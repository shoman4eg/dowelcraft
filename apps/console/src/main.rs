//! `dowel-craft.com` on Windows: the console twin of the window-type `dowel-craft.exe`.
//! A window-type program gets no console, so its output in cmd/PowerShell would be lost. Typing `dowel-craft`
//! picks the `.com` first (PATHEXT), which runs the `.exe` next to it with this console and waits for it.

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let exe = std::env::current_exe().map(|p| p.with_file_name("dowel-craft.exe"));
    match exe.and_then(|exe| Command::new(exe).args(std::env::args_os().skip(1)).status()) {
        // Windows exit codes are 32-bit; anything outside 0..=255 reports as a failure.
        Ok(s) => ExitCode::from(s.code().and_then(|c| u8::try_from(c).ok()).unwrap_or(1)),
        Err(e) => {
            eprintln!("dowel-craft: {e}");
            ExitCode::FAILURE
        }
    }
}
