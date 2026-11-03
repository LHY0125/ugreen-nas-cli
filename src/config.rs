use std::fmt;

/// 运行时配置：NAS 地址、端口、访问令牌与证书校验开关。
pub struct Config {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub insecure: bool,
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
    BadPort(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingHost => write!(
                f,
                "缺少环境变量 UGNAS_HOST。示例：\n  \
                 $env:UGNAS_HOST = \"nas.example.com\""
            ),
            ConfigError::MissingToken => write!(
                f,
                "缺少环境变量 UGPOA_TOKEN。请在 NAS 的「个人中心 → 本地账号 → 访问令牌」创建。"
            ),
            ConfigError::BadPort(v) => write!(f, "UGNAS_PORT 不是合法端口：{v}"),
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
        format!("https://{}:{}", self.host, self.port)
    }

    /// 从环境变量读取配置：UGNAS_HOST、UGPOA_TOKEN、UGNAS_PORT、UGNAS_INSECURE。
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_parts(
            std::env::var("UGNAS_HOST").ok(),
            std::env::var("UGPOA_TOKEN").ok(),
            std::env::var("UGNAS_PORT").ok(),
            std::env::var("UGNAS_INSECURE").ok(),
        )
    }

    /// 纯函数版本，便于测试；不读取任何环境变量。
    pub fn from_parts(
        host: Option<String>,
        token: Option<String>,
        port: Option<String>,
        insecure: Option<String>,
    ) -> Result<Self, ConfigError> {
        let host = host
            .filter(|h| !h.trim().is_empty())
            .ok_or(ConfigError::MissingHost)?;
        let token = token
            .filter(|t| !t.trim().is_empty())
            .ok_or(ConfigError::MissingToken)?;
        let port = match port {
            Some(p) => p.parse::<u16>().map_err(|_| ConfigError::BadPort(p))?,
            None => 9443,
        };
        let insecure = insecure.map(|v| v != "0").unwrap_or(true);
        Ok(Self {
            host,
            port,
            token,
            insecure,
        })
    }
}
