# nascli 分发（Scoop 上架）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 nascli 通过 `scoop install nascli` 安装，并让后续版本经 GitHub Actions 自动发布、自动流入 Scoop bucket。

**Architecture:** 先把 `spec/schema.json` 提交入库，使 GitHub Actions 能完整构建；再把配置文件默认路径从 CWD 迁到平台用户配置目录，使全局安装后可用；然后新增 CI 与 release 两个 workflow，产出三平台二进制；最后在 `LHY0125/scoop-bucket` 写入 scoop 清单，由既有的 excavator 工作流接管后续版本更新。

**Tech Stack:** Rust 1.88 / clap 4 / GitHub Actions / Scoop

**Spec:** `docs/superpowers/specs/2026-11-04-nascli-distribution-design.md`

## Global Constraints

- 单 crate，`edition = "2021"`，不使用 workspace。
- **git commit 信息用中文**，格式 `<类型>：<描述>`（类型取 `功能` / `修复` / `文档` / `构建` / `测试` / `重构`）。
- 不得提交任何真实环境信息（主机名、IP、序列号、用户名、邮箱、真实路径）。示例统一 `nas.example.com` / `/volume1/demo`。
- 提交前必跑：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
- **未经书面同意不得删除任何文件。**
- 不得修改 NAS 任何配置（本计划全程不接触 NAS）。
- 不引入新依赖（`dirs` crate 明确不引入，路径解析手写）。

---

## 文件结构

| 文件 | 动作 | 职责 |
| --- | --- | --- |
| `.gitignore` | 修改 | 移除对 `spec/schema.json` 的排除 |
| `spec/schema.json` | 新增入库 | CI 构建的必要输入（1.79 MB） |
| `build.rs` | 修改 | 措辞更新：schema 已入库，通常无需重新生成 |
| `src/config.rs` | 修改 | 新增平台配置根目录解析，替换 `default_config_path` 实现 |
| `src/main.rs` | 修改 | `--config` 改为 `Option<PathBuf>`，加入 `UGNAS_CONFIG` 与默认路径决议 |
| `tests/config.rs` | 修改 | 新增三平台路径解析、优先级测试；改写 `default_path_is_config_toml` |
| `README.md` | 修改 | 安装章节去掉"必须先跑提取器"；配置章节改为用户配置目录 |
| `CLAUDE.md` | 修改 | 更新 schema 入库说明、配置路径说明、常用命令 |
| `docs/design.md` | 修改 | 同步 schema 存放与配置位置相关段落 |
| `config.example.toml` | 修改 | 注释说明推荐放置位置 |
| `.github/workflows/ci.yml` | 新增 | 三平台 fmt / clippy / test |
| `.github/workflows/release.yml` | 新增 | tag 触发，矩阵构建 + 创建 Release |
| `LHY0125/scoop-bucket` → `bucket/nascli.json` | 新增（另一个仓库） | scoop 清单 |

---

## Task 1: schema 入库，解锁 CI 构建

**Files:**
- Modify: `.gitignore:2`
- Add: `spec/schema.json`
- Modify: `build.rs:3-13`

**Interfaces:**
- Consumes: 无
- Produces: 仓库处于"clone 后可直接 `cargo build`"的状态，后续 CI 任务依赖此状态

- [ ] **Step 1: 确认 schema 存在且未被其他规则排除**

Run:
```bash
cd <repo>
ls -l spec/schema.json
git check-ignore -v spec/schema.json
```

Expected: 文件存在（约 1793845 字节）；`git check-ignore` 输出 `.gitignore:2:/spec/schema.json`

- [ ] **Step 2: 从 `.gitignore` 移除排除规则**

把 `.gitignore` 第 2 行 `/spec/schema.json` 删除。改完后文件内容应为：

```
/target
/spec/openapi/
__pycache__/
.superpowers/
/config.toml
```

- [ ] **Step 3: 验证规则已失效**

Run:
```bash
cd <repo>
git check-ignore -v spec/schema.json; echo "exit=$?"
```

Expected: 无输出，`exit=1`（1 表示"不是被忽略的文件"）

- [ ] **Step 4: 修改 `build.rs` 的提示措辞**

把 `build.rs` 整个文件替换为：

