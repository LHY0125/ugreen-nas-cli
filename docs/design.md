# nascli 设计文档

> 状态：已评审通过，待实现
> 日期：2026-11-03
> 适用：绿联 UGOS Pro 的 OpenCLI 访问令牌接口

---

## 1. 背景与目标

绿联 UGOS Pro 提供「访问令牌」（`ugpoa_v1.*`），用于让第三方 AI Agent 调用 NAS 能力。
令牌背后是一套 **gRPC-over-HTTP** 接口（OpenCLI）：URL 里拼 gRPC 的 `full method`，
请求/响应体是 **protobuf 二进制**，鉴权用 `Authorization: Bearer`。

裸用这套接口对智能体极不友好：要手工编码 protobuf、要记住 socket-id 与方法名、
要处理自签名证书。本项目的目标是把这套接口封装成一个**智能体能安全使用的 CLI**。

### 目标

1. 提供 `nascli` 命令行工具，覆盖 OpenCLI 的语义化命令。
2. 提供配套的 Claude Code 技能，告诉智能体何时用、怎么用。
3. **默认只读**：写/删/执行类操作必须显式确认。
4. **完全通用**：不含任何个人环境信息，可直接开源。

### 非目标

- 不做 GUI。
- 不做 NAS 端部署（不修改 NAS 任何配置）。
- 不实现流式 RPC（见 §9）。
- 不替代绿联官方客户端。

---

## 2. 关键约束（来自实测）

这些是设计的前提，均由只读探测确认：


| 事实                                                      | 影响                                    |
| ----------------------------------------------------------- | ----------------------------------------- |
| 公共接口路径为 `/ugreen/opencli/v1/<socket-id><gRPC方法>` | URL 构造规则固定                        |
| 只接受 `Content-Type: application/x-protobuf`             | 必须自实现 protobuf 编解码              |
| 只认 `Authorization: Bearer`                              | 其他头/查询参数无效                     |
| 令牌管理接口走 `/ugreen/v1/opencli/...`，需登录会话       | 本工具不涉及令牌管理                    |
| 服务端为 gRPC，**一元方法可用，流式方法不可用**           | 部分能力需替代方案或标记不支持          |
| 官方 CLI 二进制内嵌完整 `FileDescriptorProto`             | schema 可自动提取，无需手工试字段号     |
| 官方 CLI 连接本机 Unix socket，root 专属                  | 官方 CLI 无法在客户端复用，仅作命名参考 |

---

## 3. 架构

单 crate，无 workspace。

```
nascli/
├── Cargo.toml
├── build.rs                   # 校验 spec/schema.json 存在
├── .gitignore                 # spec/ 在此
├── src/
│   ├── main.rs                # clap 分发
│   ├── config.rs              # 环境变量、地址、TLS
│   ├── wire.rs                # 通用 protobuf 编解码
│   ├── schema.rs              # 加载内嵌 schema，字段号 ↔ 名字
│   ├── client.rs              # HTTP POST + Bearer + 错误映射
│   └── commands/
│       ├── mod.rs             # 命令表
│       ├── storage.rs
│       ├── system.rs
│       ├── network.rs
│       ├── docker.rs
│       ├── files.rs
│       ├── apps.rs
│       ├── download.rs
│       └── users.rs
├── tools/extract_schema.py    # 从官方 CLI 二进制提取 schema
├── tests/
│   ├── wire.rs
│   ├── schema.rs
│   ├── commands.rs
│   └── fixtures/*.bin         # 脱敏后的响应样本
└── docs/design.md             # 本文档
```

### 数据流

```
clap 解析参数
  → 命令表查到 (socket-id, gRPC 方法, 字段映射)
  → wire 编码 protobuf
  → client POST（Bearer + application/x-protobuf）
  → wire 解码响应
  → schema 把字段号还原成名字
  → stdout 输出 JSON
```

### 三条边界

1. **CLI 不硬编码字段号** —— 全部来自 `spec/schema.json`，固件升级只需重跑提取器。
2. **编解码器通用** —— 一套 `encode`/`decode` 服务全部方法。
3. **命令表静态** —— 每个命令声明方法 + 参数映射，`--help` 可读、参数可校验。

