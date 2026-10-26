use super::SkillDirectoryError;
use ora_effect::EffectOperation;
use ora_logging::{ErrorReport, ora_info, ora_warn};
use std::error::Error;
use std::path::Path;
use std::time::Instant;

/// Names both sides of an atomic swap and reports contention without changing authority.
pub(super) fn rename_step(action: &'static str, from: &Path, to: &Path) -> std::io::Result<()> {
    let started = Instant::now();
    let result = ora_utils::fs::rename_with_retry(from, to);
    match &result {
        Ok(attempts) => ora_info!(operation = "skill_materialization", stage = action,
            source_path = %from.display(), destination_path = %to.display(), attempts,
            duration_ms = started.elapsed().as_millis() as u64,
            "Skill directory rename completed"),
        Err(error) => ora_warn!(operation = "skill_materialization", stage = action,
            source_path = %from.display(), destination_path = %to.display(),
            os_error = error.raw_os_error(), duration_ms = started.elapsed().as_millis() as u64,
            error = %error, "Skill directory rename failed after bounded retry"),
    }
    result.map(|_| ())
}

/// Correlates adapter evidence with the immutable journal without logging package contents.
pub(super) fn operation_step<T>(
    operation: &EffectOperation,
    stage: &'static str,
    run: impl FnOnce() -> Result<T, SkillDirectoryError>,
) -> Result<T, SkillDirectoryError> {
    let started = Instant::now();
    ora_info!(
        operation = "skill_materialization", operation_id = operation.identity().as_str(),
        attempt_id = operation.attempt().as_str(), resource_id = operation.resource().as_str(),
        generation = operation.generation().value(), mutation = ?operation.mutation(),
        stage, outcome = "started", "Skill operation stage started",
    );
    let result = run();
    match &result {
        Ok(_) => ora_info!(
            operation = "skill_materialization",
            operation_id = operation.identity().as_str(),
            resource_id = operation.resource().as_str(),
            stage,
            outcome = "success",
            duration_ms = started.elapsed().as_millis() as u64,
            "Skill operation stage completed",
        ),
        Err(
            SkillDirectoryError::VerificationFailed { .. }
            | SkillDirectoryError::ExpectedVerificationFailed { .. },
        ) => {
            ora_info!(
                operation = "skill_materialization",
                operation_id = operation.identity().as_str(),
                resource_id = operation.resource().as_str(),
                stage,
                outcome = "no_match",
                "Skill operation exact state did not match"
            );
        }
        Err(error) => {
            let report = ErrorReport::from_error(error);
            let mut source: Option<&(dyn Error + 'static)> = Some(error);
            let mut os_code = None;
            while let Some(current) = source {
                if let Some(io) = current.downcast_ref::<std::io::Error>() {
                    os_code = io.raw_os_error();
                    break;
                }
                source = current.source();
            }
            ora_warn!(
                operation = "skill_materialization", operation_id = operation.identity().as_str(),
                attempt_id = operation.attempt().as_str(), resource_id = operation.resource().as_str(),
                generation = operation.generation().value(), mutation = ?operation.mutation(),
                stage, outcome = "failure", os_error = os_code,
                duration_ms = started.elapsed().as_millis() as u64,
                error.message = report.message(), error.chain = report.chain(),
                error.chain_depth = report.chain_depth(), "Skill operation stage failed",
            );
        }
    }
    result
}
