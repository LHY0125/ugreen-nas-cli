# nascli 分发设计（Scoop 上架）

日期：2026-11-04
状态：已确认，待实施
前置：nascli 已完成功能开发并发布到 `https://github.com/LHY0125/ugreen-nas-cli`

## 1 目标

让 nascli 可以通过 `scoop install nascli` 安装，并让后续版本自动流到包管理器。同时补齐 Linux / macOS 的二进制分发。

非目标：

- 不发布到 Scoop 官方 `main` misc bucket（自用工具，进官方桶需要满足维护活跃度、无同类冲突等条件）。
- 不提供 Windows arm64（用户已排除）。
- 不做安装包（MSI / NSIS）——CLI 工具单 exe 足够。

## 2 现状与障碍

| 项 | 现状 | 是否阻碍分发 |
| --- | --- | --- |
| 仓库 | `LHY0125/ugreen-nas-cli`，main 分支，38+ 提交 | 否 |
| CI | **无任何 workflow** | 是 |
| Release | 无 | 是 |
| `spec/schema.json` | 1.79 MB，被 `.gitignore` 排除，需 NAS SSH 才能生成 | **是（决定性）** |
| `build.rs` | schema 缺失时 `panic!` | 是（随上一条） |
| 配置路径 | 默认 `config.toml`（相对 CWD） | 是 |
| 用户 bucket | `lhy` = `LHY0125/scoop-bucket`，已有 4 个包 | 否 |

### 2.1 schema 是决定性障碍

`src/main.rs` 用 `include_str!("../spec/schema.json")` 把 schema 编译进二进制，`build.rs` 在文件缺失时中止编译。

这意味着**编译能力被绑定到拥有 schema 的机器上**。GitHub Actions runner 拿不到 schema，因此无法构建，release 自动化无从谈起。

三条出路：

1. 把 schema 提交进仓库 —— 已选。
2. 本地构建 + 手工上传 release。
3. 私有仓库存 schema，CI 用 PAT 拉取。

选 1 的理由：schema 经扫描确认**不含任何真实环境信息**（IP、主机名、路径、用户名、令牌、序列号均 0 命中，见第 3 节），且入库后仓库变成"输入完整、可复现构建"的状态，CI 与 release 才成立。代价是仓库增大约 1.79 MB、上游接口契约公开。

## 3 schema 安全性核查

已按「私网地址段、回环地址、标准卷名、系统用户名、个人域名、设备型号、邮箱域名、本机绝对路径、令牌前缀」等模式扫描，全部 0 命中。

schema 只包含 protobuf 消息结构（消息名、字段名、字段号、类型、枚举符号、服务方法签名），不含任何运行时数据。

遗留风险（非技术问题，记录在此供日后复查）：这是从厂商二进制逆向提取的第三方接口契约。公开的是接口结构而非厂商代码，但若日后固件改动导致 schema 暴露内部服务命名意图，应重新评估是否继续公开。

## 4 决策

| 编号 | 决策 | 选择 |
| --- | --- | --- |
| D1 | 发布二进制由谁产出 | 提交 schema 到仓库，走全自动 GitHub Actions |
| D2 | 目标平台 | Windows x64 + Linux x64 + macOS x64 |
| D3 | 配置默认位置 | 改为平台用户配置目录 |
| D4 | release 资产格式 | Windows 单 `.exe`；Linux/macOS `.tar.gz` |

## 5 schema 入库（D1）

### 5.1 改动

`.gitignore` 删除：

```
/spec/schema.json
```

保留 `/spec/openapi/`（提取器的中间产物，不入库）。

`git add spec/schema.json`，提交。

### 5.2 `build.rs`

逻辑不变——schema 入库后 panic 分支在正常流程中不再触发，但对"误删 schema 的本地克隆"仍是有效的失败提示。仅调整措辞，去掉"先跑提取器"的必选语气，说明该文件已入库、通常不需要重新生成。

### 5.3 文档同步

| 文件 | 改动 |
| --- | --- |
| `CLAUDE.md` | 「`spec/` 的产物不入库」→「`spec/schema.json` **已入库**，是 CI 构建的必要输入；`spec/openapi/` 仍不入库」 |
| `README.md` 安装章节 | 删除「新克隆必须先跑一次提取器」，改为直接 `cargo build`；保留「固件升级后需重新生成 schema」的说明 |
| `docs/design.md` | 更新与 schema 存放相关的段落 |

## 6 配置路径迁移（D3）

### 6.1 现状

`src/main.rs:19-26`：

```rust
/// TOML 配置文件路径（默认当前目录的 config.toml）
#[arg(long, global = true, value_name = "PATH", default_value = "config.toml")]
config: PathBuf,
```

