use nascli::commands::system::{resolve_network, resolve_system};

#[test]
fn maps_system_actions() {
    assert_eq!(
        resolve_system("cpu").unwrap(),
        (
            "ugreen.taskmgr.internal.grpc",
            "/taskserv.TaskManage/GetCpuMetric"
        )
    );
    assert_eq!(
        resolve_system("identity").unwrap(),
        (
            "ugreen.ctlserv.internal.grpc",
            "/ctlserv.Internal/GetOsIdentity"
        )
    );
    assert_eq!(
        resolve_system("ups").unwrap(),
        (
            "ugreen.ctlserv.internal.grpc",
            "/ctlserv.Internal/GetUpsConfig"
        )
    );
    assert_eq!(
        resolve_system("fan").unwrap(),
        (
            "ugreen.ctlserv.internal.grpc",
            "/ctlserv.Hardware/GetFanConfig"
        )
    );
}

#[test]
fn maps_network_actions() {
    assert_eq!(
        resolve_network("general").unwrap(),
        (
            "ugreen.ctlserv.internal.grpc",
            "/ctlserv.Network/GetGeneralConfig"
        )
    );
    assert_eq!(
        resolve_network("ifaces").unwrap(),
        (
            "ugreen.ctlserv.internal.grpc",
            "/ctlserv.Network/GetIfaceList"
        )
    );
}

#[test]
fn rejects_unknown() {
    assert!(resolve_system("nope").is_err());
    assert!(resolve_network("nope").is_err());
}

/// 命令表里的每个动作都必须能解析，避免文档与实现漂移。
#[test]
fn resolves_every_documented_action() {
    let system = [
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
    ];
    for action in system {
        assert!(
            resolve_system(action).is_ok(),
            "system 动作未映射：{action}"
        );
    }
    for action in ["general", "ifaces"] {
        assert!(
            resolve_network(action).is_ok(),
            "network 动作未映射：{action}"
        );
    }
}
