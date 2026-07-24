use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Requests the global Git identity; carries no fields because the scope is fixed to `--global`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "git.ts")]
pub struct GetGitIdentityRequest {}

/// Returns the global Git identity, with each field absent when its config key is unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "git.ts")]
pub struct GitIdentityResponse {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Exports every TypeScript binding declared in this module into the target directory.
pub(crate) fn export(config: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    GetGitIdentityRequest::export(config)?;
    GitIdentityResponse::export(config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{GetGitIdentityRequest, GitIdentityResponse};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// Verifies the identity response preserves both fields and encodes an unset key as null.
    #[test]
    fn serializes_git_identity_contracts() {
        assert_eq!(
            serde_json::to_value(GetGitIdentityRequest::default()).unwrap(),
            json!({})
        );
        assert_eq!(
            serde_json::to_value(GitIdentityResponse {
                name: Some("RuihaoZhang".to_string()),
                email: None,
            })
            .unwrap(),
            json!({ "name": "RuihaoZhang", "email": null })
        );
    }
}
