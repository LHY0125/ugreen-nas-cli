use nascli::wire::{decode, encode, Message, Value};
use std::collections::BTreeMap;

fn msg(pairs: &[(u32, Value)]) -> Message {
    pairs.iter().cloned().collect::<BTreeMap<_, _>>()
}

#[test]
fn roundtrip_string_field() {
    let m = msg(&[(3, Value::Bytes(b"/volume1/demo".to_vec()))]);
    assert_eq!(decode(&encode(&m)), m);
}

#[test]
fn roundtrip_varint_field() {
    let m = msg(&[(8, Value::Varint(500))]);
    assert_eq!(decode(&encode(&m)), m);
}

#[test]
fn encodes_known_bytes() {
    // field 1, wire type 2, len 1, "A"
    let m = msg(&[(1, Value::Bytes(b"A".to_vec()))]);
    assert_eq!(encode(&m), vec![0x0a, 0x01, 0x41]);
}

#[test]
fn varint_boundaries() {
    for v in [0u64, 1, 127, 128, 16383, 16384, u32::MAX as u64, u64::MAX] {
        let m = msg(&[(1, Value::Varint(v))]);
        assert_eq!(decode(&encode(&m)), m, "failed at {v}");
    }
}

#[test]
fn empty_message_roundtrips() {
    assert_eq!(decode(&encode(&Message::new())), Message::new());
}

#[test]
fn decodes_nested_bytes_as_opaque() {
    // 嵌套消息在 wire 层就是 Bytes，展开由 schema 层负责
    let inner = encode(&msg(&[(1, Value::Bytes(b"x".to_vec()))]));
    let outer = msg(&[(2, Value::Bytes(inner))]);
    assert_eq!(decode(&encode(&outer)), outer);
}

#[test]
fn repeated_field_last_wins() {
    // 同一字段号出现两次时，BTreeMap 保留最后一个
    let raw = vec![0x08, 0x01, 0x08, 0x02];
    assert_eq!(decode(&raw).get(&1), Some(&Value::Varint(2)));
}
