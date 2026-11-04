use nascli::commands::system::{resolve_network, resolve_system};

/// 套接字字面量：`system.rs` 里的常量是私有的，测试侧直接写字面量，
/// 顺便锁死套接字名本身（改错同样会红）。
const TASK: &str = "ugreen.taskmgr.internal.grpc";
const CTL: &str = "ugreen.ctlserv.internal.grpc";

#[test]
fn every_system_action_maps_to_expected_method() {
    let cases = [
        ("identity", CTL, "/ctlserv.Internal/GetOsIdentity"),
        ("update-status", CTL, "/ctlserv.Internal/GetOsUpdateStatus"),
        ("cpu", TASK, "/taskserv.TaskManage/GetCpuMetric"),
        ("mem", TASK, "/taskserv.TaskManage/GetMemMetric"),
        ("net", TASK, "/taskserv.TaskManage/GetNetTraffic"),
        ("disk", TASK, "/taskserv.TaskManage/GetDiskMetric"),
        ("volume", TASK, "/taskserv.TaskManage/GetVolumeMetric"),
        ("overview", TASK, "/taskserv.TaskManage/GetRunOverview"),
        ("services", TASK, "/taskserv.TaskManage/GetServiceResource"),
        ("processes", TASK, "/taskserv.TaskManage/GetProcessResource"),
        ("ups", CTL, "/ctlserv.Internal/GetUpsConfig"),
        ("power", CTL, "/ctlserv.Hardware/GetPowerConfig"),
        ("fan", CTL, "/ctlserv.Hardware/GetFanConfig"),
        ("led", CTL, "/ctlserv.Hardware/GetLedConfig"),
        ("beep", CTL, "/ctlserv.Hardware/GetBeepConfig"),
        ("cpu-config", CTL, "/ctlserv.Hardware/GetCpuConfig"),
        ("zram", CTL, "/ctlserv.Hardware/GetZramConfig"),
    ];
    for (action, socket, method) in cases {
        assert_eq!(
            resolve_system(action).unwrap(),
            (socket, method),
            "action={action}"
        );
    }
}

#[test]
fn every_network_action_maps_to_expected_method() {
    let cases = [
        ("general", CTL, "/ctlserv.Network/GetGeneralConfig"),
        ("ifaces", CTL, "/ctlserv.Network/GetIfaceList"),
    ];
    for (action, socket, method) in cases {
        assert_eq!(
            resolve_network(action).unwrap(),
            (socket, method),
            "action={action}"
        );
    }
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