```rust
use std::path::Path;

fn main() {
    let schema = Path::new("spec/schema.json");
    println!("cargo:rerun-if-changed=spec/schema.json");
    if !schema.exists() {
        panic!(
            "\n\n缺少 spec/schema.json。\n\
             该文件应随仓库提供（已入库），请检查是否被误删：\n\
               git checkout -- spec/schema.json\n\
             仅在固件升级、需要重新提取时才运行：\n\
               python tools/extract_schema.py --host <NAS 地址>\n"
        );
    }
}
```

- [ ] **Step 5: 验证构建仍通过**

Run:
```bash
cd <repo>
cargo build --release 2>&1 | tail -5
```

Expected: 编译成功，无 panic

- [ ] **Step 6: 加入暂存区并确认文件大小**

Run:
```bash
cd <repo>
git add .gitignore build.rs spec/schema.json
git status --short
git diff --cached --stat
```

Expected: `spec/schema.json` 显示约 1.8 MB 的新增；`.gitignore` 显示 1 行删除；`build.rs` 有修改

- [ ] **Step 7: 提交**

```bash
git commit -m "构建：schema.json 入库，使 CI 可完整构建

spec/schema.json 经敏感信息扫描确认不含主机名、IP、路径、令牌等
环境数据，可随仓库分发。入库后 clone 即可编译，GitHub Actions
无需 NAS 访问即可产出发布二进制。"
```

---

## Task 2: 平台配置目录解析（纯函数 + 单元测试）

**Files:**
- Modify: `src/config.rs:15-20`
- Test: `tests/config.rs:21-25`

**Interfaces:**
- Consumes: 无
- Produces:
  - `pub enum Platform { Windows, MacOs, Linux }` — `Clone + Copy + PartialEq + Eq + Debug`
  - `pub fn config_root_for(platform: Platform, appdata: Option<&Path>, xdg: Option<&Path>, home: Option<&Path>) -> Option<PathBuf>`
  - `pub fn default_config_path() -> PathBuf`
  - `pub const DEFAULT_CONFIG_FILE: &str = "config.toml"`（保持不变）

- [ ] **Step 1: 写失败测试**

把 `tests/config.rs` 第 21-25 行的旧测试：

```rust
#[test]
fn default_path_is_config_toml() {
    assert_eq!(DEFAULT_CONFIG_FILE, "config.toml");
    assert_eq!(default_config_path().to_str(), Some("config.toml"));
}
```

替换为下面这组测试，并同步修改第 1 行的 `use` 语句为：

```rust
use nascli::config::{
    config_root_for, default_config_path, redact, Config, ConfigError, Platform,
    DEFAULT_CONFIG_FILE,
};
```

新测试：

```rust
#[test]
fn config_file_name_is_stable() {
    assert_eq!(DEFAULT_CONFIG_FILE, "config.toml");
}

#[test]
fn windows_uses_appdata() {
    let appdata = Path::new("C:/Users/u/AppData/Roaming");
    let root = config_root_for(Platform::Windows, Some(appdata), None, None);
    assert_eq!(root, Some(PathBuf::from("C:/Users/u/AppData/Roaming")));
}

#[test]
fn windows_without_appdata_has_no_root() {
    let root = config_root_for(Platform::Windows, None, Some(Path::new("/xdg")), Some(Path::new("/home/u")));
    assert_eq!(root, None);
}

#[test]
fn linux_prefers_xdg_over_home() {
    let root = config_root_for(
        Platform::Linux,
        None,
        Some(Path::new("/xdg")),
        Some(Path::new("/home/u")),
    );
    assert_eq!(root, Some(PathBuf::from("/xdg")));
}

#[test]
fn linux_falls_back_to_home_dot_config() {
    let root = config_root_for(Platform::Linux, None, None, Some(Path::new("/home/u")));
    assert_eq!(root, Some(PathBuf::from("/home/u/.config")));
}

#[test]
fn macos_uses_application_support() {
    let root = config_root_for(Platform::MacOs, None, None, Some(Path::new("/Users/u")));
    assert_eq!(
        root,
        Some(PathBuf::from("/Users/u/Library/Application Support"))
    );
}

#[test]
fn default_path_ends_with_nascli_config_toml() {
    let path = default_config_path();
    assert!(
        path.ends_with(Path::new("nascli").join(DEFAULT_CONFIG_FILE))
            || path == PathBuf::from(DEFAULT_CONFIG_FILE),
        "默认路径应以 nascli/config.toml 结尾，实际为 {}",
        path.display()
    );
}
```

