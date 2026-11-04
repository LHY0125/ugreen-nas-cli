# nascli

绿联 UGOS Pro「访问令牌」（OpenCLI）的命令行客户端。默认只读，适合交给 AI 智能体安全使用。

完整命令查询表见 [`docs/CLI-QUERY.md`](docs/CLI-QUERY.md)。

## 它解决什么问题

UGOS Pro 的访问令牌背后是一套 **gRPC-over-HTTP** 接口（OpenCLI）：URL 里拼 gRPC 的 full method，请求体和响应体都是 **protobuf 二进制**，鉴权用 `Authorization: Bearer`。

直接用 `curl` 调用并不现实：要手工编码 protobuf、记住每个方法的 socket-id 与字段号、处理自签名证书，而且一次误操作就可能改动 NAS 状态。nascli 把这套接口封装成语义化命令，并强制「读直接跑、写要确认」。

主要特性：

- 语义化命令域：`storage` / `system` / `network` / `docker` / `apps` / `users` / `download` / `fs`
- **默认只读**：写、删、执行类操作必须显式加 `--yes`
- stdout 只输出 JSON，退出码稳定，便于程序化处理
- 令牌只从 TOML 配置读取，不写日志；`Debug` 与错误输出会脱敏
- 字段号运行时从内嵌 schema 查询，固件升级调整字段号也不影响命令

## 环境要求

- 运行：无需额外依赖，单个可执行文件
- 从源码构建：Rust 1.88+
- 重新提取 schema（仅固件升级后需要）：Python 3.10+，且需要到 NAS 的 SSH 访问
  - NAS 上需要有 `python3`（提取器在 NAS 本地运行）
  - SSH 用户名与 `~/.ssh/config` 不一致时，提取器加 `--ssh-user <user>`
- NAS 上已创建访问令牌：**个人中心 → 本地账号 → 访问令牌**

## 安装

### Scoop（Windows 推荐）

```powershell
scoop bucket add lhy https://github.com/LHY0125/scoop-bucket
scoop install nascli
```

### 从源码构建

`schema.json` 已随仓库提供，clone 即可编译：

```bash
git clone https://github.com/LHY0125/ugreen-nas-cli.git
cd ugreen-nas-cli
cargo build --release
cargo install --path .
```

### 预编译二进制

