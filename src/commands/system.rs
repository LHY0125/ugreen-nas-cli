//! `system` 与 `network` 命令域：系统指标、硬件配置与网络配置。
//!
//! 全部动作都是只读查询，因此不需要 `--yes` 闸门。
//! 两个域共用同一套套接字：`ugreen.taskmgr.internal.grpc`（任务/指标服务）
//! 与 `ugreen.ctlserv.internal.grpc`（控制/配置服务）。

use crate::client::{ApiError, Client};
use crate::wire::Message;
use std::collections::BTreeMap;

/// 任务与指标服务。
const TASK: &str = "ugreen.taskmgr.internal.grpc";
/// 控制与配置服务。
const CTL: &str = "ugreen.ctlserv.internal.grpc";

/// 把 system 子命令映射为 `(套接字, gRPC 方法)`。
pub fn resolve_system(action: &str) -> Result<(&'static str, &'static str), String> {
    let pair = match action {
        "identity" => (CTL, "/ctlserv.Internal/GetOsIdentity"),
        "update-status" => (CTL, "/ctlserv.Internal/GetOsUpdateStatus"),
        "cpu" => (TASK, "/taskserv.TaskManage/GetCpuMetric"),
        "mem" => (TASK, "/taskserv.TaskManage/GetMemMetric"),
        "net" => (TASK, "/taskserv.TaskManage/GetNetTraffic"),
        "disk" => (TASK, "/taskserv.TaskManage/GetDiskMetric"),
        "volume" => (TASK, "/taskserv.TaskManage/GetVolumeMetric"),
        "overview" => (TASK, "/taskserv.TaskManage/GetRunOverview"),
        "services" => (TASK, "/taskserv.TaskManage/GetServiceResource"),
        "processes" => (TASK, "/taskserv.TaskManage/GetProcessResource"),
        "ups" => (CTL, "/ctlserv.Internal/GetUpsConfig"),
        "power" => (CTL, "/ctlserv.Hardware/GetPowerConfig"),
        "fan" => (CTL, "/ctlserv.Hardware/GetFanConfig"),
        "led" => (CTL, "/ctlserv.Hardware/GetLedConfig"),
        "beep" => (CTL, "/ctlserv.Hardware/GetBeepConfig"),
        "cpu-config" => (CTL, "/ctlserv.Hardware/GetCpuConfig"),
        "zram" => (CTL, "/ctlserv.Hardware/GetZramConfig"),
        other => {
            return Err(format!(
                "未知的 system 子命令：{other}（可用：identity, update-status, cpu, mem, net, \
                 disk, volume, overview, services, processes, ups, power, fan, led, beep, \
                 cpu-config, zram）"
            ))
        }
    };
    Ok(pair)
}

/// 把 network 子命令映射为 `(套接字, gRPC 方法)`。
pub fn resolve_network(action: &str) -> Result<(&'static str, &'static str), String> {
    let pair = match action {
        "general" => (CTL, "/ctlserv.Network/GetGeneralConfig"),
        "ifaces" => (CTL, "/ctlserv.Network/GetIfaceList"),
        other => {
            return Err(format!(
                "未知的 network 子命令：{other}（可用：general, ifaces）"
            ))
        }
    };
    Ok(pair)
}

/// 执行只读查询，返回原始响应消息。
///
/// 字段号 → 字段名的翻译由 `main` 用 schema 完成，这里保持传输层纯粹。
pub fn run(client: &Client, socket: &str, method: &str) -> Result<Message, ApiError> {
    client.call(socket, method, &BTreeMap::new())
}
