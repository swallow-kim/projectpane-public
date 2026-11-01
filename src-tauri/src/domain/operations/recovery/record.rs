use super::{RecoveryPhase, model::Record, validate};
use crate::domain::{Fault, FaultCode, MAX_FRAME};

/// Validated staged replacement record. Construction grants no mutation authority.
#[derive(Debug)]
pub struct RecoveryRecord(Record);

impl RecoveryRecord {
    /// Parses one record, independently of current filesystem observations.
    ///
    /// # Errors
    /// Rejects invalid schema, binding, identity, paths, content or frame overflow.
    pub fn parse(bytes: &[u8]) -> Result<Self, Fault> {
        if bytes.len() >= MAX_FRAME {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        let record =
            serde_json::from_slice(bytes).map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
        validate::record(&record)?;
        Ok(Self(record))
    }

    /// Returns a bounded canonical representation without exposing mutable fields.
    ///
    /// # Errors
    /// Returns a payload fault if the serialized record exceeds the existing frame.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Fault> {
        super::super::bounded_json(&self.0)
    }

    pub fn id(&self) -> &str {
        &self.0.id
    }

    pub const fn phase(&self) -> RecoveryPhase {
        self.0.phase
    }

    #[cfg(windows)]
    pub(crate) fn observe_with(
        &self,
        mut observe: impl FnMut(&str) -> super::FileObservation,
    ) -> super::RecoveryAssessment {
        let observations = super::model::Observations {
            parent: observe(&self.0.parent.path),
            artifacts: observe(&self.0.artifacts.path),
            target: observe(&self.0.original.object.path),
            staged: observe(&self.0.staged.object.path),
            backup: observe(&self.0.backup_path),
        };
        let input = super::model::Inspection {
            record: self.0.clone(),
            observations,
        };
        if validate::inspection(&input).is_err() {
            return super::RecoveryAssessment::Unknown;
        }
        super::classify(&input)
    }

    /// Requires the immediate next phase and equality of every immutable field.
    #[must_use]
    pub fn follows(&self, previous: &Self) -> bool {
        if previous.phase().next() != Some(self.phase()) {
            return false;
        }
        // Bounded copies include every binding field in equality, including future fields.
        let mut binding = previous.0.clone();
        binding.phase = self.phase();
        self.0 == binding
    }
}

impl RecoveryPhase {
    pub const fn file_label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::PreserveRequested => "preserve_requested",
            Self::OriginalPreserved => "original_preserved",
            Self::InstallRequested => "install_requested",
            Self::InstallObserved => "install_observed",
        }
    }

    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Ready => Some(Self::PreserveRequested),
            Self::PreserveRequested => Some(Self::OriginalPreserved),
            Self::OriginalPreserved => Some(Self::InstallRequested),
            Self::InstallRequested => Some(Self::InstallObserved),
            Self::InstallObserved => None,
        }
    }
}
