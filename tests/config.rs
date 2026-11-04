use nascli::config::{
    config_root_for, default_config_path, redact, Config, ConfigError, Platform,
    DEFAULT_CONFIG_FILE,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, NamedTempFile};

fn cfg(host: &str, token: &str) -> Config {
    Config {
        host: host.into(),
        port: 9443,
        token: token.into(),
        insecure: true,
    }
}

fn parse(text: &str) -> Result<Config, ConfigError> {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.flush().unwrap();
    Config::from_file(file.path())
}

#[test]
fn config_file_name_is_stable() {
    assert_eq!(DEFAULT_CONFIG_FILE, "config.toml");
}

#[test]
fn windows_uses_appdata() {
    let appdata = Path::new("C:/Users/u/AppData/Roaming");
    let root = config_root_for(Platform::Windows, Some(appdata), None, None);
    assert_eq!(root, Some(PathBuf::from("C:/Users/u/AppData/Roaming")));
}

#[test]
fn windows_without_appdata_has_no_root() {
    let root = config_root_for(
        Platform::Windows,
        None,
        Some(Path::new("/xdg")),
        Some(Path::new("/home/u")),
    );
    assert_eq!(root, None);
}

#[test]
fn linux_prefers_xdg_over_home() {
    let root = config_root_for(
        Platform::Linux,
        None,
        Some(Path::new("/xdg")),
        Some(Path::new("/home/u")),
    );
    assert_eq!(root, Some(PathBuf::from("/xdg")));
}

#[test]
fn linux_falls_back_to_home_dot_config() {
    let root = config_root_for(Platform::Linux, None, None, Some(Path::new("/home/u")));
    assert_eq!(root, Some(PathBuf::from("/home/u/.config")));
}

#[test]
fn macos_uses_application_support() {
    let root = config_root_for(Platform::MacOs, None, None, Some(Path::new("/Users/u")));
    assert_eq!(
        root,
        Some(PathBuf::from("/Users/u/Library/Application Support"))
    );
}

#[test]
fn default_path_ends_with_nascli_config_toml() {
    let path = default_config_path();
    assert!(
        path.ends_with(Path::new("nascli").join(DEFAULT_CONFIG_FILE))
            || path == Path::new(DEFAULT_CONFIG_FILE),
        "默认路径应以 nascli/config.toml 结尾，实际为 {}",
        path.display()
    );
}

#[test]
fn write_file_roundtrips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nested/config.toml");
    let mut c = cfg("nas.example.com", "ugpoa_v1.abcdef.secret");
    c.port = 9444;
    c.insecure = false;
    c.write_file(&path).unwrap();

    let read = Config::from_file(&path).unwrap();
    assert_eq!(read.host, "nas.example.com");
    assert_eq!(read.port, 9444);
    assert_eq!(read.token, "ugpoa_v1.abcdef.secret");
    assert!(!read.insecure);
}

#[test]
fn redacted_toml_hides_token() {
    let c = cfg("nas.example.com", "ugpoa_v1.abcdef.secretsecret");
    let shown = c.redacted_toml();
    assert!(shown.contains("ugpoa_v1.****"), "{shown}");
    assert!(!shown.contains("secretsecret"), "{shown}");
}

