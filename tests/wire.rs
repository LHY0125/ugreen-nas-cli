use nascli::wire::{decode, decode_strict, encode, read_varint_at, Message, Value};
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
fn packed_scalar_stays_opaque_bytes_at_wire_layer() {
    // packed repeated 标量在 wire 层就是一个 length-delimited 字段
    // （field 1, len 3, 值 1/2/3）。解包需要字段类型信息，属于 schema 层职责。
    let raw = vec![0x0a, 0x03, 0x01, 0x02, 0x03];
    assert_eq!(
        decode(&raw).get(&1),
        Some(&vec![Value::Bytes(vec![1, 2, 3])])
    );
}

#[test]
fn read_varint_at_returns_value_and_next_offset() {
    // 150 = 0x96 0x01；末尾的 0xff 是下一个 varint 的开头，不参与本次读取。
    assert_eq!(read_varint_at(&[0x96, 0x01, 0xff], 0), Some((150, 2)));
    // 从中间开始读
    assert_eq!(read_varint_at(&[0x00, 0x96, 0x01], 1), Some((150, 3)));
    // 数据不足（只有续位没有终止字节）
    assert_eq!(read_varint_at(&[0x96], 0), None);
    // 起点越界
    assert_eq!(read_varint_at(&[0x96, 0x01], 2), None);
}

#[test]
fn strict_decode_accepts_valid_message() {
    let m = msg(&[(3, Value::Bytes(b"/volume1/demo".to_vec()))]);
    assert_eq!(decode_strict(&encode(&m)).unwrap(), m);
}

#[test]
fn strict_decode_rejects_truncated_length() {
    assert!(decode_strict(&[0x0a, 0x05, 0x01]).is_err());
}

#[test]
fn strict_decode_rejects_field_zero() {
    assert!(decode_strict(&[0x00]).is_err());
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
