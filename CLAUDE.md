# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## NAS 只读原则（硬约束）

**不得修改目标 NAS 的任何配置。** 凡涉及变更 NAS 状态的操作——改设置、装/卸应用、启停容器、增删改用户、写文件、删文件、改网络——**必须先取得用户书面同意**后才能执行。

本项目自身的设计与此一致：`tools/extract_schema.py` 仅通过 SSH **读取** NAS 上的官方二进制（`ssh <host> python3 -` 读文件），不写远端；CLI 的写类命令默认被 `--yes` 闸门拦住。

## 项目定位

`nascli` 是绿联 UGOS Pro「访问令牌」（`ugpoa_v1.*`）的 Rust 命令行客户端。令牌背后是 **OpenCLI**——一套 gRPC-over-HTTP 接口：URL 里拼 gRPC full method，请求/响应体是 protobuf 二进制，鉴权用 `Authorization: Bearer`。

项目的核心价值不是「能调用」，而是**让 AI 智能体安全地调用**：默认只读、写操作需显式确认、令牌全程脱敏、JSON 输出可程序化解析。

完整设计与决策记录见 `docs/design.md`,实现计划见 `docs/plans/`。
面向人/AI 的命令查询表见 `docs/CLI-QUERY.md`。

## 常用命令

```bash
# 构建（要求 spec/schema.json 存在，否则 build.rs 会中止并提示）
cargo build
cargo build --release

# 测试
cargo test                      # 全量
cargo test --test wire          # 单个集成测试文件
cargo test roundtrip_varint     # 按名称过滤单个用例

# 代码质量（提交前必跑）
cargo fmt --check
cargo clippy --all-targets -- -D warnings

# Python 提取器测试
cd tools && python -m pytest tests/test_extract.py -v

# 生成 schema（需到 NAS 的 SSH 访问；只读）
python tools/extract_schema.py --host <NAS 地址>

# 配置（只读一个 TOML 文件；默认当前目录 config.toml）
nascli config init              # 交互式创建；--force 覆盖
nascli config edit              # 逐项修改
nascli config set host <host>
nascli config set port 9443
nascli config set token         # 不带 value 时提示输入
nascli config show              # token 脱敏
nascli config path
```

## 架构：四层，数据单向流动

```
main.rs (clap)
  → commands/*  命令表：把子命令映射到 (socket-id, gRPC 方法, operation)
  → schema.rs   字段名 ↔ 字段号（从内嵌 schema.json 查）
  → wire.rs     protobuf 编解码
  → client.rs   HTTP POST + Bearer + 错误分类
```

**关键设计：CLI 不硬编码任何字段号。** 命令只声明字段**名**（如 `"path"`），字段号由 `schema.field_no(request_type, "path")` 在运行时查出。固件升级即使调整字段号，只要字段名不变，CLI 代码无需改动——只需重跑提取器。

### 各模块职责


| 模块        | 职责                                                                     | 关键接口                                                               |
| ------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `wire.rs`   | 通用 protobuf 编解码，不含任何业务语义                                   | `Value`、`Message`、`encode`、`decode`                                 |
| `schema.rs` | 加载 `spec/schema.json`，字段号→名字、枚举→符号、嵌套消息递归展开      | `Schema::from_json_str`、`request_type`、`field_no`、`message_to_json` |
| `config.rs` | 仅从 TOML 文件读配置；令牌脱敏                                           | `Config::from_file`、`redact`                                          |
| `client.rs` | HTTP 传输 + 错误分类成稳定退出码                                         | `Client::call`、`classify`、`ApiError::exit_code`                      |
| `commands/` | 每个子模块一个域（storage/system/docker/fs/apps/...）；命令表 + 参数映射 | `resolve(action)` 返回 `(socket, method, operation)`                   |

### schema 是构建期嵌入的

`spec/schema.json` **不入库**（含上游接口契约，`.gitignore` 已排除）。`build.rs` 在它缺失时**让编译失败**并提示先跑提取器；`src/main.rs` 用 `include_str!` 嵌入。因此新克隆的仓库**必须先跑一次 `tools/extract_schema.py`** 才能编译。

