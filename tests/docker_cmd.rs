use nascli::commands::docker::{empty, require_target, resolve};
use nascli::wire::Value;

/// 套接字字面量：`docker.rs` 里的常量是公开的，但测试侧仍写字面量，
/// 顺便锁死套接字名本身（改错同样会红）。
const SOCKET: &str = "ugreen.dockerserv.internal.grpc";

#[test]
fn maps_read_actions() {
    assert_eq!(
        resolve("containers").unwrap(),
        (SOCKET, "/docker.Docker/ContainerList", "read")
    );
    assert_eq!(
        resolve("images").unwrap(),
        (SOCKET, "/docker.Docker/ShowLocalImage", "read")
    );
    assert_eq!(
        resolve("compose").unwrap(),
        (SOCKET, "/docker.Docker/GetComposeProjectList", "read")
    );
}

#[test]
fn maps_write_actions_with_operation() {
    let (_, method, op) = resolve("stop").unwrap();
    assert_eq!(method, "/docker.Docker/StopContainer");
    assert_eq!(op, "execute");
    let (_, _, op) = resolve("compose-delete").unwrap();
    assert_eq!(op, "delete");
}

#[test]
fn rejects_unknown_action() {
    assert!(resolve("nope").is_err());
}

/// 全部动作的表驱动映射，防止命令表与实现漂移。
#[test]
fn every_action_maps_to_expected_method_and_operation() {
    let cases = [
        ("containers", "/docker.Docker/ContainerList", "read"),
        ("images", "/docker.Docker/ShowLocalImage", "read"),
        ("compose", "/docker.Docker/GetComposeProjectList", "read"),
        ("logs", "/docker.Docker/GetContainerLogs", "read"),
        (
            "port-check",
            "/docker.Docker/CheckPortOccupiedByContainer",
            "read",
        ),
        (
            "dir-check",
            "/docker.Docker/CheckDirUsedByContainer",
            "read",
        ),
        ("usage", "/docker.Docker/GetAppUsage", "read"),
        ("pull", "/docker.Docker/PullImage", "write"),
        ("start", "/docker.Docker/StartContainer", "execute"),
        ("stop", "/docker.Docker/StopContainer", "execute"),
        ("restart", "/docker.Docker/RestartContainer", "execute"),
        (
            "compose-start",
            "/docker.Docker/StartComposeProject",
            "execute",
        ),
        (
            "compose-stop",
            "/docker.Docker/StopComposeProject",
            "execute",
        ),
        (
            "compose-restart",
            "/docker.Docker/RestartComposeProject",
            "execute",
        ),
        (
            "compose-delete",
            "/docker.Docker/DeleteComposeProject",
            "delete",
        ),
        ("rebuild", "/docker.Docker/RebuildContainerApp", "execute"),
    ];
    for (action, method, op) in cases {
        assert_eq!(
            resolve(action).unwrap(),
            (SOCKET, method, op),
            "action={action}"
        );
    }
}

/// 写类动作必须落在非 read 的 operation 上，否则 `--yes` 闸门会被绕过。
#[test]
fn write_actions_are_not_read() {
    for action in [
        "start",
        "stop",
        "restart",
        "pull",
        "compose-start",
        "compose-stop",
        "compose-restart",
        "compose-delete",
        "rebuild",
    ] {
        let (_, _, op) = resolve(action).unwrap();
        assert_ne!(op, "read", "写类动作被误标为只读：{action}");
    }
}

/// 写类动作没有目标参数时必须 fail-closed，不能把空请求当成「全部目标」。
#[test]
fn write_actions_without_target_are_rejected() {
    for action in [
        "start",
        "stop",
        "restart",
        "pull",
        "compose-start",
        "compose-stop",
        "compose-restart",
        "compose-delete",
        "rebuild",
    ] {
        let err = require_target(action, &empty()).expect_err(&format!("{action} 应被拒绝"));
        assert!(err.contains("需要目标参数"), "action={action}：{err}");
        assert!(err.contains("nascli call"), "action={action}：{err}");
        assert!(
            err.contains("--yes"),
            "写类动作的提示应带 --yes：{action}：{err}"
        );
    }
}

/// 只读但需要 container_id / port / dir 的动作同样不该发空请求。
#[test]
fn read_actions_needing_target_are_rejected() {
    for action in ["logs", "port-check", "dir-check"] {
        let err = require_target(action, &empty()).expect_err(&format!("{action} 应被拒绝"));
        assert!(err.contains("需要目标参数"), "action={action}：{err}");
        assert!(err.contains("nascli call"), "action={action}：{err}");
        // 只读动作不需要 --yes，提示里不应出现，避免误导。
        assert!(
            !err.contains("--yes"),
            "只读动作不应提示 --yes：{action}：{err}"
        );
    }
}

/// 不需要目标参数的动作保持放行。
#[test]
fn actions_without_target_requirement_pass() {
    for action in ["containers", "images", "compose", "usage"] {
        assert!(
            require_target(action, &empty()).is_ok(),
            "action={action} 不应被目标闸门拦截"
        );
    }
}

/// 请求体非空时放行，为将来的参数化留出口。
#[test]
fn non_empty_request_passes_target_gate() {
    let mut msg = empty();
    msg.insert(1, vec![Value::Bytes(b"demo".to_vec())]);
    assert!(require_target("stop", &msg).is_ok());
}

/// 示例字段类型必须与 schema 一致：只有 `port-check` 是 int，其余是 string。
///
/// 提示照抄即执行，类型写错会让用户拿到「不是合法整数」之类的参数错误。
#[test]
fn target_hint_uses_correct_field_type() {
    let err = require_target("port-check", &empty()).unwrap_err();
    assert!(
        err.contains("--field 1:int=<端口号>"),
        "port-check 的示例应为 int：{err}"
    );

    for action in [
        "logs",
        "dir-check",
        "start",
        "stop",
        "restart",
        "pull",
        "compose-start",
        "compose-stop",
        "compose-restart",
        "compose-delete",
        "rebuild",
    ] {
        let err = require_target(action, &empty()).unwrap_err();
        assert!(
            err.contains("--field 1:string="),
            "action={action} 的示例应为 string：{err}"
        );
    }
}
