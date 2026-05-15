use crate::{
    ApplicationError, Clock, CreateWorktreeHandler, DeleteWorktreeHandler, GetWorktreeHandler,
    ListWorktreesHandler, UpdateWorktreeHandler, WorktreeIdGenerator, WorktreeRepository,
    WorktreeRepositoryError,
};
use ora_contracts::{
    CreateWorktreeRequest, CreateWorktreeResponse, DeleteWorktreeRequest, DeleteWorktreeResponse,
    GetWorktreeRequest, GetWorktreeResponse, ListWorktreesRequest, ListWorktreesResponse,
    UpdateWorktreeRequest, UpdateWorktreeResponse, Worktree as ContractWorktree,
    WorktreeActivity as ContractWorktreeActivity,
};
use ora_domain::{
    AuditFields, TaskId, Worktree, WorktreeActivity as DomainWorktreeActivity, WorktreeId,
};
use ora_logging::{with_recorded_trace_logging, with_trace_logging};
use pretty_assertions::assert_eq;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// Verifies create handlers build domain worktrees and return the shared contract response.
#[test]
fn creates_worktrees_with_generated_identity_and_clock_values() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeWorktreeRepository::default());
        let handler = CreateWorktreeHandler::new(
            repository.clone(),
            FixedWorktreeIdGenerator::new("worktree-1"),
            FixedClock::new(1_700_000_000_000),
        );

        let response = match handler.handle(CreateWorktreeRequest {
            task_id: "task-1".to_string(),
            branch_name: Some("feature/task-handlers".to_string()),
            activity: ContractWorktreeActivity::Active,
        }) {
            Ok(response) => response,
            Err(error) => panic!("create handler failed: {error}"),
        };

        assert_eq!(
            response,
            CreateWorktreeResponse {
                worktree: ContractWorktree {
                    id: "worktree-1".to_string(),
                    task_id: "task-1".to_string(),
                    branch_name: Some("feature/task-handlers".to_string()),
                    activity: ContractWorktreeActivity::Active,
                },
            }
        );
        assert_eq!(
            repository.visible_worktrees(),
            vec![Worktree::new(
                WorktreeId::new("worktree-1"),
                TaskId::new("task-1"),
                Some("feature/task-handlers".to_string()),
                DomainWorktreeActivity::Active,
                AuditFields::new(1_700_000_000_000, 1_700_000_000_000, false),
            )]
        );
    });
}

/// Verifies get handlers return the shared contract projection for existing worktrees.
#[test]
fn gets_worktrees_by_identifier() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeWorktreeRepository::with_worktrees(vec![Worktree::new(
            WorktreeId::new("worktree-1"),
            TaskId::new("task-1"),
            None,
            DomainWorktreeActivity::Inactive,
            AuditFields::new(1, 2, false),
        )]));
        let handler = GetWorktreeHandler::new(repository);

        let response = match handler.handle(GetWorktreeRequest {
            worktree_id: "worktree-1".to_string(),
        }) {
            Ok(response) => response,
            Err(error) => panic!("get handler failed: {error}"),
        };

        assert_eq!(
            response,
            GetWorktreeResponse {
                worktree: ContractWorktree {
                    id: "worktree-1".to_string(),
                    task_id: "task-1".to_string(),
                    branch_name: None,
                    activity: ContractWorktreeActivity::Inactive,
                },
            }
        );
    });
}

/// Verifies list handlers map every stored worktree into the shared contract payload.
#[test]
fn lists_visible_worktrees() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeWorktreeRepository::with_worktrees(vec![
            Worktree::new(
                WorktreeId::new("worktree-1"),
                TaskId::new("task-1"),
                None,
                DomainWorktreeActivity::Inactive,
                AuditFields::new(1, 2, false),
            ),
            Worktree::new(
                WorktreeId::new("worktree-2"),
                TaskId::new("task-2"),
                Some("feature/updated-branch".to_string()),
                DomainWorktreeActivity::Active,
                AuditFields::new(3, 4, false),
            ),
        ]));
        let handler = ListWorktreesHandler::new(repository);

        let response = match handler.handle(ListWorktreesRequest {}) {
            Ok(response) => response,
            Err(error) => panic!("list handler failed: {error}"),
        };

        assert_eq!(
            response,
            ListWorktreesResponse {
                worktrees: vec![
                    ContractWorktree {
                        id: "worktree-1".to_string(),
                        task_id: "task-1".to_string(),
                        branch_name: None,
                        activity: ContractWorktreeActivity::Inactive,
                    },
                    ContractWorktree {
                        id: "worktree-2".to_string(),
                        task_id: "task-2".to_string(),
                        branch_name: Some("feature/updated-branch".to_string()),
                        activity: ContractWorktreeActivity::Active,
                    },
                ],
            }
        );
    });
}

