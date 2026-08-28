//! Owns the Desktop updater state machine and the scheduler registrations that drive it.

use super::cache::{UpdateCache, UpdateMetadata, digest, hex_digest};
use super::job::UpdateJob;
use super::platform::{InstallSupport, install_support};
use super::{DesktopUpdateStatus, UpdateError};
use ora_backend::Backend;
use ora_logging::{ora_error, ora_info, ora_warn};
use ora_scheduler::{CronHandle, DelayHandle, Scheduler};
use semver::Version;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

const UPDATE_EVENT: &str = "desktop-update-status-changed";
const INITIAL_CHECK_DELAY: Duration = Duration::from_secs(60);
/// Progress is emitted per megabyte: a per-chunk event would flood the webview bridge without
/// moving a progress bar any further than this does.
const PROGRESS_EVENT_INTERVAL: u64 = 1024 * 1024;

/// Selects whether the Desktop composition root should register release update work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopUpdateMode {
    /// Register the delayed and recurring release update checks.
    Enabled,
    /// Keep commands available but do not perform network work in development builds.
    Disabled,
}

/// Holds the updater object whose package bytes were verified by `Update::download`.
#[derive(Clone)]
struct PendingUpdate {
    update: Update,
    version: String,
    digest: [u8; 32],
}

/// Drives update checks, downloads, and installation for the Desktop application.
#[derive(Clone)]
pub struct UpdateService {
    inner: Arc<UpdateServiceInner>,
}

struct UpdateServiceInner {
    app: AppHandle,
    backend: Backend,
    cache: UpdateCache,
    status: Mutex<DesktopUpdateStatus>,
    pending: Mutex<Option<PendingUpdate>>,
    operation: AsyncMutex<()>,
    _scheduler: Scheduler,
    _initial_check: Mutex<Option<DelayHandle>>,
    _cron: Mutex<Option<CronHandle>>,
}