同时在 `tests/config.rs` 顶部的 `use std::io::Write;` 后补一行：

```rust
use std::path::{Path, PathBuf};
```

- [ ] **Step 2: 运行测试确认失败**

Run:
```bash
cd <repo>
cargo test --test config 2>&1 | tail -20
```

Expected: 编译失败，报 `no config_root_for in the root` / `no Platform in the root`

- [ ] **Step 3: 实现路径解析**

把 `src/config.rs` 第 13-20 行（`DEFAULT_CONFIG_FILE` 常量与 `default_config_path` 函数）替换为：

```rust
/// 默认配置文件名。
pub const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// 用于路径解析的目标平台。抽成参数而非只用 `cfg!`，
/// 使得任意平台上的测试都能覆盖全部分支。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

/// 当前编译目标平台。
fn current_platform() -> Platform {
    if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Linux
    }
}

/// 给定平台与环境变量值，算出配置根目录。
///
/// 三个参数分别对应 `%APPDATA%`、`$XDG_CONFIG_HOME`、`$HOME`；
/// 各平台只使用其中相关的项，其余传 `None` 也不会误用。
pub fn config_root_for(
    platform: Platform,
    appdata: Option<&Path>,
    xdg: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    match platform {
        Platform::Windows => appdata.map(Path::to_path_buf),
        Platform::MacOs => home.map(|h| h.join("Library").join("Application Support")),
        Platform::Linux => xdg
            .map(Path::to_path_buf)
            .or_else(|| home.map(|h| h.join(".config"))),
    }
}

/// 默认配置路径：`<配置根目录>/nascli/config.toml`。
///
/// 环境变量全部缺失时退回当前目录的 `config.toml`——这是最后手段，
/// 正常安装环境下不会走到。
pub fn default_config_path() -> PathBuf {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);

    config_root_for(
        current_platform(),
        appdata.as_deref(),
        xdg.as_deref(),
        home.as_deref(),
    )
    .map(|root| root.join("nascli").join(DEFAULT_CONFIG_FILE))
    .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE))
}
```

- [ ] **Step 4: 运行测试确认通过**

Run:
```bash
cd <repo>
cargo test --test config 2>&1 | tail -15
```

Expected: 全部通过（约 22 个用例），无失败

- [ ] **Step 5: 确认其他调用点未断裂**

Run:
```bash
cd <repo>
cargo build 2>&1 | tail -10
```

Expected: 编译成功。`src/main.rs` 此时仍在用 `default_value = "config.toml"`，与本任务不冲突

- [ ] **Step 6: 格式与 lint**

Run:
```bash
cd <repo>
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

Expected: 无警告、无错误

- [ ] **Step 7: 提交**

```bash
git add src/config.rs tests/config.rs
git commit -m "功能：配置路径解析支持 Windows/Linux/macOS 用户目录

新增 config_root_for 纯函数，按平台返回配置根目录：
Windows 用 %APPDATA%，Linux 优先 $XDG_CONFIG_HOME 再退回 ~/.config，
macOS 用 ~/Library/Application Support。抽成平台参数以便在任意
平台上单测全部分支。"
```

---

## Task 3: main.rs 接入配置路径决议

**Files:**
- Modify: `src/main.rs:8`（use 语句）、`src/main.rs:19-26`（clap 参数）、`src/main.rs:288-289`（入口解析）

**Interfaces:**
- Consumes: `nascli::config::default_config_path() -> PathBuf`（Task 2）
- Produces: 配置路径决议顺序 `--config` > `UGNAS_CONFIG` > `default_config_path()`；后续任务与 scoop 清单依赖此行为

- [ ] **Step 1: 更新 use 语句**

`src/main.rs` 第 6 行：

```rust
use nascli::config::Config;
```

改为：

```rust
use nascli::config::{default_config_path, Config};
```

- [ ] **Step 2: 把 `--config` 改为可选**

`src/main.rs` 第 19-26 行：

```rust
    /// TOML 配置文件路径（默认当前目录的 config.toml）
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = "config.toml"
    )]
    config: PathBuf,
