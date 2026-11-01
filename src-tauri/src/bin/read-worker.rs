fn main() {
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--watch") {
        if projectpane_core::platform::change_watch::run().is_err() {
            std::process::exit(1);
        }
        return;
    }
    if projectpane_core::platform::worker::run().is_err() {
        // Do not log paths, names, or raw OS errors.
        std::process::exit(1);
    }
}
