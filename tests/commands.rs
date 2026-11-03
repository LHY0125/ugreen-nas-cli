use nascli::commands::{parse_field, require_yes};
use nascli::wire::Value;

#[test]
fn parses_string_field() {
    let (no, v) = parse_field("3:string=/volume1/demo").unwrap();
    assert_eq!(no, 3);
    assert_eq!(v, Value::Bytes(b"/volume1/demo".to_vec()));
}

#[test]
fn parses_int_field() {
    let (no, v) = parse_field("8:int=50").unwrap();
    assert_eq!(no, 8);
    assert_eq!(v, Value::Varint(50));
}

#[test]
fn parses_bool_field() {
    let (_, v) = parse_field("9:bool=true").unwrap();
    assert_eq!(v, Value::Varint(1));
}

#[test]
fn rejects_bad_field_spec() {
    assert!(parse_field("3").is_err());
    assert!(parse_field("x:string=a").is_err());
    assert!(parse_field("3:unknown=a").is_err());
    assert!(parse_field("8:int=abc").is_err());
}

#[test]
fn read_operation_needs_no_confirmation() {
    assert!(require_yes("read", "/a/B", "", false).is_ok());
}

#[test]
fn write_operation_requires_yes() {
    let e = require_yes("delete", "/a/B", "删除 /volume1/demo", false).unwrap_err();
    assert!(e.contains("--yes"));
    assert!(require_yes("delete", "/a/B", "删除 /volume1/demo", true).is_ok());
}
