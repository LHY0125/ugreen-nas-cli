use nascli::wire::{decode, encode, Message, Value};
use std::collections::BTreeMap;

fn msg(pairs: &[(u32, Value)]) -> Message {
    pairs
        .iter()
        .cloned()
        .map(|(no, v)| (no, vec![v]))
        .collect::<BTreeMap<_, _>>()
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
fn repeated_field_preserves_all_values() {
    // 同号字段在 wire 上重复出现（repeated 字段），解码必须全部保留。
    let raw = vec![0x08, 0x01, 0x08, 0x02];
    let decoded = decode(&raw);
    assert_eq!(
        decoded.get(&1),
        Some(&vec![Value::Varint(1), Value::Varint(2)])
    );
}

#[test]
fn encodes_repeated_field_as_repeated_keys() {
    // 多值字段编码时每个元素写一次同号 key。
    let mut m = Message::new();
    m.insert(1, vec![Value::Varint(1), Value::Varint(2)]);
    assert_eq!(encode(&m), vec![0x08, 0x01, 0x08, 0x02]);
}

#[test]
fn roundtrip_repeated_field() {
    let mut m = Message::new();
    m.insert(1, vec![Value::Varint(1), Value::Varint(2)]);
    m.insert(
        2,
        vec![Value::Bytes(b"a".to_vec()), Value::Bytes(b"b".to_vec())],
    );
    assert_eq!(decode(&encode(&m)), m);
}

#[test]
fn malformed_length_prefix_does_not_panic() {
    // 长度前缀 = u64::MAX，远超剩余字节数
    let raw = vec![
        0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
    ];
    let _ = decode(&raw); // 不 panic 即通过
}

#[test]
fn roundtrip_fixed32_field() {
    let m = msg(&[(4, Value::Fixed32(0xdead_beef))]);
    assert_eq!(decode(&encode(&m)), m);
}

#[test]
fn roundtrip_fixed64_field() {
    let m = msg(&[(9, Value::Fixed64(0x0123_4567_89ab_cdef))]);
    assert_eq!(decode(&encode(&m)), m);
}
