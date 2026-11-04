use clap::{Parser, Subcommand};
use nascli::client::Client;
use nascli::commands::{emit_err, emit_ok, operation_of, parse_field, require_yes, Ctx};
use nascli::config::Config;
use nascli::schema::{is_read_only, Schema};
use nascli::wire::Message;
use std::collections::BTreeMap;
use std::process::ExitCode;

/// 构建期嵌入固件 schema：`build.rs` 已保证该文件存在。
const SCHEMA_JSON: &str = include_str!("../spec/schema.json");

#[derive(Parser)]
#[command(name = "nascli", version, about = "UGOS Pro OpenCLI 客户端")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 列出所有可用方法
    List {
        /// 按套接字名或方法路径子串过滤
        #[arg(long)]
        module: Option<String>,
        /// 只列只读方法
        #[arg(long)]
        read_only: bool,
    },
    /// 存储管理：磁盘 / 存储卷 / 存储池 / SMART
    Storage {
        /// 动作：disks | volumes | pools | smart
        #[arg(value_name = "ACTION")]
        action: String,
    },
    /// 系统信息与指标
    System {
        /// 动作：identity | update-status | cpu | mem | net | disk | volume | overview |
        /// services | processes | ups | power | fan | led | beep | cpu-config | zram
        #[arg(value_name = "ACTION")]
        action: String,
    },
    /// 网络配置
    Network {
        /// 动作：general | ifaces
        #[arg(value_name = "ACTION")]
        action: String,
    },
    /// Docker 管理：容器 / 镜像 / Compose / 占用检查
    Docker {
        /// 动作：containers | images | compose | logs | port-check | dir-check | usage |
        /// pull | start | stop | restart | compose-start | compose-stop | compose-restart |
        /// compose-delete | rebuild
        #[arg(value_name = "ACTION")]
        action: String,
        /// 确认执行写类操作
        #[arg(long)]
        yes: bool,
    },
    /// 应用中心：已安装应用列表 / 详情 / 状态
    Apps {
        /// 动作：list | detail | status
        #[arg(value_name = "ACTION")]
        action: String,
    },
    /// 用户与用户组
    Users {
        /// 动作：list | groups
        #[arg(value_name = "ACTION")]
        action: String,
    },
    /// 下载中心
    Download {
        /// 动作：tasks
        #[arg(value_name = "ACTION")]
        action: String,
    },
    /// 文件操作：浏览 / 属性 / 创建 / 删除 / 回收站
    Fs {
        /// 动作：stat | ls | cat | getfattr | recycle-status | path-perm | md5sum |
        /// sha256sum | mkdir | touch | echo | rename | cpmv | del | restore | chtimes |
        /// modify-acl | share-mkdir（du / find 为流式，不支持）
        #[arg(value_name = "ACTION")]
        action: String,
        /// 目标路径（单参数动作用）
        #[arg(long, default_value = "")]
        path: String,
        /// 分页页码（仅 ls）
        #[arg(long)]
        page: Option<u64>,
        /// 每页条数（仅 ls）
        #[arg(long)]
        limit: Option<u64>,
        /// 确认执行写类操作
        #[arg(long)]
        yes: bool,
    },
    /// 通用调用（兜底）
    Call {
        /// 套接字 ID，例如 storage
        socket: String,
        /// gRPC 方法路径，例如 /storage.Storage/VolumeList
        method: String,
        /// 请求字段，可重复，格式 <编号>:<类型>=<值>
        #[arg(long = "field", value_name = "N:TYPE=V")]
        fields: Vec<String>,
        /// 确认执行写类操作
        #[arg(long)]
        yes: bool,
    },
}

/// 构造执行上下文：配置缺失视为参数错误（2），客户端构建失败视为网络错误（6）。
fn build_ctx() -> Result<Ctx, ExitCode> {
    let cfg = Config::from_env().map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })?;
    let client = Client::new(cfg).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(6)
    })?;
    Ok(Ctx { client })
}

