#!/usr/bin/env python3
"""从绿联官方二进制中提取 protobuf 描述符，生成 spec/schema.json。

只读操作：仅通过 SSH 读取 NAS 上的文件，不修改任何配置。
产出结构见 docs/design.md §5.1，必须与 src/schema.rs 的 FieldDef / MethodSig /
MethodInfo 对齐——尤其是 `type` 必须是字符串类型名，而不是描述符里的数字编码。
"""

from __future__ import annotations

import argparse
import base64
import glob
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

PROTO_RE = re.compile(rb"[a-zA-Z0-9_/.-]{3,80}\.proto")
# FieldDescriptorProto.label 的取值：1=optional、2=required、3=repeated。
# proto3 里普通字段一律是 optional(1)，只有 repeated 才是 3。
LABEL_REPEATED = 3
MAX_DESC_FIELD = 20  # FileDescriptorProto 最大字段号（edition=14），超过即视为越界
MAX_DESC_WINDOW = 1 << 20  # 描述符解析窗口上限（是上限而非预期长度，正常描述符远小于此）
MIN_BINARY_SIZE = 200_000  # 小于此体积的文件不可能内嵌描述符
SSH_TIMEOUT = 600  # 单次远端扫描的超时（秒）

# 应用目录与二进制/清单的布局（服务端 sbin、CLI bin、应用 openapi.json）
APP_ROOTS = ("/ugreen/@appstore", "/volume1/@appstore")
BIN_PATTERNS = ("*/sbin/*", "*/bin/*", "*/cli/bin/*")
OPENAPI_PATTERNS = ("*/openapi.json", "openapi.json")
SKIP_NAME_PREFIXES = ("google/",)

# FileDescriptorProto 各字段的 wire type；字段表外或类型不符即说明已越过描述符末尾。
FDP_FIELD_WIRES: dict[int, int] = {
    1: 2,   # name
    2: 2,   # package
    3: 2,   # dependency
    4: 2,   # message_type
    5: 2,   # enum_type
    6: 2,   # service
    7: 2,   # extension
    8: 2,   # options
    9: 2,   # source_code_info
    10: 0,  # public_dependency
    11: 0,  # weak_dependency
    12: 2,  # syntax
    14: 0,  # edition
}

# 有效描述符至少应含有其中一个字段：package / message_type / enum_type / service / syntax。
# 只有 name 的字节串不构成描述符，不能仅凭 name 匹配就接受。
DESCRIPTOR_MARKER_FIELDS = frozenset({2, 4, 5, 6, 12})

# descriptor.proto 的字段声明顺序。protoc 按声明顺序序列化，public/weak_dependency
# (10/11) 会排在 message_type(4) 之前，所以判断「回退」要用声明顺序而不是字段号。
FDP_DECL_ORDER: dict[int, int] = {
    1: 0,
    2: 1,
    3: 2,
    10: 3,
    11: 4,
    4: 5,
    5: 6,
    6: 7,
    7: 8,
    8: 9,
    9: 10,
    12: 11,
    14: 12,
}

# FieldDescriptorProto.type 的数字编码 → 字符串类型名。
# schema.json 里必须是字符串：src/schema.rs 的 FieldDef.kind 是 String。
PROTO_TYPE_NAMES: dict[int, str] = {
    1: "double",
    2: "float",
    3: "int64",
    4: "uint64",
    5: "int32",
    6: "fixed64",
    7: "fixed32",
    8: "bool",
    9: "string",
    10: "group",
    11: "message",
    12: "bytes",
    13: "uint32",
    14: "enum",
    15: "sfixed32",
    16: "sfixed64",
    17: "sint32",
    18: "sint64",
}

_TARGET_RE = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_.\-]*$")


# ---- protobuf 解析 ----


def read_varint(buf: bytes, i: int, end: int | None = None) -> tuple[int, int] | None:
    """读取一个 varint，返回 (值, 新下标)；越界或超长返回 None。"""
    limit = len(buf) if end is None else min(end, len(buf))
    result = 0
    shift = 0
    while True:
        if i >= limit:
            return None
        byte = buf[i]
        i += 1
        result |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return result, i
        shift += 7
        if shift >= 64:
            return None


