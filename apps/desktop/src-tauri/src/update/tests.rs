//! Unit tests for the update cache lifecycle, the proxy URL seam, and the webview status contract.

use crate::update::cache::{UpdateCache, UpdateMetadata, digest, hex_digest};
use crate::update::service::proxy_url;
use crate::update::{DesktopUpdateStatus, ManualUpdateReason};
use ora_application::NetworkProxySettings;
use pretty_assertions::assert_eq;
use semver::Version;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Returns the cache directory the service derives from an Ora home directory.
fn cache_directory(home: &Path) -> PathBuf {
    home.join(".ora").join("cache")
}

/// Stores a package so the cache holds both the artifact and its identity record.
async fn store_release(cache: &UpdateCache, release_version: &str, bytes: &[u8]) {
    let metadata = UpdateMetadata {
        schema_version: 1,
        release_version: release_version.to_owned(),
        sha256: hex_digest(&digest(bytes)),
        file_name: cache.package_file_name().to_owned(),
    };
    cache.store(bytes, &metadata).await.expect("store succeeds");
}

#[tokio::test]
async fn store_writes_the_package_and_a_matching_metadata_record() {
    let home = TempDir::new().expect("temp home");
    let cache = UpdateCache::open(home.path()).expect("cache opens");
    let bytes = b"signed-package".as_slice();

    store_release(&cache, "0.2.0", bytes).await;

    let metadata_path = cache_directory(home.path()).join("ora-update.json");
    let stored: UpdateMetadata =
        serde_json::from_slice(&std::fs::read(&metadata_path).expect("metadata is readable"))
            .expect("metadata parses");
    assert_eq!(
        (
            std::fs::read(cache.package_path()).expect("package readable"),
            stored
        ),
        (
            bytes.to_vec(),
            UpdateMetadata {
                schema_version: 1,
                release_version: "0.2.0".to_owned(),
                sha256: hex_digest(&digest(bytes)),
                file_name: cache.package_file_name().to_owned(),
            }
        )
    );
}

#[tokio::test]
async fn store_leaves_no_temporary_files_behind() {
    let home = TempDir::new().expect("temp home");
    let cache = UpdateCache::open(home.path()).expect("cache opens");

    store_release(&cache, "0.2.0", b"signed-package").await;

    let mut names = std::fs::read_dir(cache_directory(home.path()))
        .expect("cache directory is readable")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            cache.package_file_name().to_owned(),
            "ora-update.json".to_owned()
        ]
    );
}

#[tokio::test]
async fn discard_superseded_removes_a_release_the_running_build_already_includes() {
    let home = TempDir::new().expect("temp home");
    let cache = UpdateCache::open(home.path()).expect("cache opens");
    store_release(&cache, "0.2.0", b"signed-package").await;

    cache
        .discard_superseded(&Version::parse("0.2.0").expect("version"))
        .expect("discard succeeds");

    assert_eq!(
        std::fs::read_dir(cache_directory(home.path()))
            .expect("cache directory is readable")
            .count(),
        0
    );
}

#[tokio::test]
async fn discard_superseded_keeps_a_release_newer_than_the_running_build() {
    let home = TempDir::new().expect("temp home");
    let cache = UpdateCache::open(home.path()).expect("cache opens");
    store_release(&cache, "0.3.0", b"signed-package").await;

    cache
        .discard_superseded(&Version::parse("0.2.0").expect("version"))
        .expect("discard succeeds");

    assert_eq!(
        std::fs::read(cache.package_path()).expect("package survives"),
        b"signed-package".to_vec()
    );
}

#[tokio::test]
async fn discard_superseded_drops_a_package_whose_identity_record_is_unreadable() {
    let home = TempDir::new().expect("temp home");
    let cache = UpdateCache::open(home.path()).expect("cache opens");
    store_release(&cache, "0.3.0", b"signed-package").await;
    std::fs::write(
        cache_directory(home.path()).join("ora-update.json"),
        b"{ truncated",
    )
    .expect("metadata is writable");

    cache
        .discard_superseded(&Version::parse("0.2.0").expect("version"))
        .expect("discard succeeds");

    assert_eq!(
        std::fs::read_dir(cache_directory(home.path()))
            .expect("cache directory is readable")
            .count(),
        0
    );
}

#[test]
fn discard_superseded_is_a_no_op_on_an_empty_cache() {
    let home = TempDir::new().expect("temp home");
    let cache = UpdateCache::open(home.path()).expect("cache opens");

    cache
        .discard_superseded(&Version::parse("0.2.0").expect("version"))
        .expect("a missing identity record is not an error");

    assert_eq!(
        std::fs::read_dir(cache_directory(home.path()))
            .expect("cache directory is readable")
            .count(),
        0
    );
}

#[test]
fn proxy_url_carries_the_host_port_and_credentials() {
    let url = proxy_url(&NetworkProxySettings {
        host: "proxy.internal".to_owned(),
        port: 8080,
        username: Some("agent".to_owned()),
        password: Some("s3cr3t".to_owned()),
    })
    .expect("proxy URL builds");

    assert_eq!(url.as_str(), "http://agent:s3cr3t@proxy.internal:8080/");
}

#[test]
fn proxy_url_omits_credentials_that_are_not_configured() {
    let url = proxy_url(&NetworkProxySettings {
        host: "proxy.internal".to_owned(),
        port: 3128,
        username: None,
        password: None,
    })
    .expect("proxy URL builds");

    assert_eq!(url.as_str(), "http://proxy.internal:3128/");
}

/// The webview switches on `kind` and reads the payload fields directly, so the serialized shape
/// is part of the platform contract in `packages/app-shell/src/platform/types.ts`.
#[test]
fn status_serializes_to_the_shape_the_webview_consumes() {
    let statuses = vec![
        DesktopUpdateStatus::Current,
        DesktopUpdateStatus::Checking,
        DesktopUpdateStatus::Downloading {
            version: "0.3.0".to_owned(),
            downloaded: 1024,
            total: Some(4096),
        },
        DesktopUpdateStatus::Ready {
            version: "0.3.0".to_owned(),
        },
        DesktopUpdateStatus::ManualUpdate {
            version: "0.3.0".to_owned(),
            reason: ManualUpdateReason::SystemPackage,
        },
        DesktopUpdateStatus::Installing {
            version: "0.3.0".to_owned(),
        },
        DesktopUpdateStatus::Failed {
            message: "endpoint unreachable".to_owned(),
        },
    ];

    assert_eq!(
        serde_json::to_value(&statuses).expect("statuses serialize"),
        json!([
            { "kind": "current" },
            { "kind": "checking" },
            { "kind": "downloading", "version": "0.3.0", "downloaded": 1024, "total": 4096 },
            { "kind": "ready", "version": "0.3.0" },
            { "kind": "manual_update", "version": "0.3.0", "reason": "system_package" },
            { "kind": "installing", "version": "0.3.0" },
            { "kind": "failed", "message": "endpoint unreachable" },
        ]) as Value
    );
}

#[test]
fn manual_update_reasons_serialize_as_the_webview_discriminants() {
    assert_eq!(
        serde_json::to_value([
            ManualUpdateReason::SystemPackage,
            ManualUpdateReason::UnpackagedBinary,
        ])
        .expect("reasons serialize"),
        json!(["system_package", "unpackaged_binary"]) as Value
    );
}
