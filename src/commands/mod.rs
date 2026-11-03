// 各命令模块由后续任务逐个加入，未加入的模块不能提前声明，否则无法编译：
//   已加入: pub mod storage;（Task 8）
//   待加入: Task 9: pub mod system;
//   Task 10: pub mod docker;   Task 11: pub mod apps; pub mod users; pub mod download;
//   Task 12: pub mod files;

pub mod storage;

use crate::client::{ApiError, Client};
use crate::schema::{is_read_only, Schema};
use crate::wire::Value;
use serde_json::json;

/// 命令执行上下文。schema 由 `main` 持有并按需传入，故不放在这里。
pub struct Ctx {
    pub client: Client,
}

/// 成功结果：stdout 只输出这一行 JSON。
pub fn emit_ok(method: &str, data: serde_json::Value) {
    println!("{}", json!({"ok": true, "method": method, "data": data}));
}

/// 失败结果：同样输出 JSON 到 stdout，返回对应退出码。
pub fn emit_err(e: &ApiError) -> i32 {
    println!(
        "{}",
        json!({"ok": false, "error": {
            "code": e.code(),
            "message": e.message(),
            "exit_code": e.exit_code()
        }})
    );
    e.exit_code()
}

/// 解析 `--field <no>:<type>=<value>`。
pub fn parse_field(spec: &str) -> Result<(u32, Value), String> {
    let (head, value) = spec
        .split_once('=')
        .ok_or_else(|| format!("字段格式应为 <编号>:<类型>=<值>，收到：{spec}"))?;
    let (no, kind) = head
        .split_once(':')
        .ok_or_else(|| format!("字段格式应为 <编号>:<类型>=<值>，收到：{spec}"))?;
    let no: u32 = no.parse().map_err(|_| format!("字段编号不是数字：{no}"))?;
    let v = match kind {
        "string" => Value::Bytes(value.as_bytes().to_vec()),
        "int" => Value::Varint(
            value
                .parse::<u64>()
                .map_err(|_| format!("不是合法整数：{value}"))?,
        ),
        "bool" => Value::Varint(match value {
            "true" | "1" => 1,
            "false" | "0" => 0,
            other => return Err(format!("不是合法布尔值：{other}")),
        }),
        other => return Err(format!("不支持的类型：{other}（可用 string/int/bool）")),
    };
    Ok((no, v))
}

/// 查方法的 operation，**未知方法一律按 `write` 处理**。
///
/// `call` 的兜底场景正是「固件升级出现了新方法」——此时读写属性不可知。
/// fail-open 会绕过 `--yes` 闸门，因此这里选择 fail-closed：宁可多确认一次，
/// 也不能把未知的写操作当只读放行。
pub fn operation_of<'a>(schema: &'a Schema, method: &str) -> &'a str {
    schema
        .method(method)
        .map(|i| i.operation.as_str())
        .unwrap_or("write")
}

/// 写类操作的确认闸门：`read` 直接放行，其余必须显式 `--yes`。
pub fn require_yes(operation: &str, method: &str, detail: &str, yes: bool) -> Result<(), String> {
    if is_read_only(operation) || yes {
        return Ok(());
    }
    let mut msg = format!("该操作会修改 NAS 状态，已被拒绝\n  method:  {method}   [{operation}]");
    if !detail.is_empty() {
        msg.push_str(&format!("\n  将执行:  {detail}"));
    }
    msg.push_str("\n  如确认执行，请重试并加上 --yes");
    Err(msg)
}
