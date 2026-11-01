use super::*;
use serde_json::{Value, json};
const FIXTURE: &str = include_str!("../../../../tests/fixtures/projects/project-v1.json");

#[test]
fn supplementary_unicode_limits_use_utf16_units() {
    for (pointer, limit) in [
        ("/name", 256),
        ("/preset/panes/0/tabs/0/view/filter", 256),
        ("/preset/panes/0/tabs/0/scroll/entryId", 2048),
    ] {
        for count in [limit / 2, limit / 2 + 1] {
            let mut input: Value = serde_json::from_str(FIXTURE).unwrap();
            *input.pointer_mut(pointer).unwrap() = json!("😀".repeat(count));
            let parsed = parse_project(&serde_json::to_vec(&input).unwrap());
            if count * 2 == limit {
                assert!(parsed.is_ok(), "{pointer}");
            } else {
                assert_eq!(
                    parsed.unwrap_err(),
                    ProjectContractError::InvalidDocument,
                    "{pointer}"
                );
            }
        }
    }
}
#[test]
fn duplicate_properties_are_rejected_recursively_before_future_dispatch() {
    for input in [
        r#"{"schemaVersion":1,"schemaVersion":2,"lastProjectId":null}"#,
        r#"{"schemaVersion":2,"lastProjectId":null,"lastProject\u0049d":"other"}"#,
        r#"{"schemaVersion":2,"future":{"key":null,"key":true}}"#,
        r#"{"schemaVersion":2,"future":[{"key":null,"\u006bey":true}]}"#,
    ] {
        for error in [
            parse_project(input.as_bytes()).unwrap_err(),
            parse_session(input.as_bytes()).unwrap_err(),
            parse_settings(input.as_bytes()).unwrap_err(),
        ] {
            assert_eq!(error, ProjectContractError::InvalidDocument, "{input}");
        }
    }
}
#[test]
fn well_formed_future_schema_with_unknown_fields_stays_unsupported() {
    for version in ["2", "2e0", "2.0", "9007199254740991"] {
        let input = format!(
            r#"{{"schemaVersion":{version},"future":[{{"key":null}},{{"key":true}}],"other":{{"key":"value"}}}}"#
        );
        for error in [
            parse_project(input.as_bytes()).unwrap_err(),
            parse_session(input.as_bytes()).unwrap_err(),
            parse_settings(input.as_bytes()).unwrap_err(),
        ] {
            assert_eq!(error, ProjectContractError::UnsupportedSchema, "{input}");
        }
    }
}

