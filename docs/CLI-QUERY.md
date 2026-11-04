# nascli 命令查询表

> 面向人读，也面向 AI 读。所有示例使用占位符，不包含真实环境信息。

## 1. 配置文件

CLI **只读取一个 TOML 文件**，不再读取环境变量。默认路径是当前目录的
`config.toml`，也可以用全局参数 `--config <path>` 指定。

```toml
host = "nas.example.com"   # 必填：主机名或 IP，不要带协议/路径/端口
port = 9443                # 可选，默认 9443
token = "ugpoa_v1...."     # 必填：访问令牌
insecure = true            # 可选，默认 true（接受自签名证书）
```

| 字段 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `host` | 是 | — | NAS 主机名或 IP；不要带协议、路径或端口 |
| `token` | 是 | — | 访问令牌，形如 `ugpoa_v1.*` |
| `port` | 否 | `9443` | OpenCLI HTTPS 端口 |
| `insecure` | 否 | `true` | 接受自签名证书；`false` 表示严格校验 |

```bash
nascli --config /path/to/config.toml storage volumes
```

> 未知键会直接报错；`host` / `token` 缺失或为空也会报错。`insecure = true` 时 CLI 会向 stderr 打印中间人风险警告。

### 配置命令

```bash
nascli config init              # 交互式创建；--force 覆盖
nascli config show              # 查看配置，token 脱敏
nascli config set host nas.example.com
nascli config set port 9443
nascli config set token         # 不带 value 时提示输入
nascli config set insecure false
nascli config edit              # 逐项修改，回车保持当前值
nascli config path              # 显示实际配置文件路径
```

首次运行需要配置的命令时，如果当前是交互式终端且配置文件不存在，CLI 会自动进入初始化向导；非交互环境请先执行 `nascli config init`。

---

## 2. 安全边界

- **默认只读**：schema 标记为 `read` 的操作直接执行。
- **写 / 删 / 执行**：必须取得用户明确同意，并加 `--yes`；否则在建立连接前退出码 `2`。
- **上游误标修正**：以下方法在上游权限表里是 `read`，但会改状态，CLI 本地强制按写处理，并且**不会出现在 `list --read-only`**：
  - `/search.Search/CancelTitleTask`
  - `/search.Search/CreateTitleStreamTask`
  - `/search.Search/RemoveTitleTask`
  - `/ugidl.filesystem.Filesystem/DeleteFindTask`
  - `/filemgr.Filemgr/DownloadFile`
- **`list --read-only` 不是绝对安全边界**：它只代表“schema 声明只读 + 当前本地修正”，新固件仍可能引入新的误标。写操作永远要用户确认。
- **令牌不落盘**：不要写进文件、脚本、命令历史或报告；示例只写变量名。
- **不修改 NAS 配置**：本工具只封装 OpenCLI 调用；schema 提取器也只读。
- **`fs del` 默认进回收站**：CLI 显式发送 `forever=false`；类型化命令没有永久删除入口。

---

## 3. 输出与退出码

### stdout

成功：

```json
{"ok": true, "method": "/storage.Storage/VolumeList", "data": { ... }}
```

API 失败（服务端已返回响应）：

```json
{"ok": false, "error": {"code": "ACCESS_DENIED", "message": "...", "exit_code": 4}}
```

本地闸门失败（参数错误、缺 `--yes`、缺目标、配置文件缺失/解析失败、`fs find` / `fs path-perm`）**只写 stderr，stdout 为空**。

### 退出码

| 退出码 | 含义 |
|---|---|
| `0` | 成功 |
| `2` | 参数错误 / 写操作未加 `--yes` / 缺目标参数 / 配置文件缺失或解析失败；服务端 400 也是 2 |
| `3` | 未授权（401 或 gRPC 16） |
| `4` | 无权限（403、gRPC 7 或 `Access not allowed`） |
| `5` | 方法不支持（本地拒绝的 `find` / `path-perm`，或服务端流式限制） |
| `6` | 网络错误（连不上 NAS、端口、证书、超时） |
| `7` | 服务端错误（含响应 protobuf 解析失败、固件未注册方法） |

### 输出形状注意

