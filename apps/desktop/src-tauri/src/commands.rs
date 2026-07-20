use crate::config::validate_worktree_root;
use crate::error::CommandError;
use crate::state::DesktopState;
use ora_backend::{Backend, BackendError};
use ora_contracts::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

/// Executes one synchronous backend operation on the runtime's blocking executor.
async fn run_backend<Request, Response>(
    backend: Backend,
    request: Request,
    operation: fn(&Backend, Request) -> Result<Response, BackendError>,
) -> Result<Response, CommandError>
where
    Request: Send + 'static,
    Response: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || operation(&backend, request))
        .await
        .map_err(|_| CommandError::execution())?
        .map_err(CommandError::from)
}

macro_rules! backend_command {
    ($name:ident, $request:ty, $response:ty, $operation:ident, $doc:literal) => {
        #[doc = $doc]
        #[tauri::command]
        pub async fn $name(
            state: State<'_, DesktopState>,
            request: $request,
        ) -> Result<$response, CommandError> {
            run_backend(state.backend.clone(), request, Backend::$operation).await
        }
    };
}

backend_command!(
    create_project,
    CreateProjectRequest,
    CreateProjectResponse,
    create_project,
    "Creates one project through the shared Backend."
);
backend_command!(
    get_project,
    GetProjectRequest,
    GetProjectResponse,
    get_project,
    "Gets one project through the shared Backend."
);
backend_command!(
    list_projects,
    ListProjectsRequest,
    ListProjectsResponse,
    list_projects,
    "Lists projects through the shared Backend."
);
backend_command!(
    update_project,
    UpdateProjectRequest,
    UpdateProjectResponse,
    update_project,
    "Updates one project through the shared Backend."
);
backend_command!(
    delete_project,
    DeleteProjectRequest,
    DeleteProjectResponse,
    delete_project,
    "Deletes one project through the shared Backend."
);

backend_command!(
    create_task,
    CreateTaskRequest,
    CreateTaskResponse,
    create_task,
    "Creates one task through the shared Backend."
);
backend_command!(
    get_task,
    GetTaskRequest,
    GetTaskResponse,
    get_task,
    "Gets one task through the shared Backend."
);
backend_command!(
    list_tasks,
    ListTasksRequest,
    ListTasksResponse,
    list_tasks,
    "Lists tasks through the shared Backend."
);
backend_command!(
    update_task,
    UpdateTaskRequest,
    UpdateTaskResponse,
    update_task,
    "Updates one task through the shared Backend."
);
backend_command!(
    delete_task,
    DeleteTaskRequest,
    DeleteTaskResponse,
    delete_task,
    "Deletes one task through the shared Backend."
);

backend_command!(
    create_session,
    CreateSessionRequest,
    CreateSessionResponse,
    create_session,
    "Creates one session through the shared Backend."
);
backend_command!(
    get_session,
    GetSessionRequest,
    GetSessionResponse,
    get_session,
    "Gets one session through the shared Backend."
);
backend_command!(
    list_sessions,
    ListSessionsRequest,
    ListSessionsResponse,
    list_sessions,
    "Lists sessions through the shared Backend."
);
backend_command!(
    update_session,
    UpdateSessionRequest,
    UpdateSessionResponse,
    update_session,
    "Updates one session through the shared Backend."
);
backend_command!(
    delete_session,
    DeleteSessionRequest,
    DeleteSessionResponse,
    delete_session,
    "Deletes one session through the shared Backend."
);

backend_command!(
    create_skill,
    CreateSkillRequest,
    CreateSkillResponse,
    create_skill,
    "Creates one skill through the shared Backend."
);
backend_command!(
    get_skill,
    GetSkillRequest,
    GetSkillResponse,
    get_skill,
    "Gets one skill through the shared Backend."
);
backend_command!(
    list_skills,
    ListSkillsRequest,
    ListSkillsResponse,
    list_skills,
    "Lists skills through the shared Backend."
);
backend_command!(
    update_skill,
    UpdateSkillRequest,
    UpdateSkillResponse,
    update_skill,
    "Updates one skill through the shared Backend."
);
backend_command!(
    delete_skill,
    DeleteSkillRequest,
    DeleteSkillResponse,
    delete_skill,
    "Deletes one skill through the shared Backend."
);

backend_command!(
    create_agent,
    CreateAgentRequest,
    CreateAgentResponse,
    create_agent,
    "Creates one configurable agent through the shared Backend."
);
backend_command!(
    get_agent,
    GetAgentRequest,
    GetAgentResponse,
    get_agent,
    "Gets one configurable agent through the shared Backend."
);
backend_command!(
    list_agents,
    ListAgentsRequest,
    ListAgentsResponse,
    list_agents,
    "Lists configurable agents through the shared Backend."
);
backend_command!(
    update_agent,
    UpdateAgentRequest,
    UpdateAgentResponse,
    update_agent,
    "Updates one configurable agent through the shared Backend."
);
backend_command!(
    delete_agent,
    DeleteAgentRequest,
    DeleteAgentResponse,
    delete_agent,
    "Deletes one configurable agent through the shared Backend."
);

/// Carries the empty request used to read Desktop runtime configuration consistently.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDesktopConfigRequest {}

/// Returns the current non-sensitive Desktop runtime configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDesktopConfigResponse {
    pub worktree_root: String,
}

/// Carries a user-selected worktree creation root into the Desktop configuration command.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetWorktreeRootRequest {
    pub worktree_root: String,
}

/// Confirms the active worktree root after a successful configuration update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetWorktreeRootResponse {
    pub worktree_root: String,
}

/// Reads the current Desktop worktree configuration without touching the Web API surface.
#[tauri::command]
pub async fn get_desktop_config(
    state: State<'_, DesktopState>,
    request: GetDesktopConfigRequest,
) -> Result<GetDesktopConfigResponse, CommandError> {
    let _ = request;
    let config = state.config.snapshot().map_err(CommandError::from)?;

    Ok(GetDesktopConfigResponse {
        worktree_root: config.worktree_root().to_string_lossy().into_owned(),
    })
}

/// Persists a new creation root and updates Backend configuration without interrupting in-flight work.
#[tauri::command]
pub async fn set_worktree_root(
    state: State<'_, DesktopState>,
    request: SetWorktreeRootRequest,
) -> Result<SetWorktreeRootResponse, CommandError> {
    let backend = state.backend.clone();
    let config_store = state.config.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let previous = config_store.snapshot().map_err(CommandError::from)?;
        let worktree_root = PathBuf::from(request.worktree_root);

        validate_worktree_root(&worktree_root).map_err(CommandError::from)?;
        backend
            .set_worktree_root(worktree_root.clone())
            .map_err(CommandError::from)?;
        if let Err(error) = config_store.set_worktree_root(worktree_root.clone()) {
            let _ = backend.set_worktree_root(previous.worktree_root().to_path_buf());
            return Err(CommandError::from(error));
        }

        Ok(SetWorktreeRootResponse {
            worktree_root: worktree_root.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|_| CommandError::execution())?
}
