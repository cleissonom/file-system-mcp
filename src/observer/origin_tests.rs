use super::{Attribution, Origin, Source, Transport};
use serde_json::json;

#[test]
fn only_fixed_product_labels_are_accepted() {
    let labels = [
        "chatgpt",
        "chatgpt_work",
        "codex",
        "codex_cloud",
        "openai_dot",
        "unknown",
    ];
    for label in labels {
        assert_eq!(Source::parse(label).unwrap().as_str(), label);
    }
    for invalid in [
        "",
        "ChatGPT/private-user",
        "sk-private-sentinel",
        "/private/root",
    ] {
        assert_eq!(Source::parse(invalid), None);
    }
}

#[test]
fn caller_tags_are_per_request_and_explicitly_unverified() {
    let params = json!({"_meta": {"file-system-mcp/source": "codex_cloud"}});
    let origin = Origin::for_call(Some(&params), Source::Chatgpt, Transport::Tunnel);
    assert_eq!(origin.source, Source::CodexCloud);
    assert_eq!(origin.attribution, Attribution::ClientReported);
    assert_eq!(origin.transport, Transport::Tunnel);
}

#[test]
fn an_operator_label_is_used_only_when_a_caller_tag_is_absent() {
    let origin = Origin::for_call(None, Source::ChatgptWork, Transport::Stdio);
    assert_eq!(origin.source, Source::ChatgptWork);
    assert_eq!(origin.attribution, Attribution::OperatorConfigured);
    assert_eq!(origin.transport, Transport::Stdio);
    let invalid = json!({"_meta": {"file-system-mcp/source": "private-secret"}});
    let origin = Origin::for_call(Some(&invalid), Source::ChatgptWork, Transport::Stdio);
    assert_eq!(origin.source, Source::Unknown);
    assert_eq!(origin.attribution, Attribution::Unknown);
}

#[test]
fn unrelated_client_hints_and_identity_metadata_are_not_origin_evidence() {
    let params = json!({"clientInfo":{"name":"chatgpt"},"_meta": {
        "openai/userAgent":"ChatGPT/private-device", "openai/session":"private-session",
        "openai/subject":"private-user", "openai/organization":"private-org"
    }});
    let origin = Origin::for_call(Some(&params), Source::Unknown, Transport::Tunnel);
    let encoded = serde_json::to_string(&origin).unwrap();
    assert_eq!(origin.source, Source::Unknown);
    assert_eq!(origin.attribution, Attribution::Unknown);
    assert_eq!(origin.transport, Transport::Tunnel);
    assert!(!encoded.contains("private"));
}

#[test]
fn storage_values_are_normalized_without_retaining_arbitrary_labels() {
    let origin = Origin::from_storage("chatgpt_work", "operator_configured", "stdio");
    assert_eq!(origin.source, Source::ChatgptWork);
    assert_eq!(origin.attribution, Attribution::OperatorConfigured);
    assert_eq!(origin.transport, Transport::Stdio);
    let unknown = Origin::from_storage("private-secret", "client_reported", "private-host");
    assert_eq!(unknown, Origin::default());
    assert_eq!(Attribution::ClientReported.as_str(), "client_reported");
    assert_eq!(Transport::Tunnel.as_str(), "tunnel");
}
