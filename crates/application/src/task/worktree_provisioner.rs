use super::{
    CreateTaskWorktreeRequest, DeleteTaskWorktreeRequest, TaskWorktreeDeletionMode,
    TaskWorktreeProvisioner, TaskWorktreeProvisionerError,
};
use gitlancer::git::branch::ListBranchesRequest;
use gitlancer::git::worktree::{
    CreateWorktreeRequest as GitCreateWorktreeRequest,
    DeleteWorktreeRequest as GitDeleteWorktreeRequest, ResolveWorktreeByBranchRequest,
    WorktreeDeletionMode as GitWorktreeDeletionMode,
};
use gitlancer::{BranchName, CliGitRunner, Git, RepoRoot, Repository, WorktreeRoot};
use std::fs;
use std::path::{Path, PathBuf};

/// Provisions and removes application-owned task worktrees through the shared Git runtime.
#[derive(Clone, Debug)]
pub struct GitTaskWorktreeProvisioner {
    git: Git<CliGitRunner>,
    repository: Repository,
}

impl GitTaskWorktreeProvisioner {
    /// Builds a Git-backed task worktree provisioner for one configured project repository.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            git: Git::new(CliGitRunner),
            repository: Repository::new(RepoRoot::new(project_root)),
        }
    }
}

impl TaskWorktreeProvisioner for GitTaskWorktreeProvisioner {
    /// Checks local refs so orphaned task branches also participate in id collision avoidance.
    fn task_branch_exists(&self, branch_name: &str) -> Result<bool, TaskWorktreeProvisionerError> {
        self.git
            .list_branches(ListBranchesRequest {
                repository: &self.repository,
            })
            .map(|response| {
                response
                    .branches
                    .iter()
                    .any(|branch| branch.as_str() == branch_name)
            })
            .map_err(|_| {
                TaskWorktreeProvisionerError::OperationFailed(
                    "failed to inspect task branches".to_string(),
                )
            })
    }

    /// Creates one linked worktree while keeping Git-specific diagnostics inside the application.
    fn create_task_worktree(
        &self,
        request: CreateTaskWorktreeRequest,
    ) -> Result<(), TaskWorktreeProvisionerError> {
        create_parent_directory(&request.worktree_path)?;
        self.git
            .create_worktree(GitCreateWorktreeRequest {
                repository: &self.repository,
                worktree_root: WorktreeRoot::new(&request.worktree_path),
                branch_name: BranchName::new(request.branch_name),
            })
            .map(|_| ())
            .map_err(|_| {
                TaskWorktreeProvisionerError::OperationFailed(
                    "failed to create linked worktree".to_string(),
                )
            })
    }

    /// Resolves and deletes one linked worktree through Git's authoritative branch metadata.
    fn delete_task_worktree(
        &self,
        request: DeleteTaskWorktreeRequest,
    ) -> Result<(), TaskWorktreeProvisionerError> {
        let worktree = self
            .git
            .resolve_worktree_by_branch(ResolveWorktreeByBranchRequest {
                repository: &self.repository,
                branch_name: &request.branch_name,
            })
            .map_err(|_| {
                TaskWorktreeProvisionerError::OperationFailed(
                    "failed to delete linked worktree".to_string(),
                )
            })?;
        let mode = match request.mode {
            TaskWorktreeDeletionMode::Force => GitWorktreeDeletionMode::Force,
        };

        self.git
            .delete_worktree(GitDeleteWorktreeRequest {
                repository: &self.repository,
                worktree: &worktree,
                mode,
            })
            .map(|_| ())
            .map_err(|_| {
                TaskWorktreeProvisionerError::OperationFailed(
                    "failed to delete linked worktree".to_string(),
                )
            })
    }
}

/// Creates the parent eagerly because Git expects the worktree path's ancestor to exist.
fn create_parent_directory(worktree_path: &Path) -> Result<(), TaskWorktreeProvisionerError> {
    match worktree_path.parent() {
        Some(parent_directory) => fs::create_dir_all(parent_directory).map_err(|_| {
            TaskWorktreeProvisionerError::OperationFailed(
                "failed to create linked worktree".to_string(),
            )
        }),
        None => Ok(()),
    }
}