硬编码 `default_value` 使配置文件位置依赖调用者的 CWD。scoop 安装后从任意目录调用都会找不到配置。

### 6.2 目标查找顺序

| 顺序 | 来源 | 说明 |
| --- | --- | --- |
| 1 | `--config <path>` | 显式指定，最高优先级 |
| 2 | `UGNAS_CONFIG` 环境变量 | 供 CI 与特殊场景 |
| 3 | 平台用户配置目录 | 见 6.3 |

### 6.3 平台路径

| 平台 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\nascli\config.toml` |
| Linux | `$XDG_CONFIG_HOME/nascli/config.toml`，未设置则 `$HOME/.config/nascli/config.toml` |
| macOS | `$HOME/Library/Application Support/nascli/config.toml` |

### 6.4 实现

新增 `src/config.rs::default_config_path() -> PathBuf`。不引入 `dirs` crate——以 3 个 `#[cfg]` 分支手写约 30 行，可直接单测。

`main.rs` 的 `--config` 改为 `Option<PathBuf>`（clap 的 `default_value` 无法调用函数），在 `main()` 入口解析：

```rust
let config_path = cli.config
    .or_else(|| std::env::var_os("UGNAS_CONFIG").map(PathBuf::from))
    .unwrap_or_else(default_config_path);
```

### 6.5 行为变更（有意为之）

CWD 下存在 `config.toml` 但未传 `--config` 时，**不再被读取**。单一配置源避免"到底读的哪个文件"这类排查困难。仓库内开发用 `--config config.toml` 或设 `UGNAS_CONFIG=config.toml`。

### 6.6 测试

`tests/config.rs` 新增：

1. 三平台路径解析（`#[cfg]` 分别断言，含 `APPDATA` / `XDG_CONFIG_HOME` 已设置与未设置两种情形）。
2. `UGNAS_CONFIG` 优先级高于默认路径。
3. `--config` 优先级高于环境变量（走 CLI 层断言）。
4. 首次运行在交互式终端下自动初始化到默认路径（已有 `load_or_init` 逻辑，补一条路径断言）。

### 6.7 附带收益

scoop 的 `uninstall` 只清理 `apps/<name>` 目录。配置在 `%APPDATA%` 下，卸载与重装不会丢失。若改用 scoop 的 `persist` 方案，卸载会连配置一起删。

## 7 CI（D2 的前置）

新增 `.github/workflows/ci.yml`，参考 `PathEditor/.github/workflows/ci.yml` 的结构。

```
触发：push main / pull_request / workflow_dispatch
matrix：windows-latest（MSVC）、ubuntu-latest、macos-latest
步骤：
  1. checkout
  2. rustup 安装 stable；Windows 覆盖为 stable-x86_64-pc-windows-msvc
  3. Swatinem/rust-cache@v2
  4. cargo fmt --check
  5. cargo clippy --all-targets -- -D warnings
  6. cargo test
  7. python -m pytest tools/tests/（仅 ubuntu-latest 跑，避免重复）
```

不需要 PathEditor 里的 `OPENSSL_SRC_PERL` / `PERL5LIB` / PATH 三行——nascli 用 `rustls-tls`，不依赖 OpenSSL。

## 8 发布（D2 / D4）

新增 `.github/workflows/release.yml`，沿用 `PathEditor/.github/workflows/release.yml` 骨架：

```
触发：push tag 'v*'
权限：contents: write
步骤：
  1. 解析 tag，校验 ^v\d+\.\d+\.\d+$
  2. 幂等检查：Release 已存在则跳过构建
  3. 版本一致性：tag 去掉 v 后必须等于 Cargo.toml 的 version
  4. matrix 构建：
     windows-latest → cargo build --release → nascli.exe
     ubuntu-latest  → cargo build --release → nascli-linux-x64（tar.gz）
     macos-15-intel → cargo build --release → nascli-macos-x64（tar.gz）
  5. 若已提交 CHANGELOG.md 则取对应段落，否则 git log 生成
  6. gh release create --latest，上传全部资产
```

### 8.1 资产命名

| 平台 | 文件名 |
| --- | --- |
| Windows x64 | `nascli_<version>_x64.exe` |
| Linux x64 | `nascli_<version>_linux_x64.tar.gz` |
| macOS x64 | `nascli_<version>_macos_x64.tar.gz` |

### 8.2 三个平台坑

