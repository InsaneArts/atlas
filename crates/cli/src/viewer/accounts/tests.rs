use super::*;

#[test]
fn an_app_is_added_only_by_its_id_and_name() {
    for body in [
        json!({}),
        json!({"id": "com.tinyspeck.slackmacgap"}),
        json!({"id": " ", "name": "Slack"}),
        json!({"id": "x".repeat(201), "name": "Slack"}),
    ] {
        assert_eq!(add(&body).unwrap_err().code, Code::Input, "{body}");
    }
}

#[test]
fn an_account_is_removed_only_by_its_kind_and_id() {
    for body in [
        json!({"id": "github.com"}),
        json!({"kind": "page", "id": "github.com"}),
        json!({"kind": "site"}),
    ] {
        assert_eq!(remove(&body).unwrap_err().code, Code::Input, "{body}");
    }
}
