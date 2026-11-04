use clap::{Parser, Subcommand};
use nascli::client::Client;
use nascli::commands::{
    emit_err, emit_ok, is_forced_write, operation_of, parse_field, require_yes, Ctx,
};
use nascli::config::Config;
use nascli::schema::{is_read_only, Schema};
use nascli::wire::Message;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// 构建期嵌入固件 schema：`build.rs` 已保证该文件存在。
const SCHEMA_JSON: &str = include_str!("../spec/schema.json");

#[derive(Parser)]
#[command(name = "nascli", version, about = "UGOS Pro OpenCLI 客户端")]
struct Cli {
    /// TOML 配置文件路径（默认当前目录的 config.toml）
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = "config.toml"
    )]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 配置文件：初始化 / 查看 / 修改
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
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
        /// 动作：stat | ls | cat | getfattr | recycle-status | md5sum | sha256sum |
        /// du | mkdir | touch | echo | rename | cpmv | del（默认移入回收站） |
        /// restore | chtimes | modify-acl | share-mkdir（find 为流式，不支持；
        /// path-perm 当前固件未登记；du 大目录可能返回 5）
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

#[derive(Subcommand)]
enum ConfigAction {
    /// 交互式创建配置；--force 覆盖已有文件
    Init {
        #[arg(long)]
        force: bool,
    },
    /// 显示当前配置（token 脱敏）
    Show,
    /// 修改单个字段：host / port / token / insecure
    Set { key: String, value: Option<String> },
    /// 交互式修改配置
    Edit,
    /// 显示配置文件路径
    Path,
}

