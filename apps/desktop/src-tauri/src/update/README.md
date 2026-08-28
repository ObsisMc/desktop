# update

Desktop application self-update orchestration around Tauri's signed updater plugin.

## Responsibilities

- Own the Desktop-side lifecycle of software updates: when to check, when to download, when the
  user may install, and what the main webview is told about it.
- Hold a Desktop-scoped `ora_scheduler::Scheduler` that runs one delayed check after start and a
  recurring check afterwards.
- Persist exactly one downloaded package under `~/.ora/cache/` together with an identity record,
  and drop that package once the running build supersedes it.
- Decide whether the running installation is one the updater is allowed to replace in place.

## Non-responsibilities

- Update transport, manifest parsing, version comparison, artifact selection, and signature
  verification belong to `tauri_plugin_updater`. This module never re-implements them and never
  treats its own SHA-256 record as a substitute for the plugin's signature check.
- Release artifact production and `latest.json` generation belong to the build workflow.
- Proxy configuration is owned by the backend user configuration; this module only reads it.

## Boundaries

- `UpdateService` is the only public entry point besides the three Tauri commands re-exported from
  the module root (status, install, and an on-demand check). `DesktopUpdateStatus` and
  `ManualUpdateReason` are the wire contract shared with
  `packages/app-shell/src/platform/types.ts`.
- `DesktopUpdateMode` lets the composition root disable network work in development builds while
  keeping the commands and the state machine reachable from tests.

## Lifecycle

1. `UpdateService::start` opens the cache, discards a package the running build already includes,
   and — in `Enabled` mode — registers the delayed check and the cron job.
2. Each check rebuilds the updater so a proxy the user changed since the last check is picked up,
   then asks the plugin for an update.
3. A newer release is downloaded, verified by the plugin, hashed, and written to the fixed cache
   path; the status becomes `Ready` and the webview shows the install affordance.
4. `install` re-reads the cached bytes, compares the digest recorded at download time, and hands
   the bytes to the plugin. Non-Windows platforms restart; Windows is terminated by the installer.

## Invariants

- Only one update operation runs at a time. The delayed first check and the cron schedule can
  overlap in principle, so both go through the same asynchronous operation lock.
- A package that has already been downloaded stays advertised. Neither a later check nor a failed
  check retracts a `Ready` status, because the verified bytes remain installable.
- The cache never holds two releases. Writes go through sibling temporary files and a rename, so a
  reader never sees a partial package or a record naming a file that is not there.
- Signature verification only happens inside `Update::download`. A package left on disk by a
  previous process is therefore never installed directly; the next check downloads it again.

## Failure semantics

- A failed check is logged and surfaced as `Failed` only when nothing was installable before it.
- A failed installation restores `Ready` and keeps the cache, so the user can retry.
- An installation that the updater cannot perform — a `deb` or `rpm` package, or a bare executable
  on Linux — is reported as `ManualUpdate` before any download is spent, because the static
  manifest only advertises an AppImage for Linux.

## Interactions

- `crate::state::DesktopState` holds the service; `crate::lib` builds it in the composition root.
- `ora_scheduler` provides the delayed and cron registrations.
- `ora_backend::Backend` provides the network proxy settings read before every check.
