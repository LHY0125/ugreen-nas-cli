use nascli::commands::docker::resolve;

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
