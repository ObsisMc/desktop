/**
 * ORA SDK（fixture 占位实现）—— ⚠️ 仅供 test_add 这个测试使用，不是正式 SDK。
 *
 * 这个文件是为了让 test_add.rs 能直接拉起 main.ts 跑通一次加法而临时造的占位，
 * 只实现了本测试需要的 getNums / returnNums，签名也是为「加法」量身定的
 * （getNums 固定返回两个整数）。它【不是】通用的 ora-sdk，也不打算覆盖真实
 * 协议的全部能力。
 *
 * 等真实的 ora-sdk 包就绪后，把 main.ts 里的
 *     import { getNums, returnNums } from './ora-sdk';
 * 换成真实包路径即可——但前提是真实包导出的函数名与签名和这里一致；
 * 若不一致（大概率如此，真实 SDK 是通用的、不会内建加法专用的取数接口），
 * 需要在 main.ts 里做一层适配，或由真实包提供同形状的适配器。
 *
 * 和 add-plugin 里的 ora-sdk 一样，插件业务逻辑只依赖这里暴露的函数，
 * 完全不关心底层通信细节。区别在于：这里用的是「启动即跑」模型
 * （getNums 取输入 / returnNums 返回结果），而不是 VS Code 风格的
 * registerCommand 命令注册模型——因为本 fixture 只需要打通宿主(Rust)
 * 直接拉起 main.ts 并跑通一次加法的最小链路。
 *
 * 线路协议：以换行分隔的 JSON，一行一条消息。
 *   - 插件 → 宿主 (stdout)：{"type":"getNums"}
 *                          {"type":"returnNums","result":<number>}
 *   - 宿主 → 插件 (stdin) ：{"a":<number>,"b":<number>}
 *
 * 注意：stdout 被协议独占，任何调试输出都必须走 stderr（console.error），
 * 否则会污染协议流。
 */

import { createInterface } from 'node:readline';

// 逐行读取 stdin。node:readline 在 bun 下同样可用。
const rl = createInterface({ input: process.stdin });

// 已到达但还没被 await 消费的行，做一个简单缓冲，避免丢消息。
const buffered: string[] = [];
// 正在等待下一行的消费者（同一时刻最多一个）。
let waiter: ((line: string) => void) | null = null;

rl.on('line', (line) => {
  if (waiter) {
    const resolve = waiter;
    waiter = null;
    resolve(line);
  } else {
    buffered.push(line);
  }
});

/** 取宿主发来的下一行；已有缓冲则立即返回，否则挂起等待。 */
function nextLine(): Promise<string> {
  const queued = buffered.shift();
  if (queued !== undefined) {
    return Promise.resolve(queued);
  }
  return new Promise((resolve) => {
    waiter = resolve;
  });
}

/** 向宿主发送一条协议消息（stdout，一行一条）。 */
function send(message: unknown): void {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

/**
 * 向宿主索取本次运行的两个整型输入。
 * 发出 getNums 请求后阻塞等待宿主回传 {"a":..,"b":..}。
 */
export async function getNums(): Promise<[number, number]> {
  send({ type: 'getNums' });
  const line = await nextLine();
  const message = JSON.parse(line) as { a: number; b: number };
  return [message.a, message.b];
}

/**
 * 把计算结果回传给宿主。
 * 发完即结束，调用方应随后退出进程（宿主据此判定一次运行完成）。
 */
export function returnNums(result: number): void {
  send({ type: 'returnNums', result });
}
