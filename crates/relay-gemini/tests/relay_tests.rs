use relay_gemini::GeminiRelay;

#[test]
fn test_api_base_uses_cloudcode() {
    let api_base = GeminiRelay::default_api_base();

    assert!(
        api_base.contains("cloudcode.googleapis.com"),
        "Should use cloudcode.googleapis.com, got: {}",
        api_base
    );
    assert!(
        !api_base.contains("generativelanguage.googleapis.com"),
        "Should NOT use generativelanguage.googleapis.com"
    );
}