- repeated 字段只有 1 个元素时输出**标量**，多个元素时输出**数组**；解析时必须两种都兼容。
- packed repeated 标量（整数列表等）总是输出数组。
- `storage smart` 的 `data.value` 是 **JSON 字符串**，需要二次解析。
- `fs cat` 的 `data.data` 是文件内容（UTF-8 文本按字符串输出，非 UTF-8 按十六进制字符串输出）。
- 响应体超过 64 MiB 时直接返回退出码 `7`。

---

## 4. 常用场景速查

| 需求 | 命令 |
|---|---|
| 存储卷与容量 | `nascli storage volumes` |
| 物理磁盘 | `nascli storage disks` |
| SMART 健康 | `nascli storage smart` |
| CPU 使用率/温度 | `nascli system cpu` |
| 内存 | `nascli system mem` |
| 网络吞吐 | `nascli system net` |
| 磁盘指标 | `nascli system disk` |
| 运行总览 | `nascli system overview` |
| 系统版本与型号 | `nascli system identity` |
| 进程占用 | `nascli system processes` |
| 服务资源 | `nascli system services` |
| UPS | `nascli system ups` |
| 风扇/电源/LED/蜂鸣器 | `nascli system fan` / `power` / `led` / `beep` |
| 网络配置（网关/DNS） | `nascli network general` |
| 容器列表 | `nascli docker containers` |
| 本地镜像 | `nascli docker images` |
| Compose 项目 | `nascli docker compose` |
| 容器日志 | 用 `nascli call .../GetContainerLogs --field 1:string=<容器ID>` |
| 已安装应用 | `nascli apps list` |
| 用户 / 用户组 | `nascli users list` / `groups` |
| 下载任务 | `nascli download tasks` |
| 列目录 | `nascli fs ls --path /volume1/demo --limit 50` |
| 文件属性 | `nascli fs stat --path /volume1/demo` |
| 目录容量 | `nascli fs du --path /volume1/demo` |
| 读取文本文件 | `nascli fs cat --path /volume1/demo/file.txt` |
| 文件 MD5 / SHA256 | `nascli fs md5sum --path /volume1/demo/file.txt` / `fs sha256sum` |
| 回收站状态 | `nascli fs recycle-status --path /volume1/demo` |
| 新建目录 / 文件 | `nascli fs mkdir --path /volume1/demo/new --yes` / `fs touch` |
| 删除到回收站 | `nascli fs del --path /volume1/demo/old --yes` |
| 从回收站恢复 | `nascli fs restore --path /volume1/demo/old --yes` |

---

## 5. 子命令查询表

### 5.1 `list`

```bash
nascli list
nascli list --read-only
nascli list --module storage
nascli list --module docker
```

| 选项 | 说明 |
|---|---|
| `--module <子串>` | 按套接字名或方法路径过滤 |
| `--read-only` | 只列有效只读方法（排除本地强制写名单） |

每行字段：

```json
{
  "method": "/storage.Storage/VolumeList",
  "socket": "ugreen.storage.grpc",
  "operation": "read",
  "declared_operation": "read",
  "forced_write": false,
  "role": "admin",
  "server_streaming": false,
  "supported": true
}
```

### 5.2 `storage`

```bash
nascli storage <ACTION>
```

| 动作 | gRPC 方法 | 说明 |
|---|---|---|
| `disks` | `/storage.Storage/DiskList` | 物理磁盘 |
| `volumes` | `/storage.Storage/VolumeList` | 存储卷与容量 |
| `pools` | `/storage.Storage/PoolList` | 存储池 |
| `smart` | `/storage.Storage/AllDiskSmartInfo` | SMART；`data.value` 是 JSON 字符串 |

### 5.3 `system`

```bash
nascli system <ACTION>
```