```

改为：

```rust
    /// TOML 配置文件路径
    ///
    /// 未指定时优先读环境变量 UGNAS_CONFIG，
    /// 再退回平台用户配置目录（Windows: %APPDATA%\nascli\config.toml）。
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
```

- [ ] **Step 3: 在入口处决议路径**

`src/main.rs` 第 288-289 行：

```rust
    let cli = Cli::parse();
    let config_path = cli.config;
```

改为：

```rust
    let cli = Cli::parse();
    let config_path = resolve_config_path(cli.config);
```

- [ ] **Step 4: 新增决议函数**

在 `src/main.rs` 中 `fn build_ctx` 之前插入：

```rust
/// 决议配置文件路径：`--config` > `UGNAS_CONFIG` > 平台默认目录。
///
/// 单一配置源：当前目录下的 `config.toml` 不再被隐式读取。
/// 仓库内开发请显式传 `--config config.toml` 或设 `UGNAS_CONFIG`。
fn resolve_config_path(explicit: Option<PathBuf>) -> PathBuf {
    explicit
        .or_else(|| std::env::var_os("UGNAS_CONFIG").map(PathBuf::from))
        .unwrap_or_else(default_config_path)
}
```

- [ ] **Step 5: 编译验证**

Run:
```bash
cd <repo>
cargo build 2>&1 | tail -15
```

Expected: 编译成功，无 `config: PathBuf` 相关的类型错误

- [ ] **Step 6: 手工验证三条优先级路径**

Run（PowerShell，逐条执行并观察 stderr 中的路径）：

```powershell
cd <repo>
cargo build --release

# 1) 默认：应显示 %APPDATA%\nascli\config.toml
.\target\release\nascli.exe config path

# 2) UGNAS_CONFIG 覆盖
$env:UGNAS_CONFIG = "<tmp>/alt.toml"
.\target\release\nascli.exe config path
Remove-Item Env:\UGNAS_CONFIG

# 3) --config 覆盖（优先级最高）
$env:UGNAS_CONFIG = "<tmp>/alt.toml"
.\target\release\nascli.exe --config <tmp>/highest.toml config path
Remove-Item Env:\UGNAS_CONFIG
```

Expected:
1. 输出形如 `C:\Users\<user>\AppData\Roaming\nascli\config.toml`
2. 输出 `<tmp>/alt.toml`
3. 输出 `<tmp>/highest.toml`

（若 `config path` 的输出实现与此不符，先看 `src/main.rs` 的 `run_config` 中 `Command::ConfigAction::Path` 分支，按其实际输出格式判断。）

- [ ] **Step 7: 全量测试**

Run:
```bash
cd <repo>
cargo test 2>&1 | tail -20
```

Expected: 全部通过

- [ ] **Step 8: 格式与 lint**

Run:
```bash
cd <repo>
cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

Expected: 无警告

- [ ] **Step 9: 提交**

```bash
git add src/main.rs
git commit -m "功能：配置文件默认路径改用平台用户配置目录

--config 改为可选参数，决议顺序为 --config > UGNAS_CONFIG >
平台默认目录。scoop 等包管理器全局安装后，从任意工作目录调用
都能找到同一份配置，不再依赖当前目录。"
```

---

## Task 4: 文档同步

**Files:**
- Modify: `README.md`（安装章节、配置章节）
- Modify: `CLAUDE.md`（schema 说明、常用命令、配置说明）
- Modify: `docs/design.md`
- Modify: `config.example.toml`

**Interfaces:**
- Consumes: Task 1 的 schema 入库事实、Task 2/3 的配置路径行为
- Produces: 无代码接口；为后续使用者与 AI 智能体提供正确前提

- [ ] **Step 1: 更新 `README.md` 安装章节**

把「环境要求」与「安装」两节替换为：

```markdown
## 环境要求

- 运行：无需额外依赖，单个可执行文件
- 从源码构建：Rust 1.88+
- 重新提取 schema（仅固件升级后需要）：Python 3.10+，且需要到 NAS 的 SSH 访问

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
```