### 依赖

`clap`、`reqwest`、`serde`、`serde_json`。不使用 `tonic`/`prost`（避免 proto 构建链）。

---

## 4. 命令表

文件类命令对齐官方 CLI 命名，降低学习成本。

### 文件（`fs`）

```
nascli fs stat   --path <p>
nascli fs ls     --path <p> [--page N] [--limit N] [--root-type <t>]
nascli fs find   --root-path <p> [--ext-names a,b] [--max-depth N]
nascli fs du     --path <p>                    # 不支持，见 §9
nascli fs cat    --path <p>
nascli fs md5sum --paths a,b
nascli fs sha256sum --paths a,b
nascli fs getfattr --path <p>
nascli fs recycle-status --path <p>
nascli fs path-perm --path <p>
nascli fs mkdir | touch | echo | rename | cpmv | del | restore | chtimes | modify-acl | share-mkdir
```

### 其他模块

```
nascli storage  disks | volumes | pools | smart
nascli system   identity | update-status | cpu | mem | net | disk | volume
                | overview | services | processes | ups | power | fan | led
                | beep | cpu-config | zram
nascli network  general | ifaces
nascli docker   containers | images | compose | logs | port-check | dir-check | usage
nascli apps     list | detail | status
nascli download tasks
nascli users    list | groups
```

### 发现与兜底

```
nascli list [--module <m>] [--read-only]     # 列出全部方法及其读写属性、是否支持
nascli call <socket-id> <method> [--field N:type=value]...
```

`call` 保证固件升级出现新方法时「永远能用」，不必等 CLI 更新。

---

## 5. schema 与编解码

### 5.1 提取器

`tools/extract_schema.py` 通过 SSH **只读**扫描 NAS 上官方 CLI 二进制，
解析内嵌的 `FileDescriptorProto`，产出 `spec/schema.json`：

```jsonc
{
  "messages": {
    "<fqn>": [ {"no": 3, "name": "path", "type": "string"}, ... ]
  },
  "enums": { "<fqn>": {"SAFE": 1} },
  "services": {
    "<Service 全名>": {
      "<Method>": {"in": "...", "out": "...", "server_streaming": false}
    }
  },
  "methods": { "<gRPC 方法>": {"socket": "...", "operation": "read", "role": "admin"} }
}
```

`spec/` 进 `.gitignore`；`build.rs` 找不到 `spec/schema.json` 时**编译失败**并提示先运行提取器。

### 5.2 编解码器

```rust
enum Value { Varint(u64), Bytes(Vec<u8>), Fixed32(u32), Fixed64(u64) }

fn encode(msg: &BTreeMap<u32, Vec<Value>>) -> Vec<u8>;
fn decode(raw: &[u8]) -> BTreeMap<u32, Vec<Value>>;
fn to_json(msg: &BTreeMap<u32, Vec<Value>>, schema: &Schema) -> serde_json::Value;
```

`to_json` 按 schema 还原字段名、递归展开嵌套消息、把枚举数字转成符号名。
**未知字段号保留为 `"field_<N>"`**，固件升级新增字段时不丢数据。

**repeated 字段的输出契约**：`Message` 的值是 `Vec<Value>`，同一字段号可出现多次。
渲染时按元素个数决定形状——**多元素输出 JSON 数组，单元素输出标量，空则跳过**。
因此消费方不能假设某个字段一定是数组或一定是标量（例如 `pools[0].disks` 是数组而
`pools[0].volumes` 是标量，取决于实际元素个数）。

> 已知限制：schema 目前不记录 `FieldDescriptorProto.label`，因此**无法区分 repeated 与单值**。
> 对 `repeated int32`/`repeated enum` 这类 **packed 编码**的标量，wire 上表现为单个 Bytes 字段，
> 当前会被当作 bytes 渲染（UTF-8 或 hex），而非解包成数组。

### 5.3 参数映射

命令只声明**字段名**，字段号由 schema 在运行时查出：

