use crate::clock::SystemClock;
use ora_application::{
    ApplicationError, CreateProjectHandler, DeleteProjectHandler, GetProjectHandler,
    ListProjectsHandler, UpdateProjectHandler, UuidProjectIdGenerator,
};
use ora_contracts::{
    CreateProjectRequest, CreateProjectResponse, DeleteProjectRequest, DeleteProjectResponse,
    GetProjectRequest, GetProjectResponse, ListProjectsRequest, ListProjectsResponse,
    UpdateProjectRequest, UpdateProjectResponse,
};
use ora_db::{RepositoryPool, SqliteProjectRepository};

/// Groups the concrete project handlers shared by runtime adapters.
pub(crate) struct ProjectApi {
    create: CreateProjectHandler<SqliteProjectRepository, UuidProjectIdGenerator, SystemClock>,
    get: GetProjectHandler<SqliteProjectRepository>,
    list: ListProjectsHandler<SqliteProjectRepository>,
    update: UpdateProjectHandler<SqliteProjectRepository, SystemClock>,
    delete: DeleteProjectHandler<SqliteProjectRepository, SystemClock>,
}

impl ProjectApi {
    /// Builds project handlers from the shared repository pool.
    pub(crate) fn new(pool: RepositoryPool, clock: SystemClock) -> Self {
        let repository = SqliteProjectRepository::new(pool);

        Self {
            create: CreateProjectHandler::new(
                repository.clone(),
                UuidProjectIdGenerator::new(),
                clock,
            ),
            get: GetProjectHandler::new(repository.clone()),
            list: ListProjectsHandler::new(repository.clone()),
            update: UpdateProjectHandler::new(repository.clone(), clock),
            delete: DeleteProjectHandler::new(repository, clock),
        }
    }

    /// Executes project creation through the application handler.
    pub(crate) fn create(
        &self,
        request: CreateProjectRequest,
    ) -> Result<CreateProjectResponse, ApplicationError> {
        self.create.handle(request)
    }

    /// Executes one project lookup through the application handler.
    pub(crate) fn get(
        &self,
        request: GetProjectRequest,
    ) -> Result<GetProjectResponse, ApplicationError> {
        self.get.handle(request)
    }

    /// Executes project listing through the application handler.
    pub(crate) fn list(
        &self,
        request: ListProjectsRequest,
    ) -> Result<ListProjectsResponse, ApplicationError> {
        self.list.handle(request)
    }

    /// Executes project replacement through the application handler.
    pub(crate) fn update(
        &self,
        request: UpdateProjectRequest,
    ) -> Result<UpdateProjectResponse, ApplicationError> {
        self.update.handle(request)
    }

    /// Executes project deletion through the application handler.
    pub(crate) fn delete(
        &self,
        request: DeleteProjectRequest,
    ) -> Result<DeleteProjectResponse, ApplicationError> {
        self.delete.handle(request)
    }
}
