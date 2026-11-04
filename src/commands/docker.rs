//! `docker` 命令域：容器、镜像、Compose 项目与占用检查。
//!
//! 这是第一个包含写操作的命令域，因此 `resolve` 返回三元组，多带一个
//! operation（`read` / `write` / `delete` / `execute`）。`main` 依据它
//! 决定是否要求 `--yes`：只有 `read` 直接放行，其余一律先过闸门。

use crate::client::{ApiError, Client};
use crate::schema::is_read_only;
use crate::wire::Message;
use std::collections::BTreeMap;

/// Docker 服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.dockerserv.internal.grpc";

/// 需要必填目标参数的动作：`(动作, 字段类型, 占位说明)`。
///
/// 当前版本尚未支持参数化，这些动作若发出空请求，目标缺失——写类动作可能造成
/// 范围未限定的破坏（例如空 `project_name` 被服务端解释为「全部项目」），
/// 只读动作也只会拿到服务端参数错误。因此一律在发请求前 fail-closed。
///
/// 类型与占位说明取自 schema 的 field 1，保证提示里的示例可以照抄执行。
pub const NEEDS_TARGET: &[(&str, &str, &str)] = &[
    // 只读但需要目标
    ("logs", "string", "容器 ID"),
    ("port-check", "int", "端口号"),
    ("dir-check", "string", "目录路径"),
    // 写类
    ("start", "string", "容器 ID"),
    ("stop", "string", "容器 ID"),
    ("restart", "string", "容器 ID"),
    ("pull", "string", "镜像引用"),
    ("compose-start", "string", "项目名"),
    ("compose-stop", "string", "项目名"),
    ("compose-restart", "string", "项目名"),
    ("compose-delete", "string", "项目名"),
    ("rebuild", "string", "应用 ID"),
];

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

/// 目标参数闸门：需要目标却发出空请求时拒绝执行。
///
/// 必须在 `require_yes` 之后调用——先让用户确认「要写」，再告诉他「写什么」；
/// 两者都满足才允许建连。错误信息指向 `nascli call`，因为那是当前唯一能
/// 显式指定字段的入口。
pub fn require_target(action: &str, msg: &Message) -> Result<(), String> {
    if !msg.is_empty() {
        return Ok(());
    }
    let Some(&(_, kind, hint)) = NEEDS_TARGET.iter().find(|(a, _, _)| *a == action) else {
        return Ok(());
    };
    let (socket, method, op) = resolve(action)?;
    let mut text = format!(
        "该动作需要目标参数，当前版本尚未支持参数化，已拒绝执行\n  method: {method} [{op}]\n  \
         请使用 nascli call 显式指定字段，例如：\n  \
         nascli call {socket} {method} --field 1:{kind}=<{hint}>"
    );
    // 只读动作不需要 --yes，提示里也不该出现，否则会误导用户以为必须加。
    if !is_read_only(op) {
        text.push_str(" --yes");
    }
    Err(text)
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