```rust
// fs ls：路径字段名为 "path"，可选 "page" / "limit"
let req_type = schema.request_type("/ugidl.filesystem.Filesystem/CategoryOrFileDirList")?;
let no = schema.field_no(&req_type, "path")?;
```

好处：固件升级即使调整字段号，只要字段名不变，CLI 无需改动。

`call` 兜底用 `--field 3:string=/volume1` 显式形式，绕过名字查找。

---

## 6. 配置与安全

### 6.1 配置（仅环境变量，无配置文件）


| 变量             | 默认       | 说明                                     |
| ------------------ | ------------ | ------------------------------------------ |
| `UGPOA_TOKEN`    | 无（必填） | 访问令牌                                 |
| `UGNAS_HOST`     | 无（必填） | NAS 地址；支持 IP / DDNS / UGREENlink ID |
| `UGNAS_PORT`     | `9443`     |                                          |
| `UGNAS_INSECURE` | `1`        | 接受自签名证书；`0` 为严格校验           |

优先级：命令行参数 > 环境变量 > 默认值。

### 6.2 安全模型


| 机制     | 行为                                                          |
| ---------- | --------------------------------------------------------------- |
| 读写分级 | `read` 直接执行；`write`/`delete`/`execute` 无 `--yes` 则拒绝 |
| 令牌脱敏 | 日志与错误中令牌一律显示为 `ugpoa_v1.****`                    |
| 不落盘   | 令牌只在内存，不写历史、不写临时文件                          |
| 只读发现 | `nascli list --read-only` 列出可安全使用的方法                |

拒绝时的输出：

```
error: 该操作会修改 NAS 状态，已被拒绝
  method:  /ugidl.filesystem.Filesystem/DelPath   [delete]
  将执行:  删除 /volume1/demo/tmp
  如确认执行，请重试并加上 --yes
```

### 6.3 开源脱敏要求

仓库公开，以下内容**不得**出现真实环境信息：


| 位置                   | 要求                                                           |
| ------------------------ | ---------------------------------------------------------------- |
| README 示例            | 使用 `nas.example.com` 占位                                    |
| `extract_schema.py`    | 通过 `--host` / `$UGNAS_HOST` 传入，无默认值                   |
| `tests/fixtures/*.bin` | **结构保留、值替换**：字段号/类型/嵌套不变，字符串内容换占位符 |
| 代码注释               | 使用「目标 NAS」泛称，不提具体型号                             |

---

## 7. 错误处理与输出契约

### 7.1 退出码


| 场景                         | 退出码 |
| ------------------------------ | -------- |
| 成功                         | 0      |
| 参数错误                     | 2      |
| 未授权（401）                | 3      |
| 无权限（Access not allowed） | 4      |
| 方法不支持（流式）           | 5      |
| 网络/证书错误                | 6      |
| 服务端错误                   | 7      |

### 7.2 输出

**stdout 永远是纯 JSON**；stderr 才是人类可读信息。

```jsonc
// 成功
{"ok": true, "method": "/storage.Storage/VolumeList", "data": { ... }}

// 失败
{"ok": false, "error": {"code": "ACCESS_DENIED", "message": "...", "grpc_code": 3}}
```

`--format table|pretty` 只影响人类可读性，不改变 `--format json` 的机器契约。

---

## 8. 测试


| 层级 | 内容                                          | 需要 NAS |
| ------ | ----------------------------------------------- | ---------- |
| 单元 | 编解码往返、schema 解析、参数映射、错误码映射 | ❌       |
| 快照 | 每个命令的请求字节 + 响应解码结果             | ❌       |
| 集成 | 真实调用，`#[ignore]` 标记，手动触发          | ✅       |

测试**不依赖 NAS 在线**。fixture 为脱敏后的真实响应。

---

## 9. 已知限制

### 9.1 流式方法不可用

OpenCLI 的 HTTP 代理只支持一元调用。以下方法为服务端流式，调用会返回
`cardinality violation`（`X-Grpc-Code: 13`）：


| 方法      | 影响      | 应对                                                 |
| ----------- | ----------- | ------------------------------------------------------ |
| `Readdir` | `fs ls`   | 改用一元的 `CategoryOrFileDirList`                   |
| `Find`    | `fs find` | 尝试 `Find` + `GetFindResult` 组合；不通则标记不支持 |
| `Du`      | `fs du`   | **标记不支持**，`--help` 中说明原因                  |

