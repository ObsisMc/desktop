//! 直接把 fixture/main.ts 当作插件进程拉起来，跑通加法链路的宿主端测试套件。
//!
//! 本文件只依赖标准库，用 rustc 的内置测试框架编译运行（无需接入 cargo workspace）：
//!
//!     rustc --test test/test_add.rs -o /tmp/test_add && /tmp/test_add
//!
//! 需要从仓库根目录运行（默认按相对路径定位 fixture/main.ts），
//! 也可以设置环境变量 ORA_MAIN_TS 指定 main.ts 的绝对路径。
//!
//! 协议（与 fixture/ora-sdk.ts 对齐，换行分隔 JSON）：
//!   - 插件 → 宿主 (stdout)：{"type":"getNums"} / {"type":"returnNums","result":N}
//!   - 宿主 → 插件 (stdin) ：{"a":..,"b":..}

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

/// 从 {"type":"returnNums","result":15} 这类消息中抽出 result 的数值。
fn parse_result(line: &str) -> i64 {
    const KEY: &str = "\"result\":";
    let start = line.find(KEY).expect("returnNums 消息缺少 result 字段") + KEY.len();
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

        if trimmed.contains("\"getNums\"") {
            // 回应输入：把两个整数以 JSON 写回子进程 stdin。
            writeln!(child_stdin, "{{\"a\":{a},\"b\":{b}}}").expect("写入子进程 stdin 失败");
            child_stdin.flush().expect("flush 子进程 stdin 失败");
        } else if trimmed.contains("\"returnNums\"") {
            result = Some(parse_result(trimmed));
            break;
        }
    }

    let _ = child.wait();
    result.expect("没有收到 returnNums 结果")
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
