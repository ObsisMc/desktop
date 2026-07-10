//! 可运行的二进制版：与 tests/test_add.rs 同样的逻辑，但把结果打印到 stdout。
//!
//! 通过真实的 `ora_plugin_manager::PluginManager::number_add` 拉起 bun 执行 `.data/plugins/main.ts`，
//! 走完 JSON-RPC 协议后打印加法结果。运行现场（data_dir = 仓库根 .data、bin 下补建 bun、
//! node_modules 解析）与集成测试一致，只是入口从 `#[tokio::test]` 断言换成 `#[tokio::main]` 打印。
//!
//! **跨平台**：可在 Windows / unix 上运行。宿主按平台找 `bin/bun.exe`（Windows）或 `bin/bun`（unix）；
//! 本例把真实 bun 补建到该位置——unix 用符号链接（省空间），Windows 用拷贝（免去符号链接的权限要求）。
//!
//! # 如何运行
//!
//!     cargo run -p ora-plugin-manager --example number_add -- <a> <b>
//!     # 例：cargo run -p ora-plugin-manager --example number_add -- 3 4   => 7
//!
//! 前置条件同测试：PATH 上有 `bun`（Windows 为 `bun.exe`；或设 `ORA_BUN`）、仓库根 node_modules 里有
//! `@ora-space/plugin-sdk`（先 `pnpm install`）、`.data/plugins/main.ts` 存在（.data 被 gitignore）。
//! 放在 `examples/` 下由 cargo 自动发现，无需改 Cargo.toml（example 可用 dev-dependencies 的 tokio rt）。

use std::io;
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

/// 当前平台的 bun 可执行文件名（宿主 process_spec 也是这么拼的）。
fn bun_file_name() -> &'static str {
    if cfg!(windows) { "bun.exe" } else { "bun" }
}

/// 定位 bun 可执行文件：优先 ORA_BUN，否则在 PATH 上逐目录查找。
fn locate_bun() -> PathBuf {
    if let Some(explicit) = env::var_os("ORA_BUN") {
        return PathBuf::from(explicit);
    }
    let name = bun_file_name();
    let path = env::var_os("PATH").expect("PATH 未设置，无法定位 bun");
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("PATH 上找不到 {name}；请安装 bun 或设置 ORA_BUN 指向可执行文件"))
}

/// 把真实 bun 补建到 `dst`：unix 用符号链接（省空间），Windows 用拷贝（免符号链接权限）。
#[cfg(unix)]
fn provision_bun(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

/// 把真实 bun 补建到 `dst`：unix 用符号链接（省空间），Windows 用拷贝（免符号链接权限）。
#[cfg(windows)]
fn provision_bun(src: &Path, dst: &Path) -> io::Result<()> {
    fs::copy(src, dst).map(|_| ())
}

/// 确保 data_dir 里有 `number_add` 需要的布局：校验插件存在、按需补建 bin/<bun>。
fn ensure_plugin_runtime(data_dir: &Path) {
    let plugin = data_dir.join("plugins").join("main.ts");
    assert!(
        plugin.is_file(),
        "缺少插件 {}；.data 被 gitignore，请先把插件 stage 到 .data/plugins",
        plugin.display(),
    );

    let bin_dir = data_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("创建 .data/bin 目录失败");
    let bun_dst = bin_dir.join(bun_file_name());
    if !bun_dst.exists() {
        // 唯一临时名 + rename 原子落地，避免部分写入 / 并发补建的竞态。
        static STAGING_SEQ: AtomicUsize = AtomicUsize::new(0);
        let staging = bin_dir.join(format!(
            "{}.{}.{}.tmp",
            bun_file_name(),
            process::id(),
            STAGING_SEQ.fetch_add(1, Ordering::SeqCst),
        ));
        let _ = fs::remove_file(&staging);
        if let Err(error) = provision_bun(&locate_bun(), &staging) {
            panic!("补建 {} 失败：{error}", bun_dst.display());
        }
        // rename 覆盖：即便别的进程已抢先建好也无妨（Windows 下目标不存在才走到这，rename 可成功）。
        let _ = fs::rename(&staging, &bun_dst);
        let _ = fs::remove_file(&staging);
    }
    assert!(bun_dst.exists(), "补建 {} 后仍不存在", bun_dst.display());
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