#[test]
fn set_field_updates_and_validates() {
    let mut c = cfg("nas.example.com", "tok");
    c.set_field("port", "9444").unwrap();
    c.set_field("insecure", "false").unwrap();
    c.set_field("host", "nas2.example.com").unwrap();
    c.set_field("token", "tok2").unwrap();
    assert_eq!(c.port, 9444);
    assert!(!c.insecure);
    assert_eq!(c.host, "nas2.example.com");
    assert_eq!(c.token, "tok2");

    assert!(c.set_field("nope", "x").is_err());
    assert!(c.set_field("port", "0").is_err());
    assert!(c.set_field("host", "https://bad.example.com").is_err());
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
fn reads_valid_toml_with_defaults() {
    let c = parse(
        r#"
host = "nas.example.com"
token = "tok"
"#,
    )
    .unwrap();
    assert_eq!(c.host, "nas.example.com");
    assert_eq!(c.port, 9443);
    assert_eq!(c.token, "tok");
    assert!(c.insecure);
}

#[test]
fn insecure_can_be_disabled() {
    let c = parse(
        r#"
host = "nas.example.com"
token = "tok"
insecure = false
"#,
    )
    .unwrap();
    assert!(!c.insecure);
}

#[test]
fn trims_host_and_token() {
    let c = parse(
        r#"
host = "  nas.example.com  "
token = "  tok  "
"#,
    )
    .unwrap();
    assert_eq!(c.host, "nas.example.com");
    assert_eq!(c.token, "tok");
}

#[test]
fn missing_file_is_a_read_error() {
    let path = std::path::Path::new("definitely-not-a-real-config.toml");
    let e = Config::from_file(path).unwrap_err();
    assert!(matches!(e, ConfigError::Read { .. }), "{e:?}");
    assert!(e.to_string().contains("config.toml"), "{e}");
}

#[test]
fn missing_host_is_rejected() {
    let e = parse(r#"token = "tok""#).unwrap_err();
    assert!(matches!(e, ConfigError::Parse { .. }), "{e:?}");
}

#[test]
fn missing_token_is_rejected() {
    let e = parse(r#"host = "h""#).unwrap_err();
    assert!(matches!(e, ConfigError::Parse { .. }), "{e:?}");
}

#[test]
fn blank_host_is_rejected() {
    let e = parse(
        r#"
host = "   "
token = "tok"
"#,
    )
    .unwrap_err();
    assert!(matches!(e, ConfigError::MissingHost), "{e:?}");
}

#[test]
fn blank_token_is_rejected() {
    let e = parse(
        r#"
host = "h"
token = "   "
"#,
    )
    .unwrap_err();
    assert!(matches!(e, ConfigError::MissingToken), "{e:?}");
}

#[test]
fn unknown_key_is_rejected() {
    let e = parse(
        r#"
host = "h"
token = "t"
typo = true
"#,
    )
    .unwrap_err();
    assert!(matches!(e, ConfigError::Parse { .. }), "{e:?}");
    assert!(e.to_string().contains("typo"), "{e}");
}

#[test]
fn rejects_scheme_or_path_in_host() {
    for host in ["https://nas.example.com", "nas.example.com/path"] {
        let e = parse(&format!(
            r#"
host = "{host}"
token = "t"
"#
        ))
        .unwrap_err();
        assert!(
            matches!(e, ConfigError::BadHost(_)),
            "host={host}, err={e:?}"
        );
    }
}

#[test]
fn brackets_ipv6_host() {
    let c = parse(
        r#"
host = "::1"
token = "t"
"#,
    )
    .unwrap();
    assert_eq!(c.base_url(), "https://[::1]:9443");
}

#[test]
fn rejects_zero_port() {
    let e = parse(
        r#"
host = "h"
token = "t"
port = 0
"#,
    )
    .unwrap_err();
    assert!(matches!(e, ConfigError::BadPort(_)), "{e:?}");
}

#[test]
fn bad_port_type_is_rejected() {
    let e = parse(
        r#"
host = "h"
token = "t"
port = "abc"
"#,
    )
    .unwrap_err();
    assert!(matches!(e, ConfigError::Parse { .. }), "{e:?}");
}

#[test]
fn error_messages_mention_the_right_field() {
    assert!(ConfigError::MissingHost.to_string().contains("host"));
    assert!(ConfigError::MissingToken.to_string().contains("token"));
    assert!(ConfigError::BadPort("0".into())
        .to_string()
        .contains("port"));
    assert!(ConfigError::Read {
        path: "config.toml".into(),
        error: "not found".into()
    }
    .to_string()
    .contains("config.toml"));
}
