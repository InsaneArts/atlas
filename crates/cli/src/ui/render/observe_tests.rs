use super::*;
use serde_json::json;

#[test]
fn an_oversized_app_name_cannot_amplify_padding_across_many_rows() {
    console::set_colors_enabled(false);

    let mut entries = vec![json!({
        "name": "a".repeat(1024 * 1024), "pid": 1, "policy": "regular"
    })];
    assert!(apps(&json!({"apps": &entries})).len() < 128);
    entries.extend((0..10_000).map(|_| json!({"name": "Short", "pid": 2, "policy": "regular"})));

    let shown = apps(&json!({"apps": entries}));
    assert!(shown.len() < 400_000);
    assert_eq!(shown.lines().count(), 10_002);
    assert!(shown.contains("aaaaaaaaaaaaaaaaaaaaaaa…  1"));
    assert!(shown.ends_with("Short                     2"));
}

#[test]
fn app_names_are_made_printable_and_clipped_by_display_width() {
    console::set_colors_enabled(false);

    let shown = apps(&json!({"apps": [{
        "name": format!("\n{}", "界".repeat(20)),
        "pid": 1, "policy": "regular", "active": true
    }]}));

    assert_eq!(
        shown,
        format!(
            "APP                      PID  STATE\n\\n{}…  1    frontmost",
            "界".repeat(10)
        )
    );
}
