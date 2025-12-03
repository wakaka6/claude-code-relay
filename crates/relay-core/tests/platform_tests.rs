use relay_core::Platform;

#[test]
fn test_platform_codex_exists() {
    let platform = Platform::Codex;
    assert_eq!(format!("{:?}", platform), "Codex");
}

#[test]
fn test_platform_codex_display() {
    let platform = Platform::Codex;
    assert_eq!(platform.to_string(), "codex");
}

#[test]
fn test_platform_codex_serde() {
    let platform = Platform::Codex;
    let json = serde_json::to_string(&platform).unwrap();
    assert_eq!(json, "\"codex\"");

    let parsed: Platform = serde_json::from_str("\"codex\"").unwrap();
    assert_eq!(parsed, Platform::Codex);
}
