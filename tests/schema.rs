use nascli::schema::{is_read_only, to_json, Schema};
use nascli::wire::{Message, Value};

const SAMPLE: &str = r#"{
  "messages": {
    "demo.Req": [
      {"no": 3, "name": "path", "type": "string"},
      {"no": 8, "name": "limit", "type": "int32"},
      {"no": 16, "name": "mode", "type": "enum", "type_name": "demo.Mode"}
    ]
  },
  "enums": { "demo.Mode": { "FAST": 0, "SAFE": 1 } },
  "services": {
    "storage.Storage": {
      "VolumeList": {"in": "demo.Req", "out": "demo.Resp", "server_streaming": false}
    }
  },
  "methods": {
    "/storage.Storage/VolumeList": {"socket": "ugreen.storage.grpc", "operation": "read", "role": "admin"},
    "/ugidl.filesystem.Filesystem/DelPath": {"socket": "ugreen.filemgr.internal.grpc", "operation": "delete", "role": "user"}
  }
}"#;

#[test]
fn parses_methods() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let m = s.method("/storage.Storage/VolumeList").unwrap();
    assert_eq!(m.socket, "ugreen.storage.grpc");
    assert_eq!(m.operation, "read");
}

#[test]
fn read_only_classification() {
    assert!(is_read_only("read"));
    assert!(!is_read_only("write"));
    assert!(!is_read_only("delete"));
    assert!(!is_read_only("execute"));
}

#[test]
fn maps_field_numbers_to_names() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(3, vec![Value::Bytes(b"/volume1/demo".to_vec())]);
    m.insert(8, vec![Value::Varint(50)]);
    let j = s.message_to_json("demo.Req", &m);
    assert_eq!(j["path"], "/volume1/demo");
    assert_eq!(j["limit"], 50);
}

#[test]
fn resolves_enum_symbol() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(16, vec![Value::Varint(1)]);
    assert_eq!(s.message_to_json("demo.Req", &m)["mode"], "SAFE");
}

#[test]
fn unknown_field_is_preserved() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(99, vec![Value::Varint(7)]);
    assert_eq!(to_json(&m, &s)["field_99"], 7);
}

#[test]
fn rejects_malformed_json() {
    assert!(Schema::from_json_str("not json").is_err());
}

#[test]
fn resolves_request_type_from_method() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    assert_eq!(
        s.request_type("/storage.Storage/VolumeList").as_deref(),
        Some("demo.Req")
    );
    assert!(s.request_type("/nope.Nope/Nope").is_none());
    assert!(s.request_type("malformed").is_none());
}

#[test]
fn resolves_response_type_from_method() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    assert_eq!(
        s.response_type("/storage.Storage/VolumeList").as_deref(),
        Some("demo.Resp")
    );
    assert!(s.response_type("/nope.Nope/Nope").is_none());
    assert!(s.response_type("malformed").is_none());
}

#[test]
fn looks_up_field_number_by_name() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    assert_eq!(s.field_no("demo.Req", "path"), Some(3));
    assert_eq!(s.field_no("demo.Req", "limit"), Some(8));
    assert_eq!(s.field_no("demo.Req", "missing"), None);
    assert_eq!(s.field_no("no.Such", "path"), None);
}

#[test]
fn renders_float_field_as_number() {
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.F": [ {"no": 1, "name": "ratio", "type": "float"} ] },
      "enums": {}, "services": {}, "methods": {}
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Fixed32(1.5f32.to_bits())]);
    assert_eq!(s.message_to_json("demo.F", &m)["ratio"], 1.5);
}

#[test]
fn renders_float_field_in_untyped_path_as_number() {
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.F": [ {"no": 1, "name": "ratio", "type": "float"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Fixed32(1.5f32.to_bits())]);
    // 无类型入口拿不到字段定义，只能按位模式输出整数——这是有意的兜底行为。
    assert_eq!(to_json(&m, &s)["field_1"], 1069547520);
}

#[test]
fn varint_double_declaration_does_not_reinterpret_bits() {
    // double 合法走 fixed64；即便 schema 误把某字段声明为 double 而线上是 varint，
    // 也不应把整数当 f64 位模式重解释，应按整数输出。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.D": [ {"no": 2, "name": "value", "type": "double"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(2, vec![Value::Varint(42)]);
    assert_eq!(s.message_to_json("demo.D", &m)["value"], 42);
}

#[test]
fn negative_enum_values_from_descriptor_are_wrapped() {
    // 描述符里 int32 的负数被提取器写成了 u64 补码（-1 → 18446744073709551615），
    // 解析时必须回绕成 i64，否则整份 schema 会因超范围而解析失败。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.E": [ {"no": 1, "name": "code", "type": "enum", "type_name": "demo.C"} ] },
      "enums": { "demo.C": { "ERR_IO": 18446744073709551615 } }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Varint(u64::MAX)]);
    assert_eq!(s.message_to_json("demo.E", &m)["code"], "ERR_IO");
}

#[test]
fn repeated_field_renders_as_array() {
    // 同号字段多值 → JSON 数组，顺序与 wire 上出现顺序一致。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.R": [ {"no": 1, "name": "ids", "type": "int32"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Varint(1), Value::Varint(2)]);
    assert_eq!(
        s.message_to_json("demo.R", &m)["ids"],
        serde_json::json!([1, 2])
    );
}

#[test]
fn single_value_field_stays_scalar() {
    // 单值仍输出标量，保持既有输出契约不变。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.R": [ {"no": 1, "name": "ids", "type": "int32"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Varint(7)]);
    assert_eq!(s.message_to_json("demo.R", &m)["ids"], 7);
}

