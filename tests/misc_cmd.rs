use nascli::commands::{apps, download, users};

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
