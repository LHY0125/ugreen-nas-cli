//! `docker` 命令域：容器、镜像、Compose 项目与占用检查。
//!
//! 这是第一个包含写操作的命令域，因此 `resolve` 返回三元组，多带一个
//! operation（`read` / `write` / `delete` / `execute`）。`main` 依据它
//! 决定是否要求 `--yes`：只有 `read` 直接放行，其余一律先过闸门。

use crate::client::{ApiError, Client};
use crate::wire::Message;
use std::collections::BTreeMap;

/// Docker 服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.dockerserv.internal.grpc";

/// 把子命令映射为 `(套接字, gRPC 方法, operation)`。
///
/// operation 与 schema 中的标注保持一致；写类动作绝不能标成 `read`，
/// 否则会绕过 `--yes` 闸门。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "containers" => ("/docker.Docker/ContainerList", "read"),
        "images" => ("/docker.Docker/ShowLocalImage", "read"),
        "compose" => ("/docker.Docker/GetComposeProjectList", "read"),
        "logs" => ("/docker.Docker/GetContainerLogs", "read"),
        "port-check" => ("/docker.Docker/CheckPortOccupiedByContainer", "read"),
        "dir-check" => ("/docker.Docker/CheckDirUsedByContainer", "read"),
        "usage" => ("/docker.Docker/GetAppUsage", "read"),
        "pull" => ("/docker.Docker/PullImage", "write"),
        "start" => ("/docker.Docker/StartContainer", "execute"),
        "stop" => ("/docker.Docker/StopContainer", "execute"),
        "restart" => ("/docker.Docker/RestartContainer", "execute"),
        "compose-start" => ("/docker.Docker/StartComposeProject", "execute"),
        "compose-stop" => ("/docker.Docker/StopComposeProject", "execute"),
        "compose-restart" => ("/docker.Docker/RestartComposeProject", "execute"),
        "compose-delete" => ("/docker.Docker/DeleteComposeProject", "delete"),
        "rebuild" => ("/docker.Docker/RebuildContainerApp", "execute"),
        other => {
            return Err(format!(
                "未知的 docker 子命令：{other}（可用：containers, images, compose, logs, \
                 port-check, dir-check, usage, pull, start, stop, restart, compose-start, \
                 compose-stop, compose-restart, compose-delete, rebuild）"
            ))
        }
    };
    Ok((SOCKET, method, op))
}

/// 发送请求，返回原始响应消息。
///
/// 字段号 → 字段名的翻译由 `main` 用 schema 完成，这里保持传输层纯粹。
pub fn run(
    client: &Client,
    socket: &str,
    method: &str,
    msg: &Message,
) -> Result<Message, ApiError> {
    client.call(socket, method, msg)
}

/// 空请求体：当前所有 docker 动作都不带参数。
pub fn empty() -> Message {
    BTreeMap::new()
}