fn run_config(path: &Path, action: ConfigAction) -> ExitCode {
    let fail = |e: nascli::config::ConfigError| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    match action {
        ConfigAction::Init { force } => match Config::init_interactive(path, force) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => fail(e),
        },
        ConfigAction::Show => match Config::from_file(path) {
            Ok(cfg) => {
                eprintln!("配置文件：{}", path.display());
                print!("{}", cfg.redacted_toml());
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        ConfigAction::Set { key, value } => {
            let mut cfg = match Config::from_file(path) {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let value = match value {
                Some(v) => v,
                None if key.eq_ignore_ascii_case("token") => match Config::prompt_value("token") {
                    Ok(v) if !v.is_empty() => v,
                    Ok(_) => {
                        eprintln!("token 不能为空");
                        return ExitCode::from(2);
                    }
                    Err(e) => return fail(e),
                },
                None => {
                    eprintln!("缺少 value；用法：nascli config set {key} <value>");
                    return ExitCode::from(2);
                }
            };
            if let Err(e) = cfg.set_field(&key, &value) {
                return fail(e);
            }
            match cfg.write_file(path) {
                Ok(()) => {
                    eprintln!("配置已更新 {}", path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        ConfigAction::Edit => match Config::edit_interactive(path) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => fail(e),
        },
        ConfigAction::Path => {
            let shown = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let text = shown.display().to_string();
            // Windows canonicalize 会加 \\?\ 前缀；对用户展示时去掉。
            println!("{}", text.strip_prefix(r"\\?\").unwrap_or(&text));
            ExitCode::SUCCESS
        }
    }
}

/// 构造执行上下文：配置缺失视为参数错误（2），客户端构建失败视为网络错误（6）。
fn build_ctx(config_path: &Path) -> Result<Ctx, ExitCode> {
    let cfg = Config::load_or_init(config_path).map_err(|e| {
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
    // 运行时 fail-closed：只读域不带 --yes 闸门，因此必须确认 schema 里该方法确实是 read。
    // 若有人把写方法加进只读域，这里会拒绝而不是静默放行（release 构建同样生效）。
    if nascli::commands::is_forced_write(method) {
        eprintln!("内部错误：{method} 是本地强制写方法，不能放在只读命令域；已拒绝执行");
        return ExitCode::from(2);
    }
    match schema.method(method) {
        Some(info) if nascli::schema::is_read_only(&info.operation) => {}
        Some(info) => {
            eprintln!(
                "内部错误：{method} 在只读命令域中，但 schema 标记为 [{}]；已拒绝执行",
                info.operation
            );
            return ExitCode::from(2);
        }
        None => {
            eprintln!("内部错误：schema 中找不到 {method}；已拒绝执行");
            return ExitCode::from(2);
        }
    }
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
    let config_path = cli.config;
    let schema = match Schema::from_json_str(SCHEMA_JSON) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("内置 schema 解析失败：{e}");
            return ExitCode::from(7);
        }
    };

    match cli.command {
        Command::Config { action } => run_config(&config_path, action),
        Command::List { module, read_only } => {
            let mut rows = vec![];
            for (path, info) in schema.methods() {
                // 上游权限表可能把会改状态的方法标成 read，本地强制写名单优先。
                let forced = is_forced_write(path);
                let operation = if forced {
                    "write"
                } else {
                    info.operation.as_str()
                };
                if read_only && !is_read_only(operation) {
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
                    "operation": operation,
                    "declared_operation": info.operation,
                    "forced_write": forced,
                    "role": info.role,
                    // 流式方法不被 HTTP 接口支持，标注出来避免智能体反复试错
                    "server_streaming": schema.is_streaming(path),
                    "supported": !schema.is_streaming(path),
                }));
            }
            emit_ok("list", serde_json::Value::Array(rows));
            ExitCode::SUCCESS
        }
        Command::Storage { action } => {
            let ctx = match build_ctx(&config_path) {
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
            let ctx = match build_ctx(&config_path) {
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
            let ctx = match build_ctx(&config_path) {
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
            let ctx = match build_ctx(&config_path) {
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
            let ctx = match build_ctx(&config_path) {
                Ok(c) => c,
                Err(code) => return code,
            };
            run_read(&ctx, &schema, nascli::commands::users::resolve(&action))
        }
        Command::Download { action } => {
            let ctx = match build_ctx(&config_path) {
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
            let ctx = match build_ctx(&config_path) {
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
            // 先解析动作：du / find 是流式方法，按「方法不支持」退出（5）；
            // 拼错的子命令是参数错误（2），不能混为一谈。
            let (socket, method, op) = match nascli::commands::files::resolve(&action) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(nascli::commands::files::resolve_error_exit_code(
                        &action,
                    ));
                }
            };
            // 只读动作必须能在 schema 里查到并确认是 read；查不到说明固件未登记，
            // 不能因为动作表里写着 read 就直接发请求（path-perm 就是这种情况）。
            if op == "read" {
                match schema.method(method) {
                    Some(info) if is_read_only(&info.operation) => {}
                    Some(info) => {
                        eprintln!(
                            "内部错误：{method} 在 fs 只读动作中，但 schema 标记为 [{}]；已拒绝执行",
                            info.operation
                        );
                        return ExitCode::from(2);
                    }
                    None => {
                        eprintln!(
                            "{method} 未在内置 schema 中登记，无法确认只读属性；已拒绝执行（退出码 5）"
                        );
                        return ExitCode::from(5);
                    }
                }
            }
            // 先判断动作能否用 --path 参数化：多参数动作直接引导 nascli call。
            // 若放在 require_yes 之后，用户会先被要求 --yes、加上后又被拒绝，形成死胡同。
            let msg = match nascli::commands::files::build_message(
                &schema, method, &action, &path, page, limit,
            ) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            // del 发送 path + 显式 forever=false——默认移入回收站，不是永久删除。
            let detail = if action == "del" && !path.is_empty() {
                format!("{path}（默认移入回收站，不永久删除）")
            } else {
                path.clone()
            };
            // 目标参数闸门先于 --yes：缺路径时直接告诉用户缺什么，避免
            // “先提示加 --yes，加了才提示缺 --path”的两步死胡同。
            if let Err(e) = nascli::commands::files::require_target(&action, &path) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            if let Err(e) = require_yes(op, method, &detail, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let ctx = match build_ctx(&config_path) {
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
            let ctx = match build_ctx(&config_path) {
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
                    // 已知方法按 schema 的响应类型解码，输出字段名而不是 field_N；
                    // 只有 schema 缺签名时才退回按字段号裸转。
                    let data = match schema.response_type(&method) {
                        Some(t) => schema.message_to_json(&t, &resp),
                        None => nascli::schema::to_json(&resp, &schema),
                    };
                    emit_ok(&method, data);
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
    }
}
