use super::*;
use crate::domain::operations::bounded_json;

impl OperationJournal {
    pub fn retire(&mut self, completion: &OperationCompletion) -> io::Result<()> {
        self.ready()?;
        completion.validate().map_err(files::invalid)?;
        if completion.report.status() == OperationStatus::InterruptedUnknown {
            return Err(files::invalid("unknown_result_must_be_retained"));
        }
        let intent = self
            .records
            .get(&completion.id)
            .ok_or_else(|| files::invalid("unknown_operation"))?;
        completion
            .report
            .validate_for_plan(&intent.plan)
            .map_err(files::invalid)?;
        let result = (|| {
            let path = self
                .directory
                .join(format!("{}.intent.json", completion.id));
            let marker = self
                .directory
                .join(format!("{}.retired.json", completion.id));
            let stored = files::read::<OperationIntent>(&path)?;
            let finished = files::read::<OperationCompletion>(
                &self
                    .directory
                    .join(format!("{}.result.json", completion.id)),
            )?
            .ok_or_else(|| files::invalid("missing_completion"))?;
            finished.validate().map_err(files::invalid)?;
            if stored.as_ref() != Some(intent)
                || bounded_json(&finished).map_err(files::invalid)?
                    != bounded_json(completion).map_err(files::invalid)?
            {
                return Err(files::invalid("changed_operation_record"));
            }
            files::check(&marker)?;
            match fs::symlink_metadata(&marker) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => return Err(files::invalid("retirement_already_exists")),
            }
            fs::rename(&path, &marker)?;
            files::finish_retirement(&self.directory, &completion.id)
        })();
        if result.is_ok() {
            self.records.remove(&completion.id);
        } else {
            self.writable = false;
        }
        result
    }
}
