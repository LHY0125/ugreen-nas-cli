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
