use super::*;
const FIXTURE: &str = include_str!("../../../../tests/fixtures/projects/project-v1.json");

#[test]
fn project_round_trip_when_valid() {
    let input: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let parsed = parse_project(FIXTURE.as_bytes()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), input);
}
#[test]
fn project_rejects_corruption_when_bounded_fields_invalid() {
    for (pointer, value) in [
        ("/revision", serde_json::json!(9007199254740992u64)),
        ("/preset/columns", serde_json::json!(0)),
        ("/preset/panes/0/activeTabId", serde_json::json!("missing")),
        ("/preset/panes/1/id", serde_json::json!("pane-0")),
        ("/preset/panes/1/tabs/0/id", serde_json::json!("tab-0")),
        (
            "/preset/panes/0/tabs/0/path/value",
            serde_json::json!("..\\escape"),
        ),
        ("/rootPath", serde_json::json!("C:relative")),
    ] {
        let mut input: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        *input.pointer_mut(pointer).unwrap() = value;
        assert!(
            parse_project(&serde_json::to_vec(&input).unwrap()).is_err(),
            "{pointer}"
        );
    }
}
#[test]
fn future_schema_is_rejected_when_unknown() {
    assert!(parse_project(br#"{"schemaVersion":2}"#).is_err());
}

#[test]
fn serialized_payload_is_bounded_even_when_individual_fields_are_valid() {
    let mut project = parse_project(FIXTURE.as_bytes()).unwrap();
    for (i, pane) in project.preset.panes.iter_mut().enumerate() {
        let tab = pane.tabs[0].clone();
        pane.active_tab_id = format!("large-{i}-0");
        pane.tabs = (0..32)
            .map(|j| {
                let mut tab = tab.clone();
                tab.id = format!("large-{i}-{j}");
                tab.scroll.entry_id = Some("x".repeat(2048));
                tab
            })
            .collect();
    }
    assert_eq!(
        serialize_project(&project).unwrap_err(),
        ProjectContractError::PayloadLimit
    );
}