| 坑 | 说明 | 处理 |
| --- | --- | --- |
| Windows 工具链 | 本机是 GNU，GitHub runner 默认 MSVC | `rustup override set stable-x86_64-pc-windows-msvc` |
| macOS runner 架构 | `macos-latest` 已是 arm64，产物会是 arm64 | 用 `macos-15-intel`（Intel 宿主）原生编译，避免交叉编译的 SDK 问题 |
| OpenSSL | PathEditor 需要三行 perl 环境变量 | **不需要**，rustls 不碰 OpenSSL |

### 8.3 关于 macOS 架构

用户选择 x64。`macos-latest` 产出的 arm64 二进制与预期不符，因此固定 `macos-15-intel`（`macos-13` 已于 2025-12-04 退役）。若日后需要 arm64，在 matrix 中加一行 `macos-latest` 并区分资产名即可。

## 9 Scoop 清单

在 `LHY0125/scoop-bucket` 新建 `bucket/nascli.json`：

```json
{
    "version": "0.1.0",
    "description": "CLI for UGOS Pro OpenCLI access-token API",
    "homepage": "https://github.com/LHY0125/ugreen-nas-cli",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": "https://github.com/LHY0125/ugreen-nas-cli/releases/download/v0.1.0/nascli_0.1.0_x64.exe#/nascli.exe",
            "hash": "<sha256>"
        }
    },
    "bin": "nascli.exe",
    "checkver": {
        "url": "https://api.github.com/repos/LHY0125/ugreen-nas-cli/releases/latest",
        "jsonpath": "$.tag_name",
        "regex": "v?([\\d.]+)"
    },
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/LHY0125/ugreen-nas-cli/releases/download/v$version/nascli_$version_x64.exe#/nascli.exe",
                "hash": {
                    "url": "https://api.github.com/repos/LHY0125/ugreen-nas-cli/releases/tags/v$version",
                    "jsonpath": "$.assets[?(@.name == 'nascli_$version_x64.exe')].digest"
                }
            }
        }
    }
}
```

### 9.1 设计要点

1. `#/nascli.exe` 直接下载并重命名，与同 bucket 的 `patheditor-cli` 一致；不需要 `extract_dir` 或 `installer`。
2. `hash` 走 release API 的 `digest` 字段，由 GitHub 在上传时自动计算，无需手工维护——`patheditor-cli` 已验证该机制可用。
3. `bin` 为字符串，只有一个可执行文件，不需要 `path` 别名对。
4. 首次发布后需手工填一次真实 hash（见 10）。

### 9.2 自动更新

用户 bucket 已有 `.github/workflows/excavator.yml`（每 4 小时跑 ScoopInstaller/GithubActions）。清单提交后，后续版本无需人工介入。

## 10 实施顺序

1. schema 入库 + 文档同步 → 提交。
2. 配置路径迁移 + 测试 → 提交。
3. 新增 `ci.yml` → 观察一次绿的构建。
4. 打 `v0.1.0` tag → 触发 release workflow → 检查三个资产。
5. 从 release API 取 Windows exe 的 sha256 → 写 `bucket/nascli.json` → 提交 bucket 仓库。
6. `scoop install nascli` 验证 → 运行 `nascli config init` → `nascli storage volumes` 验证端到端。

### 10.1 验证清单

- [ ] `git clone` 到全新目录后 `cargo build --release` 成功（证明 schema 入库有效）
- [ ] CI 三平台全绿
- [ ] release 三个资产存在且大小合理
- [ ] `scoop install nascli` 后 `nascli --version` 输出版本号
- [ ] 在任意目录执行 `nascli config init` 生成 `%APPDATA%\nascli\config.toml`
- [ ] `nascli storage volumes` 返回 JSON
- [ ] `scoop uninstall nascli` 后 `%APPDATA%\nascli\config.toml` 仍在
- [ ] 版本号 +1 再发一次，excavator 自动更新 bucket 清单

## 11 风险与回退

| 风险 | 规避 | 回退 |
| --- | --- | --- |
| schema 公开后在法律或商业上有顾虑 | 已确认不含环境信息 | 从仓库移除该文件，恢复 `.gitignore`，退回本地发布 |
| CI 因大文件变慢 | 1.79 MB，编译期读入，影响可忽略 | — |
| `macos-15-intel` runner 未来下线 | 届时改用 matrix 加 `--target x86_64-apple-darwin` 交叉编译 | — |
| 首次 hash 填错导致安装失败 | 实施顺序第 5 步从 API 取值，不手算 | 修正清单并重新提交 |
| 配置路径迁移破坏已有用法 | 文档明示 `--config` 与 `UGNAS_CONFIG` 两条兼容路径 | 保留 CWD 兜底（当前设计已放弃，若反馈强烈再加） |

## 12 待办（不在本次范围）

- Windows arm64
- 发布到 Scoop 官方桶
- `cargo install nascli` 走 crates.io
- Docker 镜像