#[test]
fn paths_match_shared_corpus() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/projects/path-cases.json"
    ))
    .unwrap();
    for case in cases["absolute"].as_array().unwrap() {
        assert_eq!(
            normalize_absolute_path(case["input"].as_str().unwrap()).unwrap(),
            case["output"].as_str().unwrap()
        );
    }
    for case in cases["relative"].as_array().unwrap() {
        assert_eq!(
            normalize_relative_path(case["input"].as_str().unwrap()).unwrap(),
            case["output"].as_str().unwrap()
        );
    }
    for case in cases["invalidAbsolute"].as_array().unwrap() {
        assert_eq!(
            normalize_absolute_path(case.as_str().unwrap()),
            Err(ProjectContractError::InvalidPath)
        );
    }
    for case in cases["invalidRelative"].as_array().unwrap() {
        assert_eq!(
            normalize_relative_path(case.as_str().unwrap()),
            Err(ProjectContractError::InvalidPath)
        );
    }
}
#[test]
fn corruption_matches_shared_corpus() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/projects/corruption-cases.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let mut input: Value = serde_json::from_str(FIXTURE).unwrap();
        let pointer = case["pointer"].as_str().unwrap();
        *input.pointer_mut(pointer).unwrap() = case["value"].clone();
        assert_eq!(
            parse_project(&serde_json::to_vec(&input).unwrap()).unwrap_err(),
            ProjectContractError::InvalidDocument,
            "{pointer}"
        );
    }
}
#[test]
fn paths_distinguish_root_empty_and_prefix() {
    assert_eq!(
        to_path_ref(r"C:\Example", r"c:/Example/src").unwrap(),
        PathRef::Relative {
            value: "src".into()
        }
    );
    assert_eq!(
        to_path_ref(r"C:\Example", r"C:\Examples").unwrap(),
        PathRef::Absolute {
            value: r"C:\Examples".into()
        }
    );
    assert_eq!(
        resolve_path_ref(
            r"\\host\share",
            &PathRef::Relative {
                value: "src".into()
            }
        )
        .unwrap(),
        Some(r"\\host\share\src".into())
    );
    assert_eq!(
        resolve_path_ref(
            r"C:\Example",
            &PathRef::Empty {
                value: String::new()
            }
        )
        .unwrap(),
        None
    );
    assert_eq!(
        resolve_path_ref(
            r"C:\Example",
            &PathRef::Relative {
                value: String::new()
            }
        )
        .unwrap(),
        Some(r"C:\Example".into())
    );
    assert!(matches!(
        to_path_ref(r"C:\Ä", r"C:\ä\x").unwrap(),
        PathRef::Absolute { .. }
    ));
}
#[test]
fn session_and_settings_round_trip() {
    let project = parse_project(FIXTURE.as_bytes()).unwrap();
    let session = Session {
        schema_version: 1,
        project_id: project.id,
        project_revision: 0,
        revision: 1,
        workspace: project.preset,
    };
    assert_eq!(
        parse_session(&serialize_session(&session).unwrap()).unwrap(),
        session
    );
    let settings = Settings {
        schema_version: 1,
        last_project_id: None,
    };
    assert_eq!(
        parse_settings(&serialize_settings(&settings).unwrap()).unwrap(),
        settings
    );
}
#[test]
fn required_nulls_unknown_fields_and_duplicate_keys_are_rejected() {
    for input in [
        r#"{"schemaVersion":1}"#,
        r#"{"schemaVersion":1,"lastProjectId":null,"extra":0}"#,
        r#"{"schemaVersion":1,"lastProjectId":null,"lastProject\u0049d":"other"}"#,
    ] {
        assert_eq!(
            parse_settings(input.as_bytes()).unwrap_err(),
            ProjectContractError::InvalidDocument
        );
    }
    let mut project: Value = serde_json::from_str(FIXTURE).unwrap();
    project["preset"]["panes"][0]["tabs"][0]["scroll"]
        .as_object_mut()
        .unwrap()
        .remove("entryId");
    assert!(parse_project(&serde_json::to_vec(&project).unwrap()).is_err());
}
#[test]
fn exact_byte_bound_is_allowed_but_next_byte_rejected() {
    let mut bytes = FIXTURE.as_bytes().to_vec();
    bytes.resize(MAX_PROJECT_BYTES, b' ');
    assert!(parse_project(&bytes).is_ok());
    bytes.push(b' ');
    assert_eq!(
        parse_project(&bytes).unwrap_err(),
        ProjectContractError::PayloadLimit
    );
}
#[test]
fn lone_surrogate_and_future_schema_are_distinct() {
    assert_eq!(
        parse_project(br#"{"schemaVersion":2}"#).unwrap_err(),
        ProjectContractError::UnsupportedSchema
    );
    let input = FIXTURE.replace("\"Example\"", r#""\ud800""#);
    assert_eq!(
        parse_project(input.as_bytes()).unwrap_err(),
        ProjectContractError::InvalidDocument
    );
}
#[test]
fn recovery_preserves_original_and_warns_without_renumbering() {
    let project: Value = serde_json::from_str(FIXTURE).unwrap();
    let mut input = project["preset"].clone();
    input["columns"] = json!(200);
    input["activePane"] = json!(9);
    input["panes"][0]["activeTabId"] = json!("missing");
    let before = input.clone();
    let result = recover_workspace(&input).unwrap();
    assert_eq!(input, before);
    assert_eq!(result.original, before);
    assert_eq!(
        result.workspace,
        parse_project(FIXTURE.as_bytes()).unwrap().preset
    );
    assert_eq!(
        result.warnings,
        [
            RecoveryWarning::ColumnsReset,
            RecoveryWarning::ActivePaneReset,
            RecoveryWarning::ActiveTabReset
        ]
    );
    input["panes"][1]["id"] = json!("pane-0");
    assert_eq!(
        recover_workspace(&input).unwrap_err(),
        ProjectContractError::InvalidDocument
    );
}
#[test]
fn too_many_tabs_and_wrong_pane_counts_are_rejected() {
    let mut project: Value = serde_json::from_str(FIXTURE).unwrap();
    project["preset"]["panes"][0]["tabs"] = Value::Array(
        (0..33)
            .map(|i| {
                let mut tab = project["preset"]["panes"][1]["tabs"][0].clone();
                tab["id"] = json!(format!("test-{i}"));
                tab
            })
            .collect(),
    );
    assert!(parse_project(&serde_json::to_vec(&project).unwrap()).is_err());
    project["preset"]["panes"] = json!([]);
    assert!(parse_project(&serde_json::to_vec(&project).unwrap()).is_err());
}
#[test]
fn integer_exponents_and_maximum_revision_are_accepted() {
    let mut project = parse_project(FIXTURE.as_bytes()).unwrap();
    project.revision = MAX_SAFE_REVISION;
    assert_eq!(
        parse_project(&serialize_project(&project).unwrap()).unwrap(),
        project
    );
    let input = FIXTURE.replace("\"revision\": 0", "\"revision\": 0e0");
    assert_eq!(parse_project(input.as_bytes()).unwrap().revision, 0);
}

#[test]
fn negative_zero_matches_json_number_semantics() {
    let input = FIXTURE.replace("\"revision\": 0", "\"revision\": -0");
    assert_eq!(parse_project(input.as_bytes()).unwrap().revision, 0);
}
#[test]
fn recovery_refuses_unsafe_active_integer() {
    let mut input: Value = serde_json::from_str(FIXTURE).unwrap();
    input["preset"]["activePane"] = json!(9007199254740992u64);
    assert_eq!(
        recover_workspace(&input["preset"]).unwrap_err(),
        ProjectContractError::InvalidDocument
    );
}