从 [Releases](https://github.com/LHY0125/ugreen-nas-cli/releases) 下载对应平台的文件：

| 平台 | 文件 |
| --- | --- |
| Windows x64 | `nascli_<version>_x64.exe` |
| Linux x64 | `nascli_<version>_linux_x64.tar.gz` |
| macOS x64 | `nascli_<version>_macos_x64.tar.gz` |

## 配置

首次运行 `nascli config init` 会创建配置文件：

| 平台 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\nascli\config.toml` |
| Linux | `$XDG_CONFIG_HOME/nascli/config.toml`，未设置则 `~/.config/nascli/config.toml` |
| macOS | `~/Library/Application Support/nascli/config.toml` |

### 覆盖路径

优先级从高到低：

```bash
nascli --config /path/to/config.toml storage volumes   # 1. 显式参数
UGNAS_CONFIG=/path/to/config.toml nascli storage volumes  # 2. 环境变量
nascli storage volumes                                  # 3. 平台默认目录
```

正常安装环境下，当前目录的 `config.toml` **不会**被读取；当平台配置目录无法确定时（Windows 缺 `APPDATA`、macOS 缺 `HOME`、Linux 的 `XDG_CONFIG_HOME` 与 `HOME` 都缺），才退回当前目录的 `config.toml`。在仓库内开发时请用上面的参数或环境变量指定。

### 字段

```toml
host = "nas.example.com"   # 必填：主机名或 IP，不要带协议/路径/端口
port = 9443                # 可选，默认 9443
token = "ugpoa_v1...."     # 必填：访问令牌
insecure = true            # 可选，默认 true（接受自签名证书）
```

- `host` / `token` 必填；
- `port` 默认 `9443`，`insecure` 默认 `true`；
- 未知键会直接报错，避免拼写错误被静默忽略；
- 示例文件见 [`config.example.toml`](config.example.toml)。

```bash
# 首次使用：交互式创建配置
nascli config init

# 修改单项 / 交互式修改 / 查看 / 查看路径
nascli config set host nas.example.com
nascli config set port 9443
nascli config set token          # 不带 value 时提示输入，避免进入命令历史
nascli config set insecure false
nascli config edit
nascli config show               # token 自动脱敏
nascli config path
```

首次直接运行需要配置的命令时，如果当前是交互式终端且配置文件不存在，CLI 会自动进入初始化向导；非交互环境（例如 AI 工具）请先执行 `nascli config init`。

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
nascli fs du --path /volume1/demo          # 大目录可能因流式限制返回 5
nascli fs md5sum --path /volume1/demo/file.txt

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


| 退出码 | 含义                                     |
| -------- | ------------------------------------------ |
| `0`    | 成功                                     |
| `2`    | 参数错误（含缺少 `--yes`、缺少目标参数） |
| `3`    | 未授权（令牌无效或过期）                 |
| `4`    | 无权限                                   |
| `5`    | 方法不支持（服务端流式 RPC）             |
| `6`    | 网络错误                                 |
| `7`    | 服务端错误                               |

## 安全模型

- **默认只读**：`read` 类方法直接执行；`write` / `delete` / `execute` 类必须加 `--yes`，否则在建立连接前就以退出码 2 拒绝。
- **上游误标修正**：少数会改状态的方法在上游权限表里被标成 `read`，本地强制写名单会把它们提升为写操作；`list --read-only` 也会排除。`list --read-only` 仍只是“上游声明 + 本地已知修正”，不是绝对安全边界。
- **未知方法按写处理**：`nascli call` 调用 schema 之外的方法时无法判断读写属性，按 fail-closed 要求 `--yes`。
- **目标参数闸门**：需要目标却发出空请求的动作一律拒绝，避免范围未限定的破坏。
- **令牌在 TOML 配置中**：按当前方案，`config.toml` 明文保存令牌；CLI 不写日志、不写其他缓存，`Debug` 输出经过脱敏，错误信息另有输出侧兜底替换。
- **不修改 NAS 配置**：schema 提取器仅通过 SSH 只读读取官方二进制。

## 已知限制

- **部分流式方法不可用**：`Find` 是服务端流式，`fs find` 本地拒绝并返回退出码 `5`；`Readdir` 由 `fs ls` 改用一元 `CategoryOrFileDirList`。`Du` 对部分目录可返回单条结果，`fs du` 会实际调用，大目录若触发流式限制再返回 `5`。
- **repeated 字段的输出形状**：同一字段号只有一个值时输出标量，多个值时输出 JSON 数组；packed 编码的 repeated 标量会先解包再输出数组。因此不能假设 repeated 字段永远是数组。
- **需要目标参数的动作尚未参数化**：`docker` 的 `logs` / `port-check` / `dir-check` / `start` / `stop` / `restart` / `pull` / `compose-*` / `rebuild`，以及 `fs` 的 `rename` / `cpmv` / `echo` / `chtimes` / `modify-acl` / `share-mkdir`，当前不提供专用参数入口，会 fail-closed 并提示改用 `nascli call --field ...`。
- **`path-perm` 不可用**：当前固件没有登记 `GetPathPermission`，`fs path-perm` 会本地拒绝并返回退出码 `5`。
- **schema 与固件版本绑定**：`spec/schema.json` 由目标 NAS 的固件生成。固件升级后如新增或调整方法，需重新运行提取器。
- **仅支持一元 RPC**：不实现服务端流式或双向流式调用。

## 许可

MIT
