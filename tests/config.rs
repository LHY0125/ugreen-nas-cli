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
