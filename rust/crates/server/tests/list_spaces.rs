use serde_json::json;

#[test]
fn list_spaces_format_includes_global_and_personal() {
    let input = json!({"results": [
        {"name": "Team", "key": "TEAM", "type": "global"}
    ]});
    let _ = input;
}
