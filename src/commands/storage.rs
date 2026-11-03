//! `storage` 命令域：磁盘、存储卷、存储池与 SMART 信息。
//!
//! 全部动作都是只读查询，因此不需要 `--yes` 闸门。

use crate::client::{ApiError, Client};
use crate::wire::Message;
use std::collections::BTreeMap;

/// 存储服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.storage.grpc";

/// 把子命令映射为 `(套接字, gRPC 方法)`。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str), String> {
    let method = match action {
        "disks" => "/storage.Storage/DiskList",
        "volumes" => "/storage.Storage/VolumeList",
        "pools" => "/storage.Storage/PoolList",
        "smart" => "/storage.Storage/AllDiskSmartInfo",
        other => {
            return Err(format!(
                "未知的 storage 子命令：{other}（可用：disks, volumes, pools, smart）"
            ))
        }
    };
    Ok((SOCKET, method))
}

/// 执行只读查询，返回原始响应消息。
///
/// 字段号 → 字段名的翻译由 `main` 用 schema 完成，这里保持传输层纯粹。
pub fn run(client: &Client, action: &str) -> Result<Message, ApiError> {
    let (socket, method) = resolve(action).map_err(ApiError::BadRequest)?;
    client.call(socket, method, &BTreeMap::new())
}