/// Verifies update handlers preserve created timestamps while refreshing mutable fields.
#[test]
fn updates_worktrees_with_refreshed_timestamps() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeWorktreeRepository::with_worktrees(vec![Worktree::new(
            WorktreeId::new("worktree-1"),
            TaskId::new("task-1"),
            None,
            DomainWorktreeActivity::Inactive,
            AuditFields::new(10, 20, false),
        )]));
        let handler = UpdateWorktreeHandler::new(repository.clone(), FixedClock::new(30));

        let response = match handler.handle(UpdateWorktreeRequest {
            worktree_id: "worktree-1".to_string(),
            task_id: "task-2".to_string(),
            branch_name: Some("feature/updated-branch".to_string()),
            activity: ContractWorktreeActivity::Active,
        }) {
            Ok(response) => response,
            Err(error) => panic!("update handler failed: {error}"),
        };

        assert_eq!(
            response,
            UpdateWorktreeResponse {
                worktree: ContractWorktree {
                    id: "worktree-1".to_string(),
                    task_id: "task-2".to_string(),
                    branch_name: Some("feature/updated-branch".to_string()),
                    activity: ContractWorktreeActivity::Active,
                },
            }
        );
        assert_eq!(
            repository.visible_worktrees(),
            vec![Worktree::new(
                WorktreeId::new("worktree-1"),
                TaskId::new("task-2"),
                Some("feature/updated-branch".to_string()),
                DomainWorktreeActivity::Active,
                AuditFields::new(10, 30, false),
            )]
        );
    });
}

/// Verifies delete handlers keep the external CRUD contract while soft-deleting storage state.
#[test]
fn deletes_worktrees_through_soft_delete_repository_calls() {
    with_trace_logging(|| {
        let repository = Rc::new(FakeWorktreeRepository::with_worktrees(vec![Worktree::new(
            WorktreeId::new("worktree-1"),
            TaskId::new("task-1"),
            None,
            DomainWorktreeActivity::Inactive,
            AuditFields::new(10, 20, false),
        )]));
        let handler = DeleteWorktreeHandler::new(repository.clone(), FixedClock::new(40));

        let response = match handler.handle(DeleteWorktreeRequest {
            worktree_id: "worktree-1".to_string(),
        }) {
            Ok(response) => response,
            Err(error) => panic!("delete handler failed: {error}"),
        };

        assert_eq!(
            response,
            DeleteWorktreeResponse {
                worktree_id: "worktree-1".to_string(),
            }
        );
        assert_eq!(repository.visible_worktrees(), Vec::<Worktree>::new());
        assert_eq!(
            repository.all_worktrees(),
            vec![Worktree::new(
                WorktreeId::new("worktree-1"),
                TaskId::new("task-1"),
                None,
                DomainWorktreeActivity::Inactive,
                AuditFields::new(10, 40, true),
            )]
        );
    });
}

/// Verifies handlers expose stable application errors for missing worktrees and repository failures.
#[test]
fn reports_application_errors() {
    with_trace_logging(|| {
        let missing_repository = Rc::new(FakeWorktreeRepository::default());
        let get_handler = GetWorktreeHandler::new(missing_repository);
        let failing_repository = Rc::new(FakeWorktreeRepository::default());
        failing_repository.fail_next(WorktreeRepositoryError::OperationFailed(
            "storage unavailable".to_string(),
        ));
        let list_handler = ListWorktreesHandler::new(failing_repository);

        let missing_error = match get_handler.handle(GetWorktreeRequest {
            worktree_id: "missing".to_string(),
        }) {
            Ok(response) => panic!("expected missing error, got response: {response:?}"),
            Err(error) => error,
        };
        let repository_error = match list_handler.handle(ListWorktreesRequest {}) {
            Ok(response) => panic!("expected repository error, got response: {response:?}"),
            Err(error) => error,
        };

        assert_eq!(
            missing_error,
            ApplicationError::WorktreeNotFound {
                worktree_id: "missing".to_string(),
            }
        );
        assert_eq!(
            repository_error,
            ApplicationError::WorktreeRepository {
                message: "storage unavailable".to_string(),
            }
        );
    });
}