/// 执行只读命令：解析动作 → 调用 → 按响应类型翻译 → 输出 JSON。
///
/// system / network / apps / users / download 五个只读域共用；解析失败按参数错误（2）退出。
fn run_read(
    ctx: &Ctx,
    schema: &Schema,
    resolved: Result<(&'static str, &'static str), String>,
) -> ExitCode {
    let (socket, method) = match resolved {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match nascli::commands::system::run(&ctx.client, socket, method) {
        Ok(resp) => {
            let data = match schema.response_type(method) {
                Some(t) => schema.message_to_json(&t, &resp),
                // schema 缺该方法签名时退回按字段号裸转，至少不丢数据。
                None => nascli::schema::to_json(&resp, schema),
            };
            emit_ok(method, data);
            ExitCode::SUCCESS
        }
        Err(e) => ExitCode::from(emit_err(&e) as u8),
    }
}

/// 只读域的三元组 resolve 结果 → 二元组，并断言 operation 确实是 `read`。
///
/// `run_read` 不带 `--yes` 闸门；若某动作被误标为非 read，这里在调试构建下立即暴露，
/// 而不是把「只读」停留在注释里。
fn expect_read(
    r: Result<(&'static str, &'static str, &'static str), String>,
) -> Result<(&'static str, &'static str), String> {
    r.map(|(socket, method, op)| {
        debug_assert_eq!(op, "read", "只读域的动作被标记为非 read：{method}");
        (socket, method)
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let schema = match Schema::from_json_str(SCHEMA_JSON) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("内置 schema 解析失败：{e}");
            return ExitCode::from(7);
        }
    };

    match cli.command {
        Command::List { module, read_only } => {
            let mut rows = vec![];
            for (path, info) in schema.methods() {
                if read_only && !is_read_only(&info.operation) {
                    continue;
                }
                if let Some(m) = &module {
                    if !info.socket.contains(m.as_str()) && !path.contains(m.as_str()) {
                        continue;
                    }
                }
                rows.push(serde_json::json!({
                    "method": path,
                    "socket": info.socket,
                    "operation": info.operation,
                    "role": info.role,
                }));
            }
            emit_ok("list", serde_json::Value::Array(rows));
            ExitCode::SUCCESS
        }
        Command::Storage { action } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            let (_, method) = match nascli::commands::storage::resolve(&action) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            match nascli::commands::storage::run(&ctx.client, &action) {
                Ok(resp) => {
                    let data = match schema.response_type(method) {
                        Some(t) => schema.message_to_json(&t, &resp),
                        // schema 缺该方法签名时退回按字段号裸转，至少不丢数据。
                        None => nascli::schema::to_json(&resp, &schema),
                    };
                    emit_ok(method, data);
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
        Command::System { action } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            run_read(
                &ctx,
                &schema,
                nascli::commands::system::resolve_system(&action),
            )
        }
        Command::Network { action } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            run_read(
                &ctx,
                &schema,
                nascli::commands::system::resolve_network(&action),
            )
        }
        Command::Apps { action } => {
            // 目标参数闸门必须在建连之前：detail/status 的空请求必然失败，
            // 且服务端错误（not found app / Parameter error!）会误导归因。
            if let Err(e) =
                nascli::commands::apps::require_target(&action, &nascli::commands::apps::empty())
            {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            run_read(
                &ctx,
                &schema,
                expect_read(nascli::commands::apps::resolve(&action)),
            )
        }
        Command::Users { action } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            run_read(&ctx, &schema, nascli::commands::users::resolve(&action))
        }
        Command::Download { action } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            run_read(
                &ctx,
                &schema,
                expect_read(nascli::commands::download::resolve(&action)),
            )
        }
        Command::Docker { action, yes } => {
            // 先解析动作再判闸门：写类动作必须在建立连接前就被拒绝。
            let (socket, method, op) = match nascli::commands::docker::resolve(&action) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            if let Err(e) = require_yes(op, method, &action, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            // 目标参数闸门：写类动作必须有明确目标，空请求可能造成范围未限定的破坏；
            // 只读但需要 container_id / port / dir 的动作同样不该发空请求。
            let msg = nascli::commands::docker::empty();
            if let Err(e) = nascli::commands::docker::require_target(&action, &msg) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            match nascli::commands::docker::run(&ctx.client, socket, method, &msg) {
                Ok(resp) => {
                    let data = match schema.response_type(method) {
                        Some(t) => schema.message_to_json(&t, &resp),
                        // schema 缺该方法签名时退回按字段号裸转，至少不丢数据。
                        None => nascli::schema::to_json(&resp, &schema),
                    };
                    emit_ok(method, data);
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
        Command::Fs {
            action,
            path,
            page,
            limit,
            yes,
        } => {
            // 先解析动作再判闸门：写类动作必须在建立连接前就被拒绝。
            // du / find 是流式方法，解析阶段即失败，按「方法不支持」退出（5）。
            let (socket, method, op) = match nascli::commands::files::resolve(&action) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(5);
                }
            };
            if let Err(e) = require_yes(op, method, &path, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            // 目标参数闸门：单参数动作缺 --path 时不能发空请求。
            if let Err(e) = nascli::commands::files::require_target(&action, &path) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let msg = match nascli::commands::files::build_message(
                &schema, method, &action, &path, page, limit,
            ) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            match ctx.client.call(socket, method, &msg) {
                Ok(resp) => {
                    let data = match schema.response_type(method) {
                        Some(t) => schema.message_to_json(&t, &resp),
                        // schema 缺该方法签名时退回按字段号裸转，至少不丢数据。
                        None => nascli::schema::to_json(&resp, &schema),
                    };
                    emit_ok(method, data);
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
        Command::Call {
            socket,
            method,
            fields,
            yes,
        } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            // 未知方法按写操作处理（fail-closed），并明确告知原因。
            if schema.method(&method).is_none() {
                eprintln!("提示：{method} 不在内置 schema 中，无法判定读写属性，已按写操作处理");
            }
            let operation = operation_of(&schema, &method);
            let detail = fields.join(" ");
            if let Err(e) = require_yes(operation, &method, &detail, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let mut msg: Message = BTreeMap::new();
            for spec in &fields {
                match parse_field(spec) {
                    Ok((no, v)) => {
                        // 同号字段可重复传入，正好对应 protobuf 的 repeated 字段。
                        msg.entry(no).or_default().push(v);
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(2);
                    }
                }
            }
            match ctx.client.call(&socket, &method, &msg) {
                Ok(resp) => {
                    emit_ok(&method, nascli::schema::to_json(&resp, &schema));
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
    }
}
