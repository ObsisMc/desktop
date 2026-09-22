# Controller–Cloud 契约

[English](controller-cloud-contract.md) | 中文

API Server（Ora Cloud，Go）通过 gRPC 调用 Controller。契约就是本仓库
[`proto/`](../../proto/ora/controller/v1/controller.proto) 下的 `.proto` 文件，由作为服务端的
Controller 拥有；两侧都从同一份文件生成代码，不做任何人工对齐。租户留在 Cloud：契约只携带
API Server 已经授权过的身份，没有 tenant、user 或 membership 字段。传输与服务端组合见
[Controller 运行时](../controller/local-runtime.zh.md)；本页只讲契约本身和它如何变更。

## Package 与接口

`ora.controller.v1` 定义 `ControllerService`，包含 `AcceptClone`、`ListOperations`、`GetOperation`。
语义与过渡期的 JSON clone 接口一致：接受只在意图持久提交后返回；同一请求身份、相同输入重传返回
原回执；没有终态的操作是 `pending` 而不是失败；未知执行返回 `NOT_FOUND` 而不是空结果。

失败以 gRPC 规范状态码作为主分类，并把 `ora.controller.v1.ErrorDetail` 作为 `google.rpc.Status`
的 detail 附上，调用方按任一方式分支都不需要解析消息文本：

| `ErrorCode` | gRPC 状态 | 含义 |
|---|---|---|
| `CONFLICT` | `ABORTED` | 身份或输入与持久记录冲突，重试同一调用不可能成功 |
| `INVALID_INPUT` | `INVALID_ARGUMENT` | 请求本身不合法 |
| `UNAVAILABLE` | `UNAVAILABLE` | 持久化暂时不可用，没有任何接受发生，可以重试 |
| `NOT_FOUND` | `NOT_FOUND` | 没有该执行身份的接受记录 |

## 生成代码

`crates/controller-proto`（`ora-controller-proto`）只放 `src/gen/` 下的 Rust 生成物，随仓库提交。
由 `buf` 用固定版本的远程插件（`neoeinstein-prost`、`neoeinstein-tonic`）生成，重新生成需要网络，
不需要本机 `protoc`。

- `task proto:lint`：按 `STANDARD` 规则 `buf lint`。
- `task proto:generate`：重新生成 `crates/controller-proto/src/gen`。
- `task proto:check`：重新生成并在有 diff 时失败；已纳入 `task lint:crates`。
- `task proto:breaking`：以 `PROTO_BASE_REF`（默认 `main`）为基线 `buf breaking`。`v1` 拒绝
  破坏性修改；新的主版本以并列的 `v2` package 表达。

Cloud 以本仓库的 git tag（`controller-proto/vX.Y.Z`）为输入、用自己的 `buf` 配置生成并提交 Go 包，
从不复制 `.proto`。

## 变更契约的流程

1. 修改 `proto/ora/controller/v1/*.proto`，运行 `task proto:lint` 与 `task proto:breaking`。
2. 运行 `task proto:generate`，把重新生成的 crate 与改动一起提交。
3. 合并后在该提交上打 tag `controller-proto/vX.Y.Z`。
4. Cloud 更新其 `buf.gen.yaml` 中的 tag，重新生成并编译。编译失败就是唯一需要的对齐信号，
   不做逐字段评审。

生成证明的是结构一致；行为一致由 Cloud 对真实 `ora-controller` 的集成测试保证，在契约稳定后登记。
