use nascli::commands::files::{
    build_message, is_streaming_unsupported, require_target, resolve, resolve_error_exit_code,
};
use nascli::schema::Schema;
use nascli::wire::Value;

const SAMPLE: &str = r#"{
  "messages": {
    "ugidl.filesystem.cli.FileDirListRequest": [
      {"no": 3, "name": "path", "type": "string"},
      {"no": 7, "name": "page", "type": "int32"},
      {"no": 8, "name": "limit", "type": "int32"}
    ],
    "ugidl.filesystem.cli.StatRequest": [
      {"no": 1, "name": "path", "type": "string"}
    ],
    "ugidl.filesystem.cli.DelPathRequest": [
      {"no": 1, "name": "forever", "type": "bool"},
      {"no": 2, "name": "path", "type": "string"}
    ],
    "ugidl.filesystem.cli.DelPathRequestNoForever": [
      {"no": 2, "name": "path", "type": "string"}
    ],
    "ugidl.filesystem.cli.DuRequest": [
      {"no": 1, "name": "path", "type": "string"}
    ],
    "ugidl.filesystem.cli.Md5SumRequest": [
      {"no": 1, "name": "paths", "type": "string"}
    ]
  },
  "enums": {},
  "services": {
    "ugidl.filesystem.Filesystem": {
      "CategoryOrFileDirList": {"in": "ugidl.filesystem.cli.FileDirListRequest", "out": "x", "server_streaming": false},
      "Stat": {"in": "ugidl.filesystem.cli.StatRequest", "out": "x", "server_streaming": false},
      "DelPath": {"in": "ugidl.filesystem.cli.DelPathRequest", "out": "x", "server_streaming": false},
      "DelPathNoForever": {"in": "ugidl.filesystem.cli.DelPathRequestNoForever", "out": "x", "server_streaming": false},
      "Du": {"in": "ugidl.filesystem.cli.DuRequest", "out": "x", "server_streaming": true},
      "Md5sum": {"in": "ugidl.filesystem.cli.Md5SumRequest", "out": "x", "server_streaming": false}
    }
  },
  "methods": {}
}"#;

fn schema() -> Schema {
    Schema::from_json_str(SAMPLE).unwrap()
}

#[test]
fn ls_uses_unary_file_dir_list() {
    let (_, method, op) = resolve("ls").unwrap();
    assert_eq!(method, "/ugidl.filesystem.Filesystem/CategoryOrFileDirList");
    assert_eq!(op, "read");
}

#[test]
fn ls_resolves_path_field_from_schema() {
    let s = schema();
    let (_, method, _) = resolve("ls").unwrap();
    let m = build_message(&s, method, "ls", "/volume1/demo", Some(1), Some(50)).unwrap();
    assert_eq!(
        m.get(&3),
        Some(&vec![Value::Bytes(b"/volume1/demo".to_vec())])
    );
    assert_eq!(m.get(&7), Some(&vec![Value::Varint(1)]));
    assert_eq!(m.get(&8), Some(&vec![Value::Varint(50)]));
}

#[test]
fn stat_resolves_path_field_from_schema() {
    let s = schema();
    let (_, method, _) = resolve("stat").unwrap();
    let m = build_message(&s, method, "stat", "/volume1/demo", None, None).unwrap();
    assert_eq!(
        m.get(&1),
        Some(&vec![Value::Bytes(b"/volume1/demo".to_vec())])
    );
}

#[test]
fn multi_param_action_is_rejected() {
    let s = schema();
    let (_, method, _) = resolve("rename").unwrap();
    let e = build_message(&s, method, "rename", "/volume1/demo", None, None).unwrap_err();
    assert!(e.contains("nascli call"));
}

#[test]
fn du_uses_the_real_unary_method() {
    let (_, method, op) = resolve("du").unwrap();
    assert_eq!(method, "/ugidl.filesystem.Filesystem/Du");
    assert_eq!(op, "read");
}

#[test]
fn du_resolves_path_field_from_schema() {
    let s = schema();
    let (_, method, _) = resolve("du").unwrap();
    let m = build_message(&s, method, "du", "/volume1/demo", None, None).unwrap();
    assert_eq!(
        m.get(&1),
        Some(&vec![Value::Bytes(b"/volume1/demo".to_vec())])
    );
}