#[test]
fn repeated_nested_message_renders_as_array_of_objects() {
    // repeated message：每个元素都是独立的 Bytes，各自递归展开。
    let s = Schema::from_json_str(
        r#"{
      "messages": {
        "demo.List": [ {"no": 1, "name": "items", "type": "message", "type_name": "demo.Item"} ],
        "demo.Item": [ {"no": 1, "name": "name", "type": "string"} ]
      }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(
        1,
        vec![
            Value::Bytes(vec![0x0a, 0x01, b'a']),
            Value::Bytes(vec![0x0a, 0x01, b'b']),
        ],
    );
    assert_eq!(
        s.message_to_json("demo.List", &m)["items"],
        serde_json::json!([{"name": "a"}, {"name": "b"}])
    );
}

#[test]
fn untyped_path_renders_repeated_field_as_array() {
    let s = Schema::from_json_str(r#"{"messages": {}}"#).unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Varint(1), Value::Varint(2)]);
    assert_eq!(to_json(&m, &s)["field_1"], serde_json::json!([1, 2]));
}

#[test]
fn empty_value_vector_is_skipped() {
    // 空 Vec 不应产生 null 字段。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.R": [ {"no": 1, "name": "ids", "type": "int32"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![]);
    assert!(s.message_to_json("demo.R", &m).get("ids").is_none());
    assert!(to_json(&m, &s).get("field_1").is_none());
}

#[test]
fn packed_repeated_varint_renders_as_array() {
    // packed int32：一个 Bytes 内含 3 个裸 varint。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.P": [ {"no": 1, "name": "ids", "type": "int32", "repeated": true} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(vec![1, 2, 3])]);
    assert_eq!(
        s.message_to_json("demo.P", &m)["ids"],
        serde_json::json!([1, 2, 3])
    );
}

#[test]
fn packed_repeated_enum_renders_symbols() {
    // packed enum 解包后仍走 value_to_json，枚举值应翻成符号。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.P": [ {"no": 1, "name": "flags", "type": "enum", "type_name": "demo.F", "repeated": true} ] },
      "enums": { "demo.F": { "OK": 0, "WARN": 1, "BAD": 2 } }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(vec![0, 1, 2])]);
    assert_eq!(
        s.message_to_json("demo.P", &m)["flags"],
        serde_json::json!(["OK", "WARN", "BAD"])
    );
}

#[test]
fn packed_repeated_bool_renders_booleans() {
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.P": [ {"no": 1, "name": "bits", "type": "bool", "repeated": true} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(vec![1, 0, 1])]);
    assert_eq!(
        s.message_to_json("demo.P", &m)["bits"],
        serde_json::json!([true, false, true])
    );
}

#[test]
fn packed_repeated_fixed32_and_double_render_as_numbers() {
    let s = Schema::from_json_str(
        r#"{
      "messages": {
        "demo.P": [
          {"no": 1, "name": "ratio", "type": "float", "repeated": true},
          {"no": 2, "name": "weight", "type": "double", "repeated": true}
        ]
      }
    }"#,
    )
    .unwrap();
    let mut f32_payload = Vec::new();
    f32_payload.extend_from_slice(&1.5f32.to_le_bytes());
    f32_payload.extend_from_slice(&2.5f32.to_le_bytes());
    let mut f64_payload = Vec::new();
    f64_payload.extend_from_slice(&3.5f64.to_le_bytes());
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(f32_payload)]);
    m.insert(2, vec![Value::Bytes(f64_payload)]);
    let j = s.message_to_json("demo.P", &m);
    assert_eq!(j["ratio"], serde_json::json!([1.5, 2.5]));
    assert_eq!(j["weight"], serde_json::json!([3.5]));
}

#[test]
fn repeated_string_is_not_unpacked() {
    // repeated string 永远不是 packed：每个元素是独立的 length-delimited 字段，
    // 单元素时仍输出字符串标量。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.P": [ {"no": 1, "name": "tags", "type": "string", "repeated": true} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(b"alpha".to_vec())]);
    assert_eq!(s.message_to_json("demo.P", &m)["tags"], "alpha");

    let mut two = Message::new();
    two.insert(
        1,
        vec![
            Value::Bytes(b"alpha".to_vec()),
            Value::Bytes(b"beta".to_vec()),
        ],
    );
    assert_eq!(
        s.message_to_json("demo.P", &two)["tags"],
        serde_json::json!(["alpha", "beta"])
    );
}

#[test]
fn non_repeated_bytes_are_not_unpacked() {
    // 非 repeated 的 bytes 字段即便内容是合法 varint 序列，也不能被解包。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.P": [ {"no": 1, "name": "blob", "type": "bytes"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(vec![0xff, 0xfe, 0xfd])]);
    // 非 UTF-8 字节按十六进制输出，而不是数组。
    assert_eq!(s.message_to_json("demo.P", &m)["blob"], "fffefd");
}

#[test]
fn schema_without_repeated_key_still_loads() {
    // 旧 schema（提取器尚未写入 repeated）必须仍能反序列化，缺省为 false。
    let s = Schema::from_json_str(
        r#"{
      "messages": { "demo.P": [ {"no": 1, "name": "ids", "type": "int32"} ] }
    }"#,
    )
    .unwrap();
    let mut m = Message::new();
    m.insert(1, vec![Value::Bytes(vec![0xff, 0xfe, 0xfd])]);
    // 没有 repeated 标记 → 不做 packed 解包，按十六进制兜底。
    assert_eq!(s.message_to_json("demo.P", &m)["ids"], "fffefd");
}