- [ ] **Step 2: 更新 `README.md` 配置章节**

把「配置」一节替换为：

```markdown
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
```

- [ ] **Step 3: 更新 `CLAUDE.md`**

三处改动：

(a) 「常用命令」中 `config` 段落后补一行：

```bash
# 配置文件默认位于平台用户配置目录；仓库内开发时用 --config 或 UGNAS_CONFIG 指定
```

(b) 「### 配置（仅 TOML 文件）」一节，把开头的

> CLI 只读取一个 TOML 文件；默认当前目录的 `config.toml`，可用全局参数
> `--config <path>` 指定。没有环境变量兜底，也没有第二配置源。

替换为：

> CLI 只读取一个 TOML 文件。路径决议顺序：`--config <path>` →
> 环境变量 `UGNAS_CONFIG` → 平台用户配置目录（Windows 为
> `%APPDATA%\nascli\config.toml`）。当前目录的 `config.toml` **不会**被隐式读取。

(c) 「## 仓库约定」一节，把

> - `spec/` 的产物与 `docs/NAS-UGOS-OpenCLI-API.md` 均不入库。

替换为：

> - `spec/schema.json` **已入库**，是 CI 构建的必要输入，不要删除或加入 `.gitignore`。
> - `spec/openapi/`（提取器中间产物）与 `docs/NAS-UGOS-OpenCLI-API.md` 不入库。

- [ ] **Step 4: 更新 `config.example.toml`**

在文件开头插入注释：

```toml
# 推荐放置位置（nascli config init 会自动写到这里）：
#   Windows: %APPDATA%\nascli\config.toml
#   Linux:   ~/.config/nascli/config.toml
#   macOS:   ~/Library/Application Support/nascli/config.toml
#
# 也可以用 --config <path> 或环境变量 UGNAS_CONFIG 指定其他位置。
```

- [ ] **Step 5: 更新 `docs/design.md`**

在涉及 schema 生成与配置来源的段落中：

- 把「schema.json 不入库、构建前需先跑提取器」改为「schema.json 已入库」
- 把配置来源描述改为上面的三级决议顺序

（先 `grep -n "入不入库\|不入库\|config.toml\|提取器" docs/design.md` 定位具体行号再改。）

- [ ] **Step 6: 检查文档中无残留旧描述**

Run:
```bash
cd <repo>
grep -rn "当前目录的 config.toml\|spec/schema.json 不入库\|spec/ 的产物" README.md CLAUDE.md docs/design.md config.example.toml
```

Expected: 无输出（若有输出，指出残留位置并修正）

- [ ] **Step 7: 提交**

```bash
git add README.md CLAUDE.md docs/design.md config.example.toml
git commit -m "文档：同步 schema 入库与配置路径变更

README 安装章节改为 scoop / 源码 / 预编译三种方式；配置章节说明
平台用户配置目录与三级覆盖优先级。CLAUDE.md 更新仓库约定中关于
spec 目录的描述。"
```

---

## Task 5: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: Task 1 的 schema 入库（无此前提 CI 会在 `build.rs` 处失败）
- Produces: 三平台质量门，Task 6 的 release 复用同一套工具链设置

- [ ] **Step 1: 创建 `ci.yml`**

创建 `.github/workflows/ci.yml`：

```yaml
# nascli — 持续集成
# push main / PR / 手动触发时，在三个平台跑格式、lint 与测试。
name: CI

on:
  push:
    branches: [main]
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  test:
    name: 质量门（${{ matrix.os }}）
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest, macos-latest]

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      # GitHub 的 Windows runner 默认已是 MSVC，这里显式固定，
      # 避免本机 rust-toolchain 配置影响云端行为。
      - name: 固定 Rust 工具链
        shell: pwsh
        run: |
          rustup toolchain install stable --profile minimal
          rustup default stable
          rustup show

      - name: 缓存 Cargo
        uses: Swatinem/rust-cache@v2

      - name: 格式检查
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings

      - name: 测试
        run: cargo test

      # Python 提取器的测试只在 Linux 跑一次，避免三平台重复。
      - name: Python 提取器测试
        if: matrix.os == 'ubuntu-latest'
        run: |
          python3 -m pip install --upgrade pip
          python3 -m pip install pytest
          python3 -m pytest tools/tests -v
```