/// Verifies worktree handlers emit structured success and failure events under a scoped subscriber.
#[test]
fn emits_structured_operational_events() {
    let recorder = EventRecorder::default();
    with_recorded_trace_logging(recorder.layer(), || {
        let create_repository = Rc::new(FakeWorktreeRepository::default());
        let create_handler = CreateWorktreeHandler::new(
            create_repository,
            FixedWorktreeIdGenerator::new("worktree-42"),
            FixedClock::new(5),
        );
        let get_handler = GetWorktreeHandler::new(Rc::new(FakeWorktreeRepository::default()));

        create_handler
            .handle(CreateWorktreeRequest {
                task_id: "task-1".to_string(),
                branch_name: None,
                activity: ContractWorktreeActivity::Inactive,
            })
            .unwrap();
        assert_eq!(
            get_handler
                .handle(GetWorktreeRequest {
                    worktree_id: "missing".to_string(),
                })
                .unwrap_err(),
            ApplicationError::WorktreeNotFound {
                worktree_id: "missing".to_string(),
            }
        );
    });

    assert_eq!(
        recorder.events(),
        vec![
            LoggedEvent {
                level: "INFO".to_string(),
                target: "ora_application::worktree::handlers".to_string(),
                fields: BTreeMap::from([
                    (
                        "message".to_string(),
                        "worktree operation completed".to_string(),
                    ),
                    ("method".to_string(), "log_worktree_success".to_string()),
                    ("operation".to_string(), "create_worktree".to_string()),
                    ("worktree_id".to_string(), "worktree-42".to_string()),
                ]),
            },
            LoggedEvent {
                level: "ERROR".to_string(),
                target: "ora_application::worktree::handlers".to_string(),
                fields: BTreeMap::from([
                    ("error.kind".to_string(), "worktree_not_found".to_string(),),
                    (
                        "error.message".to_string(),
                        "worktree not found: missing".to_string(),
                    ),
                    (
                        "message".to_string(),
                        "worktree operation failed".to_string(),
                    ),
                    ("method".to_string(), "log_worktree_failure".to_string()),
                    ("operation".to_string(), "get_worktree".to_string()),
                    ("worktree_id".to_string(), "missing".to_string()),
                ]),
            },
        ]
    );
}

#[derive(Debug, Default)]
struct FakeWorktreeRepository {
    worktrees: RefCell<Vec<Worktree>>,
    next_error: RefCell<Option<WorktreeRepositoryError>>,
}

impl FakeWorktreeRepository {
    /// Builds a fake repository seeded with the provided worktree rows.
    fn with_worktrees(worktrees: Vec<Worktree>) -> Self {
        Self {
            worktrees: RefCell::new(worktrees),
            next_error: RefCell::new(None),
        }
    }

    /// Configures the next repository call to fail with a deterministic error.
    fn fail_next(&self, error: WorktreeRepositoryError) {
        self.next_error.replace(Some(error));
    }

    /// Returns every non-deleted worktree so tests can assert visible repository state.
    fn visible_worktrees(&self) -> Vec<Worktree> {
        self.worktrees
            .borrow()
            .iter()
            .filter(|worktree| !worktree.audit_fields.is_deleted)
            .cloned()
            .collect()
    }

    /// Returns all stored worktrees, including soft-deleted rows, for state assertions.
    fn all_worktrees(&self) -> Vec<Worktree> {
        self.worktrees.borrow().clone()
    }

