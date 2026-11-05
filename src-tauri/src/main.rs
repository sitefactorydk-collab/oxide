// Hide the console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = oxide_lib::run() {
        eprintln!("Oxide failed to start: {err:#}");
        std::process::exit(1);
    }
}
