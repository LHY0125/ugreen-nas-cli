# nascli

绿联 UGOS Pro「访问令牌」（OpenCLI）的命令行客户端。默认只读，适合交给 AI 智能体安全使用。

## 它解决什么问题

UGOS Pro 的访问令牌背后是一套 **gRPC-over-HTTP** 接口（OpenCLI）：URL 里拼 gRPC 的 full method，请求体和响应体都是 **protobuf 二进制**，鉴权用 `Authorization: Bearer`。

直接用 `curl` 调用并不现实：要手工编码 protobuf、记住每个方法的 socket-id 与字段号、处理自签名证书，而且一次误操作就可能改动 NAS 状态。nascli 把这套接口封装成语义化命令，并强制「读直接跑、写要确认」。

主要特性：

- 语义化命令域：`storage` / `system` / `network` / `docker` / `apps` / `users` / `download` / `fs`
- **默认只读**：写、删、执行类操作必须显式加 `--yes`
- stdout 只输出 JSON，退出码稳定，便于程序化处理
- 令牌只从环境变量读取，不落盘、不写日志
- 字段号运行时从内嵌 schema 查询，固件升级调整字段号也不影响命令

## 环境要求

- Rust 1.88+
- Python 3.10+（仅生成 schema 时需要）
- 到 NAS 的 SSH 访问（仅生成 schema 时需要）
- NAS 上已创建访问令牌：**个人中心 → 本地账号 → 访问令牌**

## 安装

`spec/schema.json` 含上游接口契约，**不入库**。新克隆的仓库必须先跑一次提取器生成它，否则 `build.rs` 会让编译直接失败。

```bash
# 1. 生成 schema（只读：通过 SSH 读取 NAS 上的官方二进制，不修改任何配置）
python tools/extract_schema.py --host nas.example.com

# 2. 构建并安装
cargo build --release
cargo install --path .
```

如果 SSH 用户名与 `~/.ssh/config` 中的不同，加 `--ssh-user <user>`。提取器需要 NAS 上有 `python3`。

## 配置

只认环境变量，不写配置文件：

| 变量 | 必填 | 默认 | 说明 |
| --- | --- | --- | --- |
| `UGNAS_HOST` | 是 | — | NAS 地址，支持 IP / DDNS / UGREENlink ID |
| `UGPOA_TOKEN` | 是 | — | 访问令牌，形如 `ugpoa_v1.*` |
| `UGNAS_PORT` | 否 | `9443` | OpenCLI HTTPS 端口 |
| `UGNAS_INSECURE` | 否 | `1` | 接受自签名证书；仅 `"0"` 表示校验证书 |

```powershell
# PowerShell
$env:UGNAS_HOST = "nas.example.com"
$env:UGPOA_TOKEN = "ugpoa_v1...."
```

```bash
# Bash / Zsh
export UGNAS_HOST="nas.example.com"
export UGPOA_TOKEN="ugpoa_v1...."
```

## 用法

```bash
# 发现能力
nascli list --read-only                 # 列出所有只读方法
nascli list --module storage            # 按套接字名或方法路径过滤

# 只读查询
nascli storage volumes                  # 存储卷与容量
nascli system cpu                       # CPU 使用率
nascli docker containers                # 容器列表
nascli fs ls --path /volume1/demo --limit 20
nascli fs stat --path /volume1/demo

# 写类操作：必须显式 --yes
nascli fs mkdir --path /volume1/demo/new --yes
nascli fs del --path /volume1/demo/old --yes   # 默认移入回收站，不永久删除

# 兜底：直接调用任意 OpenCLI 方法
nascli call ugreen.storage.grpc /storage.Storage/VolumeList
nascli call <socket> <method> --field 3:string=/volume1/demo
```

### 输出契约

stdout **只输出一行 JSON**，人类可读信息一律走 stderr。

```jsonc
{"ok": true,  "method": "/storage.Storage/VolumeList", "data": {...}}
{"ok": false, "error": {"code": "UNAUTHORIZED", "message": "令牌无效或已过期", "exit_code": 3}}
```

请求发出前的失败（参数错误、流式方法不支持）只写 stderr，stdout 为空。

### 退出码

| 退出码 | 含义 |
| --- | --- |
| `0` | 成功 |
| `2` | 参数错误（含缺少 `--yes`、缺少目标参数） |
| `3` | 未授权（令牌无效或过期） |
| `4` | 无权限 |
| `5` | 方法不支持（服务端流式 RPC） |
| `6` | 网络错误 |
| `7` | 服务端错误 |

## 安全模型

- **默认只读**：`read` 类方法直接执行；`write` / `delete` / `execute` 类必须加 `--yes`，否则在建立连接前就以退出码 2 拒绝。
- **未知方法按写处理**：`nascli call` 调用 schema 之外的方法时无法判断读写属性，按 fail-closed 要求 `--yes`。
- **目标参数闸门**：需要目标却发出空请求的动作一律拒绝，避免范围未限定的破坏。
- **令牌不落盘**：只从 `UGPOA_TOKEN` 读取，不写配置文件、不写日志；`Debug` 输出经过脱敏，错误信息另有输出侧兜底替换。
- **不修改 NAS 配置**：schema 提取器仅通过 SSH 只读读取官方二进制。

## 已知限制

- **流式方法不支持**：服务端流式 RPC（`fs du`、`fs find`、底层 `Readdir`）不被 OpenCLI 的 HTTP 接口支持，调用返回退出码 `5`。`fs ls` 已改用一元方法 `CategoryOrFileDirList`。
- **repeated 字段的输出形状**：同一字段号只有一个值时输出标量，多个值时输出 JSON 数组；packed 编码的 repeated 标量会先解包再输出数组。因此不能假设 repeated 字段永远是数组。
- **需要目标参数的动作尚未参数化**：`docker` 的 `logs` / `port-check` / `dir-check` / `start` / `stop` / `restart` / `pull` / `compose-*` / `rebuild`，以及 `fs` 的 `rename` / `cpmv` / `echo` / `md5sum` / `sha256sum` / `chtimes` / `modify-acl` / `share-mkdir`，当前不提供专用参数入口，会 fail-closed 并提示改用 `nascli call --field ...`。
- **schema 与固件版本绑定**：`spec/schema.json` 由目标 NAS 的固件生成。固件升级后如新增或调整方法，需重新运行提取器。
- **仅支持一元 RPC**：不实现服务端流式或双向流式调用。

## 许可

MIT
