//! Fixed-name cache for the one downloaded update package.
//!
//! The cache is deliberately single-slot: parallel versions on disk would make it ambiguous which
//! artifact a later process is allowed to install, and the updater's signature verification only
//! happens inside the download that produced the bytes.

use super::UpdateError;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const METADATA_FILE: &str = "ora-update.json";

/// Describes the persisted cache identity without treating it as a replacement for signature
/// verification. The digest lets the active process detect a cache file changed after download.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateMetadata {
    pub(super) schema_version: u32,
    pub(super) release_version: String,
    pub(super) sha256: String,
    pub(super) file_name: String,
}

/// Owns the cache paths and the atomic write and cleanup rules that apply to them.
pub(super) struct UpdateCache {
    package_path: PathBuf,
    metadata_path: PathBuf,
}

impl UpdateCache {
    /// Creates the cache directory below the Ora home and binds the platform's package file name.
    pub(super) fn open(home_directory: &Path) -> Result<Self, UpdateError> {
        let directory = home_directory.join(".ora").join("cache");
        std::fs::create_dir_all(&directory).map_err(UpdateError::CacheDirectory)?;
        Ok(Self {
            package_path: directory.join(package_file_name()),
            metadata_path: directory.join(METADATA_FILE),
        })
    }

    /// Returns the fixed path the installer reads the package bytes back from.
    pub(super) fn package_path(&self) -> &Path {
        &self.package_path
    }

    /// Returns the platform-specific package file name recorded in the metadata sidecar.
    pub(super) fn package_file_name(&self) -> &str {
        package_file_name()
    }

    /// Drops a cached package that the running build already supersedes.
    ///
    /// An unreadable sidecar is treated the same way: without it the package identity cannot be
    /// established, and the next scheduled check downloads a freshly verified artifact anyway.
    pub(super) fn discard_superseded(&self, current: &Version) -> Result<(), UpdateError> {
        let bytes = match std::fs::read(&self.metadata_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(UpdateError::ReadMetadata(error)),
        };
        let superseded = serde_json::from_slice::<UpdateMetadata>(&bytes)
            .ok()
            .and_then(|metadata| Version::parse(&metadata.release_version).ok())
            .is_none_or(|release| release <= *current);
        if superseded {
            self.clear();
        }
        Ok(())
    }

    /// Writes the package and its sidecar through siblings so a reader never observes a partial
    /// package or a metadata record that points at a file that is not there yet.
    pub(super) async fn store(
        &self,
        bytes: &[u8],
        metadata: &UpdateMetadata,
    ) -> Result<(), UpdateError> {
        let metadata_bytes =
            serde_json::to_vec_pretty(metadata).map_err(UpdateError::EncodeMetadata)?;
        write_atomically(&self.package_path, bytes).await?;
        write_atomically(&self.metadata_path, &metadata_bytes).await
    }

    /// Removes the cached package, its identity record, and any interrupted temporary write.
    pub(super) fn clear(&self) {
        for path in [&self.package_path, &self.metadata_path] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(temp_path(path));
        }
    }
}

/// Returns the one platform-specific cache file name used to prevent parallel versions.
fn package_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ora-update.exe"
    } else if cfg!(target_os = "macos") {
        "ora-update.app.tar.gz"
    } else {
        "ora-update.AppImage"
    }
}

/// Appends a suffix rather than replacing the extension, so the package and the sidecar cannot
/// collapse onto the same temporary path (`ora-update.exe` and `ora-update.json` both would).
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("ora-update"));
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes bytes to a sibling temporary file and renames it over the destination.
async fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), UpdateError> {
    let temporary = temp_path(path);
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(UpdateError::CacheWrite)?;
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(UpdateError::CacheWrite)
}

/// Computes the SHA-256 digest used to detect a changed cached file before installation.
pub(super) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Formats a digest as lowercase hexadecimal for the persisted metadata record.
pub(super) fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