def _parse_message(
    buf: bytes,
    i: int,
    end: int,
    wires: dict[int, int] | None = None,
    strict_ordering: bool = False,
) -> tuple[dict[int, list], int, bool]:
    """解析 protobuf 消息，同时返回消费到的下标。遇到非法字段号或越界即停止。

    `wires` 给定字段表时，还会在字段表外或 wire type 不符处停止——
    描述符在二进制里没有长度前缀，只能靠这一条判断真实边界。
    """
    out: dict[int, list] = {}
    truncated = False
    last_rank = -1
    end = min(end, len(buf))
    while i < end:
        got = read_varint(buf, i, end)
        if got is None:
            break
        key, i = got
        field, wire = key >> 3, key & 7
        if field == 0 or field > MAX_DESC_FIELD:
            break
        if wires is not None and wires.get(field) != wire:
            break
        if strict_ordering:
            # 第二段描述符必以 field 1 开头：name 重复或声明顺序回退即停止。
            if field == 1 and 1 in out:
                break
            rank = FDP_DECL_ORDER[field]
            if rank < last_rank:
                break
            last_rank = rank
        if wire == 0:
            got = read_varint(buf, i, end)
            if got is None:
                break
            value, i = got
        elif wire == 2:
            got = read_varint(buf, i, end)
            if got is None:
                break
            length, i = got
            if i + length > end:
                truncated = True
                break
            value = buf[i : i + length]
            i += length
        elif wire == 5:
            if i + 4 > end:
                truncated = True
                break
            value = buf[i : i + 4]
            i += 4
        elif wire == 1:
            if i + 8 > end:
                truncated = True
                break
            value = buf[i : i + 8]
            i += 8
        else:
            break
        out.setdefault(field, []).append(value)
    return out, i, truncated


def parse_message(buf: bytes, i: int, end: int) -> dict[int, list]:
    """解析 protobuf 消息。遇到非法字段号或越界长度即停止。"""
    return _parse_message(buf, i, end)[0]


def _parse_descriptor(buf: bytes, i: int, end: int) -> tuple[dict[int, list], int, bool]:
    """按 FileDescriptorProto 字段表解析，wire type 不符即停止。"""
    return _parse_message(buf, i, end, FDP_FIELD_WIRES, strict_ordering=True)


def _is_bytes(value: Any) -> bool:
    """wire type 2 的值是 bytes；其余（varint / fixed）不是子消息。"""
    return isinstance(value, (bytes, bytearray))


def _submessages(values: list) -> list[bytes]:
    """只保留 wire type 2 的子消息字节串，避免把 varint 当成子消息解析。"""
    return [value for value in values if _is_bytes(value)]


def _name_pattern(name: bytes) -> bytes:
    """构造 `0x0A <len> <name>` 的字节串，即 FileDescriptorProto.name 字段。"""
    length = len(name)
    if length < 128:
        return bytes([0x0A, length]) + name
    return bytes([0x0A, (length & 0x7F) | 0x80, length >> 7]) + name


def find_descriptors(blob: bytes, names: list[str]) -> dict[str, dict[int, list]]:
    """在二进制中定位并解析描述符。解析结果必须自证（name 匹配）才算命中。"""
    found: dict[str, dict[int, list]] = {}
    for name in names:
        raw_name = name.encode("utf-8") if isinstance(name, str) else name
        if not raw_name.endswith(b".proto"):
            continue
        pattern = _name_pattern(raw_name)
        start = 0
        while True:
            at = blob.find(pattern, start)
            if at < 0:
                break
            window_end = min(at + MAX_DESC_WINDOW, len(blob))
            parsed, consumed, truncated = _parse_descriptor(blob, at, window_end)
            got = parsed.get(1)
            # 仅凭 name 匹配是恒真断言（搜索模式本身就含 name），必须再确认
            # 真的解析出了描述符的其它字段，否则裸串会挡住后面真正的描述符。
            if got and got[0] == raw_name and set(parsed) & DESCRIPTOR_MARKER_FIELDS:
                if truncated or (consumed >= window_end and window_end < len(blob)):
                    print(
                        f"警告：{name} 的解析被截断（窗口上限 {MAX_DESC_WINDOW} 字节），结果可能不完整",
                        file=sys.stderr,
                    )
                found[name] = parsed
                break
            start = at + 1
    return found


def _text(value: Any) -> str:
    return value.decode("utf-8", "replace") if isinstance(value, bytes) else str(value)


def type_name_of(fd: dict[int, list]) -> str:
    """把 FieldDescriptorProto 的类型编码翻成字符串类型名。"""
    raw = fd.get(5)
    if raw:
        return PROTO_TYPE_NAMES.get(raw[0], f"unknown_{raw[0]}")
    if fd.get(6):
        return "message"
    return "unknown"


# ---- 描述符 → schema 片段 ----


