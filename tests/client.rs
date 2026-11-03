use nascli::client::{classify, ApiError};

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
fn missing_grpc_code_on_200_gives_actionable_message() {
    // 200 但缺 x-grpc-code：不能当作成功，也不该把 protobuf 乱码甩给用户。
    let e = classify(200, None, "\u{0}\u{1}binary-garbage");
    assert_eq!(e.exit_code(), 7);
    assert_eq!(e.code(), "SERVER_ERROR");
    assert!(e.message().contains("x-grpc-code"), "{}", e.message());
}

#[test]
fn scrub_removes_token_from_error_message() {
    let token = "ugpoa_v1.abcdef.secretsecret";
    let e = ApiError::Server(format!("bad auth header: Bearer {token}")).scrub(token);
    assert!(
        !e.message().contains("secretsecret"),
        "令牌不得泄露：{}",
        e.message()
    );
    assert!(e.message().contains("ugpoa_v1.****"));
}

#[test]
fn scrub_leaves_clean_messages_alone() {
    let e = ApiError::BadRequest("path is empty".into()).scrub("ugpoa_v1.abcdef.secretsecret");
    assert_eq!(e.message(), "path is empty");
}

#[test]
fn scrub_is_noop_for_empty_token() {
    // 空令牌时不得让 str::replace 在每个字符边界插入替换串、打碎文案。
    let e = ApiError::BadRequest("path is empty".into()).scrub("");
    assert_eq!(e.message(), "path is empty");
}
