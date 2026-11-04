use nascli::commands::{operation_of, parse_field, require_yes};
use nascli::schema::Schema;
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
    assert!(parse_field("0:string=a").is_err());
    assert!(parse_field("536870912:string=a").is_err());
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

#[test]
fn forced_write_overrides_schema_read() {
    let s = Schema::from_json_str(
        r#"{
      "methods": {
        "/search.Search/RemoveTitleTask": {"socket": "s", "operation": "read"},
        "/search.Search/CancelTitleTask": {"socket": "s", "operation": "read"}
      }
    }"#,
    )
    .unwrap();
    for method in [
        "/search.Search/RemoveTitleTask",
        "/search.Search/CancelTitleTask",
    ] {
        assert_eq!(operation_of(&s, method), "write", "method={method}");
        assert!(
            require_yes(operation_of(&s, method), method, "", false).is_err(),
            "method={method} 必须要求 --yes"
        );
    }
}

#[test]
fn unknown_method_is_treated_as_write() {
    let s = Schema::from_json_str(
        r#"{
      "methods": {
        "/a.B/Read": {"socket": "s", "operation": "read"},
        "/a.B/Del": {"socket": "s", "operation": "delete"}
      }
    }"#,
    )
    .unwrap();
    assert_eq!(operation_of(&s, "/a.B/Read"), "read");
    assert_eq!(operation_of(&s, "/a.B/Del"), "delete");
    // fail-closed：未登记的方法读写属性不可知，必须按写操作处理。
    assert_eq!(operation_of(&s, "/x.Y/Z"), "write");
    let e = require_yes(operation_of(&s, "/x.Y/Z"), "/x.Y/Z", "", false).unwrap_err();
    assert!(e.contains("--yes"));
}
