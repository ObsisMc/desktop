use std::path::Path;

use crate::error::{GitExecError, GitlancerError};
use crate::exec::command::{GitCommand, GitIntent};
use crate::exec::env::GitEnv;
use crate::exec::runner::GitRunner;
use crate::git::Git;

/// Represents the global Git identity read from the user's `~/.gitconfig`.
///
/// Either field is `None` when the corresponding key is unset, so callers can
/// distinguish "no global identity configured" from an execution failure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

impl<R: GitRunner> Git<R> {
    /// Reads `user.name` and `user.email` from the global Git configuration.
    ///
    /// The `--global` scope ignores any repository the process happens to sit in,
    /// so `cwd` only needs to be an existing directory the Git CLI can spawn from.
    /// An unset key is reported as `None` rather than an error.
    pub fn read_global_identity(&self, cwd: &Path) -> Result<GlobalIdentity, GitlancerError> {
        Ok(GlobalIdentity {
            name: self.read_global_config_value(cwd, "user.name")?,
            email: self.read_global_config_value(cwd, "user.email")?,
        })
    }

    /// Reads one global configuration value, mapping an unset key to `None`.
    fn read_global_config_value(
        &self,
        cwd: &Path,
        key: &str,
    ) -> Result<Option<String>, GitlancerError> {
        let command = GitCommand::new(
            cwd.to_path_buf(),
            vec![
                "config".to_string(),
                "--global".to_string(),
                "--get".to_string(),
                key.to_string(),
            ],
            GitEnv::default(),
            GitIntent::ReadOnly,
        );

        match self.runner().run(&command) {
            Ok(output) => {
                let value = output.stdout.trim();
                Ok(if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                })
            }
            // `git config --get` exits with code 1 when the key is absent; that is a
            // missing value, not a failure the caller should surface as an error.
            Err(GitExecError::NonZeroExit { code: Some(1), .. }) => Ok(None),
            Err(other) => Err(GitlancerError::Exec(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GlobalIdentity;
    use crate::error::{GitExecError, GitlancerError};
    use crate::exec::command::GitCommand;
    use crate::exec::output::GitOutput;
    use crate::exec::runner::GitRunner;
    use crate::git::Git;
    use std::path::Path;

    /// Answers each `git config --get` call from a scripted queue so both keys can be exercised.
    struct ScriptedRunner {
        outputs: std::sync::Mutex<std::collections::VecDeque<Result<GitOutput, GitExecError>>>,
    }

    impl ScriptedRunner {
        fn new(outputs: Vec<Result<GitOutput, GitExecError>>) -> Self {
            Self {
                outputs: std::sync::Mutex::new(outputs.into_iter().collect()),
            }
        }
    }

    impl GitRunner for ScriptedRunner {
        fn run(&self, _command: &GitCommand) -> Result<GitOutput, GitExecError> {
            self.outputs
                .lock()
                .expect("scripted runner queue is available")
                .pop_front()
                .expect("scripted runner has a queued response")
        }
    }

    fn ok(stdout: &str) -> Result<GitOutput, GitExecError> {
        Ok(GitOutput::new(Some(0), stdout.to_string(), String::new(), 0))
    }

    fn unset() -> Result<GitOutput, GitExecError> {
        Err(GitExecError::NonZeroExit {
            code: Some(1),
            args: vec!["config".to_string()],
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    /// Verifies configured values are trimmed and returned for both identity fields.
    #[test]
    fn reads_configured_identity() {
        let git = Git::new(ScriptedRunner::new(vec![
            ok("RuihaoZhang\n"),
            ok("r9644360@gmail.com\n"),
        ]));

        assert_eq!(
            git.read_global_identity(Path::new(".")).expect("identity read"),
            GlobalIdentity {
                name: Some("RuihaoZhang".to_string()),
                email: Some("r9644360@gmail.com".to_string()),
            }
        );
    }

    /// Verifies an unset key surfaces as `None` instead of a hard error.
    #[test]
    fn maps_unset_keys_to_none() {
        let git = Git::new(ScriptedRunner::new(vec![unset(), ok("only@email.dev")]));

        assert_eq!(
            git.read_global_identity(Path::new(".")).expect("identity read"),
            GlobalIdentity {
                name: None,
                email: Some("only@email.dev".to_string()),
            }
        );
    }

    /// Verifies non-"unset" execution failures propagate to the caller.
    #[test]
    fn propagates_execution_failures() {
        let git = Git::new(ScriptedRunner::new(vec![Err(GitExecError::GitNotFound)]));

        assert!(matches!(
            git.read_global_identity(Path::new(".")),
            Err(GitlancerError::Exec(GitExecError::GitNotFound))
        ));
    }
}
