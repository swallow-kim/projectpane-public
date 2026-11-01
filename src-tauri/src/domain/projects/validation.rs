use super::*;
use std::collections::HashSet;

pub(super) fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn invalid() -> ProjectContractError {
    ProjectContractError::InvalidDocument
}
fn version(value: u32) -> Result<(), ProjectContractError> {
    match value {
        1 => Ok(()),
        2.. => Err(ProjectContractError::UnsupportedSchema),
        0 => Err(invalid()),
    }
}
impl Workspace {
    pub fn validate(&self) -> Result<(), ProjectContractError> {
        if !matches!(self.layout, 1 | 2 | 4)
            || self.active_pane >= usize::from(self.layout)
            || !self.columns.is_finite()
            || !(20.0..=80.0).contains(&self.columns)
            || !self.rows.is_finite()
            || !(20.0..=80.0).contains(&self.rows)
        {
            return Err(invalid());
        }
        let mut panes = HashSet::new();
        let mut tabs = HashSet::new();
        for pane in &self.panes {
            if !token(&pane.id)
                || !panes.insert(&pane.id)
                || !token(&pane.active_tab_id)
                || pane.tabs.is_empty()
                || pane.tabs.len() > 32
                || !pane.tabs.iter().any(|tab| tab.id == pane.active_tab_id)
            {
                return Err(invalid());
            }
            for tab in &pane.tabs {
                if !token(&tab.id) || !tabs.insert(&tab.id) {
                    return Err(invalid());
                }
                tab.path.validate().map_err(|_| invalid())?;
                tab.view.validate().map_err(|_| invalid())?;
                let scroll = &tab.scroll;
                if scroll.index >= 100_000
                    || !scroll.intra_row_offset.is_finite()
                    || !(0.0..30.0).contains(&scroll.intra_row_offset)
                    || scroll.entry_id.as_ref().is_some_and(|id| {
                        id.is_empty() || id.encode_utf16().count() > 2048 || id.contains('\0')
                    })
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}
impl Project {
    pub fn validate(&self) -> Result<(), ProjectContractError> {
        version(self.schema_version)?;
        // Match ECMAScript whitespace explicitly (Rust trim also strips U+0085).
        let blank = self.name.chars().all(|c| {
            matches!(c,
            '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
        });
        if !token(&self.id)
            || self.revision > MAX_SAFE_REVISION
            || blank
            || self.name.len() > 1024
            || self.name.encode_utf16().count() > 256
            || self.name.chars().any(|c| c <= '\u{1f}')
        {
            return Err(invalid());
        }
        normalize_absolute_path(&self.root_path).map_err(|_| invalid())?;
        self.preset.validate()
    }
}
impl Session {
    pub fn validate(&self) -> Result<(), ProjectContractError> {
        version(self.schema_version)?;
        if !token(&self.project_id)
            || self.revision > MAX_SAFE_REVISION
            || self.project_revision > MAX_SAFE_REVISION
        {
            return Err(invalid());
        }
        self.workspace.validate()
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), ProjectContractError> {
        version(self.schema_version)?;
        if self.last_project_id.as_ref().is_some_and(|id| !token(id)) {
            return Err(invalid());
        }
        Ok(())
    }
}