def _fields_of(md: dict[int, list]) -> list[dict]:
    fields: list[dict] = []
    for raw_field in _submessages(md.get(2, [])):
        fd = parse_message(raw_field, 0, len(raw_field))
        name = fd.get(1)
        number = fd.get(3)
        if not name or not number:
            continue
        type_name = fd.get(6)
        label = fd.get(4)
        fields.append(
            {
                "no": number[0],
                "name": _text(name[0]),
                "type": type_name_of(fd),
                "type_name": _text(type_name[0]).lstrip(".") if type_name else None,
                # repeated 标量在 proto3 里默认 packed 编码，CLI 需要该标记才能解包
                "repeated": bool(label) and label[0] == LABEL_REPEATED,
            }
        )
    return fields


def _add_enum(ed: dict[int, list], prefix: str, enums: dict[str, dict[str, int]]) -> None:
    name = ed.get(1)
    if not name:
        return
    fqn = f"{prefix}.{_text(name[0])}" if prefix else _text(name[0])
    values: dict[str, int] = {}
    for raw_value in _submessages(ed.get(2, [])):
        vd = parse_message(raw_value, 0, len(raw_value))
        symbol = vd.get(1)
        number = vd.get(2)
        if not symbol or not number:
            continue
        values[_text(symbol[0])] = number[0]
    enums.setdefault(fqn, {}).update(values)


def _merge_fields(messages: dict[str, list], fqn: str, fields: list[dict]) -> None:
    """同名消息按字段号合并，避免后出现的副本覆盖掉已有字段。"""
    existing = messages.setdefault(fqn, [])
    known = {field["no"] for field in existing}
    existing.extend(field for field in fields if field["no"] not in known)
    existing.sort(key=lambda field: field["no"])


def _walk_message(
    md: dict[int, list],
    prefix: str,
    messages: dict[str, list],
    enums: dict[str, dict[str, int]],
) -> None:
    name = md.get(1)
    if not name:
        return
    fqn = f"{prefix}.{_text(name[0])}" if prefix else _text(name[0])
    _merge_fields(messages, fqn, _fields_of(md))
    for raw in _submessages(md.get(3, [])):  # nested_type
        _walk_message(parse_message(raw, 0, len(raw)), fqn, messages, enums)
    for raw in _submessages(md.get(4, [])):  # enum_type
        _add_enum(parse_message(raw, 0, len(raw)), fqn, enums)


def extract_messages(descriptors: dict[str, dict[int, list]]) -> dict[str, list]:
    """提取所有消息（含嵌套），键为消息全名。"""
    messages: dict[str, list] = {}
    for desc in descriptors.values():
        package = _text(desc[2][0]) if desc.get(2) else ""
        for raw in _submessages(desc.get(4, [])):  # message_type
            _walk_message(parse_message(raw, 0, len(raw)), package, messages, {})
    return messages


def extract_enums(descriptors: dict[str, dict[int, list]]) -> dict[str, dict[str, int]]:
    """提取所有枚举（含消息内嵌套），键为枚举全名。"""
    enums: dict[str, dict[str, int]] = {}
    for desc in descriptors.values():
        package = _text(desc[2][0]) if desc.get(2) else ""
        for raw in _submessages(desc.get(5, [])):  # 顶层 enum_type
            _add_enum(parse_message(raw, 0, len(raw)), package, enums)
        for raw in _submessages(desc.get(4, [])):  # message_type 内嵌的枚举
            _walk_message(parse_message(raw, 0, len(raw)), package, {}, enums)
    return enums


def extract_services(descriptors: dict[str, dict[int, list]]) -> dict[str, dict]:
    """提取服务：两层结构 <Service 全名> -> <Method> -> 签名。"""
    services: dict[str, dict] = {}
    for desc in descriptors.values():
        package = _text(desc[2][0]) if desc.get(2) else ""
        for raw in _submessages(desc.get(6, [])):  # service
            sd = parse_message(raw, 0, len(raw))
            name = sd.get(1)
            if not name:
                continue
            service = f"{package}.{_text(name[0])}" if package else _text(name[0])
            methods: dict[str, dict] = {}
            for raw_method in _submessages(sd.get(2, [])):
                md = parse_message(raw_method, 0, len(raw_method))
                method = md.get(1)
                if not method:
                    continue
                input_type = md.get(2)
                output_type = md.get(3)
                methods[_text(method[0])] = {
                    "in": _text(input_type[0]).lstrip(".") if input_type else "",
                    "out": _text(output_type[0]).lstrip(".") if output_type else "",
                    "server_streaming": bool(md.get(6, [0])[0]),
                }
            services.setdefault(service, {}).update(methods)
    return services


