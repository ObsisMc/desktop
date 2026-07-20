use crate::clock::SystemClock;
use ora_application::{
    ApplicationError, CreateSessionHandler, DeleteSessionHandler, GetSessionHandler,
    ListSessionsHandler, UpdateSessionHandler, UuidSessionIdGenerator,
};
use ora_contracts::{
    CreateSessionRequest, CreateSessionResponse, DeleteSessionRequest, DeleteSessionResponse,
    GetSessionRequest, GetSessionResponse, ListSessionsRequest, ListSessionsResponse,
    UpdateSessionRequest, UpdateSessionResponse,
};
use ora_db::{RepositoryPool, SqliteSessionRepository};

/// Groups the concrete session handlers shared by runtime adapters.
pub(crate) struct SessionApi {
    create: CreateSessionHandler<SqliteSessionRepository, UuidSessionIdGenerator, SystemClock>,
    get: GetSessionHandler<SqliteSessionRepository>,
    list: ListSessionsHandler<SqliteSessionRepository>,
    update: UpdateSessionHandler<SqliteSessionRepository, SystemClock>,
    delete: DeleteSessionHandler<SqliteSessionRepository, SystemClock>,
}

impl SessionApi {
    /// Builds session handlers from the shared repository pool.
    pub(crate) fn new(pool: RepositoryPool, clock: SystemClock) -> Self {
        let repository = SqliteSessionRepository::new(pool);

        Self {
            create: CreateSessionHandler::new(
                repository.clone(),
                UuidSessionIdGenerator::new(),
                clock,
            ),
            get: GetSessionHandler::new(repository.clone()),
            list: ListSessionsHandler::new(repository.clone()),
            update: UpdateSessionHandler::new(repository.clone(), clock),
            delete: DeleteSessionHandler::new(repository, clock),
        }
    }

    /// Executes session creation through the application handler.
    pub(crate) fn create(
        &self,
        request: CreateSessionRequest,
    ) -> Result<CreateSessionResponse, ApplicationError> {
        self.create.handle(request)
    }

    /// Executes one session lookup through the application handler.
    pub(crate) fn get(
        &self,
        request: GetSessionRequest,
    ) -> Result<GetSessionResponse, ApplicationError> {
        self.get.handle(request)
    }

    /// Executes session listing through the application handler.
    pub(crate) fn list(
        &self,
        request: ListSessionsRequest,
    ) -> Result<ListSessionsResponse, ApplicationError> {
        self.list.handle(request)
    }

    /// Executes session replacement through the application handler.
    pub(crate) fn update(
        &self,
        request: UpdateSessionRequest,
    ) -> Result<UpdateSessionResponse, ApplicationError> {
        self.update.handle(request)
    }

    /// Executes session deletion through the application handler.
    pub(crate) fn delete(
        &self,
        request: DeleteSessionRequest,
    ) -> Result<DeleteSessionResponse, ApplicationError> {
        self.delete.handle(request)
    }
}