#[test]
fn find_is_reported_unsupported() {
    let e = resolve("find").unwrap_err();
    assert!(e.contains("流式"));
}

#[test]
fn delete_requires_yes_operation() {
    let (_, method, op) = resolve("del").unwrap();
    assert_eq!(method, "/ugidl.filesystem.Filesystem/DelPath");
    assert_eq!(op, "delete");
}

#[test]
fn rejects_unknown_action() {
    assert!(resolve("nope").is_err());
}

/// 流式方法与未知子命令必须映射到不同退出码：
/// 5 表示「方法不支持」，2 表示「参数错误」。混用会让智能体错误归因。
#[test]
fn streaming_actions_map_to_exit_code_5() {
    // du 现在真实调用，大目录的流式失败由 client 映射为 5；find 仍在本地拒绝。
    assert!(!is_streaming_unsupported("du"));
    assert!(is_streaming_unsupported("find"));
    assert_eq!(resolve_error_exit_code("find"), 5);
}

#[test]
fn unknown_action_maps_to_exit_code_2() {
    assert!(!is_streaming_unsupported("nope"));
    assert_eq!(resolve_error_exit_code("nope"), 2);
}

/// 写类动作必须落在非 read 的 operation 上，否则 `--yes` 闸门会被绕过。
#[test]
fn every_write_action_is_not_read() {
    for action in [
        "mkdir",
        "touch",
        "echo",
        "rename",
        "cpmv",
        "del",
        "restore",
        "chtimes",
        "modify-acl",
        "share-mkdir",
    ] {
        let (_, _, op) = resolve(action).unwrap();
        assert!(
            matches!(op, "write" | "delete"),
            "{action} 被标记为 {op}，写操作必须不是 read"
        );
    }
}

/// 单参数动作缺少 `--path` 时必须 fail-closed，不能把空路径发给服务端。
#[test]
fn empty_path_is_rejected_for_single_path_actions() {
    for action in ["stat", "ls", "cat", "getfattr", "del", "mkdir"] {
        let err = require_target(action, "").expect_err(&format!("{action} 应被拒绝"));
        assert!(err.contains("--path"), "action={action}：{err}");
    }
}

/// 写类动作的提示应带 `--yes`，只读动作不该出现，避免误导。
#[test]
fn path_hint_mentions_yes_only_for_writes() {
    let err = require_target("del", "").unwrap_err();
    assert!(err.contains("--yes"), "{err}");
    let err = require_target("stat", "").unwrap_err();
    assert!(!err.contains("--yes"), "{err}");
}

/// 多参数动作由 `build_message` 统一拒绝，不归路径闸门管。
#[test]
fn multi_param_action_is_not_gated_by_path() {
    assert!(require_target("rename", "").is_ok());
}

/// 路径非空时放行。
#[test]
fn non_empty_path_passes_target_gate() {
    assert!(require_target("stat", "/volume1/demo").is_ok());
}

#[test]
fn md5sum_uses_paths_field() {
    let s = schema();
    let (_, method, op) = resolve("md5sum").unwrap();
    assert_eq!(op, "read");
    let m = build_message(&s, method, "md5sum", "/volume1/demo/x", None, None).unwrap();
    assert_eq!(
        m.get(&1),
        Some(&vec![Value::Bytes(b"/volume1/demo/x".to_vec())])
    );
}

#[test]
fn del_without_forever_field_fails_closed() {
    let s = schema();
    let method = "/ugidl.filesystem.Filesystem/DelPathNoForever";
    let e = build_message(&s, method, "del", "/volume1/demo/x", None, None).unwrap_err();
    assert!(e.contains("forever"), "{e}");
    assert!(e.contains("回收站"), "{e}");
}

#[test]
fn del_explicitly_sends_forever_false() {
    let s = schema();
    let (_, method, _) = resolve("del").unwrap();
    let m = build_message(&s, method, "del", "/volume1/demo/x", None, None).unwrap();
    // 必须显式发送 forever=false，不能依赖服务端缺省值
    assert_eq!(
        m.get(&1),
        Some(&vec![Value::Varint(0)]),
        "del 必须显式发送 forever=0（移入回收站），实际: {m:?}"
    );
    assert_eq!(
        m.get(&2),
        Some(&vec![Value::Bytes(b"/volume1/demo/x".to_vec())])
    );
}
