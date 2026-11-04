//! `apps` 命令域：应用中心的只读查询。
//!
//! 三个动作全部只读，因此 `resolve` 返回三元组里的 operation 恒为 `read`，
//! `main` 不需要 `--yes` 闸门。

/// 应用中心服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.app.grpc";

/// 把 apps 子命令映射为 `(套接字, gRPC 方法, operation)`。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "list" => ("/app.App/DetailList", "read"),
        "detail" => ("/app.App/GetAppDetail", "read"),
        "status" => ("/app.App/GetAppStatus", "read"),
        other => {
            return Err(format!(
                "未知的 apps 子命令：{other}（可用：list, detail, status）"
            ))
        }
    };
    Ok((SOCKET, method, op))
}
