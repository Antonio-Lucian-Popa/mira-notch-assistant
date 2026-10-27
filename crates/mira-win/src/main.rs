#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod host;

#[cfg(windows)]
fn main() {
    // All Win32 objects are owned and accessed by the message-loop thread.
    unsafe { host::run() }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("mira-win requires Windows 10 or 11.");
}
