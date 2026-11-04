//! `apps` 命令域：应用中心的只读查询。
//!
//! 三个动作全部只读，因此 `resolve` 返回三元组里的 operation 恒为 `read`，
//! `main` 不需要 `--yes` 闸门。
//!
//! `detail` / `status` 需要应用 ID，而当前版本尚未支持参数化；空请求只会拿到
//! 语义误导的服务端错误（`not found app` / `Parameter error!`），因此在发请求前
//! 由 [`require_target`] fail-closed。

use crate::wire::Message;
use std::collections::BTreeMap;

/// 应用中心服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.app.grpc";

/// 需要必填目标参数的动作：`(动作, 字段类型, 占位说明)`。
///
/// 类型与占位说明取自 schema 的 field 1，保证提示里的示例可以照抄执行。
const NEEDS_TARGET: &[(&str, &str, &str)] = &[
    ("detail", "string", "应用 ID"),
    ("status", "string", "应用 ID"),
];

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

/// 目标参数闸门：需要目标却发出空请求时拒绝执行。
///
/// 必须在建连之前调用——空请求不可能成功，放行只会把用户引向服务端的
/// 误导性错误。错误信息指向 `nascli call`，因为那是当前唯一能显式指定
/// 字段的入口。apps 全部只读，提示里不出现 `--yes`。
pub fn require_target(action: &str, msg: &Message) -> Result<(), String> {
    let Some(&(_, kind, hint)) = NEEDS_TARGET.iter().find(|(a, _, _)| *a == action) else {
        return Ok(());
    };
    if !msg.is_empty() {
        return Ok(());
    }
    let (socket, method, _) = resolve(action)?;
    Err(format!(
        "该动作需要目标参数，当前版本尚未支持参数化，已拒绝执行\n  method: {method}\n  \
         请使用 nascli call 显式指定字段，例如：\n  \
         nascli call {socket} {method} --field 1:{kind}=<{hint}>"
    ))
}

/// 空请求体：当前所有 apps 动作都不带参数。
pub fn empty() -> Message {
    BTreeMap::new()
}