`nascli list` 明确标注哪些方法因流式不可用，避免智能体反复试错。

### 9.2 路径权限边界

令牌的访问范围等于账号本身有权访问的资源，不支持按文件夹单独限权。
卷根目录（如 `/volume1`）会被拒绝，需使用具体共享目录或用户目录。

### 9.3 请求限制

令牌默认限制为 600 次/分钟、最大并发 10。CLI 不做额外限流，由调用方注意。

### 9.4 `network ifaces` 恒返回空

`GetIfaceList` 的请求类型是 `common.SingleString`，CLI 发送空串，服务端因此返回空列表。
真正能列出网卡状态的 `GetInterfaceStatus` **未登记在 `openapi.json` 的 `methods` 中**，
`call` 会按未知方法（fail-closed）拒绝执行。

因此 `nascli network ifaces` 返回 `{"ok":true,"data":{}}` 且退出码 0——**这是「无数据」而非「无网卡」**。
需要网卡信息时请用 `nascli network general`（含 `ipv4_gateway.iface_name` 等）。

### 9.5 `system update-status` 恒返回空

请求类型为 `google.protobuf.Empty`，空响应在协议层无法区分「无可用更新」与「服务端未填充」。
CLI 侧无可做的判别，需上游补充字段。

---

## 10. 技能

位置：`D:\Code\.claude\skills\nascli\`（项目级，非公开仓库，可含个人环境信息）。

结构对齐现有 `nas-compose-intake`：

```markdown
---
name: nascli
description: 通过 nascli 查询绿联 NAS 的存储、系统指标、Docker、文件与应用状态……
---

# NAS CLI 使用指南

## 权限边界（硬约束）
- 默认只用只读命令
- 写/删/执行类命令必须经用户明确同意后才能加 --yes
- 不把令牌写进任何文件或命令历史

## 常用命令速查
（15-20 条高频命令表格）

## 发现能力
nascli list --read-only

## 不支持的能力
（流式方法清单及原因）
```

技能只写**高频命令**，冷门方法交给 `nascli list` 与 `--help` 发现，避免文档膨胀。

---

## 11. 构建与发布

```powershell
python tools\extract_schema.py --host <NAS>   # 生成 spec/schema.json
cargo build --release
cargo install --path .
```

`extract_schema.py` 通过 SSH 只读读取 NAS 上的官方 CLI 二进制，不修改任何配置。

### 固件升级后的同步

```
NAS 升级固件
  → 重跑 extract_schema.py
  → cargo build（schema 变化会被重新嵌入）
  → 跑快照测试，检查哪些方法的请求/响应结构变了
```

---

## 12. 决策记录


| 决策                           | 理由                                         |
| -------------------------------- | ---------------------------------------------- |
| 用 Rust 单 crate               | 单二进制、无运行时依赖；规模不需要 workspace |
| 通用编解码而非 prost 代码生成  | 101 个方法规模下，避免 proto 构建链的复杂度  |
| `spec/` 不入库，编译期强制检查 | 不公开上游 schema；同时保证构建可复现        |
| 命令名对齐官方 CLI             | 降低学习成本，语义一致                       |
| 保留 `call` 兜底               | 固件升级出现新方法时无需等 CLI 更新          |
| 流式方法标记不支持             | HTTP 代理限制，硬啃成本高收益低              |
| 令牌仅走环境变量               | 避免明文落盘                                 |
| 失败也输出 JSON 到 stdout      | 智能体统一解析 stdout                        |
| 公开仓库完全通用化             | 便于开源，避免环境信息泄露                   |

---

## 13. 交付物

```
nascli/                        # 公开仓库
├── Cargo.toml / build.rs / .gitignore
├── src/                       # 见 §3
├── tools/extract_schema.py
├── tests/ + fixtures/
├── docs/design.md
└── README.md

D:\Code\.claude\skills\nascli\ # 私有
├── SKILL.md
└── DECISIONS.md
```
