//! 可运行的二进制版：与 tests/test_add.rs 同样的逻辑，但把结果打印到 stdout。
//!
//! 通过真实的 `ora_plugin_manager::PluginManager::number_add` 拉起 bun 执行 `.data/plugins/main.ts`，
//! 走完 JSON-RPC 协议后打印加法结果。运行现场（data_dir = 仓库根 .data、bin/bun 符号链接、
//! node_modules 解析）与集成测试完全一致，只是入口从 `#[tokio::test]` 断言换成 `#[tokio::main]` 打印。
//!
//! # 如何运行
//!
//!     cargo run -p ora-plugin-manager --example number_add -- <a> <b>
//!     # 例：cargo run -p ora-plugin-manager --example number_add -- 3 4   => 7
//!
//! 前置条件同测试：PATH 上有 `bun`（或设 `ORA_BUN`）、仓库根 node_modules 里有
//! `@ora-space/plugin-sdk`（先 `pnpm install`）、`.data/plugins/main.ts` 存在（.data 被 gitignore）。
//! 放在 `examples/` 下由 cargo 自动发现，无需改 Cargo.toml（example 可用 dev-dependencies 的 tokio rt）。

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::{env, fs, process};

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

    let bin_dir = data_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("创建 .data/bin 目录失败");
    let bun_link = bin_dir.join("bun");
    if !bun_link.exists() {
        // 唯一临时名 + rename 原子落地，规避并发建链接的竞态。
        static STAGING_SEQ: AtomicUsize = AtomicUsize::new(0);
        let staging = bin_dir.join(format!(
            "bun.{}.{}.tmp",
            process::id(),
            STAGING_SEQ.fetch_add(1, Ordering::SeqCst),
        ));
        let _ = fs::remove_file(&staging);
        if let Err(error) = symlink(locate_bun(), &staging) {
            panic!("链接 .data/bin/bun 失败：{error}");
        }
        let _ = fs::rename(&staging, &bun_link);
        let _ = fs::remove_file(&staging);
    }
    assert!(bun_link.exists(), "补建 .data/bin/bun 后仍不存在");
}

/// 解析命令行的两个整数操作数；缺参或非法则打印用法并退出。
fn parse_operands() -> (i64, i64) {
    let mut args = env::args().skip(1);
    let a = args.next().and_then(|value| value.parse::<i64>().ok());
    let b = args.next().and_then(|value| value.parse::<i64>().ok());
    match (a, b) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            eprintln!(
                "用法: cargo run -p ora-plugin-manager --example number_add -- <a> <b>\n\
                 例:   cargo run -p ora-plugin-manager --example number_add -- 3 4"
            );
            process::exit(2);
        }
    }
}

// current_thread flavor：只需 dev-dependencies 里已有的 tokio `rt` + `macros`，
// 不必开 `rt-multi-thread`，从而不用改 Cargo.toml。
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (a, b) = parse_operands();

    let data_dir = data_dir();
    ensure_plugin_runtime(&data_dir);

    let manager = PluginManager::new(
        PluginManagerConfig::with_request_timeout(data_dir, Duration::from_secs(30)),
        TokioProcessSpawner::new(),
    );

    match manager.number_add(a, b).await {
        Ok(result) => println!("{a} + {b} = {result}"),
        Err(error) => {
            eprintln!("number_add({a}, {b}) 失败：{error}");
            process::exit(1);
        }
    }
}
