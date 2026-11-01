use super::{PathRef, ProjectContractError};
use ProjectContractError::InvalidPath;

fn text(value: &str) -> Result<String, ProjectContractError> {
    if value.len() > 32768 || value.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}') {
        return Err(InvalidPath);
    }
    Ok(value.replace('/', "\\"))
}
fn component(value: &str) -> bool {
    let stem = value.split('.').next().unwrap_or("").to_ascii_lowercase();
    let reserved = matches!(
        stem.as_str(),
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
    ) || ["com", "lpt"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    !value.is_empty()
        && !reserved
        && !value.ends_with(['.', ' '])
        && !value.chars().any(|c| "<>:\"|?*".contains(c))
}
fn parts(value: &str) -> Result<String, ProjectContractError> {
    let mut result = Vec::new();
    for part in value.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                result.pop().ok_or(InvalidPath)?;
            }
            _ => {
                if !component(part) {
                    return Err(InvalidPath);
                }
                result.push(part);
            }
        }
    }
    Ok(result.join("\\"))
}
pub fn normalize_relative_path(value: &str) -> Result<String, ProjectContractError> {
    let clean = text(value)?;
    if clean.starts_with('\\') || clean.contains(':') {
        return Err(InvalidPath);
    }
    parts(&clean)
}
pub fn normalize_absolute_path(value: &str) -> Result<String, ProjectContractError> {
    let clean = text(value)?;
    let bytes = clean.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1..3] == *b":\\" {
        return Ok(format!(
            "{}:\\{}",
            char::from(bytes[0]).to_ascii_uppercase(),
            parts(&clean[3..])?
        ));
    }
    let mut components = clean.strip_prefix("\\\\").ok_or(InvalidPath)?.split('\\');
    let host = components.next().ok_or(InvalidPath)?;
    let share = components.next().ok_or(InvalidPath)?;
    if !component(host) || !component(share) {
        return Err(InvalidPath);
    }
    let tail = parts(&components.collect::<Vec<_>>().join("\\"))?;
    Ok(if tail.is_empty() {
        format!("\\\\{host}\\{share}")
    } else {
        format!("\\\\{host}\\{share}\\{tail}")
    })
}
impl PathRef {
    pub fn validate(&self) -> Result<(), ProjectContractError> {
        match self {
            Self::Relative { value } => {
                normalize_relative_path(value)?;
            }
            Self::Absolute { value } => {
                normalize_absolute_path(value)?;
            }
            Self::Empty { value } => {
                if !value.is_empty() {
                    return Err(InvalidPath);
                }
            }
        }
        Ok(())
    }
}
pub fn to_path_ref(root: &str, target: &str) -> Result<PathRef, ProjectContractError> {
    let base = normalize_absolute_path(root)?;
    let base = base.trim_end_matches('\\');
    if target.is_empty() {
        return Ok(PathRef::Empty {
            value: String::new(),
        });
    }
    let full = normalize_absolute_path(target)?;
    let clean = full.trim_end_matches('\\');
    if clean.eq_ignore_ascii_case(base) {
        return Ok(PathRef::Relative {
            value: String::new(),
        });
    }
    if clean
        .to_ascii_lowercase()
        .starts_with(&(base.to_ascii_lowercase() + "\\"))
    {
        return Ok(PathRef::Relative {
            value: clean[base.len() + 1..].to_owned(),
        });
    }
    Ok(PathRef::Absolute { value: full })
}
pub fn resolve_path_ref(
    root: &str,
    path: &PathRef,
) -> Result<Option<String>, ProjectContractError> {
    let base = normalize_absolute_path(root)?;
    path.validate()?;
    match path {
        PathRef::Empty { .. } => Ok(None),
        PathRef::Absolute { value } => normalize_absolute_path(value).map(Some),
        PathRef::Relative { value } => {
            let relative = normalize_relative_path(value)?;
            if relative.is_empty() {
                return Ok(Some(base));
            }
            normalize_absolute_path(&format!("{}\\{relative}", base.trim_end_matches('\\')))
                .map(Some)
        }
    }
}
