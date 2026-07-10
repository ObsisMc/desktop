//! 端到端测试：通过真实的 `ora_plugin_manager::PluginManager::number_add` 跑通加法链路。
//!
//! 与旧版（自己手写 JSON-RPC、`rustc --test` 独立编译）不同，本文件现在是
//! **plugin-manager crate 的一个集成测试**（位于该 crate 的 `tests/` 下，由 cargo 自动发现，
//! 无需在 Cargo.toml 里显式登记），直接调用宿主的公开 API
//! `PluginManager::number_add(a, b)`，让它用真实的 `TokioProcessSpawner` 拉起 bun
//! 执行插件，走完整条 stdin→插件→stdout 的 JSON-RPC 协议。
//! 协议细节（请求/响应形状、params 为 {a,b}）由 manager 与 SDK 各自负责，测试只断言结果。
//!
//! # 如何运行
//!
//!     cargo test -p ora-plugin-manager --test test_add
//!     # 看插件 stderr（console-guard 会加 [plugin] 前缀）：加 -- --nocapture
//!
//! # 运行现场（data_dir = 仓库根 .data）
//!
//! `number_add` 写死了从 `<data_dir>/bin/bun` 拉起 `<data_dir>/plugins/main.ts`、cwd=data_dir。
//! 这里直接用仓库根的 `.data` 作为 data_dir——插件源就是 `.data/plugins/main.ts`（真源）。布局：
//!   - `.data/plugins/main.ts`：插件本体（已在库外，.data 被 gitignore）。
//!   - `.data/bin/bun`：测试按需补建的、指向真实 bun 的符号链接。
//!   - node_modules：无需额外处理——`.data` 在仓库内，bun 从 `.data/plugins` 向上即可
//!     解析到仓库根 `node_modules` 里的 `@ora-space/plugin-sdk`。
//!
//! # 前置条件与坑
//!
//! 1. 需要 `bun` 在 PATH 上（或用 `ORA_BUN` 指向可执行文件）——manager 会真的拉起 bun 子进程。
//! 2. 需要仓库根 `node_modules` 里有 `@ora-space/plugin-sdk`（先 `pnpm install`），否则插件里的
//!    `import ... from "@ora-space/plugin-sdk/host"` 解析失败、子进程报错退出。
//! 3. 需要 `.data/plugins/main.ts` 存在。`.data` 被 gitignore、不随仓库分发；干净 checkout / CI
//!    上要先把插件 stage 到 `.data/plugins`，否则本测试会 panic 提示缺文件。
//! 4. unix-only：依赖符号链接与 bun；非 unix 下本目标编译为空。

#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{env, fs};

use ora_plugin_manager::{PluginManager, PluginManagerConfig};
use ora_process::TokioProcessSpawner;

/// 仓库根目录。CARGO_MANIFEST_DIR 指向 crates/plugin-manager，向上两级即根。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("无法定位仓库根目录")
}

/// 插件运行时目录 data_dir（仓库根 .data）。
fn data_dir() -> PathBuf {
    repo_root().join(".data")
}

/// 定位 bun 可执行文件：优先 ORA_BUN，否则在 PATH 上逐目录查找。
fn locate_bun() -> PathBuf {
    if let Some(explicit) = env::var_os("ORA_BUN") {
        return PathBuf::from(explicit);
    }
    let path = env::var_os("PATH").expect("PATH 未设置，无法定位 bun");
    env::split_paths(&path)
        .map(|dir| dir.join("bun"))
        .find(|candidate| candidate.is_file())
        .expect("PATH 上找不到 bun；请安装 bun 或设置 ORA_BUN 指向可执行文件")
}

/// 确保 data_dir 里有 `number_add` 需要的布局：校验插件存在、按需补建 bin/bun 符号链接。
fn ensure_plugin_runtime(data_dir: &Path) {
    let plugin = data_dir.join("plugins").join("main.ts");
    assert!(
        plugin.is_file(),
        "缺少插件 {}；.data 被 gitignore，请先把插件 stage 到 .data/plugins",
        plugin.display(),
    );

    // bin/bun → 真实 bun。已存在则原样复用（多个用例并发时避免重复创建报错）。
    let bin_dir = data_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("创建 .data/bin 目录失败");
    let bun_link = bin_dir.join("bun");
    if !bun_link.exists() {
        // 用「唯一临时名 + rename 原子落地」规避并发用例同时建链接的竞态。
        // 同一测试二进制内各用例是线程、共享 pid，故临时名再加一个进程内自增序号保证唯一。
        static STAGING_SEQ: AtomicUsize = AtomicUsize::new(0);
        let staging = bin_dir.join(format!(
            "bun.{}.{}.tmp",
            std::process::id(),
            STAGING_SEQ.fetch_add(1, Ordering::SeqCst),
        ));
        let _ = fs::remove_file(&staging);
        if let Err(error) = symlink(locate_bun(), &staging) {
            panic!("链接 .data/bin/bun 失败：{error}");
        }
        // rename 覆盖：即便别的用例已抢先建好 bun 也无妨。
        let _ = fs::rename(&staging, &bun_link);
        let _ = fs::remove_file(&staging);
    }
    assert!(bun_link.exists(), "补建 .data/bin/bun 后仍不存在");
}

/// 通过真实 PluginManager 跑一次加法：拉起 bun 执行插件，走完 JSON-RPC 协议后返回结果。
async fn number_add(a: i64, b: i64) -> i64 {
    let data_dir = data_dir();
    ensure_plugin_runtime(&data_dir);

    // 冷启动 bun 首次解析/转译可能略慢，给个宽松超时避免偶发 flake。
    let manager = PluginManager::new(
        PluginManagerConfig::with_request_timeout(data_dir, Duration::from_secs(30)),
        TokioProcessSpawner::new(),
    );

    manager
        .number_add(a, b)
        .await
        .unwrap_or_else(|error| panic!("number_add({a}, {b}) 失败：{error}"))
}

#[tokio::test]
async fn adds_two_positive_integers() {
    assert_eq!(number_add(3, 4).await, 7);
}

#[tokio::test]
async fn adds_with_zero() {
    assert_eq!(number_add(0, 0).await, 0);
    assert_eq!(number_add(0, 9).await, 9);
}

#[tokio::test]
async fn adds_negative_operand() {
    assert_eq!(number_add(-5, 2).await, -3);
}

#[tokio::test]
async fn adds_two_negative_integers() {
    assert_eq!(number_add(-10, -20).await, -30);
}

#[tokio::test]
async fn addition_is_commutative() {
    assert_eq!(number_add(7, 3).await, number_add(3, 7).await);
}

#[tokio::test]
async fn adds_large_integers() {
    assert_eq!(number_add(1_000_000, 2_000_000).await, 3_000_000);
}
