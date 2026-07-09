/**
 * 加法插件入口（fixture）。
 *
 * 启动即执行：向宿主索取输入数字（getNums），求和后把结果回传（returnNums）。
 * 加法业务逻辑与 add-plugin 中的 `add` 保持一致，只是去掉了 VS Code 风格的
 * activate/deactivate/registerCommand 那套插件生命周期机制——宿主(test_add.rs)
 * 直接把本文件当作一个一次性进程拉起来跑通即可。
 */

// ⚠️ './ora-sdk' 是仅供本测试的占位实现；真实 SDK 就绪后把此路径换成真实包名
//    （前提是其导出的 getNums/returnNums 名称与签名一致，否则需在此适配）。
import { getNums, returnNums } from './ora-sdk';

/** 纯函数：两个整数相加。 */
function add(a: number, b: number): number {
  return a + b;
}

async function main(): Promise<void> {
  const [a, b] = await getNums();
  returnNums(add(a, b));
  // 结果已回传，显式退出，让宿主的读循环收到 EOF 并结束。
  process.exit(0);
}

void main();
