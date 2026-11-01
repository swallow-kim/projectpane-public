use super::*;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;

// Preserve duplicate-key evidence at every depth before inspecting schemaVersion.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueValueVisitor)
    }
}

struct UniqueValueVisitor;

impl<'de> Visitor<'de> for UniqueValueVisitor {
    type Value = UniqueValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("JSON with unique object keys")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Null))
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::Bool(value)))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::from(value)))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::from(value)))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(|number| UniqueValue(Value::Number(number)))
            .ok_or_else(|| E::custom("invalid JSON number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(UniqueValue(Value::String(value.to_owned())))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(UniqueValue(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(UniqueValue(Value::Array(values)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Self::Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            let UniqueValue(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(UniqueValue(Value::Object(values)))
    }
}

fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProjectContractError> {
    if bytes.len() > MAX_PROJECT_BYTES {
        return Err(ProjectContractError::PayloadLimit);
    }
    let UniqueValue(value) =
        serde_json::from_slice(bytes).map_err(|_| ProjectContractError::InvalidDocument)?;
    if value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_f64)
        .is_some_and(|version| {
            version.fract() == 0.0 && (2.0..=9_007_199_254_740_991.0).contains(&version)
        })
    {
        return Err(ProjectContractError::UnsupportedSchema);
    }
    // Keep the original numeric spellings for typed serde decoding.
    serde_json::from_slice(bytes).map_err(|_| ProjectContractError::InvalidDocument)
}
fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectContractError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ProjectContractError::InvalidDocument)?;
    if bytes.len() > MAX_PROJECT_BYTES {
        return Err(ProjectContractError::PayloadLimit);
    }
    Ok(bytes)
}
pub fn parse_project(bytes: &[u8]) -> Result<Project, ProjectContractError> {
    let value: Project = parse(bytes)?;
    value.validate()?;
    Ok(value)
}
pub fn parse_session(bytes: &[u8]) -> Result<Session, ProjectContractError> {
    let value: Session = parse(bytes)?;
    value.validate()?;
    Ok(value)
}
pub fn parse_settings(bytes: &[u8]) -> Result<Settings, ProjectContractError> {
    let value: Settings = parse(bytes)?;
    value.validate()?;
    Ok(value)
}
pub fn serialize_project(value: &Project) -> Result<Vec<u8>, ProjectContractError> {
    value.validate()?;
    serialize(value)
}
pub fn serialize_session(value: &Session) -> Result<Vec<u8>, ProjectContractError> {
    value.validate()?;
    serialize(value)
}
pub fn serialize_settings(value: &Settings) -> Result<Vec<u8>, ProjectContractError> {
    value.validate()?;
    serialize(value)
}