| 动作 | gRPC 方法 |
|---|---|
| `identity` | `/ctlserv.Internal/GetOsIdentity` |
| `update-status` | `/ctlserv.Internal/GetOsUpdateStatus`（恒空，固件未提供） |
| `cpu` | `/taskserv.TaskManage/GetCpuMetric` |
| `mem` | `/taskserv.TaskManage/GetMemMetric` |
| `net` | `/taskserv.TaskManage/GetNetTraffic` |
| `disk` | `/taskserv.TaskManage/GetDiskMetric` |
| `volume` | `/taskserv.TaskManage/GetVolumeMetric` |
| `overview` | `/taskserv.TaskManage/GetRunOverview` |
| `services` | `/taskserv.TaskManage/GetServiceResource` |
| `processes` | `/taskserv.TaskManage/GetProcessResource` |
| `ups` | `/ctlserv.Internal/GetUpsConfig` |
| `power` | `/ctlserv.Hardware/GetPowerConfig` |
| `fan` | `/ctlserv.Hardware/GetFanConfig` |
| `led` | `/ctlserv.Hardware/GetLedConfig` |
| `beep` | `/ctlserv.Hardware/GetBeepConfig` |
| `cpu-config` | `/ctlserv.Hardware/GetCpuConfig` |
| `zram` | `/ctlserv.Hardware/GetZramConfig` |

### 5.4 `network`

```bash
nascli network <ACTION>
```

| 动作 | gRPC 方法 | 说明 |
|---|---|---|
| `general` | `/ctlserv.Network/GetGeneralConfig` | 网关、DNS、网卡名等 |
| `ifaces` | `/ctlserv.Network/GetIfaceList` | 当前固件恒返回空，改用 `general` |

### 5.5 `docker`

```bash
nascli docker <ACTION> [--yes]
```

| 动作 | gRPC 方法 | 类型 | 目标参数 |
|---|---|---|---|
| `containers` | `/docker.Docker/ContainerList` | read | 无 |
| `images` | `/docker.Docker/ShowLocalImage` | read | 无 |
| `compose` | `/docker.Docker/GetComposeProjectList` | read | 无 |
| `usage` | `/docker.Docker/GetAppUsage` | read | 无 |
| `logs` | `/docker.Docker/GetContainerLogs` | read | 容器 ID |
| `port-check` | `/docker.Docker/CheckPortOccupiedByContainer` | read | 端口 |
| `dir-check` | `/docker.Docker/CheckDirUsedByContainer` | read | 目录 |
| `pull` | `/docker.Docker/PullImage` | write | 镜像引用 |
| `start` | `/docker.Docker/StartContainer` | execute | 容器 ID |
| `stop` | `/docker.Docker/StopContainer` | execute | 容器 ID |
| `restart` | `/docker.Docker/RestartContainer` | execute | 容器 ID |
| `compose-start` | `/docker.Docker/StartComposeProject` | execute | 项目名 |
| `compose-stop` | `/docker.Docker/StopComposeProject` | execute | 项目名 |
| `compose-restart` | `/docker.Docker/RestartComposeProject` | execute | 项目名 |
| `compose-delete` | `/docker.Docker/DeleteComposeProject` | delete | 项目名 |
| `rebuild` | `/docker.Docker/RebuildContainerApp` | execute | 应用 ID |

> 需要目标参数的动作在类型化子命令里会 fail-closed（退出码 2），并提示改用 `nascli call`；写类动作同时需要 `--yes`。

### 5.6 `apps` / `users` / `download`

```bash
nascli apps <list|detail|status>
nascli users <list|groups>
nascli download tasks
```

| 命令 | gRPC 方法 | 说明 |
|---|---|---|
| `apps list` | `/app.App/DetailList` | 已安装应用 |
| `apps detail` | `/app.App/GetAppDetail` | 需要应用 ID → 用 `call` |
| `apps status` | `/app.App/GetAppStatus` | 需要应用 ID → 用 `call`；字段 1 是 repeated `ids` |
| `users list` | `/ctlserv.Internal/GetUgreenUserList` | 用户列表 |
| `users groups` | `/ctlserv.Internal/GetCreatedGroupList` | 用户组 |
| `download tasks` | `/download.DownloadService/BatchGetDownloadTask` | 下载任务 |

### 5.7 `fs`

```bash
nascli fs <ACTION> [--path <路径>] [--page N] [--limit N] [--yes]
```

