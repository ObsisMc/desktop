# Desktop Runtime

`apps/desktop/src-tauri` is an independent Cargo workspace that hosts the same persisted CRUD capabilities as the Web server without running an HTTP server.

## Shared Backend and Commands

Desktop constructs one cloneable `ora-backend::Backend`. It exposes one snake-case Tauri command for each Project, Task, Session, Skill, and Agent CRUD operation (25 commands total). Every command accepts the complete `ora-contracts` request DTO, executes the synchronous backend method on Tauri's blocking executor, and returns the matching response DTO.

The frontend injects `createTauriTransport()` into `createContractsClient`. The transport maps contract operation names to Tauri commands and forwards the original request DTO unchanged. Shared backend errors retain the same public code and message as Web errors; Tauri transport errors have no HTTP status.

The current Desktop slice explicitly returns `unsupported_operation` for:

- opening a project work context;
- renewing a project work context;
- listing a server filesystem directory.

These exclusions do not affect the 25 shared CRUD operations. `ProjectWorkContext` remains outside this extraction.

## Persistent Paths

The Tauri identifier is `space.ora.desktop`. Tauri's system `app_data_dir` owns all default runtime state:

- SQLite: `app_data_dir/ora.sqlite3`
- Configuration: `app_data_dir/config.json`
- Logs: `app_data_dir/logs/ora.log`
- Default new-worktree root: `app_data_dir/worktrees`

On first launch, Desktop creates the app data directory, default worktree directory, and a versioned configuration file using an atomic sibling-temporary-file replacement. Existing malformed, unknown-version, or otherwise invalid configuration is fatal; Desktop does not silently reset it.

The worktree root is non-sensitive configuration. Users can change it from Settings → Data & privacy on Desktop. A selected value must be an absolute path to an existing directory. The new value affects task creations that start after the update; in-flight operations retain their original snapshot, and existing worktrees are not moved.

The configured root is only a creation target. Existing worktree locations are resolved from the stored branch name and `git worktree list --porcelain`, so changing the root does not break later task deletion.

## Logging

Desktop initializes `ora-logging` before opening the backend and registers the Gitlancer logger bridge. Logs rotate daily and retain three files. Debug builds write to stdout and the file; release builds write to the file only. The logging guard remains managed for the application lifetime.

## Verification

The Tauri Rust crate keeps its own `Cargo.lock` and is intentionally excluded from the root Cargo workspace. `task test:desktop` checks the Desktop transport, formatting, Clippy, and the independent Rust tests. `task test` includes this task explicitly.
