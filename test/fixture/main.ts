/**
 * 加法插件入口（fixture）。
 *
 * 启动即执行：从宿主读取一条 JSON-RPC 请求（getNums），取出待相加的两个整数，
 * 求和后按同一个 id 回传结果（returnNums）。加法业务逻辑与 add-plugin 中的
 * `add` 保持一致，只是去掉了 VS Code 风格的 activate/deactivate/registerCommand
 * 那套插件生命周期机制——宿主(test_add.rs)直接把本文件当作一次性进程拉起来跑通即可。
 *
 * 协议（真实 SDK 的 JSON-RPC 2.0，换行分隔，一行一条）：
 *   - 宿主 → 插件 (stdin) ：{"jsonrpc":"2.0","id":"1","method":"add","params":[a,b]}
 *   - 插件 → 宿主 (stdout)：{"jsonrpc":"2.0","id":"1","result":<number>}
 *
 * 单独手跑（宿主先发请求，插件读后回）；完整测试见 test/test_add.rs：
 *   echo '{"jsonrpc":"2.0","id":"1","method":"add","params":[3,4]}' | bun test/fixture/main.ts
 *   # => {"jsonrpc":"2.0","id":"1","result":7}
 * 需能解析到真实 SDK 包 @ora-space/plugin-sdk（须先 bun install）。
 */

import { getNums, returnNums } from "@ora-space/plugin-sdk/host";

/** 纯函数：两个整数相加。 */
function add(a: number, b: number): number {
  return a + b;
}

async function main(): Promise<void> {
  // getNums 是真实 SDK 的通用「读取下一条请求」原语，返回 {id, method, params}；
  // null 表示 stdin 已关闭（宿主没发请求就退出）。
  const request = await getNums();
  if (request === null) {
    process.exit(0);
  }

  const [a, b] = request.params as [number, number];
  // returnNums 是通用「按 id 回传成功响应」原语，须带上请求的 id。
  await returnNums(request.id, add(a, b));

  // 结果已回传，显式退出，让宿主的读循环收到 EOF 并结束。
  process.exit(0);
}

void main();
