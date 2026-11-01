//! Small, deterministic updater admission rules. No network or personal data.
use semver::Version;

pub const RELEASES: &str = "https://github.com/swallow-kim/projectpane-public/releases";
pub const DAY: u64 = 24 * 60 * 60;
pub const MAX_DOWNLOAD: u64 = 100 * 1024 * 1024;

pub fn automatic_due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| now < last || now.saturating_sub(last) >= DAY)
}

pub fn accepted_release(current: &str, next: &str, url: &str) -> bool {
    let (Ok(current), Ok(next_version)) = (Version::parse(current), Version::parse(next)) else {
        return false;
    };
    next_version > current
        && next_version.pre.is_empty()
        && next_version.build.is_empty()
        && next_version.to_string() == next
        && url == format!("{RELEASES}/download/v{next}/ProjectPane_{next}_x64-setup.exe")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn daily_cache_and_clock_rollback() {
        assert!(automatic_due(None, 1));
        assert!(!automatic_due(Some(100), 100 + DAY - 1));
        assert!(automatic_due(Some(100), 100 + DAY));
        assert!(automatic_due(Some(100), 99));
    }
    #[test]
    fn only_new_stable_version_from_exact_public_release_is_admitted() {
        let url = format!("{RELEASES}/download/v1.3.1/ProjectPane_1.3.1_x64-setup.exe");
        assert!(accepted_release("1.3.0", "1.3.1", &url));
        for version in ["1.3.1", "1.4.0", "2.0.0"] {
            assert!(!accepted_release(version, "1.3.1", &url));
        }
        for bad in [
            format!("{url}?x=1"),
            url.replace("https:", "http:"),
            url.replace("projectpane-public", "other"),
            url.replace("x64-setup.exe", "x64.zip"),
        ] {
            assert!(!accepted_release("1.3.0", "1.3.1", &bad));
        }
        assert!(!accepted_release("1.3.0", "1.4.0-rc.1", &url));
        assert!(!accepted_release("1.3.0", "invalid", &url));
    }
}
