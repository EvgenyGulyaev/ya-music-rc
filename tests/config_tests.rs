use ya_player::config::AppConfig;

#[test]
fn config_roundtrip_preserves_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.json");

    let config = AppConfig {
        token: Some("secret-token".to_owned()),
        volume_percent: 85,
    };

    config.save_to_path(&path).expect("save config");
    let loaded = AppConfig::load_from_path(&path).expect("load config");

    assert_eq!(loaded.token.as_deref(), Some("secret-token"));
    assert_eq!(loaded.volume_percent, 85);
}

#[test]
fn missing_config_loads_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing.json");

    let loaded = AppConfig::load_from_path(&path).expect("load default");

    assert!(loaded.token.is_none());
    assert_eq!(loaded.volume_percent, 100);
}

#[test]
fn legacy_config_without_volume_uses_default_volume() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.json");
    std::fs::write(&path, r#"{"token":"secret-token"}"#).expect("write config");

    let loaded = AppConfig::load_from_path(&path).expect("load legacy config");

    assert_eq!(loaded.token.as_deref(), Some("secret-token"));
    assert_eq!(loaded.volume_percent, 100);
}

#[test]
fn redacted_token_keeps_edges_only() {
    assert_eq!(AppConfig::redact_token("abcdef123456"), "abcd...3456");
    assert_eq!(AppConfig::redact_token("short"), "*****");
}
