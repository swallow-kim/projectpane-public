use super::*;
use std::{ffi::OsString, hint::black_box, time::Instant};
fn legacy(a: &OsStr, b: &OsStr) -> Ordering {
    natural(
        &a.to_string_lossy().to_lowercase(),
        &b.to_string_lossy().to_lowercase(),
    )
}
#[test]
fn fast_paths_match_original_unicode_and_numeric_semantics() {
    let mut names: Vec<String> = [
        "",
        "0",
        "00",
        "A0",
        "a00",
        "a1",
        "A01",
        "a10",
        "a2",
        ".TXT",
        "999999999999999999999999999999999",
        "\u{130}\u{3a3}",
        "\u{39f}\u{3a3}",
        "\u{3bf}\u{3c3}",
        "\u{3bf}\u{3c2}",
        "한글2",
        "한글10",
        "ÄBC",
        "äbc",
        "ß",
        "ẞ",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut seed = 41_u64;
    for _ in 0..400 {
        let mut name = String::new();
        for _ in 0..24 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            name.push(b"aAbB09._-XYZxyz"[(seed >> 32) as usize % 14] as char);
        }
        names.push(name);
    }
    for a in &names {
        for b in &names {
            assert_eq!(
                natural_ignore_case(OsStr::new(a), OsStr::new(b)),
                legacy(OsStr::new(a), OsStr::new(b)),
                "{a:?} {b:?}"
            );
        }
        for filter in [
            "",
            "A",
            "00",
            "Xy",
            ".",
            "\u{39f}\u{3a3}",
            "\u{130}",
            "한글",
        ] {
            assert_eq!(
                matches_normalized_filter(OsStr::new(a), &filter.to_lowercase()),
                a.to_lowercase().contains(&filter.to_lowercase())
            );
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let invalid = OsString::from_wide(&[0x41, 0xd800, 0x32]);
        for other in [
            OsStr::new("a\u{fffd}2"),
            OsStr::new("a\u{fffd}10"),
            &invalid,
        ] {
            assert_eq!(
                natural_ignore_case(&invalid, other),
                legacy(&invalid, other)
            );
        }
    }
}
// In-memory comparator benchmark, not an end-to-end disk/IPC measurement.
// cargo test --release --lib benchmark_ascii_listing -- --ignored --nocapture
#[test]
#[ignore]
fn benchmark_ascii_listing() {
    for prefix in ["File_".to_owned(), "Long_Mixed_CASE_Prefix_".repeat(8)] {
        let mut seed = 41_u64;
        let names: Vec<OsString> = (0..100_000)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                OsString::from(format!("{prefix}{:010}.TXT", seed >> 32))
            })
            .collect();
        for _ in 0..3 {
            let mut before: Vec<usize> = (0..names.len()).collect();
            let mut after = before.clone();
            let start = Instant::now();
            before.sort_unstable_by(|a, b| {
                legacy(&names[*a], &names[*b])
                    .then_with(|| super::super::native_id::compare(&names[*a], &names[*b]))
            });
            let baseline = start.elapsed();
            let start = Instant::now();
            after.sort_unstable_by(|a, b| {
                natural_ignore_case(&names[*a], &names[*b])
                    .then_with(|| super::super::native_id::compare(&names[*a], &names[*b]))
            });
            let optimized = start.elapsed();
            assert_eq!(before, after);
            let filter = black_box("123");
            let start = Instant::now();
            let before_count = names
                .iter()
                .filter(|name| name.to_string_lossy().to_lowercase().contains(filter))
                .count();
            let baseline_filter = start.elapsed();
            let start = Instant::now();
            let after_count = names
                .iter()
                .filter(|name| matches_normalized_filter(name, filter))
                .count();
            let optimized_filter = start.elapsed();
            assert_eq!(before_count, after_count);
            eprintln!(
                "100k prefix_bytes={} sort baseline={baseline:?} optimized={optimized:?}; filter baseline={baseline_filter:?} optimized={optimized_filter:?}",
                prefix.len()
            );
        }
    }
}
