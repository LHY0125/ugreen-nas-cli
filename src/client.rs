use crate::config::{redact, Config};
use crate::wire::{decode, encode, Message};
use std::time::Duration;

/// 一次 OpenCLI 调用的失败分类。
///
/// 变体顺序与退出码契约一致：BadRequest=2、Unauthorized=3、Forbidden=4、
/// Unsupported=5、Network=6、Server=7。
#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden(String),
    BadRequest(String),
    Unsupported(String),
    Network(String),
    Server(String),
}

impl ApiError {
    /// 进程退出码（硬契约，脚本按此判断失败类型）。
    pub fn exit_code(&self) -> i32 {
        match self {
            ApiError::BadRequest(_) => 2,
            ApiError::Unauthorized => 3,
            ApiError::Forbidden(_) => 4,
            ApiError::Unsupported(_) => 5,
            ApiError::Network(_) => 6,
            ApiError::Server(_) => 7,
        }
    }

    /// 稳定的错误码字符串（写入 JSON 的 `error.code`）。
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(_) => "BAD_REQUEST",
            ApiError::Unauthorized => "UNAUTHORIZED",
            ApiError::Forbidden(_) => "ACCESS_DENIED",
            ApiError::Unsupported(_) => "UNSUPPORTED",
            ApiError::Network(_) => "NETWORK_ERROR",
            ApiError::Server(_) => "SERVER_ERROR",
        }
    }

    /// 面向用户的错误描述；401 不带服务端原文，统一提示令牌问题。
    pub fn message(&self) -> &str {
        match self {
            ApiError::BadRequest(m)
            | ApiError::Forbidden(m)
            | ApiError::Unsupported(m)
            | ApiError::Network(m)
            | ApiError::Server(m) => m,
            ApiError::Unauthorized => "令牌无效或已过期",
        }
    }

    /// 输出侧兜底：若错误信息里出现令牌明文，替换成脱敏形式。
    ///
    /// 用于防御服务端把 `Authorization` 头原样回显进错误体的情况；
    /// `Unauthorized` 不带服务端原文，无需处理。
    pub fn scrub(self, token: &str) -> Self {
        // 空令牌没有可泄漏内容；且 `str::replace("")` 会在每个字符边界插入替换串，
        // 把整条文案打碎，故直接原样返回。
        if token.is_empty() {
            return self;
        }
        let redacted = redact(token);
        let fix = |m: String| m.replace(token, &redacted);
        match self {
            ApiError::BadRequest(m) => ApiError::BadRequest(fix(m)),
            ApiError::Forbidden(m) => ApiError::Forbidden(fix(m)),
            ApiError::Unsupported(m) => ApiError::Unsupported(fix(m)),
            ApiError::Network(m) => ApiError::Network(fix(m)),
            ApiError::Server(m) => ApiError::Server(fix(m)),
            ApiError::Unauthorized => ApiError::Unauthorized,
        }
    }
}

/// 从响应体里取 `message` 字段，其次取非空的 `msg`，都取不到就返回原文。
///
/// 原文按 `char` 截断到 200 个字符，避免把整页 HTML 错误写进 JSON。
fn extract_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
            return m.to_string();
        }
        if let Some(m) = v
            .get("msg")
            .and_then(|m| m.as_str())
            .filter(|m| !m.is_empty())
        {
            return m.to_string();
        }
    }
    body.chars().take(200).collect()
}

/// 把 HTTP 状态码 + gRPC 状态码 + 响应体归一成 `ApiError`。
///
/// 纯函数，不发网络请求；判定优先级固定：
/// 401 → Unauthorized；`cardinality violation` → Unsupported（流式方法不支持）；
/// `Access not allowed` → Forbidden；400 / gRPC 3 / gRPC 1005 → BadRequest；
/// 200 但缺 `x-grpc-code` → Server（提示代理可能剥离了该头）；
/// 其余（含 415、5xx、未知状态）→ Server。
pub fn classify(status: u16, grpc_code: Option<i64>, body: &str) -> ApiError {
    let msg = extract_message(body);
    if status == 401 {
        return ApiError::Unauthorized;
    }
    if msg.contains("cardinality violation") {
        return ApiError::Unsupported(
            "该方法为服务端流式 RPC，OpenCLI 的 HTTP 接口不支持".to_string(),
        );
    }
    if msg.contains("Access not allowed") {
        return ApiError::Forbidden(msg);
    }
    if status == 400 || grpc_code == Some(3) || grpc_code == Some(1005) {
        return ApiError::BadRequest(msg);
    }
    if status == 415 {
        return ApiError::Server(msg);
    }
    // 200 却没有 x-grpc-code：无法判断成功与否，且 body 是 protobuf 二进制，
    // 直接展示会是乱码，故给出明确文案（常见于反向代理剥离响应头）。
    if status == 200 && grpc_code.is_none() {
        return ApiError::Server(
            "响应缺少 x-grpc-code 头，可能被代理剥离；请确认直连 NAS 或代理透传该头".to_string(),
        );
    }
    if status >= 500 {
        return ApiError::Server(msg);
    }
    ApiError::Server(msg)
}

/// OpenCLI HTTP 客户端：protobuf 请求体 + Bearer 令牌。
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    token: String,
}

impl Client {
    pub fn new(cfg: Config) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(cfg.insecure)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            base: cfg.base_url(),
            token: cfg.token,
        })
    }

    /// 调用 `POST {base}/ugreen/opencli/v1/{socket}{method}`，返回解码后的响应消息。
    pub fn call(&self, socket: &str, method: &str, msg: &Message) -> Result<Message, ApiError> {
        let url = format!("{}/ugreen/opencli/v1/{}{}", self.base, socket, method);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/x-protobuf")
            .body(encode(msg))
            .send()
            .map_err(|e| {
                let hint = if e.is_timeout() { "（超时）" } else { "" };
                ApiError::Network(format!("{e}{hint}"))
            })?;

        let status = resp.status().as_u16();
        let grpc_code = resp
            .headers()
            .get("x-grpc-code")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok());
        let bytes = resp.bytes().map_err(|e| ApiError::Network(e.to_string()))?;

        if status == 200 && grpc_code == Some(0) {
            return Ok(decode(&bytes));
        }
        let body = String::from_utf8_lossy(&bytes).to_string();
        // 输出侧兜底脱敏：服务端若回显 Authorization 头，明文令牌不得进入错误信息。
        Err(classify(status, grpc_code, &body).scrub(&self.token))
    }
}
