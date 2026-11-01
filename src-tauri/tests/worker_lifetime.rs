#[cfg(windows)]
#[path = "worker_lifetime/windows.rs"]
mod windows;

fn main() {
    #[cfg(windows)]
    windows::run();
}