def build_schema(
    messages: dict[str, list],
    enums: dict[str, dict[str, int]],
    services: dict[str, dict],
    methods: dict[str, dict],
) -> dict:
    """组装 spec/schema.json 的顶层结构。"""
    return {
        "messages": messages,
        "enums": enums,
        "services": services,
        "methods": methods,
    }


# ---- 扫描与提取 ----


def local_binary_paths(roots: Iterable[str]) -> list[str]:
    """按应用布局展开候选二进制路径。"""
    paths: list[str] = []
    for root in roots:
        for pattern in BIN_PATTERNS:
            paths.extend(glob.glob(os.path.join(root, pattern)))
    return paths


def _read_binary(path: str) -> bytes | None:
    """读取一个候选二进制；体积不足或读取失败返回 None。"""
    try:
        if not os.path.isfile(path) or os.path.getsize(path) <= MIN_BINARY_SIZE:
            return None
        with open(path, "rb") as handle:
            return handle.read()
    except OSError:
        return None


def _proto_names_in(blob: bytes) -> list[str]:
    """从单个二进制里正则捞出候选 .proto 名（跳过 google/ 自带描述符）。

    描述符里的文件名前面紧挨着它的长度前缀字节。该字节若恰好落在正则的
    字符集里（例如长度 46 的 `0x2E` 就是 `.`），会被一并吞进匹配，得到
    `.filesystem/...` 这种不存在的名字，对应描述符就永远找不到。
    因此每个匹配都额外剥掉首字节作为候选；`find_descriptors` 会用描述符
    自带的 name 字段自证，错误候选不会命中，只会多一次无果的搜索。
    """
    names: set[str] = set()
    for raw in PROTO_RE.findall(blob):
        name = raw.decode("utf-8", "replace")
        names.add(name)
        if len(name) > 1:
            names.add(name[1:])
    return sorted(
        name
        for name in names
        if name.endswith(".proto") and not name.startswith(SKIP_NAME_PREFIXES)
    )


def _merge_descriptors(
    descriptors: dict[str, dict[int, list]],
    messages: dict[str, list],
    enums: dict[str, dict[str, int]],
    services: dict[str, dict],
) -> None:
    for fqn, fields in extract_messages(descriptors).items():
        _merge_fields(messages, fqn, fields)
    for fqn, values in extract_enums(descriptors).items():
        enums.setdefault(fqn, {}).update(values)
    for service, methods in extract_services(descriptors).items():
        services.setdefault(service, {}).update(methods)


def extract_from_blobs(blobs: dict[str, bytes]) -> dict:
    """对一批二进制做描述符提取，返回 messages / enums / services。"""
    messages: dict[str, list] = {}
    enums: dict[str, dict[str, int]] = {}
    services: dict[str, dict] = {}
    for blob in blobs.values():
        descriptors = find_descriptors(blob, _proto_names_in(blob))
        _merge_descriptors(descriptors, messages, enums, services)
    return {"messages": messages, "enums": enums, "services": services}


def extract_paths(paths: Iterable[str]) -> dict:
    """逐个读取并提取，避免同时把所有二进制读进内存。"""
    messages: dict[str, list] = {}
    enums: dict[str, dict[str, int]] = {}
    services: dict[str, dict] = {}
    for path in paths:
        blob = _read_binary(path)
        if blob is None:
            continue
        descriptors = find_descriptors(blob, _proto_names_in(blob))
        _merge_descriptors(descriptors, messages, enums, services)
    return {"messages": messages, "enums": enums, "services": services}


def methods_from_openapi(data: dict) -> dict[str, dict]:
    """从单个 openapi.json 里取出 gRPC 方法及其读写属性。"""
    socket = data.get("socket") or {}
    socket_id = _text(socket.get("id", "")) if isinstance(socket, dict) else ""
    out: dict[str, dict] = {}
    permissions = data.get("permissions") or []
    if not isinstance(permissions, list):
        return out
    for permission in permissions:
        if not isinstance(permission, dict):
            continue
        operation = _text(permission.get("operation", ""))
        role = _text(permission.get("role", ""))
        methods = permission.get("methods") or []
        if not isinstance(methods, list):
            continue
        for method in methods:
            if isinstance(method, str):
                out[method] = {"socket": socket_id, "operation": operation, "role": role}
    return out