### 提取器为什么自己解析二进制

`schema.json` 由 `tools/extract_schema.py` 从绿联官方 CLI 二进制里提取内嵌的 `FileDescriptorProto` 生成。两个不显然但关键的实现约束：

1. **描述符在二进制里没有长度前缀**，解析必须限定边界，否则会越界读到相邻数据。当前用「按 `FileDescriptorProto` 字段表校验 wire type」+「字段出现顺序必须符合 `descriptor.proto` 的**声明顺序**」双重判据确定终点。注意是声明顺序而非字段号大小——`public_dependency`(10) / `weak_dependency`(11) 声明在 `message_type`(4) 之前，按字段号排序会误截断 `import public` 的文件。
2. **命中必须自证**：解析出的 `name` 必须等于目标名，且描述符必须含 `package`/`message_type`/`enum_type`/`service`/`syntax` 之一——否则视为假阳性继续向后搜索。

`openapi.json` 的位置**有两处**：`/ugreen/@appstore/*/`（6 个系统应用）与 `/volume1/@appstore/*/`（Docker、下载中心）。只扫前者会漏掉 26 个方法。

## 对外契约（改动时不可破坏）

### 退出码

`0` 成功 / `2` 参数错误 / `3` 未授权 / `4` 无权限 / `5` 方法不支持 / `6` 网络错误 / `7` 服务端错误。

映射定义在 `ApiError::exit_code()`。这是智能体判断失败原因的依据，属于公开契约。

### 输出

**stdout 只输出 JSON**，人类可读信息一律走 stderr。

```jsonc
{"ok": true,  "method": "/storage.Storage/VolumeList", "data": {...}}
{"ok": false, "error": {"code": "...", "message": "...", "exit_code": 3}}
```

**API 层失败**（服务端返回错误）也输出 JSON 到 stdout，便于统一解析。

**请求发出前的失败**（退出码 2 的参数错误、5 的流式不支持）只写 stderr，stdout 为空——此时没有 API 响应可序列化。

### 配置（仅 TOML 文件）


CLI 只读取一个 TOML 文件；默认当前目录的 `config.toml`，可用全局参数
`--config <path>` 指定。没有环境变量兜底，也没有第二配置源。


| 字段       | 默认   | 说明                              |
| ------------ | -------- | ----------------------------------- |
| `host`     | **必填** | 主机名或 IP，不带协议/路径/端口  |
| `token`    | **必填** | 访问令牌                          |
| `port`     | `9443` | OpenCLI HTTPS 端口                |
| `insecure` | `true` | 接受自签名证书；`false` 为严格校验 |

### 令牌脱敏

`Config` **手写**了 `Debug`（不是 `derive`），令牌字段走 `redact()`；`ApiError` 经 `scrub()` 做输出侧兜底替换。新增持有敏感信息的字段时，必须同步在 `Config` 的 `Debug` impl 中脱敏。

## 已知限制

服务端为**流式**的方法（`Readdir`、`Find`、`Du`）在返回多条消息时会触发
`cardinality violation`，CLI 映射为退出码 `5`。

`fs ls` 因此改用**一元**的 `CategoryOrFileDirList`；`fs find` 直接报不支持；
`fs du` 会实际调用 `Du`，只有大目录触发流式限制时才返回 `5`。

上游权限表把少量会改状态的方法标成 `read`（搜索任务/查找任务类）。维护
`FORCED_WRITE_METHODS` 时，必须同步更新 `list --read-only` 的过滤行为与测试。

## 仓库约定

- 单 crate，`edition = "2021"`，不使用 workspace。
- **git commit 信息用中文**，格式 `<类型>：<描述>`。
- 不得提交任何真实环境信息（主机名、IP、序列号、用户名、邮箱、真实路径）。示例统一用 `nas.example.com` / `/volume1/demo`。
- `spec/` 的产物与 `docs/NAS-UGOS-OpenCLI-API.md` 均不入库。
- 不使用 `tonic` / `prost`（避免 proto 构建链，代价是自实现编解码）。
