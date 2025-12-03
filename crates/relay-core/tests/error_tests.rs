use relay_core::RelayError;

#[test]
fn test_organization_disabled_error() {
    let body = r#"{"error": {"message": "Your organization has been disabled"}}"#;
    let error = RelayError::from_response_body(403, body);

    match error {
        RelayError::OrganizationDisabled(_) => {}
        _ => panic!("Expected OrganizationDisabled error, got: {:?}", error),
    }
}

#[test]
fn test_overloaded_error_with_retry_duration() {
    let error = RelayError::from_response_body(529, "API overloaded");

    match error {
        RelayError::Overloaded { retry_after_minutes } => {
            assert_eq!(retry_after_minutes, 5);
        }
        _ => panic!("Expected Overloaded error, got: {:?}", error),
    }
}

#[test]
fn test_opus_weekly_limit_detection() {
    let body = r#"{"error": {"message": "You have exceeded your weekly usage limit for claude-3-opus"}}"#;
    let error = RelayError::from_response_body(429, body);

    match error {
        RelayError::OpusWeeklyLimit => {}
        _ => panic!("Expected OpusWeeklyLimit error, got: {:?}", error),
    }
}

#[test]
fn test_normal_rate_limit() {
    let body = r#"{"error": {"message": "Rate limit exceeded"}}"#;
    let error = RelayError::from_response_body(429, body);

    match error {
        RelayError::RateLimited(_) => {}
        _ => panic!("Expected RateLimited error, got: {:?}", error),
    }
}

#[test]
fn test_unauthorized_error() {
    let body = r#"{"error": {"message": "Invalid API key"}}"#;
    let error = RelayError::from_response_body(401, body);

    match error {
        RelayError::Unauthorized(_) => {}
        _ => panic!("Expected Unauthorized error, got: {:?}", error),
    }
}

#[test]
fn test_insufficient_quota_error() {
    let error = RelayError::from_response_body(402, "Payment required");

    match error {
        RelayError::InsufficientQuota => {}
        _ => panic!("Expected InsufficientQuota error, got: {:?}", error),
    }
}

#[test]
fn test_insufficient_quota_json_response() {
    let error = RelayError::InsufficientQuota;
    let json = error.to_json_error();

    assert_eq!(json["type"], "error");
    assert_eq!(json["error"]["code"], "402");
    assert_eq!(json["error"]["type"], "insufficient_quota");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Insufficient balance"));
}
