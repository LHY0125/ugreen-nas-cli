//! `download` 命令域：下载中心的只读查询。
//!
//! 当前只有 `tasks` 一个只读动作，`resolve` 返回三元组以与 apps/docker 保持同形。

/// 下载服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.downloadserv.internal.grpc";

/// 把 download 子命令映射为 `(套接字, gRPC 方法, operation)`。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "tasks" => ("/download.DownloadService/BatchGetDownloadTask", "read"),
        other => return Err(format!("未知的 download 子命令：{other}（可用：tasks）")),
    };
    Ok((SOCKET, method, op))
}