| 动作 | gRPC 方法 | 类型 | 参数 | 说明 |
|---|---|---|---|---|
| `stat` | `/ugidl.filesystem.Filesystem/Stat` | read | `--path` | 属性 |
| `ls` | `/ugidl.filesystem.Filesystem/CategoryOrFileDirList` | read | `--path` `--page` `--limit` | 列目录 |
| `cat` | `/ugidl.filesystem.Filesystem/ReadFile` | read | `--path` | 读文件；最大 64 MiB |
| `getfattr` | `/ugidl.filesystem.Filesystem/Getfattr` | read | `--path` | 扩展属性 |
| `recycle-status` | `/ugidl.filesystem.Filesystem/GetRecycleStatus` | read | `--path` | 回收站状态 |
| `md5sum` | `/ugidl.filesystem.Filesystem/Md5sum` | read | `--path` | 映射到请求字段 `paths` |
| `sha256sum` | `/ugidl.filesystem.Filesystem/Sha256sum` | read | `--path` | 映射到请求字段 `paths` |
| `du` | `/ugidl.filesystem.Filesystem/Du` | read | `--path` | 大目录可能触发流式限制，退出 5 |
| `mkdir` | `/ugidl.filesystem.Filesystem/CreateDir` | write | `--path` + `--yes` | 新建目录 |
| `touch` | `/ugidl.filesystem.Filesystem/CreateFile` | write | `--path` + `--yes` | 新建空文件 |
| `del` | `/ugidl.filesystem.Filesystem/DelPath` | delete | `--path` + `--yes` | **默认移入回收站** |
| `restore` | `/ugidl.filesystem.Filesystem/RecoverFromRecycle` | write | `--path` + `--yes` | 从回收站恢复 |
| `rename` | `/ugidl.filesystem.Filesystem/Rename` | write | 用 `call` | 目录 + 旧名 + 新名 |
| `cpmv` | `/ugidl.filesystem.Filesystem/CpMovePath` | write | 用 `call` | 复制/移动，多字段 |
| `echo` | `/ugidl.filesystem.Filesystem/WriteFile` | write | 用 `call` | 写文件内容 |
| `chtimes` | `/ugidl.filesystem.Filesystem/Chtimes` | write | 用 `call` | 修改 atime/mtime |
| `modify-acl` | `/ugidl.filesystem.Filesystem/ModifyPathAcl` | write | 用 `call` | 需要嵌套 `permissions`，当前 `--field` 无法表达 |
| `share-mkdir` | `/ugidl.filesystem.Filesystem/CreateShareFolder` | write | 用 `call` | volume/name/quota 等 |
| `find` | `/ugidl.filesystem.Filesystem/Find` | read | 本地拒绝 | 服务端流式，退出 5 |
| `path-perm` | `/ugidl.filesystem.Filesystem/GetPathPermission` | — | 本地拒绝 | 当前 schema 未登记，退出 5 |

> `fs` 路径必须落在**共享文件夹**内，例如 `/volume1/demo`、`/volume2/demo`。卷根 `/volume1`、`/volume2` 会返回 `ACCESS_DENIED`（退出码 4）。

---

## 6. `call` 兜底

```bash
nascli call <socket> <method> [--field N:TYPE=V]... [--yes]
```

- `TYPE` 仅支持 `string`、`int`、`bool`；`int` 是无符号十进制。
- 同一个字段号可重复传，表示 repeated 字段。
- 已知方法按 schema 解码输出字段名；未知方法退回 `field_N`，并按写操作要求 `--yes`。
- 嵌套消息、bytes 任意二进制、float/double 不能通过 `--field` 表达。

### 常用示例

