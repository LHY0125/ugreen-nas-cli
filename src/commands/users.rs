//! `users` 命令域：用户与用户组的只读查询。
//!
//! 两个动作都走控制服务 `ugreen.ctlserv.internal.grpc`，全部只读，
//! 因此 `resolve` 只返回 `(套接字, gRPC 方法)` 二元组。

/// 控制与配置服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.ctlserv.internal.grpc";

/// 把 users 子命令映射为 `(套接字, gRPC 方法)`。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str), String> {
    let method = match action {
        "list" => "/ctlserv.Internal/GetUgreenUserList",
        "groups" => "/ctlserv.Internal/GetCreatedGroupList",
        other => {
            return Err(format!(
                "未知的 users 子命令：{other}（可用：list, groups）"
            ))
        }
    };
    Ok((SOCKET, method))
}
