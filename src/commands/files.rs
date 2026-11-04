//! `fs` 命令域：文件与目录操作。
//!
//! 这是参数最复杂的命令域，也是唯一含删除动作的域。服务端的 `Readdir` /
//! `Find` / `Du` 是**服务端流式** RPC，OpenCLI 的 HTTP 接口不支持（返回
//! `cardinality violation`），因此：
//!
//! - `ls` 改用一元的 `CategoryOrFileDirList`；
//! - `du` / `find` 在 `resolve` 阶段直接报不支持，退出码 5。
//!
//! 字段号一律由 schema 按字段名查出：`Stat` 的 `path` 在字段 1，而
//! `DelPath` 的 `path` 在字段 2——硬编码任何一处都会错。

use crate::schema::{is_read_only, Schema};
use crate::wire::{Message, Value};
use std::collections::BTreeMap;

/// 文件管理服务对应的套接字 ID。
pub const SOCKET: &str = "ugreen.filemgr.internal.grpc";

/// 只接受单个 `--path` 的动作。
///
/// 这些动作的请求消息里都有名为 `path` 的字段，字段号交给 schema 查。
pub const SINGLE_PATH: &[&str] = &[
    "stat",
    "ls",
    "cat",
    "getfattr",
    "recycle-status",
    "path-perm",
    "mkdir",
    "touch",
    "del",
    "restore",
];

/// 需要多个参数、无法用单个 `--path` 表达的动作。
///
/// 当前版本不提供参数化入口，一律 fail-closed 并引导用户走 `nascli call`。
/// 注意 `share-mkdir` 需要 `volume` / `name` / `quota` 等字段，同样不在此列。
pub const NEEDS_CALL: &[&str] = &[
    "rename",
    "cpmv",
    "echo",
    "md5sum",
    "sha256sum",
    "chtimes",
    "modify-acl",
    "share-mkdir",
];

/// 该方法是否因服务端流式而不可用（`resolve` 失败时应映射为退出码 5）。
///
/// 与「未知子命令」必须区分：后者是参数错误（2）。若统一返回 5，智能体会
/// 把拼错的子命令当成流式限制，转而去 `nascli call` 调一个错误的方法。
pub fn is_streaming_unsupported(action: &str) -> bool {
    matches!(action, "du" | "find")
}

/// `resolve` 失败时的退出码：流式方法不支持（5）与未知子命令（2）分开。
pub fn resolve_error_exit_code(action: &str) -> u8 {
    if is_streaming_unsupported(action) {
        5
    } else {
        2
    }
}

/// 把子命令映射为 `(套接字, gRPC 方法, operation)`。
///
/// operation 必须与 schema 中的标注一致：写类动作标成 `read` 会绕过
/// `--yes` 闸门，删除动作标成 `write` 则丢失「不可逆」的语义。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "stat" => ("/ugidl.filesystem.Filesystem/Stat", "read"),
        // `ls` 用一元的 CategoryOrFileDirList；Readdir 是服务端流式，HTTP 不支持。
        "ls" => ("/ugidl.filesystem.Filesystem/CategoryOrFileDirList", "read"),
        "cat" => ("/ugidl.filesystem.Filesystem/ReadFile", "read"),
        "getfattr" => ("/ugidl.filesystem.Filesystem/Getfattr", "read"),
        "recycle-status" => ("/ugidl.filesystem.Filesystem/GetRecycleStatus", "read"),
        "path-perm" => ("/ugidl.filesystem.Filesystem/GetPathPermission", "read"),
        "md5sum" => ("/ugidl.filesystem.Filesystem/Md5sum", "read"),
        "sha256sum" => ("/ugidl.filesystem.Filesystem/Sha256sum", "read"),
        "mkdir" => ("/ugidl.filesystem.Filesystem/CreateDir", "write"),
        "touch" => ("/ugidl.filesystem.Filesystem/CreateFile", "write"),
        "echo" => ("/ugidl.filesystem.Filesystem/WriteFile", "write"),
        "rename" => ("/ugidl.filesystem.Filesystem/Rename", "write"),
        "cpmv" => ("/ugidl.filesystem.Filesystem/CpMovePath", "write"),
        "del" => ("/ugidl.filesystem.Filesystem/DelPath", "delete"),
        "restore" => ("/ugidl.filesystem.Filesystem/RecoverFromRecycle", "write"),
        "chtimes" => ("/ugidl.filesystem.Filesystem/Chtimes", "write"),
        "modify-acl" => ("/ugidl.filesystem.Filesystem/ModifyPathAcl", "write"),
        "share-mkdir" => ("/ugidl.filesystem.Filesystem/CreateShareFolder", "write"),
        "du" => return Err("du 对应的服务端方法为流式 RPC，OpenCLI 的 HTTP 接口不支持".to_string()),
        "find" => {
            return Err("find 对应的服务端方法为流式 RPC，OpenCLI 的 HTTP 接口不支持".to_string())
        }
        other => return Err(format!("未知的 fs 子命令：{other}")),
    };
    Ok((SOCKET, method, op))
}

/// 目标参数闸门：单参数动作没有 `--path` 时拒绝执行。
///
/// 必须在 `require_yes` 之后调用——先让用户确认「要写」，再告诉他「写什么」。
/// 空路径对只读动作只会拿到服务端参数错误，对写类动作则可能造成范围未限定的
/// 破坏，因此一律在发请求前 fail-closed。
pub fn require_target(action: &str, path: &str) -> Result<(), String> {
    if !path.is_empty() || !SINGLE_PATH.contains(&action) {
        return Ok(());
    }
    let (_, method, op) = resolve(action)?;
    let mut text = format!(
        "该动作需要 --path 参数，已拒绝执行\n  method: {method} [{op}]\n  请加上 --path <路径>"
    );
    // 只读动作不需要 --yes，提示里也不该出现，否则会误导用户以为必须加。
    if !is_read_only(op) {
        text.push_str("，写类动作还需 --yes");
    }
    Err(text)
}

/// 按 schema 里的**字段名**构造请求，不硬编码字段号。
///
/// 多参数动作（`rename` / `cpmv` / `echo` / `md5sum` 等）不在此处理，
/// 引导用户走 `nascli call` 显式指定字段。
pub fn build_message(
    schema: &Schema,
    method: &str,
    action: &str,
    path: &str,
    page: Option<u64>,
    limit: Option<u64>,
) -> Result<Message, String> {
    if NEEDS_CALL.contains(&action) {
        return Err(format!(
            "{action} 需要多个参数，请改用 nascli call 显式指定字段"
        ));
    }
    if !SINGLE_PATH.contains(&action) {
        return Err(format!(
            "{action} 尚未支持参数化，请改用 nascli call 显式指定字段"
        ));
    }

    let req_type = schema
        .request_type(method)
        .ok_or_else(|| format!("schema 中找不到 {method} 的请求类型"))?;
    let path_no = schema
        .field_no(&req_type, "path")
        .ok_or_else(|| format!("{req_type} 中没有字段 path"))?;

    let mut m: Message = BTreeMap::new();
    m.insert(path_no, vec![Value::Bytes(path.as_bytes().to_vec())]);

    // 分页只对 `ls` 有意义；其余动作忽略 --page / --limit。
    if action == "ls" {
        for (name, value) in [("page", page), ("limit", limit)] {
            if let Some(v) = value {
                let no = schema
                    .field_no(&req_type, name)
                    .ok_or_else(|| format!("{req_type} 中没有字段 {name}"))?;
                m.insert(no, vec![Value::Varint(v)]);
            }
        }
    }
    Ok(m)
}
