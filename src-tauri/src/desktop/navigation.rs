pub fn allowed(url: &tauri::Url, development: bool) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match (url.scheme(), url.host_str(), url.port()) {
        ("tauri", Some("localhost"), None) | ("http", Some("tauri.localhost"), None) => true,
        ("http", Some("localhost" | "127.0.0.1"), Some(1420)) => development,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packaged_origins_allow_paths_and_fragments() {
        for value in [
            "tauri://localhost/",
            "http://tauri.localhost/index.html?q=1#x",
        ] {
            assert!(allowed(&tauri::Url::parse(value).unwrap(), false));
        }
    }
    #[test]
    fn development_origins_require_exact_host_port_and_debug_mode() {
        for value in ["http://localhost:1420/", "http://127.0.0.1:1420/path"] {
            let url = tauri::Url::parse(value).unwrap();
            assert!(allowed(&url, true));
            assert!(!allowed(&url, false));
        }
    }
    #[test]
    fn remote_credentials_and_nonlocal_schemes_are_denied() {
        for value in [
            "https://tauri.localhost/",
            "tauri://localhost:1420/",
            "http://tauri.localhost:1420/",
            "http://localhost:1421/",
            "http://localhost/",
            "http://localhost.evil:1420/",
            "http://[::1]:1420/",
            "https://example.com/",
            "http://user@tauri.localhost/",
            "tauri://user:pass@localhost/",
            "http://user@localhost:1420/",
            "file:///C:/index.html",
            "data:text/html,hi",
            "javascript:alert(1)",
            "about:blank",
        ] {
            assert!(
                !allowed(&tauri::Url::parse(value).unwrap(), true),
                "{value}"
            );
        }
    }
}
