//! 命令表与内置 schema 的一致性检查。
//!
//! `spec/schema.json` 是构建期嵌入的固件快照；命令表是手写的。固件升级后
//! 两者可能漂移。本测试只在 schema 存在时运行（仓库默认 gitignore 该文件），
//! 用来提前发现方法名、套接字或读写属性不一致。

use nascli::commands::{apps, docker, download, files, storage, system, users};
use nascli::schema::Schema;
use std::path::PathBuf;

fn load_schema() -> Option<Schema> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/schema.json");
    let text = std::fs::read_to_string(path).ok()?;
    Some(Schema::from_json_str(&text).expect("spec/schema.json 解析失败"))
}

#[test]
fn command_tables_match_schema() {
    let Some(schema) = load_schema() else {
        eprintln!("跳过：spec/schema.json 不存在");
        return;
    };

    let mut checks: Vec<(&str, &str, &'static str, &'static str, &'static str)> = vec![];

    for action in ["disks", "volumes", "pools", "smart"] {
        let (sock, method) = storage::resolve(action).unwrap();
        checks.push(("storage", action, sock, method, "read"));
    }
    for action in [
        "identity",
        "update-status",
        "cpu",
        "mem",
        "net",
        "disk",
        "volume",
        "overview",
        "services",
        "processes",
        "ups",
        "power",
        "fan",
        "led",
        "beep",
        "cpu-config",
        "zram",
    ] {
        let (sock, method) = system::resolve_system(action).unwrap();
        checks.push(("system", action, sock, method, "read"));
    }
    for action in ["general", "ifaces"] {
        let (sock, method) = system::resolve_network(action).unwrap();
        checks.push(("network", action, sock, method, "read"));
    }
    for action in [
        "containers",
        "images",
        "compose",
        "logs",
        "port-check",
        "dir-check",
        "usage",
        "pull",
        "start",
        "stop",
        "restart",
        "compose-start",
        "compose-stop",
        "compose-restart",
        "compose-delete",
        "rebuild",
    ] {
        let (sock, method, op) = docker::resolve(action).unwrap();
        checks.push(("docker", action, sock, method, op));
    }
    for action in ["list", "detail", "status"] {
        let (sock, method, op) = apps::resolve(action).unwrap();
        checks.push(("apps", action, sock, method, op));
    }
    for action in ["list", "groups"] {
        let (sock, method) = users::resolve(action).unwrap();
        checks.push(("users", action, sock, method, "read"));
    }
    {
        let action = "tasks";
        let (sock, method, op) = download::resolve(action).unwrap();
        checks.push(("download", action, sock, method, op));
    }

    // path-perm / modify-acl 在 proto 描述符里有签名，但不在 openapi 的
    // 101 个方法清单中；它们由 fs 分支单独处理，这里显式登记例外。
    let unregistered = [
        "/ugidl.filesystem.Filesystem/GetPathPermission",
        "/ugidl.filesystem.Filesystem/ModifyPathAcl",
    ];
    for action in files::SINGLE_PATH.iter().chain(files::NEEDS_CALL.iter()) {
        let (sock, method, op) = files::resolve(action).unwrap();
        if unregistered.contains(&method) {
            assert!(schema.method(method).is_none(), "{method} 不应登记");
            assert!(
                schema.request_type(method).is_some(),
                "{method} 应有请求类型"
            );
            continue;
        }
        checks.push(("fs", action, sock, method, op));
    }

    for (domain, action, socket, method, operation) in checks {
        let info = schema
            .method(method)
            .unwrap_or_else(|| panic!("{domain} {action}: {method} 不在 schema.methods 中"));
        assert_eq!(info.socket, socket, "{domain} {action}: 套接字不一致");
        assert_eq!(
            info.operation, operation,
            "{domain} {action}: 读写属性不一致（schema={}, cli={}）",
            info.operation, operation
        );
    }
}

#[test]
fn force_write_methods_are_upstream_read() {
    let Some(schema) = load_schema() else {
        eprintln!("跳过：spec/schema.json 不存在");
        return;
    };
    for method in nascli::commands::FORCED_WRITE_METHODS {
        let info = schema
            .method(method)
            .unwrap_or_else(|| panic!("{method} 不在 schema.methods 中"));
        assert_eq!(
            info.operation, "read",
            "{method} 已不再是上游 read，可从强制写名单移除"
        );
    }
}

#[test]
fn fs_path_parameters_exist_in_schema() {
    let Some(schema) = load_schema() else {
        eprintln!("跳过：spec/schema.json 不存在");
        return;
    };
    for action in files::SINGLE_PATH {
        let (_, method, _) = files::resolve(action).unwrap();
        let Some(req_type) = schema.request_type(method) else {
            panic!("{action}: {method} 缺少请求类型");
        };
        let field = match *action {
            "md5sum" | "sha256sum" => "paths",
            _ => "path",
        };
        assert!(
            schema.field_no(&req_type, field).is_some(),
            "{action}: {req_type} 缺少字段 {field}"
        );
        if *action == "del" {
            assert!(
                schema.field_no(&req_type, "forever").is_some(),
                "del: {req_type} 缺少 forever 字段"
            );
        }
    }
}