- [ ] **Step 2: 本地语法检查（YAML 结构）**

Run:
```bash
cd <repo>
python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml',encoding='utf-8')); print('YAML OK')"
```

Expected: `YAML OK`

- [ ] **Step 3: 确认 `tools/tests` 路径存在**

Run:
```bash
cd <repo>
ls tools/tests/ 2>&1 | head -5
```

Expected: 列出测试文件（至少一个 `test_*.py`）。若目录不存在，把 `ci.yml` 中「Python 提取器测试」整步删除，并在提交信息中说明。

- [ ] **Step 4: 提交**

```bash
git add .github/workflows/ci.yml
git commit -m "构建：新增三平台 CI 工作流

push main、PR 与手动触发时，在 Windows / Linux / macOS 上跑
cargo fmt --check、clippy -D warnings 与 cargo test；Python
提取器测试只在 Linux 跑一次。"
```

- [ ] **Step 5: 推送后观察首次 CI**

Run:
```bash
cd <repo>
git push origin main
gh run list --workflow=ci.yml --limit 3
```

Expected: 出现三个 job，全部 `completed / success`。若失败，读日志定位（最常见是 clippy 在某个平台报平台相关警告）

---

## Task 6: Release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: Task 1（schema 入库）、Task 5（CI 工具链设置）、Task 3（配置路径，影响资产可用性）
- Produces: 三平台 release 资产，文件名为 `nascli_<version>_x64.exe` / `nascli_<version>_linux_x64.tar.gz` / `nascli_<version>_macos_x64.tar.gz`；Task 7 依赖这些名称

- [ ] **Step 1: 创建 `release.yml`**

创建 `.github/workflows/release.yml`：

```yaml
# nascli — 自动发布
# 推送 v* tag 时，在三个平台构建二进制并创建 GitHub Release。
# 若该 tag 的 Release 已存在，则跳过构建与发布。
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build:
    name: 构建（${{ matrix.os }}）
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: true
      matrix:
        include:
          - os: windows-latest
            artifact: windows
          - os: ubuntu-latest
            artifact: linux
          # 执行时 R9 裁决改为 macos-15-intel（macos-13 已于 2025-12-04 退役）
          - os: macos-13
            artifact: macos

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: 解析版本号
        id: ver
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          if ($tag -notmatch '^v(?<v>\d+\.\d+\.\d+)$') {
            throw "发布 tag 必须符合 vMAJOR.MINOR.PATCH，当前为: $tag"
          }
          "version=$($Matches['v'])" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append

      - name: 校验 Cargo.toml 版本一致
        shell: pwsh
        run: |
          $cargo = Get-Content -LiteralPath 'Cargo.toml' -Raw
          if ($cargo -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
            throw '无法从 Cargo.toml 读取 version'
          }
          if ($Matches[1] -ne "${{ steps.ver.outputs.version }}") {
            throw "tag 版本 ${{ steps.ver.outputs.version }} 与 Cargo.toml=$($Matches[1]) 不一致"
          }

      - name: 固定 Rust 工具链
        shell: pwsh
        run: |
          rustup toolchain install stable --profile minimal
          rustup default stable

      - name: 缓存 Cargo
        uses: Swatinem/rust-cache@v2

      - name: 构建（Windows）
        if: matrix.artifact == 'windows'
        shell: pwsh
        run: |
          cargo build --release
          Copy-Item -LiteralPath 'target\release\nascli.exe' `
            -Destination "nascli_${{ steps.ver.outputs.version }}_x64.exe"

      - name: 构建（Linux）
        if: matrix.artifact == 'linux'
        run: |
          cargo build --release
          tar -czf "nascli_${{ steps.ver.outputs.version }}_linux_x64.tar.gz" \
            -C target/release nascli

      - name: 构建（macOS）
        if: matrix.artifact == 'macos'
        run: |
          cargo build --release
          tar -czf "nascli_${{ steps.ver.outputs.version }}_macos_x64.tar.gz" \
            -C target/release nascli

      - name: 上传构建产物
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact }}
          path: nascli_*
          if-no-files-found: error

  release:
    name: 创建 Release
    needs: build
    runs-on: windows-latest

    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: 下载全部构建产物
        uses: actions/download-artifact@v4
        with:
          path: assets
          merge-multiple: true

      - name: 列产物
        shell: pwsh
        run: Get-ChildItem -LiteralPath 'assets' | Select-Object Name, Length

      - name: 生成发布日志
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          $notes = @("## nascli $tag", "")
          $prev = git describe --tags --abbrev=0 "$tag^" 2>$null
          if ($LASTEXITCODE -eq 0 -and $prev) {
            $notes += '### 变更'
            $notes += ''
            $notes += git log --pretty=format:'- %s (%h)' "$prev..$tag"
          } else {
            $notes += '首次发布'
          }
          $notes | Set-Content -LiteralPath 'RELEASE_NOTES.md' -Encoding utf8

      - name: 创建 GitHub Release
        shell: pwsh
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          $existing = gh release view "${{ github.ref_name }}" --repo "$env:GITHUB_REPOSITORY" --json tagName 2>$null
          if ($LASTEXITCODE -eq 0 -and $existing) {
            Write-Host "Release 已存在，跳过。"
            exit 0
          }
          $assets = Get-ChildItem -LiteralPath 'assets' -File | ForEach-Object { $_.FullName }
          gh release create "${{ github.ref_name }}" `
            --repo "$env:GITHUB_REPOSITORY" `
            --title "nascli ${{ github.ref_name }}" `
            --notes-file RELEASE_NOTES.md `
            --latest `
            @assets
```