```bash
# 容器日志
nascli call ugreen.dockerserv.internal.grpc \
  /docker.Docker/GetContainerLogs --field 1:string=<容器ID>

# 端口占用
nascli call ugreen.dockerserv.internal.grpc \
  /docker.Docker/CheckPortOccupiedByContainer --field 1:int=9001

# 目录占用
nascli call ugreen.dockerserv.internal.grpc \
  /docker.Docker/CheckDirUsedByContainer --field 1:string=/volume1/demo

# 应用详情 / 状态
nascli call ugreen.app.grpc /app.App/GetAppDetail --field 1:string=<应用ID>
nascli call ugreen.app.grpc /app.App/GetAppStatus --field 1:string=<应用ID>
# 多 ID：重复 --field 1:string=<应用ID>

# 重命名（写）
nascli call ugreen.filemgr.internal.grpc \
  /ugidl.filesystem.Filesystem/Rename \
  --field 1:string=/volume1/demo --field 2:string=<旧名> --field 3:string=<新名> --yes

# 写文本文件（写）
nascli call ugreen.filemgr.internal.grpc \
  /ugidl.filesystem.Filesystem/WriteFile \
  --field 1:string=/volume1/demo/file.txt --field 2:string=<内容> --yes

# 修改时间（写）
nascli call ugreen.filemgr.internal.grpc \
  /ugidl.filesystem.Filesystem/Chtimes \
  --field 1:string=/volume1/demo/file.txt \
  --field 2:int=<atime-epoch> --field 3:int=<mtime-epoch> --yes

# 创建共享文件夹（写）
nascli call ugreen.filemgr.internal.grpc \
  /ugidl.filesystem.Filesystem/CreateShareFolder \
  --field 1:string=<卷> --field 2:string=<名称> \
  --field 3:int=<配额> --field 4:int=<单位> --yes
```

> `cpmv` 的字段：1 `src`、2 `dst`、3 `type`(bool)、4 `cover`(bool)、5 `action_type`(int)。字段语义随固件版本变化，操作前先用小文件验证。
>
> `modify-acl` 需要嵌套 `permissions` 消息，当前 `--field` 无法表达；不要用不完整字段调用。

---

## 7. 已知限制

| 能力 | 现象 | 处理 |
|---|---|---|
| `fs find` | 本地拒绝，退出 5 | 不要重试；需要搜索时由用户在 NAS 界面操作 |
| `fs du` | 大目录可能触发流式限制，退出 5 | 先试 `fs du`；失败则换更小目录或 `call` |
| `fs path-perm` | 本地拒绝，退出 5 | 当前固件未登记 `GetPathPermission`，不可用 |
| `network ifaces` | 恒返回空 `data` | 用 `network general` |
| `system update-status` | 恒返回空 `data` | 固件未提供，不要据此判断“无更新” |
| 目标参数动作 | 类型化命令退出 2，提示用 `call` | 用 `call --field 1:...` |
| 多参数 fs 动作 | `rename` / `cpmv` / `echo` / `chtimes` / `modify-acl` / `share-mkdir` 类型化入口拒绝 | 用 `call` |
| `modify-acl` | 嵌套 `permissions` 无法用 `--field` 构造 | 当前 CLI 不可用 |
| repeated 输出 | 单元素是标量，多元素是数组 | 解析时两种都兼容 |

---

## 8. AI 使用协议

1. **先发现能力**：`nascli list --read-only`、`nascli list --module <域>`、`nascli <子命令> --help`。
2. **能用类型化子命令就不用 `call`**：类型化命令会做 schema 解码和闸门检查。
3. **写操作先问用户**：得到明确同意后才加 `--yes`；不要自行推断“应该可以”。
4. **令牌在配置文件中**：不要 `cat` / `echo` 整个 `config.toml`，不要把 `token` 写进命令、报告或对话。
5. **退出码 5 不要盲目重试**：先区分本地 `find` / `path-perm` 与服务端流式限制。
6. **退出码 2 先看 stderr**：本地闸门错误不会在 stdout 输出 JSON。
7. **解析 stdout 前先判断形状**：repeated 字段可能是标量或数组；`storage smart` 的 `value` 是 JSON 字符串。
8. **`fs` 只用共享文件夹路径**：卷根会返回 4。
9. **Git Bash 注意路径转换**：`/volume1/...` 可能被 MSYS 改写；优先 PowerShell，必要时 `MSYS_NO_PATHCONV=1`。

---

## 9. 发现更多

```bash
nascli --help
nascli <子命令> --help
nascli list
nascli list --read-only
nascli list --module docker
```

完整设计见 `docs/design.md`；实现状态见 `docs/plans/2026-11-03-nascli-implementation.md`。
