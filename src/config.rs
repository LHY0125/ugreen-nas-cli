use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// 运行时配置：NAS 地址、端口、访问令牌与证书校验开关。
pub struct Config {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub insecure: bool,
}

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

/// TOML 文件结构。
///
/// `deny_unknown_fields` 让拼错的键直接报错，避免“配置没生效但静默用默认值”。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    host: String,
    token: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_insecure")]
    insecure: bool,
}

fn default_port() -> u16 {
    9443
}

fn default_insecure() -> bool {
    true
}

/// 手写 Debug：令牌一律脱敏，避免 `{:?}` / `dbg!` / 断言失败信息泄露明文。
///
/// 注意：新增字段时需在此同步脱敏，否则会以明文出现在 Debug 输出中。
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("token", &redact(&self.token))
            .field("insecure", &self.insecure)
            .finish()
    }
}

/// 配置解析失败的原因。
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    MissingHost,
    MissingToken,
    BadHost(String),
    BadPort(String),
    AlreadyExists(String),
    UnknownKey(String),
    Prompt(String),
    Read { path: String, error: String },
    Write { path: String, error: String },
    Parse { path: String, error: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingHost => write!(f, "配置文件缺少 host（或为空）"),
            ConfigError::MissingToken => write!(
                f,
                "配置文件缺少 token（或为空）。令牌在 NAS 的「个人中心 → 本地账号 → 访问令牌」创建。"
            ),
            ConfigError::BadHost(v) => write!(
                f,
                "配置项 host 格式不合法：{v}（只填主机名或 IP，不要带协议、路径或端口）"
            ),
            ConfigError::BadPort(v) => write!(f, "配置项 port 不是合法端口：{v}"),
            ConfigError::AlreadyExists(path) => write!(
                f,
                "配置文件已存在：{path}\n如需覆盖，请使用 nascli config init --force"
            ),
            ConfigError::UnknownKey(key) => write!(
                f,
                "未知配置项：{key}（可用：host, port, token, insecure）"
            ),
            ConfigError::Prompt(e) => write!(f, "读取输入失败：{e}"),
            ConfigError::Read { path, error } => write!(
                f,
                "无法读取配置文件 {path}：{error}\n\
                 可运行 nascli config init 生成配置，或手动创建：\n  \
                 host = \"nas.example.com\"\n  \
                 port = 9443\n  \
                 token = \"ugpoa_v1....\"\n  \
                 insecure = true"
            ),
            ConfigError::Write { path, error } => {
                write!(f, "无法写入配置文件 {path}：{error}")
            }
            ConfigError::Parse { path, error } => {
                write!(f, "配置文件 {path} 解析失败：{error}")
            }
        }
    }
}

/// 令牌脱敏：仅当首段是 `ugpoa` 方案前缀时保留它，其余一律 ****。
///
/// 非标准格式（如 `a.b.c`、`trailing.`）整体脱敏，避免回显用户输入的首段。
pub fn redact(token: &str) -> String {
    match token.split_once('.') {
        Some((head, _)) if head.starts_with("ugpoa") => format!("{head}.****"),
        _ => "****".to_string(),
    }
}

impl Config {
    pub fn base_url(&self) -> String {
        // IPv6 字面量必须放进方括号，否则 URL 解析会把冒号当成端口分隔符。
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("https://{host}:{}", self.port)
    }

