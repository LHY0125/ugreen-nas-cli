use nascli::config::{redact, Config, ConfigError};

fn cfg(host: &str, token: &str) -> Config {
    Config {
        host: host.into(),
        port: 9443,
        token: token.into(),
        insecure: true,
    }
}

#[test]
fn builds_base_url() {
    assert_eq!(
        cfg("nas.example.com", "t").base_url(),
        "https://nas.example.com:9443"
    );
}

#[test]
fn redacts_token() {
    assert_eq!(redact("ugpoa_v1.abcdef0123456789.secret"), "ugpoa_v1.****");
    assert_eq!(redact("short"), "****");
}

#[test]
fn redacts_non_standard_token_entirely() {
    // 非 ugpoa 前缀时不得回显首段明文。
    assert_eq!(redact("a.b.c"), "****");
    assert_eq!(redact("trailing."), "****");
}

#[test]
fn debug_output_never_leaks_token() {
    let c = cfg("nas.example.com", "ugpoa_v1.abcdef.secretsecret");
    let shown = format!("{c:?}");
    assert!(
        shown.contains("ugpoa_v1.****"),
        "脱敏后的形式应出现：{shown}"
    );
    assert!(!shown.contains("secretsecret"), "完整令牌不得出现：{shown}");
}

#[test]
fn missing_host_is_an_error() {
    let e = Config::from_parts(None, None, None, None).unwrap_err();
    assert!(matches!(e, ConfigError::MissingHost));
}

#[test]
fn missing_token_is_an_error() {
    let e = Config::from_parts(Some("nas.example.com".into()), None, None, None).unwrap_err();
    assert!(matches!(e, ConfigError::MissingToken));
}

#[test]
fn defaults_port_and_insecure() {
    let c = Config::from_parts(
        Some("nas.example.com".into()),
        Some("tok".into()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(c.port, 9443);
    assert!(c.insecure);
}

#[test]
fn insecure_can_be_disabled() {
    let c = Config::from_parts(Some("h".into()), Some("t".into()), None, Some("0".into())).unwrap();
    assert!(!c.insecure);
}

#[test]
fn bad_port_is_an_error() {
    let e = Config::from_parts(Some("h".into()), Some("t".into()), Some("abc".into()), None)
        .unwrap_err();
    assert!(matches!(e, ConfigError::BadPort(_)));
}

#[test]
fn blank_host_is_an_error() {
    let e = Config::from_parts(Some("   ".into()), Some("t".into()), None, None).unwrap_err();
    assert!(matches!(e, ConfigError::MissingHost));
}

#[test]
fn blank_token_is_an_error() {
    let e = Config::from_parts(Some("h".into()), Some("   ".into()), None, None).unwrap_err();
    assert!(matches!(e, ConfigError::MissingToken));
}

#[test]
fn error_messages_mention_the_right_variable() {
    let missing_host = ConfigError::MissingHost.to_string();
    assert!(missing_host.contains("UGNAS_HOST"), "{missing_host}");

    let missing_token = ConfigError::MissingToken.to_string();
    assert!(missing_token.contains("UGPOA_TOKEN"), "{missing_token}");

    let bad_port = ConfigError::BadPort("abc".to_string()).to_string();
    assert!(bad_port.contains("UGNAS_PORT"), "{bad_port}");
    assert!(bad_port.contains("abc"), "{bad_port}");
}