impl UpdateService {
    /// Creates the service, removes cache entries already superseded by this build, and schedules
    /// the first delayed check plus the recurring six-hour check.
    pub fn start(
        app: AppHandle,
        backend: Backend,
        home_directory: &Path,
        timezone: chrono_tz::Tz,
        mode: DesktopUpdateMode,
    ) -> Result<Self, UpdateError> {
        let cache = UpdateCache::open(home_directory)?;
        cache.discard_superseded(
            &Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0)),
        )?;

        let scheduler = Scheduler::new(timezone);
        let service = Self {
            inner: Arc::new(UpdateServiceInner {
                app,
                backend,
                cache,
                status: Mutex::new(DesktopUpdateStatus::Current),
                pending: Mutex::new(None),
                operation: AsyncMutex::new(()),
                _scheduler: scheduler.clone(),
                _initial_check: Mutex::new(None),
                _cron: Mutex::new(None),
            }),
        };

        if matches!(mode, DesktopUpdateMode::Enabled) {
            let delayed_service = service.clone();
            let initial = scheduler
                .schedule_after(INITIAL_CHECK_DELAY, async move {
                    delayed_service.check_and_download().await;
                })
                .map_err(UpdateError::Scheduler)?;
            let cron = scheduler
                .schedule_cron(UpdateJob::new(service.clone()))
                .map_err(UpdateError::Scheduler)?;
            *service
                .inner
                ._initial_check
                .lock()
                .expect("update initial handle mutex is not poisoned") = Some(initial);
            *service
                .inner
                ._cron
                .lock()
                .expect("update cron handle mutex is not poisoned") = Some(cron);
        }
        Ok(service)
    }

    /// Returns the latest status snapshot for a command or a freshly mounted frontend.
    pub fn status(&self) -> DesktopUpdateStatus {
        self.inner
            .status
            .lock()
            .expect("update status mutex is not poisoned")
            .clone()
    }

    /// Runs one check and downloads a verified package when a newer release exists.
    ///
    /// A package that is already installable stays advertised across a failed check: retracting
    /// the notification would strand the user without an install entry point until the next cron
    /// tick, even though the verified bytes are still in memory and on disk.
    pub async fn check_and_download(&self) {
        let _operation = self.inner.operation.lock().await;
        let previous = self.status();
        if !matches!(previous, DesktopUpdateStatus::Ready { .. }) {
            self.set_status(DesktopUpdateStatus::Checking);
        }
        if let Err(error) = self.check_and_download_inner().await {
            ora_warn!(message = "Desktop update check failed", error = %error);
            match previous {
                DesktopUpdateStatus::Ready { .. } => self.set_status(previous),
                _ => self.set_status(DesktopUpdateStatus::Failed {
                    message: error.to_string(),
                }),
            }
        }
    }

    /// Installs the package downloaded by the most recent successful check and restarts the app
    /// on platforms where the updater does not terminate the current process itself.
    pub async fn install(&self) -> Result<(), UpdateError> {
        let _operation = self.inner.operation.lock().await;
        let pending = self
            .inner
            .pending
            .lock()
            .expect("pending update mutex is not poisoned")
            .clone()
            .ok_or(UpdateError::NoPendingUpdate)?;
        self.set_status(DesktopUpdateStatus::Installing {
            version: pending.version.clone(),
        });

        // A failed installation keeps the verified package advertised so the user can retry.
        let restore = DesktopUpdateStatus::Ready {
            version: pending.version.clone(),
        };
        let bytes = match tokio::fs::read(self.inner.cache.package_path()).await {
            Ok(bytes) => bytes,
            Err(error) => {
                self.set_status(restore);
                return Err(UpdateError::CacheRead(error));
            }
        };
        if digest(&bytes) != pending.digest {
            self.set_status(restore);
            return Err(UpdateError::CachedArtifactChanged);
        }
        if let Err(error) = pending.update.install(bytes) {
            self.set_status(restore);
            return Err(UpdateError::Updater(error));
        }
        self.clear_pending();
        self.inner.cache.clear();
        // Windows hands control to the NSIS updater, which terminates this process itself; the
        // other platforms replace the bundle in place and have to be restarted here.
        #[cfg(target_os = "windows")]
        {
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.inner.app.restart()
        }
    }

    /// Performs one update request through the configured Tauri updater endpoint.
    async fn check_and_download_inner(&self) -> Result<(), UpdateError> {
        let mut updater_builder = self.inner.app.updater_builder();
        if let Some(settings) = self
            .inner
            .backend
            .network_proxy_settings()
            .map_err(|error| UpdateError::ProxySettings(error.to_string()))?
        {
            updater_builder = updater_builder.proxy(proxy_url(&settings)?);
        }
        let updater = updater_builder.build().map_err(UpdateError::Updater)?;
        let Some(update) = updater.check().await.map_err(UpdateError::Updater)? else {
            self.clear_pending();
            self.inner.cache.clear();
            self.set_status(DesktopUpdateStatus::Current);
            return Ok(());
        };

        // The bytes verified earlier in this process are still installable, so a repeat check for
        // the same release must not spend the download again.
        if self.pending_version().as_deref() == Some(update.version.as_str()) {
            self.set_status(DesktopUpdateStatus::Ready {
                version: update.version,
            });
            return Ok(());
        }

        if let InstallSupport::Manual(reason) = install_support() {
            self.clear_pending();
            self.inner.cache.clear();
            ora_info!(
                message = "Desktop update requires a manual installation",
                version = %update.version,
            );
            self.set_status(DesktopUpdateStatus::ManualUpdate {
                version: update.version,
                reason,
            });
            return Ok(());
        }

        self.set_status(DesktopUpdateStatus::Downloading {
            version: update.version.clone(),
            downloaded: 0,
            total: None,
        });
        let bytes = self.download(&update).await?;
        let digest = digest(&bytes);
        self.inner
            .cache
            .store(
                &bytes,
                &UpdateMetadata {
                    schema_version: 1,
                    release_version: update.version.clone(),
                    sha256: hex_digest(&digest),
                    file_name: self.inner.cache.package_file_name().to_owned(),
                },
            )
            .await?;

        let version = update.version.clone();
        *self
            .inner
            .pending
            .lock()
            .expect("pending update mutex is not poisoned") = Some(PendingUpdate {
            version: version.clone(),
            update,
            digest,
        });
        ora_info!(message = "Desktop update downloaded", version = %version);
        self.set_status(DesktopUpdateStatus::Ready { version });
        Ok(())
    }

    /// Downloads the signed package while publishing throttled progress to the webview.
    async fn download(&self, update: &Update) -> Result<Vec<u8>, UpdateError> {
        let service = self.clone();
        let version = update.version.clone();
        let mut downloaded = 0u64;
        let mut published = 0u64;
        update
            .download(
                move |chunk_length, content_length| {
                    downloaded += chunk_length as u64;
                    let complete = content_length == Some(downloaded);
                    if !complete && downloaded - published < PROGRESS_EVENT_INTERVAL {
                        return;
                    }
                    published = downloaded;
                    service.set_status(DesktopUpdateStatus::Downloading {
                        version: version.clone(),
                        downloaded,
                        total: content_length,
                    });
                },
                || {},
            )
            .await
            .map_err(UpdateError::Updater)
    }

    /// Publishes a status snapshot to the main webview without making event delivery mandatory.
    fn set_status(&self, status: DesktopUpdateStatus) {
        *self
            .inner
            .status
            .lock()
            .expect("update status mutex is not poisoned") = status.clone();
        if let Err(error) = self.inner.app.emit(UPDATE_EVENT, status) {
            ora_error!(message = "failed to publish Desktop update status", error = %error);
        }
    }

    /// Returns the release currently held as an installable package, if any.
    fn pending_version(&self) -> Option<String> {
        self.inner
            .pending
            .lock()
            .expect("pending update mutex is not poisoned")
            .as_ref()
            .map(|pending| pending.version.clone())
    }

    /// Drops the in-memory installable package.
    fn clear_pending(&self) {
        self.inner
            .pending
            .lock()
            .expect("pending update mutex is not poisoned")
            .take();
    }
}

/// Converts persisted host proxy settings into the URL accepted by Tauri updater.
pub(super) fn proxy_url(
    settings: &ora_application::NetworkProxySettings,
) -> Result<Url, UpdateError> {
    let mut url = Url::parse("http://proxy.invalid").map_err(UpdateError::Proxy)?;
    url.set_host(Some(&settings.host))
        .map_err(|_| UpdateError::ProxyCredentials)?;
    url.set_port(Some(settings.port))
        .map_err(|_| UpdateError::ProxyCredentials)?;
    if let Some(username) = &settings.username {
        url.set_username(username)
            .map_err(|_| UpdateError::ProxyCredentials)?;
    }
    if let Some(password) = &settings.password {
        url.set_password(Some(password))
            .map_err(|_| UpdateError::ProxyCredentials)?;
    }
    Ok(url)
}
