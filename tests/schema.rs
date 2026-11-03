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
    m.insert(3, Value::Bytes(b"/volume1/demo".to_vec()));
    m.insert(8, Value::Varint(50));
    let j = s.message_to_json("demo.Req", &m);
    assert_eq!(j["path"], "/volume1/demo");
    assert_eq!(j["limit"], 50);
}

#[test]
fn resolves_enum_symbol() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(16, Value::Varint(1));
    assert_eq!(s.message_to_json("demo.Req", &m)["mode"], "SAFE");
}

#[test]
fn unknown_field_is_preserved() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(99, Value::Varint(7));
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
    m.insert(1, Value::Fixed32(1.5f32.to_bits()));
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
    m.insert(1, Value::Fixed32(1.5f32.to_bits()));
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
    m.insert(2, Value::Varint(42));
    assert_eq!(s.message_to_json("demo.D", &m)["value"], 42);
}