def collect_methods_local(roots: Iterable[str]) -> dict[str, dict]:
    """扫描各应用的 openapi.json，汇总方法元信息。"""
    out: dict[str, dict] = {}
    for root in roots:
        for pattern in OPENAPI_PATTERNS:
            for path in glob.glob(os.path.join(root, pattern)):
                try:
                    with open(path, encoding="utf-8") as handle:
                        data = json.load(handle)
                except (OSError, ValueError):
                    continue
                if isinstance(data, dict):
                    out.update(methods_from_openapi(data))
    return out


# ---- 远端执行 ----


def build_target(host: str, ssh_user: str | None) -> str:
    """校验并拼出 ssh 目标，避免把可疑字符串当成 ssh 选项。"""
    if not _TARGET_RE.match(host):
        raise SystemExit(f"非法的 NAS 地址：{host!r}")
    if ssh_user is None:
        return host
    if not _TARGET_RE.match(ssh_user):
        raise SystemExit(f"非法的 SSH 用户名：{ssh_user!r}")
    return f"{ssh_user}@{host}"


def _run_remote(target: str, script: str) -> str:
    try:
        proc = subprocess.run(
            ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", target, "python3", "-"],
            input=script.encode("utf-8"),
            capture_output=True,
            timeout=SSH_TIMEOUT,
        )
    except subprocess.TimeoutExpired:
        raise SystemExit(f"远端扫描超时（>{SSH_TIMEOUT}s）")
    except OSError as exc:
        raise SystemExit(f"无法执行 ssh：{exc}")
    stderr_text = proc.stderr.decode("utf-8", "replace")
    if proc.returncode != 0:
        detail = stderr_text.strip()
        raise SystemExit(f"远端命令失败（exit {proc.returncode}）：{detail}")
    if stderr_text:
        sys.stderr.write(stderr_text)
    return proc.stdout.decode("utf-8", "replace")


def _load_json(text: str) -> Any:
    """远端 stdout 可能混入横幅，必要时取最后一行 JSON。"""
    try:
        return json.loads(text)
    except ValueError:
        for line in reversed(text.splitlines()):
            stripped = line.strip()
            if stripped.startswith("{"):
                return json.loads(stripped)
        raise SystemExit("远端输出不是合法 JSON")


def _remote_script(call: str) -> str:
    """把本模块源码 base64 后送到远端执行，避免依赖远端存在同名文件。"""
    source = base64.b64encode(Path(__file__).read_bytes()).decode("ascii")
    return "\n".join(
        [
            "import base64, glob, json, os",
            f"source = base64.b64decode({source!r}).decode('utf-8')",
            "ns = {'__name__': 'extract_schema_remote'}",
            "exec(compile(source, 'extract_schema.py', 'exec'), ns)",
            f"roots = {list(APP_ROOTS)!r}",
            f"patterns = {list(BIN_PATTERNS)!r}",
            "paths = []",
            "for root in roots:",
            "    for pattern in patterns:",
            "        paths.extend(glob.glob(os.path.join(root, pattern)))",
            f"print(json.dumps({call}))",
        ]
    )


def extract_remote(target: str) -> dict:
    return _load_json(
        _run_remote(target, _remote_script("ns['extract_paths'](paths)"))
    )


def methods_remote(target: str) -> dict:
    return _load_json(
        _run_remote(target, _remote_script("ns['collect_methods_local'](roots)"))
    )


# ---- 入口 ----


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="从绿联官方二进制提取 protobuf schema")
    parser.add_argument("--host", default=None, help="NAS 地址（SSH 可达）")
    parser.add_argument("--ssh-user", default=None, help="SSH 用户名，默认取 ssh 配置")
    parser.add_argument("--out", default="spec/schema.json", help="输出路径")
    parser.add_argument(
        "--local-root",
        action="append",
        default=None,
        help="改为扫描本地目录（可重复），与 --host 互斥",
    )
    args = parser.parse_args(argv)

    if args.local_root and args.host:
        parser.error("--host 与 --local-root 只能二选一")
    if not args.local_root and not args.host:
        parser.error("必须提供 --host 或 --local-root")

    if args.local_root:
        roots = [str(Path(root).resolve()) for root in args.local_root]
        remote = extract_paths(local_binary_paths(roots))
        methods = collect_methods_local(roots)
    else:
        target = build_target(args.host, args.ssh_user)
        remote = extract_remote(target)
        methods = methods_remote(target)

    schema = build_schema(
        remote["messages"], remote["enums"], remote["services"], methods
    )
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        json.dumps(schema, ensure_ascii=False, indent=1) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(
        f"wrote {out} ({len(schema['messages'])} messages, "
        f"{len(schema['enums'])} enums, {len(schema['services'])} services, "
        f"{len(schema['methods'])} methods)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
