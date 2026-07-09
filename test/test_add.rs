//! 直接把 fixture/main.ts 当作插件进程拉起来，跑通加法链路的宿主端测试套件。
//!
//! 本文件只依赖标准库，用 rustc 的内置测试框架编译运行（无需接入 cargo workspace）：
//!
//!     rustc --test test/test_add.rs -o /tmp/test_add && /tmp/test_add
//!
//! # 如何运行
//!
//! 从仓库根目录：
//!
//!     rustc --test test/test_add.rs -o <可写目录>/test_add && <可写目录>/test_add
//!
//! - 只跑某个用例：`<...>/test_add adds_with_zero`
//! - 看插件的 stderr 调试输出（console-guard 会给它加 `[plugin]` 前缀）：加 `--nocapture`
//!
//! # 坑（都是踩过的）
//!
//! 1. 必须从**仓库根目录**运行。fixture 按相对路径 `test/fixture/main.ts` 定位；
//!    在别处运行会找不到 main.ts。可用 `ORA_MAIN_TS=<绝对路径>` 覆盖，绕过这个约束。
//!
//! 2. `rustc` 需要**两处可写目录**，二者独立、缺一不可：
//!      - `-o` 指定的产物路径所在目录（最终测试二进制要落地）；
//!      - 编译期临时目录，存放 `.rmeta`/`.o` 等中间文件，位置由 `TMPDIR` 决定
//!        （未设置则回退 `/tmp`）。这步发生在生成产物**之前**。
//!    本机 `/tmp` 对当前用户不可写，只指 `-o` 仍会在临时文件那步报
//!    `couldn't create a temp dir: Permission denied`。所以要连 `TMPDIR` 一起指到可写目录：
//!
//!        OUT=<可写目录>
//!        TMPDIR=$OUT rustc --test test/test_add.rs -o "$OUT/test_add" && "$OUT/test_add"
//!
//!    （一般环境 `/tmp` 可写，无需设 `TMPDIR`；用 cargo 则中间文件全进 `target/`，不会遇到。）
//!
//! 3. 需要 `bun` 在 PATH 里——harness 内部用 `bun` 拉起 main.ts，且 main.ts 依赖真实
//!    SDK 包 `@ora-space/plugin-sdk`（须已 `bun install`），否则子进程会启动失败。
//!
//! # 协议（与真实 SDK @ora-space/plugin-sdk/host 的 JSON-RPC 2.0 对齐，换行分隔）
//!
//!   - 宿主 → 插件 (stdin) ：{"jsonrpc":"2.0","id":"1","method":"add","params":[a,b]}
//!   - 插件 → 宿主 (stdout)：{"jsonrpc":"2.0","id":"1","result":N}
//!
//! 注意方向：真实 SDK 下由**宿主先主动发请求**，插件读到后再回；这跟旧占位实现
//! （插件先问宿主要数）是反的。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// 定位 fixture/main.ts：优先取 ORA_MAIN_TS 环境变量，否则相对本源文件推导。
fn resolve_main_ts() -> PathBuf {
    if let Some(path) = std::env::var_os("ORA_MAIN_TS") {
        return PathBuf::from(path);
    }
    // file!() 形如 "test/test_add.rs"，其父目录即 test/。
    let source_dir = PathBuf::from(file!())
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default();
    source_dir.join("fixture").join("main.ts")
}

/// 从 {"jsonrpc":"2.0","id":"1","result":15} 这类响应中抽出 result 的数值。
fn parse_result(line: &str) -> i64 {
    const KEY: &str = "\"result\":";
    let start = line.find(KEY).expect("响应消息缺少 result 字段") + KEY.len();
    let rest = &line[start..];
    // result 后面跟着 } 或 ,（本协议里是 }），取到分隔符为止。
    let end = rest
        .find(|c: char| c == '}' || c == ',')
        .unwrap_or(rest.len());
    rest[..end].trim().parse().expect("result 不是合法整数")
}

/// 拉起一次 main.ts，喂入 (a, b)，走完协议后返回插件回传的结果。
///
/// 每次调用都是独立的一次性子进程，方便并行跑多个用例互不干扰。
fn run_add(a: i64, b: i64) -> i64 {
    let main_ts = resolve_main_ts();
    assert!(
        main_ts.exists(),
        "找不到 main.ts：{}（请从仓库根目录运行，或设置 ORA_MAIN_TS）",
        main_ts.display()
    );

    let mut child = Command::new("bun")
        .arg(&main_ts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("拉起 bun 失败：请确认已安装 bun");

    let mut child_stdin = child.stdin.take().expect("拿不到子进程 stdin");
    let child_stdout = child.stdout.take().expect("拿不到子进程 stdout");
    let mut reader = BufReader::new(child_stdout);

    // 新协议下由宿主主动发起请求：把 add 请求以一行 JSON-RPC 写入子进程 stdin。
    writeln!(
        child_stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"add\",\"params\":[{a},{b}]}}"
    )
    .expect("写入子进程 stdin 失败");
    child_stdin.flush().expect("flush 子进程 stdin 失败");

    let mut result: Option<i64> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).expect("读取子进程 stdout 失败");
        if read == 0 {
            break; // EOF：子进程已退出
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains("\"result\":") {
            result = Some(parse_result(trimmed));
            break;
        }
    }

    let _ = child.wait();
    result.expect("没有收到 result 响应")
}

#[test]
fn adds_two_positive_integers() {
    assert_eq!(run_add(3, 4), 7);
}

#[test]
fn adds_with_zero() {
    assert_eq!(run_add(0, 0), 0);
    assert_eq!(run_add(0, 9), 9);
}

#[test]
fn adds_negative_operand() {
    assert_eq!(run_add(-5, 2), -3);
}

#[test]
fn adds_two_negative_integers() {
    assert_eq!(run_add(-10, -20), -30);
}

#[test]
fn addition_is_commutative() {
    assert_eq!(run_add(7, 3), run_add(3, 7));
}

#[test]
fn adds_large_integers() {
    assert_eq!(run_add(1_000_000, 2_000_000), 3_000_000);
}