    /// Returns a queued error when a test wants to simulate repository failure.
    fn take_error(&self) -> Result<(), WorktreeRepositoryError> {
        match self.next_error.borrow_mut().take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl WorktreeRepository for Rc<FakeWorktreeRepository> {
    fn create_worktree(&self, worktree: Worktree) -> Result<Worktree, WorktreeRepositoryError> {
        self.take_error()?;

        self.worktrees.borrow_mut().push(worktree.clone());
        Ok(worktree)
    }

    fn find_worktree(
        &self,
        worktree_id: &WorktreeId,
    ) -> Result<Option<Worktree>, WorktreeRepositoryError> {
        self.take_error()?;

        Ok(self
            .worktrees
            .borrow()
            .iter()
            .find(|worktree| worktree.id == *worktree_id && !worktree.audit_fields.is_deleted)
            .cloned())
    }

    fn list_worktrees(&self) -> Result<Vec<Worktree>, WorktreeRepositoryError> {
        self.take_error()?;

        Ok(self.visible_worktrees())
    }

    fn update_worktree(&self, worktree: Worktree) -> Result<Worktree, WorktreeRepositoryError> {
        self.take_error()?;

        let mut worktrees = self.worktrees.borrow_mut();
        if let Some(existing_worktree) = worktrees.iter_mut().find(|existing_worktree| {
            existing_worktree.id == worktree.id && !existing_worktree.audit_fields.is_deleted
        }) {
            *existing_worktree = worktree.clone();
            Ok(worktree)
        } else {
            Err(WorktreeRepositoryError::OperationFailed(format!(
                "missing worktree during update: {}",
                worktree.id
            )))
        }
    }

    fn soft_delete_worktree(
        &self,
        worktree_id: &WorktreeId,
        deleted_at: i64,
    ) -> Result<bool, WorktreeRepositoryError> {
        self.take_error()?;

        let mut worktrees = self.worktrees.borrow_mut();
        if let Some(worktree) = worktrees
            .iter_mut()
            .find(|worktree| worktree.id == *worktree_id && !worktree.audit_fields.is_deleted)
        {
            worktree.audit_fields.updated_at = deleted_at;
            worktree.audit_fields.is_deleted = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

struct FixedWorktreeIdGenerator {
    worktree_id: WorktreeId,
}

impl FixedWorktreeIdGenerator {
    /// Builds an identifier generator that always returns the provided worktree id.
    fn new(worktree_id: impl Into<String>) -> Self {
        Self {
            worktree_id: WorktreeId::new(worktree_id),
        }
    }
}

impl WorktreeIdGenerator for FixedWorktreeIdGenerator {
    fn generate_worktree_id(&self) -> WorktreeId {
        self.worktree_id.clone()
    }
}

struct FixedClock {
    timestamp_millis: i64,
}

impl FixedClock {
    /// Builds a clock that always returns the provided timestamp.
    fn new(timestamp_millis: i64) -> Self {
        Self { timestamp_millis }
    }
}

impl Clock for FixedClock {
    fn now_timestamp_millis(&self) -> i64 {
        self.timestamp_millis
    }
}

/// Captures one emitted event in a comparison-friendly structure for logging assertions.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LoggedEvent {
    level: String,
    target: String,
    fields: BTreeMap<String, String>,
}

/// Records tracing events into shared memory so tests can assert full structured outcomes.
#[derive(Clone, Debug, Default)]
struct EventRecorder {
    events: Arc<Mutex<Vec<LoggedEvent>>>,
}

impl EventRecorder {
    /// Builds the recording layer attached to one scoped test subscriber.
    fn layer(&self) -> RecordingLayer {
        RecordingLayer {
            events: self.events.clone(),
        }
    }

    /// Returns every captured event in emission order.
    fn events(&self) -> Vec<LoggedEvent> {
        self.events.lock().unwrap().clone()
    }
}

/// Pushes each tracing event into the shared recorder without relying on global subscriber state.
#[derive(Clone, Debug)]
struct RecordingLayer {
    events: Arc<Mutex<Vec<LoggedEvent>>>,
}

impl<S> Layer<S> for RecordingLayer
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    /// Converts each event into a stable, fully comparable structure for test assertions.
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);
        self.events.lock().unwrap().push(LoggedEvent {
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            fields: visitor.fields,
        });
    }
}

/// Records tracing fields as strings because these tests care about semantic content, not JSON formatting.
#[derive(Debug, Default)]
struct EventFieldVisitor {
    fields: BTreeMap<String, String>,
}

impl tracing::field::Visit for EventFieldVisitor {
    /// Preserves string fields exactly as handler logs emitted them.
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    /// Preserves signed integers in decimal form for stable assertions.
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    /// Preserves unsigned integers in decimal form for stable assertions.
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    /// Falls back to debug formatting for field types without a more specific visitor hook.
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            format!("{value:?}").trim_matches('"').to_string(),
        );
    }
}
