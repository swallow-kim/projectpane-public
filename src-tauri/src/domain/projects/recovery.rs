use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryWarning {
    ColumnsReset,
    RowsReset,
    ActivePaneReset,
    ActiveTabReset,
}
#[derive(Clone, Debug)]
pub struct WorkspaceRecovery {
    pub original: Value,
    pub workspace: Workspace,
    pub warnings: Vec<RecoveryWarning>,
}
pub fn recover_workspace(original: &Value) -> Result<WorkspaceRecovery, ProjectContractError> {
    let mut value = original.clone();
    let mut warnings = Vec::new();
    for (key, warning) in [
        ("columns", RecoveryWarning::ColumnsReset),
        ("rows", RecoveryWarning::RowsReset),
    ] {
        if value
            .get(key)
            .and_then(Value::as_f64)
            .is_some_and(|n| n.is_finite() && !(20.0..=80.0).contains(&n))
        {
            value[key] = Value::from(50);
            warnings.push(warning);
        }
    }
    if let (Some(active), Some(layout)) = (
        value.get("activePane").and_then(Value::as_f64),
        value.get("layout").and_then(Value::as_f64),
    ) && [1.0, 2.0, 4.0].contains(&layout)
        && active.is_finite()
        && active.fract() == 0.0
        && active.abs() <= 9_007_199_254_740_991.0
        && (active < 0.0 || active >= layout)
    {
        value["activePane"] = Value::from(0);
        warnings.push(RecoveryWarning::ActivePaneReset);
    }
    if let Some(panes) = value.get_mut("panes").and_then(Value::as_array_mut) {
        for pane in panes {
            let active = pane.get("activeTabId").and_then(Value::as_str);
            let tabs = pane.get("tabs").and_then(Value::as_array);
            if let (Some(active), Some(tabs)) = (active, tabs)
                && super::validation::token(active)
                && !tabs
                    .iter()
                    .any(|tab| tab.get("id").and_then(Value::as_str) == Some(active))
                && let Some(first) = tabs.first().and_then(|tab| tab.get("id")).cloned()
            {
                pane["activeTabId"] = first;
                warnings.push(RecoveryWarning::ActiveTabReset);
            }
        }
    }
    let workspace: Workspace =
        serde_json::from_value(value).map_err(|_| ProjectContractError::InvalidDocument)?;
    workspace.validate()?;
    Ok(WorkspaceRecovery {
        original: original.clone(),
        workspace,
        warnings,
    })
}