- [ ] **Step 2: 本地语法检查**

Run:
```bash
cd <repo>
python -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml',encoding='utf-8')); print('YAML OK')"
```

Expected: `YAML OK`

- [ ] **Step 3: 确认 `macos-13` runner 可用**

Run（需要已认证的 gh）：
```bash
gh api repos/LHY0125/ugreen-nas-cli/actions/runners 2>&1 | head -5
```

Expected: 能看到 runner 信息或空列表（403 表示 token 权限不足，不影响后续——`macos-13` 是 GitHub 托管的标准标签）

> 偏差说明：执行时按 R9 裁决改用 `macos-15-intel`，因为 `macos-13` 已于 2025-12-04 退役。本段及上方 matrix 保留原稿，仅作记录。

- [ ] **Step 4: 提交**

```bash
git add .github/workflows/release.yml
git commit -m "构建：新增 release 工作流，产出三平台二进制

打 v* tag 时矩阵构建 Windows/Linux/macOS 三个平台，校验 tag 与
Cargo.toml 版本一致后上传资产并创建 Release。macOS 固定用
macos-13（Intel 宿主）以保证产出 x64 二进制。"
```

---

## Task 7: 首次发布与 Scoop 上架

**Files:**
- Create（另一个仓库）: `LHY0125/scoop-bucket` → `bucket/nascli.json`

**Interfaces:**
- Consumes: Task 6 产出的 release 资产名称
- Produces: `scoop install nascli` 可用；excavator 接管后续版本更新

> **执行前置（用户确认）**：本任务包含推送到 GitHub、创建 tag 与 Release、向第二个仓库提交——都是外部可见操作。**执行前须获得用户明确同意。**

- [ ] **Step 1: 提升版本号并提交**

`Cargo.toml` 的 `version` 当前为 `0.1.0`。若此前已发布过 `v0.1.0`，改为 `0.1.1`；否则保持 `0.1.0`。

Run:
```bash
cd <repo>
grep -n '^version' Cargo.toml
gh release list --repo LHY0125/ugreen-nas-cli
```

Expected: 确认无同名 tag 已发布

- [ ] **Step 2: 推送 main 并打 tag**

Run:
```bash
cd <repo>
git push origin main
git tag -a v0.1.0 -m "nascli 首个发布版本"
git push origin v0.1.0
```

Expected: tag 推送成功

- [ ] **Step 3: 等待 release workflow 完成**

Run:
```bash
cd <repo>
gh run list --workflow=release.yml --limit 3
```

Expected: build 三个 job + release 一个 job，全部 success

- [ ] **Step 4: 校验三个资产存在**

Run:
```bash
gh release view v0.1.0 --repo LHY0125/ugreen-nas-cli --json assets --jq '.assets[] | "\(.name)\t\(.size)"'
```

