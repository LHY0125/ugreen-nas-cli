use nascli::commands::{apps, download, users};
use nascli::wire::{Message, Value};
use std::collections::BTreeMap;

/// 套接字字面量：模块里的常量是公开的，但测试侧仍写字面量，
/// 顺便锁死套接字名本身（改错同样会红）。
const APP: &str = "ugreen.app.grpc";
const CTL: &str = "ugreen.ctlserv.internal.grpc";
const DOWNLOAD: &str = "ugreen.downloadserv.internal.grpc";

#[test]
fn maps_apps() {
    let cases = [
        ("list", "/app.App/DetailList"),
        ("detail", "/app.App/GetAppDetail"),
        ("status", "/app.App/GetAppStatus"),
    ];
    for (action, method) in cases {
        assert_eq!(
            apps::resolve(action).unwrap(),
            (APP, method, "read"),
            "action={action}"
        );
    }
}

#[test]
fn maps_users() {
    let cases = [
        ("list", "/ctlserv.Internal/GetUgreenUserList"),
        ("groups", "/ctlserv.Internal/GetCreatedGroupList"),
    ];
    for (action, method) in cases {
        assert_eq!(
            users::resolve(action).unwrap(),
            (CTL, method),
            "action={action}"
        );
    }
}

#[test]
fn maps_download() {
    let cases = [("tasks", "/download.DownloadService/BatchGetDownloadTask")];
    for (action, method) in cases {
        assert_eq!(
            download::resolve(action).unwrap(),
            (DOWNLOAD, method, "read"),
            "action={action}"
        );
    }
}

#[test]
fn rejects_unknown() {
    assert!(apps::resolve("nope").is_err());
    assert!(users::resolve("nope").is_err());
    assert!(download::resolve("nope").is_err());
}

/// 三个域都是只读：operation 必须是 `read`，否则 `main` 会误加 `--yes` 闸门。
#[test]
fn all_actions_are_read_only() {
    for action in ["list", "detail", "status"] {
        assert_eq!(apps::resolve(action).unwrap().2, "read", "apps {action}");
    }
    assert_eq!(download::resolve("tasks").unwrap().2, "read");
}

/// `detail` / `status` 需要应用 ID：空请求必须在建连前被拒，
/// 且提示指向 `nascli call`——apps 只读，提示里不能出现 `--yes`。
#[test]
fn rejects_apps_actions_that_need_a_target() {
    for action in ["detail", "status"] {
        let err = apps::require_target(action, &apps::empty()).unwrap_err();
        assert!(err.contains("nascli call"), "action={action}, err={err}");
        assert!(
            err.contains("--field 1:string="),
            "action={action}, err={err}"
        );
        assert!(!err.contains("--yes"), "只读域提示不应出现 --yes：{err}");
    }
}

/// `list` 不需要目标，空请求必须放行。
#[test]
fn allows_apps_list_without_target() {
    assert!(apps::require_target("list", &apps::empty()).is_ok());
}

/// 带目标字段时闸门放行（为将来参数化留出行为契约）。
#[test]
fn allows_apps_actions_when_target_present() {
    let mut msg: Message = BTreeMap::new();
    msg.insert(1, vec![Value::Bytes(b"com.example.app".to_vec())]);
    for action in ["detail", "status"] {
        assert!(
            apps::require_target(action, &msg).is_ok(),
            "action={action}"
        );
    }
}
