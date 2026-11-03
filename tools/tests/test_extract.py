"""tools/extract_schema.py 的单元测试。

全部使用合成的 demo/demo.proto 描述符，不涉及任何真实环境信息。
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from extract_schema import (  # noqa: E402
    PROTO_TYPE_NAMES,
    build_schema,
    collect_methods_local,
    extract_enums,
    extract_from_blobs,
    extract_messages,
    extract_paths,
    extract_services,
    find_descriptors,
    local_binary_paths,
    parse_message,
    type_name_of,
)


def varint(n: int) -> bytes:
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out += bytes([b | 0x80])
        else:
            return out + bytes([b])


def field_bytes(no: int, raw: bytes) -> bytes:
    return varint((no << 3) | 2) + varint(len(raw)) + raw


def field_varint(no: int, value: int) -> bytes:
    return varint((no << 3) | 0) + varint(value)


def make_descriptor(name: str, package: str) -> bytes:
    return field_bytes(1, name.encode()) + field_bytes(2, package.encode())


def make_field_descriptor(
    name: str, number: int, type_code: int, type_name: str | None = None
) -> bytes:
    raw = field_bytes(1, name.encode()) + field_varint(3, number) + field_varint(5, type_code)
    if type_name is not None:
        raw += field_bytes(6, type_name.encode())
    return raw


def make_message(name: str, *fields: bytes, nested: bytes = b"") -> bytes:
    raw = field_bytes(1, name.encode())
    raw += b"".join(field_bytes(2, f) for f in fields)
    if nested:
        raw += field_bytes(3, nested)
    return raw


def make_enum(name: str, values: list[tuple[str, int]]) -> bytes:
    raw = field_bytes(1, name.encode())
    for symbol, number in values:
        value = field_bytes(1, symbol.encode()) + field_varint(2, number)
        raw += field_bytes(2, value)
    return raw


def make_service(name: str, method: str, in_type: str, out_type: str, streaming: bool) -> bytes:
    m = field_bytes(1, method.encode())
    m += field_bytes(2, in_type.encode())
    m += field_bytes(3, out_type.encode())
    m += field_varint(6, 1 if streaming else 0)
    return field_bytes(1, name.encode()) + field_bytes(2, m)


# ---- 简报指定的四个测试 ----


def test_parse_message_reads_name_and_package():
    raw = make_descriptor("demo/demo.proto", "demo.v1")
    parsed = parse_message(raw, 0, len(raw))
    assert parsed[1][0] == b"demo/demo.proto"
    assert parsed[2][0] == b"demo.v1"


def test_parse_message_stops_at_invalid_field_number():
    raw = make_descriptor("demo/demo.proto", "demo.v1") + varint((99 << 3) | 0) + varint(1)
    parsed = parse_message(raw, 0, len(raw))
    assert 99 not in parsed


def test_find_descriptors_skips_false_positives():
    # 先放一个只有 name 字段的假描述符，真描述符在后——必须命中后者
    decoy = field_bytes(1, b"demo/demo.proto")
    good = make_descriptor("demo/demo.proto", "demo.v1")
    blob = b"\x00" * 8 + decoy + b"\x00" * 8 + good + b"\x00" * 8
    found = find_descriptors(blob, ["demo/demo.proto"])
    assert "demo/demo.proto" in found
    assert found["demo/demo.proto"][2][0] == b"demo.v1"


def test_find_descriptors_rejects_name_only_fragment():
    # 只有 name 字段的裸串不构成有效描述符，不能因为「自证」就接受
    decoy = field_bytes(1, b"demo/demo.proto")
    assert find_descriptors(decoy, ["demo/demo.proto"]) == {}


def test_find_descriptors_accepts_syntax_only_descriptor():
    # 只有 name + syntax 的极简描述符仍然有效，不能被收紧后的判据误伤
    descriptor = field_bytes(1, b"demo/a.proto") + field_bytes(12, b"proto3")
    found = find_descriptors(descriptor, ["demo/a.proto"])
    assert "demo/a.proto" in found


def test_build_schema_shape():
    schema = build_schema(
        {},
        {},
        {},
        {
            "/storage.Storage/VolumeList": {
                "socket": "ugreen.storage.grpc",
                "operation": "read",
                "role": "admin",
            }
        },
    )
    assert schema["methods"]["/storage.Storage/VolumeList"]["operation"] == "read"
    assert set(schema) == {"messages", "enums", "services", "methods"}


# ---- 边界与类型名 ----


def test_parse_message_stops_at_truncated_length():
    # 声明长度 10，实际只有 3 字节：必须停止而不是越界读取相邻数据
    raw = varint((4 << 3) | 2) + varint(10) + b"abc"
    parsed = parse_message(raw, 0, len(raw))
    assert 4 not in parsed


def test_parse_message_ignores_data_beyond_end():
    raw = make_descriptor("demo/demo.proto", "demo.v1")
    tail = field_bytes(5, b"should-not-be-read")
    parsed = parse_message(raw + tail, 0, len(raw))
    assert 5 not in parsed


def test_type_name_of_maps_numeric_codes_to_names():
    assert type_name_of({5: [9]}) == "string"
    assert type_name_of({5: [5]}) == "int32"
    assert type_name_of({5: [1]}) == "double"
    assert type_name_of({5: [2]}) == "float"
    assert type_name_of({5: [14], 6: [b".demo.v1.Health"]}) == "enum"
    assert type_name_of({5: [99]}) == "unknown_99"


def test_proto_type_names_cover_all_codes():
    assert sorted(PROTO_TYPE_NAMES) == list(range(1, 19))


# ---- messages ----


def test_extract_messages_emits_string_type_names():
    message = make_message("Volume", make_field_descriptor("path", 1, 9))
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, message)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    messages = extract_messages(found)
    assert messages["demo.v1.Volume"] == [
        {"no": 1, "name": "path", "type": "string", "type_name": None}
    ]


def test_extract_messages_resolves_enum_and_message_type_names():
    message = make_message(
        "Volume",
        make_field_descriptor("health", 1, 14, ".demo.v1.Health"),
        make_field_descriptor("detail", 2, 11, ".demo.v1.Detail"),
    )
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, message)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    fields = extract_messages(found)["demo.v1.Volume"]
    assert fields[0]["type"] == "enum"
    assert fields[0]["type_name"] == "demo.v1.Health"
    assert fields[1]["type"] == "message"
    assert fields[1]["type_name"] == "demo.v1.Detail"


def test_extract_messages_recurses_into_nested_types():
    inner = make_message("Inner", make_field_descriptor("id", 1, 5))
    outer = make_message("Outer", make_field_descriptor("id", 1, 5), nested=inner)
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, outer)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    messages = extract_messages(found)
    assert "demo.v1.Outer.Inner" in messages
    assert messages["demo.v1.Outer.Inner"][0]["type"] == "int32"


# ---- enums ----


def test_extract_enums_reads_symbols():
    enum = make_enum("Health", [("SAFE", 1), ("BAD", 2)])
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(5, enum)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    assert extract_enums(found) == {"demo.v1.Health": {"SAFE": 1, "BAD": 2}}


def test_extract_enums_recurses_into_nested_enums():
    enum = make_enum("State", [("ON", 0), ("OFF", 1)])
    message = field_bytes(1, b"Outer") + field_bytes(4, enum)
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, message)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    assert extract_enums(found)["demo.v1.Outer.State"] == {"ON": 0, "OFF": 1}


# ---- services ----


def test_extract_services_groups_methods_under_service():
    service = make_service(
        "Storage", "VolumeList", ".demo.v1.VolumeListReq", ".demo.v1.VolumeListResp", True
    )
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(6, service)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    services = extract_services(found)
    assert services == {
        "demo.v1.Storage": {
            "VolumeList": {
                "in": "demo.v1.VolumeListReq",
                "out": "demo.v1.VolumeListResp",
                "server_streaming": True,
            }
        }
    }


# ---- 端到端 ----


def test_extract_from_blobs_end_to_end():
    message = make_message("Volume", make_field_descriptor("path", 1, 9))
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, message)
    blob = b"\x00" * 16 + descriptor + b"\x00" * 16
    result = extract_from_blobs({"demo-bin": blob})
    assert result["messages"]["demo.v1.Volume"][0]["type"] == "string"
    assert set(result) == {"messages", "enums", "services"}


def test_find_descriptors_stops_at_wire_type_mismatch():
    # 描述符后面紧跟相邻数据，且恰好是「字段 4 + varint」：
    # FileDescriptorProto 的字段 4 必须是 wire type 2，必须在此停止。
    good = make_descriptor("demo/demo.proto", "demo.v1")
    tail = field_varint(4, 12345)
    found = find_descriptors(good + tail, ["demo/demo.proto"])
    assert 4 not in found["demo/demo.proto"]


def test_find_descriptors_keeps_valid_descriptor_fields():
    message = make_message("Volume", make_field_descriptor("path", 1, 9))
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, message)
    found = find_descriptors(descriptor, ["demo/demo.proto"])
    assert found["demo/demo.proto"][4][0] == message


def test_extract_from_blobs_merges_duplicate_messages():
    msg_a = make_message("Volume", make_field_descriptor("path", 1, 9))
    msg_b = make_message("Volume", make_field_descriptor("size", 2, 4))
    blob_a = make_descriptor("demo/a.proto", "demo.v1") + field_bytes(4, msg_a)
    blob_b = make_descriptor("demo/b.proto", "demo.v1") + field_bytes(4, msg_b)
    result = extract_from_blobs({"a": blob_a, "b": blob_b})
    names = [field["name"] for field in result["messages"]["demo.v1.Volume"]]
    assert names == ["path", "size"]


def test_extract_from_blobs_merges_duplicate_services():
    svc_a = make_service("Storage", "VolumeList", ".demo.v1.ReqA", ".demo.v1.RespA", False)
    svc_b = make_service("Storage", "VolumeGet", ".demo.v1.ReqB", ".demo.v1.RespB", False)
    blob_a = make_descriptor("demo/a.proto", "demo.v1") + field_bytes(6, svc_a)
    blob_b = make_descriptor("demo/b.proto", "demo.v1") + field_bytes(6, svc_b)
    result = extract_from_blobs({"a": blob_a, "b": blob_b})
    assert set(result["services"]["demo.v1.Storage"]) == {"VolumeList", "VolumeGet"}


def test_extract_paths_finds_binary_in_app_layout(tmp_path):
    message = make_message("Volume", make_field_descriptor("path", 1, 9))
    descriptor = make_descriptor("demo/demo.proto", "demo.v1") + field_bytes(4, message)
    blob = b"\x00" * 16 + descriptor
    blob += b"\x00" * (200_001 - len(blob))
    binary = tmp_path / "demoapp" / "bin" / "demo"
    binary.parent.mkdir(parents=True)
    binary.write_bytes(blob)

    result = extract_paths(local_binary_paths([str(tmp_path)]))
    assert result["messages"]["demo.v1.Volume"][0]["type"] == "string"


def test_collect_methods_local_reads_openapi(tmp_path):
    app = tmp_path / "demoapp"
    app.mkdir()
    (app / "openapi.json").write_text(
        json.dumps(
            {
                "socket": {"id": "demo.grpc"},
                "permissions": [
                    {"operation": "read", "role": "admin", "methods": ["/demo.App/Get"]},
                    {"operation": "delete", "role": "user", "methods": ["/demo.App/Del"]},
                ],
            }
        ),
        encoding="utf-8",
    )

    methods = collect_methods_local([str(tmp_path)])
    assert methods["/demo.App/Get"] == {
        "socket": "demo.grpc",
        "operation": "read",
        "role": "admin",
    }
    assert methods["/demo.App/Del"]["operation"] == "delete"