Expected: 三行，`nascli_0.1.0_x64.exe`、`nascli_0.1.0_linux_x64.tar.gz`、`nascli_0.1.0_macos_x64.tar.gz`，大小均 > 1 MB

- [ ] **Step 5: 取 Windows exe 的 sha256**

Run:
```bash
gh api repos/LHY0125/ugreen-nas-cli/releases/tags/v0.1.0 \
  --jq '.assets[] | select(.name=="nascli_0.1.0_x64.exe") | .digest'
```

Expected: 输出形如 `sha256:abc123...`。记下冒号后的 64 位十六进制串

- [ ] **Step 6: 写 scoop 清单**

创建 `bucket/nascli.json`（在 `LHY0125/scoop-bucket` 仓库中）：

```json
{
    "version": "0.1.0",
    "description": "CLI for UGOS Pro OpenCLI access-token API",
    "homepage": "https://github.com/LHY0125/ugreen-nas-cli",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": "https://github.com/LHY0125/ugreen-nas-cli/releases/download/v0.1.0/nascli_0.1.0_x64.exe#/nascli.exe",
            "hash": "0000000000000000000000000000000000000000000000000000000000000000"
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

写完后**必须**把上面那个全 0 的占位 hash 换成 Step 5 取到的真实值：

```bash
# 从 Step 5 的输出复制冒号后的 64 位十六进制，替换占位符
# 校验：长度必须正好 64，且只含 0-9a-f
```

- [ ] **Step 7: 校验清单**

Run:
```bash
cd <scoop-bucket-repo>
powershell -NoProfile -Command "./bin/checkver.ps1 -App nascli -Dir . -ForceUpdate"
powershell -NoProfile -Command "./bin/checkurls.ps1 -App nascli -Dir ."
```

Expected: `checkver` 输出 `nascli: 0.1.0`；`checkurls` 全部 URL 返回 200

- [ ] **Step 8: 提交并推送 bucket 仓库**

Run:
```bash
cd <scoop-bucket-repo>
git add bucket/nascli.json
git commit -m "新增 nascli：UGOS Pro OpenCLI 客户端"
git push
```

- [ ] **Step 9: 安装验证**

Run:
```powershell
scoop update
scoop install nascli
nascli --version
nascli config path
```

Expected: 版本号正确；`config path` 输出 `%APPDATA%\nascli\config.toml`

- [ ] **Step 10: 端到端验证**

Run（需用户提供 NAS 地址与令牌，自行填写）：

```powershell
nascli config init
nascli storage volumes
```

Expected: `config init` 交互式写入 `%APPDATA%\nascli\config.toml`；`storage volumes` 输出 `{"ok": true, ...}`

- [ ] **Step 11: 卸载不丢配置**

Run:
```powershell
scoop uninstall nascli
Test-Path "$env:APPDATA\nascli\config.toml"
scoop install nascli
```

Expected: `Test-Path` 输出 `True`；重装后配置仍在

---

## 自检结果

**1. Spec 覆盖**

| Spec 节 | 对应任务 |
| --- | --- |
| 5 schema 入库 | Task 1 |
| 6 配置路径迁移（6.1-6.7） | Task 2、Task 3 |
| 5.3 / 6 文档同步 | Task 4 |
| 7 CI | Task 5 |
| 8 发布 | Task 6 |
| 9 Scoop 清单 | Task 7 |
| 10 实施顺序 | Task 1→7 顺序即实施顺序 |
| 10.1 验证清单 | Task 3 Step 6、Task 7 Step 9-11 |
| 11 风险与回退 | Task 1 提交信息记录可回退点 |

**2. 占位符扫描**：无 TBD/TODO。Task 7 Step 6 的 hash 用全 0 占位并在紧接着的步骤给出替换指令——全 0 是一个语法合法但校验必然失败的值，`scoop install` 会立即报 hash mismatch，不会静默通过。

**3. 类型一致性**：`Platform`、`config_root_for`、`default_config_path`、`resolve_config_path` 在 Task 2/3 中定义一次、在使用处签名一致。`DEFAULT_CONFIG_FILE` 保持 `&str = "config.toml"` 未变。
