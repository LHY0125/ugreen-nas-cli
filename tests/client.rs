use nascli::client::{classify, ApiError, Client};
use nascli::config::Config;

#[test]
fn unauthorized_maps_to_3() {
    let e = classify(401, None, r#"{"code":"unauthorized"}"#);
    assert!(matches!(e, ApiError::Unauthorized));
    assert_eq!(e.exit_code(), 3);
    assert_eq!(e.code(), "UNAUTHORIZED");
}

#[test]
fn access_denied_maps_to_4() {
    let e = classify(500, Some(7), r#"{"message":"Access not allowed"}"#);
    assert_eq!(e.exit_code(), 4);
    assert_eq!(e.code(), "ACCESS_DENIED");
}

#[test]
fn cardinality_violation_means_streaming_unsupported() {
    let e = classify(
        500,
        Some(13),
        r#"{"message":"cardinality violation: expected <EOF> for non server-streaming RPCs"}"#,
    );
    assert!(matches!(e, ApiError::Unsupported(_)));
    assert_eq!(e.exit_code(), 5);
}

#[test]
fn invalid_argument_maps_to_2() {
    let e = classify(400, Some(3), r#"{"message":"path is empty"}"#);
    assert_eq!(e.exit_code(), 2);
    assert_eq!(e.code(), "BAD_REQUEST");
}

#[test]
fn unknown_server_error_maps_to_7() {
    let e = classify(500, Some(2), r#"{"message":"not found app"}"#);
    assert_eq!(e.exit_code(), 7);
    assert_eq!(e.code(), "SERVER_ERROR");
}

#[test]
fn message_is_extracted_from_body() {
    let e = classify(400, Some(3), r#"{"message":"path is empty"}"#);
    assert_eq!(e.message(), "path is empty");
}

#[test]
fn msg_field_is_used_when_message_is_missing() {
    let e = classify(400, Some(3), r#"{"msg":"boom"}"#);
    assert_eq!(e.message(), "boom");
}

#[test]
fn empty_msg_falls_back_to_raw_body() {
    // 空串不算有效 message，退回原文（此时即 JSON 本体）。
    let body = r#"{"msg":""}"#;
    let e = classify(400, Some(3), body);
    assert_eq!(e.message(), body);
}

#[test]
fn raw_body_is_truncated_to_200_chars() {
    let body = "x".repeat(300);
    let e = classify(500, None, &body);
    assert_eq!(e.message().chars().count(), 200);
}

#[test]
fn unsupported_http_status_falls_back_to_server_error() {
    // 415 与 5xx 之外的未知状态同样归为服务端错误，退出码 7。
    let e = classify(415, None, r#"{"message":"unsupported media type"}"#);
    assert_eq!(e.exit_code(), 7);
    assert_eq!(e.code(), "SERVER_ERROR");
}

#[test]
fn network_error_has_stable_exit_code_and_message() {
    let e = ApiError::Network("connection refused".to_string());
    assert_eq!(e.exit_code(), 6);
    assert_eq!(e.code(), "NETWORK_ERROR");
    assert_eq!(e.message(), "connection refused");
}

#[test]
fn client_exposes_only_redacted_token() {
    // 错误信息只能拿到脱敏后的令牌，完整明文不得出现在任何字符串里。
    let client = Client::new(Config {
        host: "nas.example.com".into(),
        port: 9443,
        token: "ugpoa_v1.abcdef.secretsecret".into(),
        insecure: true,
    })
    .expect("构造客户端不应失败");
    let shown = client.redacted_token();
    assert_eq!(shown, "ugpoa_v1.****");
    assert!(!shown.contains("secretsecret"), "完整令牌不得出现：{shown}");
}
