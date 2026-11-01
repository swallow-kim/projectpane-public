//! Explicit local release checks. No private key access or user installation mutation.
use serde_json::json;
use tauri_plugin_updater::UpdaterExt;

fn fixture() -> (
    tauri::App<tauri::test::MockRuntime>,
    serde_json::Value,
    Vec<u8>,
) {
    let package =
        std::env::var_os("PROJECTPANE_TEST_UPDATE_PACKAGE").expect("explicit fixture package");
    let signature = std::fs::read_to_string(format!("{}.sig", package.to_string_lossy())).unwrap();
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.package_info_mut().version = "1.2.1".parse().unwrap();
    context.config_mut().identifier = "com.swallowkim.projectpane.signature-fixture".into();
    context
        .config_mut()
        .plugins
        .0
        .insert("updater".into(), config["plugins"]["updater"].clone());
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    let version = config["version"].as_str().unwrap();
    let metadata = json!({"version":version,"platforms":{"windows-x86_64":{
        "url":format!("https://github.com/swallow-kim/projectpane-public/releases/download/v{version}/ProjectPane_{version}_x64-setup.exe"),
        "signature":signature.trim()
    }}});
    (app, metadata, std::fs::read(package).unwrap())
}

#[test]
#[ignore = "requires an explicit locally signed release package"]
fn actual_plugin_verifies_release_bytes_and_rejects_tampering_and_wrong_version() {
    let (app, metadata, mut bytes) = fixture();
    let updater = app
        .updater_builder()
        .target("windows-x86_64")
        .build()
        .unwrap();
    let update = updater.restore_update(metadata.clone()).unwrap().unwrap();
    update
        .verify(&bytes)
        .expect("actual updater signature validation");
    let mut wrong_version = metadata.clone();
    wrong_version["version"] = json!("9.0.0");
    assert!(
        updater
            .restore_update(wrong_version)
            .unwrap()
            .unwrap()
            .verify(&bytes)
            .is_err()
    );
    let mut downgrade = metadata.clone();
    downgrade["version"] = json!("1.0.0");
    assert!(updater.restore_update(downgrade).unwrap().is_none());
    bytes[0] ^= 1;
    assert!(update.verify(&bytes).is_err());
    let mut missing_signature = metadata;
    missing_signature["platforms"]["windows-x86_64"]["signature"] = json!("");
    assert!(
        updater
            .restore_update(missing_signature)
            .unwrap()
            .unwrap()
            .verify(&bytes)
            .is_err()
    );
}

#[test]
#[ignore = "isolated child process only: real plugin install exits this process"]
fn actual_plugin_hands_off_only_to_explicit_synthetic_installer() {
    let marker = std::env::var_os("PROJECTPANE_SYNTHETIC_INSTALL_MARKER")
        .expect("synthetic marker required");
    let root = std::path::PathBuf::from(marker)
        .parent()
        .unwrap()
        .to_owned();
    assert!(
        root.join("synthetic-installer-only.txt").is_file(),
        "explicit fixture boundary required"
    );
    if root.join("installed.marker").is_file() {
        assert_eq!(
            std::fs::read(root.join("synthetic-project.json")).unwrap(),
            b"{\"fixture\":true,\"saved\":true}"
        );
        std::fs::write(
            root.join("restarted.marker"),
            b"saved synthetic project restored",
        )
        .unwrap();
        return;
    }
    let (app, metadata, bytes) = fixture();
    let updater = app
        .updater_builder()
        .target("windows-x86_64")
        .executable_path(root.join("synthetic-host.exe"))
        .build()
        .unwrap();
    updater
        .restore_update(metadata)
        .unwrap()
        .unwrap()
        .install(bytes)
        .unwrap();
    panic!("the Windows plugin must hand off and exit; returning is not a successful install");
}

#[tokio::test]
#[ignore = "after publication only: anonymous public feed and package download, never installation"]
async fn published_feed_download_matches_the_verified_local_package() {
    let (app, _, expected) = fixture();
    let updater = app
        .updater_builder()
        .target("windows-x86_64")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let update = updater
        .check()
        .await
        .unwrap()
        .expect("older fixture must see the public release");
    assert_eq!(update.version, env!("CARGO_PKG_VERSION"));
    assert!(super::update_policy::accepted_release(
        "1.2.1",
        &update.version,
        update.download_url.as_str()
    ));
    let downloaded = update.download(|_, _| {}, || {}).await.unwrap();
    assert_eq!(downloaded, expected);
}
