#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;

fn main() {
    if desktop::run().is_err() {
        eprintln!("desktop_runtime_failed");
        std::process::exit(1);
    }
}