    /// 从 TOML 文件读取配置；这是 CLI 唯一的配置来源。
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
            path: path.display().to_string(),
            error: e.to_string(),
        })?;
        let file: ConfigFile = toml::from_str(&text).map_err(|e| ConfigError::Parse {
            path: path.display().to_string(),
            error: e.to_string(),
        })?;
        Self::validate(file.host, file.token, file.port, file.insecure)
    }

    /// 写回 TOML 文件。会创建父目录，并覆盖同名文件。
    pub fn write_file(&self, path: &Path) -> Result<(), ConfigError> {
        let file = ConfigFile {
            host: self.host.clone(),
            token: self.token.clone(),
            port: self.port,
            insecure: self.insecure,
        };
        let text = toml::to_string_pretty(&file).map_err(|e| ConfigError::Write {
            path: path.display().to_string(),
            error: e.to_string(),
        })?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| ConfigError::Write {
                    path: path.display().to_string(),
                    error: e.to_string(),
                })?;
            }
        }
        std::fs::write(path, text).map_err(|e| ConfigError::Write {
            path: path.display().to_string(),
            error: e.to_string(),
        })
    }

    /// 读取配置；文件不存在且当前是交互式终端时，自动进入初始化向导。
    pub fn load_or_init(path: &Path) -> Result<Self, ConfigError> {
        match Self::from_file(path) {
            Ok(c) => Ok(c),
            Err(e @ ConfigError::Read { .. }) if !path.exists() => {
                if io::stdin().is_terminal() && io::stderr().is_terminal() {
                    eprintln!("未找到配置文件 {}，进入初始化向导。", path.display());
                    Self::init_interactive(path, false)
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// 交互式初始化；`force` 为真时覆盖已有文件。
    pub fn init_interactive(path: &Path, force: bool) -> Result<Self, ConfigError> {
        if path.exists() && !force {
            return Err(ConfigError::AlreadyExists(path.display().to_string()));
        }
        let cfg = prompt_config(None)?;
        cfg.write_file(path)?;
        eprintln!("配置已写入 {}", path.display());
        Ok(cfg)
    }

    /// 交互式修改：每项显示当前值，直接回车保持不变。
    pub fn edit_interactive(path: &Path) -> Result<Self, ConfigError> {
        let current = Self::from_file(path)?;
        let cfg = prompt_config(Some(&current))?;
        cfg.write_file(path)?;
        eprintln!("配置已更新 {}", path.display());
        Ok(cfg)
    }

    /// 供 `config set token` 无参数时提示输入。
    pub fn prompt_value(label: &str) -> Result<String, ConfigError> {
        prompt(label, None)
    }

    /// 用于 `config show`：输出合法 TOML，但令牌已脱敏。
    pub fn redacted_toml(&self) -> String {
        let file = ConfigFile {
            host: self.host.clone(),
            token: redact(&self.token),
            port: self.port,
            insecure: self.insecure,
        };
        toml::to_string_pretty(&file).unwrap_or_else(|_| "（配置序列化失败）\n".to_string())
    }

    /// 修改单个字段并写回。
    pub fn set_field(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key.to_ascii_lowercase().as_str() {
            "host" => self.host = value.to_string(),
            "token" => self.token = value.to_string(),
            "port" => {
                let port = value
                    .parse::<u16>()
                    .map_err(|_| ConfigError::BadPort(value.to_string()))?;
                if port == 0 {
                    return Err(ConfigError::BadPort(value.to_string()));
                }
                self.port = port;
            }
            "insecure" => self.insecure = parse_bool(value)?,
            other => return Err(ConfigError::UnknownKey(other.to_string())),
        }
        let validated = Self::validate(
            self.host.clone(),
            self.token.clone(),
            self.port,
            self.insecure,
        )?;
        *self = validated;
        Ok(())
    }

    fn validate(
        host: String,
        token: String,
        port: u16,
        insecure: bool,
    ) -> Result<Self, ConfigError> {
        let host = host.trim().to_string();
        if host.is_empty() {
            return Err(ConfigError::MissingHost);
        }
        // 只接受主机名或 IP；协议、路径、端口应分别写在 host/port。
        let bare_host = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        if bare_host.contains("://") || bare_host.contains('/') {
            return Err(ConfigError::BadHost(host));
        }
        if bare_host.contains(':') && bare_host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(ConfigError::BadHost(host));
        }
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(ConfigError::MissingToken);
        }
        if port == 0 {
            return Err(ConfigError::BadPort(port.to_string()));
        }
        Ok(Self {
            host,
            port,
            token,
            insecure,
        })
    }
}

fn prompt_config(existing: Option<&Config>) -> Result<Config, ConfigError> {
    loop {
        let host = prompt("host", existing.map(|c| c.host.as_str()))?;
        if host.is_empty() {
            eprintln!("host 不能为空，请重新输入。");
            continue;
        }
        let port_default = existing
            .map(|c| c.port.to_string())
            .unwrap_or_else(|| "9443".into());
        let port_text = prompt("port", Some(&port_default))?;
        let port = match port_text.parse::<u16>() {
            Ok(p) if p != 0 => p,
            _ => {
                eprintln!("端口必须是 1..=65535 的整数，请重新输入。");
                continue;
            }
        };
        let token = if let Some(c) = existing {
            let value = prompt("token（留空保持不变）", None)?;
            if value.is_empty() {
                c.token.clone()
            } else {
                value
            }
        } else {
            let value = prompt("token", None)?;
            if value.is_empty() {
                eprintln!("token 不能为空，请重新输入。");
                continue;
            }
            value
        };
        let insecure_default = existing.map(|c| c.insecure).unwrap_or(true);
        let insecure = match prompt_bool("insecure（接受自签名证书？）", insecure_default)
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };
        match Config::validate(host, token, port, insecure) {
            Ok(c) => return Ok(c),
            Err(e) => {
                eprintln!("{e}");
                eprintln!("请重新输入。");
            }
        }
    }
}

fn prompt(label: &str, default: Option<&str>) -> Result<String, ConfigError> {
    eprint!("{label}");
    if let Some(d) = default {
        eprint!(" [{d}]");
    }
    eprint!(": ");
    io::stderr()
        .flush()
        .map_err(|e| ConfigError::Prompt(e.to_string()))?;
    let mut line = String::new();
    let read = io::stdin()
        .read_line(&mut line)
        .map_err(|e| ConfigError::Prompt(e.to_string()))?;
    if read == 0 {
        return Err(ConfigError::Prompt("输入结束".to_string()));
    }
    let value = line.trim().to_string();
    if value.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(value)
    }
}

fn prompt_bool(label: &str, default: bool) -> Result<bool, ConfigError> {
    let hint = if default { "Y/n" } else { "y/N" };
    let value = prompt(&format!("{label} [{hint}]"), None)?;
    if value.is_empty() {
        return Ok(default);
    }
    parse_bool(&value)
}

fn parse_bool(value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        other => Err(ConfigError::Prompt(format!(
            "不是合法布尔值：{other}（可用 true/false）"
        ))),
    }
}
